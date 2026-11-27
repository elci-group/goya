//! `goya-disparate-electron extract <dir|app.asar> [--root-label <label>] [--summary]`
//!
//! Writes canonical GY-FAIR/2 JSON to stdout. With `--summary`, also writes
//! per-status counts to stderr. Exit codes: 0 done, 2 usage, 3 i/o,
//! 4 invalid input or budget exceeded, 5 the extractor produced an invalid
//! document.

use goya_disparate_electron::{extract_path, material::Budgets, ExtractError};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str =
    "usage: goya-disparate-electron extract <dir|app.asar> [--root-label <label>] [--summary]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("extract") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let mut path: Option<PathBuf> = None;
    let mut label: Option<String> = None;
    let mut summary = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root-label" => match args.next() {
                Some(v) if label.is_none() => label = Some(v),
                _ => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--summary" => summary = true,
            _ if path.is_none() && !arg.starts_with("--") => path = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(path) = path else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let label = label.unwrap_or_else(|| path.display().to_string());
    match extract_path(&path, &label, &Budgets::default()) {
        Ok(document) => {
            print!("{}", document.canonical_json());
            if summary {
                for (status, count) in document.coverage_summary() {
                    eprintln!("{status:?}: {count}");
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("goya-disparate-electron: {error}");
            ExitCode::from(match error {
                ExtractError::Io(_) => 3,
                ExtractError::Invalid(_) | ExtractError::Budget(_) => 4,
                ExtractError::Validation(_) => 5,
            })
        }
    }
}
