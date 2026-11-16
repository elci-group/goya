use std::io::Write;
use std::process::Command;
use std::process::Stdio;

#[test]
fn swift_parse_reports_missing_source_before_missing_compiler() {
    let binary = env!("CARGO_BIN_EXE_neptune");
    let missing = Command::new(binary)
        .args(["swift-parse", "/definitely/missing/neptune.swift"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("could not read Swift source"));

    let unavailable = Command::new(binary)
        .args([
            "swift-parse",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/static.gscene"),
        ])
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(unavailable.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&unavailable.stderr).contains("swiftc unavailable"));
}

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
            .args([
                "preview",
                path,
                "--module",
                "tests/static.gscene",
                "--format",
                "json",
            ])
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
            .args([
                "preview",
                fixture,
                "--module",
                "tests/static.gscene",
                "--out",
            ])
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
    let swift_text = std::str::from_utf8(&swift).unwrap();
    let offset = swift_text.find("Text(").unwrap();
    let before = &swift_text.as_bytes()[..offset];
    let line = 1 + before.iter().filter(|byte| **byte == b'\n').count();
    let column = 1 + offset
        - before
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);
    let map = |source: &std::path::Path, line: usize, column: usize| {
        Command::new(env!("CARGO_BIN_EXE_neptune"))
            .arg("map-swift")
            .arg(&output_dir)
            .arg("--source")
            .arg(source)
            .arg("--line")
            .arg(line.to_string())
            .arg("--column")
            .arg(column.to_string())
            .args(["--format", "json"])
            .output()
            .unwrap()
    };
    let mapped = map(std::path::Path::new(fixture), line, column);
    assert!(mapped.status.success());
    let mapped: serde_json::Value = serde_json::from_slice(&mapped.stdout).unwrap();
    assert_eq!(mapped["status"], "MAPPED");
    assert_eq!(mapped["mappings"][0]["semantic_id"]["declared_id"], "root");
    assert_eq!(
        mapped["mappings"][0]["semantic_id"]["module"],
        "tests/static.gscene"
    );
    assert_eq!(mapped["verification"]["authenticity"], "NOT_CHECKED");
    let source = std::fs::read_to_string(fixture).unwrap();
    let bundle: serde_json::Value =
        serde_json::from_str(&neptune::preview_bundle(&source, "tests/static.gscene").unwrap())
            .unwrap();
    let request = serde_json::json!({"schema_version":"1","job_id":"mapped","operation":"map-swift","source":source,"bundle":bundle,"line":line,"column":column,"column_encoding":"utf8-byte","raw_diagnostic":"App.swift: error: example\n"});
    let mut session = neptune::service::Session::default();
    let service = session.handle(&request);
    assert_eq!(service["status"], "PASS");
    assert_eq!(service["result"], mapped);
    assert_eq!(service["raw_diagnostic"], "App.swift: error: example\n");
    let mut bad_request = request.clone();
    bad_request["job_id"] = serde_json::json!("tampered");
    bad_request["bundle"]["artifacts"]["App.swift"] = serde_json::json!("altered");
    assert_eq!(session.handle(&bad_request)["status"], "FAIL");
    let unmapped: serde_json::Value =
        serde_json::from_slice(&map(std::path::Path::new(fixture), 1, 1).stdout).unwrap();
    assert_eq!(unmapped["status"], "UNMAPPED");
    // A source map may associate multiple source spans with one Swift location.
    // Recompute the unsigned consistency hash for this synthetic map.
    use sha2::{Digest, Sha256};
    let map_path = output_dir.join("source-map.json");
    let report_path = output_dir.join("build-report.json");
    let original_map = std::fs::read(&map_path).unwrap();
    let original_report = std::fs::read(&report_path).unwrap();
    let mut expanded: serde_json::Value = serde_json::from_slice(&original_map).unwrap();
    let mut second_mapping = expanded["mappings"][0].clone();
    let start = second_mapping["source"]["start_byte"].as_u64().unwrap();
    second_mapping["source"]["start_byte"] = serde_json::json!(start + 6);
    expanded["mappings"]
        .as_array_mut()
        .unwrap()
        .push(second_mapping);
    let expanded_bytes = format!("{expanded}\n");
    let mut expanded_report: serde_json::Value = serde_json::from_slice(&original_report).unwrap();
    expanded_report["artifact_sha256"]["source-map.json"] =
        serde_json::json!(format!("{:x}", Sha256::digest(expanded_bytes.as_bytes())));
    std::fs::write(&map_path, expanded_bytes).unwrap();
    std::fs::write(&report_path, format!("{expanded_report}\n")).unwrap();
    let multiple: serde_json::Value =
        serde_json::from_slice(&map(std::path::Path::new(fixture), line, column).stdout).unwrap();
    assert_eq!(multiple["mappings"].as_array().unwrap().len(), 2);
    std::fs::write(&map_path, original_map).unwrap();
    std::fs::write(&report_path, original_report).unwrap();
    let changed = output_dir.join("changed.gscene");
    std::fs::write(&changed, b"different source").unwrap();
    assert!(!map(&changed, line, column).status.success());
    let lock_path = output_dir.join("toolchain-lock.json");
    let valid_lock = br#"{"schema_version":"1","xcode_version":"16.2","xcode_build":"16C5032a","swift_version":"6.0.3","sdk_version":"18.2","sdk_build":"22C150","minimum_os":"17.0"}"#;
    std::fs::write(&lock_path, valid_lock).unwrap();
    let plan = || {
        Command::new(env!("CARGO_BIN_EXE_neptune"))
            .arg("apple-plan")
            .arg(&output_dir)
            .arg("--lock")
            .arg(&lock_path)
            .args(["--format", "json"])
            .output()
            .unwrap()
    };
    let planned = plan();
    assert_eq!(planned.status.code(), Some(3));
    let plan_value: serde_json::Value = serde_json::from_slice(&planned.stdout).unwrap();
    assert_eq!(plan_value["status"], "BLOCKED");
    assert_eq!(plan_value["verification"]["apple_typecheck"], "BLOCKED");
    assert_eq!(
        plan_value["artifact_sha256"]["App.swift"],
        report["artifact_sha256"]["App.swift"]
    );
    assert_eq!(
        plan_value["invocation"]["arguments"][4],
        "arm64-apple-ios17.0-simulator"
    );
    assert!(!String::from_utf8_lossy(&planned.stdout).contains(output_dir.to_str().unwrap()));
    let invalid_lock = String::from_utf8(valid_lock.to_vec())
        .unwrap()
        .replace("\"minimum_os\":\"17.0\"", "\"minimum_os\":\"19.0\"");
    std::fs::write(&lock_path, invalid_lock).unwrap();
    assert_eq!(plan().status.code(), Some(1));
    std::fs::write(&lock_path, valid_lock).unwrap();
    std::fs::write(output_dir.join("App.swift"), b"modified").unwrap();
    assert!(!verify().status.success());
    assert!(!map(std::path::Path::new(fixture), line, column)
        .status
        .success());
    assert_eq!(plan().status.code(), Some(1));
    std::fs::remove_dir_all(output_dir).unwrap();
}

