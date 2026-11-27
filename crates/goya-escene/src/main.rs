//! `goya-escene` CLI — inspection, extraction and validation (§13.4).
//!
//! Commands:
//!   extract  <artefact> [--output <file>]   recover the exact source bytes
//!   validate <artefact>                     semantic + integrity validation
//!   inspect  <artefact> [--class C|I|S]     display the emojified projection

use goya_escene::{validate, RenderProfile};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!("usage: goya-escene <extract|validate|inspect> <artefact> [--output F | --class C|I|S]");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("extract") => extract(rest(args)),
        Some("validate") => validate_cmd(rest(args)),
        Some("inspect") => inspect(rest(args)),
        _ => Err("missing command".into()),
    }
}

fn rest(args: &[String]) -> &[String] {
    args.get(1..).unwrap_or(&[])
}

fn load(path: &str) -> Result<(goya_escene::Artefact, goya_escene::RuleRegistry), String> {
    let raw = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    if raw.len() > 64 * 1024 * 1024 {
        return Err("artefact exceeds configured size limit".into());
    }
    let artefact: goya_escene::Artefact =
        serde_json::from_slice(&raw).map_err(|e| format!("invalid artefact: {e}"))?;
    Ok((artefact, goya_escene::RuleRegistry::v1()))
}

fn extract(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("missing artefact path")?;
    let (artefact, rules) = load(path)?;
    if let Err(violations) = validate(&artefact, &rules) {
        return Err(format!("artefact failed validation: {}", violations[0]));
    }
    let bytes = artefact.source.recover().map_err(|e| e.to_string())?;
    let output = args
        .windows(2)
        .find(|w| w[0] == "--output")
        .map(|w| w[1].clone());
    match output {
        Some(dest) => {
            std::fs::write(&dest, &bytes).map_err(|e| format!("cannot write {dest}: {e}"))?;
            println!("wrote {} bytes to {dest}", bytes.len());
        }
        None => {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn validate_cmd(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("missing artefact path")?;
    let (artefact, rules) = load(path)?;
    match validate(&artefact, &rules) {
        Ok(()) => {
            println!(
                "valid: {} target={} status={:?} annotations={} evidence={}",
                path,
                artefact.target.id,
                artefact.header.status,
                artefact.annotations.len(),
                artefact.evidence.len()
            );
            Ok(())
        }
        Err(violations) => {
            for v in &violations {
                eprintln!("{v}");
            }
            Err(format!("{} semantic violations", violations.len()))
        }
    }
}

fn inspect(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("missing artefact path")?;
    let (artefact, _rules) = load(path)?;
    let class_filter = args.windows(2).find(|w| w[0] == "--class").map(|w| w[1].clone());
    let annotations: Vec<_> = artefact
        .annotations
        .iter()
        .filter(|a| class_filter.as_deref().map(|c| format!("{:?}", a.class).to_uppercase() == *c).unwrap_or(true))
        .cloned()
        .collect();
    let shown = if class_filter.is_some() || annotations.len() != artefact.annotations.len() {
        goya_escene::render_projection(&artefact.source, &annotations, RenderProfile::Standard).map_err(|e| e)?
    } else {
        artefact.projection.content.clone()
    };
    println!("{shown}");
    Ok(())
}
