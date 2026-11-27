//! Block layout: normal flow, margins, auto-width fill and auto-margin
//! centering (directive §9).
//!
//! Children stack vertically inside the parent's content box. Margins do
//! not collapse (progressive scope); each child's vertical advance
//! includes its full top and bottom margins.

use super::FlowItem;
use crate::geometry::{Px, Rect};

/// Assign border-box rects in content-local coordinates. Returns the
/// rects and the flow extent (final bottom edge including the last
/// child's bottom margin).
pub(super) fn assign(items: Vec<FlowItem>, content: &Rect) -> (Vec<Rect>, Px) {
    let mut rects = Vec::with_capacity(items.len());
    let mut cursor = 0.0;
    for item in &items {
        // Horizontal: auto left+right centers a definite-width box; a
        // single auto side absorbs the remaining space.
        let x = match (item.margin_auto[3], item.margin_auto[1]) {
            (true, true) => ((content.width - item.width) / 2.0).max(0.0),
            (true, false) => (content.width - item.width - item.margin.right).max(0.0),
            (false, _) => item.margin.left,
        };
        let y = cursor + item.margin.top;
        rects.push(Rect {
            x,
            y,
            width: item.width,
            height: item.height,
        });
        cursor = y + item.height + item.margin.bottom;
    }
    let extent = items.last().map(|_| cursor).unwrap_or(0.0);
    (rects, extent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::structural::NodeId;
    use crate::geometry::Edges;

    fn item(width: Px, height: Px, top: Px, bottom: Px) -> FlowItem {
        FlowItem {
            node: NodeId(0),
            width,
            height,
            margin: Edges {
                top,
                right: 0.0,
                bottom,
                left: 0.0,
            },
            margin_auto: [false; 4],
            width_definite: true,
            height_definite: true,
            position: super::super::Position::Static,
            offsets: [None; 4],
        }
    }

    #[test]
    fn children_stack_with_margins() {
        let content = Rect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 0.0,
        };
        let (rects, extent) = assign(
            vec![item(400.0, 50.0, 10.0, 20.0), item(400.0, 30.0, 5.0, 0.0)],
            &content,
        );
        assert_eq!(rects[0].y, 10.0);
        assert_eq!(rects[1].y, 10.0 + 50.0 + 20.0 + 5.0);
        assert_eq!(extent, 115.0);
    }

    #[test]
    fn auto_side_margins_center_definite_widths() {
        let content = Rect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 0.0,
        };
        let mut centered = item(100.0, 10.0, 0.0, 0.0);
        centered.margin_auto = [false, true, false, true];
        let (rects, _) = assign(vec![centered], &content);
        assert_eq!(rects[0].x, 150.0);
    }
}
