use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use goya_bench::{measure_ffmpeg_mjpeg, Benchmark, BenchmarkResult};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("goya-watermark-benchmark: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let mut seed = 0x4759_4245_4E43_4831u64;
    let mut images = 12usize;
    let mut size = 64u32;
    let mut key = 0x0A11_CEC0_FFEEu64;
    let mut alpha = 24.0f64;
    let mut output: Option<PathBuf> = None;
    let mut codec_roundtrip = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: goya-watermark-benchmark [--seed N] [--images N] [--size PX] [--key HEX] [--alpha N] [--codec-roundtrip] [--output PATH]\n\
                     Defaults: seed={seed}, images={images}, size={size}, key={key:016x}, alpha={alpha}.\n\
                     Reports deterministic pixel-surrogate measurements; --codec-roundtrip optionally measures installed ffmpeg MJPEG. No general codec robustness is implied."
                );
                return Ok(());
            }
            "--seed" => {
                seed = value(&mut args, "--seed")?
                    .parse()
                    .map_err(|_| "--seed must be an unsigned decimal integer".to_string())?;
            }
            "--images" => {
                images = value(&mut args, "--images")?
                    .parse()
                    .map_err(|_| "--images must be an unsigned integer".to_string())?;
            }
            "--size" => {
                size = value(&mut args, "--size")?
                    .parse()
                    .map_err(|_| "--size must be an unsigned integer".to_string())?;
            }
            "--key" => {
                let raw = value(&mut args, "--key")?;
                key = u64::from_str_radix(raw.trim_start_matches("0x"), 16)
                    .map_err(|_| "--key must be a hexadecimal u64".to_string())?;
            }
            "--alpha" => {
                alpha = value(&mut args, "--alpha")?
                    .parse()
                    .map_err(|_| "--alpha must be a finite number in 1..=64".to_string())?;
            }
            "--output" => output = Some(PathBuf::from(value(&mut args, "--output")?)),
            "--codec-roundtrip" => codec_roundtrip = true,
            unknown => return Err(format!("unknown argument `{unknown}` (use --help)")),
        }
    }
    if !(1..=512).contains(&images) {
        return Err("--images must be between 1 and 512".into());
    }
    if !(64..=1024).contains(&size) {
        return Err("--size must be between 64 and 1024 pixels".into());
    }
    let total_pixels = u64::from(size)
        .checked_mul(u64::from(size))
        .and_then(|pixels| pixels.checked_mul(images as u64))
        .ok_or_else(|| "requested corpus size overflows".to_string())?;
    if total_pixels > 8 * 1024 * 1024 {
        return Err("corpus exceeds the 8-million-pixel allocation bound".into());
    }
    if !alpha.is_finite() || !(1.0..=64.0).contains(&alpha) {
        return Err("--alpha must be finite and between 1 and 64".into());
    }

    let mut result: BenchmarkResult = Benchmark.run_adversarial(seed, images, size, key, alpha);
    if codec_roundtrip {
        measure_ffmpeg_mjpeg(&mut result, "ffmpeg");
    }
    let json = result.to_json();
    if let Some(path) = output {
        fs::write(&path, json)
            .map_err(|error| format!("could not write report to {}: {error}", path.display()))?;
        println!("wrote reproducible report to {}", path.display());
    } else {
        print!("{json}");
    }
    Ok(())
}

fn value<'a>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}
