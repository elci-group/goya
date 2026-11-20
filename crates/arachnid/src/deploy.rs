//! Developer Mode deployment, built without ares-cli or Node.js.
//!
//! Contract `arachnid.deploy/0.0.1`. It follows the protocol of
//! `@webos-tools/cli` (`lib/install.js`, `lib/base/luna.js`, TV profile
//! `config-tv.json`), read on 20 November 2026:
//!
//! 1. The Developer Mode app's key server serves an encrypted RSA key at
//!    `http://<tv>:9991/webos_rsa`. The passphrase shown in the app unlocks
//!    it. [`fetch_key`] saves it with mode 0600.
//! 2. SSH to `prisoner@<tv>`, port 9922.
//! 3. `test -d /media/developer/temp || mkdir -p /media/developer/temp`. It
//!    never removes the directory: on webOS 26 the prisoner user can write
//!    only inside it, and `rm -rf` of it fails (dev-manager-desktop #403).
//! 4. Stream the IPK in with `cat >`, then compare its byte count.
//! 5. `luna-send-pub -i luna://com.webos.appInstallService/dev/install
//!    {"id":"com.ares.defaultName","ipkUrl":…,"subscribe":true}`. Wait for
//!    `"state": "installed"` or a `FAILED` / `returnValue: false` reply.
//! 6. `rm -f` the staged IPK.
//! 7. Optionally `luna-send-pub -n 1
//!    luna://com.webos.applicationManager/launch {"id":…}`.
//!
//! The system `ssh` runs every step. One multiplexed control connection is
//! opened first, so the key's passphrase is asked for at most once.
//! `ssh-rsa` is re-enabled for host and user keys, because the TV's SSH
//! server predates OpenSSH's SHA-2 defaults.
//!
//! **Not yet run against a TV.** Only command construction, reply parsing
//! and key fetching against a local server are tested here.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub const CONTRACT: &str = "arachnid.deploy/0.0.1";
pub const SSH_PORT: u16 = 9922;
pub const KEY_PORT: u16 = 9991;
pub const USER: &str = "prisoner";
pub const TEMP_DIR: &str = "/media/developer/temp";
pub const INSTALL_URI: &str = "luna://com.webos.appInstallService/dev/install";
pub const LAUNCH_URI: &str = "luna://com.webos.applicationManager/launch";

#[derive(Clone, Debug)]
pub struct Device {
    pub host: String,
    pub key: PathBuf,
    /// Control socket for SSH multiplexing.
    pub control: PathBuf,
}

/// Hosts are IPv4/IPv6 literals or DNS names; nothing that a shell or ssh
/// could read as an option.
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with('-')
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

/// Single-quote for the remote POSIX shell.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl Device {
    pub fn ssh_args(&self) -> Vec<String> {
        vec![
            "-p".into(),
            SSH_PORT.to_string(),
            "-i".into(),
            self.key.display().to_string(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
            "-o".into(),
            "HostKeyAlgorithms=+ssh-rsa".into(),
            "-o".into(),
            "PubkeyAcceptedAlgorithms=+ssh-rsa".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-o".into(),
            "ControlMaster=auto".into(),
            "-o".into(),
            format!("ControlPath={}", self.control.display()),
            "-o".into(),
            "ControlPersist=60".into(),
            format!("{USER}@{}", self.host),
        ]
    }

    fn command(&self, remote: &str) -> Command {
        let mut c = Command::new("ssh");
        c.args(self.ssh_args()).arg("--").arg(remote);
        c
    }
}

/// Remote commands for one install, in order. `launch` adds the launch step.
pub fn plan(ipk_name: &str, app_id: &str, launch: bool) -> Vec<String> {
    let staged = format!("{TEMP_DIR}/{ipk_name}");
    let install = format!(
        "{{\"id\":\"com.ares.defaultName\",\"ipkUrl\":\"{}\",\"subscribe\":true}}",
        staged.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let mut steps = vec![
        format!("/usr/bin/test -d {TEMP_DIR} || /bin/mkdir -p {TEMP_DIR}"),
        format!("/bin/cat > {}", sh_quote(&staged)),
        format!("/usr/bin/wc -c < {}", sh_quote(&staged)),
        format!("luna-send-pub -i {INSTALL_URI} {}", sh_quote(&install)),
        format!("/bin/rm -f {}", sh_quote(&staged)),
    ];
    if launch {
        let payload = format!("{{\"id\":\"{}\"}}", app_id);
        steps.push(format!(
            "luna-send-pub -n 1 {LAUNCH_URI} {}",
            sh_quote(&payload)
        ));
    }
    steps
}

/// What one line of `luna-send-pub -i …/dev/install` output means.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallReply {
    Installed,
    Failed(String),
    Pending,
}

