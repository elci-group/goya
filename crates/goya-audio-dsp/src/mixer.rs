//! Voice mixing with graceful overload handling.

use crate::synthesis::Tap;

/// One playable voice wrapping a [`Tap`].
#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    /// Whether the voice contributes to the mix.
    pub active: bool,
    /// Output gain applied to the tap sample.
    pub gain: f32,
    /// Playback position in seconds into the tap.
    pub phase: f32,
    /// Eviction priority: lower values are more urgent (Unix-nice style) and
    /// are evicted last.
    pub priority: u8,
    /// The synthesized source for this voice.
    pub tap: Tap,
}

impl Voice {
    /// Current sample of this voice (0.0 if inactive). Pure.
    pub fn sample(&self) -> f32 {
        if !self.active {
            return 0.0;
        }
        self.gain * self.tap.sample(self.phase)
    }
}

/// Sums active voices into a single sample.
///
/// When at [`Mixer::max_voices`] active voices, allocation evicts the voice
/// with the lowest priority first (ties broken by highest index) — graceful
/// overload rather than failure.
#[derive(Clone, Debug)]
pub struct Mixer {
    pub max_voices: usize,
}

impl Mixer {
    /// Create a mixer that allows at most `max_voices` active voices.
    /// Zero is clamped to one so the mixer is always usable.
    pub fn new(max_voices: usize) -> Self {
        Self {
            max_voices: max_voices.max(1),
        }
    }

    fn active_count(voices: &[Voice]) -> usize {
        voices.iter().filter(|v| v.active).count()
    }

    /// Sum all active voice samples into one output sample.
    ///
    /// Deterministic, allocation-free. The sum is hard-clamped to ±2.0 so a
    /// pathological number of loud voices cannot produce an unbounded value.
    pub fn mix(&mut self, voices: &mut [Voice]) -> f32 {
        let mut sum = 0.0f32;
        for v in voices.iter() {
            if !v.active {
                continue;
            }
            sum += v.sample();
        }
        sum.clamp(-2.0, 2.0)
    }

    /// Allocate a new voice slot at the given priority, returning its index.
    ///
    /// - If an inactive voice exists, it is reused.
    /// - Else if under `max_voices` active voices, a new voice is pushed.
    /// - Else (at capacity) the request is admitted only when its priority
    ///   strictly outranks the highest active priority; the least urgent
    ///   (lowest-numeric) active voice is then evicted, ties broken by
    ///   highest index for determinism. Otherwise returns `None`.
    pub fn allocate(&mut self, voices: &mut Vec<Voice>, priority: u8) -> Option<usize> {
        if let Some(idx) = voices.iter().position(|v| !v.active) {
            voices[idx] = Voice::fresh(priority);
            return Some(idx);
        }
        if Mixer::active_count(voices) < self.max_voices {
            voices.push(Voice::fresh(priority));
            return Some(voices.len() - 1);
        }
        // At capacity: admit only if we outrank the most urgent active voice.
        let highest = voices
            .iter()
            .filter(|v| v.active)
            .map(|v| v.priority)
            .max()?;
        if priority < highest {
            // Evict the least urgent active voice (lowest numeric priority).
            let (idx, _) = voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.active)
                .min_by_key(|(i, v)| (v.priority, std::cmp::Reverse(*i)))?;
            voices[idx] = Voice::fresh(priority);
            Some(idx)
        } else {
            None
        }
    }
}

impl Voice {
    fn fresh(priority: u8) -> Self {
        Self {
            active: true,
            gain: 1.0,
            phase: 0.0,
            priority,
            tap: Tap::new(440.0, 48_000.0, 0.05),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_voices_contribute_zero() {
        let mut voices = vec![Voice {
            active: false,
            gain: 1.0,
            phase: 0.0,
            priority: 1,
            tap: Tap::new(440.0, 48_000.0, 0.05),
        }];
        let mut mixer = Mixer::new(4);
        assert_eq!(mixer.mix(&mut voices), 0.0);
    }

    #[test]
    fn allocation_reuses_inactive_voice() {
        let mut mixer = Mixer::new(2);
        let mut voices = vec![Voice {
            active: false,
            gain: 1.0,
            phase: 0.0,
            priority: 9,
            tap: Tap::new(440.0, 48_000.0, 0.05),
        }];
        let idx = mixer.allocate(&mut voices, 3).expect("reuse inactive");
        assert_eq!(idx, 0);
        assert!(voices[0].active);
        assert_eq!(voices[0].priority, 3);
    }
}
