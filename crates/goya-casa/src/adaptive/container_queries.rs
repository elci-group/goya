//! `@container` handling (directive §13.3): container rules are recorded
//! faithfully but their conditions depend on the containing block's size —
//! a runtime property. This module turns them into deferred constraints
//! the host evaluates once layout is known.

use std::collections::BTreeSet;

use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::{CasaRule, ContainerRule};

/// A container condition handed to the host for runtime evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerConstraint {
    /// Container name when authored (`@container card (...)`), else `None`
    /// for the nearest container ancestor.
    pub name: Option<String>,
    /// The parenthesised condition, e.g. `(min-width: 400px)`.
    pub condition: String,
    /// Selector texts of the rules gated on this condition, in source
    /// order — the host needs to know what becomes eligible.
    pub gated_selectors: Vec<String>,
}

/// Extract every container constraint from a rule set, reporting each
/// with `CASA-ADP-002`. Rules inside are excluded from cascade-time
/// matching by the engine; this is the host-facing record of them.
pub fn collect_constraints(
    rules: &[CasaRule],
    reporter: &mut Reporter,
) -> Vec<ContainerConstraint> {
    let mut constraints = Vec::new();
    for rule in rules {
        match rule {
            CasaRule::Container(container) => {
                constraints.push(record(container, reporter));
                constraints.extend(collect_constraints(&container.rules, reporter));
            }
            CasaRule::Media(media) => {
                constraints.extend(collect_constraints(&media.rules, reporter));
            }
            CasaRule::Layer(layer) => {
                constraints.extend(collect_constraints(&layer.rules, reporter));
            }
            CasaRule::Supports(supports) => {
                constraints.extend(collect_constraints(&supports.rules, reporter));
            }
            _ => {}
        }
    }
    constraints
}

fn record(container: &ContainerRule, reporter: &mut Reporter) -> ContainerConstraint {
    reporter.emit(
        codes::ADAPTIVE_CONTAINER_DEFERRED,
        match &container.name {
            Some(name) => format!(
                "container rule `{name}` deferred to host: condition {}",
                container.condition
            ),
            None => format!(
                "container rule deferred to host: condition {}",
                container.condition
            ),
        },
        None,
    );
    ContainerConstraint {
        name: container.name.clone(),
        condition: container.condition.clone(),
        gated_selectors: gated_selectors(&container.rules),
    }
}

/// Selector texts of all style rules directly or transitively inside a
/// rule group, deduplicated and in source order.
pub fn gated_selectors(rules: &[CasaRule]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut selectors = Vec::new();
    for rule in rules {
        let texts: Vec<String> = match rule {
            CasaRule::Style(style) => style.selectors.iter().map(|s| s.text.clone()).collect(),
            CasaRule::Container(c) => gated_selectors(&c.rules),
            CasaRule::Media(m) => gated_selectors(&m.rules),
            CasaRule::Layer(l) => gated_selectors(&l.rules),
            CasaRule::Supports(s) => gated_selectors(&s.rules),
            _ => Vec::new(),
        };
        for text in texts {
            if seen.insert(text.clone()) {
                selectors.push(text);
            }
        }
    }
    selectors
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
    fn container_rule_becomes_constraint() {
        let mut reporter = Reporter::new();
        let rules = lower("@container card (min-width: 400px) { .a { color: red } }");
        let constraints = collect_constraints(&rules, &mut reporter);
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].name.as_deref(), Some("card"));
        assert_eq!(constraints[0].condition, "(min-width: 400px)");
        assert_eq!(constraints[0].gated_selectors, vec![".a".to_string()]);
        assert_eq!(reporter.len(), 1);
    }

    #[test]
    fn anonymous_container_has_no_name() {
        let mut reporter = Reporter::new();
        let rules = lower("@container (width > 300px) { .b { color: blue } }");
        let constraints = collect_constraints(&rules, &mut reporter);
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].name, None);
    }

    #[test]
    fn nested_containers_collect_both() {
        let mut reporter = Reporter::new();
        let rules = lower(
            "@container card (min-width: 400px) { @container (min-height: 100px) { .c { color: red } } }",
        );
        let constraints = collect_constraints(&rules, &mut reporter);
        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0].name.as_deref(), Some("card"));
        assert_eq!(constraints[1].name, None);
        // Gated selectors are transitive: the outer container's set
        // includes the inner rule it gates, and the inner names its own.
        assert_eq!(constraints[0].gated_selectors, vec![".c".to_string()]);
        assert_eq!(constraints[1].gated_selectors, vec![".c".to_string()]);
    }

    #[test]
    fn plain_rules_yield_no_constraints() {
        let mut reporter = Reporter::new();
        let rules = lower(".a { color: red } @media (min-width: 100px) { .b { color: blue } }");
        assert!(collect_constraints(&rules, &mut reporter).is_empty());
        assert!(reporter.is_empty());
    }
}
