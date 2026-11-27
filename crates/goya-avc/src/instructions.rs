//! The visual instruction set (directive §4; ADR-AVC-001 F1, F7).
//!
//! The compiled timeline is not general-purpose machine code: it is a
//! constrained visual instruction set the compiler may choose the
//! cheapest equivalent representation from. Addresses name inventory
//! entries (rows, regions, deltas, checkpoints) rather than raw pixels,
//! so the same definition is referenced across thousands of frames
//! without duplication (directive §6).

use crate::pixel::{ColourIndex, Light, PixelState};

/// Addressable target of a mutation (directive §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Pixel {
        row: u32,
        pixel: u32,
    },
    Row {
        row: u32,
    },
    /// Rectangular region: rows `row..row+height`, pixels `pixel..pixel+width`.
    Region {
        row: u32,
        pixel: u32,
        height: u32,
        width: u32,
    },
}

/// Maximum reconstruction depth for a delta chain: `APPLY_DELTA` targets
/// must resolve within this depth, mirroring GVI's bounded differential
/// reconstruction (`goya-gvi/src/diff.rs`).
pub const MAX_DELTA_DEPTH: u8 = 16;

/// One visual mutation (directive §4). Instruction indices reference the
/// compiled inventories; no instruction embeds an unindexed colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    /// `SET_FC` — change fill colour.
    SetFill { target: Target, colour: ColourIndex },
    /// `SET_TC` — change one trim anchor (slot 0–5).
    SetTrim {
        target: Target,
        slot: u8,
        colour: ColourIndex,
    },
    /// `SET_L` — change light intensity only; chroma untouched.
    SetLight { target: Target, light: Light },
    /// `SET_RUN` — apply one state to consecutive pixels of a row.
    SetRun {
        row: u32,
        start: u32,
        len: u32,
        state: PixelState,
    },
    /// `SET_ROW` — apply a precompiled row from the row inventory.
    SetRow {
        target: Target,
        states: Vec<PixelState>,
    },
    /// `SET_REGION` — apply a state to a rectangular region.
    SetRegion { region: Target, state: PixelState },
    /// `COPY_REGION` — reuse an existing visual region at another position.
    CopyRegion { from: Target, to: Target },
    /// `APPLY_DELTA` — apply a precomputed state difference to one row;
    /// resolution depth is bounded by `MAX_DELTA_DEPTH` and the
    /// precondition (the row's current content) is checkable (AVC-C13).
    ApplyDelta { row: u32, delta_id: u32 },
    /// `LOAD_STATE` — restore a complete visual checkpoint.
    LoadState { checkpoint_id: u32 },
    /// `SYNC` — synchronise with the media clock.
    Sync,
    /// `PRESENT` — atomically present the completed visual state.
    Present,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstructionBudget {
    /// Maximum scheduled events in one timeline.
    pub max_events: usize,
    /// Maximum `APPLY_DELTA` applications (bounded reconstruction).
    pub max_delta_applications: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramError {
    EventBudgetExceeded { events: usize, budget: usize },
    DeltaBudgetExceeded { applications: usize, budget: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionRectError {
    ZeroExtent,
}

/// A validated rectangular region: extents must be positive (a
/// zero-height or zero-width region presents nothing and is rejected at
/// the contract boundary rather than silently ignored at playback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionRect {
    pub row: u32,
    pub pixel: u32,
    pub height: u32,
    pub width: u32,
}

impl RegionRect {
    pub fn new(row: u32, pixel: u32, height: u32, width: u32) -> Result<Self, RegionRectError> {
        if height == 0 || width == 0 {
            return Err(RegionRectError::ZeroExtent);
        }
        Ok(Self {
            row,
            pixel,
            height,
            width,
        })
    }

    pub fn as_target(&self) -> Target {
        Target::Region {
            row: self.row,
            pixel: self.pixel,
            height: self.height,
            width: self.width,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_depth_constant_is_bounded_like_gvi_diff() {
        // A depth of 16 mirrors GVI's bounded differential reconstruction;
        // it must never regress to unbounded.
        const _: () = {
            assert!(MAX_DELTA_DEPTH > 0 && MAX_DELTA_DEPTH < 64);
        };
    }

    #[test]
    fn zero_extent_regions_are_rejected() {
        assert_eq!(
            RegionRect::new(0, 0, 0, 10),
            Err(RegionRectError::ZeroExtent)
        );
        assert_eq!(
            RegionRect::new(0, 0, 10, 0),
            Err(RegionRectError::ZeroExtent)
        );
        assert!(RegionRect::new(1, 2, 3, 4).is_ok());
    }

    #[test]
    fn instructions_are_inventory_indexed() {
        let ev = Instruction::SetFill {
            target: Target::Pixel { row: 0, pixel: 0 },
            colour: ColourIndex(2),
        };
        assert!(matches!(
            ev,
            Instruction::SetFill {
                colour: ColourIndex(2),
                ..
            }
        ));
        let delta = Instruction::ApplyDelta {
            row: 3,
            delta_id: 7,
        };
        assert!(matches!(
            delta,
            Instruction::ApplyDelta {
                delta_id: 7,
                row: 3
            }
        ));
    }
}
