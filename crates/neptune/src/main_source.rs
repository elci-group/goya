//! Source-bearing CLI commands and preview dispatch.
use super::*;

pub(super) fn run_source_command(args: &[std::ffi::OsString]) -> ExitCode {
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
