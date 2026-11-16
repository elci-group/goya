use goya_caprichios::{NetworkDeclaration, Target};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
#[ignore = "requires Chrome, openssl, bank, Python websocket-client and loopback"]
fn chrome_local_https_contract() {
    let artifact = NetworkDeclaration {
        timeout_ms: 200,
        concurrency: 2,
        retries: 0,
        offline_queue: false,
    }
    .normalize()
    .unwrap()
    .lower(Target::Browser);
    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/https_contract.py"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 available");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(artifact.source.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
