//! Deterministic tap/click synthesis.

use std::time::Duration;

/// A short deterministic click: an exponentially decaying sine burst.
///
/// `sample` is pure and allocation-free. The tap is exactly zero at and past
/// its [`duration`](Tap::duration), and total energy is bounded by the fixed
/// amplitude and decay constant.
#[derive(Clone, Debug, PartialEq)]
pub struct Tap {
    freq_hz: f32,
    sample_rate: f32,
    duration_s: f32,
    /// Amplitude at t = 0.
    amplitude: f32,
    /// Exponential decay constant (1/s).
    decay: f32,
}

impl Tap {
    /// Build a tap at `freq_hz` lasting `duration_s` at `sample_rate`.
    ///
    /// Non-finite or non-positive parameters are clamped to safe defaults so
    /// construction can never produce a misbehaving tap.
    pub fn new(freq_hz: f32, sample_rate: f32, duration_s: f32) -> Self {
        let freq_hz = if freq_hz.is_finite() && freq_hz > 0.0 {
            freq_hz
        } else {
            440.0
        };
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let duration_s = if duration_s.is_finite() && duration_s > 0.0 {
            duration_s
        } else {
            0.05
        };
        Self {
            freq_hz,
            sample_rate,
            duration_s,
            amplitude: 0.8,
            decay: 60.0,
        }
    }

    /// Pure, allocation-free sample at time `t` in seconds.
    ///
    /// Returns 0.0 for any `t` outside `[0, duration_s)`, and the decaying
    /// sine within the envelope.
    pub fn sample(&self, t: f32) -> f32 {
        if !(t.is_finite()) || t < 0.0 || t >= self.duration_s {
            return 0.0;
        }
        let env = self.amplitude * (-self.decay * t).exp();
        let arg = 2.0 * core::f32::consts::PI * self.freq_hz * t;
        env * arg.sin()
    }

    /// Duration of the tap.
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f32(self.duration_s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_outside_envelope() {
        let tap = Tap::new(1000.0, 48_000.0, 0.02);
        assert_eq!(tap.sample(-0.001), 0.0);
        assert_eq!(tap.sample(0.02), 0.0);
        assert_eq!(tap.sample(f32::NAN), 0.0);
    }

    #[test]
    fn monotone_peak_decay() {
        let tap = Tap::new(440.0, 48_000.0, 0.1);
        let peak_at = |t: f32| tap.sample(t).abs();
        // Envelope strictly decays; sample peaks early vs late reflect that.
        assert!(peak_at(0.001) > peak_at(0.01));
        assert!(peak_at(0.01) > peak_at(0.05));
    }
}
