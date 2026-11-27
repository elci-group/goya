//! CLI command implementations.

use crate::cli::args::*;
use crate::diagnostics::codes::Severity;
use crate::ingestion::imports::{resolve_entry, FileSystemLoader};

pub fn analyse(args: AnalyseArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut exit_code = 0;
    for path in &args.input {
        let path = path.display().to_string();
        let resolved = resolve_entry(&path, &FileSystemLoader);
        let ir = &resolved.ir;
        if args.format == OutputFormat::Json {
            println!("{}", ir.to_json());
        } else {
            println!(
                "{}: {} sheets, {} resources",
                path,
                ir.sheets.len(),
                ir.resources.len()
            );
            for sheet in &ir.sheets {
                println!("  {}: {} rules", sheet.name, sheet.rules.len());
            }
        }
        if !resolved.reporter.is_empty() {
            match args.format {
                OutputFormat::Json => println!("{}", resolved.reporter.to_json()),
                _ => println!("{}", resolved.reporter.render_text()),
            }
        }
        if args.strict
            && resolved
                .reporter
                .diagnostics()
                .iter()
                .any(|d| d.severity != Severity::Info)
        {
            exit_code = 1;
        }
    }
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

pub fn convert(args: ConvertArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("CASA convert - not yet implemented");
    println!("Input: {:?}", args.input);
    println!("Context: {:?}", args.context);
    println!("Environment: {:?}", args.environment);
    println!("Output: {:?}", args.output);
    println!("Profile: {:?}", args.profile);
    println!("Strict: {}", args.strict);
    println!("Report: {:?}", args.report);
    Ok(())
}

pub fn validate(args: ValidateArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("CASA validate - not yet implemented");
    println!("Artifact: {:?}", args.artifact);
    println!("Level: {:?}", args.level);
    Ok(())
}

pub fn inspect(args: InspectArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("CASA inspect - not yet implemented");
    println!("Input: {:?}", args.input);
    println!("Target: {:?}", args.target);
    Ok(())
}

pub fn compare(args: CompareArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("CASA compare - not yet implemented");
    println!("Reference: {:?}", args.reference);
    println!("Native: {:?}", args.native);
    println!("Mode: {:?}", args.mode);
    Ok(())
}

pub fn benchmark(args: BenchmarkArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("CASA benchmark - not yet implemented");
    println!("Workload: {:?}", args.workload);
    println!("Iterations: {}", args.iterations);
    Ok(())
}

pub fn explain(args: ExplainArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("CASA explain - not yet implemented");
    println!("Input: {:?}", args.input);
    println!("Target: {:?}", args.target);
    Ok(())
}

pub fn capabilities(args: CapabilitiesArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Phase 1 (GOYA-DIS-CASA-001): ingestion and CasaIR are live; every
    // later stage still reports below.
    let stages = [
        (
            "ingestion",
            "implemented: parse, imports, resources, shadow audit",
        ),
        (
            "casa_ir",
            "implemented: model, value categories, dependency graph, serialization",
        ),
        ("diagnostics", "implemented: code registry, JSON reporter"),
        (
            "cascade",
            "implemented: origins, layers, specificity, importance, custom properties, conditions",
        ),
        (
            "context",
            "implemented: scene tree, selector matching, computed styles",
        ),
        (
            "geometry",
            "implemented: proportional conversion, dimensional units, calc()/min()/max()/clamp(), block/flex/grid/table/float/multicol/absolute/fixed/sticky layout",
        ),
        (
            "appearance",
            "implemented: colors, gradients, borders, shadows, transforms, filters, masks, backgrounds, style pooling",
        ),
        ("typography", "planned (Phase 3)"),
        ("animation", "planned (Phase 4)"),
        ("adaptive", "planned (Phase 4)"),
        ("behavioural", "planned (Phase 4)"),
        ("optimisation", "planned (Phase 5)"),
        (
            "emission",
            "implemented: .gscene scene emission, transformation manifest, CSS provenance (compatibility pending, Phase 5)",
        ),
        ("validation", "planned (Phase 7)"),
    ];
    match args.format {
        OutputFormat::Json => {
            let entries: Vec<serde_json::Value> = stages
                .iter()
                .map(|(stage, status)| serde_json::json!({ "stage": stage, "status": status }))
                .collect();
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        _ => {
            for (stage, status) in stages {
                println!("{stage}: {status}");
            }
        }
    }
    Ok(())
}
