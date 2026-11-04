//! Goya spatial audio: geometry-derived panning plus pluggable HRTF and
//! speaker-array rendering.
//!
//! Contract: `GY-SPATIAL-AUDIO/1`.
//!
//! This crate maps millipoint [`goya_layout::Rect`]s onto a normalised pan
//! position `p ∈ [−1, +1]` (viewport-relative horizontal centre, clamped),
//! applies equal-power stereo panning, bounded distance attenuation for
//! z-layer depth, exponential trajectory smoothing, and mono fold-down. The
//! crate also provides a deterministic FIR binaural renderer that accepts
//! measured HRTF impulse responses from a host-supplied profile, plus
//! direction-aware speaker-array gains for layouts with height channels.

use goya_layout::Rect;

/// Maximum number of taps accepted from a host HRTF profile.
pub const MAX_HRTF_TAPS: usize = 1024;
/// Maximum number of directional responses accepted by the text profile loader.
pub const MAX_HRTF_RESPONSES: usize = 4096;
/// Absolute coefficient limit for profile data, preventing pathological gains.
pub const MAX_HRTF_TAP_MAGNITUDE: f32 = 16.0;

/// Deterministic contract identifier for this crate.
pub const SPATIAL_CONTRACT: &str = "GY-SPATIAL-AUDIO/1";

/// Maximum attenuation floor for the bounded inverse distance law.
const MIN_DISTANCE_GAIN: f32 = 0.05;

/// Clamp a value into an inclusive range (panic-free by construction).
fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Map a layout rect to a normalised pan position in `[−1, +1]`.
///
/// The pan is the viewport-relative horizontal centre of `rect`: `−1` is the
/// left edge of the viewport, `+1` the right edge, `0` the centre. Values are
/// clamped, so rects outside the viewport or degenerate viewports never
/// produce out-of-range or non-finite results.
pub fn pan_from_rect(rect: Rect, viewport: Rect) -> f32 {
    let rect_cx = rect.x + rect.width / 2;
    let view_cx = viewport.x + viewport.width / 2;
    let half_w = viewport.width / 2;
    if half_w <= 0 {
        return 0.0;
    }
    // i64 → f32 casts are safe in practice for millipoint layouts; clamp
    // after the conversion so extreme values still yield [-1, 1].
    let p = (rect_cx - view_cx) as f32 / half_w as f32;
    clamp(p, -1.0, 1.0)
}

/// Equal-power stereo gains for pan `p ∈ [−1, +1]`.
///
/// `gL = cos(π/4 · (p + 1))`, `gR = sin(π/4 · (p + 1))`, so
/// `gL² + gR² = 1` for every input; out-of-range inputs are clamped.
pub fn stereo_gains(p: f32) -> (f32, f32) {
    let p = clamp(p, -1.0, 1.0);
    let theta = core::f32::consts::FRAC_PI_4 * (p + 1.0);
    (theta.cos(), theta.sin())
}

/// Bounded inverse distance attenuation for a z-layer depth.
///
/// `gain = reference / (reference + max(0, z_layers − reference))`, clamped
/// to `[0.05, 1.0]`. Foreground layers (`z_layers ≤ reference`) sit at full
/// gain; deeper layers fade but never below the floor. Degenerate
/// `reference ≤ 0` falls back to full gain rather than dividing by zero.
pub fn distance_gain(z_layers: i32, reference: i32) -> f32 {
    if reference <= 0 {
        return 1.0;
    }
    let z = i64::from(z_layers.max(0));
    let r = i64::from(reference);
    let gain = r as f32 / (r + (z - r).max(0)) as f32;
    clamp(gain, MIN_DISTANCE_GAIN, 1.0)
}

/// Mono fold-down: the equal-power pair collapses to its midpoint so the
/// signal remains audible on mono outputs.
pub fn fold_mono(gains: (f32, f32)) -> f32 {
    (gains.0 + gains.1) / 2.0
}

/// Exponential trajectory smoother for stereo pan positions.
///
/// Holds a current pan that exponentially interpolates toward a target on
/// each [`StereoPan::step`], preventing the discontinuities (clicks) that a
/// hard retarget would produce.
#[derive(Debug, Clone)]
pub struct StereoPan {
    current: f32,
    target: f32,
}

