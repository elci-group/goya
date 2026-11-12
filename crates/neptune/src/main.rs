use std::{env, fs::File, io::Read, process::ExitCode};

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
    for option in args[3..].chunks(2) {
        if option.len() != 2 {
            eprintln!("missing option value");
            return ExitCode::from(2);
        }
        if option[0] == "--format" && option[1] == "json" && !json {
            json = true;
        } else if option[0] == "--target" && option[1] == "ios" && !target {
            target = true;
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
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"schema_version":"1", "operation":"preview", "verification":"SOURCE_PREVIEW", "swift":preview.swift, "source_map":[{"source":{"start_byte":preview.source_range.start,"end_byte":preview.source_range.end},"swift":{"start_byte":preview.swift_range.start,"end_byte":preview.swift_range.end}}]})
                    );
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