#[test]
fn preview_artifacts_match_across_three_fresh_input_and_output_directories() {
    let root =
        std::env::temp_dir().join(format!("neptune-determinism-test-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source =
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/static.gscene")).unwrap();
    let names = [
        "App.swift",
        "source-map.json",
        "capabilities.json",
        "build-report.json",
    ];
    let mut first: Option<Vec<Vec<u8>>> = None;
    for i in 0..3 {
        let run = root.join(format!("run-{i}"));
        std::fs::create_dir(&run).unwrap();
        std::fs::write(run.join("input.gscene"), &source).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_neptune"))
            .current_dir(&run)
            .args([
                "preview",
                "input.gscene",
                "--module",
                "app/main.gscene",
                "--out",
                "output",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let artifacts: Vec<_> = names
            .iter()
            .map(|name| std::fs::read(run.join("output").join(name)).unwrap())
            .collect();
        if let Some(expected) = &first {
            assert_eq!(&artifacts, expected);
        } else {
            first = Some(artifacts);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn transition_preview_cli_and_service_agree_and_require_experimental_profile() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/transition.gscene");
    let missing = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(["preview-transition", path, "--module", "app/main.gscene"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    let cli = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args([
            "preview-transition",
            path,
            "--module",
            "app/main.gscene",
            "--profile",
            "experimental",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(expected["stages"]["apple_validation"], "NOT_RUN");
    assert_eq!(expected["stages"]["runtime_limit_parity"], "NOT_VERIFIED");
    let source = std::fs::read_to_string(path).unwrap();
    let request = serde_json::json!({"schema_version":"1", "job_id":"job", "operation":"preview-transition", "profile":"experimental", "module":"app/main.gscene", "source":source});
    let mut session = neptune::service::Session::default();
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], expected);
}

#[test]
fn d1_corpus_reports_only_its_scope_and_admission_is_blocked() {
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(["test", "--suite", "d1-corpus", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["schema_version"], "2");
    assert_eq!(report["application_contract"], "GY-D1-APP/1");
    assert_eq!(report["cases"].as_array().unwrap().len(), 10);
    assert_eq!(report["stages"]["target_ir"], "PASS");
    assert_eq!(report["stages"]["application_owner"], "PASS");
    assert_eq!(report["stages"]["generated_swift"], "BLOCKED");
    assert_eq!(report["stages"]["physical_device"], "BLOCKED");
    let mut session = neptune::service::Session::default();
    let result = session.handle(&serde_json::json!({"schema_version":"1","job_id":"corpus","operation":"test","suite":"d1-corpus"}));
    assert_eq!(result["status"], "PASS");
    assert_eq!(result["result"], report);
    let blocked = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args([
            "test",
            "--suite",
            "admission",
            "--worker",
            "mac-primary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(blocked.status.code(), Some(3));
    let admission: serde_json::Value = serde_json::from_slice(&blocked.stdout).unwrap();
    assert_eq!(admission["status"], "BLOCKED");
}

#[test]
fn state_model_cli_and_service_share_validation_and_scope() {
    let scene_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/conformance/d1-increment.gscene"
    );
    let initial_path = concat!(env!("CARGO_MANIFEST_DIR"), "/conformance/d1-initial.json");
    let no_profile = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args([
            "model-d1-state",
            scene_path,
            "--module",
            "app/main.gscene",
            "--initial",
            initial_path,
        ])
        .output()
        .unwrap();
    assert!(!no_profile.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args([
            "model-d1-state",
            scene_path,
            "--module",
            "app/main.gscene",
            "--profile",
            "experimental",
            "--initial",
            initial_path,
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["scope"], "state_only");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    assert_eq!(result["state"][0]["initial"], 9);
    let source = std::fs::read_to_string(scene_path).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"model","operation":"model-d1-state","module":"app/main.gscene","profile":"experimental","source":source,"initial":initial});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut reordered = request.clone();
    reordered["job_id"] = serde_json::json!("owner-reordered");
    reordered["initial"].as_array_mut().unwrap().reverse();
    assert_eq!(session.handle(&reordered)["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad");
    bad["initial"] = serde_json::json!([["count", "int", 9], ["count", "int", 2]]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}

#[test]
fn multi_action_owner_preview_cli_and_service_match() {
    let scene_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/owner.gscene");
    let initial_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/owner-initial.json");
    let missing_profile = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args([
            "preview-d1-owner",
            scene_path,
            "--module",
            "app/main.gscene",
            "--initial",
            initial_path,
        ])
        .output()
        .unwrap();
    assert!(!missing_profile.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args([
            "preview-d1-owner",
            scene_path,
            "--module",
            "app/main.gscene",
            "--profile",
            "experimental",
            "--initial",
            initial_path,
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "SOURCE_PREVIEW");
    assert_eq!(result["scope"], "state_only");
    assert_eq!(result["actions"].as_array().unwrap().len(), 2);
    assert_eq!(result["stages"]["apple_validation"], "NOT_RUN");
    assert_eq!(result["stages"]["runtime_limit_parity"], "NOT_VERIFIED");
    let source = std::fs::read_to_string(scene_path).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"owner","operation":"preview-d1-owner","module":"app/main.gscene","profile":"experimental","source":source,"initial":initial});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut reordered = request.clone();
    reordered["job_id"] = serde_json::json!("owner-reordered");
    reordered["initial"].as_array_mut().unwrap().reverse();
    assert_eq!(session.handle(&reordered)["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad-owner");
    bad["initial"] = serde_json::json!([["count", "int", 4]]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}

#[test]
fn native_button_preview_cli_and_service_match_and_require_profile() {
    let scene = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/native-button.gscene");
    let initial_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/native-button-initial.json"
    );
    let base = [
        "preview-native-button",
        scene,
        "--module",
        "app/main.gscene",
        "--initial",
        initial_path,
    ];
    assert!(!Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .output()
        .unwrap()
        .status
        .success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .args(["--profile", "experimental", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "SOURCE_PREVIEW");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    let source = std::fs::read_to_string(scene).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1", "job_id":"native", "operation":"preview-native-button", "module":"app/main.gscene", "profile":"experimental", "source":source, "initial":initial});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad-native");
    bad["initial"] = serde_json::json!([]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}

#[test]
fn static_column_preview_cli_and_service_match_and_require_profile() {
    let scene = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/static-column.gscene");
    let base = [
        "preview-static-column",
        scene,
        "--module",
        "app/main.gscene",
    ];
    assert!(!Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .output()
        .unwrap()
        .status
        .success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .args(["--profile", "experimental", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "SOURCE_PREVIEW");
    assert_eq!(result["scope"], "static_text_column");
    assert_eq!(result["cell_count"], 3);
    assert_eq!(result["stages"]["layout_fidelity"], "NOT_CLAIMED");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    let swift = result["swift"].as_str().unwrap();
    let hello = swift.find("Text(verbatim: \"Hello\")").unwrap();
    let title = swift.find("Text(verbatim: \"Title\")").unwrap();
    let note = swift.find("Text(verbatim: \"Note\")").unwrap();
    assert!(hello < title && title < note);
    let source = std::fs::read_to_string(scene).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"column","operation":"preview-static-column","module":"app/main.gscene","profile":"experimental","source":source});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut missing = request.clone();
    missing["job_id"] = serde_json::json!("column-missing-profile");
    missing.as_object_mut().unwrap().remove("profile");
    assert_eq!(session.handle(&missing)["status"], "FAIL");
}

#[test]
fn observable_button_preview_cli_and_service_match_and_require_profile() {
    let scene = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/native-button.gscene");
    let initial_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/native-button-initial.json"
    );
    let base = [
        "preview-observable-button",
        scene,
        "--module",
        "app/main.gscene",
        "--initial",
        initial_path,
    ];
    assert!(!Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .output()
        .unwrap()
        .status
        .success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .args(["--profile", "experimental", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "SOURCE_PREVIEW");
    assert_eq!(result["scope"], "single_button_readout");
    assert_eq!(result["stages"]["state_readout"], "SOURCE_PREVIEW");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    let swift = result["swift"].as_str().unwrap();
    let value = swift
        .find("Text(verbatim: String(store.state.s0))")
        .unwrap();
    let revision = swift
        .find("Text(verbatim: String(store.state.revision))")
        .unwrap();
    assert!(value < revision);
    assert_eq!(swift.matches("store.dispatch0").count(), 1);
    let source = std::fs::read_to_string(scene).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"observable","operation":"preview-observable-button","module":"app/main.gscene","profile":"experimental","source":source,"initial":initial});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad-observable");
    bad["initial"] = serde_json::json!([]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}

#[test]
fn observable_column_preview_cli_and_service_match_and_require_profile() {
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-column.gscene"
    );
    let initial_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-column-initial.json"
    );
    let base = [
        "preview-observable-column",
        scene,
        "--module",
        "app/main.gscene",
        "--initial",
        initial_path,
    ];
    assert!(!Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .output()
        .unwrap()
        .status
        .success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .args(["--profile", "experimental", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "SOURCE_PREVIEW");
    assert_eq!(result["scope"], "static_text_and_one_button");
    assert_eq!(result["node_count"], 2);
    assert_eq!(result["stages"]["state_readout"], "SOURCE_PREVIEW");
    assert_eq!(result["stages"]["layout_fidelity"], "NOT_CLAIMED");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    let swift = result["swift"].as_str().unwrap();
    let heading = swift.find("Text(verbatim: \"Counter\")").unwrap();
    let label = swift.find("Text(verbatim: \"Increment\")").unwrap();
    let value = swift
        .find("Text(verbatim: String(store.state.s0))")
        .unwrap();
    let revision = swift
        .find("Text(verbatim: String(store.state.revision))")
        .unwrap();
    assert!(heading < label && label < value && value < revision);
    assert_eq!(swift.matches("store.dispatch0").count(), 1);
    let source = std::fs::read_to_string(scene).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"observable-column","operation":"preview-observable-column","module":"app/main.gscene","profile":"experimental","source":source,"initial":initial});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad-observable-column");
    bad["initial"] = serde_json::json!([]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}

#[test]
fn observable_buttons_preview_cli_and_service_match_and_require_profile() {
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-buttons.gscene"
    );
    let initial_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-buttons-initial.json"
    );
    let base = [
        "preview-observable-buttons",
        scene,
        "--module",
        "app/main.gscene",
        "--initial",
        initial_path,
    ];
    assert!(!Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .output()
        .unwrap()
        .status
        .success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .args(["--profile", "experimental", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "SOURCE_PREVIEW");
    assert_eq!(result["scope"], "static_text_and_buttons");
    assert_eq!(result["node_count"], 3);
    assert_eq!(result["button_count"], 2);
    assert_eq!(result["actions"][0]["method"], "dispatch0");
    assert_eq!(result["actions"][0]["trigger"], "button.mark");
    assert_eq!(result["actions"][1]["method"], "dispatch1");
    assert_eq!(result["actions"][1]["trigger"], "button.up");
    assert_eq!(result["stages"]["state_readout"], "SOURCE_PREVIEW");
    assert_eq!(result["stages"]["layout_fidelity"], "NOT_CLAIMED");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    let swift = result["swift"].as_str().unwrap();
    let up = swift.find("Text(verbatim: \"Up\")").unwrap();
    let mark = swift.find("Text(verbatim: \"Mark\")").unwrap();
    assert!(swift.find("store.dispatch1(").unwrap() < up);
    assert!(up < swift.find("store.dispatch0(").unwrap());
    assert!(swift.find("store.dispatch0(").unwrap() < mark);
    let source = std::fs::read_to_string(scene).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"observable-buttons","operation":"preview-observable-buttons","module":"app/main.gscene","profile":"experimental","source":source,"initial":initial});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad-observable-buttons");
    bad["initial"] = serde_json::json!([]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}

#[test]
fn observable_button_trace_cli_and_service_match_and_require_profile() {
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-buttons.gscene"
    );
    let initial_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-buttons-initial.json"
    );
    let activations_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-buttons-activations.json"
    );
    let base = [
        "trace-observable-buttons",
        scene,
        "--module",
        "app/main.gscene",
        "--initial",
        initial_path,
        "--activations",
        activations_path,
    ];
    assert!(!Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .output()
        .unwrap()
        .status
        .success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .args(["--profile", "experimental", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "HOST_TRACE");
    assert_eq!(result["scope"], "button_activation_trace");
    assert_eq!(result["activation_count"], 3);
    assert_eq!(result["stages"]["host_trace"], "PASS");
    assert_eq!(result["stages"]["swift_execution"], "NOT_RUN");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    let steps = result["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[1]["method"], "dispatch1");
    assert_eq!(steps[1]["status"], "PASS");
    assert_eq!(steps[2]["method"], "dispatch0");
    assert_eq!(steps[3]["revision"], 3);
    assert_eq!(steps[3]["state"][0]["value"], 2);
    assert_eq!(steps[3]["state"][1]["value"], "ok");
    let source = std::fs::read_to_string(scene).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let activations: serde_json::Value =
        serde_json::from_slice(&std::fs::read(activations_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"observable-trace","operation":"trace-observable-buttons","module":"app/main.gscene","profile":"experimental","source":source,"initial":initial,"activations":activations});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad-observable-trace");
    bad["activations"] = serde_json::json!(["action.missing"]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}

#[test]
fn observable_column_trace_cli_and_service_match_and_require_profile() {
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-column.gscene"
    );
    let initial_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-column-initial.json"
    );
    let activations_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/observable-column-activations.json"
    );
    let base = [
        "trace-observable-column",
        scene,
        "--module",
        "app/main.gscene",
        "--initial",
        initial_path,
        "--activations",
        activations_path,
    ];
    assert!(!Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .output()
        .unwrap()
        .status
        .success());
    let output = Command::new(env!("CARGO_BIN_EXE_neptune"))
        .args(base)
        .args(["--profile", "experimental", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"], "HOST_TRACE");
    assert_eq!(result["scope"], "single_button_activation_trace");
    assert_eq!(result["activation_count"], 2);
    assert_eq!(result["stages"]["swift_execution"], "NOT_RUN");
    assert_eq!(result["stages"]["ios_admission"], "NOT_RUN");
    let steps = result["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[1]["method"], "dispatch0");
    assert_eq!(steps[2]["revision"], 2);
    assert_eq!(steps[2]["state"][0]["value"], 2);
    let source = std::fs::read_to_string(scene).unwrap();
    let initial: serde_json::Value =
        serde_json::from_slice(&std::fs::read(initial_path).unwrap()).unwrap();
    let activations: serde_json::Value =
        serde_json::from_slice(&std::fs::read(activations_path).unwrap()).unwrap();
    let mut session = neptune::service::Session::default();
    let request = serde_json::json!({"schema_version":"1","job_id":"observable-column-trace","operation":"trace-observable-column","module":"app/main.gscene","profile":"experimental","source":source,"initial":initial,"activations":activations});
    let response = session.handle(&request);
    assert_eq!(response["status"], "PASS");
    assert_eq!(response["result"], result);
    let mut bad = request;
    bad["job_id"] = serde_json::json!("bad-observable-column-trace");
    bad["activations"] = serde_json::json!(["action.missing"]);
    assert_eq!(session.handle(&bad)["status"], "FAIL");
}
