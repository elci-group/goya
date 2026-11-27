//! Target compilation profiles (directive §9; ADR-AVC-001 F9).
//!
//! §9's numeric values are illustrative and MUST NOT become universal
//! defaults before the benchmarking programme produces evidence:
//! `CompilationProfile` therefore has no `Default` implementation and
//! every field must be set explicitly (AVR decision 2 pattern, AVC-C09).

use crate::clock::MediaClock;
use crate::colour::{PaletteBound, Tolerance};
use crate::instructions::MAX_DELTA_DEPTH;

/// Target optimisation priority (directive §9). Device mapping is via
/// Caprichios; this enum is the priority contract only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileKind {
    /// Minimal processing and memory.
    Embedded,
    /// Energy efficiency.
    Mobile,
    /// Predictable presentation, low CPU overhead.
    Television,
    /// Fidelity and high throughput.
    Desktop,
    /// Maximum storage efficiency.
    Archival,
    /// Random access and spatial manipulation.
    Interactive,
}

/// Explicit budgets for one compilation. No `Default`: an unnamed
/// configuration has no provenance and no benchmark evidence (F9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilationBudgets {
    /// Colour-inventory bound for the closure rule (F2, decision 2).
    pub palette_bound: PaletteBound,
    /// Declared tolerance for approximate records (decision 3).
    pub tolerance: Tolerance,
    /// Maximum checkpoint interval on the media clock: reconstruction
    /// depth and seek latency are bounded by this, never by time zero.
    pub max_checkpoint_interval: MediaClock,
    /// Maximum `APPLY_DELTA` chain depth; must not exceed
    /// `MAX_DELTA_DEPTH`.
    pub max_delta_depth: u8,
    /// Residency budget for expanded segments, in bytes.
    pub residency_budget_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileError {
    /// Delta depth exceeds the contractual maximum.
    DeltaDepthExceeded { declared: u8, maximum: u8 },
    /// The checkpoint interval is zero: a checkpoint every zero ticks is
    /// meaningless and would hide unbounded reconstruction.
    ZeroCheckpointInterval,
}

/// A fully explicit compilation request: profile kind plus budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilationProfile {
    pub kind: ProfileKind,
    pub budgets: CompilationBudgets,
}

impl CompilationProfile {
    /// Validate internal consistency before compilation runs.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.budgets.max_delta_depth > MAX_DELTA_DEPTH {
            return Err(ProfileError::DeltaDepthExceeded {
                declared: self.budgets.max_delta_depth,
                maximum: MAX_DELTA_DEPTH,
            });
        }
        if self.budgets.max_checkpoint_interval.ticks() == 0 {
            return Err(ProfileError::ZeroCheckpointInterval);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budgets(interval: MediaClock) -> CompilationBudgets {
        CompilationBudgets {
            palette_bound: PaletteBound(256),
            tolerance: Tolerance::ZERO,
            max_checkpoint_interval: interval,
            max_delta_depth: 8,
            residency_budget_bytes: 1 << 20,
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // clippy const-folds the include_str! scan; the check is meaningful when the source changes
    fn profiles_have_no_default_budgets() {
        // The absence is contractual (F9, AVC-C09). If a `Default` impl is
        // ever added to `CompilationBudgets` or `CompilationProfile`, this
        // source-scan test fails and the change must go through an ADR
        // amendment with benchmark evidence.
        let forbidden = ["impl", " Default", " for CompilationProfile"];
        let forbidden_budgets = ["impl", " Default", " for CompilationBudgets"];
        let source = include_str!("profile.rs");
        assert!(
            !source.contains(forbidden.concat().as_str()),
            "CompilationProfile must not gain a Default impl (AVC-C09)"
        );
        assert!(
            !source.contains(forbidden_budgets.concat().as_str()),
            "CompilationBudgets must not gain a Default impl (AVC-C09)"
        );
    }

    #[test]
    fn validation_rejects_overdepth_and_zero_intervals() {
        let ok = CompilationProfile {
            kind: ProfileKind::Embedded,
            budgets: budgets(MediaClock::new(30_000, 30_000).unwrap()),
        };
        assert_eq!(ok.validate(), Ok(()));

        let deep = CompilationProfile {
            kind: ProfileKind::Desktop,
            budgets: CompilationBudgets {
                max_delta_depth: MAX_DELTA_DEPTH + 1,
                ..ok.budgets
            },
        };
        assert_eq!(
            deep.validate(),
            Err(ProfileError::DeltaDepthExceeded {
                declared: MAX_DELTA_DEPTH + 1,
                maximum: MAX_DELTA_DEPTH,
            })
        );

        let zero = CompilationProfile {
            kind: ProfileKind::Television,
            budgets: budgets(MediaClock::new(0, 30_000).unwrap()),
        };
        assert_eq!(zero.validate(), Err(ProfileError::ZeroCheckpointInterval));
    }
}
