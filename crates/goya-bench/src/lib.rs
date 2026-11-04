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
        if size == 0 {
            return Vec::new();
        }
        let mut images = Vec::with_capacity(n_images);
        for i in 0..n_images {
            let payload = mix64(seed ^ mix64((i as u64).wrapping_add(1))) | 1;
            let rng_seed = mix64(seed ^ mix64((i as u64).wrapping_add(0x9E37)));
            let mut rng = Xorshift64(rng_seed | 1);
            let name = match i % 3 {
                0 => format!("gradient_{:02}", i),
                1 => format!("checkerboard_{:02}", i),
                _ => format!("noise_{:02}", i),
            };
            let pixels = match i % 3 {
                0 => Self::gradient(size),
                1 => Self::checkerboard(size),
                _ => Self::noise_field(size, &mut rng),
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
            .collect()
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
}

impl Compression {
    /// Stable label used in report output.
    pub fn label(&self) -> String {
        match self {
            Compression::JPEGish { quality } => format!("jpegish(q{})", quality),
            Compression::Resize { factor } => format!("resize(x{})", factor),
            Compression::Crop { fraction } => format!("crop({:.2})", fraction),
            Compression::Noise { amplitude } => format!("noise(+-{})", amplitude),
        }
    }

    /// Apply the transformation, returning the transformed pixel buffer and
    /// its dimensions.
    fn apply(&self, pixels: &[u8], w: u32, h: u32, image_index: usize) -> (Vec<u8>, u32, u32) {
        match *self {
            Compression::JPEGish { quality } => (jpegish(pixels, w, h, quality), w, h),
            Compression::Resize { factor } => resize(pixels, w, h, factor.max(1)),
            Compression::Crop { fraction } => crop(pixels, w, h, fraction),
            Compression::Noise { amplitude } => (add_noise(pixels, image_index, amplitude), w, h),
        }
    }
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
fn add_noise(pixels: &[u8], image_index: usize, amplitude: i32) -> Vec<u8> {
    let mut rng = Xorshift64(mix64((image_index as u64).wrapping_add(0x51_5E_2B)) | 1);
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
}

/// The reproducible robustness evaluation demanded by directive §9.
#[derive(Debug, Clone, PartialEq)]
pub struct RobustnessReport {
    /// Number of corpus images evaluated.
    pub n_images: usize,
    /// One measurement per requested transformation, in order.
    pub transforms: Vec<TransformResult>,
    /// Fraction of UNMARKED corpus images on which detection reported a
    /// payload (false positives; target is 0).
    pub false_positive_rate: f64,
}

impl std::fmt::Display for RobustnessReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "robustness report: {} corpus images", self.n_images)?;
        for t in &self.transforms {
            writeln!(
                f,
                "  {:<16} detection_rate={:.3} mean_max_delta={:.1}",
                t.label, t.detection_rate, t.mean_max_delta
            )?;
        }
        write!(
            f,
            "  {:<16} false_positive_rate={:.3}",
            "unmarked", self.false_positive_rate
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
        // False positives: detection on unmarked images must report nothing.
        let valid = |img: &&CorpusImage| {
            usize::try_from(img.w)
                .ok()
                .and_then(|w| usize::try_from(img.h).ok().and_then(|h| w.checked_mul(h)))
                == Some(img.pixels.len())
                && img.w > 0
                && img.h > 0
        };
        let false_positives = corpus
            .iter()
            .filter(valid)
            .filter(|img| detect(&img.pixels, img.w, img.h, key).is_some())
            .count();
        let false_positive_rate = rate(false_positives, corpus.len());

        let mut transforms = Vec::with_capacity(transformations.len());
        for (ti, t) in transformations.iter().enumerate() {
            let mut hits = 0usize;
            let mut delta_sum = 0.0f64;
            for (ii, img) in corpus.iter().enumerate() {
                if !valid(&img) {
                    // Malformed public CorpusImage values are treated as
                    // misses; a benchmark report must not panic on bad shape.
                    continue;
                }
                let marked = embed(&img.pixels, img.w, img.h, key, img.payload, alpha);
                let (transformed, tw, th) = t.apply(&marked, img.w, img.h, ii + ti * 7);
                if detect(&transformed, tw, th, key) == Some(img.payload) {
                    hits += 1;
                }
                delta_sum += f64::from(max_delta(&marked, img.w, img.h, &transformed, tw, th));
            }
            transforms.push(TransformResult {
                label: t.label(),
                detection_rate: rate(hits, corpus.len()),
                mean_max_delta: if corpus.is_empty() {
                    0.0
                } else {
                    delta_sum / corpus.len() as f64
                },
            });
        }

        RobustnessReport {
            n_images: corpus.len(),
            transforms,
            false_positive_rate,
        }
    }
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
