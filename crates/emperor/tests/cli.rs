use std::ffi::OsString;
use std::path::PathBuf;

fn run(args: &[&str]) -> emperor::orchestrate::Run {
    let mut full = vec![OsString::from("emperor")];
    full.extend(args.iter().map(OsString::from));
    emperor::cli::execute(&full)
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../sash/fixtures")
        .join(name);
    let path = path.canonicalize().expect("fixture");
    path.display().to_string()
}

#[test]
fn check_counter_exits_zero_and_names_the_profile() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&[
        "check",
        &scene,
        "--target",
        "x86_64-unknown-linux-gnu",
        "--format",
        "json",
    ]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(run.json["compiler"], "emperor");
    assert_eq!(run.json["directive"], "GY-EMP-001");
    let stages = run.json["stages"].as_array().expect("stages");
    assert_eq!(stages.len(), 13);
    assert_eq!(stages[5]["status"], "PASS");
    assert_eq!(stages[6]["status"], "NOT_APPLICABLE");
    assert_eq!(stages[11]["status"], "BLOCKED");
    assert!(stages.iter().all(|stage| stage["status"] != "blocked"));
}

#[test]
fn static_scene_is_a_source_error() {
    let scene = fixture("static-window.gscene");
    let run = run(&["check", &scene, "--target", "x86_64-unknown-linux-gnu"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert_eq!(run.json["diagnostic"], "SASH-CAP");
}

#[test]
fn musl_target_is_blocked() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["check", &scene, "--target", "x86_64-unknown-linux-musl"]);
    assert_eq!(run.code, 3, "{}", run.text);
    let stages = run.json["stages"].as_array().expect("stages");
    assert!(stages
        .iter()
        .all(|stage| stage["status"] != "PASS" || stage["id"].as_u64().unwrap() < 5));
    assert_eq!(stages[4]["status"], "BLOCKED");
}

#[test]
fn unknown_target_is_rejected() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["check", &scene, "--target", "wasm32-unknown-unknown"]);
    assert_eq!(run.code, 1, "{}", run.text);
}

#[test]
fn parent_directory_output_is_rejected() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["build", &scene, "--out", "dist/../escape"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert_eq!(run.json["diagnostic"], "EMP-PATH");
}

#[test]
fn benchmark_does_not_invent_a_number() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["benchmark", &scene, "--suite", "desktop-sota"]);
    assert_eq!(run.code, 3, "{}", run.text);
    let rendered = run.json.to_string();
    assert!(!rendered.contains("150 ms"));
    assert_eq!(run.json["stages"][0]["status"], "BLOCKED");
}
