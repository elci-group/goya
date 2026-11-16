use goya_caprichios::{NetworkDeclaration, Target};
use std::process::Command;

// Bun executes both recipes against the same controlled Fetch host. This
// verifies adapter behavior, not browser CORS, TLS or platform conformance.
#[test]
#[ignore = "requires bun; run explicitly with --ignored"]
fn shared_fetch_contract() {
    let ir = NetworkDeclaration {
        timeout_ms: 30,
        concurrency: 1,
        retries: 0,
        offline_queue: false,
    }
    .normalize()
    .unwrap();
    for target in [Target::Browser, Target::Node] {
        let script = format!("const get = new Function(process.env.CAP_SOURCE.replace('export async function get', 'async function get') + '\\nreturn get;')();\n{}", include_str!("contract.js"));
        let output = Command::new("bun")
            .args(["-e", &script])
            .env("CAP_SOURCE", ir.lower(target).source)
            .output()
            .expect("bun must be installed");
        assert!(
            output.status.success(),
            "{target:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
