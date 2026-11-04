//! goya-bench — reproducible watermark robustness benchmark (GY-BENCH/1).
//!
//! Directive §9 assurance gate: "No robustness threshold without a
//! reproducible evaluation corpus." This crate provides exactly that:
//! a deterministic synthetic image corpus (gradients, checkerboards, noise
//! fields), a family of compression-*simulating* perturbations (no codec
//! dependencies), and a benchmark harness that measures per-transformation
//! exact-payload detection rates and false positives on unmarked images.
//!
//! The [`RobustnessReport`] prints what was actually measured — including
//! transformations the watermark survives poorly (e.g. crop) — so reported
//! robustness thresholds always trace back to a reproducible run.

use goya_watermark::{detect, embed};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_LOCAL_IMAGES: usize = 512;
const MAX_LOCAL_EDGE: u32 = 8192;
const MAX_LOCAL_SOURCE_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_LOCAL_TOTAL_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_LOCAL_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LOCAL_TOTAL_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PROBE_OUTPUT_BYTES: usize = 256;
const MAX_VERSION_OUTPUT_BYTES: usize = 64 * 1024;

/// xorshift64 PRNG for deterministic corpus and noise generation.
struct Xorshift64(u64);

impl Xorshift64 {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_byte(&mut self) -> u8 {
        (self.next_u64() >> 24) as u8
    }
}

/// splitmix64 finalizer: decorrelates seeds and payload ids.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One synthetic corpus image with its distinct payload id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusImage {
    /// Human-readable name (e.g. `"gradient_00"`).
    pub name: String,
    /// 8-bit luma pixels, row-major, `w` x `h`.
    pub pixels: Vec<u8>,
    /// Image width in pixels.
    pub w: u32,
    /// Image height in pixels.
    pub h: u32,
    /// Distinct 64-bit payload id embedded into this image.
    pub payload: u64,
}

/// A deterministic synthetic evaluation corpus.
#[derive(Debug, Clone)]
pub struct Corpus;

impl Corpus {
    /// Decode local images in lexicographic path order through ffprobe/ffmpeg.
    /// Inputs are dimension-checked before pixel decoding; every image is
    /// normalized to `size`×`size` grayscale by aspect-preserving fit with a
    /// neutral-gray pad. Source bytes are streamed through SHA-256 for report
    /// provenance, and the decoded corpus is bounded by count/pixel limits.
    pub fn from_local_paths(
        paths: &[PathBuf],
        seed: u64,
        size: u32,
        ffmpeg: &str,
        ffprobe: &str,
    ) -> Result<LocalCorpus, LocalCorpusError> {
        if paths.is_empty() {
            return Err(LocalCorpusError::EmptyCorpus);
        }
        if paths.len() > MAX_LOCAL_IMAGES {
            return Err(LocalCorpusError::TooManyImages(paths.len()));
        }
        if !(64..=1024).contains(&size) {
            return Err(LocalCorpusError::InvalidTargetSize(size));
        }
        let output_pixels = u64::from(size)
            .checked_mul(u64::from(size))
            .and_then(|pixels| pixels.checked_mul(paths.len() as u64))
            .ok_or(LocalCorpusError::PixelLimit)?;
        if output_pixels > MAX_LOCAL_TOTAL_PIXELS {
            return Err(LocalCorpusError::PixelLimit);
        }
        let ordered = sort_local_paths(paths);
        if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(LocalCorpusError::DuplicatePath);
        }
        let ffmpeg_version = tool_version(ffmpeg)?;

        let mut images = Vec::with_capacity(ordered.len());
        let mut sources = Vec::with_capacity(ordered.len());
        let mut source_pixels_total = 0u64;
        let mut file_bytes_total = 0u64;
        let mut payloads = std::collections::BTreeSet::new();
        for (index, path) in ordered.iter().enumerate() {
            let (file_hash, file_bytes) = hash_local_file(path)?;
            file_bytes_total = file_bytes_total
                .checked_add(file_bytes)
                .filter(|total| *total <= MAX_LOCAL_TOTAL_FILE_BYTES)
                .ok_or(LocalCorpusError::TotalFileBytesLimit)?;
            let (source_width, source_height) = probe_dimensions(ffprobe, path)?;
            source_pixels_total =
                add_source_dimensions(source_pixels_total, path, source_width, source_height)?;

            let pixels = decode_local_image(ffmpeg, path, size)?;
            let (hash_after_decode, _) = hash_local_file(path)?;
            if hash_after_decode != file_hash {
                return Err(LocalCorpusError::SourceChanged(path.clone()));
            }
            let mut payload = mix64(seed ^ mix64(index as u64) ^ digest_u64(&file_hash)) | 1;
            while !payloads.insert(payload) {
                payload = payload.wrapping_add(2);
            }
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
            images.push(CorpusImage {
                name,
                pixels,
                w: size,
                h: size,
                payload,
            });
            sources.push(SourceImageMetadata {
                path: path.to_string_lossy().into_owned(),
                sha256: hex_digest(&file_hash),
                source_width,
                source_height,
                normalized_width: size,
                normalized_height: size,
            });
        }
        Ok(LocalCorpus {
            images,
            sources,
            ffmpeg_version,
        })
    }

    /// Generate `n_images` deterministic synthetic images of `size` x `size`
    /// pixels, cycling through gradient, checkerboard, and noise-field
    /// patterns. Each image carries a distinct payload id derived from
    /// `seed` and its index. Identical arguments always yield identical
    /// corpora.
    pub fn generate(seed: u64, n_images: usize, size: u32) -> Vec<CorpusImage> {
        // A zero-sized image has no meaningful luma pattern. Avoid the
        // modulo-by-zero path in gradient generation and keep corpus entries
        // valid for the watermark API.
        let Some(pixels_per_image) = usize::try_from(size)
            .ok()
            .and_then(|width| width.checked_mul(width))
        else {
            return Vec::new();
        };
        if size == 0
            || size > 1024
            || n_images > 512
            || pixels_per_image
                .checked_mul(n_images)
                .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Vec::new();
        }
        let mut images = Vec::with_capacity(n_images);
        for i in 0..n_images {
            let payload = mix64(seed ^ mix64((i as u64).wrapping_add(1))) | 1;
            let rng_seed = mix64(seed ^ mix64((i as u64).wrapping_add(0x9E37)));
            let mut rng = Xorshift64(rng_seed | 1);
            let (name, pixels) = match i % 6 {
                0 => (format!("gradient_{:02}", i), Self::gradient(size)),
                1 => (format!("checkerboard_{:02}", i), Self::checkerboard(size)),
                2 => (format!("noise_{:02}", i), Self::noise_field(size, &mut rng)),
                3 => (format!("stripes_{:02}", i), Self::stripes(size)),
                4 => (format!("rings_{:02}", i), Self::rings(size)),
                _ => (format!("low_contrast_{:02}", i), Self::low_contrast(size)),
            };
            images.push(CorpusImage {
                name,
                pixels,
                w: size,
                h: size,
                payload,
            });
        }
        images
    }

    /// Generate the fixed six-pattern adversarial corpus with a documented
    /// seed and distinct watermark payload per image. The patterns exercise
    /// smooth, blocky, textured, edge-rich, radial, and low-contrast images.
    pub fn adversarial(seed: u64, n_images: usize, size: u32) -> Vec<CorpusImage> {
        Self::generate(seed, n_images, size)
    }

    /// Smooth diagonal luminance ramp.
    fn gradient(size: u32) -> Vec<u8> {
        let n = size as usize;
        (0..n * n)
            .map(|i| {
                let x = i % n;
                let y = i / n;
                let v = (x + y) * 255 / ((2 * n - 1).max(1));
                v as u8
            })
            .collect::<Vec<_>>()
    }

    /// 8-pixel checkerboard.
    fn checkerboard(size: u32) -> Vec<u8> {
        let n = size as usize;
        (0..n * n)
            .map(|i| {
                let x = i % n;
                let y = i / n;
                if (x / 8 + y / 8).is_multiple_of(2) {
                    230
                } else {
                    25
                }
            })
            .collect()
    }

    /// One-pixel alternating bars expose sensitivity to resampling and
    /// high-frequency loss.
    fn stripes(size: u32) -> Vec<u8> {
        let n = size as usize;
        (0..n * n)
            .map(|i| {
                if (i % n / 2).is_multiple_of(2) {
                    220
                } else {
                    36
                }
            })
            .collect()
    }

    /// Integer radial rings expose block alignment and crop sensitivity.
    fn rings(size: u32) -> Vec<u8> {
        let n = size as usize;
        let center = n as i64 / 2;
        (0..n * n)
            .map(|i| {
                let x = i % n;
                let y = i / n;
                let dx = x as i64 - center;
                let dy = y as i64 - center;
                if ((dx * dx + dy * dy).isqrt() / 4) % 2 == 0 {
                    168
                } else {
                    88
                }
            })
            .collect()
    }

    /// Low-contrast checker pattern for near-quantisation-limit inputs.
    fn low_contrast(size: u32) -> Vec<u8> {
        let n = size as usize;
        (0..n * n)
            .map(|i| {
                if ((i % n / 4) + (i / n / 4)).is_multiple_of(2) {
                    136
                } else {
                    120
                }
            })
            .collect()
    }

    /// Deterministic pseudo-random field.
    fn noise_field(size: u32, rng: &mut Xorshift64) -> Vec<u8> {
        let n = size as usize;
        (0..n * n).map(|_| rng.next_byte()).collect()
    }
}

