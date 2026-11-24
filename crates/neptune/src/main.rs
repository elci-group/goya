use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, Read, Write},
    path::Path,
    process::ExitCode,
};

fn write_stdout(args: std::fmt::Arguments<'_>, newline: bool) {
    let mut stdout = io::stdout().lock();
    let result = if newline {
        writeln!(stdout, "{args}")
    } else {
        write!(stdout, "{args}")
    };
    match result {
        Ok(()) => {}
        // Match ordinary CLI pipeline behavior without letting the standard
        // print macros panic when the reader exits early (for example `head`).
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
            std::process::exit(0);
        }
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "failed to write stdout: {error}");
        }
    }
}

macro_rules! println {
    () => { write_stdout(format_args!(""), true) };
    ($($arg:tt)*) => { write_stdout(format_args!($($arg)*), true) };
}

macro_rules! print {
    ($($arg:tt)*) => { write_stdout(format_args!($($arg)*), false) };
}

#[path = "main_source.rs"]
mod source_command;

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
    for name in ["App.swift", "source-map.json", "capabilities.json"] {
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
        if let Err(error) = writeln!(output, "{response}").and_then(|_| output.flush()) {
            return if error.kind() == io::ErrorKind::BrokenPipe {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
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

fn ios_project(args: &[std::ffi::OsString]) -> ExitCode {
    let usage = || {
        eprintln!("usage: neptune ios-project <scene.gscene> --arachnid-bin <path> --bundle-id <id> --name <display-name> --out <new-directory>");
        ExitCode::from(2)
    };
    let Some(source) = args.first() else {
        return usage();
    };
    let mut options = std::collections::BTreeMap::new();
    for pair in args[1..].chunks(2) {
        let [flag, value] = pair else {
            return usage();
        };
        let Some(flag @ ("--arachnid-bin" | "--bundle-id" | "--name" | "--out")) = flag.to_str()
        else {
            return usage();
        };
        if options.insert(flag, value).is_some() {
            return usage();
        }
    }
    if options.len() != 4 {
        return usage();
    }
    let (Some(bundle_id), Some(name)) =
        (options["--bundle-id"].to_str(), options["--name"].to_str())
    else {
        return usage();
    };
    match neptune::ios_project::generate(
        Path::new(source),
        Path::new(options["--arachnid-bin"]),
        bundle_id,
        name,
        Path::new(options["--out"]),
    ) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ios-project: {error}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "ios-project") {
        return ios_project(&args[2..]);
    }
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
        let mut swift = None;
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
            } else if option[0] == "--swift" && swift.is_none() {
                swift = Some(std::path::Path::new(&option[1]));
            } else if option[0] == "--format" && option[1] == "json" && !json {
                json = true;
            } else {
                eprintln!("unknown or duplicate option");
                return ExitCode::from(2);
            }
        }
        return match suite {
            Some("d1-corpus") if worker.is_none() => {
                match swift.map_or_else(
                    neptune::conformance::run_d1_corpus,
                    neptune::conformance::run_d1_corpus_with_swift,
                ) {
                    Ok(report) => {
                        if json {
                            println!("{}", report.json);
                        } else {
                            println!(
                            "D1 scalar corpus: {}; generated Swift: {}; Apple validation: BLOCKED",
                            if report.passed { "PASS" } else { "FAIL" },
                            report.json["stages"]["generated_swift"].as_str().unwrap_or("UNKNOWN"),
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
                }
            }
            Some("admission") if swift.is_none() => {
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
                eprintln!("usage: neptune test --suite d1-corpus [--swift path] [--format json]");
                eprintln!("       neptune test --suite admission [--worker name] [--format json]");
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
    if args.len() >= 2 && args[1] == "sideload" {
        if !(args.len() == 3 || (args.len() == 5 && args[3] == "--format" && args[4] == "json")) {
            eprintln!("usage: neptune sideload <artifact> [--format json]");
            return ExitCode::from(2);
        }
        let json = args.len() == 5;
        if neptune::sideload::find_open_i().is_none() {
            if json {
                println!("{}", neptune::sideload::blocked_report());
            } else {
                eprintln!("open-i unavailable; install open-i or set NEPTUNE_OPEN_I_BIN");
            }
            return ExitCode::from(3);
        }
        return match neptune::sideload::sideload(std::path::Path::new(&args[2])) {
            Ok(report) => {
                if json {
                    println!("{report}");
                } else {
                    println!(
                        "Artifact installed for manual testing; device admission is not claimed"
                    );
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                if json {
                    let mut report = serde_json::json!({
                        "schema_version":"1","operation":"sideload","status":"FAIL",
                        "stages":{"goya_parse":"NOT_RUN","ios_validation":"NOT_RUN","apple_validation":"NOT_RUN","device_install":"FAIL"}
                    });
                    report["message"] = serde_json::Value::String(error.clone());
                    println!("{report}");
                } else {
                    eprintln!("sideload failed: {error}");
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
    if args.len() >= 2 && args[1] == "plan-bluetooth-privacy" {
        let mut usage_description = None;
        let mut contracts = None;
        for option in args[2..].chunks(2) {
            if option.len() != 2 {
                eprintln!("missing option value");
                return ExitCode::from(2);
            }
            if option[0] == "--usage-description" && usage_description.is_none() {
                usage_description = option[1].to_str();
                if usage_description.is_none() {
                    eprintln!("usage description must be UTF-8");
                    return ExitCode::from(2);
                }
            } else if option[0] == "--contracts" && contracts.is_none() {
                contracts = option[1].to_str();
                if contracts.is_none() {
                    eprintln!("contract list must be UTF-8");
                    return ExitCode::from(2);
                }
            } else {
                eprintln!("unknown or duplicate option");
                return ExitCode::from(2);
            }
        }
        let Some(usage_description) = usage_description else {
            eprintln!("usage: neptune plan-bluetooth-privacy --usage-description <string> [--contracts <comma-separated-contracts>]");
            return ExitCode::from(2);
        };
        // Build the same request the service protocol receives and emit the
        // exact response Session::handle produces for it.
        let mut request = serde_json::json!({
            "schema_version": "1",
            "job_id": "cli",
            "operation": "plan-bluetooth-privacy",
            "usage_description": usage_description,
        });
        if let Some(contracts) = contracts {
            let list: Vec<&str> = contracts.split(',').collect();
            request["contracts"] = serde_json::json!(list);
        }
        let mut session = neptune::service::Session::default();
        let response = session.handle(&request);
        println!("{response}");
        return match response["status"].as_str() {
            Some("PASS") => ExitCode::SUCCESS,
            _ => ExitCode::FAILURE,
        };
    }
    source_command::run_source_command(&args)
}
