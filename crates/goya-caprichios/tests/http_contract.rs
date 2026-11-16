/// Runs the local Chrome HTTPS probe against source emitted by the actual
/// `http_caprichio` constructor. It is ignored by default because it requires
/// Chrome, Python websocket-client, OpenSSL, and usable loopback networking.
#[test]
#[ignore = "local Chrome HTTPS integration; certificate bypass is disclosed by the harness"]
fn generated_http_recipe_in_chrome() {
    let harness = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/http_contract.py");
    let output = std::process::Command::new("python3")
        .arg(harness)
        .output()
        .expect("python3 must be available to run the browser harness");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
