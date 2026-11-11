//! Native transport selection and the private Window Host attachment handshake.
//!
//! The Host owns the runtime and service scope. This module only connects a
//! Unix-domain stream and validates the Host's authoritative attachment ACK.

use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DESCRIPTOR_MAX_BYTES: u64 = 16 * 1024;
const ATTACH_ACK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostAttachmentDescriptor {
    pub protocol_version: u64,
    pub socket_path: PathBuf,
    pub attachment_id: String,
    pub generation: u64,
    pub capability: String,
    pub workspace_path: String,
    #[serde(default)]
    pub workspace_identity: Option<String>,
    #[serde(default)]
    pub remote_session_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AttachAck {
    protocol_version: u64,
    attachment_id: String,
    generation: u64,
    workspace_path: String,
    workspace_key: String,
    #[serde(default)]
    workspace_identity: Option<String>,
    #[serde(default)]
    remote_session_id: Option<String>,
    client_mode: String,
    client_id: String,
}

impl HostAttachmentDescriptor {
    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("read Host attachment descriptor metadata: {error}"))?;
        if !metadata.file_type().is_file() {
            return Err("Host attachment descriptor must be a regular non-symlink file".into());
        }
        if metadata.len() > DESCRIPTOR_MAX_BYTES {
            return Err("Host attachment descriptor exceeds 16 KiB".into());
        }
        if metadata.permissions().mode() & 0o777 != 0o600 {
            return Err("Host attachment descriptor permissions must be 0600".into());
        }
        let parent = path
            .parent()
            .ok_or_else(|| "Host attachment descriptor has no private parent".to_string())?;
        let parent_metadata = fs::symlink_metadata(parent)
            .map_err(|error| format!("read Host attachment directory metadata: {error}"))?;
        if !parent_metadata.file_type().is_dir()
            || parent_metadata.permissions().mode() & 0o777 != 0o700
            || parent_metadata.uid() != metadata.uid()
        {
            return Err("Host attachment descriptor must be in a same-owner 0700 directory".into());
        }
        let bytes =
            fs::read(path).map_err(|error| format!("read Host attachment descriptor: {error}"))?;
        let descriptor: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid Host attachment descriptor: {error}"))?;
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_version != 1 {
            return Err("unsupported Host attachment descriptor version".into());
        }
        if self.socket_path.as_os_str().is_empty()
            || self.attachment_id.trim().is_empty()
            || self.capability.trim().is_empty()
            || self.workspace_path.trim().is_empty()
        {
            return Err("Host attachment descriptor has an empty required field".into());
        }
        if self.generation == 0 {
            return Err("Host attachment descriptor generation must be nonzero".into());
        }
        if self.generation > 9_007_199_254_740_991 {
            return Err(
                "Host attachment descriptor generation exceeds the safe integer range".into(),
            );
        }
        if self.remote_session_id.is_some()
            && self
                .workspace_identity
                .as_deref()
                .is_none_or(|identity| identity.trim().is_empty())
        {
            return Err("remote Host attachment requires workspaceIdentity".into());
        }
        Ok(())
    }

    /// Connect and bind this stream to the single-use Host attachment.
    pub fn connect(&self, client_id: &str) -> Result<UnixStream, String> {
        self.validate()?;
        self.validate_socket_path()?;
        if client_id.trim().is_empty() || client_id.len() > 128 {
            return Err("native attachment clientId must contain 1 to 128 bytes".into());
        }
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|error| format!("connect to existing Window Host: {error}"))?;
        stream
            .set_read_timeout(Some(ATTACH_ACK_TIMEOUT))
            .map_err(|error| format!("set Host attachment timeout: {error}"))?;
        stream
            .set_write_timeout(Some(ATTACH_ACK_TIMEOUT))
            .map_err(|error| format!("set Host attachment timeout: {error}"))?;
        let mut params = serde_json::Map::new();
        params.insert("protocolVersion".into(), json!(self.protocol_version));
        params.insert("attachmentId".into(), json!(self.attachment_id));
        params.insert("generation".into(), json!(self.generation));
        params.insert("capability".into(), json!(self.capability));
        params.insert("workspacePath".into(), json!(self.workspace_path));
        params.insert("clientId".into(), json!(client_id));
        if let Some(identity) = &self.workspace_identity {
            params.insert("workspaceIdentity".into(), json!(identity));
        }
        if let Some(remote_session_id) = &self.remote_session_id {
            params.insert("remoteSessionId".into(), json!(remote_session_id));
        }
        let request = json!({
            "id": 0,
            "method": "native/attach",
            "params": Value::Object(params)
        });
        serde_json::to_writer(&mut stream, &request)
            .map_err(|error| format!("encode native/attach request: {error}"))?;
        stream
            .write_all(b"\n")
            .and_then(|_| stream.flush())
            .map_err(|error| format!("write native/attach request: {error}"))?;

        // The Host does not subscribe until the client receives this ACK, so
        // reading exactly one line cannot consume a later topic notification.
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|error| format!("clone Host attachment stream for handshake: {error}"))?,
        );
        let mut line = Vec::new();
        reader
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("read native/attach ACK: {error}"))?;
        if line.is_empty() || line.last() != Some(&b'\n') {
            return Err("Host attachment ACK was empty or exceeded its frame limit".into());
        }
        let response: Value = serde_json::from_slice(&line)
            .map_err(|error| format!("invalid native/attach ACK JSON: {error}"))?;
        if response.get("id").and_then(Value::as_u64) != Some(0) {
            return Err("Host attachment ACK request ID mismatch".into());
        }
        if let Some(error) = response.get("error") {
            return Err(format!("Host rejected native attachment: {error}"));
        }
        let ack: AttachAck = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .ok_or_else(|| "Host attachment ACK has no result".to_string())?,
        )
        .map_err(|error| format!("invalid Host attachment identity: {error}"))?;
        self.validate_ack(&ack, client_id)?;
        stream
            .set_read_timeout(None)
            .map_err(|error| format!("clear Host attachment timeout: {error}"))?;
        stream
            .set_write_timeout(None)
            .map_err(|error| format!("clear Host attachment timeout: {error}"))?;
        Ok(stream)
    }

    fn validate_socket_path(&self) -> Result<(), String> {
        let socket_metadata = fs::symlink_metadata(&self.socket_path)
            .map_err(|error| format!("read Window Host socket metadata: {error}"))?;
        let socket_parent = self
            .socket_path
            .parent()
            .ok_or_else(|| "Host socket has no private parent".to_string())?;
        let socket_parent_metadata = fs::symlink_metadata(socket_parent)
            .map_err(|error| format!("read Host socket directory metadata: {error}"))?;
        if !socket_metadata.file_type().is_socket()
            || socket_metadata.permissions().mode() & 0o777 != 0o600
            || !socket_parent_metadata.file_type().is_dir()
            || socket_parent_metadata.permissions().mode() & 0o777 != 0o700
            || socket_metadata.uid() != socket_parent_metadata.uid()
        {
            return Err("Window Host socket must be in a same-owner 0700 directory".into());
        }
        Ok(())
    }

    fn validate_ack(&self, ack: &AttachAck, client_id: &str) -> Result<(), String> {
        let expected_key = crate::protocol::workspace_key(
            &self.workspace_path,
            self.workspace_identity.as_deref(),
        );
        if ack.protocol_version != 1
            || ack.attachment_id != self.attachment_id
            || ack.generation != self.generation
            || ack.workspace_path != self.workspace_path
            || ack.workspace_key != expected_key
            || ack.workspace_identity != self.workspace_identity
            || ack.remote_session_id != self.remote_session_id
            || ack.client_mode != "desktop-continuous"
            || ack.client_id != client_id
        {
            return Err("Host attachment ACK identity does not match descriptor".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn descriptor() -> HostAttachmentDescriptor {
        HostAttachmentDescriptor {
            protocol_version: 1,
            socket_path: "/tmp/window-host.sock".into(),
            attachment_id: "att-1".into(),
            generation: 7,
            capability: "one-time-token".into(),
            workspace_path: "/work/project".into(),
            workspace_identity: Some(" workspace-1 ".into()),
            remote_session_id: None,
        }
    }

    #[test]
    fn descriptor_requires_remote_identity_and_nonempty_capability() {
        let mut value = descriptor();
        value.remote_session_id = Some("remote-1".into());
        value.workspace_identity = None;
        assert!(value
            .validate()
            .unwrap_err()
            .contains("requires workspaceIdentity"));
        value.workspace_identity = Some("workspace-1".into());
        assert!(value.validate().is_ok());
        value.capability.clear();
        assert!(value
            .validate()
            .unwrap_err()
            .contains("empty required field"));
    }

    #[test]
    fn descriptor_ack_requires_host_identity_generation_and_mode_match() {
        let descriptor = descriptor();
        let valid = AttachAck {
            protocol_version: 1,
            attachment_id: "att-1".into(),
            generation: 7,
            workspace_path: "/work/project".into(),
            workspace_key: "workspace-1".into(),
            workspace_identity: Some(" workspace-1 ".into()),
            remote_session_id: None,
            client_mode: "desktop-continuous".into(),
            client_id: "goya-123".into(),
        };
        assert!(descriptor.validate_ack(&valid, "goya-123").is_ok());
        let mut stale = valid.clone();
        stale.generation += 1;
        assert!(descriptor
            .validate_ack(&stale, "goya-123")
            .unwrap_err()
            .contains("identity"));
        let mut remote_descriptor = descriptor.clone();
        remote_descriptor.remote_session_id = Some("remote-1".into());
        let mut remote_ack = valid;
        remote_ack.remote_session_id = Some("remote-other".into());
        assert!(remote_descriptor
            .validate_ack(&remote_ack, "goya-123")
            .unwrap_err()
            .contains("identity"));
    }

    #[test]
    fn socket_attach_sends_descriptor_fields_without_socket_path_and_checks_ack() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("goya-host-attach-{}-{nonce}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let socket_path = directory.join("host.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).unwrap();
        let host = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "native/attach");
            assert_eq!(request["params"]["clientId"], "goya-123");
            assert!(request["params"].get("socketPath").is_none());
            assert!(request["params"].get("remoteSessionId").is_none());
            let ack = json!({
                "id": 0,
                "result": {
                    "protocolVersion": 1,
                    "attachmentId": "att-1",
                    "generation": 7,
                    "workspacePath": "/work/project",
                    "workspaceKey": "workspace-1",
                    "workspaceIdentity": " workspace-1 ",
                    "clientMode": "desktop-continuous",
                    "clientId": "goya-123"
                }
            });
            let mut writer = stream;
            writeln!(writer, "{ack}").unwrap();
            writer.flush().unwrap();
        });
        let mut descriptor = descriptor();
        descriptor.socket_path = socket_path;
        let stream = descriptor.connect("goya-123").unwrap();
        drop(stream);
        host.join().unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
}
