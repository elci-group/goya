//! Runs every shared conformance fixture in `fixtures/chromatics`.

use std::fs;
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/chromatics")
}

#[test]
fn every_fixture_passes() -> std::io::Result<()> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir())? {
        let p = entry?.path();
        if p.extension().is_some_and(|x| x == "chr") {
            paths.push(p);
        }
    }
    paths.sort();
    assert!(
        paths.len() >= 12,
        "expected the fixture set, found {}",
        paths.len()
    );
    let mut failures = Vec::new();
    for p in &paths {
        let src = fs::read_to_string(p)?;
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        match goya_chromatics::fixture::run(&src) {
            Ok(o) => assert!(o.resolves > 0, "{} resolves nothing", p.display()),
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    Ok(())
}

#[test]
fn a_wrong_expectation_fails() {
    let src = "capacity red\nintents open\ncandidate a intent=open label=A rank=0\nresolve\nexpect red=b\n";
    let e = goya_chromatics::fixture::run(src).err().unwrap_or_default();
    assert!(e.contains("line 5"), "{e}");
}

#[test]
fn unknown_commands_and_keys_fail_closed() {
    assert!(goya_chromatics::fixture::run("capacity red\nfrobnicate\n").is_err());
    assert!(goya_chromatics::fixture::run("candidate a colour=red\n").is_err());
    assert!(goya_chromatics::fixture::run("policy primary=magic\n").is_err());
}