fn sort_local_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut ordered = paths.to_vec();
    ordered.sort();
    ordered
}

fn add_source_dimensions(
    current_total: u64,
    path: &Path,
    width: u32,
    height: u32,
) -> Result<u64, LocalCorpusError> {
    if width == 0 || height == 0 || width > MAX_LOCAL_EDGE || height > MAX_LOCAL_EDGE {
        return Err(LocalCorpusError::SourceDimensionLimit {
            path: path.to_path_buf(),
            width,
            height,
        });
    }
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| current_total.checked_add(pixels))
        .filter(|total| *total <= MAX_LOCAL_SOURCE_PIXELS)
        .ok_or(LocalCorpusError::SourcePixelLimit)
}

/// A successfully decoded caller-owned image corpus and source provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCorpus {
    pub images: Vec<CorpusImage>,
    pub sources: Vec<SourceImageMetadata>,
    pub ffmpeg_version: String,
}

/// Per-image source identity and deterministic decode geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceImageMetadata {
    pub path: String,
    pub sha256: String,
    pub source_width: u32,
    pub source_height: u32,
    pub normalized_width: u32,
    pub normalized_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCorpusError {
    EmptyCorpus,
    TooManyImages(usize),
    InvalidTargetSize(u32),
    PixelLimit,
    SourcePixelLimit,
    TotalFileBytesLimit,
    DuplicatePath,
    ToolVersionUnavailable(String),
    FileTooLarge(PathBuf),
    ReadSource(PathBuf),
    SourceChanged(PathBuf),
    ProbeFailed(PathBuf),
    MalformedProbe(PathBuf),
    SourceDimensionLimit {
        path: PathBuf,
        width: u32,
        height: u32,
    },
    DecodeFailed(PathBuf),
    DecodedSizeMismatch(PathBuf),
}

impl std::fmt::Display for LocalCorpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCorpus => write!(f, "local image corpus is empty"),
            Self::TooManyImages(n) => write!(f, "local image count {n} exceeds {MAX_LOCAL_IMAGES}"),
            Self::InvalidTargetSize(n) => write!(f, "target size {n} is outside 64..=1024"),
            Self::PixelLimit => write!(f, "normalized corpus exceeds the 8-million-pixel limit"),
            Self::SourcePixelLimit => write!(f, "source images exceed the 8-million-pixel limit"),
            Self::TotalFileBytesLimit => write!(f, "source files exceed the 256-MiB total limit"),
            Self::DuplicatePath => write!(f, "local corpus contains duplicate paths"),
            Self::ToolVersionUnavailable(tool) => {
                write!(f, "could not identify decoder version for `{tool}`")
            }
            Self::FileTooLarge(path) => write!(f, "source file exceeds 64 MiB: {}", path.display()),
            Self::ReadSource(path) => write!(f, "could not read source image: {}", path.display()),
            Self::SourceChanged(path) => {
                write!(f, "source image changed while being decoded: {}", path.display())
            }
            Self::ProbeFailed(path) => write!(f, "ffprobe failed for {}", path.display()),
            Self::MalformedProbe(path) => write!(
                f,
                "ffprobe returned invalid dimensions for {}",
                path.display()
            ),
            Self::SourceDimensionLimit {
                path,
                width,
                height,
            } => write!(
                f,
                "source dimensions {width}x{height} exceed the 8192-pixel edge limit: {}",
                path.display()
            ),
            Self::DecodeFailed(path) => write!(f, "ffmpeg could not decode {}", path.display()),
            Self::DecodedSizeMismatch(path) => write!(
                f,
                "ffmpeg returned an unexpected normalized frame size for {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LocalCorpusError {}

fn hash_local_file(path: &Path) -> Result<([u8; 32], u64), LocalCorpusError> {
    let metadata =
        fs::metadata(path).map_err(|_| LocalCorpusError::ReadSource(path.to_path_buf()))?;
    if !metadata.is_file() || metadata.len() > MAX_LOCAL_FILE_BYTES {
        return Err(LocalCorpusError::FileTooLarge(path.to_path_buf()));
    }
    let mut file =
        File::open(path).map_err(|_| LocalCorpusError::ReadSource(path.to_path_buf()))?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| LocalCorpusError::ReadSource(path.to_path_buf()))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_LOCAL_FILE_BYTES {
            return Err(LocalCorpusError::FileTooLarge(path.to_path_buf()));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((hasher.finalize().into(), total))
}

fn probe_dimensions(ffprobe: &str, path: &Path) -> Result<(u32, u32), LocalCorpusError> {
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
        ])
        .arg("-i")
        .arg(path);
    let output = command_output_bounded(&mut command, MAX_PROBE_OUTPUT_BYTES)
        .map_err(|_| LocalCorpusError::ProbeFailed(path.to_path_buf()))?;
    let text = std::str::from_utf8(&output)
        .map_err(|_| LocalCorpusError::MalformedProbe(path.to_path_buf()))?
        .trim();
    let (width, height) = text
        .split_once('x')
        .ok_or_else(|| LocalCorpusError::MalformedProbe(path.to_path_buf()))?;
    let width = width
        .parse()
        .map_err(|_| LocalCorpusError::MalformedProbe(path.to_path_buf()))?;
    let height = height
        .parse()
        .map_err(|_| LocalCorpusError::MalformedProbe(path.to_path_buf()))?;
    Ok((width, height))
}

