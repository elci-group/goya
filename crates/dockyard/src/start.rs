//! Start an admitted layout with the `runc` already on `PATH`.
//!
//! Dockyard does not install a runtime and does not start a Docker daemon.
//! The process runs in its own network namespace. Loopback is the only
//! interface, and the host address does not answer. An unprivileged caller
//! can map only one uid. `runc` requires that uid to be container uid 0, so
//! image user 65532 is recorded as not applied.

use crate::compile::{check_layout, exchange_when_up, Failure};
use crate::oci::sha256_hex;
use crate::{CONTENT_TYPE, IMAGE_USER};
use std::fs;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const IMAGE_UID: u32 = 65532;
const NOFILE: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartReport {
    pub image_start: &'static str,
    pub runtime: String,
    pub rootfs: &'static str,
    pub capabilities: &'static str,
    pub network: &'static str,
    pub runtime_user: String,
    pub user_applied: &'static str,
    pub qualification: &'static str,
    pub detail: String,
    pub exit_code: i32,
}

pub fn start(dir: &Path) -> Result<StartReport, Failure> {
    let checked = check_layout(dir)?;
    if !runc_available() {
        return Ok(blocked(
            "runc is unavailable. Dockyard does not install a runtime.".to_string(),
        ));
    }
    let (host_uid, host_gid) = host_ids().map_err(io_failure)?;
    let bundle = temp_dir("bundle")?;
    let _bundle_cleanup = RemoveDir(bundle.clone());
    let state = temp_dir("state")?;
    let _state_cleanup = RemoveDir(state.clone());
    install_rootfs(&bundle, &checked.binary).map_err(io_failure)?;
    let user_is_image_user = host_uid == 0 && host_gid == 0;
    fs::write(bundle.join("config.json"), runtime_spec(host_uid, host_gid))
        .map_err(|error| io_failure(format!("cannot write runtime spec: {error}")))?;
    let stderr_path = bundle.join("runc.err");
    let stderr = fs::File::create(&stderr_path)
        .map_err(|error| io_failure(format!("cannot record runtime stderr: {error}")))?;
    let id = container_id();
    let child = Command::new("runc")
        .arg("--root")
        .arg(&state)
        .arg("run")
        .arg("--bundle")
        .arg(&bundle)
        .arg(&id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .map_err(|error| Failure::Output {
            code: "DK-E5002",
            message: format!("runc failed to start: {error}"),
        })?;
    let stop = StopRunc {
        root: state,
        id,
        child: Some(child),
    };
    let observed = match observe_when_ready(&stop) {
        Ok(observed) => observed,
        Err(detail) => {
            drop(stop);
            let stderr_text = fs::read_to_string(&stderr_path).unwrap_or_default();
            return Ok(failed(detail, &stderr_text));
        }
    };
    let unknown = exchange_inside(observed.pid, checked.port, "/no-such");
    let admitted = exchange_inside(observed.pid, checked.port, &checked.path);
    let isolated = host_port_closed(checked.port);
    drop(stop);
    let stderr_text = fs::read_to_string(&stderr_path).unwrap_or_default();
    let unknown = match unknown {
        Ok(response) => response,
        Err(detail) => return Ok(failed(detail, &stderr_text)),
    };
    if unknown.status != 404
        || unknown.content_type != CONTENT_TYPE
        || unknown.body_sha256 != sha256_hex(b"not found")
    {
        return Ok(failed(
            format!(
                "unknown path returned {} {}",
                unknown.status, unknown.content_type
            ),
            &stderr_text,
        ));
    }
    let admitted = match admitted {
        Ok(response) => response,
        Err(detail) => return Ok(failed(detail, &stderr_text)),
    };
    if admitted.status != 200
        || admitted.content_type != CONTENT_TYPE
        || admitted.body_sha256 != checked.body_sha256
    {
        return Ok(failed(
            format!(
                "admitted route returned {} {}",
                admitted.status, admitted.content_type
            ),
            &stderr_text,
        ));
    }
    if !observed.readonly {
        return Ok(failed(
            "runc rootfs was not read-only".to_string(),
            &stderr_text,
        ));
    }
    if !observed.caps_dropped {
        return Ok(failed(
            "runc process retained capabilities".to_string(),
            &stderr_text,
        ));
    }
    if !observed.nofile {
        return Ok(failed(
            "runc process did not apply the open-file limit".to_string(),
            &stderr_text,
        ));
    }
    if !isolated {
        return Ok(failed(
            "admitted port answered on the host network".to_string(),
            &stderr_text,
        ));
    }
    let user_applied = user_is_image_user && observed.image_uid;
    Ok(StartReport {
        image_start: "pass",
        runtime: "runc".to_string(),
        rootfs: "read-only",
        capabilities: "dropped",
        network: "namespace",
        runtime_user: observed.runtime_user,
        user_applied: if user_applied { "pass" } else { "BLOCKED" },
        qualification: "BLOCKED",
        detail: if user_applied {
            "CPU and memory limits are unset.".to_string()
        } else {
            format!(
                "image user {IMAGE_USER} was not applied. This caller can map one uid and runc requires container uid 0."
            )
        },
        exit_code: 3,
    })
}

#[derive(Debug)]
struct Observation {
    pid: u32,
    readonly: bool,
    caps_dropped: bool,
    nofile: bool,
    image_uid: bool,
    runtime_user: String,
}

fn runc_available() -> bool {
    Command::new("runc")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn container_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("dockyard-{}-{nanos}", std::process::id())
}

fn observe_when_ready(stop: &StopRunc) -> Result<Observation, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last = String::from("runc state was not ready");
    while Instant::now() < deadline {
        match observe(stop) {
            Ok(observed) => return Ok(observed),
            Err(error) => {
                last = error;
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    Err(last)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetnsResponse {
    pub status: u16,
    pub content_type: String,
    pub body_sha256: String,
}

pub fn exchange_netns(pid: u32, port: u16, path: &str) -> Result<NetnsResponse, String> {
    one_thread()?;
    enter_container_net(pid)?;
    raise_loopback()?;
    let response = exchange_when_up(port, path)?;
    Ok(NetnsResponse {
        status: response.status,
        content_type: response.content_type,
        body_sha256: sha256_hex(&response.body),
    })
}

fn exchange_inside(pid: u32, port: u16, path: &str) -> Result<NetnsResponse, String> {
    let exe = std::env::current_exe().map_err(|error| format!("cannot find dockyard: {error}"))?;
    let output = Command::new(exe)
        .arg("--exchange-netns")
        .arg(pid.to_string())
        .arg(port.to_string())
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot enter the network namespace: {error}"))?;
    if !output.status.success() {
        let detail: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(500)
            .collect();
        return Err(format!("namespace exchange failed: {detail}"));
    }
    parse_netns(&String::from_utf8_lossy(&output.stdout))
}

fn parse_netns(text: &str) -> Result<NetnsResponse, String> {
    let status = line_value(text, "status")?
        .parse()
        .map_err(|_| "namespace status is not a number".to_string())?;
    Ok(NetnsResponse {
        status,
        content_type: line_value(text, "content-type")?,
        body_sha256: line_value(text, "body-sha256")?,
    })
}

fn line_value(text: &str, key: &str) -> Result<String, String> {
    let prefix = format!("{key}: ");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(str::to_string))
        .ok_or_else(|| format!("namespace exchange omitted {key}"))
}

fn one_thread() -> Result<(), String> {
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|error| format!("cannot read thread count: {error}"))?;
    let threads = status
        .lines()
        .find_map(|line| line.strip_prefix("Threads:"))
        .ok_or_else(|| "thread count is unavailable".to_string())?;
    let count: u32 = threads
        .trim()
        .parse()
        .map_err(|_| "thread count is not a number".to_string())?;
    if count != 1 {
        return Err(format!(
            "namespace entry needs one thread and found {count}"
        ));
    }
    Ok(())
}

fn enter_container_net(pid: u32) -> Result<(), String> {
    let net = fs::File::open(format!("/proc/{pid}/ns/net"))
        .map_err(|error| format!("cannot open net namespace: {error}"))?;
    // An unprivileged caller cannot enter the net namespace directly.
    // Joining the container user namespace first is what this host allows.
    // `nsenter -U` also calls setgroups, which this host rejects.
    if unsafe { setns(net.as_raw_fd(), CLONE_NEWNET) } == 0 {
        return Ok(());
    }
    let direct = std::io::Error::last_os_error();
    let user = fs::File::open(format!("/proc/{pid}/ns/user"))
        .map_err(|error| format!("cannot open user namespace: {error}"))?;
    if unsafe { setns(user.as_raw_fd(), CLONE_NEWUSER) } != 0 {
        let error = std::io::Error::last_os_error();
        return Err(format!(
            "cannot enter user namespace: {error}; net namespace: {direct}"
        ));
    }
    if unsafe { setns(net.as_raw_fd(), CLONE_NEWNET) } != 0 {
        let error = std::io::Error::last_os_error();
        return Err(format!("cannot enter net namespace: {error}"));
    }
    Ok(())
}

fn raise_loopback() -> Result<(), String> {
    let fd = unsafe { socket(AF_INET, SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(format!(
            "cannot open a loopback socket: {}",
            std::io::Error::last_os_error()
        ));
    }
    let sock = unsafe { fs::File::from_raw_fd(fd) };
    let mut request = IfReq {
        name: *b"lo\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        flags: 0,
        _pad: [0; 22],
    };
    if unsafe { ioctl(sock.as_raw_fd(), SIOCGIFFLAGS, &mut request as *mut IfReq) } < 0 {
        return Err(format!(
            "cannot read loopback flags: {}",
            std::io::Error::last_os_error()
        ));
    }
    if request.flags & IFF_UP != 0 {
        return Ok(());
    }
    request.flags |= IFF_UP;
    if unsafe { ioctl(sock.as_raw_fd(), SIOCSIFFLAGS, &mut request as *mut IfReq) } < 0 {
        return Err(format!(
            "cannot raise loopback: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn host_port_closed(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    matches!(
        TcpStream::connect_timeout(&address, Duration::from_millis(200)),
        Err(error) if error.kind() == ErrorKind::ConnectionRefused
    )
}

const AF_INET: i32 = 2;
const SOCK_DGRAM: i32 = 2;
const CLONE_NEWUSER: i32 = 0x1000_0000;
const CLONE_NEWNET: i32 = 0x4000_0000;
const SIOCGIFFLAGS: u64 = 0x8913;
const SIOCSIFFLAGS: u64 = 0x8914;
const IFF_UP: i16 = 1;

#[repr(C)]
struct IfReq {
    name: [u8; 16],
    flags: i16,
    _pad: [u8; 22],
}

extern "C" {
    fn setns(fd: i32, nstype: i32) -> i32;
    fn socket(domain: i32, ty: i32, protocol: i32) -> i32;
    fn ioctl(fd: i32, request: u64, ...) -> i32;
}

fn observe(stop: &StopRunc) -> Result<Observation, String> {
    let output = Command::new("runc")
        .arg("--root")
        .arg(&stop.root)
        .arg("state")
        .arg(&stop.id)
        .output()
        .map_err(|error| format!("runc state failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "runc state failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(500)
                .collect::<String>()
        ));
    }
    let state = String::from_utf8_lossy(&output.stdout);
    let pid = json_u32(&state, "pid").ok_or_else(|| "runc state has no pid".to_string())?;
    let status = fs::read_to_string(format!("/proc/{pid}/status"))
        .map_err(|error| format!("cannot read process status: {error}"))?;
    let limits = fs::read_to_string(format!("/proc/{pid}/limits"))
        .map_err(|error| format!("cannot read process limits: {error}"))?;
    let mounts = fs::read_to_string(format!("/proc/{pid}/mountinfo"))
        .map_err(|error| format!("cannot read process mounts: {error}"))?;
    let uid_map = fs::read_to_string(format!("/proc/{pid}/uid_map")).unwrap_or_default();
    let runtime_user = runtime_user(&status, &uid_map);
    Ok(Observation {
        pid,
        readonly: root_is_readonly(&mounts),
        caps_dropped: caps_dropped(&status),
        nofile: nofile_is(&limits, NOFILE),
        image_uid: effective_uid(&status) == Some(IMAGE_UID),
        runtime_user,
    })
}

pub(crate) fn runtime_spec(host_uid: u32, host_gid: u32) -> String {
    let rootless = !(host_uid == 0 && host_gid == 0);
    let (uid, gid, linux) = if rootless {
        (
            0,
            0,
            format!(
                "\"gidMappings\":[{{\"containerID\":0,\"hostID\":{host_gid},\"size\":1}}],\"namespaces\":[{{\"type\":\"pid\"}},{{\"type\":\"ipc\"}},{{\"type\":\"uts\"}},{{\"type\":\"mount\"}},{{\"type\":\"user\"}},{{\"type\":\"network\"}}],\"uidMappings\":[{{\"containerID\":0,\"hostID\":{host_uid},\"size\":1}}]"
            ),
        )
    } else {
        (
            IMAGE_UID,
            IMAGE_UID,
            "\"namespaces\":[{\"type\":\"pid\"},{\"type\":\"ipc\"},{\"type\":\"uts\"},{\"type\":\"mount\"},{\"type\":\"network\"}]".to_string(),
        )
    };
    format!(
        "{{\"hostname\":\"dockyard\",\"linux\":{{{linux}}},\"mounts\":[{{\"destination\":\"/proc\",\"source\":\"proc\",\"type\":\"proc\"}}],\"ociVersion\":\"1.0.2\",\"process\":{{\"args\":[\"/srv/dockyard/service\"],\"capabilities\":{{\"ambient\":[],\"bounding\":[],\"effective\":[],\"inheritable\":[],\"permitted\":[]}},\"cwd\":\"/\",\"env\":[\"PATH=/usr/bin\"],\"noNewPrivileges\":true,\"rlimits\":[{{\"hard\":{NOFILE},\"soft\":{NOFILE},\"type\":\"RLIMIT_NOFILE\"}}],\"terminal\":false,\"user\":{{\"gid\":{gid},\"uid\":{uid}}}}},\"root\":{{\"path\":\"rootfs\",\"readonly\":true}}}}"
    )
}

fn install_rootfs(bundle: &Path, binary: &[u8]) -> Result<(), String> {
    let executable = bundle.join("rootfs").join("srv").join("dockyard");
    fs::create_dir_all(&executable).map_err(|error| format!("cannot create rootfs: {error}"))?;
    let path = executable.join("service");
    fs::write(&path, binary).map_err(|error| format!("cannot write rootfs executable: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o555))
            .map_err(|error| format!("cannot set rootfs mode: {error}"))?;
    }
    Ok(())
}

fn host_ids() -> Result<(u32, u32), String> {
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|error| format!("cannot read caller identity: {error}"))?;
    let uid = field_u32(&status, "Uid").ok_or_else(|| "caller uid is unavailable".to_string())?;
    let gid = field_u32(&status, "Gid").ok_or_else(|| "caller gid is unavailable".to_string())?;
    Ok((uid, gid))
}

fn field_u32(status: &str, key: &str) -> Option<u32> {
    let line = status
        .lines()
        .find(|line| line.starts_with(&format!("{key}:")))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

fn effective_uid(status: &str) -> Option<u32> {
    field_u32(status, "Uid")
}

fn runtime_user(status: &str, uid_map: &str) -> String {
    if let Some(mapping) = uid_map.split_whitespace().collect::<Vec<_>>().get(..3) {
        if mapping[0] == "0" && mapping[2] == "1" {
            return format!("0 mapped to {}", mapping[1]);
        }
    }
    match effective_uid(status) {
        Some(uid) => format!("{uid}:{uid}"),
        None => "unknown".to_string(),
    }
}

pub(crate) fn root_is_readonly(mountinfo: &str) -> bool {
    mountinfo.lines().any(|line| {
        let mut fields = line.split(' ');
        let mountpoint = fields.nth(4);
        let options = fields.next().unwrap_or("");
        mountpoint == Some("/") && options.split(',').any(|option| option == "ro")
    })
}

pub(crate) fn caps_dropped(status: &str) -> bool {
    ["CapEff", "CapPrm", "CapBnd"].iter().all(|key| {
        status.lines().any(|line| {
            let mut parts = line.split_whitespace();
            parts.next() == Some(&format!("{key}:")) && parts.next() == Some("0000000000000000")
        })
    })
}

pub(crate) fn nofile_is(limits: &str, value: u64) -> bool {
    let expected = value.to_string();
    limits.lines().any(|line| {
        line.contains("Max open files")
            && line
                .split_whitespace()
                .filter(|word| *word == expected)
                .count()
                >= 2
    })
}

fn json_u32(text: &str, key: &str) -> Option<u32> {
    let pattern = format!("\"{key}\":");
    let rest = text.split(&pattern).nth(1)?.trim_start();
    let digits: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn blocked(detail: String) -> StartReport {
    StartReport {
        image_start: "BLOCKED",
        runtime: "unavailable".to_string(),
        rootfs: "unobserved",
        capabilities: "unobserved",
        network: "unobserved",
        runtime_user: "unobserved".to_string(),
        user_applied: "BLOCKED",
        qualification: "BLOCKED",
        detail,
        exit_code: 3,
    }
}

fn failed(detail: String, stderr: &str) -> StartReport {
    let stderr = stderr.trim();
    let detail = if stderr.is_empty() {
        detail
    } else {
        format!(
            "{detail}; runc: {}",
            stderr.chars().take(500).collect::<String>()
        )
    };
    StartReport {
        image_start: "fail",
        runtime: "runc".to_string(),
        rootfs: "unobserved",
        capabilities: "unobserved",
        network: "unobserved",
        runtime_user: "unobserved".to_string(),
        user_applied: "BLOCKED",
        qualification: "BLOCKED",
        detail,
        exit_code: 4,
    }
}

fn io_failure(detail: String) -> Failure {
    Failure::Output {
        code: "DK-E5001",
        message: detail,
    }
}

fn temp_dir(kind: &str) -> Result<PathBuf, Failure> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "dockyard-start-{kind}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir(&path).map_err(|error| Failure::Output {
        code: "DK-E5001",
        message: format!("cannot create runtime directory: {error}"),
    })?;
    Ok(path)
}

struct RemoveDir(PathBuf);

impl Drop for RemoveDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct StopRunc {
    root: PathBuf,
    id: String,
    child: Option<Child>,
}

impl Drop for StopRunc {
    fn drop(&mut self) {
        let _ = Command::new("runc")
            .arg("--root")
            .arg(&self.root)
            .arg("kill")
            .arg(&self.id)
            .arg("KILL")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = Command::new("runc")
            .arg("--root")
            .arg(&self.root)
            .arg("delete")
            .arg("--force")
            .arg(&self.id)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_spec_uses_the_image_user_and_no_capabilities() {
        let spec = runtime_spec(0, 0);
        assert!(spec.contains("\"uid\":65532"));
        assert!(spec.contains("\"gid\":65532"));
        assert!(spec.contains("\"readonly\":true"));
        assert!(spec.contains("\"ambient\":[]"));
        assert!(spec.contains("\"bounding\":[]"));
        assert!(!spec.contains("uidMappings"));
        assert!(!spec.contains("\"sh\""));
        assert!(spec.contains("\"/srv/dockyard/service\""));
        assert!(spec.contains("\"type\":\"network\""));
    }

    #[test]
    fn rootless_spec_maps_container_root_to_the_caller() {
        let spec = runtime_spec(1000, 1000);
        assert!(spec.contains("\"uid\":0"));
        assert!(spec.contains("\"hostID\":1000"));
        assert!(spec.contains("\"type\":\"user\""));
        assert!(spec.contains("\"type\":\"network\""));
        assert!(!spec.contains("\"uid\":65532"));
    }

    #[test]
    fn namespace_exchange_text_keeps_the_response() {
        let parsed = parse_netns("status: 404\ncontent-type: text/plain\nbody-sha256: abc\n")
            .expect("parsed");
        assert_eq!(parsed.status, 404);
        assert_eq!(parsed.content_type, "text/plain");
        assert_eq!(parsed.body_sha256, "abc");
        assert!(parse_netns("status: no\n").is_err());
    }

    #[test]
    fn observations_match_a_rootless_mount() {
        let mounts = "676 483 259:3 /tmp/rootfs / ro,noatime master:1 - ext4 /dev/nvme0n1p3 rw,errors=remount-ro\n";
        assert!(root_is_readonly(mounts));
        assert!(!root_is_readonly(
            "676 483 259:3 /tmp/rootfs / rw,noatime master:1 - ext4 /dev/nvme0n1p3 rw\n"
        ));
        let status = "Uid:\t1000\t1000\t1000\t1000\nCapPrm:\t0000000000000000\nCapEff:\t0000000000000000\nCapBnd:\t0000000000000000\n";
        assert!(caps_dropped(status));
        assert_eq!(effective_uid(status), Some(1000));
        let limits = "Max open files            1024                 1024                 files\n";
        assert!(nofile_is(limits, 1024));
        assert_eq!(
            runtime_user(status, "         0       1000          1\n"),
            "0 mapped to 1000"
        );
    }
}
