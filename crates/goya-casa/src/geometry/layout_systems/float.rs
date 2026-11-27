//! Float layout (directive §9).
//!
//! Progressive scope: floated boxes are removed from normal flow and
//! stack horizontally from their float side at the container top; any
//! in-flow boxes are placed below the tallest float (no text wrapping
//! beside floats, no `clear`).

use super::FlowItem;
use crate::context::computed_style::ComputedStyle;
use crate::geometry::{Px, Rect};

/// The float side, straight from the computed style; `None` means the
/// element is not floated.
pub(super) fn side(style: Option<&ComputedStyle>) -> Option<FloatSide> {
    match style
        .and_then(|style| style.get("float"))?
        .to_ascii_lowercase()
        .as_str()
    {
        "left" => Some(FloatSide::Left),
        "right" => Some(FloatSide::Right),
        _ => None,
    }
}

/// Which edge an element floats to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatSide {
    Left,
    Right,
}

/// Assign border-box rects in content-local coordinates. `floated`
/// gives each item's float side (`None` = in flow; the driver only
/// selects this system when at least one item floats). Returns the
/// rects and the flow extent (the deepest box bottom).
pub(super) fn assign(
    items: Vec<FlowItem>,
    floated: &[Option<FloatSide>],
    content: &Rect,
) -> (Vec<Rect>, Px) {
    let mut rects = vec![Rect::default(); items.len()];
    let mut left_cursor = 0.0;
    let mut right_cursor = content.width;
    let mut deepest: Px = 0.0;
    for (index, item) in items.iter().enumerate() {
        let Some(side) = floated[index] else {
            continue;
        };
        let (x, y) = match side {
            FloatSide::Left => {
                let x = left_cursor + item.margin.left;
                left_cursor = x + item.width + item.margin.right;
                (x, item.margin.top)
            }
            FloatSide::Right => {
                right_cursor -= item.width + item.margin.right;
                (right_cursor.max(0.0), item.margin.top)
            }
        };
        rects[index] = Rect {
            x,
            y,
            width: item.width,
            height: item.height,
        };
        deepest = deepest.max(y + item.height);
    }
    // In-flow boxes start below all floats (documented simplification).
    let mut cursor = deepest;
    for (index, item) in items.iter().enumerate() {
        if floated[index].is_some() {
            continue;
        }
        let y = cursor + item.margin.top;
        rects[index] = Rect {
            x: item.margin.left,
            y,
            width: item.width,
            height: item.height,
        };
        cursor = y + item.height + item.margin.bottom;
        deepest = deepest.max(cursor);
    }
    (rects, deepest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::structural::NodeId;
    use crate::geometry::Edges;

    fn item(width: Px, height: Px) -> FlowItem {
        FlowItem {
            node: NodeId(0),
            width,
            height,
            margin: Edges::default(),
            margin_auto: [false; 4],
            width_definite: true,
            height_definite: true,
            position: super::super::Position::Static,
            offsets: [None; 4],
        }
    }

    #[test]
    fn left_floats_stack_horizontally() {
        let content = Rect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 0.0,
        };
        let items = vec![item(100.0, 50.0), item(100.0, 30.0)];
        let floated = vec![Some(FloatSide::Left), Some(FloatSide::Left)];
        let (rects, extent) = assign(items, &floated, &content);
        assert_eq!(rects[0].x, 0.0);
        assert_eq!(rects[1].x, 100.0);
        assert_eq!(extent, 50.0);
    }

    #[test]
    fn right_floats_stack_from_the_far_edge() {
        let content = Rect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 0.0,
        };
        let items = vec![item(100.0, 50.0), item(50.0, 20.0)];
        let floated = vec![Some(FloatSide::Right), Some(FloatSide::Right)];
        let (rects, _) = assign(items, &floated, &content);
        assert_eq!(rects[0].x, 300.0);
        assert_eq!(rects[1].x, 250.0);
    }

    #[test]
    fn in_flow_boxes_start_below_floats() {
        let content = Rect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 0.0,
        };
        let items = vec![item(100.0, 50.0), item(400.0, 20.0)];
        let floated = vec![Some(FloatSide::Left), None];
        let (rects, extent) = assign(items, &floated, &content);
        assert_eq!(rects[1].y, 50.0);
        assert_eq!(extent, 70.0);
    }
}
