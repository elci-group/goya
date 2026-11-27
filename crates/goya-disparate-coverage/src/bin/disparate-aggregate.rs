//! CLI: aggregate multiple GY-COVERAGE/1 deficit reports into one
//! programme-level report — the §5 prioritisation engine at multi-corpus
//! scale.
//!
//! Usage: `disparate-aggregate <deficit.json> [<deficit.json>...] [--out <path>]`
//!
//! Loads the given deficit reports (produced by `disparate-assess`), prints
//! the combined table with per-category blocking subtotals, and writes the
//! canonical aggregate JSON to stdout — or to `--out <path>` when given.
//! Exit codes: 0 success, 1 parse/validation failure, 2 usage.

use goya_disparate_coverage::{blocking_side_of, category_of, DeficitReport, COVERAGE_CONTRACT};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: disparate-aggregate <deficit.json> [<deficit.json>...] [--out <path>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut inputs: Vec<String> = Vec::new();
    let mut out: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => match args.get(index + 1).filter(|path| !path.starts_with("--")) {
                Some(path) => {
                    out = Some(path.clone());
                    index += 1;
                }
                None => {
                    eprintln!("--out requires a path");
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            flag if flag.starts_with("--") => {
                eprintln!("unknown flag: {flag}");
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            input => inputs.push(input.to_string()),
        }
        index += 1;
    }
    if inputs.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let mut reports = Vec::with_capacity(inputs.len());
    for input in &inputs {
        let json = match std::fs::read_to_string(input) {
            Ok(json) => json,
            Err(error) => {
                eprintln!("reading {input}: {error}");
                return ExitCode::from(1);
            }
        };
        let report: DeficitReport = match serde_json::from_str(&json) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("parsing {input}: {error}");
                return ExitCode::from(1);
            }
        };
        if report.contract != COVERAGE_CONTRACT {
            eprintln!("{input}: contract {} is not {COVERAGE_CONTRACT}", report.contract);
            return ExitCode::from(1);
        }
        reports.push(report);
    }

    let aggregate = DeficitReport::aggregate(&reports);
    println!("contract: {}", aggregate.contract);
    println!("source_root: {}", aggregate.source_root);
    println!("elements: {}", aggregate.elements);
    for (status, count) in &aggregate.totals {
        println!("total: {status} {count}");
    }
    println!("deficits: {}", aggregate.deficits.len());
    for entry in &aggregate.deficits {
        let blocked: usize = entry.kinds.values().sum();
        println!(
            "deficit: {} blocked={blocked} category={} side={} files={}",
            entry.reason,
            category_of(&entry.reason),
            blocking_side_of(&entry.reason),
            entry.files.len(),
        );
    }
    let (goya_side, programme_side) = aggregate.blocking_split();
    println!("blocking-category: goya-side {goya_side}");
    println!("blocking-category: programme-side {programme_side}");
    for (category, subtotal) in aggregate.category_subtotals() {
        println!("category: {category} {subtotal}");
    }

    let json = aggregate.canonical_json();
    match out {
        Some(path) => {
            if let Err(error) = std::fs::write(Path::new(&path), &json) {
                eprintln!("writing {path}: {error}");
                return ExitCode::from(1);
            }
            println!("written: {path}");
        }
        None => print!("{json}"),
    }
    ExitCode::SUCCESS
}
