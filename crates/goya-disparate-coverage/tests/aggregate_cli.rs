//! Integration tests: the `disparate-aggregate` CLI contract (exit codes
//! 0/1/2 and output), run against the two real map-v2 deficit evidence files
//! in the worktree's `evidence/` directory.

use std::path::Path;
use std::process::Command;

const REACTOTRON_V2: &str = "disparates/reactotron-phase1/reference.map-v2.deficit.json";
const FIDDLE_V2: &str = "disparates/electron-fiddle-phase2/extraction.map-v2.deficit.json";

fn evidence(relative: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evidence/")
        .join(relative)
        .canonicalize()
        .expect("evidence file exists")
        .display()
        .to_string()
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disparate-aggregate"))
}

#[test]
fn aggregates_the_two_real_reports_to_stdout() {
    let output = cli()
        .args([evidence(REACTOTRON_V2), evidence(FIDDLE_V2)])
        .output()
        .expect("cli runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("source_root: aggregate(2 corpora)"));
    assert!(stdout.contains("elements: 746"));
    assert!(stdout.contains("deficit: PERSISTENCE_CAPABILITY_UNRESOLVED blocked=139 category=capability-decision-pending side=goya-side"));
    assert!(stdout.contains("deficit: HOOK_DATAFLOW_NOT_EXTRACTED blocked=63 category=extraction-limit side=programme-side"));
    assert!(stdout.contains("blocking-category: goya-side 219"));
    assert!(stdout.contains("blocking-category: programme-side 92"));
    assert!(stdout.contains("category: goya-capability-deficit 80"));
    assert!(stdout.contains("category: extraction-limit 92"));
    // Canonical JSON follows the table; parse from the first '{'.
    let json = &stdout[stdout.find('{').expect("aggregate JSON present")..];
    let report: goya_disparate_coverage::DeficitReport =
        serde_json::from_str(json).expect("aggregate JSON parses");
    assert_eq!(report.contract, goya_disparate_coverage::COVERAGE_CONTRACT);
    assert_eq!(report.source_root, "aggregate(2 corpora)");
    assert_eq!(report.elements, 746);
    assert_eq!(report.deficits.len(), 5);
}

#[test]
fn out_flag_writes_canonical_json_instead_of_stdout() {
    let out_path = std::env::temp_dir().join("disparate-aggregate-out.json");
    let output = cli()
        .arg(evidence(REACTOTRON_V2))
        .arg(evidence(FIDDLE_V2))
        .arg("--out")
        .arg(out_path.display().to_string())
        .output()
        .expect("cli runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("blocking-category: goya-side 219"));
    assert!(!stdout.contains("\"contract\""), "JSON must go to the file, not stdout");
    assert!(stdout.contains(&format!("written: {}", out_path.display())));
    let body = std::fs::read_to_string(&out_path).expect("output written");
    assert!(body.ends_with('\n'), "canonical JSON ends with a newline");
    let report: goya_disparate_coverage::DeficitReport =
        serde_json::from_str(&body).expect("written aggregate parses");
    assert_eq!(report.source_root, "aggregate(2 corpora)");
    assert_eq!(report.elements, 746);
}

#[test]
fn no_arguments_is_a_usage_error() {
    let output = cli().output().expect("cli runs");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn unknown_flag_is_a_usage_error() {
    let output = cli().arg("--json").output().expect("cli runs");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn dangling_out_flag_is_a_usage_error() {
    let output = cli().arg(evidence(REACTOTRON_V2)).arg("--out").output().expect("cli runs");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn unreadable_input_is_a_failure() {
    let output = cli().arg("/nonexistent/deficit.json").output().expect("cli runs");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn malformed_json_is_a_failure() {
    let path = std::env::temp_dir().join("disparate-aggregate-malformed.json");
    std::fs::write(&path, "{not json").expect("temp write");
    let output = cli().arg(path.display().to_string()).output().expect("cli runs");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn wrong_contract_is_a_validation_failure() {
    let path = std::env::temp_dir().join("disparate-aggregate-wrong-contract.json");
    std::fs::write(&path, r#"{"contract":"GY-OTHER/9","source_root":"x","elements":0,"totals":{},"deficits":[]}"#)
        .expect("temp write");
    let output = cli().arg(path.display().to_string()).output().expect("cli runs");
    assert_eq!(output.status.code(), Some(1));
}