impl StereoPan {
    /// A smoother starting centred, targeting centre.
    pub fn new() -> Self {
        StereoPan {
            current: 0.0,
            target: 0.0,
        }
    }

    /// Set the pan target; out-of-range values are clamped to `[−1, +1]`.
    pub fn set_target(&mut self, p: f32) {
        self.target = clamp(p, -1.0, 1.0);
    }

    /// Advance one smoothing step with interpolation factor `alpha ∈ (0, 1]`
    /// and return the equal-power gains for the current position.
    ///
    /// `current += alpha · (target − current)`; out-of-range `alpha` is
    /// clamped (`alpha ≤ 0` behaves as no movement, `alpha ≥ 1` snaps).
    pub fn step(&mut self, alpha: f32) -> (f32, f32) {
        let a = clamp(alpha, 0.0, 1.0);
        self.current += a * (self.target - self.current);
        self.current = clamp(self.current, -1.0, 1.0);
        stereo_gains(self.current)
    }

    /// Current smoothed pan position.
    pub fn current(&self) -> f32 {
        self.current
    }

    /// Current pan target.
    pub fn target(&self) -> f32 {
        self.target
    }
}

impl Default for StereoPan {
    fn default() -> Self {
        Self::new()
    }
}

/// Source direction in degrees. Azimuth is positive to the listener's right;
/// elevation is positive above the horizontal plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Direction {
    pub azimuth_deg: f32,
    pub elevation_deg: f32,
}

impl Direction {
    /// Construct a direction after checking the finite spherical ranges.
    pub fn new(azimuth_deg: f32, elevation_deg: f32) -> Result<Self, SpatialError> {
        if !azimuth_deg.is_finite()
            || !elevation_deg.is_finite()
            || !(-180.0..=180.0).contains(&azimuth_deg)
            || !(-90.0..=90.0).contains(&elevation_deg)
        {
            return Err(SpatialError::InvalidDirection);
        }
        Ok(Self {
            azimuth_deg,
            elevation_deg,
        })
    }

    fn vector(self) -> [f32; 3] {
        let az = self.azimuth_deg.to_radians();
        let el = self.elevation_deg.to_radians();
        [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()]
    }
}

/// Spatial renderer errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialError {
    InvalidDirection,
    EmptySpeakerLayout,
    ChannelCountMismatch,
    MissingHrtf,
    InvalidHrtf,
}

impl core::fmt::Display for SpatialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::InvalidDirection => "direction is outside finite azimuth/elevation bounds",
            Self::EmptySpeakerLayout => "speaker layout must contain at least one speaker",
            Self::ChannelCountMismatch => "output channel count does not match the layout",
            Self::MissingHrtf => "HRTF profile has no response for this direction",
            Self::InvalidHrtf => "HRTF responses must be finite, non-empty, and bounded",
        };
        f.write_str(message)
    }
}

impl std::error::Error for SpatialError {}

/// Speaker layout in spherical coordinates. Include overhead/height speakers
/// to preserve elevation; a horizontal-only layout cannot reproduce vertical
/// source direction.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerLayout {
    speakers: Vec<Direction>,
}

impl SpeakerLayout {
    pub fn new(speakers: Vec<Direction>) -> Result<Self, SpatialError> {
        if speakers.is_empty() {
            return Err(SpatialError::EmptySpeakerLayout);
        }
        // Revalidate because Direction fields are public and may be modified.
        for speaker in &speakers {
            Direction::new(speaker.azimuth_deg, speaker.elevation_deg)?;
        }
        Ok(Self { speakers })
    }

    pub fn channel_count(&self) -> usize {
        self.speakers.len()
    }

