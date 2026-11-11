//! Scene AST and deterministic absolute/proportional layout.

use goya_determinism::EffectKind;
use std::collections::BTreeMap;

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
    /// `@input` — editable text field (placeholder in `text`).
    Input,
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
    /// True for `@input` cells (mirrors [`CellKind::Input`]).
    pub is_input: bool,
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
    /// `wrap: <positive integer columns>` — greedy word-wrap width applied
    /// at layout time. Absent = no wrap.
    pub wrap: Option<u32>,
    /// Enable a clipped, user-scrollable text viewport in interactive previews.
    pub scroll: bool,
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
pub enum InteractionClass {
    D1,
    D2,
}

/// Result type of a replayed static D2 effect request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectValueType {
    Int64,
    Text,
    Bool,
}

/// Static request bytes and their explicit declared capability label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRequestDecl {
    pub name: String,
    pub effect_kind: EffectKind,
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
    /// Structural parent; used by renderers that need clipping and overflow.
    pub parent: Option<String>,
    pub is_button: bool,
    /// Editable text field (`@input`); focus is host-side state.
    pub is_input: bool,
    pub rect: RectMp,
    /// Effective screen clip from scrollable ancestors after applying offsets.
    pub clip: Option<RectMp>,
    pub fill: Option<u32>,
    pub stroke: Option<u32>,
    pub stroke_width: Option<f64>,
    pub text: Option<String>,
    pub text_size: Option<f64>,
    pub anchor: Anchor,
    /// Whether the node's text viewport accepts scrolling.
    pub scroll: bool,
}

/// Layout output: draw order is depth-first (parents before children).
#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub nodes: Vec<DrawNode>,
    index: BTreeMap<String, usize>,
    /// Parent node index for each entry in `nodes`. Name lookup is not used
    /// for this walk: a descendant may reuse an ancestor's name.
    parent_index: Vec<Option<usize>>,
}

/// Per-scene scroll offsets in millipoints. Offsets are keyed by stable cell
/// name and are clamped to the content extent each time they are changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrollOffsets {
    offsets: BTreeMap<String, (i64, i64)>,
}

impl ScrollOffsets {
    pub fn get(&self, name: &str) -> (i64, i64) {
        self.offsets.get(name).copied().unwrap_or((0, 0))
    }

    /// Scroll a viewport by millipoints. Returns false when `name` does not
    /// identify a scrollable node; otherwise stores the clamped new offset.
    pub fn scroll_by(&mut self, frame: &Frame, name: &str, dx: i64, dy: i64) -> bool {
        if !frame.find(name).is_some_and(|node| node.scroll) {
            return false;
        }
        let (max_x, max_y) = frame.scroll_limits(name);
        let (x, y) = self.get(name);
        let x = x.saturating_add(dx).clamp(0, max_x);
        let y = y.saturating_add(dy).clamp(0, max_y);
        self.offsets.insert(name.to_owned(), (x, y));
        true
    }

    /// Re-clamp stored offsets after layout, resize, or scene replacement.
    /// Offsets for viewports removed from the frame are discarded.
    pub fn reconcile(&mut self, frame: &Frame) {
        self.offsets.retain(|name, offset| {
            let Some(node) = frame.find(name).filter(|node| node.scroll) else {
                return false;
            };
            let (max_x, max_y) = frame.scroll_limits(name);
            offset.0 = offset.0.clamp(0, max_x);
            offset.1 = offset.1.clamp(0, max_y);
            // Keep explicit zero offsets so callers can observe that the
            // viewport remains registered after its content shrinks.
            let _ = node;
            true
        });
    }
}

impl Frame {
    pub fn find(&self, name: &str) -> Option<&DrawNode> {
        self.index.get(name).map(|&i| &self.nodes[i])
    }

    /// Maximum horizontal and vertical offsets for a scroll viewport,
    /// measured from all descendants in the same scene coordinate space.
    pub fn scroll_limits(&self, name: &str) -> (i64, i64) {
        let Some(viewport_index) = self.index.get(name).copied() else {
            return (0, 0);
        };
        let viewport = &self.nodes[viewport_index];
        if !viewport.scroll {
            return (0, 0);
        }
        let mut content_right = viewport.rect.x.saturating_add(viewport.rect.width);
        let mut content_bottom = viewport.rect.y.saturating_add(viewport.rect.height);
        for (index, node) in self.nodes.iter().enumerate() {
            if self
                .ancestor_indexes(index)
                .iter()
                .any(|parent| *parent == viewport_index)
            {
                content_right = content_right.max(node.rect.x.saturating_add(node.rect.width));
                content_bottom = content_bottom.max(node.rect.y.saturating_add(node.rect.height));
            }
        }
        (
            content_right.saturating_sub(viewport.rect.x.saturating_add(viewport.rect.width)),
            content_bottom.saturating_sub(viewport.rect.y.saturating_add(viewport.rect.height)),
        )
    }

    fn ancestor_indexes(&self, mut index: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        while let Some(parent) = self.parent_index.get(index).copied().flatten() {
            if chain.contains(&parent) {
                break;
            }
            chain.push(parent);
            index = parent;
        }
        chain
    }

