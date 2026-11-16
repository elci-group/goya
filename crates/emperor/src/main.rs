//! Emperor command line.

#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    emperor::cli::dispatch(&args)
}
