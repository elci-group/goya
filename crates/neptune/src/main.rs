use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, Read, Write},
    process::ExitCode,
};

fn write_preview_bundle(path: &std::path::Path, bundle: &str) -> io::Result<()> {
    let value: serde_json::Value = serde_json::from_str(bundle).map_err(io::Error::other)?;
    let artifacts = value["artifacts"].as_object().ok_or_else(|| io::Error::other("invalid preview artifacts"))?;
    let report = value.get("report").ok_or_else(|| io::Error::other("missing preview report"))?;
    // Prepare all bytes before creating the output directory. create_dir and
    // create_new reject existing paths rather than replacing user files.
    let mut files = Vec::new();
    for name in ["App.swift", "source-map.json", "capabilities.json"] {
        let content = artifacts.get(name).and_then(serde_json::Value::as_str).ok_or_else(|| io::Error::other("missing preview artifact"))?;
        files.push((name, content.to_owned()));
    }
    files.push(("build-report.json", format!("{report}\n")));
    fs::create_dir(path)?;
    for (name, content) in files {
        OpenOptions::new().write(true).create_new(true).open(path.join(name))?.write_all(content.as_bytes())?;
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
    if args.len() == 3 && args[1] == "explain" {
        let description = match args[2].to_str() {
            Some("NPT-E1000") => "Input could not be read. Check the path and file permissions.",
            Some("NPT-E1001") => "The input exceeds the 128 KiB source limit.",
            Some("NPT-E1002") => "The Goya frontend could not parse the UTF-8 scene. Correct the source at the reported line.",
            Some("NPT-E1003") => "The source exceeds the nesting limit of 128.",
            Some("NPT-E1004") => "The diagnostic limit was reached; fix reported issues and check again.",
            Some("NPT-E2104") => "The source requires an iOS mapping that Neptune has not admitted. Remove the construct or wait for an admitted implementation.",
            Some("GY-E1000") => "Goya could not parse the source. Correct the syntax at the reported line.",
            Some("GY-E1001") => "Goya does not preserve this property. Remove it or use a supported property.",
            Some("GY-E1002") => "Goya does not preserve template semantics in strict parsing. Remove the template.",
            Some("GY-E1003") => "A property is declared more than once on the same scene node. Keep one declaration.",
            Some("GY-E1004") => "Goya could not validate the interaction contract. Correct the reported interaction error.",
            _ => { eprintln!("unknown diagnostic code"); return ExitCode::FAILURE; }
        };
        println!("{description}");
        return ExitCode::SUCCESS;
    }
    if args.len() < 3 || (args[1] != "inspect" && args[1] != "check" && args[1] != "preview") {
        eprintln!("usage: neptune <inspect|check|preview> <scene.gscene> [--target ios] [--format json]\n       neptune explain <code>");
        return ExitCode::from(2);
    }
    let mut json = false;
    let mut target = false;
    let mut out = None;
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
        } else {
            eprintln!("unknown or duplicate option");
            return ExitCode::from(2);
        }
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
    if args[1] == "preview" {
        return match neptune::preview_static_text(&source) {
            Ok(preview) => {
                if let Some(path) = &out {
                    let bundle = match neptune::preview_bundle(&source) {
                        Ok(bundle) => bundle,
                        Err(diagnostic) => return input_error(json, &args[1], diagnostic.code, &diagnostic.message),
                    };
                    if let Err(error) = write_preview_bundle(path, &bundle) {
                        return input_error(json, &args[1], "NPT-E1005", &format!("could not write source preview: {error}; a partial output directory may remain"));
                    }
                    if json { println!("{}", serde_json::json!({"schema_version":"1","operation":"preview","verification":"SOURCE_PREVIEW","output_written":true,"apple_validation":"NOT_RUN"})); }
                    else { println!("Source preview written; Apple validation not run"); }
                    return ExitCode::SUCCESS;
                }
                if json {
                    match neptune::preview_bundle(&source) {
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
                    eprintln!("{}: {}", diagnostic.code, diagnostic.message);
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