    /// Equal-power direction gains for each speaker. Gains are the positive
    /// part of the source/speaker unit-vector dot product, normalized by
    /// squared power. If a source falls behind every speaker, the nearest
    /// speaker receives the full signal instead of silence.
    pub fn gains(&self, source: Direction) -> Result<Vec<f32>, SpatialError> {
        let source = Direction::new(source.azimuth_deg, source.elevation_deg)?.vector();
        let mut gains: Vec<f32> = self
            .speakers
            .iter()
            .map(|speaker| {
                let speaker = speaker.vector();
                (source[0] * speaker[0] + source[1] * speaker[1] + source[2] * speaker[2]).max(0.0)
            })
            .collect();
        let power: f32 = gains.iter().map(|gain| gain * gain).sum();
        if power > f32::EPSILON {
            let scale = power.sqrt().recip();
            gains.iter_mut().for_each(|gain| *gain *= scale);
        } else {
            let nearest = self
                .speakers
                .iter()
                .enumerate()
                .map(|(i, speaker)| {
                    let speaker = speaker.vector();
                    (
                        i,
                        source[0] * speaker[0] + source[1] * speaker[1] + source[2] * speaker[2],
                    )
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
                .unwrap_or(0);
            gains[nearest] = 1.0;
        }
        Ok(gains)
    }

    /// Apply one mono sample into a caller-provided interleaved frame.
    pub fn render_sample(
        &self,
        sample: f32,
        source: Direction,
        output: &mut [f32],
    ) -> Result<(), SpatialError> {
        if output.len() != self.speakers.len() {
            return Err(SpatialError::ChannelCountMismatch);
        }
        let gains = self.gains(source)?;
        let sample = if sample.is_finite() { sample } else { 0.0 };
        for (out, gain) in output.iter_mut().zip(gains) {
            *out = sample * gain;
        }
        Ok(())
    }
}

/// Borrowed pair of ear-specific FIR impulse responses supplied by an HRTF
/// dataset or host adapter. `taps[0]` is the current sample coefficient;
/// later taps encode delay and spectral cues such as interaural time
/// difference and pinna filtering.
#[derive(Debug, Clone, Copy)]
pub struct HrtfResponse<'a> {
    pub left: &'a [f32],
    pub right: &'a [f32],
}

/// Pluggable HRTF dataset interface. Implementations may interpolate a
/// measured database (for example SOFA-derived filters) and return stable
/// impulse-response slices for the requested direction.
///
/// Goya does not bundle measured HRTF data; hosts must provide a licensed
/// dataset and its interpolation policy.
pub trait HrtfProfile {
    fn response(&self, direction: Direction) -> Option<HrtfResponse<'_>>;
}

/// Errors returned while loading the bounded `GY-HRTF/1` interchange format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrtfProfileError {
    MissingHeader,
    MissingSampleRate,
    InvalidSampleRate,
    InvalidDirection,
    InvalidTaps,
    DuplicateDirection,
    TooManyResponses,
}

impl core::fmt::Display for HrtfProfileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::MissingHeader => "profile must start with GY-HRTF/1",
            Self::MissingSampleRate => "profile is missing sample_rate_hz",
            Self::InvalidSampleRate => "sample rate must be a positive integer",
            Self::InvalidDirection => "profile contains an invalid direction",
            Self::InvalidTaps => "profile taps must be finite and within supported bounds",
            Self::DuplicateDirection => "profile contains duplicate directions",
            Self::TooManyResponses => "profile exceeds the response count limit",
        };
        f.write_str(message)
    }
}

impl std::error::Error for HrtfProfileError {}

#[derive(Debug, Clone)]
struct OwnedHrtfResponse {
    direction: Direction,
    left: Vec<f32>,
    right: Vec<f32>,
}

/// An owned measured-filter profile loaded from the small deterministic
/// `GY-HRTF/1` text interchange format. Lookup chooses the nearest measured
/// direction by spherical angular distance; ties keep the earlier file row.
/// No resampling or filter interpolation is performed.
#[derive(Debug, Clone)]
pub struct MeasuredHrtfProfile {
    sample_rate_hz: u32,
    responses: Vec<OwnedHrtfResponse>,
}

