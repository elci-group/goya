//! Inquisition verification classes for AOT-VC inventories
//! (ADR-AVC-001, mirroring the AVR-C01–C14 pattern of GOYA-GFX-AVR-001).
//!
//! Every emitted inventory is subject to every class. Classes are named
//! here so Diablos, the runtime and Inquisition share one vocabulary; the
//! enforcement harness itself is out of Phase-0 scope.

/// Verification class identifiers, `AVC-Cnn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationClass {
    /// AVC-C01 — Colour-inventory closure is enforced: compilation either
    /// closes under declared bounds or fails closed with an actionable
    /// diagnostic (F2, decision 2).
    ClosureEnforced,
    /// AVC-C02 — Every pixel-state, row, region and checkpoint record
    /// carries an explicit exactness class; trim-assisted records are
    /// never exact (F3, decision 3).
    PerRecordExactness,
    /// AVC-C03 — Shared trim boundaries agree structurally across
    /// adjacent pixels (F8 rule 1).
    SharedTrimBoundaries,
    /// AVC-C04 — The timeline is monotonic in media-clock ticks; equal
    /// ticks preserve insertion order (F5).
    MonotonicTimeline,
    /// AVC-C05 — Checkpoint digests are fixed at construction and
    /// payload-sensitive; certification is never inferred after the fact.
    CheckpointDigest,
    /// AVC-C06 — All timestamps are fixed-point ticks over a rational
    /// timescale; floating-point timestamps are rejected at the boundary
    /// (F5, decision 5).
    FixedPointClock,
    /// AVC-C07 — Audio is an opaque sidecar; the visual contract never
    /// decodes, transforms or emits audio samples (F4, decision 4).
    AudioSidecarOpacity,
    /// AVC-C08 — Temporal state ids never share a namespace with GVI
    /// variation states or AVR observation states (F7).
    NamespaceSeparation,
    /// AVC-C09 — No implicit budget defaults: compilation profiles state
    /// every budget explicitly (F9).
    NoImplicitDefaults,
    /// AVC-C10 — Timeline and instruction budgets are validated before
    /// any binary emission.
    BudgetValidation,
    /// AVC-C11 — Trim averaging occurs in the contractual linear working
    /// space; gamma-space averaging is a contract violation (F8, decision 6).
    LinearLightTrim,
    /// AVC-C12 — Rejected sources are routed to a conventional codec with
    /// the diagnostic attached; silent palette reduction never occurs (F2).
    FailClosedRouting,
    /// AVC-C13 — Deltas are applied only against matching content
    /// preconditions; a delta applied to unmatched row content is a
    /// contract violation, never a best-effort patch (Amendment 3).
    DeltaPrecondition,
}

impl VerificationClass {
    /// Stable machine-readable identifier (`escene`-style `class.CODE`).
    pub const fn id(&self) -> &'static str {
        match self {
            VerificationClass::ClosureEnforced => "AVC-C01",
            VerificationClass::PerRecordExactness => "AVC-C02",
            VerificationClass::SharedTrimBoundaries => "AVC-C03",
            VerificationClass::MonotonicTimeline => "AVC-C04",
            VerificationClass::CheckpointDigest => "AVC-C05",
            VerificationClass::FixedPointClock => "AVC-C06",
            VerificationClass::AudioSidecarOpacity => "AVC-C07",
            VerificationClass::NamespaceSeparation => "AVC-C08",
            VerificationClass::NoImplicitDefaults => "AVC-C09",
            VerificationClass::BudgetValidation => "AVC-C10",
            VerificationClass::LinearLightTrim => "AVC-C11",
            VerificationClass::FailClosedRouting => "AVC-C12",
            VerificationClass::DeltaPrecondition => "AVC-C13",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_ids_are_unique_and_ordered() {
        let all = [
            VerificationClass::ClosureEnforced,
            VerificationClass::PerRecordExactness,
            VerificationClass::SharedTrimBoundaries,
            VerificationClass::MonotonicTimeline,
            VerificationClass::CheckpointDigest,
            VerificationClass::FixedPointClock,
            VerificationClass::AudioSidecarOpacity,
            VerificationClass::NamespaceSeparation,
            VerificationClass::NoImplicitDefaults,
            VerificationClass::BudgetValidation,
            VerificationClass::LinearLightTrim,
            VerificationClass::FailClosedRouting,
            VerificationClass::DeltaPrecondition,
        ];
        let mut ids: Vec<&str> = all.iter().map(VerificationClass::id).collect();
        let len = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), len, "verification class ids must be unique");
        assert_eq!(all[0].id(), "AVC-C01");
        assert_eq!(all[len - 1].id(), "AVC-C13");
    }
}
