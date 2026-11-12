use std::process::Command;

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
            .args(["preview", path, "--format", "json"])
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
