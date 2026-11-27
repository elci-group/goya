//! Behavioural state extraction (directive §11): which interaction states
//! the stylesheet styles, per selector, and which states scene elements
//! actually carry. This is the raw material the state-space budget (§11.2)
//! operates on.

use std::collections::{BTreeMap, BTreeSet};

use crate::adaptive::interaction_adaptation::{states_in_selector, InteractionState};
use crate::context::structural::{NodeId, SceneTree};
use crate::ir::casa_ir::CasaRule;

/// One style rule's interaction-state surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementStates {
    /// The selector text as authored (before the comma split).
    pub selector: String,
    /// Interaction states the selector conditions on.
    pub states: BTreeSet<InteractionState>,
}

/// Walk a rule set and extract every (selector, states) pair that
/// conditions on an interaction state, in source order, deduplicated.
pub fn extract_element_states(rules: &[CasaRule]) -> Vec<ElementStates> {
    let mut seen = BTreeSet::new();
    let mut extracted = Vec::new();
    visit_rules(rules, &mut seen, &mut extracted);
    extracted
}

/// Every state string scene elements carry (`hover`, `focus`, ...), with
/// the elements carrying each, in walk order. This is the runtime side of
/// the same vocabulary the selectors use.
pub fn extract_node_states(tree: &SceneTree) -> BTreeMap<String, Vec<String>> {
    let mut by_state: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in tree.walk() {
        let element = &tree.get(node).name;
        for state in &tree.get(node).state {
            let carriers = by_state.entry(state.clone()).or_default();
            if carriers.last() != Some(element) {
                carriers.push(element.clone());
            }
        }
    }
    by_state
}

/// States one node carries, in authored order.
pub fn node_states(tree: &SceneTree, node: NodeId) -> Vec<String> {
    tree.get(node).state.clone()
}

fn visit_rules(
    rules: &[CasaRule],
    seen: &mut BTreeSet<(String, BTreeSet<InteractionState>)>,
    out: &mut Vec<ElementStates>,
) {
    for rule in rules {
        match rule {
            CasaRule::Style(style) => {
                for selector in &style.selectors {
                    let states = states_in_selector(&selector.text);
                    if states.is_empty() {
                        continue;
                    }
                    let key = (selector.text.clone(), states.clone());
                    if seen.insert(key) {
                        out.push(ElementStates {
                            selector: selector.text.clone(),
                            states,
                        });
                    }
                }
            }
            CasaRule::Media(m) => visit_rules(&m.rules, seen, out),
            CasaRule::Container(c) => visit_rules(&c.rules, seen, out),
            CasaRule::Layer(l) => visit_rules(&l.rules, seen, out),
            CasaRule::Supports(s) => visit_rules(&s.rules, seen, out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::reporter::Reporter;
    use crate::ir::casa_ir::Lowering;

    fn lower(css: &str) -> Vec<CasaRule> {
        let mut reporter = Reporter::new();
        let mut resources = Vec::new();
        let mut lowering = Lowering::new("test.css", css, &mut reporter, &mut resources);
        lowering.rules(goya_css_syntax::parse_stylesheet(css))
    }

    #[test]
    fn extracts_states_per_selector() {
        let rules = lower(
            "a:hover { color: red } \
             a:focus, a:active { color: blue } \
             .plain { color: green }",
        );
        let extracted = extract_element_states(&rules);
        assert_eq!(extracted.len(), 3);
        assert_eq!(extracted[0].selector, "a:hover");
        assert_eq!(
            extracted[0].states.iter().copied().collect::<Vec<_>>(),
            vec![InteractionState::Hover]
        );
        assert_eq!(extracted[1].selector, "a:focus");
        assert_eq!(extracted[2].selector, "a:active");
    }

    #[test]
    fn extracts_states_inside_group_rules() {
        let rules = lower("@media (min-width: 400px) { button:hover { color: red } }");
        let extracted = extract_element_states(&rules);
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].selector, "button:hover");
    }

    #[test]
    fn node_states_are_collected_by_state_name() {
        let mut tree = SceneTree::with_root("root");
        let button = tree.append(NodeId(0), "button");
        tree.set_state(button, "hover");
        tree.set_state(button, "focus");
        let menu = tree.append(NodeId(0), "menu");
        tree.set_state(menu, "hover");
        let by_state = extract_node_states(&tree);
        assert_eq!(
            by_state.get("hover").map(Vec::as_slice),
            Some(&["button".to_string(), "menu".to_string()][..])
        );
        assert_eq!(
            by_state.get("focus").map(Vec::as_slice),
            Some(&["button".to_string()][..])
        );
        assert!(!by_state.contains_key("active"));
    }

    #[test]
    fn node_states_in_authored_order() {
        let mut tree = SceneTree::with_root("root");
        let node = tree.append(NodeId(0), "panel");
        tree.set_state(node, "focus");
        tree.set_state(node, "hover");
        assert_eq!(
            node_states(&tree, node),
            vec!["focus".to_string(), "hover".to_string()]
        );
    }
}
