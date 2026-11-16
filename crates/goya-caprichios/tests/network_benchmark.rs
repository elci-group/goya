//! Manual local HTTPS browser-adapter latency smoke benchmark; no speed threshold.

use goya_caprichios::{NetworkDeclaration, RuntimeCapabilities, Target};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
#[ignore = "manual benchmark; requires Chrome, openssl, bank, Python websocket-client and loopback"]
fn chrome_https_adapter_latency_smoke() {
    let ir = NetworkDeclaration {
        timeout_ms: 5000,
        concurrency: 4,
        retries: 0,
        offline_queue: false,
    }
    .normalize()
    .unwrap();
    let artifact = ir
        .lower_checked(
            Target::Browser,
            RuntimeCapabilities {
                fetch: true,
                abort_controller: true,
                url: true,
                uint8_array: true,
                timers: true,
            },
        )
        .unwrap();

    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/network_benchmark.py"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 available");
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{}", artifact.source.len()).unwrap();
    input.write_all(artifact.source.as_bytes()).unwrap();
    drop(input);

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
