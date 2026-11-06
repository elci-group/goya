//! GY-RUNTIME/1 — Event dispatch and input→state→layout pipeline.
//!
//! Contract: the runtime consumes `Event`s, applies them to a `goya_state`
//! store via transactions, recomputes layout, and produces a render list.
//! All processing is synchronous and deterministic for Wave 1; concurrency
//! arrives in Wave 2 behind the same interfaces.

use goya_ir::{ActionId, Module, Node, NodeId};
use goya_layout::{layout, Constraints, Frame, Rect};
use goya_rpi2::ConsentRecord;
use goya_speculation::{Level2Prediction, Level2Predictor};
use goya_state::{Store, Value};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// A UTF-8 text editing buffer whose cursor and selection always stay on
/// extended grapheme-cluster boundaries. Platform key events, IME updates,
/// and accessibility edits should all be applied through this type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextBuffer {
    text: String,
    cursor: usize,
    anchor: Option<usize>,
    undo: Vec<TextSnapshot>,
    redo: Vec<TextSnapshot>,
}

#[cfg(test)]
mod text_and_scroll_tests {
    use super::{ScrollState, TextBuffer};

    #[test]
    fn text_buffer_edits_extended_graphemes_without_splitting_utf8() {
        let mut text = TextBuffer::new("e\u{301}👩‍💻");
        assert_eq!(text.grapheme_len(), 2);
        text.delete_backward();
        assert_eq!(text.text(), "e\u{301}");
        text.delete_backward();
        assert_eq!(text.text(), "");
    }

    #[test]
    fn text_buffer_selection_and_undo_redo_share_one_edit_path() {
        let mut text = TextBuffer::new("alpha beta");
        text.move_home(false);
        text.move_right(true);
        text.move_right(true);
        text.insert("XY");
        assert_eq!(text.text(), "XYpha beta");
        text.undo();
        assert_eq!(text.text(), "alpha beta");
        text.redo();
        assert_eq!(text.text(), "XYpha beta");
    }

    #[test]
    fn platform_selection_offsets_snap_to_extended_graphemes() {
        let mut text = TextBuffer::new("a👩‍💻e\u{301}z");
        text.set_selection(15, 14);
        assert_eq!(text.anchor_byte(), 15);
        assert_eq!(text.cursor_byte(), 12);
        assert_eq!(text.selection_bytes(), Some(12..15));

        text.set_selection(usize::MAX, usize::MAX);
        assert_eq!(text.cursor_byte(), text.text().len());
        assert_eq!(text.selection_bytes(), None);
    }

    #[test]
    fn scroll_state_clamps_geometry_and_rejects_non_finite_values() {
        let mut scroll = ScrollState::default();
        scroll.set_geometry(100.0, 500.0, 40.0, 100.0);
        scroll.scroll_to(1000.0, 1000.0);
        assert_eq!(scroll.offset(), (60.0, 400.0));
        scroll.scroll_by(f64::NAN, f64::INFINITY);
        assert_eq!(scroll.offset(), (60.0, 400.0));
        scroll.set_geometry(0.0, 20.0, 40.0, 100.0);
        assert_eq!(scroll.offset(), (0.0, 0.0));
    }
}

/// Shared two-axis scrolling state. Offsets are content-space units (pixels,
/// points, or lines); geometry changes clamp offsets so the viewport never
/// exposes space beyond the content.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    content_width: f64,
    content_height: f64,
    viewport_width: f64,
    viewport_height: f64,
    offset_x: f64,
    offset_y: f64,
}

impl ScrollState {
    pub fn set_geometry(
        &mut self,
        content_width: f64,
        content_height: f64,
        viewport_width: f64,
        viewport_height: f64,
    ) {
        self.content_width = finite_nonnegative(content_width);
        self.content_height = finite_nonnegative(content_height);
        self.viewport_width = finite_nonnegative(viewport_width);
        self.viewport_height = finite_nonnegative(viewport_height);
        self.clamp_offsets();
    }

    pub fn scroll_by(&mut self, dx: f64, dy: f64) {
        self.offset_x += finite_or_zero(dx);
        self.offset_y += finite_or_zero(dy);
        self.clamp_offsets();
    }

    pub fn scroll_to(&mut self, x: f64, y: f64) {
        self.offset_x = finite_or_zero(x);
        self.offset_y = finite_or_zero(y);
        self.clamp_offsets();
    }

    pub fn offset(&self) -> (f64, f64) {
        (self.offset_x, self.offset_y)
    }
    pub fn max_offset(&self) -> (f64, f64) {
        (
            (self.content_width - self.viewport_width).max(0.0),
            (self.content_height - self.viewport_height).max(0.0),
        )
    }

    fn clamp_offsets(&mut self) {
        let (max_x, max_y) = self.max_offset();
        self.offset_x = self.offset_x.clamp(0.0, max_x);
        self.offset_y = self.offset_y.clamp(0.0, max_y);
    }
}

