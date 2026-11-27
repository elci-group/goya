//! Absolute positioning (directive §9).
//!
//! An absolutely positioned box is removed from flow and placed against
//! its containing block (the padding box of the nearest positioned
//! ancestor). Offsets pair per axis: `left`+`right` (with `width: auto`)
//! stretch the box between the insets; a single offset positions that
//! edge and the opposite edge follows from the size; both `auto` falls
//! back to the static position, approximated as the containing block's
//! origin (progressive scope).

use super::{Display, LayoutState};
use crate::context::structural::NodeId;
use crate::geometry::Rect;

/// Place an absolutely positioned element and its subtree against
/// `containing` (already in viewport-rooted coordinates).
pub(super) fn place(state: &mut LayoutState, node: NodeId, containing: &Rect) {
    let node_box = state.node_box(node);
    if node_box.display == Display::None {
        return;
    }
    let [top, right, bottom, left] = node_box.offsets;
    let inset_horizontal = node_box.edges.margin.horizontal()
        + node_box.edges.padding.horizontal()
        + node_box.edges.border.horizontal();
    let inset_vertical = node_box.edges.margin.vertical()
        + node_box.edges.padding.vertical()
        + node_box.edges.border.vertical();
    // When both offsets on an axis pin the box and the size is auto, the
    // size is the stretched distance between them.
    let stretched_width = match (left, right, node_box.width) {
        (Some(l), Some(r), None) => Some((containing.width - l - r - inset_horizontal).max(0.0)),
        _ => None,
    };
    let stretched_height = match (top, bottom, node_box.height) {
        (Some(t), Some(b), None) => Some((containing.height - t - b - inset_vertical).max(0.0)),
        _ => None,
    };
    let available = stretched_width.unwrap_or(containing.width);
    let (mut width, mut height) = state.measure(node, available);
    if let Some(stretched) = stretched_width {
        width = stretched;
    }
    if let Some(stretched) = stretched_height {
        height = stretched;
    }
    let x = match (left, right) {
        (Some(l), _) => containing.x + l + node_box.edges.margin.left,
        (None, Some(r)) => {
            containing.x + containing.width - r - width - node_box.edges.margin.right
        }
        (None, None) => containing.x,
    };
    let y = match (top, bottom) {
        (Some(t), _) => containing.y + t + node_box.edges.margin.top,
        (None, Some(b)) => {
            containing.y + containing.height - b - height - node_box.edges.margin.bottom
        }
        (None, None) => containing.y,
    };
    state.place(
        node,
        Rect {
            x,
            y,
            width,
            height,
        },
        containing,
    );
}
