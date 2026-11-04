//! GY-RUNTIME/1 — Event dispatch and input→state→layout pipeline.
//!
//! Contract: the runtime consumes `Event`s, applies them to a `goya_state`
//! store via transactions, recomputes layout, and produces a render list.
//! All processing is synchronous and deterministic for Wave 1; concurrency
//! arrives in Wave 2 behind the same interfaces.

use goya_ir::{ActionId, Module, Node, NodeId};
use goya_layout::{layout, Constraints, Frame, Rect};
use goya_state::{Store, Value};

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
}

impl Runtime {
    pub fn new(module: Module, store: Store, constraints: Constraints) -> Self {
        let mut rt = Runtime {
            module,
            store,
            constraints,
            frame: Frame::default(),
            revision_at_layout: None,
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
        match event {
            Event::Quit => return false,
            Event::Activate(action) => self.dispatch(action),
            Event::Character(c) => self.append_text(c),
        }
        // Relayout only when state changed since the last layout.
        if self.revision_at_layout != Some(self.store.revision()) {
            self.relayout();
        }
        true
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
}