fn finite_nonnegative(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextSnapshot {
    text: String,
    cursor: usize,
    anchor: Option<usize>,
}

impl TextBuffer {
    const HISTORY_LIMIT: usize = 100;

    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            anchor: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn cursor_byte(&self) -> usize {
        self.cursor
    }
    pub fn anchor_byte(&self) -> usize {
        self.anchor.unwrap_or(self.cursor)
    }
    pub fn cursor_grapheme(&self) -> usize {
        self.text[..self.cursor.min(self.text.len())]
            .graphemes(true)
            .count()
    }
    pub fn selection_bytes(&self) -> Option<std::ops::Range<usize>> {
        self.anchor
            .map(|anchor| anchor.min(self.cursor)..anchor.max(self.cursor))
    }
    pub fn grapheme_len(&self) -> usize {
        self.text.graphemes(true).count()
    }
    pub fn graphemes(&self) -> Vec<&str> {
        self.text.graphemes(true).collect()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.record_undo();
        self.text = text.into();
        self.cursor = self.text.len();
        self.anchor = None;
    }

    /// Set selection endpoints from platform-provided UTF-8 byte offsets.
    /// Invalid, out-of-range, and mid-grapheme positions are snapped safely.
    pub fn set_selection(&mut self, anchor_byte: usize, cursor_byte: usize) {
        let anchor = grapheme_boundary_at_or_before(&self.text, anchor_byte);
        let cursor = grapheme_boundary_at_or_before(&self.text, cursor_byte);
        self.cursor = cursor;
        self.anchor = (anchor != cursor).then_some(anchor);
    }

    fn replace_range(&mut self, range: std::ops::Range<usize>, inserted: &str) {
        if range.is_empty() && inserted.is_empty() {
            return;
        }
        self.record_undo();
        self.text.replace_range(range.clone(), inserted);
        self.cursor = range.start + inserted.len();
        self.anchor = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.len();
    }

    pub fn move_left(&mut self, extend: bool) {
        if !extend {
            if let Some(selection) = self.selection_bytes() {
                self.move_cursor(selection.start, false);
                return;
            }
        }
        let next = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|i| *i < self.cursor)
            .last()
            .unwrap_or(0);
        self.move_cursor(next, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend {
            if let Some(selection) = self.selection_bytes() {
                self.move_cursor(selection.end, false);
                return;
            }
        }
        let next = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i > self.cursor)
            .unwrap_or(self.text.len());
        self.move_cursor(next, extend);
    }

    pub fn move_home(&mut self, extend: bool) {
        self.move_cursor(0, extend);
    }
    pub fn move_end(&mut self, extend: bool) {
        self.move_cursor(self.text.len(), extend);
    }

    pub fn insert(&mut self, text: &str) {
        self.record_undo();
        if let Some(range) = self.selection_bytes() {
            self.text.replace_range(range.clone(), text);
            self.cursor = range.start + text.len();
        } else {
            self.text.insert_str(self.cursor, text);
            self.cursor += text.len();
        }
        self.anchor = None;
    }

    pub fn delete_backward(&mut self) {
        if let Some(range) = self.selection_bytes() {
            self.record_undo();
            self.text.replace_range(range.clone(), "");
            self.cursor = range.start;
            self.anchor = None;
        } else {
            let before = self.snapshot();
            let end = self.cursor;
            self.move_left(false);
            if self.cursor != end {
                self.record_snapshot(before);
                self.redo.clear();
                self.text.replace_range(self.cursor..end, "");
            }
        }
    }

    pub fn delete_forward(&mut self) {
        if let Some(range) = self.selection_bytes() {
            self.record_undo();
            self.text.replace_range(range.clone(), "");
            self.cursor = range.start;
            self.anchor = None;
        } else {
            let before = self.snapshot();
            let start = self.cursor;
            self.move_right(false);
            let end = self.cursor;
            if end != start {
                self.record_snapshot(before);
                self.redo.clear();
                self.text.replace_range(start..end, "");
                self.cursor = start;
            }
        }
    }

    /// Apply an IME delete-surrounding request expressed in UTF-8 bytes.
    /// Range endpoints are expanded to grapheme boundaries before deletion.
    pub fn delete_surrounding(&mut self, before_bytes: u32, after_bytes: u32) {
        let raw_start = self.cursor.saturating_sub(before_bytes as usize);
        let raw_end = self
            .cursor
            .saturating_add(after_bytes as usize)
            .min(self.text.len());
        let start = self
            .text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .take_while(|index| *index <= raw_start)
            .last()
            .unwrap_or(0);
        let end = self
            .text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .find(|index| *index >= raw_end)
            .unwrap_or(self.text.len());
        if start < end {
            self.record_undo();
            self.text.replace_range(start..end, "");
            self.cursor = start;
            self.anchor = None;
        }
    }

    pub fn undo(&mut self) {
        self.restore_history(true);
    }
    pub fn redo(&mut self) {
        self.restore_history(false);
    }

    fn move_cursor(&mut self, cursor: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = cursor.min(self.text.len());
    }

    fn snapshot(&self) -> TextSnapshot {
        TextSnapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    fn record_undo(&mut self) {
        self.record_snapshot(self.snapshot());
        self.redo.clear();
    }

    fn record_snapshot(&mut self, snapshot: TextSnapshot) {
        self.undo.push(snapshot);
        if self.undo.len() > Self::HISTORY_LIMIT {
            self.undo.remove(0);
        }
    }

    fn restore_history(&mut self, undo: bool) {
        let current = self.snapshot();
        let snapshot = if undo {
            self.undo.pop()
        } else {
            self.redo.pop()
        };
        if let Some(snapshot) = snapshot {
            if undo {
                self.redo.push(current);
            } else {
                self.undo.push(current);
            }
            self.text = snapshot.text;
            self.cursor = snapshot.cursor.min(self.text.len());
            self.anchor = snapshot.anchor;
        }
    }
}

fn grapheme_boundary_at_or_before(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .take_while(|index| *index <= byte)
        .last()
        .unwrap_or(0)
}

/// Contract identifier implemented by this crate.
pub const RUNTIME_CONTRACT: &str = "GY-RUNTIME/1";

/// Input events surfaced by the platform layer.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A hit on a node that maps to an action (e.g. button press).
    Activate(ActionId),
    /// Keyboard character input routed to the focused text sink.
    Character(char),
    /// Focus a declared editable input node; `None` clears input focus.
    FocusInput(Option<NodeId>),
    /// Replace the focused field's value (used by accessibility adapters).
    SetInputValue(NodeId, String),
    /// Set a focused input's selection using UTF-8 byte offsets. Offsets are
    /// clamped to extended-grapheme boundaries before they reach the buffer.
    SetInputSelection {
        node: NodeId,
        anchor_byte: usize,
        cursor_byte: usize,
    },
    /// Mark a composition span without moving or changing the selection.
    /// Endpoints are UTF-8 byte offsets and are snapped to grapheme boundaries.
    SetCompositionRegion {
        node: NodeId,
        start_byte: usize,
        end_byte: usize,
    },
    /// Atomically commit text and place the caret within the inserted text.
    CommitInputText {
        text: String,
        cursor_byte_in_text: usize,
    },
    /// Finalize any active IME preedit as committed text.
    FinishComposition,
    TextBackspace,
    TextDelete,
    TextMoveLeft {
        extend: bool,
    },
    TextMoveRight {
        extend: bool,
    },
    TextMoveHome {
        extend: bool,
    },
    TextMoveEnd {
        extend: bool,
    },
    TextSelectAll,
    TextUndo,
    TextRedo,
    /// A compositor text-input-v3 batch, delivered after its `done` event.
    ImeBatch {
        preedit: Option<(String, i32, i32)>,
        commit: Option<String>,
        delete: Option<(u32, u32)>,
    },
    /// Termination requested by the platform (window close, signal).
    Quit,
}

