use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, Read, Write},
    process::ExitCode,
};

fn write_preview_bundle(path: &std::path::Path, bundle: &str) -> io::Result<()> {
    let value: serde_json::Value = serde_json::from_str(bundle).map_err(io::Error::other)?;
    let artifacts = value["artifacts"]
        .as_object()
        .ok_or_else(|| io::Error::other("invalid preview artifacts"))?;
    let report = value
        .get("report")
        .ok_or_else(|| io::Error::other("missing preview report"))?;
    // Prepare all bytes before creating the output directory. create_dir and
    // create_new reject existing paths rather than replacing user files.
    let mut files = Vec::new();
    for name in [
        "App.swift",
        "source-map.json",
        "capabilities.json",
        "AndroidInputPreview.kt",
        "AndroidD1Runtime.kt",
    ] {
        let content = artifacts
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| io::Error::other("missing preview artifact"))?;
        files.push((name, content.to_owned()));
    }
    files.push(("build-report.json", format!("{report}\n")));
    fs::create_dir(path)?;
    for (name, content) in files {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join(name))?
            .write_all(content.as_bytes())?;
    }
    Ok(())
}

fn serve() -> ExitCode {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut session = neptune::service::Session::default();
    loop {
        let mut line = Vec::new();
        let count = match (&mut input)
            .take(neptune::service::MAX_REQUEST_BYTES + 1)
            .read_until(b'\n', &mut line)
        {
            Ok(count) => count,
            Err(_) => return ExitCode::FAILURE,
        };
        if count == 0 {
            return ExitCode::SUCCESS;
        }
        if count as u64 > neptune::service::MAX_REQUEST_BYTES || line.last() != Some(&b'\n') {
            let _ = writeln!(
                output,
                "{}",
                serde_json::json!({"schema_version":"1","job_id":null,"sequence":1,"stage":"protocol","status":"FAIL","diagnostics":[{"code":"NPT-E0001","message":"request too large or missing newline"}]})
            );
            return ExitCode::FAILURE;
        }
        let response = match serde_json::from_slice::<serde_json::Value>(&line) {
            Ok(request) => session.handle(&request),
            Err(_) => {
                serde_json::json!({"schema_version":"1","job_id":null,"sequence":1,"stage":"protocol","status":"FAIL","diagnostics":[{"code":"NPT-E0001","message":"invalid JSON"}]})
            }
        };
        if writeln!(output, "{response}")
            .and_then(|_| output.flush())
            .is_err()
        {
            return ExitCode::FAILURE;
        }
    }
}