fn decode_local_image(ffmpeg: &str, path: &Path, size: u32) -> Result<Vec<u8>, LocalCorpusError> {
    let filter = format!(
        "scale={size}:{size}:force_original_aspect_ratio=decrease:flags=area,pad={size}:{size}:(ow-iw)/2:(oh-ih)/2:color=gray,format=gray"
    );
    let mut command = Command::new(ffmpeg);
    command.args(["-v", "error", "-i"]).arg(path).args([
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-vf",
        &filter,
        "-f",
        "rawvideo",
        "-pix_fmt",
        "gray",
        "pipe:1",
    ]);
    let max_output = usize::try_from(size)
        .ok()
        .and_then(|width| width.checked_mul(width))
        .ok_or(LocalCorpusError::PixelLimit)?;
    let output = command_output_bounded(&mut command, max_output)
        .map_err(|_| LocalCorpusError::DecodeFailed(path.to_path_buf()))?;
    if output.len() != max_output {
        return Err(LocalCorpusError::DecodedSizeMismatch(path.to_path_buf()));
    }
    Ok(output)
}

fn command_output_bounded(command: &mut Command, max_output: usize) -> std::io::Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("child stdout pipe was not created"))?;
    let reader = std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut output = Vec::with_capacity(max_output.min(1024 * 1024));
            let mut buffer = [0u8; 8192];
            let mut exceeded = false;
            loop {
                let read = stdout.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                let remaining = max_output.saturating_sub(output.len());
                let keep = remaining.min(read);
                output.extend_from_slice(&buffer[..keep]);
                exceeded |= keep != read;
            }
            Ok::<_, std::io::Error>((output, exceeded))
        });
        let status = child.wait()?;
        Ok::<_, std::io::Error>((
            status.success(),
            reader
                .join()
                .map_err(|_| std::io::Error::other("output reader panicked"))??,
        ))
    })?;
    let (success, (output, exceeded)) = reader;
    if !success || exceeded {
        return Err(std::io::Error::other(
            "child failed or exceeded output limit",
        ));
    }
    Ok(output)
}

fn tool_version(executable: &str) -> Result<String, LocalCorpusError> {
    let mut command = Command::new(executable);
    command.arg("-version");
    let output = command_output_bounded(&mut command, MAX_VERSION_OUTPUT_BYTES)
        .map_err(|_| LocalCorpusError::ToolVersionUnavailable(executable.into()))?;
    let version = std::str::from_utf8(&output)
        .ok()
        .and_then(|text| text.lines().next())
        .filter(|line| !line.trim().is_empty())
        .ok_or_else(|| LocalCorpusError::ToolVersionUnavailable(executable.into()))?;
    Ok(version.to_string())
}

