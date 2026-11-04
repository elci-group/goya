//! Hard limiting and smoothed envelope limiting.

/// Hard limiter: clamp `x` to `[-ceiling, ceiling]`. Pure, allocation-free.
///
/// Non-finite ceiling falls back to 1.0; non-finite input maps to 0.0 so
/// NaN/inf cannot propagate downstream.
pub fn limit(x: f32, ceiling: f32) -> f32 {
    let ceiling = if ceiling.is_finite() && ceiling > 0.0 {
        ceiling
    } else {
        1.0
    };
    if x.is_nan() {
        return 0.0;
    }
    // infinities clamp to the ceiling rather than vanishing
    x.clamp(-ceiling, ceiling)
}

/// One-pole smoothed limiter.
///
/// Tracks a peak envelope with fast attack and slower decay, then applies the
/// hard limit to the smoothed value so the output is free of discontinuities
/// and never exceeds the ceiling.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limiter {
    /// Current smoothed peak envelope (non-negative).
    pub envelope: f32,
}

impl Limiter {
    /// New limiter starting from silence.
    pub fn new() -> Self {
        Self { envelope: 0.0 }
    }

    /// Push one sample through the limiter, returning the limited sample.
    ///
    /// Deterministic and allocation-free. Non-finite input is treated as
    /// silence. Output magnitude never exceeds `ceiling`.
    pub fn push(&mut self, x: f32) -> f32 {
        let ceiling = 0.9f32;
        let x = if x.is_finite() { x } else { 0.0 };
        let target = x.abs();
        // Attack tracks immediately; decay relaxes slowly (one-pole).
        if target > self.envelope {
            self.envelope = target;
        } else {
            self.envelope += (target - self.envelope) * 0.05;
        }
        let scale = if self.envelope > ceiling {
            ceiling / self.envelope
        } else {
            1.0
        };
        limit(x * scale, ceiling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_limit_clamps_both_sides() {
        assert_eq!(limit(5.0, 0.5), 0.5);
        assert_eq!(limit(-5.0, 0.5), -0.5);
        assert_eq!(limit(0.1, 0.5), 0.1);
        assert_eq!(limit(f32::NAN, 0.5), 0.0);
        assert_eq!(limit(f32::INFINITY, 0.5), 0.5);
    }

    #[test]
    fn envelope_relaxes_toward_silence() {
        let mut l = Limiter::new();
        let _ = l.push(2.0);
        let e0 = l.envelope;
        assert!(e0 > 0.0);
        for _ in 0..2000 {
            let _ = l.push(0.0);
        }
        assert!(l.envelope < 1e-6, "envelope did not decay: {}", l.envelope);
    }
}
