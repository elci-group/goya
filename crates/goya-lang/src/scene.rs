//! Scene AST and deterministic absolute/proportional layout.

use std::collections::BTreeMap;
use goya_determinism::EffectKind;

/// Contract millipoint scale: 1 point = 1000 mp (matches GY-LAYOUT/1).
pub const MP: f64 = 1000.0;

/// 3D vector of f64 components.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Cell,
    Button,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// A scene element: RUGID-style cell or button.
#[derive(Debug, Clone)]
pub struct SceneCell {
    pub name: String,
    pub kind: CellKind,
    /// Absolute position in points, parent-origin-relative.
    pub position: Option<Vec3>,
    /// Absolute extent in points.
    pub extent: Option<Vec3>,
    /// Proportional position (fraction of parent extent).
    pub position_p: Option<Vec3>,
    /// Proportional extent (fraction of parent extent).
    pub extent_p: Option<Vec3>,
    /// Fill colour as 0xRRGGBB (or 0x00RRGGBB for #rgb expansion).
    pub fill: Option<u32>,
    pub stroke: Option<u32>,
    pub stroke_width: Option<f64>,
    pub text: Option<String>,
    pub text_size: Option<f64>,
    pub anchor: Option<Anchor>,
    pub children: Vec<SceneCell>,
}

/// A parsed scene document.
#[derive(Debug, Clone)]
pub struct Scene {
    pub name: String,
    pub root: SceneCell,
    /// Authored, bounded interaction programs. Visual layout remains separate.
    pub interactions: Vec<InteractionDecl>,
}

/// D1 scalar schema type for authored interaction contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionType {
    Int,
    Text,
    Bool,
}

/// Authored execution class for a bounded interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionClass { D1, D2 }

/// Result type of a replayed static D2 effect request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectValueType { Int64, Text, Bool }

/// Static request bytes and their explicit declared capability label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRequestDecl {
    pub name: String,
    pub capability: String,
    pub request_utf8: String,
    pub result_type: EffectValueType,
}

/// One explicitly declared contract key and its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedKey {
    pub name: String,
    pub value_type: InteractionType,
}

/// Literal available to the deterministic transition subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionValue {
    Int(i64),
    Text(String),
    Bool(bool),
}

/// Supported typed expression AST. Addition is checked integer addition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionExpr {
    Literal(InteractionValue),
    State(String),
    Input(String),
    Effect(String),
    Add(Box<InteractionExpr>, Box<InteractionExpr>),
}

/// One straight-line state assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAssignment {
    pub key: String,
    pub value: InteractionExpr,
}

/// A D1 transition explicitly bound to one button activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionDecl {
    pub name: String,
    pub trigger_button: String,
    pub class: InteractionClass,
    pub declared_effects: Vec<EffectKind>,
    pub requests: Vec<EffectRequestDecl>,
    pub state_reads: Vec<TypedKey>,
    pub state_writes: Vec<TypedKey>,
    pub inputs: Vec<TypedKey>,
    pub assignments: Vec<StateAssignment>,
}

impl Scene {
    /// Depth-first search for a cell by name (structural, pre-layout).
    pub fn find_node(&self, name: &str) -> Option<&SceneCell> {
        fn walk<'a>(cell: &'a SceneCell, name: &str) -> Option<&'a SceneCell> {
            if cell.name == name {
                return Some(cell);
            }
            cell.children.iter().find_map(|c| walk(c, name))
        }
        walk(&self.root, name)
    }
}

/// Millipoint rect (matches GY-LAYOUT/1 semantics).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectMp {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// One resolved, drawable node.
#[derive(Debug, Clone)]
pub struct DrawNode {
    pub name: String,
    pub is_button: bool,
    pub rect: RectMp,
    pub fill: Option<u32>,
    pub stroke: Option<u32>,
    pub stroke_width: Option<f64>,
    pub text: Option<String>,
    pub text_size: Option<f64>,
    pub anchor: Anchor,
}

