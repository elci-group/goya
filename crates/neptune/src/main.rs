use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 3 || (args[1] != "inspect" && args[1] != "check") {
        eprintln!("usage: neptune <inspect|check> <scene.gscene>");
        return ExitCode::from(2);
    }
    let bytes = match fs::read(&args[2]) {
        Ok(bytes) if bytes.len() <= neptune::MAX_SOURCE_BYTES => bytes,
        Ok(_) => { eprintln!("NPT-E1001: source exceeds 128 KiB"); return ExitCode::FAILURE; }
        Err(error) => { eprintln!("NPT-E1000: {error}"); return ExitCode::FAILURE; }
    };
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => { eprintln!("NPT-E1002: source must be UTF-8"); return ExitCode::FAILURE; }
    };
    match neptune::inspect(&source) {
        Ok(diagnostics) => {
            println!("Goya parse: passed; iOS validation: failed; Apple validation: not run");
            for diagnostic in diagnostics {
                match diagnostic.range {
                    Some(range) => eprintln!("{}: bytes {}..{}: {}", diagnostic.code, range.start, range.end, diagnostic.message),
                    None => eprintln!("{}: {}", diagnostic.code, diagnostic.message),
                }
            }
        }
        Err(diagnostic) => eprintln!("{}: {}", diagnostic.code, diagnostic.message),
    }
    ExitCode::FAILURE
}