/// A resolved render primitive, positioned by layout.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Panel(Rect),
    Text(Rect, String),
    Button(Rect, String),
}

/// Focus decoration for an input primitive. Offsets are UTF-8 byte offsets
/// into the corresponding `Primitive::Text` value; hosts can translate this
/// platform-neutral description into native caret/selection drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDecoration {
    pub node_id: NodeId,
    pub primitive_index: usize,
    pub caret_byte_offset: Option<usize>,
    pub selection_byte_range: Option<(usize, usize)>,
    /// UTF-8 byte span that the active IME is composing, in displayed text.
    pub composition_byte_range: Option<(usize, usize)>,
}

/// Interactive scene target under a pointer, preserving draw order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    Action(ActionId),
    Input(NodeId),
}

/// Integrates the platform surface, state store and module into one driven
/// application runtime.
pub struct Runtime {
    module: Module,
    store: Store,
    constraints: Constraints,
    frame: Frame,
    revision_at_layout: Option<u64>,
    geometry_revision: Option<u64>,
    level2: Level2Predictor,
    text_inputs: HashMap<NodeId, TextFieldState>,
    focused_input: Option<NodeId>,
}

#[derive(Clone, Debug, Default)]
struct TextFieldState {
    buffer: TextBuffer,
    preedit: Option<(String, i32, i32)>,
    composition_region: Option<std::ops::Range<usize>>,
}

