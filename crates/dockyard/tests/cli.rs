use std::fs;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn dockyard(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_dockyard"))
        .args(args)
        .output()
        .expect("dockyard runs")
}

fn fixture() -> String {
    format!("{}/fixtures/ping.gservice", env!("CARGO_MANIFEST_DIR"))
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "dockyard-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    files
}

fn walk(root: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("prefix")
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, fs::read(&path).expect("file")));
        }
    }
}

#[test]
fn inspect_and_check_admit_the_fixture() {
    let inspected = dockyard(&["inspect", &fixture()]);
    assert_eq!(
        inspected.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let text = String::from_utf8(inspected.stdout).expect("stdout");
    assert!(text.contains("service: ping"));
    assert!(text.contains("qualification: not a build"));

    let checked = dockyard(&[
        "check",
        &fixture(),
        "--target",
        "x86_64-unknown-linux-gnu",
        "--profile",
        "dockyard.linux.http/0.0.1",
    ]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8(checked.stdout)
        .expect("stdout")
        .contains("admitted: dockyard.linux.http/0.0.1"));
}

#[test]
fn unsupported_inputs_fail_closed() {
    let musl = dockyard(&[
        "check",
        &fixture(),
        "--target",
        "x86_64-unknown-linux-musl",
        "--profile",
        "dockyard.linux.http/0.0.1",
    ]);
    assert_eq!(musl.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&musl.stderr).contains("DK-E0201"));

    let release = dockyard(&["build", &fixture(), "--release"]);
    assert_eq!(release.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&release.stderr).contains("DK-E0202"));

    let usage = dockyard(&[]);
    assert_eq!(usage.status.code(), Some(2));

    let temp = TempDir::new("scene");
    let scene = temp.0.join("window.gscene");
    fs::write(&scene, "@cell {\n    id root\n}\n").expect("scene");
    let rejected = dockyard(&[
        "check",
        scene.to_str().expect("utf8"),
        "--target",
        "x86_64-unknown-linux-gnu",
        "--profile",
        "dockyard.linux.http/0.0.1",
    ]);
    assert_eq!(rejected.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("DK-E0100"));
}

#[test]
fn build_is_deterministic_and_verify_runs_the_route() {
    let temp = TempDir::new("build");
    let first = temp.0.join("one");
    let second = temp.0.join("two");
    let args = [
        "build",
        &fixture(),
        "--target",
        "x86_64-unknown-linux-gnu",
        "--profile",
        "dockyard.linux.http/0.0.1",
    ];
    let built = dockyard(&[
        args[0],
        args[1],
        args[2],
        args[3],
        args[4],
        args[5],
        "--out",
        first.to_str().expect("utf8"),
    ]);
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let stdout = String::from_utf8(built.stdout).expect("stdout");
    assert!(stdout.contains("image-start: BLOCKED"));
    assert!(stdout.contains("runtime-qualification: BLOCKED"));

    let again = dockyard(&[
        args[0],
        args[1],
        args[2],
        args[3],
        args[4],
        args[5],
        "--out",
        second.to_str().expect("utf8"),
    ]);
    assert_eq!(
        again.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(tree(&first), tree(&second));

    let report = fs::read_to_string(first.join("evidence").join("report.txt")).expect("report");
    let layer = report
        .lines()
        .find_map(|line| line.strip_prefix("layer-sha256: "))
        .expect("layer");
    let layer_bytes = fs::read(first.join("blobs").join("sha256").join(layer)).expect("layer blob");
    assert!(!layer_bytes
        .windows(12)
        .any(|window| window == b"listen 39171"));
    assert!(!layer_bytes.windows(7).any(|window| window == b"respond"));
    assert!(report.contains("image-user: 65532:65532"));
    assert!(!report.contains("/home/"));

    let verified = dockyard(&["verify", first.to_str().expect("utf8")]);
    assert_eq!(
        verified.status.code(),
        Some(3),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    let verified_text = String::from_utf8(verified.stdout).expect("stdout");
    assert!(verified_text.contains("layout: pass"));
    assert!(verified_text.contains("executable: pass"));
    assert!(verified_text.contains("image-start: BLOCKED"));
    assert!(verified_text.contains("runtime-qualification: BLOCKED"));
}

#[test]
fn start_rejects_a_missing_layout() {
    let usage = dockyard(&["start"]);
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("DK-CLI-E0001"));

    let internal = dockyard(&["--exchange-netns"]);
    assert_eq!(internal.status.code(), Some(2));

    let missing = dockyard(&["start", "/tmp/dockyard-no-such-layout"]);
    assert_eq!(missing.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("DK-E4001"));
}

#[test]
fn start_records_an_installed_runc() {
    let runc = Command::new("runc")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let temp = TempDir::new("start");
    let source = temp.0.join("listen.gservice");
    fs::write(
        &source,
        "service ping\nlisten 39174\nroute GET /health\nrespond 200 text/plain \"ok\"\n",
    )
    .expect("source");
    let image = temp.0.join("image");
    let built = dockyard(&[
        "build",
        source.to_str().expect("utf8"),
        "--target",
        "x86_64-unknown-linux-gnu",
        "--profile",
        "dockyard.linux.http/0.0.1",
        "--out",
        image.to_str().expect("utf8"),
    ]);
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let before = tree(&image);
    let started = dockyard(&["start", image.to_str().expect("utf8")]);
    let stdout = String::from_utf8(started.stdout).expect("stdout");
    let stderr = String::from_utf8(started.stderr).expect("stderr");
    assert_eq!(before, tree(&image), "start writes into the layout");
    if runc.is_ok_and(|status| status.success()) {
        assert_eq!(
            started.status.code(),
            Some(3),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(stdout.contains("image-start: pass"), "{stdout}");
        assert!(stdout.contains("runtime: runc"), "{stdout}");
        assert!(stdout.contains("rootfs: read-only"), "{stdout}");
        assert!(stdout.contains("capabilities: dropped"), "{stdout}");
        assert!(stdout.contains("network: namespace"), "{stdout}");
        assert!(!stdout.contains("host-namespace"), "{stdout}");
        assert!(
            stdout.contains("runtime-qualification: BLOCKED"),
            "{stdout}"
        );
        if stdout.contains("user-applied: pass") {
            assert!(stdout.contains("runtime-user: 65532:65532"), "{stdout}");
        } else {
            assert!(stdout.contains("user-applied: BLOCKED"), "{stdout}");
            assert!(stdout.contains("runtime-user: 0 mapped to "), "{stdout}");
            assert!(
                stdout.contains("image user 65532:65532 was not applied"),
                "{stdout}"
            );
        }
        assert!(
            listener_released(39174),
            "listener still bound after start returned\n{stdout}\n{stderr}"
        );
    } else {
        assert_eq!(
            started.status.code(),
            Some(3),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(stdout.contains("image-start: BLOCKED"), "{stdout}");
        assert!(
            stdout.contains("runc is unavailable. Dockyard does not install a runtime."),
            "{stdout}"
        );
    }
    let verified = dockyard(&["verify", image.to_str().expect("utf8")]);
    assert_eq!(
        verified.status.code(),
        Some(3),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verified.stdout),
        String::from_utf8_lossy(&verified.stderr)
    );
    assert!(String::from_utf8(verified.stdout)
        .expect("stdout")
        .contains("image-start: BLOCKED"));
}

fn listener_released(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    for _ in 0..50 {
        match TcpStream::connect_timeout(&address, Duration::from_millis(200)) {
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => return true,
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    false
}
