//! Optional real-Chrome comparison of the generated storage adapter with direct
//! IndexedDB calls that use the same database, value, transaction, and close path.
//! Output is a measurement sample only; there are no performance thresholds.

use goya_caprichios::storage::{Durability, RuntimeCapabilities, StorageDeclaration, Target};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
#[ignore = "requires Chrome, bank, Python websocket-client and loopback"]
fn generated_get_against_native_indexeddb_reference() {
    let namespace = "reference.benchmark";
    let declaration = StorageDeclaration {
        namespace: namespace.into(),
        max_value_bytes: 8192,
        durability: Durability::BestEffort,
        multi_key_transactions: false,
        encryption_at_rest: false,
    };
    let ir = declaration.normalize().unwrap();
    let capabilities = RuntimeCapabilities { indexed_db: true };
    let artifact = ir.lower(Target::Browser, capabilities).unwrap();
    assert_eq!(artifact, ir.lower(Target::Browser, capabilities).unwrap());

    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/storage_reference_benchmark.py"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 available");
    let source = artifact.source.as_bytes();
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{}", source.len()).unwrap();
    input.write_all(source).unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
