//! Manual Chrome IndexedDB latency smoke benchmark. No timing threshold is asserted.

use goya_caprichios::storage::{Durability, RuntimeCapabilities, StorageDeclaration, Target};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
#[ignore = "manual benchmark; requires Chrome, bank, Python websocket-client and loopback"]
fn chrome_indexeddb_put_get_latency_smoke() {
    let ir = StorageDeclaration {
        namespace: "benchmark.smoke".into(),
        max_value_bytes: 1024,
        durability: Durability::BestEffort,
        multi_key_transactions: false,
        encryption_at_rest: false,
    }
    .normalize()
    .unwrap();
    let artifact = ir
        .lower(Target::Browser, RuntimeCapabilities { indexed_db: true })
        .unwrap();

    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/storage_benchmark.py"
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
