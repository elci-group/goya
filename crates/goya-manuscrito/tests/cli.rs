//! CLI conformance: argument handling, the analysis-only inventory path, and
//! the compiler-graph/gap pipeline against a real trusted TypeScript 5.4.5
//! installation.
//!
//! The `graph` and `gaps` end-to-end tests run only when
//! `MANUSCRITO_TEST_TYPESCRIPT_PATH` points at a trusted TypeScript 5.4.5
//! `lib/typescript.js`; otherwise they are skipped so `cargo test` stays
//! hermetic.

mod common;

use common::{full_project, graph_project, temp_project, write};
use goya_manuscrito::{MirReport, REPORT_SCHEMA};
use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_manuscrito");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("launch manuscrito binary")
}

fn stdout(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8");
    serde_json::from_str(&stdout).expect("stdout parses as JSON")
}

fn trusted_typescript() -> Option<String> {
    match std::env::var("MANUSCRITO_TEST_TYPESCRIPT_PATH") {
        Ok(path) if !path.is_empty() => Some(path),
        _ => {
            eprintln!(
                "skipping graph/gaps end-to-end test: MANUSCRITO_TEST_TYPESCRIPT_PATH is not set"
            );
            None
        }
    }
}

fn run_graph_command(command: &str, project: &Path, typescript: &str) -> Output {
    Command::new(BIN)
        .arg(command)
        .arg(project)
        .env("MANUSCRITO_TYPESCRIPT_PATH", typescript)
        .output()
        .expect("launch manuscrito binary")
}

#[test]
fn usage_errors_exit_with_two() {
    for args in [
        vec![],
        vec!["analyse"],
        vec!["translate", "."],
        vec!["analyse", "a", "b"],
        vec!["graph"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("usage: manuscrito"),
            "args: {args:?}: {stderr}"
        );
        assert!(output.stdout.is_empty(), "args: {args:?}");
    }
}

