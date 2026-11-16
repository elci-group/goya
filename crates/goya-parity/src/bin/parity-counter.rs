//! Headless owner for one admitted interactive counter.
//!
//! The process reads a scene, runs `run_authored_add`, and prints the result.
//! It does not open a display, socket, or window.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const COMPILED_SCENE: &str = include_str!("../../../sash/fixtures/interactive-counter.gscene");

fn main() -> ExitCode {
    match run() {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("parity-counter: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<String, String> {
    let source = scene_source()?;
    let run = goya_parity::run::run_authored_add(&source).map_err(|error| error.to_string())?;
    Ok(format!(
        "count={} revision={} profile={} copy={} paste={}",
        run.count, run.revision, run.profile, run.copy_allowed, run.paste_allowed
    ))
}

fn scene_source() -> Result<String, String> {
    let mut args = env::args_os().skip(1);
    let Some(arg) = args.next() else {
        return Ok(COMPILED_SCENE.to_owned());
    };
    if args.next().is_some() {
        return Err("expected at most one scene path".into());
    }
    let path = PathBuf::from(arg);
    if !path.exists() {
        return Err(format!("scene path is missing: {}", path.display()));
    }
    fs::read_to_string(&path).map_err(|error| format!("could not read {}: {error}", path.display()))
}
