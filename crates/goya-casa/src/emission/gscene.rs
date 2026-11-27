//! Scene emission (directive §9): resolved layout + appearance → the
//! native `goya_lang::scene` model.
//!
//! Each laid-out element becomes a [`SceneCell`] positioned
//! proportionally against its parent's border box (fractions via
//! [`crate::geometry::proportional`]). Appearance maps to the scene's
//! paint fields: `background-color` → `fill`, an expressed border →
//! `stroke`/`stroke_width`. Progressive scope: text content is not yet
//! captured by the structural tree, so `text` is only populated for
//! `<input>` placeholders; no gradients, shadows, transforms or filters
//! are emitted yet (the typed appearance payloads exist; scene fields
//! for them do not).

use crate::appearance::borders::{self, BorderStyle};
use crate::appearance::colors::parse_color;
use crate::context::computed_style::ComputedStyles;
use crate::context::structural::{NodeId, SceneTree};
use crate::geometry::layout_systems::LayoutTree;
use crate::geometry::proportional::to_proportional;
use crate::geometry::{Context, Px, Rect};
use goya_lang::{CellKind, Scene, SceneCell, Vec3};

/// Emit a scene from the fully resolved pipeline. The root cell is a
/// synthetic container spanning the viewport; every laid-out element
/// hangs below it as proportional constraints.
pub fn emit_scene(
    tree: &SceneTree,
    styles: &ComputedStyles,
    layout: &LayoutTree,
    context: &Context,
    viewport: &Rect,
    name: &str,
) -> Scene {
    let children = tree
        .get(NodeId(0))
        .children
        .iter()
        .filter_map(|child| build_cell(tree, styles, layout, context, *child, viewport))
        .collect();
    Scene {
        name: name.to_string(),
        root: SceneCell {
            name: "root".to_string(),
            children,
            ..empty_cell()
        },
        interactions: Vec::new(),
        intents: Vec::new(),
        chromatics: None,
    }
}

/// Build one cell; `None` for elements the layout stage did not place
/// (`display: none`).
fn build_cell(
    tree: &SceneTree,
    styles: &ComputedStyles,
    layout: &LayoutTree,
    context: &Context,
    node: NodeId,
    parent: &Rect,
) -> Option<SceneCell> {
    let rect = layout.get(node)?;
    let tree_node = tree.get(node);
    let style = styles.get(node);
    // Scene fractions are relative to the parent's origin; recorded
    // boxes are viewport-rooted, so the parent origin is subtracted.
    let relative = Rect {
        x: rect.x - parent.x,
        y: rect.y - parent.y,
        width: rect.width,
        height: rect.height,
    };
    let proportional = to_proportional(&relative, parent);
    let kind = match tree_node.name.to_ascii_lowercase().as_str() {
        "button" => CellKind::Button,
        "input" => CellKind::Input,
        _ => CellKind::Cell,
    };
    let fill = style
        .and_then(|style| style.get("background-color"))
        .and_then(parse_color)
        .filter(|color| color.a > 0.0)
        .map(to_rgb);
    let border = style.and_then(|style| border_painted(style, context));
    let placeholder = if kind == CellKind::Input {
        tree_node.attributes.get("placeholder").cloned()
    } else {
        None
    };
    let children = tree_node
        .children
        .iter()
        .filter_map(|child| build_cell(tree, styles, layout, context, *child, rect))
        .collect();
    Some(SceneCell {
        name: tree_node.name.clone(),
        authored_id: tree_node.id.clone(),
        kind,
        is_input: kind == CellKind::Input,
        placeholder,
        position_p: Some(Vec3 {
            x: proportional.x,
            y: proportional.y,
            z: 0.0,
        }),
        extent_p: Some(Vec3 {
            x: proportional.width,
            y: proportional.height,
            z: 0.0,
        }),
        fill,
        stroke: border.as_ref().and_then(|border| border.color.map(to_rgb)),
        stroke_width: border.map(|border| border.width_px),
        text: None,
        children,
        ..empty_cell()
    })
}

/// A scene cell with every optional field unset.
fn empty_cell() -> SceneCell {
    SceneCell {
        name: String::new(),
        authored_id: None,
        kind: CellKind::Cell,
        is_input: false,
        placeholder: None,
        placeholder_phrasing: None,
        max_length: None,
        position: None,
        extent: None,
        position_p: None,
        extent_p: None,
        text_size_p: None,
        stroke_width_p: None,
        fill: None,
        stroke: None,
        stroke_width: None,
        text: None,
        text_phrasing: None,
        text_size: None,
        anchor: None,
        wrap: None,
        scroll: false,
        gestures: None,
        text_from: None,
        image_from: None,
        remote_digit: None,
        remote_colour: None,
        colour_group: None,
        sky: None,
        present_when: None,
        fill_to: None,
        fill_period_s: None,
        drift_p: None,
        drift_period_s: None,
        appear_s: None,
        appear_from_p: None,
        draw: None,
        iden: None,
        spatial: None,
        children: Vec::new(),
    }
}

