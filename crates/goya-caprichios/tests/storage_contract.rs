use goya_caprichios::storage::{Durability, RuntimeCapabilities, StorageDeclaration, Target};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
#[ignore = "requires Chrome, bank, Python websocket-client and loopback"]
fn chrome_indexeddb_contract() {
    let source = |namespace: &str| {
        let ir = StorageDeclaration {
            namespace: namespace.into(),
            max_value_bytes: 4,
            durability: Durability::BestEffort,
            multi_key_transactions: false,
            encryption_at_rest: false,
        }
        .normalize()
        .unwrap();
        let caps = RuntimeCapabilities { indexed_db: true };
        let artifact = ir.lower(Target::Browser, caps).unwrap();
        assert_eq!(
            artifact.source,
            ir.lower(Target::Browser, caps).unwrap().source
        );
        artifact.source
    };
    // Length-prefix the two emitted modules to preserve their exact bytes.
    let first = source("contract.first");
    let second = source("contract.second");
    let mut child = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/storage_contract.py"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 available");
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{}", first.len()).unwrap();
    input.write_all(first.as_bytes()).unwrap();
    input.write_all(second.as_bytes()).unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
