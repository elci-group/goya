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
        Self { text, cursor, anchor: None, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn text(&self) -> &str { &self.text }
    pub fn cursor_byte(&self) -> usize { self.cursor }
    pub fn selection_bytes(&self) -> Option<std::ops::Range<usize>> {
        self.anchor.map(|anchor| anchor.min(self.cursor)..anchor.max(self.cursor))
    }
    pub fn grapheme_len(&self) -> usize { self.text.graphemes(true).count() }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.record_undo();
        self.text = text.into();
        self.cursor = self.text.len();
        self.anchor = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.len();
    }

    pub fn move_left(&mut self, extend: bool) {
        let next = self.text.grapheme_indices(true)
            .map(|(i, _)| i).take_while(|i| *i < self.cursor).last().unwrap_or(0);
        self.move_cursor(next, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        let next = self.text.grapheme_indices(true)
            .map(|(i, _)| i).find(|i| *i > self.cursor).unwrap_or(self.text.len());
        self.move_cursor(next, extend);
    }

    pub fn move_home(&mut self, extend: bool) { self.move_cursor(0, extend); }
    pub fn move_end(&mut self, extend: bool) { self.move_cursor(self.text.len(), extend); }

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
            let end = self.cursor;
            self.move_left(false);
            if self.cursor != end {
                self.record_snapshot(TextSnapshot { text: self.text.clone(), cursor: end, anchor: None });
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
            let start = self.cursor;
            self.move_right(false);
            let end = self.cursor;
            if end != start {
                self.record_snapshot(TextSnapshot { text: self.text.clone(), cursor: start, anchor: None });
                self.text.replace_range(start..end, "");
                self.cursor = start;
            }
        }
    }

    pub fn undo(&mut self) { self.restore_history(true); }
    pub fn redo(&mut self) { self.restore_history(false); }

    fn move_cursor(&mut self, cursor: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = cursor.min(self.text.len());
    }

    fn snapshot(&self) -> TextSnapshot {
        TextSnapshot { text: self.text.clone(), cursor: self.cursor, anchor: self.anchor }
    }

    fn record_undo(&mut self) {
        self.record_snapshot(self.snapshot());
        self.redo.clear();
    }

    fn record_snapshot(&mut self, snapshot: TextSnapshot) {
        self.undo.push(snapshot);
        if self.undo.len() > Self::HISTORY_LIMIT { self.undo.remove(0); }
    }

    fn restore_history(&mut self, undo: bool) {
        let (from, to) = if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        if let Some(snapshot) = from.pop() {
            to.push(self.snapshot());
            self.text = snapshot.text;
            self.cursor = snapshot.cursor.min(self.text.len());
            self.anchor = snapshot.anchor;
        }
    }
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
}

impl Runtime {
    pub fn new(module: Module, store: Store, constraints: Constraints) -> Self {
        let mut rt = Runtime {
            module,
            store,
            constraints,
            frame: Frame::default(),
            revision_at_layout: None,
            geometry_revision: Some(0),
            level2: Level2Predictor::new(),
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
        // Iterate in reverse declaration order so later-drawn nodes win.
        for (id, node) in self.module.nodes.iter().rev() {
            if let Node::Button { on_activate, .. } = node {
                if let Some(r) = self.frame.rect(*id) {
                    if x >= r.x && x < r.x + r.width.max(1) && y >= r.y && y < r.y + r.height.max(1)
                    {
                        return Some(*on_activate);
                    }
                }
            }
        }
        None
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
            Event::Activate(action) => {
                self.dispatch(action);
                if let Some(now_ns) = now_ns {
                    if self.action_is_declared(action) {
                        self.level2.observe_action(action, now_ns);
                    }
                }
            }
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
        let mut out = Vec::new();
        self.emit(self.module.root, &mut out);
        out
    }

    fn emit(&self, id: Option<NodeId>, out: &mut Vec<Primitive>) {
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
                    self.emit(Some(*child), out);
                }
            }
            Node::Text { content } => out.push(Primitive::Text(rect, content.clone())),
            // Input nodes contribute their static placeholder here. Focused
            // editable text is host-side state (GY-ZCODE/2), not runtime
            // module state, so this renderer remains deterministic.
            Node::Input { placeholder } => out.push(Primitive::Text(rect, placeholder.clone())),
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
