//! Flexbox: main-axis sizing and alignment, cross-axis alignment and
//! stretching, wrapping (directive §9).
//!
//! Progressive scope: single-pass flex resolution with measured bases.
//! `flex-basis: auto` uses the measured border-box size on the main
//! axis; percentage bases resolve against the container's main size.
//! `align-items: stretch` only stretches when the container's cross
//! size is definite.

use super::{FlowItem, LayoutState};
use crate::context::computed_style::ComputedStyle;
use crate::context::structural::NodeId;
use crate::geometry::{Px, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Stretch,
    Start,
    End,
    Center,
}

/// Container-level flex properties.
#[derive(Debug, Clone, Copy)]
pub(super) struct FlexProps {
    pub direction: Direction,
    pub wrap: bool,
    pub justify: Justify,
    pub align: Align,
    pub gap: Px,
    /// Whether the container's cross size is definite (authored height
    /// for rows, authored width for columns) — stretch only then applies.
    pub cross_definite: bool,
    /// The container's definite cross size, when known.
    pub cross_size: Option<Px>,
}

/// Per-item flex properties paired with the measured box.
#[derive(Debug, Clone, Copy)]
pub(super) struct FlexItem {
    pub item: FlowItem,
    pub grow: f64,
    pub shrink: f64,
    /// Resolved main-axis base size; `None` uses the measured size.
    pub basis: Option<Px>,
}

/// Parse container properties from the computed style.
pub(super) fn props(
    state: &mut LayoutState,
    node: NodeId,
    cross_definite: bool,
    cross_size: Option<Px>,
) -> FlexProps {
    let Some(style) = state.styles.get(node).cloned() else {
        return FlexProps {
            direction: Direction::Row,
            wrap: false,
            justify: Justify::Start,
            align: Align::Stretch,
            gap: 0.0,
            cross_definite,
            cross_size,
        };
    };
    let direction = match style.get("flex-direction").map(|d| d.to_ascii_lowercase()) {
        Some(ref d) if d == "column" || d == "column-reverse" => Direction::Column,
        _ => Direction::Row,
    };
    let justify = match style.get("justify-content").map(|j| j.to_ascii_lowercase()) {
        Some(ref j) if j == "flex-end" || j == "end" => Justify::End,
        Some(ref j) if j == "center" => Justify::Center,
        Some(ref j) if j == "space-between" => Justify::SpaceBetween,
        Some(ref j) if j == "space-around" => Justify::SpaceAround,
        Some(ref j) if j == "space-evenly" => Justify::SpaceEvenly,
        _ => Justify::Start,
    };
    let align = match style.get("align-items").map(|a| a.to_ascii_lowercase()) {
        Some(ref a) if a == "flex-start" || a == "start" => Align::Start,
        Some(ref a) if a == "flex-end" || a == "end" => Align::End,
        Some(ref a) if a == "center" => Align::Center,
        _ => Align::Stretch,
    };
    let wrap = style
        .get("flex-wrap")
        .is_some_and(|w| w.to_ascii_lowercase().contains("wrap"));
    // `gap` expands to row-gap column-gap; the main-axis gap for rows is
    // the column gap and vice versa.
    let gap_value = style
        .get("column-gap")
        .or_else(|| style.get("gap"))
        .map(str::to_string);
    let reference = match direction {
        Direction::Row => state.context.viewport_width,
        Direction::Column => state.context.viewport_width,
    };
    let gap = gap_value
        .and_then(|value| super::split_shorthand(&value).into_iter().next())
        .and_then(|component| state.length(&component, reference))
        .unwrap_or(0.0);
    FlexProps {
        direction,
        wrap,
        justify,
        align,
        gap,
        cross_definite,
        cross_size,
    }
}

/// Resolve per-item flex properties. The `flex` shorthand maps
/// `flex: <positive number>` to `1 1 0%`, `flex: none` to `0 0 auto`
/// and `flex: auto` to `1 1 auto` per CSS Flexbox §7.1.1.
pub(super) fn prepare(
    state: &mut LayoutState,
    items: Vec<FlowItem>,
    content_width: Px,
) -> Vec<FlexItem> {
    items
        .into_iter()
        .map(|item| {
            let style = state.styles.get(item.node).cloned();
            let (grow, shrink, basis) = flex_value(state, style.as_ref(), content_width);
            FlexItem {
                item,
                grow,
                shrink,
                basis,
            }
        })
        .collect()
}

fn flex_value(
    state: &mut LayoutState,
    style: Option<&ComputedStyle>,
    content_width: Px,
) -> (f64, f64, Option<Px>) {
    let Some(style) = style else {
        return (0.0, 1.0, None);
    };
    if let Some(shorthand) = style.get("flex").map(str::to_string) {
        let parts: Vec<&str> = shorthand.split_ascii_whitespace().collect();
        let first = parts.first().copied().unwrap_or("");
        if first.eq_ignore_ascii_case("none") {
            return (0.0, 0.0, None);
        }
        if first.eq_ignore_ascii_case("auto") {
            return (1.0, 1.0, None);
        }
        if let Ok(grow) = first.parse::<f64>() {
            let shrink = parts
                .get(1)
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(1.0);
            // A numeric grow implies basis 0%; an explicit basis
            // component resolves against the main size.
            let basis = parts
                .get(2)
                .and_then(|basis| resolve_basis(state, basis, content_width));
            return (grow, shrink, basis.or(Some(0.0)));
        }
    }
    let grow = style
        .get("flex-grow")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    let shrink = style
        .get("flex-shrink")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(1.0);
    let basis = style
        .get("flex-basis")
        .filter(|value| !value.eq_ignore_ascii_case("auto"))
        .and_then(|value| resolve_basis(state, value.trim(), content_width));
    (grow, shrink, basis)
}