fn digest_u64(digest: &[u8; 32]) -> u64 {
    u64::from_le_bytes(digest[..8].try_into().expect("fixed digest prefix"))
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A compression-simulating pixel transformation (no codec dependencies).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Compression {
    /// Lossy-quantisation proxy: coarse per-block quantisation over 8x8
    /// blocks, mimicking DCT coefficient quantisation at the given quality
    /// (0-100). Higher quality = finer step = less distortion.
    JPEGish {
        /// Quality in 0..=100.
        quality: u8,
    },
    /// Nearest-neighbour downsample by `factor`, then upsample back.
    Resize {
        /// Integer decimation factor (>= 1).
        factor: u32,
    },
    /// Keep only the centered region covering `fraction` of each axis.
    Crop {
        /// Kept fraction of each axis, in (0.0, 1.0].
        fraction: f64,
    },
    /// Deterministic PRNG pixel perturbation within `+-amplitude`.
    Noise {
        /// Maximum absolute per-pixel delta.
        amplitude: i32,
    },
    /// Integer contrast scaling around midpoint 128. A 4/5 setting lowers
    /// contrast; 6/5 increases it. This is a deterministic image-edit proxy.
    Contrast { numerator: u16, denominator: u16 },
    /// Integer brightness offset clamped to the luma range.
    Brightness { offset: i16 },
    /// Deterministic square box blur; radius is capped at 4 for bounded work.
    BoxBlur { radius: u8 },
    /// Salt-and-pepper impulse noise at `per_mille` of pixels (0..=1000).
    SaltPepper { per_mille: u16 },
}

impl Compression {
    /// Stable label used in report output.
    pub fn label(&self) -> String {
        match self {
            Compression::JPEGish { quality } => format!("jpegish(q{})", quality),
            Compression::Resize { factor } => format!("resize(x{})", factor),
            Compression::Crop { fraction } => format!("crop({:.2})", fraction),
            Compression::Noise { amplitude } => format!("noise(+-{})", amplitude),
            Compression::Contrast {
                numerator,
                denominator,
            } => {
                format!("contrast({}/{})", numerator, (*denominator).max(1))
            }
            Compression::Brightness { offset } => format!("brightness({:+})", offset),
            Compression::BoxBlur { radius } => format!("box-blur(r{})", (*radius).min(4)),
            Compression::SaltPepper { per_mille } => {
                format!("salt-pepper({})", (*per_mille).min(1000))
            }
        }
    }

    /// Apply the transformation, returning the transformed pixel buffer and
    /// its dimensions.
    fn apply(&self, pixels: &[u8], w: u32, h: u32, image_index: u64) -> (Vec<u8>, u32, u32) {
        match *self {
            Compression::JPEGish { quality } => (jpegish(pixels, w, h, quality), w, h),
            Compression::Resize { factor } => resize(pixels, w, h, factor.max(1)),
            Compression::Crop { fraction } => crop(pixels, w, h, fraction),
            Compression::Noise { amplitude } => (add_noise(pixels, image_index, amplitude), w, h),
            Compression::Contrast {
                numerator,
                denominator,
            } => (contrast(pixels, numerator, denominator), w, h),
            Compression::Brightness { offset } => (brightness(pixels, offset), w, h),
            Compression::BoxBlur { radius } => (box_blur(pixels, w, h, radius), w, h),
            Compression::SaltPepper { per_mille } => {
                (salt_pepper(pixels, image_index, per_mille), w, h)
            }
        }
    }
}

fn contrast(pixels: &[u8], numerator: u16, denominator: u16) -> Vec<u8> {
    let denominator = i64::from(denominator.max(1));
    pixels
        .iter()
        .map(|&pixel| {
            let centered = i64::from(pixel) - 128;
            ((centered * i64::from(numerator) / denominator) + 128).clamp(0, 255) as u8
        })
        .collect()
}

fn brightness(pixels: &[u8], offset: i16) -> Vec<u8> {
    pixels
        .iter()
        .map(|&pixel| (i32::from(pixel) + i32::from(offset)).clamp(0, 255) as u8)
        .collect()
}

fn box_blur(pixels: &[u8], w: u32, h: u32, radius: u8) -> Vec<u8> {
    let width = w as usize;
    let height = h as usize;
    let radius = usize::from(radius.min(4));
    if radius == 0 {
        return pixels.to_vec();
    }
    let mut out = Vec::with_capacity(pixels.len());
    for y in 0..height {
        for x in 0..width {
            let x0 = x.saturating_sub(radius);
            let y0 = y.saturating_sub(radius);
            let x1 = (x + radius + 1).min(width);
            let y1 = (y + radius + 1).min(height);
            let mut sum = 0u64;
            let mut count = 0u64;
            for yy in y0..y1 {
                for xx in x0..x1 {
                    sum += u64::from(pixels[yy * width + xx]);
                    count += 1;
                }
            }
            out.push((sum / count.max(1)) as u8);
        }
    }
    out
}

fn salt_pepper(pixels: &[u8], image_index: u64, per_mille: u16) -> Vec<u8> {
    let mut rng = Xorshift64(mix64(image_index.wrapping_add(0xA4D3_7C91)) | 1);
    let threshold = u64::from(per_mille.min(1000)) * 1000;
    pixels
        .iter()
        .map(|&pixel| {
            if rng.next_u64() % 1_000_000 < threshold {
                if rng.next_u64() & 1 == 0 {
                    0
                } else {
                    255
                }
            } else {
                pixel
            }
        })
        .collect()
}

/// Quantise each pixel to a coarse grid anchored on its 8x8 block mean.
///
/// Block-local averaging mimics how DCT quantisation preserves block
/// structure while destroying fine detail: detail within the quantisation
/// step collapses toward the block mean.
fn jpegish(pixels: &[u8], w: u32, h: u32, quality: u8) -> Vec<u8> {
    let step = u32::from(100u8.saturating_sub(quality)) / 8 + 1;
    let step = step as f64;
    let w = w as usize;
    let h = h as usize;
    let mut out = pixels.to_vec();
    for by in 0..(h / 8) {
        for bx in 0..(w / 8) {
            let mut sum = 0u64;
            for y in 0..8 {
                for x in 0..8 {
                    sum += u64::from(pixels[(by * 8 + y) * w + (bx * 8 + x)]);
                }
            }
            let mean = sum as f64 / 64.0;
            for y in 0..8 {
                for x in 0..8 {
                    let idx = (by * 8 + y) * w + (bx * 8 + x);
                    let v = f64::from(pixels[idx]);
                    let q = mean + ((v - mean) / step).round() * step;
                    out[idx] = q.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// Nearest-neighbour decimation by `factor`, then nearest-neighbour
/// upsampling back to the original size.
fn resize(pixels: &[u8], w: u32, h: u32, factor: u32) -> (Vec<u8>, u32, u32) {
    let w = w as usize;
    let h = h as usize;
    let f = factor as usize;
    let sw = (w / f).max(1);
    let sh = (h / f).max(1);
    let mut small = Vec::with_capacity(sw * sh);
    for y in 0..sh {
        for x in 0..sw {
            small.push(pixels[y * f * w + x * f]);
        }
    }
    let mut up = Vec::with_capacity(w * h);
    for y in 0..h {
        let sy = (y / f).min(sh - 1);
        for x in 0..w {
            let sx = (x / f).min(sw - 1);
            up.push(small[sy * sw + sx]);
        }
    }
    (up, w as u32, h as u32)
}

/// Keep the centered region covering `fraction` of each axis.
fn crop(pixels: &[u8], w: u32, h: u32, fraction: f64) -> (Vec<u8>, u32, u32) {
    let f = fraction.clamp(0.01, 1.0);
    let w = w as usize;
    let h = h as usize;
    let cw = ((w as f64 * f).round() as usize).clamp(1, w);
    let ch = ((h as f64 * f).round() as usize).clamp(1, h);
    let x0 = (w - cw) / 2;
    let y0 = (h - ch) / 2;
    let mut out = Vec::with_capacity(cw * ch);
    for y in y0..(y0 + ch) {
        for x in x0..(x0 + cw) {
            out.push(pixels[y * w + x]);
        }
    }
    (out, cw as u32, ch as u32)
}

/// Deterministic PRNG perturbation within `+-amplitude`, keyed by image index
/// so each image gets a distinct but reproducible noise pattern.
fn add_noise(pixels: &[u8], image_index: u64, amplitude: i32) -> Vec<u8> {
    let mut rng = Xorshift64(mix64(image_index.wrapping_add(0x51_5E_2B)) | 1);
    // Larger noise cannot produce a different clamped u8 result, and
    // clamping before arithmetic prevents overflow for adversarial inputs.
    let amplitude = amplitude.clamp(0, 255);
    pixels
        .iter()
        .map(|&p| {
            let spread = (2 * amplitude + 1).max(1) as u64;
            let delta = (rng.next_u64() % spread) as i32 - amplitude;
            (i32::from(p) + delta).clamp(0, 255) as u8
        })
        .collect()
}

/// Per-transformation measurement within a report.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformResult {
    /// Human-readable transformation label.
    pub label: String,
    /// Fraction of corpus images whose payload was recovered exactly.
    pub detection_rate: f64,
    /// Mean over images of the maximum absolute pixel delta between the
    /// watermarked image and its transformed version.
    pub mean_max_delta: f64,
    /// Valid source images measured for this transform.
    pub samples: usize,
    /// Exact-payload detections among `samples`.
    pub hits: usize,
}

/// The reproducible robustness evaluation demanded by directive §9.
#[derive(Debug, Clone, PartialEq)]
pub struct RobustnessReport {
    /// Number of corpus images evaluated.
    pub n_images: usize,
    /// Well-shaped non-empty images actually evaluated.
    pub n_valid_images: usize,
    /// Malformed or empty public corpus entries skipped by the harness.
    pub n_invalid_images: usize,
    /// One measurement per requested transformation, in order.
    pub transforms: Vec<TransformResult>,
    /// Fraction of UNMARKED corpus images on which detection reported a
    /// payload (false positives; target is 0).
    pub false_positive_rate: f64,
    /// Number of (unmarked image, key) trials.
    pub false_positive_controls: usize,
    /// Number of control trials that yielded a payload.
    pub false_positives: usize,
}

impl std::fmt::Display for RobustnessReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "robustness report: {} corpus images ({} valid, {} invalid)",
            self.n_images, self.n_valid_images, self.n_invalid_images
        )?;
        for t in &self.transforms {
            writeln!(
                f,
                "  {:<20} detection_rate={:.3} hits={}/{} mean_max_delta={:.1}",
                t.label, t.detection_rate, t.hits, t.samples, t.mean_max_delta
            )?;
        }
        write!(
            f,
            "  {:<20} false_positive_rate={:.6} false_positives={}/{}",
            "unmarked",
            self.false_positive_rate,
            self.false_positives,
            self.false_positive_controls
        )
    }
}

/// Benchmark harness: embed, perturb, detect, report.
#[derive(Debug, Clone, Copy, Default)]
pub struct Benchmark;

impl Benchmark {
    /// Run the benchmark: embed each corpus image's payload with `key` and
    /// embedding strength `alpha`, apply every transformation in
    /// `transformations`, and attempt detection. Also probes unmarked corpus
    /// images for false positives.
    ///
    /// Deterministic: identical inputs produce an identical report.
    pub fn run(
        &self,
        corpus: &[CorpusImage],
        key: u64,
        alpha: f64,
        transformations: &[Compression],
    ) -> RobustnessReport {
        self.run_with_controls(corpus, key, alpha, transformations, &[key], 0)
    }

    /// Run with an explicit collection of keys for unmarked-image false
    /// positive controls and a stable seed for stochastic surrogate attacks.
    pub fn run_with_controls(
        &self,
        corpus: &[CorpusImage],
        key: u64,
        alpha: f64,
        transformations: &[Compression],
        control_keys: &[u64],
        seed: u64,
    ) -> RobustnessReport {
        // False positives: detection on unmarked images must report nothing.
        let is_valid = |img: &CorpusImage| {
            usize::try_from(img.w)
                .ok()
                .and_then(|w| usize::try_from(img.h).ok().and_then(|h| w.checked_mul(h)))
                == Some(img.pixels.len())
                && img.w > 0
                && img.h > 0
        };
        let valid_images = corpus.iter().filter(|img| is_valid(img)).count();
        let invalid_images = corpus.len().saturating_sub(valid_images);
        let false_positive_controls = valid_images.saturating_mul(control_keys.len());
        let false_positives = corpus
            .iter()
            .filter(|img| is_valid(img))
            .flat_map(|img| {
                control_keys
                    .iter()
                    .map(move |control_key| (img, control_key))
            })
            .filter(|(img, control_key)| detect(&img.pixels, img.w, img.h, **control_key).is_some())
            .count();
        let false_positive_rate = rate(false_positives, false_positive_controls);

        let mut transforms = Vec::with_capacity(transformations.len());
        for (ti, t) in transformations.iter().enumerate() {
            let mut hits = 0usize;
            let mut delta_sum = 0.0f64;
            for (ii, img) in corpus.iter().enumerate() {
                if !is_valid(img) {
                    // Malformed public CorpusImage values are treated as
                    // misses; a benchmark report must not panic on bad shape.
                    continue;
                }
                let marked = embed(&img.pixels, img.w, img.h, key, img.payload, alpha);
                let attack_seed = seed
                    .wrapping_add(ii as u64)
                    .wrapping_add((ti as u64).wrapping_mul(7));
                let (transformed, tw, th) = t.apply(&marked, img.w, img.h, attack_seed);
                if detect(&transformed, tw, th, key) == Some(img.payload) {
                    hits += 1;
                }
                delta_sum += f64::from(max_delta(&marked, img.w, img.h, &transformed, tw, th));
            }
            transforms.push(TransformResult {
                label: t.label(),
                detection_rate: rate(hits, valid_images),
                mean_max_delta: if valid_images == 0 {
                    0.0
                } else {
                    delta_sum / valid_images as f64
                },
                samples: valid_images,
                hits,
            });
        }

        RobustnessReport {
            n_images: corpus.len(),
            n_valid_images: valid_images,
            n_invalid_images: invalid_images,
            transforms,
            false_positive_rate,
            false_positive_controls,
            false_positives,
        }
    }

    /// Run the standard adversarial synthetic corpus and the complete bounded
    /// transform suite. The return value includes enough metadata to recreate
    /// the exact corpus and attacks, and its JSON is stable across runs.
    pub fn run_adversarial(
        &self,
        seed: u64,
        n_images: usize,
        size: u32,
        key: u64,
        alpha: f64,
    ) -> BenchmarkResult {
        let corpus = Corpus::adversarial(seed, n_images, size);
        self.run_corpus(
            seed,
            size,
            key,
            alpha,
            corpus,
            Vec::new(),
            None,
            "synthetic_adversarial",
        )
    }

    /// Run the full transform suite and false-positive controls on a
    /// pre-decoded caller-owned corpus.
    pub fn run_local(
        &self,
        seed: u64,
        size: u32,
        key: u64,
        alpha: f64,
        corpus: LocalCorpus,
    ) -> BenchmarkResult {
        let decode_tool_version = Some(corpus.ffmpeg_version.clone());
        self.run_corpus(
            seed,
            size,
            key,
            alpha,
            corpus.images,
            corpus.sources,
            decode_tool_version,
            "local_files",
        )
    }

    fn run_corpus(
        &self,
        seed: u64,
        size: u32,
        key: u64,
        alpha: f64,
        corpus: Vec<CorpusImage>,
        source_images: Vec<SourceImageMetadata>,
        decode_tool_version: Option<String>,
        corpus_kind: &str,
    ) -> BenchmarkResult {
        let transformations = standard_adversarial_suite();
        let mut control_keys = Vec::with_capacity(17);
        control_keys.push(key);
        for i in 1..=16u64 {
            control_keys.push(mix64(key ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        }
        let report =
            self.run_with_controls(&corpus, key, alpha, &transformations, &control_keys, seed);
        let corpus_fingerprint = corpus_fingerprint(&corpus);
        BenchmarkResult {
            seed,
            n_images: corpus.len(),
            width: size,
            height: size,
            key,
            alpha,
            corpus_fingerprint,
            corpus_kind: corpus_kind.to_string(),
            source_images,
            decode_tool_version,
            corpus,
            transformations,
            report,
            codec_roundtrip: Some(CodecRoundTripReport::not_requested()),
        }
    }
}

/// Reproducible run metadata plus all measured results.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    pub seed: u64,
    pub n_images: usize,
    pub width: u32,
    pub height: u32,
    pub key: u64,
    pub alpha: f64,
    /// FNV-1a 64-bit corpus fingerprint (identity aid, not a security hash).
    pub corpus_fingerprint: u64,
    /// `synthetic_adversarial` or `local_files`.
    pub corpus_kind: String,
    /// Source-file hashes and decode geometry for caller-provided images.
    pub source_images: Vec<SourceImageMetadata>,
    /// Decoder build used to normalize local source images, when applicable.
    pub decode_tool_version: Option<String>,
    /// Retained decoded pixels so optional codec measurements use the same
    /// corpus as the deterministic transform measurements.
    pub corpus: Vec<CorpusImage>,
    pub transformations: Vec<Compression>,
    pub report: RobustnessReport,
    /// Optional measurement through an installed external image codec.
    pub codec_roundtrip: Option<CodecRoundTripReport>,
}

/// Result of an optional real encoder/decoder measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct CodecRoundTripReport {
    /// `not_requested`, `measured`, or `unavailable`.
    pub status: String,
    pub codec: String,
    pub encoder_version: Option<String>,
    pub note: Option<String>,
    pub qualities: Vec<CodecQualityResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodecQualityResult {
    pub quality: u8,
    pub samples: usize,
    pub hits: usize,
    pub detection_rate: f64,
    pub mean_max_delta: f64,
}

impl CodecRoundTripReport {
    fn not_requested() -> Self {
        Self {
            status: "not_requested".into(),
            codec: "mjpeg".into(),
            encoder_version: None,
            note: None,
            qualities: Vec::new(),
        }
    }
}

/// Measure an MJPEG encode/decode round-trip using an installed `ffmpeg`.
/// Failures are reported as unavailable so optional codec support cannot
/// prevent the deterministic surrogate benchmark from completing.
pub fn measure_ffmpeg_mjpeg(result: &mut BenchmarkResult, executable: &str) {
    measure_ffmpeg_codec(result, executable, "mjpeg");
}

/// Measure a lossless PNG encode/decode round-trip through installed ffmpeg.
pub fn measure_ffmpeg_png(result: &mut BenchmarkResult, executable: &str) {
    measure_ffmpeg_codec(result, executable, "png");
}

fn measure_ffmpeg_codec(result: &mut BenchmarkResult, executable: &str, codec: &str) {
    let (encoder, quality_flag, settings): (&str, &str, &[u8]) = match codec {
        "mjpeg" => ("mjpeg", "-q:v", &[2, 5, 8]),
        "png" => ("png", "-compression_level", &[1, 6, 9]),
        _ => {
            result.codec_roundtrip = Some(codec_unavailable(codec, None, "unsupported codec"));
            return;
        }
    };
    let version = match Command::new(executable).arg("-version").output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("ffmpeg version unavailable")
            .to_string(),
        Ok(_) => {
            result.codec_roundtrip = Some(codec_unavailable(codec, None, "ffmpeg -version failed"));
            return;
        }
        Err(error) => {
            result.codec_roundtrip = Some(codec_unavailable(
                codec,
                None,
                &format!("could not start ffmpeg: {error}"),
            ));
            return;
        }
    };
    let corpus = &result.corpus;
    let mut measurements = Vec::new();
    for quality in settings {
        let mut hits = 0usize;
        let mut delta_sum = 0.0;
        for image in corpus {
            let marked = embed(
                &image.pixels,
                image.w,
                image.h,
                result.key,
                image.payload,
                result.alpha,
            );
            let quality_arg = quality.to_string();
            let size_arg = format!("{}x{}", image.w, image.h);
            let Some(encoded) = ffmpeg_pipe(
                executable,
                &[
                    "-v",
                    "error",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "gray",
                    "-s:v",
                    &size_arg,
                    "-i",
                    "pipe:0",
                    "-frames:v",
                    "1",
                    "-an",
                    "-c:v",
                    encoder,
                    quality_flag,
                    &quality_arg,
                    "-f",
                    "image2pipe",
                    "-vcodec",
                    encoder,
                    "pipe:1",
                ],
                &marked,
            ) else {
                result.codec_roundtrip = Some(codec_unavailable(
                    codec,
                    Some(version),
                    &format!("ffmpeg {codec} encode failed at setting {quality}"),
                ));
                return;
            };
            let Some(decoded) = ffmpeg_pipe(
                executable,
                &[
                    "-v",
                    "error",
                    "-f",
                    "image2pipe",
                    "-vcodec",
                    encoder,
                    "-i",
                    "pipe:0",
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "gray",
                    "pipe:1",
                ],
                &encoded,
            ) else {
                result.codec_roundtrip = Some(codec_unavailable(
                    codec,
                    Some(version),
                    &format!("ffmpeg {codec} decode failed at setting {quality}"),
                ));
                return;
            };
            if decoded.len() != marked.len() {
                result.codec_roundtrip = Some(codec_unavailable(
                    codec,
                    Some(version),
                    &format!("decoded frame size mismatch at quality {quality}"),
                ));
                return;
            }
            if detect(&decoded, image.w, image.h, result.key) == Some(image.payload) {
                hits += 1;
            }
            delta_sum += f64::from(max_delta(
                &marked, image.w, image.h, &decoded, image.w, image.h,
            ));
        }
        measurements.push(CodecQualityResult {
            quality: *quality,
            samples: corpus.len(),
            hits,
            detection_rate: rate(hits, corpus.len()),
            mean_max_delta: if corpus.is_empty() {
                0.0
            } else {
                delta_sum / corpus.len() as f64
            },
        });
    }
    result.codec_roundtrip = Some(CodecRoundTripReport { status: "measured".into(), codec: codec.into(),
        encoder_version: Some(version), note: Some(format!("Synthetic grayscale corpus; applies only to this ffmpeg {codec} round-trip and reported settings.")), qualities: measurements });
}

fn codec_unavailable(codec: &str, version: Option<String>, note: &str) -> CodecRoundTripReport {
    CodecRoundTripReport {
        status: "unavailable".into(),
        codec: codec.into(),
        encoder_version: version,
        note: Some(note.into()),
        qualities: Vec::new(),
    }
}

fn ffmpeg_pipe(executable: &str, args: &[&str], input: &[u8]) -> Option<Vec<u8>> {
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;

    // Drain stdout while feeding stdin. A synchronous write followed by
    // wait_with_output can deadlock when the child fills its stdout pipe
    // before consuming the complete input frame (common with large images).
    let (status, write_result, read_result) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(input));
        let reader = scope.spawn(move || {
            let mut output = Vec::new();
            stdout.read_to_end(&mut output).map(|_| output)
        });
        let status = child.wait().ok();
        (status, writer.join().ok(), reader.join().ok())
    });
    write_result?.ok()?;
    let output = read_result?.ok()?;
    status?.success().then_some(output)
}

