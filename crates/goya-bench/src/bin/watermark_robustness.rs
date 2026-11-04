use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use goya_bench::{measure_ffmpeg_mjpeg, measure_ffmpeg_png, Benchmark, BenchmarkResult, Corpus};

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
    let mut image_dir: Option<PathBuf> = None;
    let mut images_explicit = false;
    let mut ffmpeg = "ffmpeg".to_string();
    let mut ffprobe = "ffprobe".to_string();
    let mut codec_roundtrip = false;
    let mut codec = "mjpeg".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: goya-watermark-benchmark [--seed N] [--images N | --image-dir DIR] [--size PX] [--key HEX] [--alpha N] [--codec-roundtrip] [--codec mjpeg|png] [--ffmpeg PATH] [--ffprobe PATH] [--output PATH]\n\
                     Defaults: seed={seed}, images={images}, size={size}, key={key:016x}, alpha={alpha}.\n\
                     --image-dir loads sorted local image files, normalizes to square grayscale, and records SHA-256 source hashes.\n\
                     Reports deterministic pixel-surrogate measurements; --codec-roundtrip optionally measures ffmpeg MJPEG. Select --codec png for PNG round-trip (levels 1, 6, 9)."
                );
                return Ok(());
            }
            "--seed" => {
                seed = value(&mut args, "--seed")?
                    .parse()
                    .map_err(|_| "--seed must be an unsigned decimal integer".to_string())?;
            }
            "--images" => {
                images_explicit = true;
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
            "--image-dir" => image_dir = Some(PathBuf::from(value(&mut args, "--image-dir")?)),
            "--ffmpeg" => ffmpeg = value(&mut args, "--ffmpeg")?,
            "--ffprobe" => ffprobe = value(&mut args, "--ffprobe")?,
            "--codec-roundtrip" => codec_roundtrip = true,
            "--codec" => {
                codec = match value(&mut args, "--codec")?.as_str() {
                    "mjpeg" => "mjpeg".to_string(),
                    "png" => "png".to_string(),
                    other => {
                        return Err(format!(
                            "unsupported --codec `{other}` (choose mjpeg or png)"
                        ))
                    }
                };
                codec_roundtrip = true;
            }
            unknown => return Err(format!("unknown argument `{unknown}` (use --help)")),
        }
    }
    if image_dir.is_none() && !(1..=512).contains(&images) {
        return Err("--images must be between 1 and 512".into());
    }
    if image_dir.is_some() && images_explicit {
        return Err(
            "--images cannot be combined with --image-dir; the directory determines image count"
                .into(),
        );
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

    let mut result: BenchmarkResult = if let Some(directory) = image_dir {
        let paths = local_image_paths(&directory)?;
        let corpus = Corpus::from_local_paths(&paths, seed, size, &ffmpeg, &ffprobe)
            .map_err(|error| error.to_string())?;
        Benchmark.run_local(seed, size, key, alpha, corpus)
    } else {
        Benchmark.run_adversarial(seed, images, size, key, alpha)
    };
    if codec_roundtrip {
        if codec == "png" {
            measure_ffmpeg_png(&mut result, &ffmpeg);
        } else {
            measure_ffmpeg_mjpeg(&mut result, &ffmpeg);
        }
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

fn local_image_paths(directory: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "could not read image directory {}: {error}",
            directory.display()
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read directory entry: {error}"))?;
        let path = entry.path();
        let supported = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "avif"
                )
            })
            .unwrap_or(false);
        if supported && path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err(format!(
            "no supported image files found in {}",
            directory.display()
        ));
    }
    if paths.len() > 512 {
        return Err("local image directory contains more than 512 supported images".into());
    }
    Ok(paths)
}

fn value<'a>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}
