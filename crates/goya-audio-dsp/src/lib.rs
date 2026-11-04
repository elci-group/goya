//! goya-audio-dsp — deterministic audio synthesis, mixing, and limiting.
//!
//! Contract: [`DSP_CONTRACT`] = `"GY-AUDIO-DSP/1"`.
//!
//! All functions are pure, allocation-free, and std-only. Every module is
//! covered by unit tests; output is fully deterministic for identical input.

/// Versioned contract identifier for this crate's DSP surface.
pub const DSP_CONTRACT: &str = "GY-AUDIO-DSP/1";

pub mod limiter;
pub mod mixer;
pub mod synthesis;

pub use limiter::{limit, Limiter};
pub use mixer::{Mixer, Voice};
pub use synthesis::Tap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_decays_to_near_zero_by_duration() {
        let tap = Tap::new(440.0, 48_000.0, 0.05);
        // Inside the envelope the signal is nontrivial near the start.
        assert!(tap.sample(0.001).abs() > 1e-3);
        // At and past the duration the tap is fully silent.
        assert_eq!(tap.sample(0.05), 0.0);
        assert_eq!(tap.sample(0.2), 0.0);
    }

    #[test]
    fn tap_total_energy_is_bounded() {
        let tap = Tap::new(880.0, 48_000.0, 0.05);
        let mut energy = 0.0f32;
        let n = 4_800;
        for i in 0..n {
            let s = tap.sample(i as f32 / 48_000.0);
            energy += s * s;
        }
        // Amplitude 0.8, decay 60/s. Sum of s² over samples ≈ energy ×
        // sample_rate; per-second energy ≈ 0.8²·0.5/decay ≈ 0.0053, so the
        // sample-sum stays well under 300 at 48 kHz.
        assert!(energy.is_finite());
        assert!(energy < 300.0, "energy {energy} exceeded sane bound");
    }

    #[test]
    fn mixer_sums_within_limit() {
        let mut mixer = Mixer::new(4);
        let mut voices: Vec<Voice> = (0..4)
            .map(|i| Voice {
                active: true,
                gain: 0.5,
                priority: 1,
                phase: i as f32 * 0.25,
                tap: Tap::new(440.0 + i as f32 * 110.0, 48_000.0, 1.0),
            })
            .collect();
        let out = mixer.mix(&mut voices);
        assert!(out.abs() <= 2.0 + 1e-6, "mixed sample {out} out of range");
    }

    #[test]
    fn allocation_evicts_lowest_priority_first() {
        let mut mixer = Mixer::new(2);
        let mut voices = vec![
            Voice {
                active: true,
                gain: 1.0,
                priority: 5,
                phase: 0.0,
                tap: Tap::new(440.0, 48_000.0, 1.0),
            },
            Voice {
                active: true,
                gain: 1.0,
                priority: 1,
                phase: 0.0,
                tap: Tap::new(440.0, 48_000.0, 1.0),
            },
        ];
        // Full: allocating priority 0 must evict the priority-1 voice.
        let idx = mixer
            .allocate(&mut voices, 0)
            .expect("allocate at capacity");
        assert_eq!(idx, 1);
        assert_eq!(voices[1].priority, 0);
        assert_eq!(voices[0].priority, 5);

        // Full again: priority 1 < 5, so voice 1 is evicted.
        let idx = mixer
            .allocate(&mut voices, 1)
            .expect("allocate at capacity");
        assert_eq!(idx, 1);
        assert_eq!(voices[1].priority, 1);

        // Now lowest is 1; requesting priority 0 still evicts it.
        let idx = mixer
            .allocate(&mut voices, 0)
            .expect("allocate at capacity");
        assert_eq!(idx, 1);
        assert_eq!(voices[1].priority, 0);

        // A higher priority than everything active is refused gracefully.
        assert!(mixer.allocate(&mut voices, 9).is_none());
    }

    #[test]
    fn limiter_output_never_exceeds_ceiling() {
        let mut l = Limiter::new();
        for i in -1000..=1000 {
            let x = i as f32 * 0.01;
            let y_hard = limit(x, 0.9);
            assert!(y_hard.abs() <= 0.9 + 1e-6);
            let y_smooth = l.push(x);
            assert!(y_smooth.abs() <= 0.9 + 1e-6, "envelope output {y_smooth}");
        }
    }

    #[test]
    fn limiter_silence_in_silence_out() {
        let mut l = Limiter::new();
        for _ in 0..512 {
            assert_eq!(l.push(0.0), 0.0);
        }
    }

    #[test]
    fn determinism_same_samples_twice_identical() {
        let run = || {
            let tap = Tap::new(440.0, 48_000.0, 0.05);
            let mut mixer = Mixer::new(2);
            let mut voices = vec![
                Voice {
                    active: true,
                    gain: 0.7,
                    priority: 2,
                    phase: 0.0,
                    tap,
                },
                Voice {
                    active: true,
                    gain: 0.3,
                    priority: 1,
                    phase: 0.5,
                    tap: Tap::new(660.0, 48_000.0, 0.05),
                },
            ];
            let mut l = Limiter::new();
            let mut out = Vec::new();
            for i in 0..2400 {
                voices.iter_mut().for_each(|v| v.phase += 0.001);
                let m = mixer.mix(&mut voices);
                out.push(l.push(m));
                let _ = i;
            }
            out
        };
        let a = run();
        let b = run();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.to_bits(), y.to_bits(), "nondeterministic sample");
        }
    }
}
