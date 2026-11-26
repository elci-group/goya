//! CLI: assess a GY-FAIR/1 document against the default capability map.
//!
//! Usage: `disparate-assess <input.fair.json> <output-prefix>`
//!
//! Writes `<prefix>.assessed.fair.json` (assessed FAIR document) and
//! `<prefix>.deficit.json` (GY-COVERAGE/1 deficit report), printing totals.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: disparate-assess <input.fair.json> <output-prefix>");
        return ExitCode::from(2);
    }
    let json = match std::fs::read_to_string(&args[0]) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("reading {}: {error}", args[0]);
            return ExitCode::from(1);
        }
    };
    let document: goya_fair::FairDocument = match serde_json::from_str(&json) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("parsing FAIR: {error}");
            return ExitCode::from(1);
        }
    };
    let map = goya_disparate_coverage::default_map();
    let assessment = match goya_disparate_coverage::assess(&document, &map) {
        Ok(assessment) => assessment,
        Err(error) => {
            eprintln!("assessment failed: {error}");
            return ExitCode::from(1);
        }
    };
    let report = goya_disparate_coverage::DeficitReport::derive(&assessment.document);
    println!("contract: {}", report.contract);
    println!("elements: {}", report.elements);
    for (status, count) in &report.totals {
        println!("total: {status} {count}");
    }
    println!("deficits: {}", report.deficits.len());
    for entry in &report.deficits {
        let blocked: usize = entry.kinds.values().sum();
        println!("deficit: {} {blocked}", entry.reason);
    }
    for (suffix, body) in [
        ("assessed.fair.json", assessment.document.canonical_json()),
        ("deficit.json", report.canonical_json()),
    ] {
        let path = format!("{}.{}", args[1], suffix);
        if let Err(error) = std::fs::write(Path::new(&path), body) {
            eprintln!("writing {path}: {error}");
            return ExitCode::from(1);
        }
        println!("written: {path}");
    }
    ExitCode::SUCCESS
}