/// The string value of `"key": "value"` in one JSON line, tolerating
/// whitespace around the colon. Escapes are not decoded; installer replies
/// do not use them.
fn json_string_field(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let rest = &line[line.find(&needle)? + needle.len()..];
    let rest = rest
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    Some(rest.split('"').next()?.to_string())
}

/// Classify one JSON line from the installer. The match is textual, like
/// ares-cli's: state `installed` means success, and `FAILED` or
/// `"returnValue": false` means failure.
pub fn classify_install_line(line: &str) -> InstallReply {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("\"returnValue\":false") || compact.to_ascii_uppercase().contains("FAILED")
    {
        let reason = ["reason", "errorText", "state"]
            .iter()
            .find_map(|k| json_string_field(line, k))
            .unwrap_or_else(|| line.trim().to_string());
        return InstallReply::Failed(reason);
    }
    if compact.contains("\"state\":\"installed\"") {
        return InstallReply::Installed;
    }
    InstallReply::Pending
}

fn run(device: &Device, remote: &str, stdin: Option<&[u8]>) -> Result<String, String> {
    let mut child = device
        .command(remote)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("ssh: {e}"))?;
    if let Some(bytes) = stdin {
        let mut pipe = child.stdin.take().ok_or("ssh: no stdin")?;
        pipe.write_all(bytes).map_err(|e| format!("upload: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("ssh: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("remote command failed ({}): {remote}", out.status))
    }
}

/// Install (and optionally launch) `ipk` on the TV.
pub fn deploy(device: &Device, ipk: &Path, app_id: &str, launch: bool) -> Result<(), String> {
    if !valid_host(&device.host) {
        return Err(format!("invalid host {:?}", device.host));
    }
    let name = ipk
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| n.ends_with(".ipk") && !n.contains('/'))
        .ok_or("package must be a .ipk file")?;
    let bytes = std::fs::read(ipk).map_err(|e| format!("{}: {e}", ipk.display()))?;
    let steps = plan(name, app_id, launch);

    run(device, &steps[0], None)?;
    run(device, &steps[1], Some(&bytes))?;
    let size: usize = run(device, &steps[2], None)?
        .trim()
        .parse()
        .map_err(|_| "could not read the staged size".to_string())?;
    if size != bytes.len() {
        return Err(format!("upload truncated: {size} of {} bytes", bytes.len()));
    }

    // The installer subscription never ends by itself. Read until a
    // terminal reply, then end the session.
    let mut child = device
        .command(&steps[3])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("ssh: {e}"))?;
    let stdout = child.stdout.take().ok_or("ssh: no stdout")?;
    let mut outcome = Err("installer ended without a result".to_string());
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| format!("installer: {e}"))?;
        match classify_install_line(&line) {
            InstallReply::Installed => {
                outcome = Ok(());
                break;
            }
            InstallReply::Failed(reason) => {
                outcome = Err(format!("install failed: {reason}"));
                break;
            }
            InstallReply::Pending => {}
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = run(device, &steps[4], None);
    outcome?;
    if launch {
        run(device, &steps[5], None)?;
    }
    Ok(())
}

