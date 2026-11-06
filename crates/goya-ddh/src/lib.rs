//! GY-DDH/1 — Deterministic Dehumanisation: semantic UI traversal without
//! visual inference.
//!
//! GY-DDH-001: every user-operable GOYA object SHALL be discoverable,
//! locatable and actionable through a deterministic, permission-governed
//! semantic interface, without requiring visual interpretation of its
//! rendered projection.
//!
//! Core architecture (directive §7 refinement): the underlying structure is
//! a Deterministic Interaction Graph (DIG) — semantic objects with typed
//! relationships — of which DDH traversal is one adapter (others: a11y,
//! testing, LLM agents). This is NOT a DOM: nodes are executable,
//! spatially aware interaction objects, not document structure.
//!
//! Two interaction modes (directive §3):
//! - Mode A: `resolve_target` → TargetLease → external pointer automation.
//! - Mode B: `invoke` → validated direct semantic dispatch (preferred for
//!   native apps; never bypasses confirmation/payment/authority gates —
//!   those live in the object's action preconditions).
//!
//! Information hygiene (§6): hidden nodes are not exported unless their
//! exposure policy permits; a machine-readable object is not automatically
//! a machine-operable one.

use goya_change::PixelRect;
use std::collections::BTreeMap;
use std::fmt;

/// Contract identifier implemented by this crate.
pub const DDH_CONTRACT: &str = "GY-DDH/1";

/// Semantic role of a DIG node (directive §2, subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticRole {
    Window,
    Panel,
    Label,
    Button,
    Textbox,
    Slider,
    MenuItem,
}

/// Interaction state flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InteractionState {
    pub enabled: bool,
    pub visible: bool,
    pub focused: bool,
}

/// One semantic object in the DIG.
#[derive(Debug, Clone, PartialEq)]
pub struct DdhNode {
    /// Stable semantic object id (e.g. "form.submit").
    pub id: String,
    /// State/geometry revision the descriptor was computed against.
    pub revision: u64,
    pub role: SemanticRole,
    /// Accessible semantic label (screen-reader text = agent text).
    pub label: String,
    /// Interactive hit region in millipoints.
    pub hit_region: PixelRect,
    pub state: InteractionState,
    /// Permitted actions with preconditions satisfied at this revision.
    pub actions: Vec<String>,
    /// Exposure policy: false = never exported to any machine interface.
    pub exposed: bool,
    /// DIG parent (typed "contains" edge).
    pub parent: Option<String>,
    /// Optional current user-facing value, description, selection state, and
    /// bounded numeric range projected by accessible hosts.
    pub semantics: SemanticAttributes,
}

/// Optional value semantics shared with accessibility adapters. Missing
/// values remain missing; adapters must not invent values or ranges.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SemanticAttributes {
    pub value: Option<String>,
    pub description: Option<String>,
    pub selected: Option<bool>,
    pub range: Option<NumericRange>,
}

/// A validated bounded numeric range suitable for a slider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericRange {
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
}

impl NumericRange {
    pub fn new(minimum: f64, maximum: f64, step: f64) -> Option<Self> {
        (minimum.is_finite()
            && maximum.is_finite()
            && step.is_finite()
            && minimum < maximum
            && step > 0.0)
            .then_some(Self {
                minimum,
                maximum,
                step,
            })
    }

    pub fn clamp(&self, value: f64) -> Option<f64> {
        value
            .is_finite()
            .then(|| value.clamp(self.minimum, self.maximum))
    }
}

/// The Deterministic Interaction Graph.
#[derive(Debug, Default)]
pub struct Dig {
    nodes: BTreeMap<String, DdhNode>,
    /// Monotonic state revision, bumped by recorded state changes.
    revision: u64,
    /// Revision → changed object ids (for observe_changes).
    changes: Vec<(u64, String)>,
}

impl Dig {
    pub fn new() -> Self {
        Dig::default()
    }

