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
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/clock.gscene");
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
