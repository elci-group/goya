//! CSS Grid: explicit track sizing, line-based and auto placement,
//! fractional allocation (directive §9).
//!
//! Progressive scope: row-major auto-placement without backtracking;
//! `auto` tracks behave as `1fr`; only the first explicit grid is used
//! (no named lines or areas).

use super::{FlowItem, LayoutState};
use crate::context::structural::NodeId;
use crate::geometry::{Px, Rect};

/// One grid track: fixed pixels or a fraction of the free space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    Px(Px),
    Fr(f64),
}

/// Container-level grid properties.
#[derive(Debug, Clone)]
pub(super) struct GridProps {
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
    pub column_gap: Px,
    pub row_gap: Px,
}

/// Per-item placement in 1-based grid line numbers.
#[derive(Debug, Clone, Copy)]
pub(super) struct Placement {
    pub column_start: usize,
    pub column_span: usize,
    pub row_start: usize,
    pub row_span: usize,
}

/// Parse container properties. Track lists accept `px`-style lengths,
/// percentages (of the available main size) and `fr` fractions; `auto`
/// behaves as `1fr`.
pub(super) fn props(state: &mut LayoutState, node: NodeId, content_width: Px) -> GridProps {
    let Some(style) = state.styles.get(node).cloned() else {
        return GridProps {
            columns: Vec::new(),
            rows: Vec::new(),
            column_gap: 0.0,
            row_gap: 0.0,
        };
    };
    let columns = style
        .get("grid-template-columns")
        .map(|value| parse_tracks(state, value, content_width))
        .unwrap_or_default();
    let rows = style
        .get("grid-template-rows")
        .map(|value| parse_tracks(state, value, content_width))
        .unwrap_or_default();
    let gaps = style.get("gap").map(str::to_string);
    let gap_components = gaps
        .as_deref()
        .map(super::split_shorthand)
        .unwrap_or_default();
    let row_gap_source = style
        .get("row-gap")
        .map(str::to_string)
        .or_else(|| gap_components.first().cloned());
    let column_gap_source = style.get("column-gap").map(str::to_string).or_else(|| {
        gap_components
            .get(1)
            .cloned()
            .or_else(|| gap_components.first().cloned())
    });
    let row_gap = row_gap_source
        .and_then(|value| state.length(&value, content_width))
        .unwrap_or(0.0);
    let column_gap = column_gap_source
        .and_then(|value| state.length(&value, content_width))
        .unwrap_or(0.0);
    GridProps {
        columns,
        rows,
        column_gap,
        row_gap,
    }
}

fn parse_tracks(state: &mut LayoutState, value: &str, available: Px) -> Vec<Track> {
    super::split_shorthand(value)
        .iter()
        .filter_map(|component| {
            if let Some(fr) = component.strip_suffix("fr") {
                fr.trim().parse::<f64>().ok().map(Track::Fr)
            } else if component.eq_ignore_ascii_case("auto") {
                Some(Track::Fr(1.0))
            } else {
                state.length(component, available).map(Track::Px)
            }
        })
        .collect()
}

/// Parse per-item placements from each item's own `grid-column`/
/// `grid-row` (line numbers, `span N`, or `auto`); start 0 marks
/// auto-placement.
pub(super) fn placements(state: &mut LayoutState, items: &[FlowItem]) -> Vec<Placement> {
    items
        .iter()
        .map(|item| {
            let style = state.styles.get(item.node).cloned();
            let column = parse_axis(style.as_ref(), "grid-column");
            let row = parse_axis(style.as_ref(), "grid-row");
            Placement {
                column_start: column.0,
                column_span: column.1,
                row_start: row.0,
                row_span: row.1,
            }
        })
        .collect()
}

