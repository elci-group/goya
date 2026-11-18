//! Rewrite a scene's absolute lengths as proportional ones (GSSH/3).
//!
//! ```text
//! goya-proportional <input.gscene> <width-pt> <height-pt> [output.gscene]
//! ```
//!
//! The fractions reproduce the reference layout at the given viewport. The
//! converted scene goes to the output path (which must not exist) or to
//! stdout. Exit status: 0 exact, 3 written but some lengths could not be
//! reproduced (listed on stderr), 1 parse or I/O error, 2 usage.
use std::{fs, io::Write, process};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        eprintln!("usage: goya-proportional <input.gscene> <width-pt> <height-pt> [output.gscene]");
        process::exit(2);
    }
    let dimension = |text: &str| -> f64 {
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() && value > 0.0 => value,
            _ => {
                eprintln!("viewport sizes must be positive numbers of points");
                process::exit(2);
            }
        }
    };
    let (width, height) = (dimension(&args[1]), dimension(&args[2]));
    let source = fs::read_to_string(&args[0]).unwrap_or_else(|error| {
        eprintln!("could not read {}: {error}", args[0]);
        process::exit(1);
    });
    let conversion = goya_lang::proportional::to_proportional(&source, width, height)
        .unwrap_or_else(|error| {
            eprintln!("{}: {error}", args[0]);
            process::exit(1);
        });
    let written = match args.get(3) {
        Some(path) => fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut file| file.write_all(conversion.source.as_bytes())),
        None => std::io::stdout().write_all(conversion.source.as_bytes()),
    };
    if let Err(error) = written {
        eprintln!("could not write output: {error}");
        process::exit(1);
    }
    for item in &conversion.inexact {
        eprintln!(
            "{}:{}..{}: {} {}",
            args[0], item.range.start, item.range.end, item.property, item.reason
        );
    }
    if !conversion.inexact.is_empty() {
        process::exit(3);
    }
}
