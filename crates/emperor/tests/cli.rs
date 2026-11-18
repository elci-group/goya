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

#[test]
fn interaction_suite_dispatches_the_admitted_counter() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["test", &scene, "--suite", "interaction"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(run.json["count"].as_i64(), Some(3));
    assert_eq!(run.json["revision"].as_u64(), Some(2));
    assert_eq!(run.json["edit"], "2");
    assert_eq!(run.json["focus"], "button");
    assert_eq!(run.json["paste_ignored"], true);
    assert_eq!(run.json["copy_allowed"], true);
    assert_eq!(run.json["paste_allowed"], false);
    assert_eq!(run.json["add_hit"], true);
    assert_eq!(run.json["rects_stable"], true);
    assert_eq!(run.json["space_activates"], true);
    let stages = run.json["stages"].as_array().expect("stages");
    assert_eq!(stages.len(), 13);
    for stage in stages.iter().take(6) {
        assert_eq!(stage["status"], "PASS");
    }
    for stage in stages.iter().skip(6) {
        assert_eq!(stage["status"], "NOT_APPLICABLE");
        assert_eq!(
            stage["detail"],
            "interaction suite is scripted dispatch, not a seat or compositor launch"
        );
    }
    assert_eq!(stages[11]["name"], "runtime-conformance");
    assert_ne!(stages[11]["status"], "PASS");
    assert!(!run.json.to_string().contains("150 ms"));
    assert!(!run.text.contains("wayland"));
}

#[test]
fn format_json_before_the_scene_keeps_one_positional() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&[
        "check",
        "--format",
        "json",
        &scene,
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(run.json["output"], "json");
    assert_eq!(run.json["compiler"], "emperor");
    assert_ne!(run.json["diagnostic"], "EMP-USAGE");
}

#[test]
fn format_json_does_not_swallow_the_package_format() {
    let ahead = run(&[
        "package",
        "--format",
        "json",
        "--format",
        "linux-archive",
        "missing-demo-a-package-dir",
    ]);
    assert_eq!(ahead.code, 4, "{}", ahead.text);
    assert_eq!(ahead.json["diagnostic"], "EMP-ELF");
    let behind = run(&[
        "package",
        "--format",
        "linux-archive",
        "--format",
        "json",
        "missing-demo-a-package-dir",
    ]);
    assert_eq!(behind.code, 4, "{}", behind.text);
    assert_eq!(behind.json["diagnostic"], "EMP-ELF");
    let swallowed = run(&["package", "--format", "json", "missing-demo-a-package-dir"]);
    assert_eq!(swallowed.code, 2, "{}", swallowed.text);
    assert_eq!(swallowed.json["diagnostic"], "EMP-USAGE");
    assert!(
        swallowed.text.contains("package requires --format"),
        "{}",
        swallowed.text
    );
}

#[test]
fn window_backend_accepts_wayland_and_kumimanu_and_rejects_electron() {
    let scene = fixture("interactive-counter.gscene");
    let wayland = run(&[
        "check",
        &scene,
        "--window",
        "wayland",
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(wayland.code, 0, "{}", wayland.text);
    let kumimanu = run(&[
        "check",
        &scene,
        "--window",
        "kumimanu",
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(kumimanu.code, 0, "{}", kumimanu.text);
    assert_ne!(kumimanu.json["diagnostic"], "EMP-USAGE");
    let electron = run(&["check", &scene, "--window", "electron"]);
    assert_eq!(electron.code, 2, "{}", electron.text);
    assert_eq!(electron.json["diagnostic"], "EMP-USAGE");
    assert!(electron.text.contains("unknown window backend"));
    let duplicate = run(&[
        "check", &scene, "--window", "wayland", "--window", "kumimanu",
    ]);
    assert_eq!(duplicate.code, 2, "{}", duplicate.text);
    assert_eq!(duplicate.json["diagnostic"], "EMP-USAGE");
}

#[test]
fn desktop_sota_stays_blocked_and_unknown_suites_exit_2() {
    let scene = fixture("interactive-counter.gscene");
    let blocked = run(&["test", &scene, "--suite", "desktop-sota"]);
    assert_eq!(blocked.code, 3, "{}", blocked.text);
    assert!(!blocked.json.to_string().contains("150 ms"));
    let unknown = run(&["test", &scene, "--suite", "seat"]);
    assert_eq!(unknown.code, 2, "{}", unknown.text);
    let conformance = run(&["test", &scene, "--suite", "conformance"]);
    assert_eq!(conformance.code, 0, "{}", conformance.text);
    assert_eq!(conformance.json["count"].as_i64(), Some(1));
    assert_eq!(conformance.json["revision"].as_u64(), Some(1));
}
