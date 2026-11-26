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
    let mut desktop = None;
    let mut cosmic_config_root = None;
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
        } else if text == "--desktop" {
            let value = required_value(args, &mut index, "--desktop")?;
            let value = crate::desktop::parse(&value)?;
            if desktop.replace(value).is_some() {
                return Err(Fault::new(2, "EMP-USAGE", "duplicate --desktop"));
            }
        } else if text == "--cosmic-config-root" {
            let value = PathBuf::from(required_raw(args, &mut index, "--cosmic-config-root")?);
            if cosmic_config_root.replace(value).is_some() {
                return Err(Fault::new(2, "EMP-USAGE", "duplicate --cosmic-config-root"));
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
    let window = window.unwrap_or_else(|| "wayland".to_owned());
    let desktop_explicit = desktop.is_some();
    let desktop = desktop.unwrap_or(crate::desktop::GENERIC);
    crate::desktop::check_window(desktop, &window)?;
    if cosmic_config_root.is_some() && desktop != crate::desktop::COSMIC {
        return Err(Fault::new(
            2,
            "EMP-USAGE",
            "--cosmic-config-root requires --desktop cosmic",
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
        window,
        desktop: desktop.to_owned(),
        desktop_explicit,
        cosmic_config_root,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        std::iter::once("emperor")
            .chain(list.iter().copied())
            .map(OsString::from)
            .collect()
    }

    fn parsed(list: &[&str]) -> Invocation {
        match parse(&args(list)) {
            Ok(invocation) => invocation,
            Err(fault) => panic!("{list:?}: {fault}"),
        }
    }

    fn fault(list: &[&str]) -> Fault {
        match parse(&args(list)) {
            Ok(_) => panic!("{list:?} parsed"),
            Err(fault) => fault,
        }
    }

    const TARGET: [&str; 2] = ["--target", "x86_64-unknown-linux-gnu"];

    #[test]
    fn desktop_defaults_to_generic() {
        let invocation = parsed(&["build", "scene.gscene", TARGET[0], TARGET[1]]);
        assert_eq!(invocation.desktop, "generic");
        assert!(!invocation.desktop_explicit);
        assert_eq!(invocation.window, "wayland");
        assert!(invocation.cosmic_config_root.is_none());
    }

    #[test]
    fn desktop_accepts_generic_and_cosmic_on_every_command() {
        for command in ["check", "build", "test", "inspect"] {
            let generic = parsed(&[
                command,
                "s.gscene",
                "--desktop",
                "generic",
                TARGET[0],
                TARGET[1],
            ]);
            assert_eq!(generic.desktop, "generic");
            assert!(generic.desktop_explicit);
            let cosmic = parsed(&[
                command,
                "s.gscene",
                "--desktop",
                "cosmic",
                TARGET[0],
                TARGET[1],
            ]);
            assert_eq!(cosmic.desktop, "cosmic");
            assert_eq!(cosmic.window, "wayland");
        }
        for command in ["verify", "qualify", "benchmark", "package"] {
            let cosmic = parsed(&[command, "out", "--desktop", "cosmic", TARGET[0], TARGET[1]]);
            assert_eq!(cosmic.desktop, "cosmic");
            assert!(cosmic.directory.is_some());
        }
        let explicit = parsed(&[
            "build",
            "s.gscene",
            "--desktop",
            "cosmic",
            "--window",
            "wayland",
            "--cosmic-config-root",
            "fixtures/pop",
            TARGET[0],
            TARGET[1],
        ]);
        assert_eq!(
            explicit.cosmic_config_root.as_deref(),
            Some(std::path::Path::new("fixtures/pop"))
        );
    }

    #[test]
    fn desktop_rejects_unknown_duplicate_and_missing_values() {
        let unknown = fault(&[
            "build",
            "s.gscene",
            "--desktop",
            "gnome",
            TARGET[0],
            TARGET[1],
        ]);
        assert_eq!((unknown.exit, unknown.code), (2, "EMP-USAGE"));
        let duplicate = fault(&[
            "build",
            "s.gscene",
            "--desktop",
            "cosmic",
            "--desktop",
            "cosmic",
            TARGET[0],
            TARGET[1],
        ]);
        assert_eq!((duplicate.exit, duplicate.code), (2, "EMP-USAGE"));
        assert!(duplicate.message.contains("duplicate --desktop"));
        let missing = fault(&["build", "s.gscene", TARGET[0], TARGET[1], "--desktop"]);
        assert_eq!((missing.exit, missing.code), (2, "EMP-USAGE"));
    }

    #[test]
    fn cosmic_rejects_the_x11_host_with_usage_exit_2() {
        for order in [
            ["--desktop", "cosmic", "--window", "kumimanu"],
            ["--window", "kumimanu", "--desktop", "cosmic"],
        ] {
            let mut list = vec!["build", "s.gscene", TARGET[0], TARGET[1]];
            list.extend(order);
            let fault = fault(&list);
            assert_eq!((fault.exit, fault.code), (2, "EMP-USAGE"));
            assert!(fault.message.contains("Xwayland"), "{}", fault.message);
        }
        // The generic profile keeps kumimanu.
        let generic = parsed(&[
            "build", "s.gscene", "--window", "kumimanu", TARGET[0], TARGET[1],
        ]);
        assert_eq!(
            (generic.desktop.as_str(), generic.window.as_str()),
            ("generic", "kumimanu")
        );
        // Through the public entry point the rejection is a usage run.
        let run = execute(&args(&[
            "check",
            "s.gscene",
            "--desktop",
            "cosmic",
            "--window",
            "kumimanu",
        ]));
        assert_eq!(run.code, 2);
        assert_eq!(run.json["diagnostic"], "EMP-USAGE");
    }

    #[test]
    fn config_root_requires_cosmic() {
        let fault = fault(&[
            "build",
            "s.gscene",
            "--cosmic-config-root",
            "fixtures/pop",
            TARGET[0],
            TARGET[1],
        ]);
        assert_eq!((fault.exit, fault.code), (2, "EMP-USAGE"));
    }
}
