//! Process entry for one admitted counter.
//!
//! `--oracle` is the default. `--smoke`, `--exercise`, and `--live` open the
//! window backend selected when this binary was linked.
//! Exit 0 prints the report. Exit 1 is usage or a bad native IR. Exit 5 is a
//! window-backend or runtime failure.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

include!(concat!(env!("OUT_DIR"), "/embed.rs"));

enum Mode {
    Oracle,
    ExerciseOracle,
    Smoke,
    Exercise,
    Live,
}

struct Args {
    mode: Mode,
    frames: Option<u32>,
    nir: Option<PathBuf>,
}

enum AppError {
    Usage(String),
    Runtime(String),
}

fn main() -> ExitCode {
    match app() {
        Ok(report) => {
            println!(
                "count={} revision={} presented={} wayland={} window={} copy={} paste={} edit={}",
                report.count,
                report.revision,
                report.presented,
                report.wayland,
                report.window,
                report.copy_allowed,
                report.paste_allowed,
                report.edit
            );
            if let (Some(version), Some(state)) =
                (report.xdg_wm_base_version, report.toplevel_state)
            {
                println!("xdg_wm_base_bound={version} toplevel_state={state:?}");
            }
            ExitCode::SUCCESS
        }
        Err(AppError::Usage(message)) => {
            eprintln!("emperor-app: {message}");
            ExitCode::from(1)
        }
        Err(AppError::Runtime(message)) => {
            eprintln!("emperor-app: {message}");
            ExitCode::from(5)
        }
    }
}

fn app() -> Result<emperor_runtime::OracleReport, AppError> {
    let args = parse_args().map_err(AppError::Usage)?;
    let json = if let Some(path) = &args.nir {
        read_nir(path)?
    } else if let Some(scene) = embedded() {
        emperor_runtime::assert_scene_matches(scene).map_err(AppError::Usage)?;
        scene.nir_json.to_owned()
    } else {
        return Err(AppError::Usage(
            "no native ir: pass --nir PATH or embed a CompiledScene".into(),
        ));
    };
    let mode = match args.mode {
        Mode::Oracle => emperor_runtime::RunMode::Oracle,
        Mode::ExerciseOracle => emperor_runtime::RunMode::ExerciseOracle,
        Mode::Smoke => emperor_runtime::RunMode::Smoke,
        Mode::Exercise => emperor_runtime::RunMode::Exercise,
        Mode::Live => emperor_runtime::RunMode::Live {
            max_frames: args.frames,
        },
    };
    emperor_runtime::run_with_style(&json, mode, embedded_style()).map_err(classify)
}

fn parse_args() -> Result<Args, String> {
    let mut mode = None;
    let mut frames = None;
    let mut nir = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--oracle" => set_mode(&mut mode, Mode::Oracle)?,
            "--exercise-oracle" => set_mode(&mut mode, Mode::ExerciseOracle)?,
            "--smoke" => set_mode(&mut mode, Mode::Smoke)?,
            "--exercise" => set_mode(&mut mode, Mode::Exercise)?,
            "--live" => set_mode(&mut mode, Mode::Live)?,
            "--frames" => {
                let value = args.next().ok_or("missing value for --frames")?;
                let parsed = value
                    .parse::<u32>()
                    .map_err(|_| format!("bad --frames {value}"))?;
                if frames.replace(parsed).is_some() {
                    return Err("duplicate --frames".into());
                }
            }
            "--nir" => {
                let value = args.next().ok_or("missing value for --nir")?;
                if nir.replace(PathBuf::from(value)).is_some() {
                    return Err("duplicate --nir".into());
                }
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let mode = mode.unwrap_or(Mode::Oracle);
    if frames.is_some() && !matches!(mode, Mode::Live) {
        return Err("--frames requires --live".into());
    }
    Ok(Args { mode, frames, nir })
}

fn set_mode(slot: &mut Option<Mode>, mode: Mode) -> Result<(), String> {
    if slot.is_some() {
        return Err(
            "pass only one of --oracle, --exercise-oracle, --smoke, --exercise, or --live".into(),
        );
    }
    *slot = Some(mode);
    Ok(())
}

fn read_nir(path: &Path) -> Result<String, AppError> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::Usage("nir path must not contain ..".into()));
    }
    let file = fs::File::open(path)
        .map_err(|err| AppError::Usage(format!("could not read {}: {err}", path.display())))?;
    let mut json = String::new();
    file.take((emperor_runtime::MAX_NIR_JSON_BYTES + 1) as u64)
        .read_to_string(&mut json)
        .map_err(|err| AppError::Usage(format!("could not read {}: {err}", path.display())))?;
    if json.len() > emperor_runtime::MAX_NIR_JSON_BYTES {
        return Err(AppError::Usage(format!(
            "native ir exceeds the {} byte limit",
            emperor_runtime::MAX_NIR_JSON_BYTES
        )));
    }
    Ok(json)
}

fn classify(err: String) -> AppError {
    if err.starts_with("wayland:") || err.starts_with("kumimanu:") {
        AppError::Runtime(err)
    } else {
        AppError::Usage(err)
    }
}
