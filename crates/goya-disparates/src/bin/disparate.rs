//! CLI: run the full Disparates pipeline over one application root.
//!
//! Usage: `disparate run <app-root> <output-prefix> [map.json]`
//!
//! Runs extraction (GY-DISP-001), assessment (GY-COVERAGE/1) and deficit
//! reporting in one deterministic pass, writing `<prefix>.fair.json`
//! (pre-assessment), `<prefix>.assessed.fair.json` and `<prefix>.deficit.json`
//! (all canonical JSON), and printing a summary. With `map.json`, a custom
//! capability map is loaded via `goya_disparate_coverage::load_map`; without
//! it, the default v1 map applies. Exit codes: 2 usage, 1 failure.

use goya_disparates::Pipeline;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 || args.len() > 4 || args[0] != "run" {
        eprintln!("usage: disparate run <app-root> <output-prefix> [map.json]");
        return ExitCode::from(2);
    }
    let app_root = Path::new(&args[1]);
    let output_prefix = &args[2];

    let pipeline = if let Some(map_path) = args.get(3) {
        match goya_disparate_coverage::load_map(Path::new(map_path)) {
            Ok(map) => Pipeline::with_map(map),
            Err(error) => {
                eprintln!("capability map: {error}");
                return ExitCode::from(1);
            }
        }
    } else {
        Pipeline::default_map()
    };

    let output = match pipeline.run(app_root) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };

    println!("project: {}", output.fair.project.name);
    println!("elements: {}", output.fair.elements.len());
    for (status, count) in &output.deficit.totals {
        println!("total: {status} {count}");
    }
    println!("upgraded: {} untouched: {}", output.upgraded, output.untouched);
    println!("deficits: {}", output.deficit.deficits.len());
    for entry in &output.deficit.deficits {
        let blocked: usize = entry.kinds.values().sum();
        println!("deficit: {} {blocked}", entry.reason);
    }
    for (suffix, body) in [
        ("fair.json", output.fair.canonical_json()),
        ("assessed.fair.json", output.assessed.canonical_json()),
        ("deficit.json", output.deficit.canonical_json()),
    ] {
        let path = format!("{output_prefix}.{suffix}");
        if let Err(error) = std::fs::write(Path::new(&path), body) {
            eprintln!("writing {path}: {error}");
            return ExitCode::from(1);
        }
        println!("written: {path}");
    }
    ExitCode::SUCCESS
}