impl MeasuredHrtfProfile {
    /// Parse a profile. Each data row is
    /// `azimuth_deg,elevation_deg,left_taps,right_taps`; taps are
    /// semicolon-separated decimal f32 values. Blank lines and `#` comments
    /// are ignored after the required first-line header.
    pub fn from_text(text: &str) -> Result<Self, HrtfProfileError> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some("GY-HRTF/1") {
            return Err(HrtfProfileError::MissingHeader);
        }
        let mut sample_rate_hz = None;
        let mut responses = Vec::new();
        for raw_line in lines {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(value) = line.strip_prefix("sample_rate_hz=") {
                let rate = value
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|rate| *rate > 0)
                    .ok_or(HrtfProfileError::InvalidSampleRate)?;
                if sample_rate_hz.replace(rate).is_some() {
                    return Err(HrtfProfileError::InvalidSampleRate);
                }
                continue;
            }
            if sample_rate_hz.is_none() {
                return Err(HrtfProfileError::MissingSampleRate);
            }
            if responses.len() == MAX_HRTF_RESPONSES {
                return Err(HrtfProfileError::TooManyResponses);
            }
            let fields: Vec<_> = line.split(',').map(str::trim).collect();
            if fields.len() != 4 {
                return Err(HrtfProfileError::InvalidTaps);
            }
            let azimuth = fields[0]
                .parse::<f32>()
                .map_err(|_| HrtfProfileError::InvalidDirection)?;
            let elevation = fields[1]
                .parse::<f32>()
                .map_err(|_| HrtfProfileError::InvalidDirection)?;
            let direction = Direction::new(azimuth, elevation)
                .map_err(|_| HrtfProfileError::InvalidDirection)?;
            if responses.iter().any(|entry: &OwnedHrtfResponse| {
                let a = entry.direction.vector();
                let b = direction.vector();
                a[0] * b[0] + a[1] * b[1] + a[2] * b[2] > 1.0 - 1e-6
            }) {
                return Err(HrtfProfileError::DuplicateDirection);
            }
            let left = parse_hrtf_taps(fields[2])?;
            let right = parse_hrtf_taps(fields[3])?;
            responses.push(OwnedHrtfResponse {
                direction,
                left,
                right,
            });
        }
        let sample_rate_hz = sample_rate_hz.ok_or(HrtfProfileError::MissingSampleRate)?;
        if responses.is_empty() {
            return Err(HrtfProfileError::InvalidTaps);
        }
        Ok(Self {
            sample_rate_hz,
            responses,
        })
    }

    /// Sample rate declared by the source data; callers must match or resample
    /// the host stream themselves.
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }
}

fn parse_hrtf_taps(field: &str) -> Result<Vec<f32>, HrtfProfileError> {
    let mut taps = Vec::new();
    for value in field.split(';') {
        if taps.len() == MAX_HRTF_TAPS {
            return Err(HrtfProfileError::InvalidTaps);
        }
        let tap = value
            .trim()
            .parse::<f32>()
            .map_err(|_| HrtfProfileError::InvalidTaps)?;
        if !tap.is_finite() || tap.abs() > MAX_HRTF_TAP_MAGNITUDE {
            return Err(HrtfProfileError::InvalidTaps);
        }
        taps.push(tap);
    }
    if taps.is_empty() {
        return Err(HrtfProfileError::InvalidTaps);
    }
    Ok(taps)
}

impl HrtfProfile for MeasuredHrtfProfile {
    fn response(&self, direction: Direction) -> Option<HrtfResponse<'_>> {
        let source = Direction::new(direction.azimuth_deg, direction.elevation_deg)
            .ok()?
            .vector();
        let mut closest: Option<(&OwnedHrtfResponse, f32)> = None;
        for entry in &self.responses {
            let vector = entry.direction.vector();
            let dot = source[0] * vector[0] + source[1] * vector[1] + source[2] * vector[2];
            if closest.map_or(true, |(_, best)| dot > best) {
                closest = Some((entry, dot));
            }
        }
        closest.map(|(entry, _)| HrtfResponse {
            left: &entry.left,
            right: &entry.right,
        })
    }
}

/// Deterministic binaural FIR renderer backed by a host HRTF profile.
///
/// Processing uses fixed-size history storage and writes interleaved L/R
/// samples into a caller-provided buffer. Direction changes replace the
/// filters while preserving recent input history.
pub struct HrtfRenderer<P: HrtfProfile> {
    profile: P,
    left: Vec<f32>,
    right: Vec<f32>,
    history: [f32; MAX_HRTF_TAPS],
    cursor: usize,
}

impl<P: HrtfProfile> HrtfRenderer<P> {
    pub fn new(profile: P, direction: Direction) -> Result<Self, SpatialError> {
        let mut renderer = Self {
            profile,
            left: Vec::new(),
            right: Vec::new(),
            history: [0.0; MAX_HRTF_TAPS],
            cursor: 0,
        };
        renderer.set_direction(direction)?;
        Ok(renderer)
    }

