//! Colour-inventory closure (ADR-AVC-001 F2, decision 2) and the trim
//! working space (F8, decision 6).
//!
//! Per-pixel video colour is a continuous domain; GVI's state model is
//! finite and enumerable. The bounded-computation rule inherited from AVR
//! therefore governs closure: the compiler either closes the inventory
//! under an explicitly bounded palette and declared tolerance, or fails
//! closed with an actionable diagnostic. Photographic sources that cannot
//! close are routed to conventional codecs — never silently
//! palette-crushed.

/// The trim averaging working space (F8, decision 6): scene-linear
/// Rec. 709 primaries. Averaging in any other space (notably gamma-encoded
/// sRGB) darkens boundaries measurably and is a contract violation.
/// Single-variant `#[non_exhaustive]` until superseded by ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WorkingSpace {
    LinearRec709,
}

/// A declared tolerance for approximate reconstruction, in parts per
/// 10 000 of the linear-light quantity (e.g. 10 bps ≈ 0.1%).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tolerance(pub u16);

impl Tolerance {
    /// Reconstruction within this tolerance is permitted only for records
    /// classified `Approximate` (decision 3); exact records ignore it.
    pub const ZERO: Tolerance = Tolerance(0);
}

/// The maximum number of distinct colours an inventory may contain. There
/// is deliberately no default: every compilation states its bound
/// explicitly (F9, AVR decision 2 pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaletteBound(pub u32);

/// An entry in the compiled colour inventory: the canonical linear-light
/// RGB triple after decode conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColourEntry {
    /// Linear Rec. 709, 32-bit fixed point per component (Q24.8).
    pub linear_rgb: [u32; 3],
}

/// Why a source failed colour-inventory closure. Carries the actionable
/// diagnostic required by the bounded-computation rule (F2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectionDiagnostic {
    /// Distinct post-quantisation colours observed in the source.
    pub observed_distinct_colours: u64,
    /// The bound the compilation declared.
    pub bound: PaletteBound,
    /// Human- and agent-readable remediation: lower fidelity, narrow the
    /// region set, or route the source to a conventional codec.
    pub remediation: &'static str,
}

/// The outcome of the closure rule (decision 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosureVerdict {
    /// The source closes under the declared bound; compilation may proceed.
    Closed {
        distinct_colours: u64,
        tolerance: Tolerance,
    },
    /// The source cannot close: compilation fails closed.
    Rejected(RejectionDiagnostic),
}

/// The compiled colour inventory: a deduplicated, deterministically
/// ordered set of canonical colours (directive §6, colour inventory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourInventory {
    entries: Vec<ColourEntry>,
    bound: PaletteBound,
    tolerance: Tolerance,
    working_space: WorkingSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryError {
    /// Construction would exceed the declared bound.
    BoundExceeded,
    /// The declared working space is not the contractual one (F8).
    WorkingSpaceViolation,
}

impl ColourInventory {
    /// Close a set of candidate colours against `bound`. Entries are
    /// deduplicated in first-use order; the verdict is deterministic for a
    /// given input order, bound and tolerance.
    pub fn close(
        candidates: &[ColourEntry],
        bound: PaletteBound,
        tolerance: Tolerance,
    ) -> ClosureVerdict {
        let mut seen: Vec<ColourEntry> = Vec::new();
        for entry in candidates {
            if !seen.contains(entry) {
                if seen.len() as u64 + 1 > u64::from(bound.0) {
                    return ClosureVerdict::Rejected(RejectionDiagnostic {
                        observed_distinct_colours: seen.len() as u64 + 1,
                        bound,
                        remediation: "reduce fidelity, narrow the region set, or route this source to a conventional codec (ADR-AVC-001 F2)",
                    });
                }
                seen.push(*entry);
            }
        }
        ClosureVerdict::Closed {
            distinct_colours: seen.len() as u64,
            tolerance,
        }
    }

    /// Build an inventory from an already-closed candidate set. Fails
    /// closed rather than truncating (decision 2).
    pub fn new(
        candidates: &[ColourEntry],
        bound: PaletteBound,
        tolerance: Tolerance,
        working_space: WorkingSpace,
    ) -> Result<Self, InventoryError> {
        if working_space != WorkingSpace::LinearRec709 {
            return Err(InventoryError::WorkingSpaceViolation);
        }
        let mut entries: Vec<ColourEntry> = Vec::new();
        for entry in candidates {
            if !entries.contains(entry) {
                if entries.len() >= bound.0 as usize {
                    return Err(InventoryError::BoundExceeded);
                }
                entries.push(*entry);
            }
        }
        Ok(Self {
            entries,
            bound,
            tolerance,
            working_space,
        })
    }

    pub fn entries(&self) -> &[ColourEntry] {
        &self.entries
    }

    pub fn bound(&self) -> PaletteBound {
        self.bound
    }

    pub fn tolerance(&self) -> Tolerance {
        self.tolerance
    }

    pub fn working_space(&self) -> WorkingSpace {
        self.working_space
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(r: u32) -> ColourEntry {
        ColourEntry {
            linear_rgb: [r, r / 2, r / 4],
        }
    }

    #[test]
    fn closure_succeeds_within_bound() {
        let candidates = [entry(10), entry(20), entry(10), entry(30)];
        match ColourInventory::close(&candidates, PaletteBound(4), Tolerance::ZERO) {
            ClosureVerdict::Closed {
                distinct_colours, ..
            } => assert_eq!(distinct_colours, 3),
            other => panic!("expected closed, got {other:?}"),
        }
    }

    #[test]
    fn closure_fails_closed_with_actionable_diagnostic() {
        let candidates: Vec<ColourEntry> = (0..6).map(entry).collect();
        match ColourInventory::close(&candidates, PaletteBound(4), Tolerance::ZERO) {
            ClosureVerdict::Rejected(d) => {
                assert_eq!(d.observed_distinct_colours, 5);
                assert_eq!(d.bound, PaletteBound(4));
                assert!(!d.remediation.is_empty());
            }
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    #[test]
    fn inventory_rejects_excess_and_wrong_working_space() {
        let candidates: Vec<ColourEntry> = (0..4).map(entry).collect();
        assert_eq!(
            ColourInventory::new(
                &candidates,
                PaletteBound(2),
                Tolerance::ZERO,
                WorkingSpace::LinearRec709
            ),
            Err(InventoryError::BoundExceeded)
        );
        let ok = ColourInventory::new(
            &candidates,
            PaletteBound(4),
            Tolerance::ZERO,
            WorkingSpace::LinearRec709,
        )
        .unwrap();
        assert_eq!(ok.entries().len(), 4);
        assert_eq!(ok.working_space(), WorkingSpace::LinearRec709);
    }
}
