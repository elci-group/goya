//! Project the admitted scene onto the current frame.
//!
//! The result is geometry and text a host can hand to a surface. It does not
//! open one. Node order matches `goya_lang::layout`: depth-first, parents
//! before children.

use std::collections::BTreeMap;

use goya_lang::display_binding::SceneDisplayValue;
use goya_lang::{CellKind, DrawNode, SceneCell};

use crate::diagnostics::Diagnostic;

/// One scene element in layout order, with the text the host would show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedNode {
    pub id: Option<String>,
    pub name: String,
    pub role: &'static str,
    pub text: String,
    pub x_mp: i64,
    pub y_mp: i64,
    pub width_mp: i64,
    pub height_mp: i64,
    /// Ancestor scroll clip in millipoints, when a scroll viewport covers this node.
    pub clip: Option<(i64, i64, i64, i64)>,
    /// UTF-8 caret byte when this input is focused. Unfocused nodes are `None`.
    pub caret: Option<usize>,
}

pub(crate) fn nodes(
    root: &SceneCell,
    drawn: &[DrawNode],
    displays: &[SceneDisplayValue],
    fields: &BTreeMap<String, String>,
) -> Result<Vec<ProjectedNode>, Diagnostic> {
    let mut cells = Vec::new();
    flatten(root, &mut cells);
    if cells.len() != drawn.len() {
        return Err(Diagnostic::new(
            "GY-RESOLVER-PRESENT",
            format!(
                "layout has {} nodes and the scene has {}",
                drawn.len(),
                cells.len()
            ),
        ));
    }
    let mut rendered = BTreeMap::new();
    for display in displays {
        if let Some(id) = &display.element_id {
            rendered
                .entry(id.clone())
                .or_insert_with(|| display.rendered_text.clone());
        }
    }
    let mut projected = Vec::with_capacity(cells.len());
    for (cell, node) in cells.into_iter().zip(drawn.iter()) {
        let text = if cell.input {
            cell.id
                .as_ref()
                .and_then(|id| fields.get(id))
                .cloned()
                .or_else(|| node.text.clone())
                .unwrap_or_default()
        } else if let Some(shown) = cell.id.as_ref().and_then(|id| rendered.get(id)) {
            shown.clone()
        } else {
            node.text.clone().unwrap_or_default()
        };
        projected.push(ProjectedNode {
            id: cell.id,
            name: node.name.clone(),
            role: cell.role,
            text,
            x_mp: node.rect.x,
            y_mp: node.rect.y,
            width_mp: node.rect.width,
            height_mp: node.rect.height,
            clip: node
                .clip
                .map(|clip| (clip.x, clip.y, clip.width, clip.height)),
            caret: None,
        });
    }
    Ok(projected)
}

struct Meta {
    id: Option<String>,
    role: &'static str,
    input: bool,
}

fn flatten(cell: &SceneCell, out: &mut Vec<Meta>) {
    out.push(Meta {
        id: cell.authored_id.clone(),
        role: match cell.kind {
            CellKind::Button => "button",
            CellKind::Input => "input",
            CellKind::Cell => "cell",
        },
        input: cell.kind == CellKind::Input,
    });
    for child in &cell.children {
        flatten(child, out);
    }
}