    pub fn set_direction(&mut self, direction: Direction) -> Result<(), SpatialError> {
        let direction = Direction::new(direction.azimuth_deg, direction.elevation_deg)?;
        let response = self
            .profile
            .response(direction)
            .ok_or(SpatialError::MissingHrtf)?;
        if response.left.is_empty()
            || response.right.is_empty()
            || response.left.len() > MAX_HRTF_TAPS
            || response.right.len() > MAX_HRTF_TAPS
            || response
                .left
                .iter()
                .chain(response.right)
                .any(|tap| !tap.is_finite() || tap.abs() > MAX_HRTF_TAP_MAGNITUDE)
        {
            return Err(SpatialError::InvalidHrtf);
        }
        self.left.clear();
        self.left.extend_from_slice(response.left);
        self.right.clear();
        self.right.extend_from_slice(response.right);
        Ok(())
    }

    pub fn process(
        &mut self,
        input: &[f32],
        interleaved_stereo: &mut [f32],
    ) -> Result<(), SpatialError> {
        if interleaved_stereo.len() != input.len().saturating_mul(2) {
            return Err(SpatialError::ChannelCountMismatch);
        }
        for (sample_index, input_sample) in input.iter().enumerate() {
            self.history[self.cursor] = if input_sample.is_finite() {
                *input_sample
            } else {
                0.0
            };
            let left = fir(&self.history, self.cursor, &self.left);
            let right = fir(&self.history, self.cursor, &self.right);
            interleaved_stereo[2 * sample_index] = left;
            interleaved_stereo[2 * sample_index + 1] = right;
            self.cursor = (self.cursor + 1) % MAX_HRTF_TAPS;
        }
        Ok(())
    }
}