/// Layout output: draw order is depth-first (parents before children).
#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub nodes: Vec<DrawNode>,
    index: BTreeMap<String, usize>,
}

impl Frame {
    pub fn find(&self, name: &str) -> Option<&DrawNode> {
        self.index.get(name).map(|&i| &self.nodes[i])
    }

    fn insert(&mut self, node: DrawNode) {
        self.index.insert(node.name.clone(), self.nodes.len());
        self.nodes.push(node);
    }
}

/// Resolve the scene against a viewport of `width_pt × height_pt` points.
/// The root cell's extent defaults to the full viewport; absolute child
/// geometry is parent-origin-relative; proportional geometry is a fraction
/// of the parent extent. Deterministic and total.
pub fn layout(scene: &Scene, width_pt: f64, height_pt: f64) -> Frame {
    let mut frame = Frame::default();
    let vp = RectMp {
        x: 0,
        y: 0,
        width: (width_pt * MP) as i64,
        height: (height_pt * MP) as i64,
    };
    resolve_cell(&scene.root, vp, &mut frame);
    frame
}

fn resolve_cell(cell: &SceneCell, parent: RectMp, frame: &mut Frame) {
    let rect = resolve_rect(cell, parent);
    frame.insert(DrawNode {
        name: cell.name.clone(),
        is_button: cell.kind == CellKind::Button,
        rect,
        fill: cell.fill,
        stroke: cell.stroke,
        stroke_width: cell.stroke_width,
        text: cell.text.clone(),
        text_size: cell.text_size,
        anchor: cell.anchor.unwrap_or(Anchor::Start),
    });
    for child in &cell.children {
        resolve_cell(child, rect, frame);
    }
}

fn resolve_rect(cell: &SceneCell, parent: RectMp) -> RectMp {
    // Extent first: proportional fraction of parent, else absolute points,
    // else a sensible default (half the parent, never negative).
    let (w, h) = if let Some(e) = cell.extent_p {
        (
            (e.x.clamp(0.0, 1.0) * parent.width as f64) as i64,
            (e.y.clamp(0.0, 1.0) * parent.height as f64) as i64,
        )
    } else if let Some(e) = cell.extent {
        ((e.x * MP) as i64, (e.y * MP) as i64)
    } else {
        // Undeclared extent fills the parent — cells are surfaces by default.
        (parent.width, parent.height)
    };
    let (w, h) = (w.max(0), h.max(0));
    // Position: proportional fraction of parent extent, else absolute points
    // from the parent origin. Both clamp into the parent.
    let (x, y) = if let Some(p) = cell.position_p {
        (
            parent.x + (p.x.clamp(0.0, 1.0) * parent.width as f64) as i64,
            parent.y + (p.y.clamp(0.0, 1.0) * parent.height as f64) as i64,
        )
    } else if let Some(p) = cell.position {
        (parent.x + (p.x * MP) as i64, parent.y + (p.y * MP) as i64)
    } else {
        (parent.x, parent.y)
    };
    // Clamp inside the parent (non-negative offsets only).
    let x = x.max(parent.x).min(parent.x + parent.width);
    let y = y.max(parent.y).min(parent.y + parent.height);
    RectMp {
        x,
        y,
        width: w.min(parent.x + parent.width - x).max(0),
        height: h.min(parent.y + parent.height - y).max(0),
    }
}

/// Topmost interactive element under a point (millipoints), searching
/// depth-first with later-drawn (deeper) nodes winning.
pub fn hit_test(frame: &Frame, x: i64, y: i64) -> Option<String> {
    for node in frame.nodes.iter().rev() {
        if node.is_button
            && x >= node.rect.x
            && x < node.rect.x + node.rect.width
            && y >= node.rect.y
            && y < node.rect.y + node.rect.height
        {
            return Some(node.name.clone());
        }
    }
    None
}