fn resolve_basis(state: &mut LayoutState, basis: &str, content_width: Px) -> Option<Px> {
    if let Some(percent) = basis.strip_suffix('%') {
        return percent
            .trim()
            .parse::<f64>()
            .ok()
            .map(|p| p / 100.0 * content_width);
    }
    state.length(basis, content_width)
}

/// Assign border-box rects in content-local coordinates and return the
/// flow extent (the accumulated cross size of all lines).
pub(super) fn assign(items: Vec<FlexItem>, props: &FlexProps, content: &Rect) -> (Vec<Rect>, Px) {
    let row = props.direction == Direction::Row;
    let available_main = if row { content.width } else { content.height };
    let base = |item: &FlexItem| -> Px {
        match item.basis {
            Some(basis) => basis,
            None => {
                if row {
                    item.item.width
                } else {
                    item.item.height
                }
            }
        }
    };
    let total_base: Px =
        items.iter().map(base).sum::<Px>() + props.gap * items.len().saturating_sub(1) as Px;
    let free = available_main - total_base;
    let total_grow: f64 = items.iter().map(|i| i.grow).sum();
    let total_shrink_weight: f64 = items.iter().map(|i| i.shrink * base(i)).sum();
    // Final main sizes: grow distributes positive free space by weight;
    // shrink removes negative free space weighted by shrink × base.
    let mut mains = Vec::with_capacity(items.len());
    for item in &items {
        let mut main = base(item);
        if free > 0.0 && total_grow > 0.0 {
            main += free * (item.grow / total_grow);
        } else if free < 0.0 && total_shrink_weight > 0.0 {
            main += free * (item.shrink * base(item) / total_shrink_weight);
        }
        mains.push(main.max(0.0));
    }
    // Wrap into lines: a new line starts when wrapping is on and the
    // next item would overflow the main size.
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut line_main = 0.0;
    for (index, main) in mains.iter().enumerate() {
        let proposed = if lines.last().unwrap().is_empty() {
            *main
        } else {
            line_main + props.gap + main
        };
        if props.wrap && !lines.last().unwrap().is_empty() && proposed > available_main {
            lines.push(vec![index]);
            line_main = *main;
        } else {
            lines.last_mut().unwrap().push(index);
            line_main = proposed;
        }
    }
    let cross_of = |item: &FlexItem| -> Px {
        if row {
            item.item.height
        } else {
            item.item.width
        }
    };
    // Line cross sizes: the tallest item per line.
    let line_crosses: Vec<Px> = lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|index| cross_of(&items[*index]))
                .fold(0.0, f64::max)
        })
        .collect();
    let mut rects = vec![Rect::default(); items.len()];
    let mut line_y = 0.0;
    for (line_index, line) in lines.iter().enumerate() {
        let line_main_total: Px = line.iter().map(|index| mains[*index]).sum::<Px>()
            + props.gap * line.len().saturating_sub(1) as Px;
        let extra = (available_main - line_main_total).max(0.0);
        let between = match props.justify {
            Justify::SpaceBetween if line.len() > 1 => extra / (line.len() - 1) as Px,
            Justify::SpaceAround if !line.is_empty() => extra / line.len() as Px,
            Justify::SpaceEvenly if !line.is_empty() => extra / (line.len() + 1) as Px,
            _ => 0.0,
        };
        let lead = match props.justify {
            Justify::End => extra,
            Justify::Center => extra / 2.0,
            Justify::SpaceEvenly => between,
            Justify::SpaceAround => between / 2.0,
            _ => 0.0,
        };
        let mut main_cursor = lead;
        let line_cross = line_crosses[line_index];
        for index in line {
            let item = &items[*index];
            let margin_main = if row {
                item.item.margin.left + item.item.margin.right
            } else {
                item.item.margin.top + item.item.margin.bottom
            };
            // Cross size: stretch fills a definite container cross size.
            let mut cross = cross_of(item);
            let cross_definite_item = if row {
                item.item.height_definite
            } else {
                item.item.width_definite
            };
            if props.align == Align::Stretch
                && !cross_definite_item
                && props.cross_definite
                && !props.wrap
            {
                let margin_cross = if row {
                    item.item.margin.top + item.item.margin.bottom
                } else {
                    item.item.margin.left + item.item.margin.right
                };
                cross = (props.cross_size.unwrap_or(0.0) - margin_cross).max(0.0);
            }
            let cross_offset = match props.align {
                Align::Start | Align::Stretch => 0.0,
                Align::End => (line_cross - cross).max(0.0),
                Align::Center => ((line_cross - cross) / 2.0).max(0.0),
            };
            let (x, y, width, height) = if row {
                (
                    main_cursor + item.item.margin.left,
                    line_y + cross_offset + item.item.margin.top,
                    mains[*index] - margin_main,
                    cross,
                )
            } else {
                (
                    line_y + cross_offset + item.item.margin.left,
                    main_cursor + item.item.margin.top,
                    cross,
                    mains[*index] - margin_main,
                )
            };
            rects[*index] = Rect {
                x,
                y,
                width: width.max(0.0),
                height: height.max(0.0),
            };
            main_cursor += mains[*index] + between;
        }
        line_y += line_cross;
    }
    (rects, line_y)
}
