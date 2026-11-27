//! Delta compilation contract (directive §4 `APPLY_DELTA`; ADR-AVC-001
//! Amendment 3).
//!
//! A row delta is a precomputed content difference between two row
//! contents, aligned with GVI-D's bounded differential chains
//! (`goya-gvi/src/diff.rs`). The reference compiler emits depth-1 deltas
//! only: the precondition is a concrete row content id, never another
//! delta, so reconstruction never chains. A delta is applied by a runtime
//! only when the target row's current content matches the precondition;
//! applying a delta against unmatched content is a contract violation
//! (AVC-C13), not a best-effort patch.

use crate::pixel::ColourIndex;

/// Identifier of a row *content* in the deduplicated row inventory
/// (positional row addresses are transient; content identity is what
/// deltas and checkpoints are keyed on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentId(pub u32);

/// One changed pixel within a delta: an offset from the row start and
/// the content it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelChange {
    pub offset: u32,
    pub to: ColourIndex,
}

/// A precomputed row-content difference (directive §4). `changes` is in
/// ascending offset order, deduplicated by construction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RowDelta {
    pub id: u32,
    /// Precondition: the content the row must currently hold.
    pub from: ContentId,
    /// Resulting content after application.
    pub to: ContentId,
    pub changes: Vec<PixelChange>,
}

impl RowDelta {
    /// Build a delta from two row contents; returns `None` when the
    /// contents are equal (an empty delta must never be registered).
    pub fn between(
        id: u32,
        from: ContentId,
        to: ContentId,
        from_row: &[ColourIndex],
        to_row: &[ColourIndex],
    ) -> Option<Self> {
        if from == to || from_row.len() != to_row.len() {
            return None;
        }
        let changes: Vec<PixelChange> = from_row
            .iter()
            .zip(to_row.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(offset, (_, b))| PixelChange {
                offset: offset as u32,
                to: *b,
            })
            .collect();
        if changes.is_empty() {
            return None;
        }
        Some(Self {
            id,
            from,
            to,
            changes,
        })
    }

    /// Maximum reconstruction depth of this delta: depth-1 by
    /// construction in the reference compiler; a production compiler may
    /// chain deltas only within `crate::instructions::MAX_DELTA_DEPTH`.
    pub const fn depth(&self) -> u8 {
        1
    }

    /// Pixel writes the application performs.
    pub fn write_count(&self) -> u64 {
        self.changes.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[u16]) -> Vec<ColourIndex> {
        values.iter().map(|&v| ColourIndex(v)).collect()
    }

    #[test]
    fn deltas_describe_minimal_changes() {
        let a = row(&[0, 1, 1, 0, 0, 0]);
        let b = row(&[0, 1, 2, 2, 0, 0]);
        let delta = RowDelta::between(7, ContentId(0), ContentId(1), &a, &b).unwrap();
        assert_eq!(delta.id, 7);
        assert_eq!(delta.changes.len(), 2);
        assert_eq!(
            delta.changes[0],
            PixelChange {
                offset: 2,
                to: ColourIndex(2)
            }
        );
        assert_eq!(
            delta.changes[1],
            PixelChange {
                offset: 3,
                to: ColourIndex(2)
            }
        );
        assert_eq!(delta.depth(), 1);
        assert_eq!(delta.write_count(), 2);
    }

    #[test]
    fn equal_contents_never_yield_a_delta() {
        let a = row(&[0, 1, 2]);
        assert!(RowDelta::between(1, ContentId(0), ContentId(0), &a, &a).is_none());
    }

    #[test]
    fn length_mismatch_is_rejected() {
        let a = row(&[0, 1]);
        let b = row(&[0, 1, 2]);
        assert!(RowDelta::between(1, ContentId(0), ContentId(1), &a, &b).is_none());
    }
}
