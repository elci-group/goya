//! Table layout: rows of equal-width cells (directive §9).
//!
//! Progressive scope: automatic table layout — the column count is the
//! maximum cell count across rows, cells share the content width
//! equally, and rows stack in order. Anonymous cells (children of the
//! table without an intervening row) form one implicit row, and
//! anonymous rows (cells directly in the table) are wrapped implicitly
//! per CSS 2.1 tables.

use super::{LayoutState, NodeBox};
use crate::context::structural::NodeId;
use crate::geometry::{Px, Rect};

/// Lay out a `display: table` element's rows and cells. Returns the
/// table's flow content height. Cell boxes are recorded relative to the
/// table element like any other child.
pub(super) fn layout(
    state: &mut LayoutState,
    table_node: NodeId,
    table_box: &NodeBox,
    content_width: Px,
    record: bool,
    containing: &Rect,
) -> Px {
    let _ = table_box;
    let rows = state.in_flow_children(table_node);
    // Collect (row node, cells) pairs, wrapping direct cells into one
    // anonymous row.
    let mut structure: Vec<(Option<NodeId>, Vec<NodeId>)> = Vec::new();
    for row in rows {
        let row_display = state
            .styles
            .get(row)
            .and_then(|style| style.get("display"))
            .map(|display| display.to_ascii_lowercase());
        match row_display.as_deref() {
            Some("table-row") => {
                let cells: Vec<NodeId> = state
                    .in_flow_children(row)
                    .into_iter()
                    .filter(|cell| state.is_cell(cell))
                    .collect();
                structure.push((Some(row), cells));
            }
            _ => {
                // Cells directly in the table: implicit single row.
                if state.is_cell(&row) {
                    let implicit =
                        structure.len() == 1 && matches!(structure.last(), Some((None, _)));
                    if implicit {
                        if let Some((_, cells)) = structure.last_mut() {
                            cells.push(row);
                        }
                    } else {
                        structure.push((None, vec![row]));
                    }
                }
            }
        }
    }
    let column_count = structure
        .iter()
        .map(|(_, cells)| cells.len())
        .max()
        .unwrap_or(1)
        .max(1);
    let column_width = content_width / column_count as Px;
    let mut cursor = 0.0;
    for (_row_node, cells) in &structure {
        let mut row_height: Px = 0.0;
        for (index, cell) in cells.iter().enumerate() {
            let (width, height) = state.measure(*cell, column_width);
            row_height = row_height.max(height);
            if record {
                let rect = Rect {
                    x: column_width * index as Px,
                    y: cursor,
                    width,
                    height,
                };
                state.record_table_cell(table_node, *cell, rect, containing);
            }
        }
        cursor += row_height;
    }
    cursor
}

impl LayoutState<'_> {
    fn is_cell(&self, node: &NodeId) -> bool {
        self.styles
            .get(*node)
            .and_then(|style| style.get("display"))
            .map(|display| display.eq_ignore_ascii_case("table-cell"))
            // Unstyled children of a row behave as cells.
            .unwrap_or(true)
    }

    /// Record a cell (a child of a row child of the table) and lay out
    /// its subtree. Coordinates are table-content-local; the table's own
    /// content origin is recovered from its recorded box.
    fn record_table_cell(
        &mut self,
        table_node: NodeId,
        cell: NodeId,
        local: Rect,
        containing: &Rect,
    ) {
        let table_box = self.boxes.get(&table_node).copied().unwrap_or_default();
        let table_node_box = self.node_box(table_node);
        let origin_x =
            table_box.x + table_node_box.edges.border.left + table_node_box.edges.padding.left;
        let origin_y =
            table_box.y + table_node_box.edges.border.top + table_node_box.edges.padding.top;
        let rect = Rect {
            x: origin_x + local.x,
            y: origin_y + local.y,
            width: local.width,
            height: local.height,
        };
        self.boxes.insert(cell, rect);
        self.place(cell, rect, containing);
    }
}
