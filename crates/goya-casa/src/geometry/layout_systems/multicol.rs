//! Multi-column layout: `column-count` / `column-width` splitting
//! (directive §9).
//!
//! Progressive scope: children are distributed into equal-width columns
//! in source order (the first ceil(n/N) in the first column, and so
//! on); no column balancing by height, no column spans or rules.

use super::{FlowItem, LayoutState};
use crate::context::computed_style::ComputedStyle;
use crate::context::structural::NodeId;
use crate::geometry::{Px, Rect};

/// The resolved column count: `column-count` when authored and ≥ 2, or
/// the derived count from `column-width` (N = floor((W + gap) /
/// (width + gap)) per CSS Multicol), otherwise 1 (no multicol).
pub(super) fn column_count(state: &mut LayoutState, node: NodeId, content_width: Px) -> usize {
    let Some(style) = state.styles.get(node).cloned() else {
        return 1;
    };
    if let Some(count) = style
        .get("column-count")
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        return count.max(1);
    }
    let Some(width) = style.get("column-width") else {
        return 1;
    };
    let Some(width) = state.length(width, content_width) else {
        return 1;
    };
    if width <= 0.0 {
        return 1;
    }
    let gap = gap(state, &style);
    (((content_width + gap) / (width + gap)).floor() as usize).max(1)
}

/// The column gap: `column-gap`, `gap`, or the CSS default (`normal`,
/// approximated at the 16px root font size).
fn gap(state: &mut LayoutState, style: &ComputedStyle) -> Px {
    let value = style
        .get("column-gap")
        .or_else(|| style.get("gap"))
        .map(str::to_string);
    value
        .and_then(|value| super::split_shorthand(&value).into_iter().next())
        .and_then(|component| state.length(&component, 0.0))
        .unwrap_or(16.0)
}

/// Container-level multicol properties.
#[derive(Debug, Clone, Copy)]
pub(super) struct MultiColProps {
    pub columns: usize,
    pub gap: Px,
}

/// Parse the properties once the count is known.
pub(super) fn props(state: &mut LayoutState, node: NodeId, columns: usize) -> MultiColProps {
    let gap = match state.styles.get(node).cloned() {
        Some(style) => gap(state, &style),
        None => 16.0,
    };
    MultiColProps { columns, gap }
}

/// Assign border-box rects in content-local coordinates. Children are
/// split evenly across columns in source order; returns the rects and
/// the flow extent (the deepest column bottom).
pub(super) fn assign(
    items: Vec<FlowItem>,
    props: &MultiColProps,
    content: &Rect,
) -> (Vec<Rect>, Px) {
    let columns = props.columns.max(1);
    let gap = if columns > 1 && items.len() > columns {
        props.gap
    } else {
        0.0
    };
    let column_width = ((content.width - gap * (columns - 1) as Px) / columns as Px).max(0.0);
    let per_column = items.len().div_ceil(columns);
    let mut rects = vec![Rect::default(); items.len()];
    let mut deepest: Px = 0.0;
    for column in 0..columns {
        let x = (column_width + gap) * column as Px;
        let mut cursor = 0.0;
        for index in (column * per_column)..((column + 1) * per_column).min(items.len()) {
            let item = &items[index];
            let y = cursor + item.margin.top;
            rects[index] = Rect {
                x,
                y,
                width: column_width,
                height: item.height,
            };
            cursor = y + item.height + item.margin.bottom;
            deepest = deepest.max(cursor);
        }
    }
    (rects, deepest)
}
