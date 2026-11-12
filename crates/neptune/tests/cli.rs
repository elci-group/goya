use std::io::Write;
use std::process::Command;
use std::process::Stdio;

#[test]
fn missing_input_has_machine_readable_failure() {
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args([
            "check",
            "/definitely/missing/neptune.gscene",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], "1");
    assert_eq!(report["stages"]["goya_parse"], "NOT_RUN");
    assert_eq!(report["diagnostics"][0]["code"], "NPT-E1000");
}

#[test]
fn valid_scene_inspects_but_fails_target_check() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/static.gscene");
    for (command, success) in [("inspect", true), ("check", false)] {
        let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
            .args([command, path, "--target", "ios", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["stages"]["goya_parse"], "PASS");
        assert_eq!(report["stages"]["ios_validation"], "FAIL");
        assert_eq!(report["stages"]["apple_validation"], "NOT_RUN");
    }
}

#[test]
fn preview_bytes_are_independent_of_process_working_directory() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/static.gscene");
    let mut outputs = Vec::new();
    for directory in ["/", "/tmp", env!("CARGO_MANIFEST_DIR")] {
        let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
            .current_dir(directory)
            .args(["preview", path, "--module", "tests/static.gscene", "--format", "json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        outputs.push(output.stdout);
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[1], outputs[2]);
    let bundle: serde_json::Value = serde_json::from_slice(&outputs[0]).unwrap();
    let capabilities: serde_json::Value =
        serde_json::from_str(bundle["artifacts"]["capabilities.json"].as_str().unwrap()).unwrap();
    assert_eq!(capabilities["capabilities"][0]["status"], "experimental");
    assert_eq!(bundle["report"]["stages"]["apple_validation"], "NOT_RUN");
}

#[test]
fn service_stream_reports_separate_job_results() {
    let mut process = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = process.stdin.take().unwrap();
    writeln!(
        input,
        "{{\"schema_version\":\"1\",\"job_id\":\"a\",\"operation\":\"build\"}}"
    )
    .unwrap();
    writeln!(input, "{{\"schema_version\":\"1\",\"job_id\":\"b\",\"operation\":\"check\",\"source\":\"invalid\"}}").unwrap();
    drop(input);
    let output = process.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["job_id"], "a");
    assert_eq!(lines[1]["job_id"], "b");
    assert_eq!(lines[1]["status"], "FAIL");
}

#[test]
fn preview_writes_new_directory_and_refuses_overwrite() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/static.gscene");
    let output_dir =
        std::env::temp_dir().join(format!("neptune-preview-test-{}", std::process::id()));
    assert!(!output_dir.exists());
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_neptune"))
            .args(["preview", fixture, "--module", "tests/static.gscene", "--out"])
            .arg(&output_dir)
            .args(["--format", "json"])
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let swift = std::fs::read(output_dir.join("App.swift")).unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("build-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["stages"]["apple_validation"], "NOT_RUN");
    assert!(output_dir.join("source-map.json").is_file());
    assert!(output_dir.join("capabilities.json").is_file());
    let second = run();
    assert!(!second.status.success());
    assert_eq!(swift, std::fs::read(output_dir.join("App.swift")).unwrap());
    let verify = || {
        Command::new(env!("CARGO_BIN_EXE_neptune"))
            .arg("verify-preview")
            .arg(&output_dir)
            .args(["--format", "json"])
            .output()
            .unwrap()
    };
    let checked = verify();
    assert!(checked.status.success());
    let response: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(response["authenticity"], "NOT_CHECKED");
    std::fs::write(output_dir.join("App.swift"), b"modified").unwrap();
    assert!(!verify().status.success());
    std::fs::remove_dir_all(output_dir).unwrap();
}
