//! Trim topology (directive §7; ADR-AVC-001 F8).
//!
//! Three rules are normative contract types, not conventions:
//! 1. **Shared boundaries** — adjacent logical pixels reference the same
//!    trim definition where their boundaries coincide, enforced
//!    structurally at inventory construction.
//! 2. **Linear-light averaging** — trim values are averages computed in
//!    the declared linear working space (`colour::WorkingSpace`); the
//!    space itself is fixed by ADR-AVC-001 decision 6.
//! 3. **Optional residency** — trim storage is permitted only where a
//!    boundary is non-uniform; solid regions store no trims.

use crate::pixel::{PixelState, MAX_TRIMS};

/// Identifier of a shared trim definition (directive §7: adjacent pixels
/// reference the same definition, not two copies).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrimDefinitionId(pub u32);

/// The six-direction trim sampling topology (directive §2, illustrative
/// geometry pending Phase 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrimDirection {
    Left,
    Right,
    UpperLeft,
    UpperRight,
    LowerLeft,
    LowerRight,
}

pub const TRIM_DIRECTIONS: [TrimDirection; MAX_TRIMS] = [
    TrimDirection::Left,
    TrimDirection::Right,
    TrimDirection::UpperLeft,
    TrimDirection::UpperRight,
    TrimDirection::LowerLeft,
    TrimDirection::LowerRight,
];

impl TrimDirection {
    pub const fn index(&self) -> usize {
        match self {
            TrimDirection::Left => 0,
            TrimDirection::Right => 1,
            TrimDirection::UpperLeft => 2,
            TrimDirection::UpperRight => 3,
            TrimDirection::LowerLeft => 4,
            TrimDirection::LowerRight => 5,
        }
    }

    /// The direction whose boundary is shared with this one across a
    /// horizontal adjacency: my right boundary is your left boundary.
    pub const fn horizontal_opposite(&self) -> Option<TrimDirection> {
        match self {
            TrimDirection::Left => Some(TrimDirection::Right),
            TrimDirection::Right => Some(TrimDirection::Left),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyError {
    /// Pixels i and i+1 disagree about their shared vertical boundary.
    SharedBoundaryMismatch { pixel: usize },
    /// A trim was stored on a boundary that is uniform (residency rule).
    UnnecessaryTrimResidency { pixel: usize, direction: usize },
}

/// Enforce the shared-boundary rule across one compiled row: pixel `i`'s
/// right anchor must equal pixel `i+1`'s left anchor whenever both are
/// present (rule 1).
pub fn validate_row_shared_boundaries(row: &[PixelState]) -> Result<(), TopologyError> {
    for (i, pair) in row.windows(2).enumerate() {
        let right = pair[0].trims[TrimDirection::Right.index()];
        let next_left = pair[1].trims[TrimDirection::Left.index()];
        if right != next_left {
            return Err(TopologyError::SharedBoundaryMismatch { pixel: i });
        }
    }
    Ok(())
}

/// Enforce optional residency (rule 3): a pixel state carries no trim
/// anchors. Solid-pixel compilation paths use this contract so trim
/// storage cannot silently appear where it is not useful.
pub fn require_trimless(state: &PixelState, pixel: usize) -> Result<(), TopologyError> {
    for (d, trim) in state.trims.iter().enumerate() {
        if trim.is_some() {
            return Err(TopologyError::UnnecessaryTrimResidency {
                pixel,
                direction: d,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::ColourIndex;

    #[test]
    fn six_directions_cover_all_trim_slots() {
        for (i, d) in TRIM_DIRECTIONS.iter().enumerate() {
            assert_eq!(d.index(), i);
        }
    }

    #[test]
    fn shared_boundaries_are_enforced() {
        let mut a = PixelState::exact_fill(ColourIndex(1));
        let mut b = PixelState::exact_fill(ColourIndex(2));
        assert!(validate_row_shared_boundaries(&[a, b]).is_ok());

        // Both sides agree on the shared definition: valid.
        a.trims[TrimDirection::Right.index()] = Some(ColourIndex(9));
        b.trims[TrimDirection::Left.index()] = Some(ColourIndex(9));
        assert!(validate_row_shared_boundaries(&[a, b]).is_ok());

        // Disagreement on the shared boundary: structural rejection.
        b.trims[TrimDirection::Left.index()] = Some(ColourIndex(5));
        assert_eq!(
            validate_row_shared_boundaries(&[a, b]),
            Err(TopologyError::SharedBoundaryMismatch { pixel: 0 })
        );
    }

    #[test]
    fn solid_pixels_must_be_trimless() {
        let solid = PixelState::exact_fill(ColourIndex(1));
        assert!(require_trimless(&solid, 0).is_ok());
        let mut trimmed = solid;
        trimmed.trims[0] = Some(ColourIndex(4));
        assert_eq!(
            require_trimless(&trimmed, 3),
            Err(TopologyError::UnnecessaryTrimResidency {
                pixel: 3,
                direction: 0
            })
        );
    }
}