    /// Return draw nodes translated by all ancestor scroll offsets and
    /// clipped to the intersection of their scrollable ancestors.
    pub fn scrolled_nodes(&self, offsets: &ScrollOffsets) -> Vec<DrawNode> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let mut translated = node.clone();
                let mut shift_x = 0i64;
                let mut shift_y = 0i64;
                let mut clips: Vec<RectMp> = Vec::new();
                let mut chain = self.ancestor_indexes(index);
                chain.reverse();
                for ancestor_index in chain {
                    let ancestor = &self.nodes[ancestor_index];
                    let visual_rect = RectMp {
                        x: ancestor.rect.x - shift_x,
                        y: ancestor.rect.y - shift_y,
                        width: ancestor.rect.width,
                        height: ancestor.rect.height,
                    };
                    if ancestor.scroll {
                        clips.push(visual_rect);
                        let (dx, dy) = offsets.get(&ancestor.name);
                        shift_x = shift_x.saturating_add(dx);
                        shift_y = shift_y.saturating_add(dy);
                    }
                }
                translated.rect.x = translated.rect.x.saturating_sub(shift_x);
                translated.rect.y = translated.rect.y.saturating_sub(shift_y);
                translated.clip = clips.into_iter().reduce(intersect_rects);
                translated
            })
            .collect()
    }

    /// Find the topmost visible scroll viewport under a screen point.
    pub fn scroll_target_at(&self, offsets: &ScrollOffsets, x: i64, y: i64) -> Option<String> {
        self.scrolled_nodes(offsets)
            .into_iter()
            .rev()
            .find_map(|node| {
                (node.scroll
                    && contains_point(node.rect, x, y)
                    && node.clip.is_none_or(|clip| contains_point(clip, x, y)))
                .then_some(node.name)
            })
    }

    fn insert(&mut self, node: DrawNode, parent: Option<usize>) {
        self.index.insert(node.name.clone(), self.nodes.len());
        self.parent_index.push(parent);
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
    resolve_cell(&scene.root, vp, &mut frame, None, None, false);
    frame
}

fn resolve_cell(
    cell: &SceneCell,
    parent_rect: RectMp,
    frame: &mut Frame,
    parent_name: Option<&str>,
    parent_index: Option<usize>,
    allow_overflow: bool,
) {
    let rect = resolve_rect(cell, parent_rect, allow_overflow);
    // Wrap is a pure textual transform: greedy word wrap at the declared
    // column count, emitted as '\n'-joined text. The wave-2 goya-wayland
    // raster already splits '\n' text at glyph_px*1.4 line height (rect
    // clipped), so wrapped text reaches the screen with no renderer change.
    let text = match (&cell.wrap, &cell.text) {
        (Some(width), Some(t)) => Some(wrap_text(t, *width as usize).join("\n")),
        _ => cell.text.clone(),
    };
    let index = frame.nodes.len();
    frame.insert(
        DrawNode {
            name: cell.name.clone(),
            parent: parent_name.map(str::to_owned),
            is_button: cell.kind == CellKind::Button,
            is_input: cell.is_input,
            rect,
            clip: None,
            fill: cell.fill,
            stroke: cell.stroke,
            stroke_width: cell.stroke_width,
            text,
            text_size: cell.text_size,
            anchor: cell.anchor.unwrap_or(Anchor::Start),
            scroll: cell.scroll,
        },
        parent_index,
    );
    for child in &cell.children {
        resolve_cell(
            child,
            rect,
            frame,
            Some(&cell.name),
            Some(index),
            cell.scroll,
        );
    }
}

fn contains_point(rect: RectMp, x: i64, y: i64) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn intersect_rects(a: RectMp, b: RectMp) -> RectMp {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.x.saturating_add(a.width).min(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .min(b.y.saturating_add(b.height));
    RectMp {
        x,
        y,
        width: right.saturating_sub(x).max(0),
        height: bottom.saturating_sub(y).max(0),
    }
}

/// Greedy word wrap at `width` columns (same algorithm class as the
/// monitor's host-side `wrap_line`): words fill lines until they would
/// overflow; a token longer than `width` is hard-split. Multi-space runs
/// collapse to single separators between words on a line. Deterministic
/// and measurement-free.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    // Width 0 cannot come from the parser (it rejects 0), but stay total.
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    for line in text.split('\n') {
        let mut current = String::new();
        let mut current_len = 0usize;
        // Empty tokens between consecutive spaces would accumulate as
        // separators; skip them so runs collapse to single spaces.
        for word in line.split(' ').filter(|w| !w.is_empty()) {
            let mut word = word;
            loop {
                let word_len = word.graphemes(true).count();
                let sep = usize::from(!current.is_empty());
                if current_len + sep + word_len <= width {
                    if sep == 1 {
                        current.push(' ');
                    }
                    current.push_str(word);
                    current_len += sep + word_len;
                    break;
                }
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    current_len = 0;
                    continue;
                }
                // The word alone exceeds the width: hard-split it.
                let split_at = word
                    .grapheme_indices(true)
                    .nth(width)
                    .map(|(index, _)| index)
                    .unwrap_or(word.len());
                let head = &word[..split_at];
                word = &word[split_at..];
                out.push(head.to_string());
                if word.is_empty() {
                    break;
                }
            }
        }
        // Preserve authored line breaks exactly, including empty lines.
        out.push(current);
    }
    out
}

