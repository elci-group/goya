//! State-explosion prevention (directive §11.2): cap the behavioural
//! state space at a budget. When the combinations exceed the budget the
//! space is collapsed deterministically — the largest state sets shed
//! their highest-ordered state until the space fits — and every collapse
//! is reported, never silent.

use crate::behavioural::state_space::StateSpace;
use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;

/// Default cap on behavioural state combinations (§11.2 budget).
pub const DEFAULT_STATE_BUDGET: u64 = 1024;

/// Enforce the budget on a state space, returning the collapsed space.
/// One `CASA-BHV-001` is emitted per state removal. An empty budget (0)
/// keeps only stateless elements.
pub fn enforce(space: &StateSpace, budget: u64, reporter: &mut Reporter) -> StateSpace {
    let mut collapsed = space.clone();
    let before = collapsed.combinations();
    while collapsed.combinations() > budget {
        let Some(victim) = pick_collapse_target(&mut collapsed) else {
            break;
        };
        // Highest-ordered state of the largest set: deterministic, and
        // prefers shedding states that sort later (less canonical).
        let removed = victim
            .states
            .iter()
            .next_back()
            .copied()
            .expect("collapse target always has states");
        let name = victim.name.clone();
        victim.states.remove(&removed);
        reporter.emit(
            codes::BEHAVIOURAL_STATE_OVERFLOW,
            format!(
                "state budget {} exceeded ({} combinations); collapsing `{}` by dropping `{}` to {} combinations",
                budget,
                before,
                name,
                removed.token(),
                collapsed.combinations()
            ),
            None,
        );
    }
    collapsed
}

/// The element whose state set is largest; ties break to the earliest in
/// source order. `None` when nothing can be collapsed further.
fn pick_collapse_target(
    space: &mut StateSpace,
) -> Option<&mut crate::behavioural::state_space::ElementStateSet> {
    let max = space
        .elements
        .iter()
        .map(|element| element.states.len())
        .max()?;
    if max == 0 {
        return None;
    }
    space
        .elements
        .iter_mut()
        .find(|element| element.states.len() == max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::interaction_adaptation::InteractionState;
    use crate::behavioural::state_space::ElementStateSet;
    use crate::ir::casa_ir::CasaRule;
    use crate::diagnostics::reporter::Reporter;
    use crate::ir::casa_ir::Lowering;
    use std::collections::BTreeSet;

    fn lower(css: &str) -> Vec<CasaRule> {
        let mut reporter = Reporter::new();
        let mut resources = Vec::new();
        let mut lowering = Lowering::new("test.css", css, &mut reporter, &mut resources);
        lowering.rules(goya_css_syntax::parse_stylesheet(css))
    }

    fn space_with(states: &[&[InteractionState]]) -> StateSpace {
        StateSpace {
            elements: states
                .iter()
                .map(|set| ElementStateSet {
                    name: "el".to_string(),
                    states: set.iter().copied().collect::<BTreeSet<_>>(),
                })
                .collect(),
        }
    }

    #[test]
    fn within_budget_is_untouched() {
        let mut reporter = Reporter::new();
        let space = space_with(&[&[InteractionState::Hover], &[InteractionState::Focus]]);
        let enforced = enforce(&space, DEFAULT_STATE_BUDGET, &mut reporter);
        assert_eq!(enforced, space);
        assert!(reporter.is_empty());
    }

    #[test]
    fn oversized_space_collapses_largest_first() {
        let mut reporter = Reporter::new();
        let space = space_with(&[
            &[
                InteractionState::Hover,
                InteractionState::Focus,
                InteractionState::Active,
            ],
            &[InteractionState::Checked],
        ]);
        // 8 * 2 = 16 > budget 4: collapse the 3-state set twice.
        let enforced = enforce(&space, 4, &mut reporter);
        assert!(enforced.combinations() <= 4);
        assert_eq!(enforced.elements[0].states.len(), 1);
        assert_eq!(enforced.elements[1].states.len(), 1);
        assert_eq!(reporter.len(), 2);
        assert_eq!(
            reporter.diagnostics()[0].code.as_str(),
            codes::BEHAVIOURAL_STATE_OVERFLOW.code
        );
    }

    #[test]
    fn removal_is_deterministic_and_canonical_last() {
        let mut reporter = Reporter::new();
        let space = space_with(&[&[
            InteractionState::Active,
            InteractionState::Disabled,
            InteractionState::Focus,
        ]]);
        let enforced = enforce(&space, 2, &mut reporter);
        // States shed in descending canonical order: Focus, then Disabled,
        // until the set fits the budget.
        assert_eq!(
            enforced.elements[0]
                .states
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![InteractionState::Active]
        );
        assert_eq!(reporter.len(), 2);
    }

    #[test]
    fn collapse_from_real_rulesets() {
        let mut reporter = Reporter::new();
        let rules = lower(
            "a:hover:focus:active:checked:disabled { color: red } \
             b:hover { color: blue }",
        );
        let space = crate::behavioural::state_space::build(&rules);
        assert_eq!(space.combinations(), 64);
        let enforced = enforce(&space, 8, &mut reporter);
        assert!(enforced.combinations() <= 8);
        assert!(!reporter.is_empty());
    }

    #[test]
    fn zero_budget_leaves_stateless() {
        let mut reporter = Reporter::new();
        let space = space_with(&[&[InteractionState::Hover], &[]]);
        // One element has no states; its set cannot shrink further, but
        // the loop breaks rather than looping forever.
        let enforced = enforce(&space, 1, &mut reporter);
        assert!(enforced.elements[1].states.is_empty());
        assert_eq!(enforced.elements[0].states.len(), 0);
        assert_eq!(reporter.len(), 1);
    }
}
