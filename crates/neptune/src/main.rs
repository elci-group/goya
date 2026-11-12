use std::{env, fs::File, io::Read, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 3 || (args[1] != "inspect" && args[1] != "check") {
        eprintln!("usage: neptune <inspect|check> <scene.gscene>");
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