impl BenchmarkResult {
    /// Stable JSON report with the complete run configuration and exact
    /// measurements. No pass threshold is implied by these measurements.
    pub fn to_json(&self) -> String {
        let mut json = format!(
            "{{\n  \"schema\":\"GY-BENCH-REPORT/1\",\n  \"seed\":{},\n  \"images\":{},\n  \"width\":{},\n  \"height\":{},\n  \"key_hex\":\"{:016x}\",\n  \"alpha\":{},\n  \"corpus_kind\":\"{}\",\n  \"decode_tool_version\":{},\n  \"corpus_fnv1a64\":\"{:016x}\",\n  \"threshold_policy\":\"measurements only; no general robustness threshold\",\n  \"transforms\":[",
            self.seed,
            self.n_images,
            self.width,
            self.height,
            self.key,
            self.alpha,
            json_escape(&self.corpus_kind),
            self.decode_tool_version
                .as_ref()
                .map(|version| format!("\"{}\"", json_escape(version)))
                .unwrap_or_else(|| "null".into()),
            self.corpus_fingerprint
        );
        for (index, (transform, result)) in self
            .transformations
            .iter()
            .zip(&self.report.transforms)
            .enumerate()
        {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "\n    {{\"name\":\"{}\",\"samples\":{},\"hits\":{},\"detection_rate\":{:.6},\"mean_max_delta\":{:.3}}}",
                json_escape(&transform.label()),
                result.samples,
                result.hits,
                result.detection_rate,
                result.mean_max_delta
            ));
        }
        json.push_str(&format!(
            "\n  ],\n  \"false_positive_controls\":{},\n  \"false_positives\":{},\n  \"false_positive_rate\":{:.8},\n  \"valid_images\":{},\n  \"invalid_images\":{}",
            self.report.false_positive_controls,
            self.report.false_positives,
            self.report.false_positive_rate,
            self.report.n_valid_images,
            self.report.n_invalid_images
        ));
        json.push_str(",\n  \"source_images\":[");
        for (index, source) in self.source_images.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"path\":\"{}\",\"sha256\":\"{}\",\"source_width\":{},\"source_height\":{},\"normalized_width\":{},\"normalized_height\":{}}}",
                json_escape(&source.path),
                source.sha256,
                source.source_width,
                source.source_height,
                source.normalized_width,
                source.normalized_height
            ));
        }
        json.push(']');
        if let Some(codec) = &self.codec_roundtrip {
            json.push_str(&format!(",\n  \"codec_roundtrip\":{{\"status\":\"{}\",\"codec\":\"{}\",\"encoder_version\":{},\"note\":{},\"qualities\":[",
                json_escape(&codec.status), json_escape(&codec.codec),
                codec.encoder_version.as_ref().map(|v| format!("\"{}\"", json_escape(v))).unwrap_or_else(|| "null".into()),
                codec.note.as_ref().map(|v| format!("\"{}\"", json_escape(v))).unwrap_or_else(|| "null".into())));
            for (index, quality) in codec.qualities.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&format!("{{\"quality\":{},\"samples\":{},\"hits\":{},\"detection_rate\":{:.6},\"mean_max_delta\":{:.3}}}",
                    quality.quality, quality.samples, quality.hits, quality.detection_rate, quality.mean_max_delta));
            }
            json.push_str("]}");
        }
        json.push_str("\n}\n");
        json
    }
}