#[test]
fn analyse_end_to_end_matches_library_contract() {
    let root = full_project("cli-analyse");
    let output = run(&["analyse", root.to_str().expect("utf-8 root")]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report = stdout(&output);
    assert_eq!(report["schema"], json_str(REPORT_SCHEMA));
    assert_eq!(report["mode"], json_str("analysis-only"));
    assert_eq!(report["translated"], serde_json::json!(false));
    assert_eq!(report["package_name"], json_str("fixture-app"));

    // Library and CLI must agree byte-for-byte on the same project (the CLI
    // terminates its report with one newline).
    let library = goya_manuscrito::analyse(&root).expect("library analysis succeeds");
    let expected = serde_json::to_string_pretty(&library).expect("serialize library report");
    assert_eq!(output.stdout, format!("{expected}\n").into_bytes());
}

#[test]
fn analyse_repeated_runs_are_byte_identical() {
    let root = full_project("cli-determinism");
    let first = run(&["analyse", root.to_str().expect("utf-8 root")]);
    let second = run(&["analyse", root.to_str().expect("utf-8 root")]);
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn analyse_missing_project_exits_with_failure() {
    let root = temp_project("cli-absent");
    let missing = root.join("does-not-exist");
    let output = run(&["analyse", missing.to_str().expect("utf-8 root")]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("analysis failed"), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
}

fn json_str(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.to_owned())
}

#[test]
fn graph_end_to_end_reports_modules_cycles_capabilities_and_mir() {
    let Some(typescript) = trusted_typescript() else {
        return;
    };
    let root = graph_project("cli-graph");

    let first = run_graph_command("graph", &root, &typescript);
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let report = stdout(&first);

    assert_eq!(report["schema"], json_str("goya.manuscrito.graph/1"));
    assert_eq!(report["status"], json_str("ok"));
    assert_eq!(report["typescript_version"], json_str("5.4.5"));
    assert_eq!(
        report["modules"],
        serde_json::json!(["src/main.ts", "src/util.ts"])
    );

    // The main/util import cycle collapses into one two-element SCC.
    assert_eq!(
        report["cycles"],
        serde_json::json!([["src/main.ts", "src/util.ts"]])
    );
    assert_eq!(report["coverage"]["total_edges"], serde_json::json!(3));

    // The Node built-in call site is recognised as a filesystem capability.
    let observations = report["capability_observations"].as_array().expect("array");
    assert!(
        observations
            .iter()
            .any(|o| o["capability"] == json_str("filesystem")
                && o["operation"] == json_str("readFile")
                && o["status"] == json_str("recognized")),
        "observations: {observations:?}"
    );

    // The non-literal dynamic import survives as an explicit loader boundary.
    let boundaries = report["loader_boundaries"].as_array().expect("array");
    assert!(
        boundaries
            .iter()
            .any(|b| b["kind"] == json_str("dynamic-import")
                && b["reason"] == json_str("non-literal module specifier")),
        "boundaries: {boundaries:?}"
    );

    // The Rust-side MIR contract must accept the emitted extraction.
    let mir: MirReport =
        serde_json::from_value(report["mir"].clone()).expect("frontend MIR parses");
    mir.validate_version().expect("frontend MIR validates");
    assert_eq!(
        mir.nodes.len(),
        report["mir"]["nodes"].as_array().expect("array").len()
    );
    assert!(mir
        .nodes
        .iter()
        .any(|node| node.kind == goya_manuscrito::SemanticKind::Throw));

    // The graph report is deterministic for an unchanged project.
    let second = run_graph_command("graph", &root, &typescript);
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn graph_without_tsconfig_fails_explicitly() {
    let Some(typescript) = trusted_typescript() else {
        return;
    };
    let root = temp_project("cli-graph-bare");
    write(&root, "src/only.ts", "export const only = 1;\n");

    let output = run_graph_command("graph", &root, &typescript);
    assert_eq!(output.status.code(), Some(1));
    let report = stdout(&output);
    assert_eq!(report["status"], json_str("failed"));
    assert_eq!(report["diagnostics"][0]["code"], json_str("MAN-FE003"));
}

#[test]
fn gaps_end_to_end_marks_every_backend_domain() {
    let Some(typescript) = trusted_typescript() else {
        return;
    };
    let root = graph_project("cli-gaps");

    let output = run_graph_command("gaps", &root, &typescript);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = stdout(&output);

    assert_eq!(report["schema"], json_str("goya.manuscrito.gscene-gaps/1"));
    assert_eq!(report["status"], json_str("analysis-only"));
    assert_eq!(report["canonical_language"], json_str("GY-LANG/1"));
    assert_eq!(report["translated"], serde_json::json!(false));
    assert_eq!(report["canonical_output_emitted"], serde_json::json!(false));
    assert_eq!(report["source_module_count"], serde_json::json!(2));

    let domains = report["domains"].as_array().expect("domains array");
    assert_eq!(domains.len(), 8);
    let by_domain = |name: &str| {
        domains
            .iter()
            .find(|d| d["domain"] == json_str(name))
            .unwrap_or_else(|| panic!("domain {name} missing"))
            .clone()
    };
    for domain in domains {
        assert!(!domain["reason"].as_str().expect("reason").is_empty());
        assert!(
            domain["support"] == json_str("unsupported")
                || domain["support"] == json_str("partial_domain_specific"),
            "support: {}",
            domain["support"]
        );
    }
    assert_eq!(
        by_domain("backend_functions")["evidence_count"],
        serde_json::json!(3)
    );
    assert_eq!(
        by_domain("throw_statements")["evidence_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        by_domain("interaction_effect_requests")["evidence_count"],
        serde_json::json!(0)
    );
    assert_eq!(
        by_domain("interaction_effect_requests")["support"],
        json_str("partial_domain_specific")
    );
    assert!(
        by_domain("asynchronous_tasks")["evidence_count"]
            .as_u64()
            .expect("count")
            >= 1
    );
    assert!(
        by_domain("state_and_mutation")["evidence_count"]
            .as_u64()
            .expect("count")
            >= 1
    );
    assert!(
        by_domain("resource_lifecycle")["evidence_count"]
            .as_u64()
            .expect("count")
            >= 1
    );
}

#[test]
fn gaps_without_tsconfig_reports_incomplete_and_fails() {
    let Some(typescript) = trusted_typescript() else {
        return;
    };
    let root = temp_project("cli-gaps-bare");
    write(&root, "src/only.ts", "export const only = 1;\n");

    let output = run_graph_command("gaps", &root, &typescript);
    assert_eq!(output.status.code(), Some(1));
    let report = stdout(&output);
    assert_eq!(report["status"], json_str("incomplete"));
    assert_eq!(report["translated"], serde_json::json!(false));
}
