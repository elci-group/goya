//! CLI: extract a FAIR document from an Electron application root.
//!
//! Usage: `disparate-extract <app-root> [<output.fair.json>]`
//!
//! Prints the derived coverage summary to stdout. Writes canonical JSON to
//! the output path, or to stdout when no path is given. Refuses to emit an
//! invalid document.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.len() > 2 {
        eprintln!("usage: disparate-extract <app-root> [<output.fair.json>]");
        return ExitCode::from(2);
    }
    let root = &args[0];
    let output = args.get(1);
    let document = match goya_disparate_electron::extract_app(Path::new(root)) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("extraction failed: {error}");
            return ExitCode::from(1);
        }
    };
    let total = document.elements.len();
    println!("contract: {}", document.contract);
    println!("project: {}", document.project.name);
    for detection in &document.project.frameworks {
        println!("framework: {} ({})", detection.framework, detection.evidence);
    }
    println!("elements: {total}");
    for (status, count) in document.coverage_summary() {
        println!("coverage: {status:?} {count}");
    }
    let canonical = document.canonical_json();
    match output {
        Some(path) => {
            if let Err(error) = std::fs::write(path, &canonical) {
                eprintln!("writing {path}: {error}");
                return ExitCode::from(1);
            }
            println!("written: {path}");
        }
        None => print!("{canonical}"),
    }
    ExitCode::SUCCESS
}