/// Fetch the Developer Mode key from the TV's key server and write it to
/// `out` (mode 0600, must not exist). The key stays encrypted; ssh asks
/// for the passphrase shown in the Developer Mode app.
pub fn fetch_key(host: &str, port: u16, out: &Path) -> Result<(), String> {
    if !valid_host(host) {
        return Err(format!("invalid host {host:?}"));
    }
    let addr = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut stream = TcpStream::connect(&addr).map_err(|e| format!("{addr}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    write!(
        stream,
        "GET /webos_rsa HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| format!("{addr}: {e}"))?;
    let mut response = Vec::new();
    stream
        .take(64 * 1024)
        .read_to_end(&mut response)
        .map_err(|e| format!("{addr}: {e}"))?;
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("malformed HTTP response from key server")?;
    let head = String::from_utf8_lossy(&response[..split]);
    let status = head.lines().next().unwrap_or("");
    if status.split_whitespace().nth(1) != Some("200") {
        return Err(format!(
            "key server answered {status:?}; is Key Server on in the Developer Mode app?"
        ));
    }
    let body = &response[split + 4..];
    if !String::from_utf8_lossy(body).contains("PRIVATE KEY-----") {
        return Err("key server did not return a private key".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(out)
        .and_then(|mut f| f.write_all(body))
        .map_err(|e| format!("{}: {e}", out.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn plan_never_removes_the_temp_dir() {
        let steps = plan("com.goya.a_1.0.0_all.ipk", "com.goya.a", true);
        assert_eq!(
            steps[0],
            "/usr/bin/test -d /media/developer/temp || /bin/mkdir -p /media/developer/temp"
        );
        assert!(steps.iter().all(|s| !s.contains("rm -rf")));
        assert_eq!(
            steps[1],
            "/bin/cat > '/media/developer/temp/com.goya.a_1.0.0_all.ipk'"
        );
        assert_eq!(
            steps[3],
            "luna-send-pub -i luna://com.webos.appInstallService/dev/install '{\"id\":\"com.ares.defaultName\",\"ipkUrl\":\"/media/developer/temp/com.goya.a_1.0.0_all.ipk\",\"subscribe\":true}'"
        );
        assert_eq!(steps[5], "luna-send-pub -n 1 luna://com.webos.applicationManager/launch '{\"id\":\"com.goya.a\"}'");
        assert_eq!(plan("x.ipk", "a.b", false).len(), 5);
    }

    #[test]
    fn quoting_is_shell_safe() {
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
        assert!(valid_host("192.168.1.20") && valid_host("tv.local") && valid_host("fe80::1"));
        for bad in ["", "-oProxyCommand=x", "a b", "a;b", "a$(b)"] {
            assert!(!valid_host(bad), "{bad}");
        }
    }

    /// Replies from the dev-manager-desktop #403 transcript (webOS 26).
    #[test]
    fn installer_replies_classify() {
        assert_eq!(
            classify_install_line(
                r#"{"statusValue": 30, "details": {"packageId": "app.pelagica.pelagica", "ipkFile": "/media/developer/temp/p.ipk", "state": "installed"}}"#
            ),
            InstallReply::Installed
        );
        assert_eq!(
            classify_install_line(r#"{"details": {"state": "installing"}}"#),
            InstallReply::Pending
        );
        assert_eq!(
            classify_install_line(
                r#"{"returnValue": false, "errorText": "install has been disconnected."}"#
            ),
            InstallReply::Failed("install has been disconnected.".into())
        );
        assert!(matches!(
            classify_install_line(r#"{"details": {"state": "install failed", "reason": "FAILED_IPKG_INSTALL"}}"#),
            InstallReply::Failed(r) if r == "FAILED_IPKG_INSTALL"
        ));
    }

    #[test]
    fn ssh_args_pin_port_user_and_rsa() {
        let d = Device {
            host: "10.0.0.5".into(),
            key: "/k".into(),
            control: "/c".into(),
        };
        let args = d.ssh_args().join(" ");
        assert!(args.starts_with("-p 9922 -i /k "));
        assert!(args.contains("HostKeyAlgorithms=+ssh-rsa") && args.contains("ControlPath=/c"));
        assert!(args.ends_with("prisoner@10.0.0.5"));
    }

    /// Serve one HTTP response after reading the whole request head, then
    /// close cleanly so the client never sees a reset.
    fn serve_once(response: &'static [u8]) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 256];
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => seen.extend_from_slice(&buf[..n]),
                }
            }
            s.write_all(response).unwrap();
            let _ = s.shutdown(std::net::Shutdown::Write);
            let _ = s.read(&mut buf);
        });
        port
    }

    #[test]
    fn fetch_key_against_local_server() {
        let dir = std::env::temp_dir().join(format!("arachnid-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let key = dir.join("tv_rsa");
        let port = serve_once(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\n-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nabc\n-----END RSA PRIVATE KEY-----\n");
        fetch_key("127.0.0.1", port, &key).unwrap();
        assert!(std::fs::read_to_string(&key)
            .unwrap()
            .starts_with("-----BEGIN RSA PRIVATE KEY-----"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&key).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let port = serve_once(b"HTTP/1.1 404 Not Found\r\n\r\n");
        let err = fetch_key("127.0.0.1", port, &dir.join("other")).unwrap_err();
        assert!(err.contains("Key Server"), "{err}");
        // Existing files are never overwritten.
        let port = serve_once(b"HTTP/1.1 200 OK\r\n\r\n-----BEGIN RSA PRIVATE KEY-----\n");
        assert!(fetch_key("127.0.0.1", port, &key).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