/// 0-255 channels → 0xRRGGBB; alpha is dropped (the scene fill has no
/// alpha channel; transparent fills are omitted at the call site).
fn to_rgb(color: crate::appearance::colors::Color) -> u32 {
    ((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32
}

/// The painted border, if any side declares a visible style: the widest
/// side wins. Width keywords follow the CSS convention (thin 1px,
/// medium 3px, thick 5px) matching the layout driver; numeric widths
/// resolve through the geometry stage.
fn border_painted(
    style: &crate::context::computed_style::ComputedStyle,
    context: &Context,
) -> Option<PaintedBorder> {
    let mut widest: Option<(Px, Option<crate::appearance::colors::Color>)> = None;
    for side in ["top", "right", "bottom", "left"] {
        // Per-side longhands first, then the per-side shorthand
        // (`border-top: 2px solid red`), then the global shorthand.
        let declared = match style.get(&format!("border-{side}-style")) {
            Some(line_style) => {
                let lowered = line_style.to_ascii_lowercase();
                if lowered == "none" || lowered == "hidden" {
                    continue;
                }
                let width = style
                    .get(&format!("border-{side}-width"))
                    .map(|value| {
                        keyword_width(value)
                            .or_else(|| {
                                crate::geometry::dimensional::resolve_value(value, 0.0, context)
                            })
                            .unwrap_or(3.0)
                    })
                    .unwrap_or(3.0);
                let color = style
                    .get(&format!("border-{side}-color"))
                    .and_then(parse_color)
                    .or_else(|| style.get("border-color").and_then(parse_color));
                Some((width, color))
            }
            None => style
                .get(&format!("border-{side}"))
                .or_else(|| style.get("border"))
                .and_then(borders::parse)
                .filter(|border| border.style != BorderStyle::None)
                .map(|border| (border.width_px, border.color)),
        };
        let Some((width, color)) = declared else {
            continue;
        };
        match &widest {
            Some((current, _)) if *current >= width => {}
            _ => widest = Some((width, color)),
        }
    }
    widest.map(|(width_px, color)| PaintedBorder { width_px, color })
}

struct PaintedBorder {
    width_px: Px,
    color: Option<crate::appearance::colors::Color>,
}

fn keyword_width(value: &str) -> Option<Px> {
    match value.to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::layout_systems::LayoutTree;
    use std::collections::BTreeMap;

    fn context() -> Context {
        Context {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    #[test]
    fn layout_boxes_map_to_proportional_cells() {
        let mut tree = SceneTree::with_root("root");
        let child = tree.append(NodeId(0), "div");
        let mut layout = LayoutTree::default();
        layout.boxes.insert(
            child,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 100.0,
            },
        );
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let scene = emit_scene(
            &tree,
            &ComputedStyles::default(),
            &layout,
            &context(),
            &viewport,
            "test",
        );
        let cell = &scene.root.children[0];
        assert_eq!(cell.name, "div");
        assert_eq!(cell.kind, CellKind::Cell);
        let position = cell.position_p.unwrap();
        let extent = cell.extent_p.unwrap();
        assert_eq!((position.x, position.y), (0.0, 0.0));
        assert_eq!((extent.x, extent.y), (1.0, 100.0 / 600.0));
    }

    #[test]
    fn background_and_border_paint_the_cell() {
        let mut tree = SceneTree::with_root("root");
        let child = tree.append(NodeId(0), "div");
        let mut styles = ComputedStyles::default();
        let mut properties = BTreeMap::new();
        properties.insert("background-color".to_string(), "#ff8000".to_string());
        properties.insert("border-top-style".to_string(), "solid".to_string());
        properties.insert("border-top-width".to_string(), "2px".to_string());
        properties.insert("border-top-color".to_string(), "red".to_string());
        styles.per_node.insert(
            child,
            crate::context::computed_style::ComputedStyle {
                node: child,
                properties,
            },
        );
        let mut layout = LayoutTree::default();
        layout.boxes.insert(
            child,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 300.0,
            },
        );
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let scene = emit_scene(&tree, &styles, &layout, &context(), &viewport, "test");
        let cell = &scene.root.children[0];
        assert_eq!(cell.fill, Some(0xff8000));
        assert_eq!(cell.stroke, Some(0xff0000));
        assert_eq!(cell.stroke_width, Some(2.0));
    }

    #[test]
    fn unplaced_elements_are_skipped() {
        let mut tree = SceneTree::with_root("root");
        tree.append(NodeId(0), "div");
        let layout = LayoutTree::default();
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let scene = emit_scene(
            &tree,
            &ComputedStyles::default(),
            &layout,
            &context(),
            &viewport,
            "test",
        );
        assert!(scene.root.children.is_empty());
    }
}
