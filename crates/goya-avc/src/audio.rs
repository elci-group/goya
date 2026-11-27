//! The opaque audio sidecar (ADR-AVC-001 F4, decision 4).
//!
//! Audio is outside the visual contract's competence: the media clock is
//! the only visual↔audio agreement, and the container's `AUDS` section
//! carries a **descriptor**, never samples (AVC-C07). The descriptor is
//! opaque — codec name and integer timestamps in the stream's own time
//! base; no floating-point timestamps enter even here (F5's lesson
//! applied fully).

/// The opaque audio sidecar descriptor (`AUDS`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioSidecar {
    /// Container codec name (e.g. `aac`, `opus`) — opaque.
    pub codec: String,
    /// Stream duration in `time_base` units.
    pub duration_ts: i64,
    /// Stream time base as a rational (`numerator / denominator`).
    pub time_base: (u32, u32),
    /// Presentation timestamp of the first sample, in `time_base` units
    /// (may be negative for edit lists).
    pub start_ts: i64,
}