fn resolve_rect(cell: &SceneCell, parent: RectMp, allow_overflow: bool) -> RectMp {
    // Extent first: proportional fraction of parent, else absolute points,
    // else a sensible default (half the parent, never negative).
    let (w, h) = if let Some(e) = cell.extent_p {
        (
            scale_mp(e.x.clamp(0.0, 1.0) * parent.width as f64),
            scale_mp(e.y.clamp(0.0, 1.0) * parent.height as f64),
        )
    } else if let Some(e) = cell.extent {
        (scale_mp(e.x * MP), scale_mp(e.y * MP))
    } else {
        // Undeclared extent fills the parent — cells are surfaces by default.
        (parent.width, parent.height)
    };
    let (w, h) = (w.max(0), h.max(0));
    // Position: proportional fraction of parent extent, else absolute points
    // from the parent origin. Both clamp into the parent.
    let (x, y) = if let Some(p) = cell.position_p {
        (
            parent
                .x
                .saturating_add(scale_mp(p.x.clamp(0.0, 1.0) * parent.width as f64)),
            parent
                .y
                .saturating_add(scale_mp(p.y.clamp(0.0, 1.0) * parent.height as f64)),
        )
    } else if let Some(p) = cell.position {
        (
            parent.x.saturating_add(scale_mp(p.x * MP)),
            parent.y.saturating_add(scale_mp(p.y * MP)),
        )
    } else {
        (parent.x, parent.y)
    };
    let parent_right = parent.x.saturating_add(parent.width);
    let parent_bottom = parent.y.saturating_add(parent.height);
    // Clamp inside the parent (non-negative offsets only).
    let x = if allow_overflow {
        x.max(parent.x)
    } else {
        x.max(parent.x).min(parent_right)
    };
    let y = if allow_overflow {
        y.max(parent.y)
    } else {
        y.max(parent.y).min(parent_bottom)
    };
    RectMp {
        x,
        y,
        width: if allow_overflow {
            w
        } else {
            w.min(parent_right.saturating_sub(x)).max(0)
        },
        height: if allow_overflow {
            h
        } else {
            h.min(parent_bottom.saturating_sub(y)).max(0)
        },
    }
}

fn scale_mp(value: f64) -> i64 {
    if !value.is_finite() {
        return if value.is_sign_negative() {
            i64::MIN
        } else {
            i64::MAX
        };
    }
    // Out-of-range finite values saturate; this cast does not panic.
    value as i64
}

/// Topmost interactive element under a point (millipoints), searching
/// depth-first with later-drawn (deeper) nodes winning.
pub fn hit_test(frame: &Frame, x: i64, y: i64) -> Option<String> {
    hit_test_scrolled(frame, &ScrollOffsets::default(), x, y)
}

/// Hit test a scene after applying scroll offsets and ancestor clipping.
pub fn hit_test_scrolled(frame: &Frame, offsets: &ScrollOffsets, x: i64, y: i64) -> Option<String> {
    for node in frame.scrolled_nodes(offsets).iter().rev() {
        // Interactive: buttons activate; inputs are focus targets (host-side
        // focus state, Wave 2 composer).
        if (node.is_button || node.is_input)
            && contains_point(node.rect, x, y)
            && node.clip.is_none_or(|clip| contains_point(clip, x, y))
        {
            return Some(node.name.clone());
        }
    }
    None
}

#[cfg(test)]
mod scrolling_tests {
    use super::{layout, ScrollOffsets};
    use crate::parse;

    #[test]
    fn reconcile_clamps_stale_offsets_and_removes_missing_viewports() {
        let first = parse("@ontology a { @cell root { extent: (400, 300, 0) @cell viewport { extent: (100, 80, 0) scroll: true @cell content { position: (0, 200, 0) extent: (20, 20, 0) } } } }").unwrap();
        let first_frame = layout(&first, 400.0, 300.0);
        let mut offsets = ScrollOffsets::default();
        assert!(offsets.scroll_by(&first_frame, "viewport", 0, 500_000));
        assert_eq!(offsets.get("viewport"), (0, 140_000));

        let smaller = parse("@ontology a { @cell root { extent: (400, 300, 0) @cell viewport { extent: (100, 80, 0) scroll: true @cell content { position: (0, 100, 0) extent: (20, 20, 0) } } } }").unwrap();
        offsets.reconcile(&layout(&smaller, 400.0, 300.0));
        assert_eq!(offsets.get("viewport"), (0, 40_000));

        let removed = parse("@ontology a { @cell root { extent: (400, 300, 0) } }").unwrap();
        offsets.reconcile(&layout(&removed, 400.0, 300.0));
        assert_eq!(offsets.get("viewport"), (0, 0));
    }
}
