//! GY-LAYOUT/1 — Deterministic layout primitives.
//!
//! Contract: layout is a pure function of (module, constraints). Identical
//! inputs always produce identical frames — no time, locale, randomness or
//! environment dependence. Coordinates are logical points; relativistic
//! integer-based geometry (Goya directive §3) keeps results exactly
//! reproducible by storing sizes as millipoints.

use goya_ir::{Module, Node, NodeId};

/// Contract identifier implemented by this crate.
pub const LAYOUT_CONTRACT: &str = "GY-LAYOUT/1";

/// Sizes and positions in millipoints (1 point = 1000 mp) to avoid
/// float drift while remaining legible at API boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Position in millipoints.
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// Layout output: one rect per node, keyed by node id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    pub rects: Vec<(NodeId, Rect)>,
}

impl Frame {
    pub fn rect(&self, id: NodeId) -> Option<Rect> {
        self.rects.iter().find(|(n, _)| *n == id).map(|(_, r)| *r)
    }
}

/// Available space for a subtree.
#[derive(Debug, Clone, Copy)]
pub struct Constraints {
    pub max_width: i64,
    pub max_height: i64,
}

/// Padding applied inside each container, in millipoints.
pub const CONTAINER_PADDING: i64 = 8_000;
/// Spacing between container children, in millipoints.
pub const CHILD_SPACING: i64 = 4_000;
/// Height of a text/button leaf, in millipoints.
pub const LEAF_HEIGHT: i64 = 20_000;
/// Text ascent used to estimate width: 600 millipoints per glyph.
pub const GLYPH_WIDTH: i64 = 600;
/// Font ascent + descent, in millipoints.
pub const LINE_HEIGHT: i64 = LEAF_HEIGHT;

/// Compute a deterministic frame for `module` under `constraints`.
pub fn layout(module: &Module, constraints: Constraints) -> Frame {
    let mut frame = Frame::default();
    if let Some(root) = module.root {
        measure_and_place(module, root, constraints.max_width, 0, 0, &mut frame);
    }
    frame
}

/// Returns the subtree's size (width, height) in millipoints and places it at
/// (x, y) in `frame`.
fn measure_and_place(
    module: &Module,
    id: NodeId,
    max_width: i64,
    x: i64,
    y: i64,
    frame: &mut Frame,
) -> (i64, i64) {
    let Some(node) = module.node(id) else {
        return (0, 0);
    };
    match node {
        Node::Text { content }
        | Node::Button { label: content, .. }
        | Node::Input {
            placeholder: content,
        } => {
            let width = (content.chars().count() as i64) * GLYPH_WIDTH;
            place(frame, id, x, y, width.min(max_width), LEAF_HEIGHT);
            (width.min(max_width), LEAF_HEIGHT)
        }
        Node::Spacer { size } => {
            let mp = (*size * 1000.0) as i64;
            place(frame, id, x, y, 0, mp);
            (0, mp)
        }
        Node::Column { children } => {
            let inner_w = (max_width - 2 * CONTAINER_PADDING).max(0);
            let mut total_h = 2 * CONTAINER_PADDING;
            let mut max_child_w = 0;
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    total_h += CHILD_SPACING;
                }
                let (w, h) = measure_and_place(
                    module,
                    *child,
                    inner_w,
                    x + CONTAINER_PADDING,
                    y + total_h,
                    frame,
                );
                max_child_w = max_child_w.max(w);
                total_h += h;
            }
            let width = (max_child_w + 2 * CONTAINER_PADDING).min(max_width);
            place(frame, id, x, y, width, total_h);
            (width, total_h)
        }
        Node::Row { children } => {
            let mut cursor_x = CONTAINER_PADDING;
            let mut max_h = 0;
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    cursor_x += CHILD_SPACING;
                }
                let remaining = (max_width - cursor_x - CONTAINER_PADDING).max(0);
                // A spacer inside a row is a horizontal gap along the
                // parent's axis (direction-aware spacer semantics).
                if let Some(Node::Spacer { size }) = module.node(*child) {
                    let mp = (*size * 1000.0) as i64;
                    place(frame, *child, x + cursor_x, y + CONTAINER_PADDING, mp, 0);
                    cursor_x += mp;
                    continue;
                }
                let (w, h) = measure_and_place(
                    module,
                    *child,
                    remaining,
                    x + cursor_x,
                    y + CONTAINER_PADDING,
                    frame,
                );
                cursor_x += w;
                max_h = max_h.max(h);
            }
            let width = (cursor_x + CONTAINER_PADDING).min(max_width);
            let height = max_h + 2 * CONTAINER_PADDING;
            place(frame, id, x, y, width, height);
            (width, height)
        }
    }
}

fn place(frame: &mut Frame, id: NodeId, x: i64, y: i64, width: i64, height: i64) {
    frame.rects.push((
        id,
        Rect {
            x,
            y,
            width,
            height,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_ir::parse;

    const HELLO: &str = "column 1 2 3\ntext 2 Hello!\nbutton 3 1 Go\nroot 1";

    #[test]
    fn deterministic_across_runs() {
        let m = parse(HELLO).unwrap();
        let c = Constraints {
            max_width: 800_000,
            max_height: 600_000,
        };
        let a = layout(&m, c);
        let b = layout(&m, c);
        assert_eq!(a, b);
    }

    #[test]
    fn column_stacks_children_vertically() {
        let m = parse(HELLO).unwrap();
        let frame = layout(
            &m,
            Constraints {
                max_width: 800_000,
                max_height: 600_000,
            },
        );
        let text = frame.rect(NodeId(2)).unwrap();
        let button = frame.rect(NodeId(3)).unwrap();
        assert!(button.y > text.y, "button must sit below text");
        assert_eq!(text.x, button.x, "column children share x origin");
    }

    #[test]
    fn row_spacer_is_a_horizontal_gap() {
        let m = parse("row 1 2 3\nbutton 2 9 AB\nspacer 3 10.0\nroot 1").unwrap();
        let frame = layout(
            &m,
            Constraints {
                max_width: 800_000,
                max_height: 600_000,
            },
        );
        let before = frame.rect(NodeId(2)).unwrap();
        // A second button after the spacer starts beyond it, not adjacent.
        let m2 = parse("row 1 2 3 4\nbutton 2 9 AB\nspacer 3 10.0\nbutton 4 8 CD\nroot 1").unwrap();
        let frame2 = layout(
            &m2,
            Constraints {
                max_width: 800_000,
                max_height: 600_000,
            },
        );
        let b2 = frame2.rect(NodeId(4)).unwrap();
        assert!(
            b2.x >= before.x + 10_000,
            "spacer must push siblings along the row axis: {} vs {}",
            b2.x,
            before.x
        );
    }

    #[test]
    fn respects_max_width() {
        let m = parse("text 1 averyveryverylonglabel\nroot 1").unwrap();
        let frame = layout(
            &m,
            Constraints {
                max_width: 10_000,
                max_height: 600_000,
            },
        );
        let r = frame.rect(NodeId(1)).unwrap();
        assert!(r.width <= 10_000);
    }
}
