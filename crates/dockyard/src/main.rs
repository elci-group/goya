//! Dockyard command line. Exit 0 admits or builds. Exit 3 means verify found a
//! working executable whose container start is still blocked, or `start`
//! recorded a run whose runtime qualification is still blocked.

use dockyard::{
    admit, build, exchange_netns, start, verify, Failure, CONTENT_TYPE, IMAGE_USER, METHOD,
    PROFILE, STATUS, TARGET,
};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
usage: dockyard inspect SOURCE
       dockyard check SOURCE --target x86_64-unknown-linux-gnu --profile dockyard.linux.http/0.0.1
       dockyard build SOURCE --target x86_64-unknown-linux-gnu --profile dockyard.linux.http/0.0.1 --out NEW_DIR
       dockyard verify NEW_DIR
       dockyard start NEW_DIR";

fn main() -> ExitCode {
    let mut out = io::stdout();
    let mut err = io::stderr();
    ExitCode::from(run(env::args_os().skip(1), &mut out, &mut err) as u8)
}

fn run<I>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> i32
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let args: Vec<std::ffi::OsString> = args.into_iter().collect();
    let Some(command) = args.first().and_then(|text| text.to_str()) else {
        return usage(err);
    };
    if command == "--help" || command == "-h" {
        let _ = writeln!(out, "{USAGE}");
        return 0;
    }
    if command == "--exchange-netns" {
        return exchange_netns_cli(&args[1..], out, err);
    }
    if command == "verify" {
        return verify_dir(&args[1..], out, err);
    }
    if command == "start" {
        return start_dir(&args[1..], out, err);
    }
    if !matches!(command, "inspect" | "check" | "build") || args.len() < 2 {
        return usage(err);
    }
    let source_path = Path::new(&args[1]);
    let mut profile: Option<&str> = None;
    let mut target: Option<&str> = None;
    let mut output: Option<&std::ffi::OsStr> = None;
    let mut release = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].to_str() {
            Some("--profile") if profile.is_none() && index + 1 < args.len() => {
                profile = args[index + 1].to_str();
                if profile.is_none() {
                    return usage(err);
                }
                index += 2;
            }
            Some("--target") if target.is_none() && index + 1 < args.len() => {
                target = args[index + 1].to_str();
                if target.is_none() {
                    return usage(err);
                }
                index += 2;
            }
            Some("--out") if output.is_none() && index + 1 < args.len() => {
                output = Some(&args[index + 1]);
                index += 2;
            }
            Some("--release") if !release => {
                release = true;
                index += 1;
            }
            _ => return usage(err),
        }
    }
    if release {
        let _ = writeln!(err, "DK-E0202: release builds are not qualified");
        return 4;
    }
    if command == "inspect" && (profile.is_some() || target.is_some() || output.is_some()) {
        return usage(err);
    }
    if command == "check" && (profile.is_none() || target.is_none() || output.is_some()) {
        return usage(err);
    }
    if command == "build" && (profile.is_none() || target.is_none() || output.is_none()) {
        return usage(err);
    }
    if let Some(selected) = target {
        if selected != TARGET {
            let _ = writeln!(
                err,
                "DK-E0201: unsupported target. This profile links {TARGET} with crt-static. Dockyard does not download a target."
            );
            return 4;
        }
    }
    if let Some(selected) = profile {
        if selected != PROFILE {
            let _ = writeln!(err, "DK-E0200: unknown profile");
            return 4;
        }
    }
    let source = match fs::read_to_string(source_path) {
        Ok(source) => source,
        Err(error) => {
            let _ = writeln!(err, "DK-E0300: cannot read source: {error}");
            return 3;
        }
    };
    match command {
        "inspect" => match admit(&source) {
            Ok(service) => {
                let _ = writeln!(out, "profile: {PROFILE}");
                let _ = writeln!(out, "service: {}", service.name);
                let _ = writeln!(out, "listen: {}", service.port);
                let _ = writeln!(out, "route: {METHOD} {}", service.path);
                let _ = writeln!(out, "respond: {STATUS} {CONTENT_TYPE}");
                let _ = writeln!(out, "qualification: not a build");
                0
            }
            Err(diagnostic) => reject(err, &diagnostic),
        },
        "check" => match admit(&source) {
            Ok(_) => {
                let _ = writeln!(out, "admitted: {PROFILE}");
                0
            }
            Err(diagnostic) => reject(err, &diagnostic),
        },
        "build" => {
            let destination = Path::new(output.expect("build requires output"));
            match build(&source, destination) {
                Ok(report) => {
                    let _ = writeln!(out, "built: {PROFILE}");
                    let _ = writeln!(out, "executable-sha256: {}", report.executable_sha256);
                    let _ = writeln!(out, "layer-sha256: {}", report.layer_sha256);
                    let _ = writeln!(out, "config-sha256: {}", report.config_sha256);
                    let _ = writeln!(out, "manifest-sha256: {}", report.manifest_sha256);
                    let _ = writeln!(out, "image-start: BLOCKED");
                    let _ = writeln!(out, "runtime-qualification: BLOCKED");
                    0
                }
                Err(failure) => fail(err, &failure),
            }
        }
        _ => usage(err),
    }
}

