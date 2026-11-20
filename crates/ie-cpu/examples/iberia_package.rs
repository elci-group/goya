//! Run a checked Iberia model package against one dynamic FP32 input.
//!
//! Example:
//! `cargo run --locked -p ie-cpu --example iberia_package -- crates/ie-cpu/tests/fixtures/affine.iberia.json SHA256 1 2`

use std::env;
use std::fs::File;
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
    let mut args = env::args().skip(1);
    let Some(package_path) = args.next() else {
        return Err(format!("missing package path\n\n{HELP}"));
    };
    if package_path == "--help" || package_path == "-h" {
        println!("{HELP}");
        return Ok(());
    }
    let expected_sha = args
        .next()
        .ok_or_else(|| format!("missing expected SHA-256\n\n{HELP}"))
        .and_then(|value| parse_sha256(&value))?;
    let x = parse_f32(args.next(), "first input")?;
    let y = parse_f32(args.next(), "second input")?;
    if let Some(extra) = args.next() {
        return Err(format!("unexpected extra argument {extra:?}\n\n{HELP}"));
    }

    let file = File::open(&package_path)
        .map_err(|error| format!("cannot open package {package_path:?}: {error}"))?;
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
    let values = output
        .data
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four bytes in FP32 output")))
        .collect::<Vec<_>>();

    println!("Iberia checked-package local CPU demo");
    println!("package: {package_path}");
    println!("input: [{x}, {y}]");
    println!("output: {values:?}");
    println!("queue time: {:.3} ms", completion.queue_time.as_secs_f64() * 1_000.0);
    println!("execution time: {:.3} ms", completion.execution_time.as_secs_f64() * 1_000.0);
    if values != [14.0, 15.0] {
        return Err(format!("fixture output was {values:?}, expected [14.0, 15.0] for input [1, 2]"));
    }
    Ok(())
}

fn parse_f32(value: Option<String>, label: &str) -> Result<f32, String> {
    let value = value.ok_or_else(|| format!("missing {label}\n\n{HELP}"))?;
    let parsed = value
        .parse::<f32>()
        .map_err(|error| format!("invalid {label} {value:?}: {error}"))?;
    if !parsed.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    Ok(parsed)
}

fn parse_sha256(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("expected SHA-256 must be exactly 64 hexadecimal digits, got {value:?}"));
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| "invalid SHA-256 hexadecimal encoding".to_string())?;
    }
    Ok(digest)
}
