use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "usage: nino <inspect|analyse> <file.gba> [--format json]\n       nino extract-tile <file.gba> --offset N --palette-offset N --depth <4|8> [--transparent-zero]\n       nino execute <file.gba> --steps <1..4096>";

struct ExtractionOptions {
    offset: usize,
    palette_offset: usize,
    depth: nino::graphics::TileDepth,
    transparency: nino::graphics::Transparency,
}

fn number(value: &str) -> Result<usize, (u8, String)> {
    let result = if let Some(hex) = value.strip_prefix("0x") {
        usize::from_str_radix(hex, 16)
    } else {
        value.parse()
    };
    result.map_err(|_| (2, format!("invalid nonnegative offset '{value}'")))
}

fn extraction_options(args: &[String]) -> Result<ExtractionOptions, (u8, String)> {
    let (mut offset, mut palette_offset, mut depth) = (None, None, None);
    let mut transparency = nino::graphics::Transparency::Opaque;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if flag == "--transparent-zero" && transparency == nino::graphics::Transparency::Opaque {
            transparency = nino::graphics::Transparency::IndexZero;
            continue;
        }
        match flag.as_str() {
            "--offset" if offset.is_none() => {
                offset = Some(number(
                    args.next().ok_or((2, "--offset requires a value".into()))?,
                )?)
            }
            "--palette-offset" if palette_offset.is_none() => {
                palette_offset = Some(number(
                    args.next()
                        .ok_or((2, "--palette-offset requires a value".into()))?,
                )?)
            }
            "--depth" if depth.is_none() => {
                depth = Some(match args.next().map(String::as_str) {
                    Some("4") => nino::graphics::TileDepth::FourBpp,
                    Some("8") => nino::graphics::TileDepth::EightBpp,
                    _ => return Err((2, "--depth requires 4 or 8".into())),
                })
            }
            _ => {
                return Err((
                    2,
                    format!("unknown or duplicate extraction option '{flag}'"),
                ))
            }
        }
    }
    Ok(ExtractionOptions {
        offset: offset.ok_or((2, "--offset is required".into()))?,
        palette_offset: palette_offset.ok_or((2, "--palette-offset is required".into()))?,
        depth: depth.ok_or((2, "--depth is required".into()))?,
        transparency,
    })
}

fn run() -> Result<(), (u8, String)> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or((2, USAGE.into()))?;
    if !matches!(
        command.as_str(),
        "inspect" | "analyse" | "extract-tile" | "execute"
    ) {
        return Err((2, format!("unsupported command '{command}'; {USAGE}")));
    }
    let path = args
        .next()
        .ok_or((2, "command requires a ROM path".into()))?;
    let rest: Vec<_> = args.collect();
    let extraction = if command == "extract-tile" {
        Some(extraction_options(&rest)?)
    } else {
        None
    };
    let execution_steps = if command == "execute" {
        if rest.len() != 2 || rest[0] != "--steps" {
            return Err((2, "execute requires --steps <1..4096>".into()));
        }
        let steps = number(&rest[1])?;
        if steps == 0 || steps > nino::execution::MAX_STEPS {
            return Err((2, "execute step limit must be between 1 and 4096".into()));
        }
        Some(steps)
    } else {
        None
    };
    if extraction.is_none()
        && execution_steps.is_none()
        && !rest.is_empty()
        && rest != ["--format", "json"]
    {
        return Err((
            2,
            "usage: nino <inspect|analyse> <file.gba> [--format json]".into(),
        ));
    }
    let file = std::fs::File::open(&path).map_err(|e| (3, format!("cannot open '{path}': {e}")))?;
    let mut bytes = Vec::new();
    file.take(nino::MAX_ROM_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| (3, format!("cannot read '{path}': {e}")))?;
    if let Some(steps) = execution_steps {
        let report = nino::execution::run(&bytes, Default::default(), steps)
            .map_err(|e| (4, e.to_string()))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| (5, e.to_string()))?
        );
        // A JSON stop report is produced even when execution is incomplete.
        return if report.stop == nino::execution::Stop::InstructionLimit {
            Ok(())
        } else {
            Err((6, "execution stopped at an unsupported capability or invalid fetch; see JSON stop report".into()))
        };
    }
    if let Some(options) = extraction {
        let asset = nino::extraction::extract_tile(
            &bytes,
            options.offset,
            options.palette_offset,
            options.depth,
            options.transparency,
        )
        .map_err(|e| (4, e.to_string()))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&asset).map_err(|e| (5, e.to_string()))?
        );
        return Ok(());
    }
    if command == "analyse" {
        let analysis = nino::analysis::analyse(&bytes).map_err(|e| (4, e.to_string()))?;
        if rest.is_empty() {
            println!("SHA-256: {}", analysis.source_sha256);
            println!("Discovered instructions: {}", analysis.instructions.len());
            println!("Unresolved paths: {}", analysis.unresolved.len());
            println!("Static discovery is incomplete; maturity N0");
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&analysis).map_err(|e| (5, e.to_string()))?
            );
        }
        return Ok(());
    }
    let report = nino::inspect(&bytes).map_err(|e| (4, e.to_string()))?;
    if rest.is_empty() {
        println!("GBA ROM: {} bytes", report.rom_bytes);
        println!("SHA-256: {}", report.source_sha256);
        println!(
            "Title: {}",
            report.header.title.as_deref().unwrap_or("unknown")
        );
        println!(
            "Entry: {}",
            report
                .probable_entry_point
                .map(|v| format!("0x{v:08x}"))
                .unwrap_or_else(|| "unknown".into())
        );
        println!("Save markers: {}", report.save_signatures.len());
        for warning in &report.warnings {
            println!("{}: {}", warning.code, warning.detail);
        }
        println!("Maturity: N0; translation unavailable");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| (5, e.to_string()))?
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("nino: {message}");
            ExitCode::from(code)
        }
    }
}