fn fir(history: &[f32; MAX_HRTF_TAPS], cursor: usize, taps: &[f32]) -> f32 {
    taps.iter()
        .enumerate()
        .map(|(delay, tap)| {
            let index = (cursor + MAX_HRTF_TAPS - delay) % MAX_HRTF_TAPS;
            history[index] * tap
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHrtf;

    impl HrtfProfile for TestHrtf {
        fn response(&self, direction: Direction) -> Option<HrtfResponse<'_>> {
            static FRONT_LEFT: [f32; 2] = [1.0, 0.25];
            static FRONT_RIGHT: [f32; 2] = [0.5, -0.25];
            static ELEVATED_LEFT: [f32; 2] = [0.0, 1.0];
            static ELEVATED_RIGHT: [f32; 2] = [1.0, 0.0];
            if direction.elevation_deg > 0.0 {
                Some(HrtfResponse {
                    left: &ELEVATED_LEFT,
                    right: &ELEVATED_RIGHT,
                })
            } else {
                Some(HrtfResponse {
                    left: &FRONT_LEFT,
                    right: &FRONT_RIGHT,
                })
            }
        }
    }

    fn rect(x: i64, width: i64) -> Rect {
        Rect {
            x,
            y: 0,
            width,
            height: 100,
        }
    }

    const VIEWPORT: Rect = Rect {
        x: 0,
        y: 0,
        width: 10_000,
        height: 10_000,
    };

    #[test]
    fn contract_is_declared() {
        assert_eq!(SPATIAL_CONTRACT, "GY-SPATIAL-AUDIO/1");
    }

    #[test]
    fn pan_maps_left_centre_right() {
        let left = pan_from_rect(rect(0, 1_000), VIEWPORT);
        let centre = pan_from_rect(rect(4_500, 1_000), VIEWPORT);
        let right = pan_from_rect(rect(9_000, 1_000), VIEWPORT);
        assert!(left < -0.5, "left rect pan {left} should be < −0.5");
        assert!(
            (centre - 0.0).abs() <= 0.05,
            "centre rect pan {centre} should be ≈0"
        );
        assert!(right > 0.5, "right rect pan {right} should be > 0.5");
    }

    #[test]
    fn pan_clamps_out_of_range_and_degenerate_inputs() {
        // Rect far left / far right of the viewport clamps to ±1.
        assert_eq!(pan_from_rect(rect(-50_000, 1_000), VIEWPORT), -1.0);
        assert_eq!(pan_from_rect(rect(50_000, 1_000), VIEWPORT), 1.0);
        // Degenerate (zero/negative width) viewport yields centre, no panic.
        let degenerate = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 10,
        };
        assert_eq!(pan_from_rect(rect(1_000, 1_000), degenerate), 0.0);
    }

    #[test]
    fn stereo_gains_endpoints_and_centre() {
        let (gl, gr) = stereo_gains(-1.0);
        assert!(
            (gl - 1.0).abs() < 1e-6 && gr.abs() < 1e-6,
            "p=−1 → ({gl},{gr})"
        );
        let (gl, gr) = stereo_gains(0.0);
        let s = core::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (gl - s).abs() < 1e-6 && (gr - s).abs() < 1e-6,
            "p=0 → ({gl},{gr})"
        );
        let (gl, gr) = stereo_gains(1.0);
        assert!(
            gl.abs() < 1e-6 && (gr - 1.0).abs() < 1e-6,
            "p=+1 → ({gl},{gr})"
        );
    }

    #[test]
    fn stereo_gains_preserve_power_across_sweep() {
        let steps = 201;
        for i in 0..steps {
            let p = -1.0 + 2.0 * i as f32 / (steps - 1) as f32;
            let (gl, gr) = stereo_gains(p);
            let power = gl * gl + gr * gr;
            assert!(
                (power - 1.0).abs() < 1e-5,
                "p={p}: power {power} should be ≈1"
            );
        }
    }

    #[test]
    fn distance_gain_orders_foreground_over_background() {
        let reference = 2;
        let foreground = distance_gain(1, reference);
        let at_reference = distance_gain(2, reference);
        let background = distance_gain(5, reference);
        let deep = distance_gain(100, reference);
        assert_eq!(foreground, 1.0);
        assert_eq!(at_reference, 1.0);
        assert!(background < at_reference);
        assert!(deep <= background, "deeper layers must be quieter");
        // Floor respected; degenerate inputs panic-free.
        assert!(deep >= 0.05);
        assert_eq!(distance_gain(10, 0), 1.0);
        assert_eq!(distance_gain(-3, 2), 1.0);
    }

    #[test]
    fn smoothing_converges_without_discontinuities() {
        let mut pan = StereoPan::new();
        pan.set_target(1.0);
        let mut prev = pan.current();
        let mut last_delta = f32::INFINITY;
        for _ in 0..200 {
            let (_gl, _gr) = pan.step(0.1);
            // gL decreases monotonically toward 0 as pan → +1.
            let cur = pan.current();
            let delta = (cur - prev).abs();
            assert!(delta <= 0.1 + 1e-6, "step delta {delta} exceeds bound");
            assert!(
                delta <= last_delta + 1e-6,
                "deltas should shrink: {delta} > {last_delta}"
            );
            assert!(cur > prev || (cur - 1.0).abs() < 1e-6, "must not regress");
            last_delta = delta;
            prev = cur;
        }
        assert!(
            (prev - 1.0).abs() < 1e-3,
            "smoother must converge to target, got {prev}"
        );
        let (gl, gr) = stereo_gains(prev);
        assert!(gl < 0.01 && gr > 0.99);
    }

    #[test]
    fn fold_mono_preserves_midpoint() {
        let centre = fold_mono(stereo_gains(0.0));
        let s = core::f32::consts::FRAC_1_SQRT_2;
        assert!((centre - s).abs() < 1e-6, "centre folds to {centre}");
        // Full-left / full-right both fold to half amplitude — still audible.
        assert!((fold_mono(stereo_gains(-1.0)) - 0.5).abs() < 1e-6);
        assert!((fold_mono(stereo_gains(1.0)) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn direction_rejects_non_finite_and_out_of_range_values() {
        assert_eq!(
            Direction::new(f32::NAN, 0.0),
            Err(SpatialError::InvalidDirection)
        );
        assert_eq!(
            Direction::new(181.0, 0.0),
            Err(SpatialError::InvalidDirection)
        );
        assert_eq!(
            Direction::new(0.0, 91.0),
            Err(SpatialError::InvalidDirection)
        );
    }

    #[test]
    fn height_speaker_carries_elevation_and_array_is_equal_power() {
        let layout = SpeakerLayout::new(vec![
            Direction::new(-45.0, 0.0).unwrap(),
            Direction::new(45.0, 0.0).unwrap(),
            Direction::new(0.0, 75.0).unwrap(),
            Direction::new(180.0, 0.0).unwrap(),
        ])
        .unwrap();
        let elevated = Direction::new(0.0, 75.0).unwrap();
        let gains = layout.gains(elevated).unwrap();
        assert!(gains[2] > gains[0] && gains[2] > gains[1]);
        assert!((gains.iter().map(|g| g * g).sum::<f32>() - 1.0).abs() < 1e-6);

        let mut frame = [0.0; 4];
        layout.render_sample(0.5, elevated, &mut frame).unwrap();
        assert!((frame[2] - 0.5 * gains[2]).abs() < 1e-6);
        assert_eq!(
            layout.render_sample(0.5, elevated, &mut [0.0; 2]),
            Err(SpatialError::ChannelCountMismatch)
        );
    }

    #[test]
    fn hrtf_renderer_convolves_each_ear_and_changes_with_elevation() {
        let front = Direction::new(0.0, 0.0).unwrap();
        let mut renderer = HrtfRenderer::new(TestHrtf, front).unwrap();
        let mut stereo = [0.0; 6];
        renderer.process(&[1.0, 0.0, 0.0], &mut stereo).unwrap();
        assert_eq!(stereo, [1.0, 0.5, 0.25, -0.25, 0.0, 0.0]);

        renderer
            .set_direction(Direction::new(0.0, 40.0).unwrap())
            .unwrap();
        let mut elevated = [0.0; 2];
        renderer.process(&[1.0], &mut elevated).unwrap();
        // The host profile returns different ear responses for elevated
        // sources; the result remains deterministic across direction changes.
        assert_eq!(elevated, [0.0, 1.0]);
        assert_eq!(
            renderer.process(&[1.0], &mut [0.0; 1]),
            Err(SpatialError::ChannelCountMismatch)
        );
    }

    #[test]
    fn measured_profile_parses_and_selects_nearest_direction() {
        let profile = MeasuredHrtfProfile::from_text(
            "GY-HRTF/1\nsample_rate_hz=48000\n# azimuth,elevation,left,right\n-90,0,1;0.5,0.25;0\n90,0,0.5,1;0.5\n",
        ).unwrap();
        assert_eq!(profile.sample_rate_hz(), 48_000);
        let response = profile
            .response(Direction::new(-80.0, 0.0).unwrap())
            .unwrap();
        assert_eq!(response.left, [1.0, 0.5]);
        assert_eq!(response.right, [0.25, 0.0]);
        // Equal-distance ties resolve to the first row, independent of hash order.
        let tie = profile.response(Direction::new(0.0, 0.0).unwrap()).unwrap();
        assert_eq!(tie.left, [1.0, 0.5]);
    }

    #[test]
    fn attributed_viking_fixture_loads_as_bounded_measured_profile() {
        let profile = MeasuredHrtfProfile::from_text(include_str!(
            "../fixtures/viking-v2-subj-Z-first-64.gy-hrtf"
        ))
        .unwrap();
        assert_eq!(profile.sample_rate_hz(), 48_000);
        assert_eq!(profile.responses.len(), 64);
        let front = profile
            .response(Direction::new(0.0, -45.0).unwrap())
            .unwrap();
        assert_eq!(front.left.len(), 128);
        assert_eq!(front.right.len(), 128);
        assert!(front.left.iter().all(|tap| tap.is_finite()));
    }

    #[test]
    fn measured_profile_rejects_malformed_and_unbounded_data() {
        assert!(matches!(
            MeasuredHrtfProfile::from_text("sample_rate_hz=48000"),
            Err(HrtfProfileError::MissingHeader)
        ));
        assert!(matches!(
            MeasuredHrtfProfile::from_text("GY-HRTF/1\n0,0,1,1"),
            Err(HrtfProfileError::MissingSampleRate)
        ));
        assert!(matches!(
            MeasuredHrtfProfile::from_text("GY-HRTF/1\nsample_rate_hz=48000\n0,0,1,1\n0,0,1,1"),
            Err(HrtfProfileError::DuplicateDirection)
        ));
        assert!(matches!(
            MeasuredHrtfProfile::from_text("GY-HRTF/1\nsample_rate_hz=48000\n0,0,NaN,1"),
            Err(HrtfProfileError::InvalidTaps)
        ));
        assert!(matches!(
            MeasuredHrtfProfile::from_text("GY-HRTF/1\nsample_rate_hz=48000\n0,0,17,1"),
            Err(HrtfProfileError::InvalidTaps)
        ));
    }
}
