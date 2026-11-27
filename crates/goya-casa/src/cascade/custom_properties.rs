//! Custom property substitution with cycle detection (directive §7.5.3).
//!
//! Values are spliced from token spans: `var(--x)` is replaced by the
//! resolved value of `--x`, `var(--x, fallback)` by the fallback when
//! `--x` is undefined. A cycle resolves to nothing and is reported as
//! `CASA-CAS-001`; a reference to an undefined property without a
//! fallback makes the value invalid at computed-value time
//! (`CASA-CAS-002`).

use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;
use goya_css_syntax::{parse_component_values, ComponentValue, Token};
use std::collections::BTreeMap;

/// Custom properties resolved for one element, ready for substitution.
pub type CustomProperties = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq)]
pub enum Substitution {
    /// The value with all references replaced.
    Resolved(String),
    /// A `var()` cycle was cut: the value is invalid at computed-value time.
    Cycle { property: String },
    /// An undefined property without fallback: invalid at computed-value time.
    Undefined { property: String },
}

/// Substitute `var()` references in one declared value.
pub fn substitute(
    value: &str,
    properties: &CustomProperties,
    reporter: &mut Reporter,
) -> Substitution {
    let mut visiting: Vec<String> = Vec::new();
    substitute_inner(value, properties, &mut visiting, reporter)
}

fn substitute_inner(
    value: &str,
    properties: &CustomProperties,
    visiting: &mut Vec<String>,
    reporter: &mut Reporter,
) -> Substitution {
    match resolve_value(value, properties, visiting, reporter) {
        Ok(text) => Substitution::Resolved(text),
        Err(failure) => {
            let code = match failure {
                Failure::Cycle { property } => {
                    // Report once per property chain, not once per nesting.
                    reporter.emit(
                        codes::CASCADE_PROPERTY_CYCLE,
                        format!(
                            "custom property cycle at --{property}: {}",
                            visiting.join(" -> ")
                        ),
                        None,
                    );
                    Substitution::Cycle { property }
                }
                Failure::Undefined { property } => {
                    reporter.emit(
                        codes::CASCADE_INVALID_AT_COMPUTED_VALUE_TIME,
                        format!("undefined custom property {property} without fallback"),
                        None,
                    );
                    Substitution::Undefined { property }
                }
            };
            code
        }
    }
}

enum Failure {
    Cycle { property: String },
    Undefined { property: String },
}

/// Walk component values, splicing source text outside `var(...)` and
/// resolved text inside it.
fn resolve_value(
    value: &str,
    properties: &CustomProperties,
    visiting: &mut Vec<String>,
    _reporter: &mut Reporter,
) -> Result<String, Failure> {
    let values = parse_component_values(value);
    let mut out = String::new();
    let mut index = 0;
    while index < values.len() {
        match &values[index] {
            ComponentValue::Function {
                name, arguments, ..
            } if name.eq_ignore_ascii_case("var") => {
                let span = values[index].span();
                // Take over the raw source span to know what was consumed.
                let raw = &value[span.start.min(value.len())..span.end.min(value.len())];
                let (property, fallback) = split_var_arguments(raw);
                let property = property.trim().to_string();
                if let Some(cycle_member) = visiting.iter().find(|p| **p == property) {
                    return Err(Failure::Cycle {
                        property: cycle_member.trim_start_matches('-').to_string(),
                    });
                }
                match properties.get(&property) {
                    Some(defined) => {
                        visiting.push(property.clone());
                        let resolved = resolve_value(defined, properties, visiting, _reporter)?;
                        visiting.pop();
                        out.push_str(&resolved);
                    }
                    None => match fallback {
                        Some(fallback) => {
                            let resolved =
                                resolve_value(fallback, properties, visiting, _reporter)?;
                            out.push_str(resolved.trim());
                        }
                        None => return Err(Failure::Undefined { property }),
                    },
                }
                index += 1;
            }
            ComponentValue::Function { .. } => {
                // Non-`var` functions (e.g. calc) are spliced head-first
                // with their arguments substituted recursively.
                let span = values[index].span();
                let raw = &value[span.start.min(value.len())..span.end.min(value.len())];
                match (raw.find('('), raw.ends_with(')')) {
                    (Some(open), true) => {
                        out.push_str(&raw[..=open]);
                        let resolved = resolve_value(
                            &raw[open + 1..raw.len() - 1],
                            properties,
                            visiting,
                            _reporter,
                        )?;
                        out.push_str(&resolved);
                        out.push(')');
                    }
                    _ => out.push_str(raw),
                }
                index += 1;
            }
            ComponentValue::Token(Token::Whitespace, _) => {
                // Collapse runs of whitespace to one space, like CSS.
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
                index += 1;
            }
            other => {
                let span = other.span();
                out.push_str(&value[span.start.min(value.len())..span.end.min(value.len())]);
                index += 1;
            }
        }
    }
    Ok(out)
}

/// Split the raw `--name, fallback` payload of a `var(...)` function on
/// the first top-level comma.
fn split_var_arguments(raw: &str) -> (&str, Option<&str>) {
    let inner = raw
        .trim_start()
        .strip_prefix("var")
        .and_then(|rest| rest.trim_start().strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or(raw);
    match inner.split_once(',') {
        Some((name, fallback)) => (name, Some(fallback)),
        None => (inner, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties() -> CustomProperties {
        CustomProperties::from([
            ("--accent".to_string(), "rebeccapurple".to_string()),
            ("--gap".to_string(), "calc(4px + var(--unit))".to_string()),
            ("--unit".to_string(), "4px".to_string()),
        ])
    }

    fn resolved(value: &str) -> String {
        let mut reporter = Reporter::new();
        match substitute(value, &properties(), &mut reporter) {
            Substitution::Resolved(text) => {
                assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
                text
            }
            other => panic!("expected resolved, got {other:?}"),
        }
    }

    #[test]
    fn plain_values_pass_through() {
        assert_eq!(resolved("10px solid red"), "10px solid red");
        assert_eq!(resolved("var(--accent)"), "rebeccapurple");
        assert_eq!(resolved("1px var(--accent) 2px"), "1px rebeccapurple 2px");
    }

    #[test]
    fn nested_references_resolve_transitively() {
        assert_eq!(resolved("var(--gap)"), "calc(4px + 4px)");
        assert_eq!(resolved("var(--missing, 8px)"), "8px");
        assert_eq!(resolved("var(--missing, var(--accent))"), "rebeccapurple");
    }

    #[test]
    fn undefined_without_fallback_is_invalid() {
        let mut reporter = Reporter::new();
        let outcome = substitute("var(--gone)", &properties(), &mut reporter);
        assert!(matches!(outcome, Substitution::Undefined { property } if property == "--gone"));
        assert_eq!(
            reporter.diagnostics()[0].code,
            codes::CASCADE_INVALID_AT_COMPUTED_VALUE_TIME.code
        );
    }

    #[test]
    fn cycles_are_cut_and_reported() {
        let cyclic = CustomProperties::from([
            ("--a".to_string(), "var(--b)".to_string()),
            ("--b".to_string(), "var(--a)".to_string()),
        ]);
        let mut reporter = Reporter::new();
        let outcome = substitute("var(--a)", &cyclic, &mut reporter);
        assert!(matches!(outcome, Substitution::Cycle { .. }));
        assert_eq!(
            reporter.diagnostics()[0].code,
            codes::CASCADE_PROPERTY_CYCLE.code
        );
    }

    #[test]
    fn whitespace_collapses_outside_strings() {
        assert_eq!(resolved("10px    20px"), "10px 20px");
    }
}
