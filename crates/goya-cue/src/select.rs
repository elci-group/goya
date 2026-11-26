//! Never-worse paint selection.
//!
//! Indexed paint is used only when its cost is strictly below the direct
//! colour, the same rule GVI-L uses when `LSTY` is shorter than `STYL`.
//! A tie keeps the direct colour.

/// How a fill writes colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintForm {
    /// Colour bytes written in place. Kept when costs are equal.
    Direct,
    /// Palette plus a per-draw index. Chosen only when strictly cheaper.
    Indexed,
}

/// Indexed only when `indexed_cost` < `direct_cost`. Equal costs return [`PaintForm::Direct`].
pub fn select_paint(indexed_cost: u64, direct_cost: u64) -> PaintForm {
    if indexed_cost < direct_cost {
        PaintForm::Indexed
    } else {
        PaintForm::Direct
    }
}

/// Byte cost of writing the colour itself.
pub fn paint_cost_direct(colour_bytes: u64) -> u64 {
    colour_bytes
}

/// Palette bytes + per-draw index bytes + an explicit lookup penalty.
///
/// Saturating add.
pub fn paint_cost_indexed(palette_bytes: u64, index_bytes: u64, lookup_penalty: u64) -> u64 {
    palette_bytes
        .saturating_add(index_bytes)
        .saturating_add(lookup_penalty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_paint_equal_costs_keep_direct() {
        assert_eq!(select_paint(10, 10), PaintForm::Direct);
    }

    #[test]
    fn select_paint_strictly_cheaper_indexed() {
        assert_eq!(select_paint(9, 10), PaintForm::Indexed);
    }

    #[test]
    fn select_paint_dearer_indexed_keeps_direct() {
        assert_eq!(select_paint(11, 10), PaintForm::Direct);
    }

    #[test]
    fn select_direct_cost_is_the_colour_bytes() {
        assert_eq!(paint_cost_direct(16), 16);
    }

    #[test]
    fn select_indexed_cost_sums_palette_and_index() {
        assert_eq!(paint_cost_indexed(1024, 2, 0), 1026);
    }

    #[test]
    fn select_paint_penalty_that_ties_direct_keeps_direct() {
        // 8 + 2 would be indexed; the penalty brings the sum to the direct cost.
        let indexed = paint_cost_indexed(8, 2, 6);
        let direct = paint_cost_direct(16);
        assert_eq!(indexed, 16);
        assert_eq!(direct, 16);
        assert_eq!(select_paint(indexed, direct), PaintForm::Direct);
    }

    #[test]
    fn select_indexed_cost_saturates_at_u64_max() {
        assert_eq!(paint_cost_indexed(u64::MAX, 1, 0), u64::MAX);
        assert_eq!(paint_cost_indexed(u64::MAX, u64::MAX, u64::MAX), u64::MAX);
        assert_eq!(paint_cost_indexed(u64::MAX - 1, 2, 3), u64::MAX);
        assert_eq!(
            select_paint(
                paint_cost_indexed(u64::MAX, 1, 1),
                paint_cost_direct(u64::MAX)
            ),
            PaintForm::Direct
        );
    }
}