fn exchange_netns_cli(
    args: &[std::ffi::OsString],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    if args.len() != 3 {
        let _ = writeln!(err, "DK-CLI-E0001: {USAGE}");
        return 2;
    }
    let Some(pid) = args[0].to_str().and_then(|text| text.parse::<u32>().ok()) else {
        let _ = writeln!(err, "DK-CLI-E0001: {USAGE}");
        return 2;
    };
    let Some(port) = args[1].to_str().and_then(|text| text.parse::<u16>().ok()) else {
        let _ = writeln!(err, "DK-CLI-E0001: {USAGE}");
        return 2;
    };
    let Some(path) = args[2].to_str() else {
        let _ = writeln!(err, "DK-CLI-E0001: {USAGE}");
        return 2;
    };
    match exchange_netns(pid, port, path) {
        Ok(response) => {
            let _ = writeln!(out, "status: {}", response.status);
            let _ = writeln!(out, "content-type: {}", response.content_type);
            let _ = writeln!(out, "body-sha256: {}", response.body_sha256);
            0
        }
        Err(detail) => {
            let _ = writeln!(err, "{detail}");
            1
        }
    }
}

fn start_dir(args: &[std::ffi::OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.len() != 1 {
        return usage(err);
    }
    match start(Path::new(&args[0])) {
        Ok(report) => {
            let _ = writeln!(out, "image-start: {}", report.image_start);
            let _ = writeln!(out, "runtime: {}", report.runtime);
            let _ = writeln!(out, "rootfs: {}", report.rootfs);
            let _ = writeln!(out, "capabilities: {}", report.capabilities);
            let _ = writeln!(out, "cpu: {}", report.cpu);
            let _ = writeln!(out, "memory: {}", report.memory);
            let _ = writeln!(out, "network: {}", report.network);
            let _ = writeln!(out, "runtime-user: {}", report.runtime_user);
            let _ = writeln!(out, "user-applied: {}", report.user_applied);
            let _ = writeln!(out, "runtime-qualification: {}", report.qualification);
            if !report.detail.is_empty() {
                let _ = writeln!(out, "detail: {}", report.detail);
            }
            report.exit_code
        }
        Err(failure) => fail(err, &failure),
    }
}

fn verify_dir(args: &[std::ffi::OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.len() != 1 {
        return usage(err);
    }
    match verify(Path::new(&args[0])) {
        Ok(report) if report.qualification_blocked() => {
            let _ = writeln!(out, "layout: pass");
            let _ = writeln!(out, "executable: pass");
            let _ = writeln!(out, "image-user: {IMAGE_USER}");
            let _ = writeln!(out, "image-start: BLOCKED");
            let _ = writeln!(out, "runtime-qualification: BLOCKED");
            3
        }
        Ok(report) => {
            let _ = writeln!(err, "DK-E4002: layout or executable check failed");
            if !report.detail.is_empty() {
                let _ = writeln!(err, "{}", report.detail);
            }
            4
        }
        Err(failure) => fail(err, &failure),
    }
}

fn reject(err: &mut dyn Write, diagnostic: &dockyard::Diagnostic) -> i32 {
    match diagnostic.line {
        Some(line) => {
            let _ = writeln!(
                err,
                "{}: {} [line {line}]",
                diagnostic.code, diagnostic.message
            );
        }
        None => {
            let _ = writeln!(err, "{}: {}", diagnostic.code, diagnostic.message);
        }
    }
    4
}

fn fail(err: &mut dyn Write, failure: &Failure) -> i32 {
    match failure {
        Failure::Admit(diagnostic) => reject(err, diagnostic),
        Failure::Output { code, message } => {
            let _ = writeln!(err, "{code}: {message}");
            if code.starts_with("DK-E1") || code.starts_with("DK-E2") || code.starts_with("DK-E3") {
                5
            } else {
                4
            }
        }
    }
}

fn usage(err: &mut dyn Write) -> i32 {
    let _ = writeln!(err, "DK-CLI-E0001: {USAGE}");
    2
}
