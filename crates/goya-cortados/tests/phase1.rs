use goya_cortados::{analysis::analyse, bundle::validate_artifacts, Limits};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cortados-phase1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        }
        #[cfg(not(unix))]
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/cortados/source")
        .canonicalize()
        .unwrap()
}

fn cli(input: &Path, output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cortados"))
        .arg("analyse")
        .arg(input)
        .arg("--output")
        .arg(output)
        .output()
        .unwrap()
}

#[test]
fn java17_fixture_produces_valid_fair_linked_csir_without_native_claims() {
    let bundle = analyse(&fixture(), &Limits::default()).expect("static analysis");
    bundle
        .validate()
        .expect("hashes, references and provenance");
    let artifacts = bundle.artifacts();
    for name in [
        "bundle.json",
        "application.fair.json",
        "application.csir.json",
        "diagnostics.json",
        "dependencies.json",
        "capabilities.json",
    ] {
        assert!(artifacts.contains_key(name), "missing {name}");
    }
    assert!(!artifacts.contains_key("application.gscene"));
    assert!(!artifacts.contains_key("certification.json"));
    let fair: goya_fair::FairDocument =
        serde_json::from_slice(&artifacts["application.fair.json"]).unwrap();
    fair.validate().unwrap();
    assert_eq!(
        fair.canonical_json().as_bytes(),
        artifacts["application.fair.json"]
    );
    assert!(fair.elements.len() > 20);
    let manifest: Value = serde_json::from_slice(&artifacts["bundle.json"]).unwrap();
    assert_eq!(manifest["completeness"]["syntax_complete"], true);
    assert_eq!(manifest["completeness"]["semantic_complete"], false);
    assert_eq!(
        manifest["completeness"]["native_conversion_available"],
        false
    );
    assert!(manifest["completeness"]["certification"].is_null());
    let csir: Value = serde_json::from_slice(&artifacts["application.csir.json"]).unwrap();
    assert_eq!(csir["frontend"]["construction"], "static_analysis");
    assert!(!csir["fair"].is_null());
    assert!(csir["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["level"] == "implementation"));
}

#[test]
fn input_location_and_repeated_runs_do_not_change_artifact_bytes() {
    let first = Temp::new();
    let second = Temp::new();
    for temp in [&first, &second] {
        temp.source(
            "Sample.java",
            b"class Sample { int add(int value) { return value + 1; } }",
        );
        temp.source("config.properties", b"key=value\n");
    }
    let a = analyse(&first.0, &Limits::default()).unwrap().artifacts();
    let b = analyse(&first.0, &Limits::default()).unwrap().artifacts();
    let c = analyse(&second.0, &Limits::default()).unwrap().artifacts();
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn attached_bundle_tampering_is_rejected() {
    let temp = Temp::new();
    let raw = b"class Sample { int add(int x) { return x + 1; } }";
    temp.source("Sample.java", raw);
    let bundle = analyse(&temp.0, &Limits::default()).unwrap();
    let original = bundle.artifacts();
    let sources = BTreeMap::from([("Sample.java".to_string(), raw.to_vec())]);
    validate_artifacts(&original, &sources).unwrap();
    for name in [
        "application.fair.json",
        "application.csir.json",
        "dependencies.json",
        "capabilities.json",
    ] {
        let mut damaged = original.clone();
        damaged.get_mut(name).unwrap().push(b' ');
        assert!(
            validate_artifacts(&damaged, &sources).is_err(),
            "accepted changed {name}"
        );
    }
    let wrong_sources = BTreeMap::from([("Sample.java".to_string(), b"class Other {}".to_vec())]);
    assert!(validate_artifacts(&original, &wrong_sources).is_err());
}

#[test]
fn malformed_source_is_reported_and_never_claims_complete() {
    let temp = Temp::new();
    temp.source("Broken.java", b"class Broken { int x = ;");
    let bundle = analyse(&temp.0, &Limits::default()).expect("diagnostic bundle");
    bundle.validate().unwrap();
    let artifacts = bundle.artifacts();
    let manifest: Value = serde_json::from_slice(&artifacts["bundle.json"]).unwrap();
    assert_eq!(manifest["completeness"]["syntax_complete"], false);
    assert_eq!(manifest["completeness"]["complete"], false);
    assert!(String::from_utf8_lossy(&artifacts["diagnostics.json"]).contains("COR-E100"));
}

#[test]
fn cli_publishes_once_and_never_overwrites_an_existing_directory() {
    let temp = Temp::new();
    let output = temp.0.join("analysis");
    let first = cli(&fixture(), &output);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let saved = std::fs::read(output.join("bundle.json")).unwrap();
    let second = cli(&fixture(), &output);
    assert!(!second.status.success());
    assert_eq!(std::fs::read(output.join("bundle.json")).unwrap(), saved);
    assert_eq!(
        std::fs::read_dir(&temp.0).unwrap().count(),
        1,
        "normal failures clean their staging directories"
    );
}

#[test]
fn concurrent_publishers_commit_exactly_one_complete_bundle() {
    use std::process::Stdio;
    let temp = Temp::new();
    let output = temp.0.join("analysis");
    let spawn = || {
        Command::new(env!("CARGO_BIN_EXE_cortados"))
            .arg("analyse")
            .arg(fixture())
            .arg("--output")
            .arg(&output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let a = spawn();
    let b = spawn();
    let a = a.wait_with_output().unwrap();
    let b = b.wait_with_output().unwrap();
    assert_ne!(a.status.success(), b.status.success());
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 6);
    assert_eq!(std::fs::read_dir(&temp.0).unwrap().count(), 1);
}

#[test]
fn public_worker_flag_cannot_bypass_the_supervisor() {
    let temp = Temp::new();
    let result = Command::new(env!("CARGO_BIN_EXE_cortados"))
        .arg("analyse")
        .arg(fixture())
        .arg("--output")
        .arg(temp.0.join("analysis"))
        .arg("--worker")
        .env_remove("CORTADOS_INTERNAL_WORKER")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("COR-E500"));
    assert!(!temp.0.join("analysis").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn output_in_an_unprotected_shared_parent_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Temp::new();
    std::fs::set_permissions(&temp.0, std::fs::Permissions::from_mode(0o777)).unwrap();
    let result = cli(&fixture(), &temp.0.join("analysis"));
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("COR-E500"));
    assert_eq!(std::fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn worker_memory_limit_fails_closed_without_publishing() {
    let temp = Temp::new();
    let result = Command::new(env!("CARGO_BIN_EXE_cortados"))
        .arg("analyse")
        .arg(fixture())
        .arg("--output")
        .arg(temp.0.join("analysis"))
        .args(["--memory-mib", "1"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("COR-E500"));
    assert!(!temp.0.join("analysis").exists());
}

#[test]
fn cli_returns_failure_but_preserves_malformed_source_diagnostics() {
    let temp = Temp::new();
    let input = temp.0.join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("Broken.java"), b"class {").unwrap();
    let output = temp.0.join("analysis");
    let result = cli(&input, &output);
    assert!(!result.status.success());
    assert!(output.join("diagnostics.json").is_file());
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("bundle.json")).unwrap()).unwrap();
    assert_eq!(manifest["completeness"]["syntax_complete"], false);
}

#[test]
fn budgets_and_unimplemented_commands_fail_without_false_conversion() {
    let temp = Temp::new();
    temp.source("A.java", b"class A {}");
    let result = Command::new(env!("CARGO_BIN_EXE_cortados"))
        .arg("analyse")
        .arg(&temp.0)
        .arg("--output")
        .arg(temp.0.with_extension("outside"))
        .args(["--max-file-bytes", "1"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("COR-E500"));
    let conversion = Command::new(env!("CARGO_BIN_EXE_cortados"))
        .arg("convert")
        .arg(&temp.0)
        .output()
        .unwrap();
    assert!(!conversion.status.success());
    assert!(String::from_utf8_lossy(&conversion.stderr).contains("COR-E200"));
}

#[test]
fn malformed_input_mutation_corpus_preserves_original_span_bounds() {
    let limits = Limits {
        max_file_bytes: 256,
        max_tokens_per_file: 100,
        max_ast_depth: 32,
        max_memory_bytes: 32 * 1024 * 1024,
        ..Limits::default()
    };
    let alphabet = b"class A{};\\u0123456789abcdef'\"/\n*()[]<>+_-";
    let mut state = 0x1958_0bad_u64;
    for iteration in 0..1000 {
        let mut raw = Vec::new();
        for _ in 0..(iteration % 96) {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            raw.push(alphabet[(state as usize) % alphabet.len()]);
        }
        if iteration % 7 == 0 {
            raw.push(0xff);
        }
        let parsed = goya_cortados::java::parse("Mutation.java", &raw, &limits);
        for node in parsed.nodes {
            assert!(node.span.start <= node.span.end && node.span.end <= raw.len());
            if let Ok(text) = std::str::from_utf8(&raw) {
                assert!(text.is_char_boundary(node.span.start));
                assert!(text.is_char_boundary(node.span.end));
            }
        }
        for diagnostic in parsed.diagnostics {
            if let Some(span) = diagnostic.span {
                assert!(span.start <= span.end && span.end <= raw.len());
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn symlinked_output_parent_is_rejected_without_external_writes() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    let actual = temp.0.join("actual");
    std::fs::create_dir(&actual).unwrap();
    let link = temp.0.join("link");
    symlink(&actual, &link).unwrap();
    let result = cli(&fixture(), &link.join("analysis"));
    assert!(!result.status.success());
    assert!(!actual.join("analysis").exists());
}
