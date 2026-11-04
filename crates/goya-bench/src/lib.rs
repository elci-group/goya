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
            n_images,
            width: size,
            height: size,
            key,
            alpha,
            corpus_fingerprint,
            transformations,
            report,
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
    pub transformations: Vec<Compression>,
    pub report: RobustnessReport,
}

impl BenchmarkResult {
    /// Stable JSON report with the complete run configuration and exact
    /// measurements. No pass threshold is implied by these measurements.
    pub fn to_json(&self) -> String {
        let mut json = format!(
            "{{\n  \"schema\":\"GY-BENCH-REPORT/1\",\n  \"seed\":{},\n  \"images\":{},\n  \"width\":{},\n  \"height\":{},\n  \"key_hex\":\"{:016x}\",\n  \"alpha\":{},\n  \"corpus_fnv1a64\":\"{:016x}\",\n  \"threshold_policy\":\"measurements only; no general robustness threshold\",\n  \"transforms\":[",
            self.seed,
            self.n_images,
            self.width,
            self.height,
            self.key,
            self.alpha,
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
            "\n  ],\n  \"false_positive_controls\":{},\n  \"false_positives\":{},\n  \"false_positive_rate\":{:.8},\n  \"valid_images\":{},\n  \"invalid_images\":{}\n}}\n",
            self.report.false_positive_controls,
            self.report.false_positives,
            self.report.false_positive_rate,
            self.report.n_valid_images,
            self.report.n_invalid_images
        ));
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
        assert_eq!(
            json,
            Benchmark.run_adversarial(42, 6, 64, KEY, ALPHA).to_json()
        );
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