/// Standard deterministic transform suite. `JPEGish` is a block-quantisation
/// surrogate, not an encoded JPEG round-trip; all rows in the report identify
/// the exact operation and parameter actually measured.
pub fn standard_adversarial_suite() -> Vec<Compression> {
    vec![
        Compression::JPEGish { quality: 80 },
        Compression::JPEGish { quality: 55 },
        Compression::Resize { factor: 2 },
        Compression::Resize { factor: 3 },
        Compression::Crop { fraction: 0.75 },
        Compression::Crop { fraction: 0.50 },
        Compression::Noise { amplitude: 2 },
        Compression::Noise { amplitude: 12 },
        Compression::Contrast {
            numerator: 4,
            denominator: 5,
        },
        Compression::Contrast {
            numerator: 6,
            denominator: 5,
        },
        Compression::Brightness { offset: -16 },
        Compression::Brightness { offset: 16 },
        Compression::BoxBlur { radius: 1 },
        Compression::BoxBlur { radius: 2 },
        Compression::SaltPepper { per_mille: 10 },
    ]
}

fn corpus_fingerprint(corpus: &[CorpusImage]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut update = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for image in corpus {
        update(&(image.name.len() as u64).to_le_bytes());
        update(image.name.as_bytes());
        update(&image.w.to_le_bytes());
        update(&image.h.to_le_bytes());
        update(&image.payload.to_le_bytes());
        update(&image.pixels);
    }
    hash
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push('?'),
            ch => out.push(ch),
        }
    }
    out
}