    /// Insert/replace a node and record it as changed at `revision`.
    pub fn put(&mut self, node: DdhNode, revision: u64) {
        self.revision = self.revision.max(revision);
        self.changes.push((revision, node.id.clone()));
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn get(&self, id: &str) -> Option<&DdhNode> {
        self.nodes.get(id)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Exported view: only exposed, visible nodes (information hygiene).
    pub fn exported(&self) -> impl Iterator<Item = &DdhNode> {
        self.nodes.values().filter(|n| n.exposed && n.state.visible)
    }

    /// `ddh.find(role, name)` — first exported node matching role and
    /// label substring.
    pub fn find(&self, role: SemanticRole, label_substring: &str) -> Option<&DdhNode> {
        self.exported()
            .find(|n| n.role == role && n.label.contains(label_substring))
    }

    /// `ddh.observe_changes(since)` — object ids changed at/after a
    /// revision (LCPE-fed incremental updates, directive §5).
    pub fn observe_changes(&self, since: u64) -> Vec<String> {
        self.changes
            .iter()
            .filter(|(rev, _)| *rev >= since)
            .map(|(_, id)| id.clone())
            .collect()
    }

    /// Mode B invocation validation: authority + exposure + enabled +
    /// action permitted. Returns `InvocationResult`; the CALLER performs
    /// the state transition and records its revision — DDH validates and
    /// dispatches, it does not bypass application safeguards.
    pub fn validate_invoke(
        &self,
        object: &str,
        action: &str,
        caller: &str,
        authority: &dyn Fn(&str, &str, &str) -> bool,
    ) -> InvocationResult {
        let Some(node) = self.nodes.get(object) else {
            return InvocationResult::NoSuchObject;
        };
        if !node.exposed || !node.state.visible {
            return InvocationResult::NotExposed;
        }
        if !node.state.enabled {
            return InvocationResult::Disabled;
        }
        if !node.actions.iter().any(|a| a == action) {
            return InvocationResult::ActionNotPermitted;
        }
        if !authority(caller, object, action) {
            return InvocationResult::AuthorityDenied;
        }
        InvocationResult::Dispatched {
            object: object.to_string(),
            action: action.to_string(),
            state_revision: self.revision,
        }
    }
}

/// Invocation outcomes (typed results; no panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationResult {
    Dispatched {
        object: String,
        action: String,
        state_revision: u64,
    },
    NoSuchObject,
    NotExposed,
    Disabled,
    ActionNotPermitted,
    AuthorityDenied,
}

impl fmt::Display for InvocationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvocationResult::Dispatched { object, action, .. } => {
                write!(f, "dispatched {action} on {object}")
            }
            InvocationResult::NoSuchObject => write!(f, "no such object"),
            InvocationResult::NotExposed => write!(f, "object not exposed"),
            InvocationResult::Disabled => write!(f, "object disabled"),
            InvocationResult::ActionNotPermitted => write!(f, "action not permitted"),
            InvocationResult::AuthorityDenied => write!(f, "authority denied"),
        }
    }
}

/// Mode A: a lease on a click target. Invalidated by any revision change
/// of the object — a prediction is never a guaranteed click target (§2).
#[derive(Debug, Clone, PartialEq)]
pub struct TargetLease {
    pub object: String,
    pub revision: u64,
    /// Centre of the hit region, millipoints.
    pub target: (i64, i64),
}

impl Dig {
    /// Resolve a fresh lease for an object's hit region.
    pub fn resolve_target(&self, object: &str) -> Option<TargetLease> {
        let node = self.nodes.get(object)?;
        if !node.exposed || !node.state.enabled {
            return None;
        }
        Some(TargetLease {
            object: object.to_string(),
            revision: node.revision,
            target: (
                i64::from(node.hit_region.x) + i64::from(node.hit_region.width) / 2,
                i64::from(node.hit_region.y) + i64::from(node.hit_region.height) / 2,
            ),
        })
    }