/// Parse one `grid-<axis>` value: `<start> / <end>`, `<line>`,
/// `span N`, or `auto`. Returns (start line, span); start 0 means
/// auto-placement.
fn parse_axis(
    style: Option<&crate::context::computed_style::ComputedStyle>,
    property: &str,
) -> (usize, usize) {
    let Some(value) = style.and_then(|style| style.get(property).map(str::to_string)) else {
        return (0, 1);
    };
    let parts: Vec<&str> = value.split('/').map(|part| part.trim()).collect();
    let start_source = parts.first().copied().unwrap_or("auto");
    let end_source = parts.get(1).copied().unwrap_or("auto");
    // Start: a line number, or `span N` (start auto-placed).
    if let Some(span) = start_source.strip_prefix("span") {
        let span = span.trim().parse::<usize>().unwrap_or(1).max(1);
        return (0, span);
    }
    let start = match start_source.parse::<usize>() {
        Ok(line) if line >= 1 => line,
        _ => return (0, 1), // auto or a named line (unsupported)
    };
    // End: a line number (span = end − start) or `span N`.
    let span = if let Some(span) = end_source.strip_prefix("span") {
        span.trim().parse::<usize>().unwrap_or(1).max(1)
    } else {
        match end_source.parse::<usize>() {
            Ok(end) if end > start => end - start,
            _ => 1,
        }
    };
    (start, span.max(1))
}

/// Assign border-box rects in content-local coordinates and return the
/// flow extent (total row heights including gaps).
pub(super) fn assign(
    items: Vec<FlowItem>,
    placements: Vec<Placement>,
    props: &GridProps,
    content: &Rect,
) -> (Vec<Rect>, Px) {
    // Resolve explicit column track sizes.
    let column_count = props
        .columns
        .len()
        .max(
            placements
                .iter()
                .map(|p| p.column_start + p.column_span - 1)
                .max()
                .unwrap_or(0),
        )
        .max(1);
    let row_count = props
        .rows
        .len()
        .max(
            placements
                .iter()
                .map(|p| p.row_start + p.row_span - 1)
                .max()
                .unwrap_or(0),
        )
        .max(1);
    let columns = resolve_tracks(
        &props.columns,
        content.width,
        props.column_gap,
        column_count,
    );
    // Row tracks: fr tracks need the row heights, which depend on item
    // heights; progressive scope sizes fr rows after collecting the max
    // item height per row.
    let mut row_heights: Vec<Px> = vec![0.0; row_count];
    // Auto-place items row-major, honouring explicit column starts.
    let mut occupied: Vec<Vec<bool>> = vec![vec![false; column_count]; row_count];
    let mut order: Vec<(usize, Placement)> = Vec::new();
    for (index, placement) in placements.iter().enumerate() {
        let mut placement = *placement;
        if placement.column_start == 0 {
            // Find the first row/column where `column_span` cells fit.
            'outer: for (row, row_slots) in occupied.iter().enumerate() {
                for column in 0..=(column_count - placement.column_span) {
                    let fits = (0..placement.column_span).all(|span| !row_slots[column + span]);
                    if fits {
                        placement.column_start = column + 1;
                        placement.row_start = row + 1;
                        break 'outer;
                    }
                }
            }
        }
        for span in 0..placement.column_span {
            for row_span in 0..placement.row_span {
                let row = (placement.row_start - 1) + row_span;
                let column = (placement.column_start - 1) + span;
                if row < row_count && column < column_count {
                    occupied[row][column] = true;
                }
            }
        }
        order.push((index, placement));
    }
    let mut rects = vec![Rect::default(); items.len()];
    // Row heights: explicit px rows fixed; others grow to the tallest
    // placed item.
    let track_starts = |sizes: &[Px], gap: Px| -> Vec<Px> {
        let mut starts = Vec::with_capacity(sizes.len());
        let mut cursor = 0.0;
        for size in sizes {
            starts.push(cursor);
            cursor += size + gap;
        }
        starts
    };
    let column_starts = track_starts(&columns, props.column_gap);
    for (index, placement) in &order {
        let item = &items[*index];
        let row = (placement.row_start - 1).min(row_count - 1);
        row_heights[row] = row_heights[row].max(item.height);
    }
    // Distribute fr rows against the content height when any item
    // landed there; otherwise rows keep their content heights.
    let total_row_content: Px =
        row_heights.iter().sum::<Px>() + props.row_gap * row_count.saturating_sub(1) as Px;
    if total_row_content < content.height {
        let fr_rows: Vec<usize> = props
            .rows
            .iter()
            .enumerate()
            .filter(|(_, track)| matches!(track, Track::Fr(f) if *f > 0.0))
            .map(|(index, _)| index)
            .collect();
        if !fr_rows.is_empty() && props.rows.len() == row_count {
            let extra = content.height - total_row_content;
            let total_fr: f64 = fr_rows
                .iter()
                .map(|index| match props.rows[*index] {
                    Track::Fr(f) => f,
                    Track::Px(_) => 0.0,
                })
                .sum();
            for index in fr_rows {
                if let Track::Fr(f) = props.rows[index] {
                    row_heights[index] += extra * (f / total_fr);
                }
            }
        }
    }
    let row_starts = track_starts(&row_heights, props.row_gap);
    for (index, placement) in &order {
        let column = (placement.column_start - 1).min(column_count - 1);
        let row = (placement.row_start - 1).min(row_count - 1);
        let width: Px = (0..placement.column_span)
            .filter_map(|span| columns.get(column + span).copied())
            .sum::<Px>()
            + props.column_gap * (placement.column_span - 1) as Px;
        let height: Px = (0..placement.row_span)
            .filter_map(|span| row_heights.get(row + span).copied())
            .sum::<Px>()
            + props.row_gap * (placement.row_span - 1) as Px;
        rects[*index] = Rect {
            x: column_starts.get(column).copied().unwrap_or(0.0),
            y: row_starts.get(row).copied().unwrap_or(0.0),
            width,
            height,
        };
    }
    let extent = row_heights.iter().sum::<Px>() + props.row_gap * row_count.saturating_sub(1) as Px;
    (rects, extent)
}

