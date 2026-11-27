//! The behavioural state space (directive §11.1): for each styled element,
//! which interaction states it can be in, and how many combinations the
//! host would have to present. This is the quantity the budget in §11.2
//! caps.

use std::collections::BTreeSet;

use crate::adaptive::interaction_adaptation::InteractionState;
use crate::behavioural::state_extraction::extract_element_states;
use crate::ir::casa_ir::CasaRule;

/// One element's (selector's) state set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementStateSet {
    /// The selector the states were extracted from.
    pub name: String,
    pub states: BTreeSet<InteractionState>,
}

impl ElementStateSet {
    /// Combinations one element contributes: every state independently
    /// on or off.
    pub fn combinations(&self) -> u64 {
        1u64.checked_shl(self.states.len() as u32)
            .unwrap_or(u64::MAX)
    }
}

/// The stylesheet's behavioural state space.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StateSpace {
    /// Elements that condition on at least one interaction state, in
    /// source order.
    pub elements: Vec<ElementStateSet>,
}

impl StateSpace {
    /// Total combinations across all stateful elements. Saturating: a
    /// genuine overflow means the budget must collapse the space.
    pub fn combinations(&self) -> u64 {
        self.elements
            .iter()
            .try_fold(1u64, |acc, element| acc.checked_mul(element.combinations()))
            .unwrap_or(u64::MAX)
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// Build the state space from a rule set. Elements are keyed by selector
/// text; duplicate (selector, states) pairs collapse.
pub fn build(rules: &[CasaRule]) -> StateSpace {
    let elements = extract_element_states(rules)
        .into_iter()
        .map(|extracted| ElementStateSet {
            name: extracted.selector,
            states: extracted.states,
        })
        .collect();
    StateSpace { elements }
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
    fn combinations_are_powers_of_two_per_element() {
        let rules = lower("a:hover { color: red } input:focus, input:disabled { color: blue }");
        let space = build(&rules);
        assert_eq!(space.elements.len(), 3);
        // a:hover → 2, input:focus → 2, input:disabled → 2.
        assert_eq!(space.combinations(), 8);
    }

    #[test]
    fn stateless_rules_contribute_nothing() {
        let rules = lower(".plain { color: red }");
        let space = build(&rules);
        assert!(space.is_empty());
        assert_eq!(space.combinations(), 1);
    }

    #[test]
    fn deep_state_sets_multiply() {
        let rules = lower(
            "a:hover:focus:active { color: red } \
             b:hover:focus:active:checked:disabled { color: blue }",
        );
        let space = build(&rules);
        // 2^3 * 2^5 = 256.
        assert_eq!(space.combinations(), 256);
    }

    #[test]
    fn duplicate_selectors_collapse() {
        let rules = lower("a:hover { color: red } a:hover { color: blue }");
        let space = build(&rules);
        assert_eq!(space.elements.len(), 1);
    }
}