/// Fraction `hits / total`, defined as 0.0 when `total` is 0.
fn rate(hits: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64
    }
}

/// Maximum absolute pixel delta over the overlapping region of two images.
fn max_delta(a: &[u8], aw: u32, ah: u32, b: &[u8], bw: u32, bh: u32) -> u8 {
    let w = aw.min(bw) as usize;
    let h = ah.min(bh) as usize;
    let mut max = 0u8;
    for y in 0..h {
        for x in 0..w {
            let d = i32::from(a[y * aw as usize + x]) - i32::from(b[y * bw as usize + x]);
            max = max.max(d.unsigned_abs().min(255) as u8);
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: u64 = 0x0A11_CEC0_FFEE;
    const ALPHA: f64 = 24.0;

    fn corpus() -> Vec<CorpusImage> {
        Corpus::generate(7, 6, 64)
    }

    #[test]
    fn local_corpus_preflight_rejects_invalid_requests_before_external_tools() {
        assert_eq!(
            Corpus::from_local_paths(&[], 1, 64, "missing-ffmpeg", "missing-ffprobe"),
            Err(LocalCorpusError::EmptyCorpus)
        );
        assert_eq!(
            Corpus::from_local_paths(
                &[PathBuf::from("b.png"), PathBuf::from("a.png")],
                1,
                32,
                "missing-ffmpeg",
                "missing-ffprobe"
            ),
            Err(LocalCorpusError::InvalidTargetSize(32))
        );
        assert_eq!(
            Corpus::from_local_paths(
                &[PathBuf::from("same.png"), PathBuf::from("same.png")],
                1,
                64,
                "missing-ffmpeg",
                "missing-ffprobe"
            ),
            Err(LocalCorpusError::DuplicatePath)
        );
        let too_many = vec![PathBuf::from("image.png"); MAX_LOCAL_IMAGES + 1];
        assert_eq!(
            Corpus::from_local_paths(&too_many, 1, 64, "missing-ffmpeg", "missing-ffprobe"),
            Err(LocalCorpusError::TooManyImages(MAX_LOCAL_IMAGES + 1))
        );
        let too_many_pixels = vec![PathBuf::from("image.png"); 9];
        assert_eq!(
            Corpus::from_local_paths(
                &too_many_pixels,
                1,
                1024,
                "missing-ffmpeg",
                "missing-ffprobe"
            ),
            Err(LocalCorpusError::PixelLimit)
        );
    }

    #[test]
    fn local_paths_sort_lexicographically_for_stable_corpus_order() {
        assert_eq!(
            sort_local_paths(&[PathBuf::from("z.png"), PathBuf::from("a.png")]),
            vec![PathBuf::from("a.png"), PathBuf::from("z.png")]
        );
    }

    #[test]
    fn local_source_dimensions_are_rejected_before_decode_allocation() {
        let path = Path::new("bad.png");
        assert!(matches!(
            add_source_dimensions(0, path, 0, 64),
            Err(LocalCorpusError::SourceDimensionLimit { .. })
        ));
        assert!(matches!(
            add_source_dimensions(0, path, MAX_LOCAL_EDGE + 1, 64),
            Err(LocalCorpusError::SourceDimensionLimit { .. })
        ));
        assert_eq!(
            add_source_dimensions(MAX_LOCAL_SOURCE_PIXELS, path, 1, 1),
            Err(LocalCorpusError::SourcePixelLimit)
        );
    }

    #[test]
    fn local_corpus_provenance_is_serialized_in_the_report() {
        let mut image = Corpus::generate(7, 1, 64).remove(0);
        image.name = "portrait.png".into();
        let result = Benchmark.run_local(
            7,
            64,
            KEY,
            ALPHA,
            LocalCorpus {
                images: vec![image],
                sources: vec![SourceImageMetadata {
                    path: "/licensed/data/portrait.png".into(),
                    sha256: "ab".repeat(32),
                    source_width: 120,
                    source_height: 240,
                    normalized_width: 64,
                    normalized_height: 64,
                }],
                ffmpeg_version: "ffmpeg test build".into(),
            },
        );
        let json = result.to_json();
        assert!(json.contains("\"corpus_kind\":\"local_files\""));
        assert!(json.contains("\"decode_tool_version\":\"ffmpeg test build\""));
        assert!(json.contains("\"sha256\":\"abababab"));
        assert!(json.contains("\"normalized_width\":64"));
    }

    #[cfg(unix)]
    #[test]
    fn subprocess_pipe_drains_large_stdout_while_writing_stdin() {
        // Larger than a typical OS pipe buffer. `cat` echoes stdin to stdout;
        // the helper must drain output concurrently or the writer and child
        // can block each other.
        let input = vec![0xA5; 1024 * 1024];
        assert_eq!(ffmpeg_pipe("sh", &["-c", "cat"], &input), Some(input));
    }

    /// The three test transformations shared by the rate tests.
    fn transformations() -> Vec<Compression> {
        vec![
            Compression::JPEGish { quality: 75 },
            Compression::Resize { factor: 2 },
            Compression::Crop { fraction: 0.75 },
        ]
    }

    fn rate_for(report: &RobustnessReport, label: &str) -> f64 {
        report
            .transforms
            .iter()
            .find(|t| t.label == label)
            .map(|t| t.detection_rate)
            .unwrap_or_else(|| panic!("missing transform {}", label))
    }

    #[test]
    fn clean_detection_is_perfect() {
        let corpus = corpus();
        // Direct clean round trip through the watermark crate: embed then
        // detect with no perturbation must recover every payload exactly.
        for img in &corpus {
            let marked = embed(&img.pixels, img.w, img.h, KEY, img.payload, ALPHA);
            assert_eq!(
                detect(&marked, img.w, img.h, KEY),
                Some(img.payload),
                "clean detection failed for {}",
                img.name
            );
        }
    }

    #[test]
    fn clean_detection_through_harness_is_perfect() {
        let corpus = corpus();
        // An identity-equivalent transform: crop with fraction 1.0 keeps the
        // whole image, exercising the full harness embed->transform->detect
        // path without altering pixels.
        let report = Benchmark.run(&corpus, KEY, ALPHA, &[Compression::Crop { fraction: 1.0 }]);
        assert!((rate_for(&report, "crop(1.00)") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn jpegish_moderate_quality_keeps_high_rate() {
        // Measured: block-anchored quantisation with step 4 (quality 75)
        // preserves mid-frequency DCT sign structure at alpha 24 on the
        // gradient/checkerboard/noise corpus. Empirically this lands at 1.0
        // on this corpus; 0.8 is the documented assurance gate.
        let corpus = corpus();
        let report = Benchmark.run(&corpus, KEY, ALPHA, &[Compression::JPEGish { quality: 75 }]);
        let rate = rate_for(&report, "jpegish(q75)");
        assert!(
            rate >= 0.8,
            "jpegish(q75) detection rate {} fell below the documented 0.8 gate",
            rate
        );
    }

    #[test]
    fn resize_factor_two_keeps_at_least_half() {
        let corpus = corpus();
        let report = Benchmark.run(&corpus, KEY, ALPHA, &[Compression::Resize { factor: 2 }]);
        let rate = rate_for(&report, "resize(x2)");
        assert!(
            rate >= 0.5,
            "resize(x2) detection rate {} fell below 0.5",
            rate
        );
    }

    #[test]
    fn crop_is_measured_and_honestly_reported() {
        let corpus = corpus();
        let report = Benchmark.run(&corpus, KEY, ALPHA, &[Compression::Crop { fraction: 0.5 }]);
        let t = report
            .transforms
            .iter()
            .find(|t| t.label == "crop(0.50)")
            .expect("crop result present");
        // The directive demands honest reporting, not a pass: aggressive crop
        // is expected to break block alignment. We only assert the
        // measurement exists, is in range, and that distortion is reported.
        assert!(
            (0.0..=1.0).contains(&t.detection_rate),
            "crop rate {} out of range",
            t.detection_rate
        );
        assert!(t.mean_max_delta.is_finite());
    }

    #[test]
    fn no_false_positives_on_unmarked_images() {
        let corpus = corpus();
        let report = Benchmark.run(&corpus, KEY, ALPHA, &transformations());
        assert_eq!(report.false_positive_rate, 0.0);
    }

    #[test]
    fn report_is_deterministic_across_runs() {
        let corpus = corpus();
        let a = Benchmark.run(&corpus, KEY, ALPHA, &transformations());
        let b = Benchmark.run(&corpus, KEY, ALPHA, &transformations());
        assert_eq!(a, b);
        assert_eq!(format!("{}", a), format!("{}", b));
    }

    #[test]
    fn corpus_is_deterministic_and_payloads_distinct() {
        let a = Corpus::generate(123, 9, 48);
        let b = Corpus::generate(123, 9, 48);
        assert_eq!(a, b);
        for (i, img) in a.iter().enumerate() {
            assert_eq!(img.pixels.len(), 48 * 48);
            assert_eq!(img.w, 48);
            assert_eq!(img.h, 48);
            assert!(
                a[..i].iter().all(|prev| prev.payload != img.payload),
                "payload collision at index {}",
                i
            );
        }
    }

    #[test]
    fn adversarial_corpus_cycles_six_stable_patterns_and_bounds_allocations() {
        let a = Corpus::adversarial(42, 12, 64);
        let b = Corpus::adversarial(42, 12, 64);
        assert_eq!(a, b);
        let prefixes: std::collections::BTreeSet<_> = a
            .iter()
            .map(|image| image.name.split('_').next().unwrap_or(""))
            .collect();
        assert_eq!(
            prefixes,
            [
                "checkerboard",
                "gradient",
                "low",
                "noise",
                "rings",
                "stripes"
            ]
            .into_iter()
            .collect()
        );
        assert!(Corpus::generate(0, usize::MAX, u32::MAX).is_empty());
    }

    #[test]
    fn complete_suite_reports_seeded_controls_and_serializes_deterministically() {
        let result = Benchmark.run_adversarial(42, 6, 64, KEY, ALPHA);
        assert_eq!(result.report.n_valid_images, 6);
        assert_eq!(result.report.n_invalid_images, 0);
        assert_eq!(result.report.transforms.len(), 15);
        assert_eq!(result.report.false_positive_controls, 6 * 17);
        assert_eq!(result.report.false_positives, 0);
        let json = result.to_json();
        assert!(json.starts_with("{\n  \"schema\":\"GY-BENCH-REPORT/1\""));
        assert!(json.contains(
            "\"threshold_policy\":\"measurements only; no general robustness threshold\""
        ));
        assert!(json.contains("\"codec_roundtrip\":{\"status\":\"not_requested\""));
        assert_eq!(
            json,
            Benchmark.run_adversarial(42, 6, 64, KEY, ALPHA).to_json()
        );
    }

    #[test]
    fn unavailable_optional_codec_is_explicit_and_keeps_surrogate_results() {
        let mut result = Benchmark.run_adversarial(42, 1, 64, KEY, ALPHA);
        measure_ffmpeg_mjpeg(&mut result, "/definitely/missing/ffmpeg");
        let codec = result.codec_roundtrip.as_ref().unwrap();
        assert_eq!(codec.status, "unavailable");
        assert!(codec.note.as_deref().unwrap().contains("could not start"));
        assert_eq!(result.report.transforms.len(), 15);
        let json = result.to_json();
        assert!(json.contains("\"status\":\"unavailable\""));
        assert!(json.ends_with("}\n"));
    }

    #[test]
    fn added_surrogates_are_deterministic_and_shape_preserving() {
        let pixels: Vec<u8> = (0..64 * 64).map(|i| (i % 251) as u8).collect();
        for transform in [
            Compression::Contrast {
                numerator: 4,
                denominator: 5,
            },
            Compression::Brightness { offset: -15 },
            Compression::BoxBlur { radius: 2 },
            Compression::SaltPepper { per_mille: 15 },
        ] {
            let a = transform.apply(&pixels, 64, 64, 99);
            let b = transform.apply(&pixels, 64, 64, 99);
            assert_eq!(a, b, "{} was not deterministic", transform.label());
            assert_eq!((a.0.len(), a.1, a.2), (pixels.len(), 64, 64));
        }
    }

    #[test]
    fn transformations_are_shape_correct() {
        let pixels: Vec<u8> = (0..64 * 64).map(|i| (i % 256) as u8).collect();
        let (out, w, h) = Compression::JPEGish { quality: 90 }.apply(&pixels, 64, 64, 0);
        assert_eq!((w, h, out.len()), (64, 64, 64 * 64));
        let (out, w, h) = Compression::Resize { factor: 2 }.apply(&pixels, 64, 64, 0);
        assert_eq!((w, h, out.len()), (64, 64, 64 * 64));
        let (out, w, h) = Compression::Crop { fraction: 0.5 }.apply(&pixels, 64, 64, 0);
        assert_eq!((w, h, out.len()), (32, 32, 32 * 32));
        let (out, w, h) = Compression::Noise { amplitude: 4 }.apply(&pixels, 64, 64, 0);
        assert_eq!((w, h, out.len()), (64, 64, 64 * 64));
        // Noise is deterministic per image index.
        let (again, _, _) = Compression::Noise { amplitude: 4 }.apply(&pixels, 64, 64, 0);
        assert_eq!(out, again);
    }

    #[test]
    fn display_renders_all_sections() {
        let corpus = corpus();
        let report = Benchmark.run(&corpus, KEY, ALPHA, &transformations());
        let text = format!("{}", report);
        assert!(text.contains("robustness report: 6 corpus images"));
        assert!(text.contains("jpegish(q75)"));
        assert!(text.contains("resize(x2)"));
        assert!(text.contains("crop(0.75)"));
        assert!(text.contains("false_positive_rate=0.000"));
    }
}
