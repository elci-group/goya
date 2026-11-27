//! `@media` evaluation into typed records (directive §13.1).
//!
//! Wraps the cascade engine's `evaluate_media` with a compile-time summary:
//! which features the condition consults, whether they are evaluable now or
//! deferred to the host, and the boolean verdict under a known environment.
//! Deferred features count as non-matching here — the host re-evaluates them.

use std::collections::BTreeSet;

use crate::adaptive::env_registry::{self, Support};
use crate::cascade::conditional::{evaluate_media, Environment};
use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;

/// A `@media` condition's fate under one environment snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaQueryRecord {
    pub condition: String,
    pub matched: bool,
    /// Features the condition consults that only the host can judge.
    pub deferred_features: BTreeSet<String>,
    /// True when the whole condition was judged (no deferred feature
    /// influenced the verdict). Deferred conditions re-evaluate at runtime.
    pub evaluated: bool,
}

/// Evaluate one `@media` condition, reporting unsupported features.
pub fn evaluate(
    condition: &str,
    environment: &Environment,
    reporter: &mut Reporter,
) -> MediaQueryRecord {
    let mut deferred_features = BTreeSet::new();
    let mut unsupported = false;
    for feature in extract_features(condition) {
        if env_registry::support(&feature) == Support::Evaluable {
            continue;
        }
        if env_registry::is_registered(&feature) {
            deferred_features.insert(feature);
        } else {
            unsupported = true;
            reporter.emit(
                codes::ADAPTIVE_FEATURE_UNSUPPORTED,
                format!("unsupported media feature: {feature}"),
                None,
            );
        }
    }

    let evaluated = deferred_features.is_empty() && !unsupported;
    if !deferred_features.is_empty() {
        reporter.emit(
            codes::ADAPTIVE_CONTAINER_DEFERRED,
            format!(
                "media condition consults host-only features: {}",
                deferred_features
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        );
    }

    match evaluate_media(condition, environment) {
        Ok(matched) => MediaQueryRecord {
            condition: condition.to_string(),
            matched,
            deferred_features,
            evaluated,
        },
        Err(_reason) => {
            // Unevaluable: deferred or unsupported feature, unknown media
            // type, or malformed condition — non-matching at transform time.
            MediaQueryRecord {
                condition: condition.to_string(),
                matched: false,
                deferred_features,
                evaluated: false,
            }
        }
    }
}

/// Feature names appearing in parenthesised groups: `(min-width: 400px)`
/// yields `min-width`. Media types outside parens are not features.
fn extract_features(condition: &str) -> Vec<String> {
    let bytes = condition.as_bytes();
    let mut features = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => {
                depth += 1;
                if depth == 1 {
                    start = Some(i + 1);
                }
            }
            b')' => {
                if depth == 1 {
                    if let Some(s) = start {
                        if let Some(name) = leading_ident(&condition[s..i]) {
                            features.push(name);
                        }
                    }
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    features
}

/// The identifier opening a feature group, if any (`width` from
/// `width: 100px`, `not (hover)` yields `hover`).
fn leading_ident(text: &str) -> Option<String> {
    let text = text.trim();
    let text = text
        .strip_prefix("not ")
        .map(str::trim_start)
        .unwrap_or(text);
    let name: String = text
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if name.is_empty() || name == "not" {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cascade::conditional::{ColorScheme, MediaType};
    use crate::diagnostics::reporter::Reporter;

    fn screen_env() -> Environment {
        Environment {
            viewport_width: Some(800.0),
            viewport_height: Some(600.0),
            media_type: MediaType::Screen,
            color_scheme: Some(ColorScheme::Dark),
        }
    }

    #[test]
    fn matched_simple_condition() {
        let mut reporter = Reporter::new();
        let record = evaluate("(min-width: 500px)", &screen_env(), &mut reporter);
        assert!(record.matched);
        assert!(record.evaluated);
        assert!(record.deferred_features.is_empty());
        assert_eq!(record.condition, "(min-width: 500px)");
    }

    #[test]
    fn unmatched_condition() {
        let mut reporter = Reporter::new();
        let record = evaluate("(min-width: 1200px)", &screen_env(), &mut reporter);
        assert!(!record.matched);
        assert!(record.evaluated);
    }

    #[test]
    fn host_feature_is_deferred() {
        let mut reporter = Reporter::new();
        let record = evaluate(
            "(min-width: 400px) and (hover: hover)",
            &screen_env(),
            &mut reporter,
        );
        assert!(!record.matched);
        assert!(!record.evaluated);
        assert_eq!(
            record.deferred_features.iter().cloned().collect::<Vec<_>>(),
            vec!["hover".to_string()]
        );
    }

    #[test]
    fn unknown_feature_reported_and_non_matching() {
        let mut reporter = Reporter::new();
        let record = evaluate("(gusto: 3)", &screen_env(), &mut reporter);
        assert!(!record.matched);
        assert!(!record.evaluated);
        assert_eq!(reporter.diagnostics().len(), 1);
        assert_eq!(
            reporter.diagnostics()[0].code.as_str(),
            codes::ADAPTIVE_FEATURE_UNSUPPORTED.code
        );
    }

    #[test]
    fn extract_features_finds_all_groups() {
        let features = extract_features("(min-width: 400px), (orientation: landscape) and (color)");
        assert_eq!(
            features,
            vec![
                "min-width".to_string(),
                "orientation".to_string(),
                "color".to_string()
            ]
        );
    }
}