impl Runtime {
    pub fn new(module: Module, store: Store, constraints: Constraints) -> Self {
        let text_inputs = module
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                matches!(node, Node::Input { .. }).then_some((*id, TextFieldState::default()))
            })
            .collect();
        let mut rt = Runtime {
            module,
            store,
            constraints,
            frame: Frame::default(),
            revision_at_layout: None,
            geometry_revision: Some(0),
            level2: Level2Predictor::new(),
            text_inputs,
            focused_input: None,
        };
        rt.relayout();
        rt
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn focused_input(&self) -> Option<NodeId> {
        self.focused_input
    }

    pub fn input_text(&self, node: NodeId) -> Option<&str> {
        self.text_inputs.get(&node).map(|state| state.buffer.text())
    }

    /// UTF-8 byte cursor and selection anchor for compositor surrounding-text
    /// state. Both positions are maintained on extended-grapheme boundaries.
    pub fn input_cursor_byte(&self, node: NodeId) -> Option<usize> {
        self.text_inputs
            .get(&node)
            .map(|state| state.buffer.cursor_byte())
    }

    pub fn input_anchor_byte(&self, node: NodeId) -> Option<usize> {
        self.text_inputs
            .get(&node)
            .map(|state| state.buffer.anchor_byte())
    }

    pub fn focus_input(&mut self, node: Option<NodeId>) -> bool {
        if node.is_some_and(|id| !self.text_inputs.contains_key(&id)) {
            return false;
        }
        if self.focused_input != node {
            if let Some(previous) = self
                .focused_input
                .and_then(|id| self.text_inputs.get_mut(&id))
            {
                previous.preedit = None;
            }
        }
        self.focused_input = node;
        true
    }

    /// Explicitly enable Level-2 learning. It remains inactive unless this is
    /// called with a currently valid consent record.
    pub fn opt_in_level2(&mut self, consent: ConsentRecord, max_transitions: usize) {
        self.level2.opt_in(consent, max_transitions);
    }

    /// Withdraw Level-2 consent and erase retained interaction transitions.
    pub fn opt_out_level2(&mut self) {
        self.level2.opt_out();
    }

    /// Current geometry generation, or `None` if its monotonic counter was
    /// exhausted. No prediction is issued after exhaustion.
    pub fn geometry_revision(&self) -> Option<u64> {
        self.geometry_revision
    }

    /// Return the learned next-action prediction tied to this exact runtime
    /// state/layout, if consent is valid at `now_ns` and a transition exists.
    pub fn predict_level2_at(&self, now_ns: u64) -> Option<Level2Prediction> {
        let geometry_revision = self.geometry_revision?;
        self.level2
            .predict_next_at(now_ns, self.store.revision(), geometry_revision)
    }

    /// Revalidate a prediction immediately before speculative use. State,
    /// geometry, consent, and probability are all checked again.
    pub fn level2_prediction_is_current(&self, prediction: &Level2Prediction, now_ns: u64) -> bool {
        let Some(geometry_revision) = self.geometry_revision else {
            return false;
        };
        self.level2
            .is_current_at(prediction, now_ns, self.store.revision(), geometry_revision)
    }

    /// Find the topmost button under a point, in millipoints, and return its
    /// action. Hit testing is deterministic and layout-driven.
    pub fn hit_test(&self, x: i64, y: i64) -> Option<ActionId> {
        match self.hit_test_target(x, y)? {
            HitTarget::Action(action) => Some(action),
            HitTarget::Input(_) => None,
        }
    }

    pub fn hit_test_target(&self, x: i64, y: i64) -> Option<HitTarget> {
        for (id, node) in self.module.nodes.iter().rev() {
            let target = match node {
                Node::Button { on_activate, .. } => HitTarget::Action(*on_activate),
                Node::Input { .. } => HitTarget::Input(*id),
                _ => continue,
            };
            if let Some(rect) = self.frame.rect(*id) {
                if x >= rect.x
                    && x < rect.x + rect.width.max(1)
                    && y >= rect.y
                    && y < rect.y + rect.height.max(1)
                {
                    return Some(target);
                }
            }
        }
        None
    }

    pub fn hit_test_input(&self, x: i64, y: i64) -> Option<NodeId> {
        self.module.nodes.iter().rev().find_map(|(id, node)| {
            if !matches!(node, Node::Input { .. }) {
                return None;
            }
            let rect = self.frame.rect(*id)?;
            (x >= rect.x
                && x < rect.x + rect.width.max(1)
                && y >= rect.y
                && y < rect.y + rect.height.max(1))
            .then_some(*id)
        })
    }

    /// Process one event. Returns `true` if the application should continue.
    pub fn handle(&mut self, event: Event) -> bool {
        self.handle_inner(event, None)
    }

    /// Process an event with a caller-supplied monotonic timestamp. This is
    /// the only event path that records Level-2 training data.
    pub fn handle_at(&mut self, event: Event, now_ns: u64) -> bool {
        self.handle_inner(event, Some(now_ns))
    }

    fn handle_inner(&mut self, event: Event, now_ns: Option<u64>) -> bool {
        match event {
            Event::Quit => return false,
            Event::FocusInput(node) => {
                self.focus_input(node);
            }
            Event::SetInputValue(node, value) => {
                if let Some(state) = self.text_inputs.get_mut(&node) {
                    state.buffer.set_text(value);
                    state.preedit = None;
                    state.composition_region = None;
                }
            }
            Event::SetInputSelection {
                node,
                anchor_byte,
                cursor_byte,
            } => {
                if self.focused_input == Some(node) {
                    if let Some(state) = self.text_inputs.get_mut(&node) {
                        state.buffer.set_selection(anchor_byte, cursor_byte);
                    }
                }
            }
            Event::FinishComposition => self.edit_focused(|state| {
                finish_composition(state);
            }),
            Event::SetCompositionRegion {
                node,
                start_byte,
                end_byte,
            } => {
                if self.focused_input == Some(node) {
                    if let Some(state) = self.text_inputs.get_mut(&node) {
                        finish_composition(state);
                        let start = grapheme_boundary_at_or_before(&state.buffer.text, start_byte);
                        let end = grapheme_boundary_at_or_before(&state.buffer.text, end_byte);
                        let region = start.min(end)..start.max(end);
                        state.composition_region = (region.start < region.end).then_some(region);
                    }
                }
            }
            Event::CommitInputText {
                text,
                cursor_byte_in_text,
            } => self.edit_focused(|state| {
                let range = state
                    .composition_region
                    .take()
                    .or_else(|| state.buffer.selection_bytes())
                    .unwrap_or_else(|| state.buffer.cursor_byte()..state.buffer.cursor_byte());
                state.preedit = None;
                let inserted = text.clone();
                state.buffer.replace_range(range.clone(), &text);
                let cursor = range.start
                    + grapheme_boundary_at_or_before(&inserted, cursor_byte_in_text);
                state.buffer.set_selection(cursor, cursor);
            }),
            Event::TextBackspace => self.edit_focused(|state| state.buffer.delete_backward()),
            Event::TextDelete => self.edit_focused(|state| state.buffer.delete_forward()),
            Event::TextMoveLeft { extend } => {
                self.edit_focused(|state| state.buffer.move_left(extend))
            }
            Event::TextMoveRight { extend } => {
                self.edit_focused(|state| state.buffer.move_right(extend))
            }
            Event::TextMoveHome { extend } => {
                self.edit_focused(|state| state.buffer.move_home(extend))
            }
            Event::TextMoveEnd { extend } => {
                self.edit_focused(|state| state.buffer.move_end(extend))
            }
            Event::TextSelectAll => self.edit_focused(|state| state.buffer.select_all()),
            Event::TextUndo => self.edit_focused(|state| state.buffer.undo()),
            Event::TextRedo => self.edit_focused(|state| state.buffer.redo()),
            Event::ImeBatch {
                preedit,
                commit,
                delete,
            } => self.apply_ime_batch(preedit, commit, delete),
            Event::Activate(action) => {
                self.dispatch(action);
                if let Some(now_ns) = now_ns {
                    if self.action_is_declared(action) {
                        self.level2.observe_action(action, now_ns);
                    }
                }
            }
            Event::Character('\u{8}') => {
                if self.focused_input.is_some() {
                    self.edit_focused(|state| state.buffer.delete_backward());
                } else {
                    self.append_text('\u{8}');
                }
            }
            Event::Character(c) if !c.is_control() && self.focused_input.is_some() => {
                self.edit_focused(|state| state.buffer.insert(&c.to_string()));
            }
            Event::Character(_) if self.focused_input.is_some() => {}
            Event::Character(c) => self.append_text(c),
        }
        // Relayout only when state changed since the last layout.
        if self.revision_at_layout != Some(self.store.revision()) {
            self.relayout();
        }
        true
    }

    fn action_is_declared(&self, action: ActionId) -> bool {
        self.module.nodes.iter().any(
            |(_, node)| matches!(node, Node::Button { on_activate, .. } if *on_activate == action),
        )
    }

    fn dispatch(&mut self, action: ActionId) {
        // Wave 1 built-in action: increment the `count` state key.
        if action == ActionId(1) {
            if let Some(tx) = self.store.begin() {
                let next = match self.store.get("count") {
                    Some(Value::Int(n)) => n + 1,
                    _ => 0,
                };
                tx.set("count", Value::Int(next));
                // Invariant: the counter is monotonically valid (>= 0).
                let _ = tx.commit(|m| matches!(m.get("count"), Some(Value::Int(n)) if *n >= 0));
            }
        }
    }

    fn append_text(&mut self, c: char) {
        if let Some(tx) = self.store.begin() {
            let mut s = match self.store.get("typed") {
                Some(Value::Text(s)) => s,
                _ => String::new(),
            };
            s.push(c);
            tx.set("typed", Value::Text(s));
            let _ = tx.commit(|_| true);
        }
    }

    fn edit_focused(&mut self, edit: impl FnOnce(&mut TextFieldState)) {
        if let Some(state) = self
            .focused_input
            .and_then(|id| self.text_inputs.get_mut(&id))
        {
            edit(state);
        }
    }

    fn apply_ime_batch(
        &mut self,
        preedit: Option<(String, i32, i32)>,
        commit: Option<String>,
        delete: Option<(u32, u32)>,
    ) {
        self.edit_focused(|state| {
            if delete.is_some() {
                state.preedit = None;
                state.composition_region = None;
            }
            if let Some((before, after)) = delete {
                state.buffer.delete_surrounding(before, after);
            }
            if let Some(text) = commit {
                let range = state
                    .composition_region
                    .take()
                    .or_else(|| state.buffer.selection_bytes())
                    .unwrap_or_else(|| state.buffer.cursor_byte()..state.buffer.cursor_byte());
                state.buffer.replace_range(range, &text);
                state.preedit = None;
            }
            if let Some(value) = preedit {
                if state.composition_region.is_none() {
                    state.composition_region = Some(
                        state
                            .buffer
                            .selection_bytes()
                            .unwrap_or_else(|| {
                                state.buffer.cursor_byte()..state.buffer.cursor_byte()
                            }),
                    );
                }
                state.preedit = (!value.0.is_empty()).then_some(value);
            }
        });
    }

    fn relayout(&mut self) {
        // Bind committed state into text nodes (hello shows counter + typing).
        let count = match self.store.get("count") {
            Some(Value::Int(n)) => n,
            _ => 0,
        };
        let typed = match self.store.get("typed") {
            Some(Value::Text(t)) => t,
            _ => String::new(),
        };
        for (_, node) in self.module.nodes.iter_mut() {
            if let Node::Text { content } = node {
                if content.starts_with("Hello") {
                    *content = format!("Hello, Goya! count={count} typed={typed}");
                }
            }
        }
        self.frame = layout(&self.module, self.constraints);
        self.revision_at_layout = Some(self.store.revision());
        self.geometry_revision = self
            .geometry_revision
            .and_then(|revision| revision.checked_add(1));
    }

    /// Produce the render primitive list for the current frame.
    pub fn render_list(&self) -> Vec<Primitive> {
        self.render_list_with_text_decorations().0
    }

    /// Produce primitives and text decorations with stable primitive indices.
    pub fn render_list_with_text_decorations(&self) -> (Vec<Primitive>, Vec<TextDecoration>) {
        let mut out = Vec::new();
        let mut decorations = Vec::new();
        self.emit(self.module.root, &mut out, &mut decorations);
        (out, decorations)
    }

    fn emit(
        &self,
        id: Option<NodeId>,
        out: &mut Vec<Primitive>,
        decorations: &mut Vec<TextDecoration>,
    ) {
        let Some(id) = id else { return };
        let Some(node) = self.module.node(id) else {
            return;
        };
        let rect = self.frame.rect(id).unwrap_or(Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
        match node {
            Node::Column { children } | Node::Row { children } => {
                out.push(Primitive::Panel(rect));
                for child in children {
                    self.emit(Some(*child), out, decorations);
                }
            }
            Node::Text { content } => out.push(Primitive::Text(rect, content.clone())),
            // Input values live in the runtime text model. The placeholder is
            // shown until the user commits text or an IME preedit arrives.
            Node::Input { placeholder } => {
                if let Some(state) = self.text_inputs.get(&id) {
                    let focused = self.focused_input == Some(id);
                    let text = state.buffer.text();
                    let cursor = state.buffer.cursor_byte().min(text.len());
                    let mut value = String::with_capacity(
                        text.len()
                            + placeholder.len()
                            + state.preedit.as_ref().map_or(0, |p| p.0.len()),
                    );
                    let mut caret_byte_offset = None;
                    let mut selection_byte_range = None;
                    if text.is_empty() && state.preedit.is_none() {
                        value.push_str(placeholder);
                        if focused {
                            caret_byte_offset = Some(0);
                        }
                    } else {
                        let replacement = if state.preedit.is_some() {
                            state.buffer.selection_bytes().unwrap_or(cursor..cursor)
                        } else {
                            cursor..cursor
                        };
                        value.push_str(&text[..replacement.start]);
                        if let Some((preedit, cursor_begin, _)) = &state.preedit {
                            let base = value.len();
                            value.push_str(preedit);
                            if focused {
                                let start = if *cursor_begin >= 0 {
                                    grapheme_boundary_at_or_before(preedit, *cursor_begin as usize)
                                } else {
                                    0
                                };
                                let end = if state
                                    .preedit
                                    .as_ref()
                                    .is_some_and(|(_, _, end)| *end >= 0)
                                {
                                    let end = state.preedit.as_ref().unwrap().2 as usize;
                                    grapheme_boundary_at_or_before(preedit, end)
                                } else {
                                    start
                                };
                                caret_byte_offset = Some(base + start);
                                if end > start {
                                    selection_byte_range = Some((base + start, base + end));
                                }
                            }
                        } else if focused {
                            caret_byte_offset = Some(value.len());
                            selection_byte_range =
                                state.buffer.selection_bytes().map(|r| (r.start, r.end));
                        }
                        value.push_str(&text[replacement.end..]);
                    }
                    let primitive_index = out.len();
                    out.push(Primitive::Text(rect, value));
                    if focused {
                        decorations.push(TextDecoration {
                            node_id: id,
                            primitive_index,
                            caret_byte_offset,
                            selection_byte_range,
                        });
                    }
                } else {
                    out.push(Primitive::Text(rect, placeholder.clone()));
                }
            }
            Node::Button { label, .. } => out.push(Primitive::Button(rect, label.clone())),
            Node::Spacer { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_ir::parse;

    fn runtime() -> Runtime {
        let module =
            parse("column 1 2 3\ntext 2 Hello, Goya!\nbutton 3 1 Increment\nroot 1").unwrap();
        Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 800_000,
                max_height: 600_000,
            },
        )
    }

    fn runtime_with_two_actions() -> Runtime {
        let module = parse("column 1 2 3\nbutton 2 1 Increment\nbutton 3 2 Other\nroot 1").unwrap();
        Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 800_000,
                max_height: 600_000,
            },
        )
    }

    #[test]
    fn quit_stops_the_loop() {
        let mut rt = runtime();
        assert!(!rt.handle(Event::Quit));
    }

    #[test]
    fn activation_updates_state_and_layout() {
        let mut rt = runtime();
        assert!(rt.handle(Event::Activate(ActionId(1))));
        assert_eq!(rt.store().get("count"), Some(Value::Int(0)));
        assert!(rt.handle(Event::Activate(ActionId(1))));
        assert_eq!(rt.store().get("count"), Some(Value::Int(1)));
        // The bound text node reflects the counter.
        let items = rt.render_list();
        assert!(matches!(&items[..], [
            Primitive::Panel(_),
            Primitive::Text(_, t),
            Primitive::Button(_, _),
        ] if t.contains("count=1")));
    }

    #[test]
    fn hit_test_finds_button() {
        let mut rt = runtime();
        let r = rt.frame().rect(NodeId(3)).unwrap();
        let action = rt.hit_test(r.x + 1, r.y + 1).unwrap();
        assert!(rt.handle(Event::Activate(action)));
    }

    #[test]
    fn unknown_action_is_inert() {
        let mut rt = runtime();
        assert!(rt.handle(Event::Activate(ActionId(999))));
        assert_eq!(rt.store().get("count"), None);
    }

    #[test]
    fn declared_input_uses_shared_keyboard_ime_and_accessibility_edit_path() {
        let module = parse("column 1 2\ninput 2 Search\nroot 1").unwrap();
        let mut rt = Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 400_000,
                max_height: 300_000,
            },
        );
        let input_rect = rt.frame().rect(NodeId(2)).unwrap();
        assert_eq!(
            rt.hit_test_target(input_rect.x + 1, input_rect.y + 1),
            Some(HitTarget::Input(NodeId(2)))
        );
        assert!(rt.focus_input(Some(NodeId(2))));
        assert!(rt.handle(Event::Character('e')));
        assert!(rt.handle(Event::Character('\u{301}')));
        assert_eq!(rt.input_text(NodeId(2)), Some("e\u{301}"));
        assert_eq!(rt.input_cursor_byte(NodeId(2)), Some("e\u{301}".len()));
        assert_eq!(rt.input_anchor_byte(NodeId(2)), Some("e\u{301}".len()));
        assert!(rt.handle(Event::TextBackspace));
        assert_eq!(rt.input_text(NodeId(2)), Some(""));
        assert!(rt.handle(Event::ImeBatch {
            preedit: Some(("に".into(), 0, 3)),
            commit: None,
            delete: None,
        }));
        assert_eq!(rt.input_text(NodeId(2)), Some(""));
        assert!(rt.handle(Event::ImeBatch {
            preedit: None,
            commit: Some("日".into()),
            delete: None,
        }));
        assert_eq!(rt.input_text(NodeId(2)), Some("日"));
        assert!(rt.handle(Event::TextMoveHome { extend: false }));
        assert!(rt.handle(Event::ImeBatch {
            preedit: Some(("に".into(), 0, 0)),
            commit: None,
            delete: None,
        }));
        assert_eq!(rt.input_text(NodeId(2)), Some("日"));
        assert!(matches!(
            rt.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "に日"
        ));
        assert!(rt.handle(Event::ImeBatch {
            preedit: None,
            commit: Some("日本".into()),
            delete: None,
        }));
        assert_eq!(rt.input_text(NodeId(2)), Some("日本日"));
        assert!(rt.handle(Event::SetInputValue(NodeId(2), "screen reader".into())));
        assert_eq!(rt.input_text(NodeId(2)), Some("screen reader"));
        assert_eq!(rt.input_cursor_byte(NodeId(2)), Some("screen reader".len()));
        assert!(matches!(
            rt.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "screen reader"
        ));
    }

    #[test]
    fn platform_can_set_selection_only_on_the_focused_input() {
        let module = parse("column 1 2 3\ninput 2 First\ninput 3 Second\nroot 1").unwrap();
        let mut rt = Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 400_000,
                max_height: 300_000,
            },
        );
        assert!(rt.focus_input(Some(NodeId(2))));
        assert!(rt.handle(Event::SetInputValue(NodeId(2), "a👩‍💻e\u{301}z".into(),)));
        assert!(rt.handle(Event::SetInputSelection {
            node: NodeId(2),
            anchor_byte: 15,
            cursor_byte: 14,
        }));
        assert_eq!(rt.input_anchor_byte(NodeId(2)), Some(15));
        assert_eq!(rt.input_cursor_byte(NodeId(2)), Some(12));

        assert!(rt.handle(Event::SetInputSelection {
            node: NodeId(3),
            anchor_byte: 0,
            cursor_byte: 0,
        }));
        assert_eq!(rt.input_cursor_byte(NodeId(2)), Some(12));
    }

    #[test]
    fn ime_composition_replaces_selection_and_finish_commits_at_composing_cursor() {
        let module = parse("input 1 Search\nroot 1").unwrap();
        let mut rt = Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 400_000,
                max_height: 300_000,
            },
        );
        assert!(rt.focus_input(Some(NodeId(1))));
        assert!(rt.handle(Event::SetInputValue(NodeId(1), "abc".into())));
        assert!(rt.handle(Event::SetInputSelection {
            node: NodeId(1),
            anchor_byte: 1,
            cursor_byte: 2,
        }));
        assert!(rt.handle(Event::ImeBatch {
            preedit: Some(("かな".into(), 3, 6)),
            commit: None,
            delete: None,
        }));
        assert!(matches!(
            rt.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "aかなc"
        ));

        assert!(rt.handle(Event::FinishComposition));
        assert_eq!(rt.input_text(NodeId(1)), Some("aかなc"));
        assert_eq!(rt.input_cursor_byte(NodeId(1)), Some(4));
        assert!(matches!(
            rt.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "aかなc"
        ));
    }

    #[test]
    fn input_render_keeps_text_unchanged_and_reports_focused_caret_and_preedit() {
        let module = parse("input 1 Search\nroot 1").unwrap();
        let mut rt = Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 400_000,
                max_height: 300_000,
            },
        );
        assert!(rt.focus_input(Some(NodeId(1))));
        assert!(rt.handle(Event::SetInputValue(NodeId(1), "日a".into())));
        assert!(rt.handle(Event::TextMoveHome { extend: false }));
        assert!(matches!(
            rt.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "日a"
        ));
        let (primitives, decorations) = rt.render_list_with_text_decorations();
        assert_eq!(
            primitives.last(),
            Some(&Primitive::Text(
                rt.frame().rect(NodeId(1)).unwrap(),
                "日a".into()
            ))
        );
        assert_eq!(decorations.len(), 1);
        assert_eq!(decorations[0].caret_byte_offset, Some(0));
        assert!(rt.handle(Event::ImeBatch {
            preedit: Some(("かな".into(), 3, 6)),
            commit: None,
            delete: None,
        }));
        assert!(matches!(
            rt.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "かな日a"
        ));
        let (_, decorations) = rt.render_list_with_text_decorations();
        assert_eq!(decorations[0].caret_byte_offset, Some("か".len()));
        assert_eq!(
            decorations[0].selection_byte_range,
            Some(("か".len(), "かな".len()))
        );
        assert!(rt.focus_input(None));
        assert!(matches!(
            rt.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "日a"
        ));
        assert!(rt.render_list_with_text_decorations().1.is_empty());
    }

    #[test]
    fn input_selection_decoration_uses_utf8_offsets_and_does_not_change_text() {
        let module = parse("input 1 Search\nroot 1").unwrap();
        let mut rt = Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 400_000,
                max_height: 300_000,
            },
        );
        rt.focus_input(Some(NodeId(1)));
        rt.handle(Event::SetInputValue(NodeId(1), "é漢字".into()));
        rt.handle(Event::TextSelectAll);
        let (primitives, decorations) = rt.render_list_with_text_decorations();
        assert!(
            matches!(primitives.last(), Some(Primitive::Text(_, value)) if value == "é漢字" && !value.contains('│'))
        );
        assert_eq!(decorations[0].caret_byte_offset, Some("é漢字".len()));
        assert_eq!(
            decorations[0].selection_byte_range,
            Some((0, "é漢字".len()))
        );
    }

    #[test]
    fn level2_is_off_until_explicit_consent_and_revision_bound() {
        let mut rt = runtime_with_two_actions();
        assert!(!rt.level2.is_enabled());
        for action in [1, 2, 1, 2] {
            assert!(rt.handle_at(Event::Activate(ActionId(action)), 10));
        }
        assert_eq!(rt.predict_level2_at(10), None);

        rt.opt_in_level2(ConsentRecord::granted("interaction-sequences", 100), 32);
        for action in [1, 2, 1, 2] {
            assert!(rt.handle_at(Event::Activate(ActionId(action)), 11));
        }
        let prediction = rt.predict_level2_at(11).expect("trained transition");
        assert_eq!(prediction.action, ActionId(1));
        assert_eq!(prediction.probability, 1.0);
        assert!(rt.level2_prediction_is_current(&prediction, 11));

        assert!(rt.handle_at(Event::Activate(ActionId(1)), 12));
        assert!(!rt.level2_prediction_is_current(&prediction, 12));
    }

    #[test]
    fn level2_expired_consent_and_opt_out_stop_learning_and_predictions() {
        let mut rt = runtime_with_two_actions();
        rt.opt_in_level2(ConsentRecord::granted("interaction-sequences", 20), 16);
        assert!(rt.handle_at(Event::Activate(ActionId(1)), 10));
        assert!(rt.handle_at(Event::Activate(ActionId(2)), 21));
        assert_eq!(rt.predict_level2_at(21), None);

        rt.opt_in_level2(ConsentRecord::granted("interaction-sequences", 40), 16);
        for action in [1, 2, 1, 2] {
            assert!(rt.handle_at(Event::Activate(ActionId(action)), 30));
        }
        assert!(rt.predict_level2_at(30).is_some());
        rt.opt_out_level2();
        assert_eq!(rt.predict_level2_at(30), None);
    }
}
