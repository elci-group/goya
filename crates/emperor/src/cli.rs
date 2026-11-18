//! Command line for inspect, check, build, verify, test, benchmark, package, and qualify.

use crate::orchestrate::{self, CommandKind, Invocation, Run};
use crate::target;
use crate::Fault;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

pub fn dispatch(args: &[OsString]) -> ExitCode {
    let run = execute(args);
    if run.json.get("output") == Some(&serde_json::json!("json")) {
        println!("{}", run.json);
    } else {
        print!("{}", run.text);
    }
    ExitCode::from(run.code)
}

pub fn execute(args: &[OsString]) -> Run {
    match parse(args) {
        Ok(invocation) => {
            let json = json_requested(args);
            let mut run = orchestrate::execute(invocation);
            if json {
                run.json["output"] = serde_json::json!("json");
            }
            run
        }
        Err(fault) => {
            let mut run = usage_run(fault);
            if json_requested(args) {
                run.json["output"] = serde_json::json!("json");
            }
            run
        }
    }
}

fn json_requested(args: &[OsString]) -> bool {
    args.windows(2)
        .any(|pair| pair[0] == "--format" && pair[1].to_str() == Some("json"))
}

fn parse(args: &[OsString]) -> Result<Invocation, Fault> {
    let Some(command) = args.get(1).and_then(|arg| arg.to_str()) else {
        return Err(Fault::new(2, "EMP-USAGE", "missing command"));
    };
    let kind = match command {
        "inspect" => CommandKind::Inspect,
        "check" => CommandKind::Check,
        "build" => CommandKind::Build,
        "verify" => CommandKind::Verify,
        "test" => CommandKind::Test,
        "benchmark" => CommandKind::Benchmark,
        "package" => CommandKind::Package,
        "qualify" => CommandKind::Qualify,
        _ => {
            return Err(Fault::new(
                2,
                "EMP-USAGE",
                format!("unknown command {command}"),
            ))
        }
    };
    let mut target = target::host().unwrap_or("").to_owned();
    let mut profile = "balanced".to_owned();
    let mut release = false;
    let mut launch = false;
    let mut out = None;
    let mut evidence = None;
    let mut suite = None;
    let mut package_format = None;
    let mut window = None;
    let mut positionals = Vec::new();
    let mut index = 2;
    while index < args.len() {
        let Some(text) = args[index].to_str() else {
            return Err(Fault::new(2, "EMP-USAGE", "arguments must be UTF-8"));
        };
        if text == "--release" {
            release = true;
            index += 1;
        } else if text == "--launch" {
            launch = true;
            index += 1;
        } else if text == "--target" {
            target = required_value(args, &mut index, "--target")?;
        } else if text == "--profile" {
            profile = required_value(args, &mut index, "--profile")?;
        } else if text == "--out" {
            out = Some(PathBuf::from(required_raw(args, &mut index, "--out")?));
        } else if text == "--evidence" {
            evidence = Some(PathBuf::from(required_raw(args, &mut index, "--evidence")?));
        } else if text == "--suite" {
            let value = required_value(args, &mut index, "--suite")?;
            if suite.replace(value).is_some() {
                return Err(Fault::new(2, "EMP-USAGE", "duplicate --suite"));
            }
        } else if text == "--format" {
            // `required_value` moves `index` past the flag and its value.
            // `json` selects CLI output. Any other value is a package format.
            // Both arms must consume the pair; `continue` is not an advance.
            let value = required_value(args, &mut index, "--format")?;
            if value != "json" && package_format.replace(value).is_some() {
                return Err(Fault::new(2, "EMP-USAGE", "duplicate package format"));
            }
        } else if text == "--window" {
            let value = required_value(args, &mut index, "--window")?;
            if value != "wayland" && value != "kumimanu" {
                return Err(Fault::new(
                    2,
                    "EMP-USAGE",
                    format!("unknown window backend {value}"),
                ));
            }
            if window.replace(value).is_some() {
                return Err(Fault::new(2, "EMP-USAGE", "duplicate --window"));
            }
        } else if text.starts_with('-') {
            return Err(Fault::new(2, "EMP-USAGE", format!("unknown option {text}")));
        } else {
            positionals.push(PathBuf::from(&args[index]));
            index += 1;
        }
    }
    if target.is_empty() {
        return Err(Fault::new(
            2,
            "EMP-USAGE",
            "this host has no default Linux triple; pass --target",
        ));
    }
    if positionals.len() != 1 {
        return Err(Fault::new(
            2,
            "EMP-USAGE",
            "expected one scene path or build directory",
        ));
    }
    let positional = positionals.remove(0);
    let directory_command = matches!(
        kind,
        CommandKind::Verify | CommandKind::Benchmark | CommandKind::Package | CommandKind::Qualify
    );
    Ok(Invocation {
        kind,
        scene: if directory_command {
            None
        } else {
            Some(positional.clone())
        },
        directory: if directory_command {
            Some(positional)
        } else {
            None
        },
        out,
        evidence,
        target,
        profile,
        release,
        launch,
        suite,
        package_format,
        window: window.unwrap_or_else(|| "wayland".to_owned()),
    })
}

fn required_value(args: &[OsString], index: &mut usize, flag: &str) -> Result<String, Fault> {
    let raw = required_raw(args, index, flag)?;
    raw.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Fault::new(2, "EMP-USAGE", format!("{flag} must be UTF-8")))
}

fn required_raw<'a>(
    args: &'a [OsString],
    index: &mut usize,
    flag: &str,
) -> Result<&'a OsString, Fault> {
    let Some(value) = args.get(*index + 1) else {
        return Err(Fault::new(2, "EMP-USAGE", format!("{flag} needs a value")));
    };
    *index += 2;
    Ok(value)
}

fn usage_run(fault: Fault) -> Run {
    Run {
        code: fault.exit,
        json: serde_json::json!({
            "schema_version": "1",
            "directive": crate::DIRECTIVE_ID,
            "compiler": "emperor",
            "compiler_version": crate::COMPILER_VERSION,
            "exit_code": fault.exit,
            "diagnostic": fault.code,
            "detail": fault.message,
        }),
        text: format!("{}: {}\n", fault.code, fault.message),
    }
}
