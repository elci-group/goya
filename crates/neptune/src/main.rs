use std::{env, fs::File, io::Read, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    if args.len() == 3 && args[1] == "explain" {
        let description = match args[2].to_str() {
            Some("NPT-E1000") => "Input could not be read. Check the path and file permissions.",
            Some("NPT-E1001") => "The input exceeds the 128 KiB source limit.",
            Some("NPT-E1002") => "The Goya frontend could not parse the UTF-8 scene. Correct the source at the reported line.",
            Some("NPT-E1003") => "The source exceeds the nesting limit of 128.",
            Some("NPT-E2104") => "The source requires an iOS mapping that Neptune has not admitted. Remove the construct or wait for an admitted implementation.",
            _ => { eprintln!("unknown diagnostic code"); return ExitCode::FAILURE; }
        };
        println!("{description}");
        return ExitCode::SUCCESS;
    }
    if !(args.len() == 3 || (args.len() == 5 && args[3] == "--target" && args[4] == "ios"))
        || (args[1] != "inspect" && args[1] != "check")
    {
        eprintln!("usage: neptune <inspect|check> <scene.gscene> [--target ios]\n       neptune explain <code>");
        return ExitCode::from(2);
    }
    let mut bytes = Vec::new();
    match File::open(&args[2]).and_then(|file| {
        file.take((neptune::MAX_SOURCE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
    }) {
        Ok(_) if bytes.len() <= neptune::MAX_SOURCE_BYTES => {}
        Ok(_) => {
            eprintln!("NPT-E1001: source exceeds 128 KiB");
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("NPT-E1000: {error}");
            return ExitCode::FAILURE;
        }
    }
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => {
            eprintln!("NPT-E1002: source must be UTF-8");
            return ExitCode::FAILURE;
        }
    };
    match neptune::inspect(&source) {
        Ok(diagnostics) => {
            println!("Goya parse: passed; iOS validation: failed; Apple validation: not run");
            for diagnostic in diagnostics {
                match diagnostic.range {
                    Some(range) => eprintln!(
                        "{}: bytes {}..{}: {}",
                        diagnostic.code, range.start, range.end, diagnostic.message
                    ),
                    None => eprintln!("{}: {}", diagnostic.code, diagnostic.message),
                }
            }
            if args[1] == "inspect" {
                return ExitCode::SUCCESS;
            }
        }
        Err(diagnostic) => match diagnostic.range {
            Some(range) => eprintln!(
                "{}: bytes {}..{}: {}",
                diagnostic.code, range.start, range.end, diagnostic.message
            ),
            None => eprintln!("{}: {}", diagnostic.code, diagnostic.message),
        },
    }
    ExitCode::FAILURE
}
