//! Run a checked Iberia model package against one dynamic FP32 input.
//!
//! Example:
//! `cargo run --locked -p ie-cpu --example iberia_package -- crates/ie-cpu/tests/fixtures/affine.iberia.json SHA256 1 2`

use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use ie_contract::fixture::GoldenFixture;
use ie_contract::tensors::TensorData;
use ie_cpu::local::{Limits, Runtime};
use ie_cpu::package::ModelPackage;

const HELP: &str = "Usage: cargo run --locked -p ie-cpu --example iberia_package -- <package-path> <expected-sha256> <f32> <f32>\n\nLoads the package only if its file SHA-256 matches the supplied 64-digit hex digest, then runs one local CPU request. The checked-in affine fixture with inputs 1 2 produces [14.0, 15.0]. This demo uses synthetic inputs and a local reference CPU; it does not connect to a peer.";

fn main() {
    if let Err(message) = run() {
        eprintln!("error: {message}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let Some(package_arg) = args.next() else {
        return Err(format!("missing package path\n\n{HELP}"));
    };
    if package_arg == "--help" || package_arg == "-h" {
        println!("{HELP}");
        return Ok(());
    }
    let package_path = PathBuf::from(package_arg);
    let expected_sha = parse_sha256(args.next(), "expected SHA-256")?;
    let x = parse_f32(args.next(), "first input")?;
    let y = parse_f32(args.next(), "second input")?;
    if let Some(extra) = args.next() {
        return Err(format!("unexpected extra argument {extra:?}\n\n{HELP}"));
    }

    let file = File::open(&package_path)
        .map_err(|error| format!("cannot open package {}: {error}", package_path.display()))?;
    let package = ModelPackage::load(file, expected_sha)
        .map_err(|error| format!("package rejected: {} ({error:?})", error.name()))?;
    let mut runtime = Runtime::new(Limits::default());
    let model = runtime
        .register_package(package)
        .map_err(|error| format!("cannot register package: {} ({error:?})", error.name()))?;

    let dynamic_input: TensorData = GoldenFixture::tensor(&[x, y], &[1, 2]);
    let request = runtime
        .submit(model, vec![dynamic_input], Duration::from_secs(2))
        .map_err(|error| format!("request rejected: {} ({error:?})", error.name()))?;
    if runtime.run_one() != Some(request) {
        return Err("runtime did not execute the submitted request".into());
    }
    let completion = runtime
        .poll(request)
        .map_err(|error| format!("cannot poll request: {} ({error:?})", error.name()))?
        .ok_or_else(|| "request is still pending after local execution".to_string())?;
    let output = completion
        .result
        .map_err(|error| format!("local execution failed: {} ({error:?})", error.name()))?;
    let (elements, trailing) = output.data.as_chunks::<4>();
    if !trailing.is_empty() {
        return Err("runtime returned a truncated FP32 output".into());
    }
    let values: Vec<_> = elements.iter().copied().map(f32::from_le_bytes).collect();

    println!("Iberia checked-package local CPU demo");
    println!("package: {}", package_path.display());
    println!("input: [{x}, {y}]");
    println!("output: {values:?}");
    println!(
        "queue time: {:.3} ms",
        completion.queue_time.as_secs_f64() * 1_000.0
    );
    println!(
        "execution time: {:.3} ms",
        completion.execution_time.as_secs_f64() * 1_000.0
    );
    Ok(())
}

fn parse_f32(value: Option<std::ffi::OsString>, label: &str) -> Result<f32, String> {
    let value = value.ok_or_else(|| format!("missing {label}\n\n{HELP}"))?;
    let value = value
        .into_string()
        .map_err(|_| format!("{label} must be valid UTF-8 numeric text"))?;
    let parsed = value
        .parse::<f32>()
        .map_err(|error| format!("invalid {label} {value:?}: {error}"))?;
    if !parsed.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    Ok(parsed)
}

fn parse_sha256(value: Option<std::ffi::OsString>, label: &str) -> Result<[u8; 32], String> {
    let value = value.ok_or_else(|| format!("missing {label}\n\n{HELP}"))?;
    let value = value
        .into_string()
        .map_err(|_| format!("{label} must be valid UTF-8 hexadecimal text"))?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "expected SHA-256 must be exactly 64 hexadecimal digits, got {value:?}"
        ));
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| "invalid SHA-256 hexadecimal encoding".to_string())?;
    }
    Ok(digest)
}
