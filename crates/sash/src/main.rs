#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    sash::cli::dispatch(&std::env::args_os().collect::<Vec<_>>())
}
