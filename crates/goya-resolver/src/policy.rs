//! Resource policy for one resolver.
//!
//! The performance profile is a label. This crate does not measure memory,
//! latency, or frame time, and it does not enforce the GRD-001 engineering
//! targets. Those targets are not results.

/// Host label for a future measured profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PerformanceProfile {
    Constrained,
    #[default]
    Standard,
    Performance,
}

impl PerformanceProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Constrained => "constrained",
            Self::Standard => "standard",
            Self::Performance => "performance",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "constrained" => Some(Self::Constrained),
            "standard" => Some(Self::Standard),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }
}

/// What happens when a dispatch would exceed [`ResourcePolicy::max_dispatches`].
/// The dispatch that crosses the budget is not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Exhaustion {
    /// Return a diagnostic and stay in the current phase.
    #[default]
    Refuse,
    /// Return a diagnostic and suspend the instance.
    Suspend,
    /// Return a diagnostic and terminate the instance.
    Terminate,
    /// Return a diagnostic and enter [`crate::Phase::Failed`].
    Fail,
}

/// Limits the resolver can enforce exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePolicy {
    /// Maximum UTF-8 source size accepted by [`crate::Resolver::load`].
    pub max_source_bytes: usize,
    /// Maximum number of action executions that enter the D1 program.
    /// Field-conversion failures do not consume this budget.
    pub max_dispatches: u64,
    pub on_dispatch_exhaustion: Exhaustion,
    pub performance: PerformanceProfile,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_source_bytes: 1024 * 1024,
            max_dispatches: 1_000_000,
            on_dispatch_exhaustion: Exhaustion::Refuse,
            performance: PerformanceProfile::Standard,
        }
    }
}
