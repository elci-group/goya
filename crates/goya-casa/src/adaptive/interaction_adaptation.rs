//! Interaction affordances (directive §12.1): which interaction states a
//! stylesheet styles, and which pointer/hover capabilities it consults.
//! The host maps these to platform-native affordances — touch targets
//! replace hover states, keyboard focus rings replace `:focus` styling.

use std::collections::BTreeSet;

use crate::ir::casa_ir::CasaRule;

/// An interaction state a stylesheet has rules for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InteractionState {
    Hover,
    Active,
    Focus,
    Checked,
    Disabled,
}

impl InteractionState {
    /// The selector token naming this state, if there is one.
    pub fn token(self) -> &'static str {
        match self {
            InteractionState::Hover => "hover",
            InteractionState::Active => "active",
            InteractionState::Focus => "focus",
            InteractionState::Checked => "checked",
            InteractionState::Disabled => "disabled",
        }
    }

    /// Parse one pseudo-class token; `None` for anything unsupported.
    pub fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_lowercase().as_str() {
            "hover" => Some(InteractionState::Hover),
            "active" => Some(InteractionState::Active),
            "focus" => Some(InteractionState::Focus),
            "checked" => Some(InteractionState::Checked),
            "disabled" => Some(InteractionState::Disabled),
            _ => None,
        }
    }
}

/// The interaction states one style rule's selectors mention.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InteractionAffordance {
    /// The rule's selector text, for the host to attach the affordance to.
    pub selector: String,
    pub states: BTreeSet<InteractionState>,
}

/// The pointer/hover capability features a stylesheet consults. Both are
/// host-only inputs (no compile-time verdict); they scope which affordances
/// matter on the target platform.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapabilityRequirements {
    /// `(hover: hover)` or bare `hover` consulted.
    pub hover_consulted: bool,
    /// `(pointer: fine|coarse|none)` consulted.
    pub pointer_consulted: bool,
}

/// Collect interaction affordances across a rule set, in source order,
/// deduplicated by (selector, states) pairs.
pub fn collect_affordances(rules: &[CasaRule]) -> Vec<InteractionAffordance> {
    let mut seen = BTreeSet::new();
    let mut affordances = Vec::new();
    visit_rules(rules, &mut seen, &mut affordances);
    affordances
}

/// Scan a rule set's media conditions for hover/pointer capability features.
pub fn collect_capability_requirements(rules: &[CasaRule]) -> CapabilityRequirements {
    let mut requirements = CapabilityRequirements::default();
    visit_conditions(rules, &mut requirements);
    requirements
}

fn visit_rules(
    rules: &[CasaRule],
    seen: &mut BTreeSet<(String, BTreeSet<InteractionState>)>,
    out: &mut Vec<InteractionAffordance>,
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
                        out.push(InteractionAffordance {
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

fn visit_conditions(rules: &[CasaRule], requirements: &mut CapabilityRequirements) {
    for rule in rules {
        match rule {
            CasaRule::Media(media) => {
                record_condition(&media.condition, requirements);
                visit_conditions(&media.rules, requirements);
            }
            CasaRule::Container(c) => visit_conditions(&c.rules, requirements),
            CasaRule::Layer(l) => visit_conditions(&l.rules, requirements),
            CasaRule::Supports(s) => visit_conditions(&s.rules, requirements),
            _ => {}
        }
    }
}

fn record_condition(condition: &str, requirements: &mut CapabilityRequirements) {
    for token in condition.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        match token.to_ascii_lowercase().as_str() {
            "hover" => requirements.hover_consulted = true,
            "pointer" => requirements.pointer_consulted = true,
            _ => {}
        }
    }
}

/// Interaction states mentioned in a selector text: `:hover`, `:not(:focus)`,
/// `a:hover, a:active`. Pseudo-elements (`::before`) and unsupported
/// pseudo-classes (`:focus-visible`) yield nothing.
pub fn states_in_selector(text: &str) -> BTreeSet<InteractionState> {
    let mut states = BTreeSet::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                // Pseudo-element: skip its name.
                i += 2;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                    i += 1;
                }
                continue;
            }
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-') {
                end += 1;
            }
            if end == start {
                i += 1;
                continue;
            }
            if let Some(state) = InteractionState::parse(&text[start..end]) {
                states.insert(state);
            }
            i = end;
        } else {
            i += 1;
        }
    }
    states
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
    fn hover_and_focus_affordances_collected() {
        let rules = lower(
            "a:hover { color: red } \
             input:focus { outline: 1px } \
             input:disabled { color: gray } \
             .plain { color: blue }",
        );
        let affordances = collect_affordances(&rules);
        assert_eq!(affordances.len(), 3);
        assert_eq!(
            affordances[0].states.iter().copied().collect::<Vec<_>>(),
            vec![InteractionState::Hover]
        );
        assert_eq!(
            affordances[1].states.iter().copied().collect::<Vec<_>>(),
            vec![InteractionState::Focus]
        );
        assert_eq!(
            affordances[2].states.iter().copied().collect::<Vec<_>>(),
            vec![InteractionState::Disabled]
        );
    }

    #[test]
    fn multiple_states_deduplicated() {
        let rules = lower("a:hover, a:active { color: red } a:hover { color: blue }");
        let affordances = collect_affordances(&rules);
        // The comma expands to two selectors; the duplicate pair is dropped.
        assert_eq!(affordances.len(), 2);
        assert_eq!(
            affordances[0].states.iter().copied().collect::<Vec<_>>(),
            vec![InteractionState::Hover]
        );
        assert_eq!(
            affordances[1].states.iter().copied().collect::<Vec<_>>(),
            vec![InteractionState::Active]
        );
    }

    #[test]
    fn pseudo_elements_and_unsupported_pseudos_ignored() {
        assert!(states_in_selector("a::before").is_empty());
        assert!(states_in_selector("a:focus-visible").is_empty());
        assert_eq!(
            states_in_selector("a:hover::before"),
            BTreeSet::from([InteractionState::Hover])
        );
        assert_eq!(
            states_in_selector("a:not(:focus)"),
            BTreeSet::from([InteractionState::Focus])
        );
    }

    #[test]
    fn capability_requirements_from_media_conditions() {
        let rules = lower(
            "@media (hover: hover) and (pointer: fine) { .a { color: red } } \
             @media (min-width: 400px) { .b { color: blue } }",
        );
        let requirements = collect_capability_requirements(&rules);
        assert!(requirements.hover_consulted);
        assert!(requirements.pointer_consulted);
    }

    #[test]
    fn no_capabilities_without_conditions() {
        let rules = lower(".a { color: red }");
        assert_eq!(
            collect_capability_requirements(&rules),
            CapabilityRequirements::default()
        );
    }
}
