//! Run a checked Iberia model package against one dynamic FP32 input.
//!
//! Example:
//! `cargo run --locked -p ie-cpu --example iberia_package -- crates/ie-cpu/tests/fixtures/affine.iberia.json SHA256 1 2`
//! `cargo run --locked -p ie-cpu --example iberia_package -- --onnx crates/ie-cpu/tests/fixtures/affine.onnx SHA256 1 2`
//! `cargo run --locked -p ie-cpu --example iberia_package -- crates/ie-cpu/tests/fixtures/affine.iberia.json --signed PUBLIC_KEY_HEX SIGNATURE_HEX 1 2`
//! `cargo run --locked -p ie-cpu --example iberia_package -- --onnx crates/ie-cpu/tests/fixtures/affine.onnx --signed PUBLIC_KEY_HEX SIGNATURE_HEX 1 2`

use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use ie_contract::fixture::GoldenFixture;
use ie_contract::tensors::TensorData;
use ie_cpu::local::{Limits, Runtime};
use ie_cpu::package::ModelPackage;
use ie_cpu::signing::ModelFormat;

const HELP: &str = "Usage:\n  cargo run --locked -p ie-cpu --example iberia_package -- <package-path> <expected-sha256> <f32> <f32>\n  cargo run --locked -p ie-cpu --example iberia_package -- --onnx <model.onnx> <expected-sha256> <f32> <f32>\n  cargo run --locked -p ie-cpu --example iberia_package -- <package-path> --signed <public-key-hex> <signature-hex> <f32> <f32>\n  cargo run --locked -p ie-cpu --example iberia_package -- --onnx <model.onnx> --signed <public-key-hex> <signature-hex> <f32> <f32>\n\nThe first two forms load a native Iberia JSON package or, with explicit --onnx, a supported ONNX model using an exact 64-digit SHA-256 pin. The --signed forms verify a detached Ed25519 signature with the exact 32-byte trusted public key (64 hex digits) and 64-byte signature (128 hex digits); no digest-only fallback is attempted. Both forms pack the two supplied FP32 values into one [1,2] input tensor for a local CPU request. The checked-in affine fixtures produce [14.0, 15.0] for values 1 2. This demo uses a local reference CPU and does not connect to a peer.";

fn main() {
    if let Err(message) = run() {
        eprintln!("error: {message}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let Some(first_arg) = args.next() else {
        return Err(format!("missing package path\n\n{HELP}"));
    };
    if first_arg == "--help" || first_arg == "-h" {
        println!("{HELP}");
        return Ok(());
    }
    let onnx = first_arg == "--onnx";
    let package_arg = if onnx {
        args.next()
            .ok_or_else(|| format!("missing ONNX model path after --onnx\n\n{HELP}"))?
    } else {
        first_arg
    };
    let package_path = PathBuf::from(package_arg);
    let format = if onnx {
        ModelFormat::Onnx
    } else {
        ModelFormat::Json
    };
    let first_option = args
        .next()
        .ok_or_else(|| format!("missing package integrity input\n\n{HELP}"))?;
    let (package, x, y) = if first_option == "--signed" {
        let public_key = parse_hex::<32>(args.next(), "Ed25519 public key")?;
        let signature = parse_hex::<64>(args.next(), "Ed25519 signature")?;
        let x = parse_f32(args.next(), "first input")?;
        let y = parse_f32(args.next(), "second input")?;
        let file = File::open(&package_path)
            .map_err(|error| format!("cannot open package {}: {error}", package_path.display()))?;
        let package = ModelPackage::load_signed(file, format, public_key, signature)
            .map_err(|error| format!("signed package rejected: {} ({error:?})", error.name()))?;
        (package, x, y)
    } else {
        let expected_sha = parse_sha256_value(first_option)?;
        let x = parse_f32(args.next(), "first input")?;
        let y = parse_f32(args.next(), "second input")?;
        let file = File::open(&package_path)
            .map_err(|error| format!("cannot open package {}: {error}", package_path.display()))?;
        let package = if onnx {
            ModelPackage::load_onnx(file, expected_sha)
        } else {
            ModelPackage::load(file, expected_sha)
        }
        .map_err(|error| format!("package rejected: {} ({error:?})", error.name()))?;
        (package, x, y)
    };
    if let Some(extra) = args.next() {
        return Err(format!("unexpected extra argument {extra:?}\n\n{HELP}"));
    }
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

fn parse_sha256_value(value: std::ffi::OsString) -> Result<[u8; 32], String> {
    let value = value
        .into_string()
        .map_err(|_| "expected SHA-256 must be valid UTF-8 hexadecimal text".to_string())?;
    parse_hex_text(&value, "expected SHA-256")
}

fn parse_hex<const N: usize>(
    value: Option<std::ffi::OsString>,
    label: &str,
) -> Result<[u8; N], String> {
    let value = value.ok_or_else(|| format!("missing {label}\n\n{HELP}"))?;
    let value = value
        .into_string()
        .map_err(|_| format!("{label} must be valid UTF-8 hexadecimal text"))?;
    parse_hex_text(&value, label)
}

fn parse_hex_text<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{label} must be exactly {} hexadecimal digits, got {value:?}",
            N * 2
        ));
    }
    let mut bytes = [0; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| format!("invalid {label} hexadecimal encoding"))?;
    }
    Ok(bytes)
}