/// Resolve a track list to concrete pixel sizes: fixed tracks stay, fr
/// tracks share the space left after fixed tracks and gaps.
fn resolve_tracks(tracks: &[Track], available: Px, gap: Px, minimum_count: usize) -> Vec<Px> {
    let count = tracks.len().max(minimum_count);
    let mut sizes: Vec<Px> = Vec::with_capacity(count);
    for index in 0..count {
        sizes.push(match tracks.get(index) {
            Some(Track::Px(px)) => *px,
            Some(Track::Fr(_)) | None => 0.0, // resolved below
        });
    }
    let fixed_total: Px = sizes.iter().sum();
    let gaps = gap * count.saturating_sub(1) as Px;
    let free = (available - fixed_total - gaps).max(0.0);
    let total_fr: f64 = (0..count)
        .map(|index| match tracks.get(index) {
            Some(Track::Fr(f)) if *f > 0.0 => *f,
            Some(Track::Px(_)) => 0.0,
            _ => 1.0, // auto and missing tracks behave as 1fr
        })
        .sum();
    for (index, size) in sizes.iter_mut().enumerate() {
        if *size == 0.0 {
            let fraction = match tracks.get(index) {
                Some(Track::Fr(f)) if *f > 0.0 => *f,
                _ => 1.0,
            };
            *size = free * (fraction / total_fr);
        }
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr_tracks_share_free_space() {
        let tracks = resolve_tracks(
            &[Track::Px(100.0), Track::Fr(1.0), Track::Fr(3.0)],
            500.0,
            0.0,
            3,
        );
        assert_eq!(tracks[0], 100.0);
        assert_eq!(tracks[1], 100.0);
        assert_eq!(tracks[2], 300.0);
    }

    #[test]
    fn gaps_subtract_before_fr_allocation() {
        let tracks = resolve_tracks(&[Track::Fr(1.0), Track::Fr(1.0)], 300.0, 100.0, 2);
        assert_eq!(tracks[0], 100.0);
        assert_eq!(tracks[1], 100.0);
    }
}