fn input_error(json: bool, operation: &std::ffi::OsStr, code: &str, message: &str) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::json!({"schema_version":"1", "operation":operation.to_string_lossy(), "target":"ios", "stages":{"goya_parse":"NOT_RUN", "ios_validation":"NOT_RUN", "apple_validation":"NOT_RUN"}, "diagnostics":[{"code":code, "severity":"error", "stage":"ingest", "message":message, "source_range":null}]})
        );
    } else {
        eprintln!("{code}: {message}");
    }
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    if args.len() == 2 && args[1] == "serve" {
        return serve();
    }
    if args.len() == 3 && args[1] == "swift-parse" {
        let source = (|| -> io::Result<String> {
            let file = File::open(&args[2])?;
            let mut bytes = Vec::new();
            file.take(neptune::service::MAX_REQUEST_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > neptune::service::MAX_REQUEST_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "source exceeds 256 KiB",
                ));
            }
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })();
        let source = match source {
            Ok(source) => source,
            Err(error) => {
                eprintln!("could not read Swift source: {error}");
                return ExitCode::FAILURE;
            }
        };
        let result = neptune::service::parse_swift_source(&source, "cli");
        return match result["status"].as_str() {
            Some("PASS") => {
                println!("{{\"schema_version\":\"1\",\"operation\":\"swift-parse\",\"status\":\"PASS\",\"validation\":\"swift_syntax_only\",\"apple_sdk_validation\":\"NOT_RUN\"}}");
                ExitCode::SUCCESS
            }
            Some("BLOCKED") => {
                eprintln!(
                    "{}",
                    result["reason"].as_str().unwrap_or("swiftc unavailable")
                );
                ExitCode::from(3)
            }
            _ => {
                if let Some(text) = result["diagnostics"].as_str() {
                    eprintln!("{text}");
                    if result["diagnostics_truncated"] == true {
                        eprintln!("[diagnostics truncated]");
                    }
                } else {
                    eprintln!(
                        "{}",
                        result["diagnostics"][0]["message"]
                            .as_str()
                            .unwrap_or("Swift parse failed")
                    );
                }
                ExitCode::FAILURE
            }
        };
    }
    if args.len() >= 2 && args[1] == "dock-verify" {
        if args.len() < 5 || args[3] != "--ipa" {
            eprintln!("usage: neptune dock-verify <manifest.json> --ipa <application.ipa>");
            return ExitCode::from(2);
        }
        let result = (|| -> Result<(), String> {
            let manifest_bytes = fs::read(&args[2]).map_err(|e| e.to_string())?;
            if manifest_bytes.len() > 1024 * 1024 {
                return Err("manifest exceeds 1 MiB".into());
            }
            let manifest: neptune::dock::Manifest = serde_json::from_slice(&manifest_bytes)
                .map_err(|e| format!("invalid manifest: {e}"))?;
            manifest.validate().map_err(|e| e.to_string())?;
            let manifest_path = std::path::Path::new(&args[2]);
            let root = manifest_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            let declared = root
                .join(&manifest.artifact.path)
                .canonicalize()
                .map_err(|e| format!("declared artifact unavailable: {e}"))?;
            let supplied = std::path::Path::new(&args[4])
                .canonicalize()
                .map_err(|e| format!("supplied IPA unavailable: {e}"))?;
            let canonical_root = root
                .canonicalize()
                .map_err(|e| format!("manifest directory unavailable: {e}"))?;
            if !declared.starts_with(&canonical_root) {
                return Err("declared artifact escapes manifest directory".into());
            }
            if declared != supplied {
                return Err("--ipa does not match the manifest artifact path".into());
            }
            let ipa = File::open(&declared).map_err(|e| e.to_string())?;
            manifest
                .verify_artifact_reader(ipa)
                .map_err(|e| e.to_string())
        })();
        return match result {
            Ok(()) => {
                println!("{{\"schema_version\":\"1\",\"operation\":\"dock-verify\",\"status\":\"PASS\",\"device_installation\":\"NOT_RUN\",\"signature_trust\":\"NOT_CHECKED\"}}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("dock verification failed: {message}");
                ExitCode::FAILURE
            }
        };
    }
    if args.len() >= 2 && args[1] == "test" {
        let mut suite = None;
        let mut worker = None;
        let mut json = false;
        for option in args[2..].chunks(2) {
            if option.len() != 2 {
                eprintln!("missing option value");
                return ExitCode::from(2);
            }
            if option[0] == "--suite" && suite.is_none() {
                suite = option[1].to_str();
            } else if option[0] == "--worker" && worker.is_none() {
                worker = option[1].to_str();
            } else if option[0] == "--format" && option[1] == "json" && !json {
                json = true;
            } else {
                eprintln!("unknown or duplicate option");
                return ExitCode::from(2);
            }
        }
        return match suite {
            Some("d1-corpus") if worker.is_none() => match neptune::conformance::run_d1_corpus() {
                Ok(report) => {
                    if json {
                        println!("{}", report.json);
                    } else {
                        println!(
                            "D1 scalar corpus: {}; Swift and Apple validation are BLOCKED",
                            if report.passed { "PASS" } else { "FAIL" }
                        );
                    }
                    if report.passed {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(error) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({"schema_version":"2","suite":"d1-scalar-corpus","status":"FAIL","error":error})
                        );
                    } else {
                        eprintln!("D1 corpus failed: {error}");
                    }
                    ExitCode::FAILURE
                }
            },
            Some("admission") => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","suite":"admission","status":"BLOCKED","reason":"Apple worker and device admission are not implemented","worker":worker})
                    );
                } else {
                    eprintln!(
                        "Admission BLOCKED: Apple worker and device admission are not implemented"
                    );
                }
                ExitCode::from(3)
            }
            _ => {
                eprintln!("usage: neptune test --suite <d1-corpus|admission> [--worker name] [--format json]");
                ExitCode::from(2)
            }
        };
    }
    if args.len() >= 2 && args[1] == "map-swift" {
        if args.len() < 3 {
            eprintln!("usage: neptune map-swift <preview-directory> --source <file> --line <n> --column <utf8-byte-column> [--format json]");
            return ExitCode::from(2);
        }
        let (mut source_path, mut line, mut column) = (None, None, None);
        let mut json = false;
        for option in args[3..].chunks(2) {
            if option.len() != 2 {
                eprintln!("missing option value");
                return ExitCode::from(2);
            }
            if option[0] == "--source" && source_path.is_none() {
                source_path = Some(std::path::PathBuf::from(&option[1]));
            } else if option[0] == "--line" && line.is_none() {
                line = option[1]
                    .to_str()
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0);
                if line.is_none() {
                    eprintln!("invalid line");
                    return ExitCode::from(2);
                }
            } else if option[0] == "--column" && column.is_none() {
                column = option[1]
                    .to_str()
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0);
                if column.is_none() {
                    eprintln!("invalid column");
                    return ExitCode::from(2);
                }
            } else if option[0] == "--format" && option[1] == "json" && !json {
                json = true;
            } else {
                eprintln!("unknown or duplicate option");
                return ExitCode::from(2);
            }
        }
        let (Some(source_path), Some(line), Some(column)) = (source_path, line, column) else {
            eprintln!("--source, --line and --column are required");
            return ExitCode::from(2);
        };
        let mapped = (|| -> Result<serde_json::Value, String> {
            let mut bytes = Vec::new();
            File::open(source_path)
                .and_then(|file| {
                    file.take((neptune::MAX_SOURCE_BYTES + 1) as u64)
                        .read_to_end(&mut bytes)
                })
                .map_err(|e| e.to_string())?;
            if bytes.len() > neptune::MAX_SOURCE_BYTES {
                return Err("source exceeds 128 KiB".into());
            }
            let source = std::str::from_utf8(&bytes).map_err(|_| "source must be UTF-8")?;
            neptune::source_map::map_swift_location(
                std::path::Path::new(&args[2]),
                source,
                line,
                column,
            )
        })();
        return match mapped {
            Ok(value) => {
                if json {
                    println!("{value}");
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
                    );
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","status":"FAIL","message":error})
                    );
                } else {
                    eprintln!("source mapping failed: {error}");
                }
                ExitCode::FAILURE
            }
        };
    }
    if args.len() >= 2 && args[1] == "apple-plan" {
        if !(args.len() == 5 || (args.len() == 7 && args[5] == "--format" && args[6] == "json"))
            || args[3] != "--lock"
        {
            eprintln!("usage: neptune apple-plan <preview-directory> --lock <toolchain-lock.json> [--format json]");
            return ExitCode::from(2);
        }
        let json = args.len() == 7;
        let plan = (|| -> Result<serde_json::Value, String> {
            let metadata = fs::symlink_metadata(&args[4]).map_err(|e| e.to_string())?;
            if !metadata.file_type().is_file() || metadata.len() > 16384 {
                return Err("toolchain lock must be a regular file no larger than 16 KiB".into());
            }
            let mut bytes = Vec::new();
            File::open(&args[4])
                .and_then(|file| file.take(16385).read_to_end(&mut bytes))
                .map_err(|e| e.to_string())?;
            if bytes.len() > 16384 {
                return Err("toolchain lock exceeds 16 KiB".into());
            }
            let lock: neptune::apple::ToolchainLock = serde_json::from_slice(&bytes)
                .map_err(|e| format!("invalid toolchain lock JSON: {e}"))?;
            neptune::apple::plan_simulator_typecheck(std::path::Path::new(&args[2]), &lock)
        })();
        return match plan {
            Ok(plan) => {
                if json {
                    println!("{plan}");
                } else {
                    println!("Apple simulator type-check plan created; BLOCKED until an authorized Mac worker executes it");
                }
                ExitCode::from(3)
            }
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","operation":"apple-simulator-typecheck-plan","status":"FAIL","message":error})
                    );
                } else {
                    eprintln!("Apple type-check plan failed: {error}");
                }
                ExitCode::FAILURE
            }
        };
    }
    if args.len() >= 2 && args[1] == "verify-preview" {
        if !(args.len() == 3 || (args.len() == 5 && args[3] == "--format" && args[4] == "json")) {
            eprintln!("usage: neptune verify-preview <directory> [--format json]");
            return ExitCode::from(2);
        }
        let json = args.len() == 5;
        return match neptune::artifacts::verify_preview(std::path::Path::new(&args[2])) {
            Ok(()) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","operation":"verify-preview","integrity":"PASS","authenticity":"NOT_CHECKED","apple_validation":"NOT_RUN"})
                    );
                } else {
                    println!("Preview artifact hashes match; authenticity and Apple validation were not checked");
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","operation":"verify-preview","integrity":"FAIL","message":error.to_string(),"apple_validation":"NOT_RUN"})
                    );
                } else {
                    eprintln!("Preview integrity check failed: {error}");
                }
                ExitCode::FAILURE
            }
        };
    }
    if args.len() == 3 && args[1] == "explain" {
        let description = match args[2].to_str() {
            Some("NPT-E1000") => "Input could not be read. Check the path and file permissions.",
            Some("NPT-E1001") => "The input exceeds the 128 KiB source limit.",
            Some("NPT-E1002") => "The Goya frontend could not parse the UTF-8 scene. Correct the source at the reported line.",
            Some("NPT-E1003") => "The source exceeds a frontend nesting or addition-operator limit.",
            Some("NPT-E1004") => "The diagnostic limit was reached; fix reported issues and check again.",
            Some("NPT-E1005") => "The source preview could not be written. Use a new output directory and check permissions.",
            Some("NPT-E1006") => "Supply a canonical project-relative module path ending in .gscene.",
            Some("NPT-E2104") => "The source requires an iOS mapping that Neptune has not admitted. Remove the construct or wait for an admitted implementation.",
            Some("NPT-E3102") => "The transition's state or input binding could not be resolved safely.",
            Some("GY-E1000") => "Goya could not parse the source. Correct the syntax at the reported line.",
            Some("GY-E1001") => "Goya does not preserve this property. Remove it or use a supported property.",
            Some("GY-E1002") => "Goya does not preserve template semantics in strict parsing. Remove the template.",
            Some("GY-E1003") => "A property is declared more than once on the same scene node. Keep one declaration.",
            Some("GY-E1004") => "Goya could not validate the interaction contract. Correct the reported interaction error.",
            Some("GY-E1005") => "A scene element is missing an authored ID, or the same ID is used twice.",
            _ => { eprintln!("unknown diagnostic code"); return ExitCode::FAILURE; }
        };
        println!("{description}");
        return ExitCode::SUCCESS;
    }
    if args.len() < 3
        || (args[1] != "inspect"
            && args[1] != "check"
            && args[1] != "preview"
            && args[1] != "preview-transition"
            && args[1] != "model-d1-state"
            && args[1] != "preview-d1-owner"
            && args[1] != "preview-native-button"
            && args[1] != "preview-observable-button"
            && args[1] != "preview-observable-column"
            && args[1] != "preview-observable-buttons"
            && args[1] != "trace-observable-buttons"
            && args[1] != "trace-observable-column"
            && args[1] != "preview-static-column")
    {
        eprintln!("usage: neptune <inspect|check> <scene.gscene> [--target ios] [--format json]\n       neptune preview <scene.gscene> --module <project-relative.gscene> [--format json] [--out new-directory]\n       neptune preview-static-column <scene.gscene> --module <project-relative.gscene> --profile experimental [--format json]\n       neptune preview-transition <scene.gscene> --module <project-relative.gscene> --profile experimental [--format json]\n       neptune <model-d1-state|preview-d1-owner|preview-native-button|preview-observable-button|preview-observable-column|preview-observable-buttons> <scene.gscene> --module <project-relative.gscene> --profile experimental --initial <state.json> [--format json]\n       neptune <trace-observable-buttons|trace-observable-column> <scene.gscene> --module <project-relative.gscene> --profile experimental --initial <state.json> --activations <actions.json> [--format json]\n       neptune explain <code>");
        return ExitCode::from(2);
    }
    let mut json = false;
    let mut target = false;
    let mut out = None;
    let mut module = None;
    let mut experimental = false;
    let mut initial_path = None;
    let mut activations_path = None;
    for option in args[3..].chunks(2) {
        if option.len() != 2 {
            eprintln!("missing option value");
            return ExitCode::from(2);
        }
        if option[0] == "--format" && option[1] == "json" && !json {
            json = true;
        } else if option[0] == "--target" && option[1] == "ios" && !target {
            target = true;
        } else if option[0] == "--out" && args[1] == "preview" && out.is_none() {
            out = Some(std::path::PathBuf::from(&option[1]));
        } else if option[0] == "--profile"
            && (args[1] == "preview-transition"
                || args[1] == "model-d1-state"
                || args[1] == "preview-d1-owner"
                || args[1] == "preview-native-button"
                || args[1] == "preview-observable-button"
                || args[1] == "preview-observable-column"
                || args[1] == "preview-observable-buttons"
                || args[1] == "trace-observable-buttons"
                || args[1] == "trace-observable-column"
                || args[1] == "preview-static-column")
            && option[1] == "experimental"
            && !experimental
        {
            experimental = true;
        } else if option[0] == "--module"
            && (args[1] == "preview"
                || args[1] == "preview-transition"
                || args[1] == "model-d1-state"
                || args[1] == "preview-d1-owner"
                || args[1] == "preview-native-button"
                || args[1] == "preview-observable-button"
                || args[1] == "preview-observable-column"
                || args[1] == "preview-observable-buttons"
                || args[1] == "trace-observable-buttons"
                || args[1] == "trace-observable-column"
                || args[1] == "preview-static-column")
            && module.is_none()
        {
            module = option[1].to_str();
            if module.is_none() {
                eprintln!("module path must be UTF-8");
                return ExitCode::from(2);
            }
        } else if option[0] == "--initial"
            && (args[1] == "model-d1-state"
                || args[1] == "preview-d1-owner"
                || args[1] == "preview-native-button"
                || args[1] == "preview-observable-button"
                || args[1] == "preview-observable-column"
                || args[1] == "preview-observable-buttons"
                || args[1] == "trace-observable-buttons"
                || args[1] == "trace-observable-column")
            && initial_path.is_none()
        {
            initial_path = Some(std::path::PathBuf::from(&option[1]));
        } else if option[0] == "--activations"
            && (args[1] == "trace-observable-buttons" || args[1] == "trace-observable-column")
            && activations_path.is_none()
        {
            activations_path = Some(std::path::PathBuf::from(&option[1]));
        } else {
            eprintln!("unknown or duplicate option");
            return ExitCode::from(2);
        }
    }
    if (args[1] == "preview"
        || args[1] == "preview-transition"
        || args[1] == "model-d1-state"
        || args[1] == "preview-d1-owner"
        || args[1] == "preview-native-button"
        || args[1] == "preview-observable-button"
        || args[1] == "preview-observable-column"
        || args[1] == "preview-observable-buttons"
        || args[1] == "trace-observable-buttons"
        || args[1] == "trace-observable-column"
        || args[1] == "preview-static-column")
        && module.is_none()
    {
        eprintln!("preview requires --module <project-relative-path.gscene>");
        return ExitCode::from(2);
    }
    if (args[1] == "preview-transition"
        || args[1] == "model-d1-state"
        || args[1] == "preview-d1-owner"
        || args[1] == "preview-native-button"
        || args[1] == "preview-observable-button"
        || args[1] == "preview-observable-column"
        || args[1] == "preview-observable-buttons"
        || args[1] == "trace-observable-buttons"
        || args[1] == "trace-observable-column"
        || args[1] == "preview-static-column")
        && !experimental
    {
        eprintln!("this operation requires --profile experimental");
        return ExitCode::from(2);
    }
    if (args[1] == "model-d1-state"
        || args[1] == "preview-d1-owner"
        || args[1] == "preview-native-button"
        || args[1] == "preview-observable-button"
        || args[1] == "preview-observable-column"
        || args[1] == "preview-observable-buttons"
        || args[1] == "trace-observable-buttons"
        || args[1] == "trace-observable-column")
        && initial_path.is_none()
    {
        eprintln!("this operation requires --initial <state.json>");
        return ExitCode::from(2);
    }
    if (args[1] == "trace-observable-buttons" || args[1] == "trace-observable-column")
        && activations_path.is_none()
    {
        eprintln!("this operation requires --activations <actions.json>");
        return ExitCode::from(2);
    }
    let mut bytes = Vec::new();
    match File::open(&args[2]).and_then(|file| {
        file.take((neptune::MAX_SOURCE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
    }) {
        Ok(_) if bytes.len() <= neptune::MAX_SOURCE_BYTES => {}
        Ok(_) => {
            return input_error(json, &args[1], "NPT-E1001", "source exceeds 128 KiB");
        }
        Err(error) => {
            return input_error(json, &args[1], "NPT-E1000", &error.to_string());
        }
    }
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => {
            return input_error(json, &args[1], "NPT-E1002", "source must be UTF-8");
        }
    };
    if args[1] == "model-d1-state"
        || args[1] == "preview-d1-owner"
        || args[1] == "preview-native-button"
        || args[1] == "preview-observable-button"
        || args[1] == "preview-observable-column"
        || args[1] == "preview-observable-buttons"
        || args[1] == "trace-observable-buttons"
        || args[1] == "trace-observable-column"
    {
        let result = (|| -> Result<serde_json::Value, neptune::Diagnostic> {
            let path = initial_path.as_ref().ok_or_else(|| neptune::Diagnostic {
                code: "NPT-E3102",
                message: "initial state is required".into(),
                range: None,
            })?;
            let mut data = Vec::new();
            File::open(path)
                .and_then(|file| file.take(2 * 1024 * 1024 + 1).read_to_end(&mut data))
                .map_err(|e| neptune::Diagnostic {
                    code: "NPT-E1000",
                    message: e.to_string(),
                    range: None,
                })?;
            if data.len() > 2 * 1024 * 1024 {
                return Err(neptune::Diagnostic {
                    code: "NPT-E1001",
                    message: "initial state transport exceeds 2 MiB".into(),
                    range: None,
                });
            }
            let value: serde_json::Value =
                serde_json::from_slice(&data).map_err(|e| neptune::Diagnostic {
                    code: "NPT-E3102",
                    message: format!("invalid initial state JSON: {e}"),
                    range: None,
                })?;
            let initial = neptune::application::parse_initial(&value)?;
            if args[1] == "model-d1-state" {
                Ok(
                    neptune::application::lower(&source, module.unwrap_or_default(), initial)?
                        .to_json(),
                )
            } else if args[1] == "preview-native-button" {
                Ok(
                    neptune::native_swift::preview(&source, module.unwrap_or_default(), initial)?
                        .to_json(),
                )
            } else if args[1] == "preview-observable-button" {
                Ok(neptune::native_swift::preview_observable(
                    &source,
                    module.unwrap_or_default(),
                    initial,
                )?
                .to_json())
            } else if args[1] == "preview-observable-column" {
                Ok(neptune::observable_column::preview(
                    &source,
                    module.unwrap_or_default(),
                    initial,
                )?
                .to_json())
            } else if args[1] == "preview-observable-buttons" {
                Ok(neptune::observable_buttons::preview(
                    &source,
                    module.unwrap_or_default(),
                    initial,
                )?
                .to_json())
            } else if args[1] == "trace-observable-buttons" || args[1] == "trace-observable-column"
            {
                let path = activations_path
                    .as_ref()
                    .ok_or_else(|| neptune::Diagnostic {
                        code: "NPT-E2104",
                        message: "activations are required".into(),
                        range: None,
                    })?;
                let mut data = Vec::new();
                File::open(path)
                    .and_then(|file| file.take(64 * 1024 + 1).read_to_end(&mut data))
                    .map_err(|e| neptune::Diagnostic {
                        code: "NPT-E1000",
                        message: e.to_string(),
                        range: None,
                    })?;
                if data.len() > 64 * 1024 {
                    return Err(neptune::Diagnostic {
                        code: "NPT-E1001",
                        message: "activation list exceeds 64 KiB".into(),
                        range: None,
                    });
                }
                let value: serde_json::Value =
                    serde_json::from_slice(&data).map_err(|e| neptune::Diagnostic {
                        code: "NPT-E2104",
                        message: format!("invalid activation JSON: {e}"),
                        range: None,
                    })?;
                let activations = neptune::observable_trace::parse_activations(&value)?;
                let traced = if args[1] == "trace-observable-column" {
                    neptune::observable_trace::trace_column(
                        &source,
                        module.unwrap_or_default(),
                        initial,
                        &activations,
                    )?
                } else {
                    neptune::observable_trace::trace(
                        &source,
                        module.unwrap_or_default(),
                        initial,
                        &activations,
                    )?
                };
                Ok(traced.to_json())
            } else {
                Ok(neptune::application_swift::preview(
                    &source,
                    module.unwrap_or_default(),
                    initial,
                )?
                .to_json())
            }
        })();
        return match result {
            Ok(value) => {
                if json {
                    println!("{value}");
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
                    );
                }
                ExitCode::SUCCESS
            }
            Err(diagnostic) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","operation":args[1].to_string_lossy(),"status":"FAIL","diagnostics":[{"code":diagnostic.code,"message":diagnostic.message,"source_range":diagnostic.range.map(|r| serde_json::json!({"start_byte":r.start,"end_byte":r.end}))}]})
                    );
                } else if let Some(range) = diagnostic.range {
                    eprintln!(
                        "{}: bytes {}..{}: {}",
                        diagnostic.code, range.start, range.end, diagnostic.message
                    );
                } else {
                    eprintln!("{}: {}", diagnostic.code, diagnostic.message);
                }
                ExitCode::FAILURE
            }
        };
    }
    if args[1] == "preview-static-column" {
        return match neptune::static_column::preview(&source, module.unwrap_or_default()) {
            Ok(preview) => {
                let value = preview.to_json();
                if json {
                    println!("{value}");
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
                    );
                }
                ExitCode::SUCCESS
            }
            Err(diagnostic) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","operation":"preview-static-column","status":"FAIL","diagnostics":[{"code":diagnostic.code,"message":diagnostic.message,"source_range":diagnostic.range.map(|r| serde_json::json!({"start_byte":r.start,"end_byte":r.end}))}]})
                    );
                } else if let Some(range) = diagnostic.range {
                    eprintln!(
                        "{}: bytes {}..{}: {}",
                        diagnostic.code, range.start, range.end, diagnostic.message
                    );
                } else {
                    eprintln!("{}: {}", diagnostic.code, diagnostic.message);
                }
                ExitCode::FAILURE
            }
        };
    }
    if args[1] == "preview-transition" {
        return match neptune::transition_swift::preview(&source, module.unwrap_or_default()) {
            Ok(preview) => {
                if json {
                    println!("{}", preview.to_json());
                } else {
                    eprintln!("Experimental transition source preview; Apple and runtime limit validation not run");
                    print!("{}", preview.swift);
                }
                ExitCode::SUCCESS
            }
            Err(diagnostic) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1","operation":"preview-transition","verification":"FAILED","diagnostics":[{"code":diagnostic.code,"message":diagnostic.message,"source_range":diagnostic.range.map(|r| serde_json::json!({"start_byte":r.start,"end_byte":r.end}))}]})
                    );
                } else if let Some(range) = diagnostic.range {
                    eprintln!(
                        "{}: bytes {}..{}: {}",
                        diagnostic.code, range.start, range.end, diagnostic.message
                    );
                } else {
                    eprintln!("{}: {}", diagnostic.code, diagnostic.message);
                }
                ExitCode::FAILURE
            }
        };
    }
    if args[1] == "preview" {
        return match neptune::preview_static_text(&source) {
            Ok(preview) => {
                if let Some(path) = &out {
                    let bundle = match neptune::preview_bundle(&source, module.unwrap_or_default())
                    {
                        Ok(bundle) => bundle,
                        Err(diagnostic) => {
                            return input_error(
                                json,
                                &args[1],
                                diagnostic.code,
                                &diagnostic.message,
                            )
                        }
                    };
                    if let Err(error) = write_preview_bundle(path, &bundle) {
                        let message = format!("could not write source preview: {error}; a partial output directory may remain");
                        if json {
                            println!(
                                "{}",
                                serde_json::json!({"schema_version":"1","operation":"preview","verification":"FAILED","stage":"write","diagnostics":[{"code":"NPT-E1005","message":message}]})
                            );
                        } else {
                            eprintln!("NPT-E1005: {message}");
                        }
                        return ExitCode::FAILURE;
                    }
                    if let Err(error) = neptune::artifacts::verify_preview(path) {
                        if json {
                            println!(
                                "{}",
                                serde_json::json!({"schema_version":"1","operation":"preview","verification":"FAILED","stage":"integrity","message":error.to_string()})
                            );
                        } else {
                            eprintln!("Preview integrity check failed: {error}");
                        }
                        return ExitCode::FAILURE;
                    }
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({"schema_version":"1","operation":"preview","verification":"SOURCE_PREVIEW","output_written":true,"apple_validation":"NOT_RUN"})
                        );
                    } else {
                        println!("Source preview written; Apple validation not run");
                    }
                    return ExitCode::SUCCESS;
                }
                if json {
                    match neptune::preview_bundle(&source, module.unwrap_or_default()) {
                        Ok(bundle) => print!("{bundle}"),
                        Err(diagnostic) => {
                            return input_error(
                                true,
                                &args[1],
                                diagnostic.code,
                                &diagnostic.message,
                            )
                        }
                    }
                } else {
                    if let Err(diagnostic) =
                        neptune::preview_bundle(&source, module.unwrap_or_default())
                    {
                        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
                        return ExitCode::FAILURE;
                    }
                    eprintln!("Source preview only; Apple validation not run");
                    print!("{}", preview.swift);
                }
                ExitCode::SUCCESS
            }
            Err(diagnostic) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1", "operation":"preview", "verification":"FAILED", "diagnostics":[{"code":diagnostic.code,"message":diagnostic.message,"source_range":diagnostic.range.map(|r| serde_json::json!({"start_byte":r.start,"end_byte":r.end}))}]})
                    );
                } else {
                    match diagnostic.range {
                        Some(range) => eprintln!(
                            "{}: bytes {}..{}: {}",
                            diagnostic.code, range.start, range.end, diagnostic.message
                        ),
                        None => eprintln!("{}: {}", diagnostic.code, diagnostic.message),
                    }
                }
                ExitCode::FAILURE
            }
        };
    }
    match neptune::inspect(&source) {
        Ok(diagnostics) => {
            if json {
                let items: Vec<_> = diagnostics.iter().map(|d| serde_json::json!({
                    "code": d.code, "severity": "error", "stage": "ios_validation", "message": d.message,
                    "source_range": d.range.map(|r| serde_json::json!({"start_byte": r.start, "end_byte": r.end}))
                })).collect();
                println!(
                    "{}",
                    serde_json::json!({"schema_version": "1", "operation": args[1].to_string_lossy(), "target": "ios", "stages": {"goya_parse": "PASS", "ios_validation": "FAIL", "apple_validation": "NOT_RUN"}, "diagnostics": items})
                );
                return if args[1] == "inspect" {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                };
            }
            println!("Goya parse: passed; iOS validation: failed; Apple validation: not run");
            for diagnostic in diagnostics {
                match diagnostic.range {
                    Some(range) => eprintln!(
                        "{}: bytes {}..{}: {}",
                        diagnostic.code, range.start, range.end, diagnostic.message
                    ),
                    None => eprintln!("{}: {}", diagnostic.code, diagnostic.message),
                }
            }
            if args[1] == "inspect" {
                return ExitCode::SUCCESS;
            }
        }
        Err(diagnostic) if json => {
            println!(
                "{}",
                serde_json::json!({"schema_version": "1", "operation": args[1].to_string_lossy(), "target": "ios", "stages": {"goya_parse": "FAIL", "ios_validation": "NOT_RUN", "apple_validation": "NOT_RUN"}, "diagnostics": [{"code": diagnostic.code, "severity": "error", "stage": "frontend", "message": diagnostic.message, "source_range": diagnostic.range.map(|r| serde_json::json!({"start_byte": r.start, "end_byte": r.end}))}]})
            );
        }
        Err(diagnostic) => match diagnostic.range {
            Some(range) => eprintln!(
                "{}: bytes {}..{}: {}",
                diagnostic.code, range.start, range.end, diagnostic.message
            ),
            None => eprintln!("{}: {}", diagnostic.code, diagnostic.message),
        },
    }
    ExitCode::FAILURE
}
