use goya_caprichios::{NetworkDeclaration, Target};
use std::{fs, process::Command};

/// Runs the same controlled Fetch contract in Chrome's JavaScript engine.
/// It deliberately does not claim TLS, CORS, or network-stack conformance.
#[test]
#[ignore = "requires Chrome and a usable desktop sandbox; run explicitly with --ignored"]
fn chrome_fetch_contract() {
    let source = NetworkDeclaration {
        timeout_ms: 30,
        concurrency: 1,
        retries: 0,
        offline_queue: false,
    }
    .normalize()
    .unwrap()
    .lower(Target::Browser)
    .source;
    let html = format!("<!doctype html><body>pending<script type=\"module\">\n{source}\ntry {{\n{}\ndocument.body.textContent='CAPRICHIO PASS';\n}} catch(e) {{ document.body.textContent='CAPRICHIO FAIL: '+e; }}\n</script></body>", include_str!("contract.js"));
    let path = std::env::temp_dir().join(format!(
        "goya-caprichios-browser-{}.html",
        std::process::id()
    ));
    assert!(Command::new("bank")
        .arg(&path)
        .status()
        .expect("bank available")
        .success());
    fs::write(&path, html).unwrap();
    let output = Command::new("google-chrome")
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--virtual-time-budget=2000",
            "--dump-dom",
        ])
        .arg(format!("file://{}", path.display()))
        .output()
        .expect("Chrome available");
    let _ = fs::remove_file(path);
    assert!(
        output.status.success(),
        "Chrome failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let dom = String::from_utf8_lossy(&output.stdout);
    assert!(
        dom.contains("<body>CAPRICHIO PASS</body>"),
        "unexpected DOM: {dom}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