    /// Lease validity: still valid only if the object's revision is
    /// unchanged (geometry/visibility/permission changes invalidate).
    pub fn lease_valid(&self, lease: &TargetLease) -> bool {
        self.nodes
            .get(&lease.object)
            .map(|n| n.revision == lease.revision)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(id: &str, label: &str, x: i32, y: i32, rev: u64) -> DdhNode {
        DdhNode {
            id: id.into(),
            revision: rev,
            role: SemanticRole::Button,
            label: label.into(),
            hit_region: PixelRect {
                x,
                y,
                width: 200,
                height: 60,
            },
            state: InteractionState {
                enabled: true,
                visible: true,
                focused: false,
            },
            actions: vec!["press".into()],
            exposed: true,
            parent: Some("calc.window".into()),
            semantics: SemanticAttributes::default(),
        }
    }

    #[test]
    fn numeric_range_rejects_invalid_bounds_and_steps() {
        assert!(NumericRange::new(0.0, 0.0, 1.0).is_none());
        assert!(NumericRange::new(2.0, 1.0, 1.0).is_none());
        assert!(NumericRange::new(0.0, 1.0, 0.0).is_none());
        assert!(NumericRange::new(0.0, f64::INFINITY, 0.1).is_none());
        assert_eq!(
            NumericRange::new(-1.0, 1.0, 0.25).unwrap().clamp(2.0),
            Some(1.0)
        );
    }

    fn dig() -> Dig {
        let mut d = Dig::new();
        d.put(button("calc.btn_8", "8", 290, 700, 1), 1);
        d.put(button("calc.btn_7", "7", 20, 700, 1), 1);
        d.put(button("calc.btn_mul", "*", 770, 700, 1), 1);
        d.put(button("calc.btn_4", "4", 20, 950, 1), 1);
        d.put(button("calc.btn_2", "2", 290, 1200, 1), 1);
        d.put(button("calc.btn_eq", "=", 770, 1450, 1), 1);
        d
    }

    fn allow_all(_: &str, _: &str, _: &str) -> bool {
        true
    }

    #[test]
    fn semantic_agent_discovers_and_targets_without_pixels() {
        let d = dig();
        let target = d.find(SemanticRole::Button, "8").expect("find by label");
        assert_eq!(target.id, "calc.btn_8");
        let lease = d.resolve_target("calc.btn_8").unwrap();
        assert_eq!(lease.target.0, 290 + 100); // centre x in millipoints
        let r = d.validate_invoke("calc.btn_8", "press", "agent", &allow_all);
        assert!(matches!(r, InvocationResult::Dispatched { .. }));
    }

    #[test]
    fn hidden_nodes_are_never_exported() {
        let mut d = dig();
        let mut secret = button("calc.secret", "panic-key", 0, 0, 1);
        secret.exposed = false;
        d.put(secret, 1);
        assert!(d.find(SemanticRole::Button, "panic").is_none());
        assert_eq!(
            d.validate_invoke("calc.secret", "press", "agent", &allow_all),
            InvocationResult::NotExposed
        );
    }

    #[test]
    fn disabled_object_cannot_be_invoked() {
        let mut d = dig();
        let mut off = button("calc.btn_off", "off", 0, 0, 1);
        off.state.enabled = false;
        d.put(off, 1);
        assert_eq!(
            d.validate_invoke("calc.btn_off", "press", "agent", &allow_all),
            InvocationResult::Disabled
        );
    }

    #[test]
    fn authority_denied_without_grant() {
        let d = dig();
        let deny = |_caller: &str, _: &str, _: &str| false;
        assert_eq!(
            d.validate_invoke("calc.btn_8", "press", "agent", &deny),
            InvocationResult::AuthorityDenied
        );
    }

    #[test]
    fn leases_invalidate_on_revision_change() {
        let mut d = dig();
        let lease = d.resolve_target("calc.btn_eq").unwrap();
        assert!(d.lease_valid(&lease));
        // Geometry change bumps the node revision.
        let mut moved = button("calc.btn_eq", "=", 770, 1450, 2);
        moved.revision = 2;
        d.put(moved, 2);
        assert!(!d.lease_valid(&lease), "stale lease must be invalid");
        let fresh = d.resolve_target("calc.btn_eq").unwrap();
        assert!(d.lease_valid(&fresh));
    }

    #[test]
    fn observe_changes_returns_incremental_updates() {
        let mut d = dig();
        d.put(button("calc.btn_a", "a", 0, 0, 2), 2);
        d.put(button("calc.btn_b", "b", 0, 0, 3), 3);
        let since_2 = d.observe_changes(2);
        assert_eq!(since_2, vec!["calc.btn_a", "calc.btn_b"]);
        let since_3 = d.observe_changes(3);
        assert_eq!(since_3, vec!["calc.btn_b"]);
    }

    #[test]
    fn action_not_permitted_for_undeclared_actions() {
        let d = dig();
        assert_eq!(
            d.validate_invoke("calc.btn_8", "format_disk", "agent", &allow_all),
            InvocationResult::ActionNotPermitted
        );
    }
}
