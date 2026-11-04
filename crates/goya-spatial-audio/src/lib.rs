//! Goya spatial audio: stereo panning derived from goya-layout geometry.
//!
//! Contract: `GY-SPATIAL-AUDIO/1`.
//!
//! This crate maps millipoint [`goya_layout::Rect`]s onto a normalised pan
//! position `p ∈ [−1, +1]` (viewport-relative horizontal centre, clamped),
//! applies an equal-power stereo panning law, bounded distance attenuation
//! for z-layer depth, exponential trajectory smoothing to prevent clicks,
//! and a mono fold-down so information remains audible on mono outputs.

use goya_layout::Rect;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
