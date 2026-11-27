//! Sticky positioning (directive §9).
//!
//! A sticky box participates in normal flow, then shifts by its `top`/
//! `left` offsets and is clamped so it never leaves its parent's border
//! box (progressive scope: no scroll-offset physics — the clamp models
//! the stuck state directly).

use crate::geometry::{Px, Rect};

/// Clamp a sticky box into its parent: the offsets shift the flow
/// position, and the shifted box is confined to the parent's border box.
pub(super) fn clamp(rect: &mut Rect, parent_box: &Rect, offsets: [Option<Px>; 4]) {
    let [top, _right, _bottom, left] = offsets;
    if let Some(top) = top {
        rect.y += top;
    }
    if let Some(left) = left {
        rect.x += left;
    }
    let max_x = (parent_box.x + parent_box.width - rect.width).max(parent_box.x);
    let max_y = (parent_box.y + parent_box.height - rect.height).max(parent_box.y);
    rect.x = rect.x.clamp(parent_box.x, max_x);
    rect.y = rect.y.clamp(parent_box.y, max_y);
}
