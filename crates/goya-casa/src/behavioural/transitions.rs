//! Transition parsing (directive §12): `transition` and its longhands
//! into typed [`Transition`] values. Easing reuses the animation family's
//! [`Easing`] vocabulary so keyframes and transitions share one timing
//! model; unparsable values fall back to the CSS default (linear) and are
//! reported.

use crate::animation::Easing;
use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::DeclaredProperty;

/// One transition, fully defaulted: `transition: color 300ms ease-in 100ms`.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// Property the transition watches (`all` by default).
    pub property: String,
    /// Duration in seconds (0 for instant).
    pub duration: f64,
    pub easing: Easing,
    /// Delay in seconds.
    pub delay: f64,
}

impl Default for Transition {
    fn default() -> Self {
        Transition {
            property: "all".to_string(),
            duration: 0.0,
            easing: Easing::Linear,
            delay: 0.0,
        }
    }
}

/// Parse a declaration list's transition properties. Shorthand and
/// longhands combine: longhands win when both are present, lists pair by
/// index with defaults filling gaps, per CSS Transitions.
pub fn parse(declarations: &[DeclaredProperty], reporter: &mut Reporter) -> Vec<Transition> {
    let mut shorthand: Vec<Transition> = Vec::new();
    let mut property_list: Option<Vec<String>> = None;
    let mut duration_list: Option<Vec<f64>> = None;
    let mut easing_list: Option<Vec<Easing>> = None;
    let mut delay_list: Option<Vec<f64>> = None;

    for declaration in declarations {
        match declaration.name.to_ascii_lowercase().as_str() {
            "transition" => {
                shorthand = parse_shorthand(&declaration.value, reporter);
            }
            "transition-property" => {
                property_list = Some(
                    split_top_level(&declaration.value)
                        .into_iter()
                        .map(|name| normalize_property(&name))
                        .collect(),
                );
            }
            "transition-duration" => {
                duration_list = Some(parse_time_list(&declaration.value, reporter));
            }
            "transition-timing-function" => {
                easing_list = Some(parse_easing_list(&declaration.value, reporter));
            }
            "transition-delay" => {
                delay_list = Some(parse_time_list(&declaration.value, reporter));
            }
            _ => {}
        }
    }

    let has_longhands = property_list.is_some()
        || duration_list.is_some()
        || easing_list.is_some()
        || delay_list.is_some();
    if !has_longhands {
        return shorthand;
    }
    let count = shorthand
        .len()
        .max(property_list.as_ref().map_or(0, Vec::len))
        .max(duration_list.as_ref().map_or(0, Vec::len))
        .max(easing_list.as_ref().map_or(0, Vec::len))
        .max(delay_list.as_ref().map_or(0, Vec::len));
    (0..count)
        .map(|index| Transition {
            property: property_list
                .as_ref()
                .and_then(|list| list.get(index).cloned())
                .or_else(|| shorthand.get(index).map(|t| t.property.clone()))
                .unwrap_or_else(|| "all".to_string()),
            duration: duration_list
                .as_ref()
                .and_then(|list| list.get(index).copied())
                .or_else(|| shorthand.get(index).map(|t| t.duration))
                .unwrap_or(0.0),
            easing: easing_list
                .as_ref()
                .and_then(|list| list.get(index).copied())
                .or_else(|| shorthand.get(index).map(|t| t.easing))
                .unwrap_or(Easing::Linear),
            delay: delay_list
                .as_ref()
                .and_then(|list| list.get(index).copied())
                .or_else(|| shorthand.get(index).map(|t| t.delay))
                .unwrap_or(0.0),
        })
        .collect()
}

/// `transition: <property> <duration> <easing> <delay>` per comma group;
/// the first time is the duration, the second the delay, easing anywhere.
fn parse_shorthand(value: &str, reporter: &mut Reporter) -> Vec<Transition> {
    split_top_level(value)
        .into_iter()
        .map(|group| {
            let mut transition = Transition::default();
            let mut times: Vec<f64> = Vec::new();
            for token in tokens_with_functions(&group) {
                if let Some(seconds) = parse_time(&token) {
                    times.push(seconds);
                } else if let Some(easing) = Easing::parse(&token) {
                    transition.easing = easing;
                } else {
                    transition.property = normalize_property(&token);
                }
            }
            match times.as_slice() {
                [] => {}
                [duration] => transition.duration = *duration,
                [duration, delay, ..] => {
                    transition.duration = *duration;
                    transition.delay = *delay;
                }
            }
            if group.trim().is_empty() {
                reporter.emit(
                    codes::SYNTAX_INVALID,
                    "empty transition shorthand group".to_string(),
                    None,
                );
            }
            transition
        })
        .collect()
}

fn parse_easing_list(value: &str, reporter: &mut Reporter) -> Vec<Easing> {
    split_top_level(value)
        .into_iter()
        .map(|group| {
            let text = group.trim();
            match Easing::parse(text) {
                Some(easing) => easing,
                None => {
                    reporter.emit(
                        codes::ANIMATION_UNINTERPOLABLE,
                        format!("unsupported transition timing function: `{text}`"),
                        None,
                    );
                    Easing::Linear
                }
            }
        })
        .collect()
}

fn parse_time_list(value: &str, reporter: &mut Reporter) -> Vec<f64> {
    split_top_level(value)
        .into_iter()
        .map(|group| {
            let text = group.trim();
            match parse_time(text) {
                Some(seconds) => seconds,
                None => {
                    reporter.emit(
                        codes::SYNTAX_INVALID,
                        format!("invalid transition time: `{text}`"),
                        None,
                    );
                    0.0
                }
            }
        })
        .collect()
}

/// Whitespace split that keeps parenthesised functions
/// (`cubic-bezier(0.25, 0.1, 0.25, 1)`) as single tokens.
fn tokens_with_functions(group: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in group.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current);
    }
    tokens
}

/// `250ms` → 0.25, `1s` → 1.0; other units or junk → None.
fn parse_time(token: &str) -> Option<f64> {
    let token = token.trim();
    let (number, unit) = token.split_at(token.find(|c: char| c.is_ascii_alphabetic())?);
    let value: f64 = number.parse().ok()?;
    match unit.to_ascii_lowercase().as_str() {
        "s" => Some(value),
        "ms" => Some(value / 1000.0),
        _ => None,
    }
}

fn normalize_property(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Split on commas not inside parentheses (e.g. `cubic-bezier(0, 0, 1, 1)`).
fn split_top_level(value: &str) -> Vec<String> {
    let mut groups = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in value.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' if depth == 0 => {
                groups.push(current.clone());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    groups.push(current);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::reporter::Reporter;

    fn decl(name: &str, value: &str) -> DeclaredProperty {
        DeclaredProperty {
            name: name.to_string(),
            value: value.to_string(),
            important: false,
            start: 0,
            end: 0,
            category: crate::ir::value_categories::ValueCategory::Declared,
        }
    }

    #[test]
    fn parses_full_shorthand() {
        let mut reporter = Reporter::new();
        let transitions = parse(
            &[decl("transition", "color 300ms ease-in 100ms")],
            &mut reporter,
        );
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].property, "color");
        assert!((transitions[0].duration - 0.3).abs() < 1e-9);
        assert_eq!(transitions[0].easing, Easing::EaseIn);
        assert!((transitions[0].delay - 0.1).abs() < 1e-9);
        assert!(reporter.is_empty());
    }

    #[test]
    fn minimal_shorthand_defaults() {
        let mut reporter = Reporter::new();
        let transitions = parse(&[decl("transition", "opacity")], &mut reporter);
        assert_eq!(transitions.len(), 1);
        assert_eq!(
            transitions[0],
            Transition {
                property: "opacity".to_string(),
                duration: 0.0,
                easing: Easing::Linear,
                delay: 0.0,
            }
        );
    }

    #[test]
    fn comma_separated_shorthand_groups() {
        let mut reporter = Reporter::new();
        let transitions = parse(
            &[decl("transition", "color 200ms, transform 400ms ease-out")],
            &mut reporter,
        );
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].property, "color");
        assert!((transitions[0].duration - 0.2).abs() < 1e-9);
        assert_eq!(transitions[1].property, "transform");
        assert_eq!(transitions[1].easing, Easing::EaseOut);
    }

    #[test]
    fn cubic_bezier_with_commas_survives_splitting() {
        let mut reporter = Reporter::new();
        let transitions = parse(
            &[decl(
                "transition",
                "width 500ms cubic-bezier(0.25, 0.1, 0.25, 1)",
            )],
            &mut reporter,
        );
        assert_eq!(transitions.len(), 1);
        assert_eq!(
            transitions[0].easing,
            Easing::CubicBezier {
                x1: 0.25,
                y1: 0.1,
                x2: 0.25,
                y2: 1.0
            }
        );
    }

    #[test]
    fn longhands_pair_by_index() {
        let mut reporter = Reporter::new();
        let transitions = parse(
            &[
                decl("transition-property", "color, opacity"),
                decl("transition-duration", "200ms, 400ms"),
                decl("transition-timing-function", "ease-in, linear"),
            ],
            &mut reporter,
        );
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].property, "color");
        assert!((transitions[0].duration - 0.2).abs() < 1e-9);
        assert_eq!(transitions[0].easing, Easing::EaseIn);
        assert_eq!(transitions[1].property, "opacity");
        assert!((transitions[1].duration - 0.4).abs() < 1e-9);
        assert_eq!(transitions[1].easing, Easing::Linear);
    }

    #[test]
    fn longhands_fill_missing_with_defaults() {
        let mut reporter = Reporter::new();
        let transitions = parse(
            &[decl("transition-property", "color, opacity")],
            &mut reporter,
        );
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].duration, 0.0);
        assert_eq!(transitions[0].easing, Easing::Linear);
        assert_eq!(transitions[1].property, "opacity");
    }

    #[test]
    fn non_transition_declarations_ignored() {
        let mut reporter = Reporter::new();
        let transitions = parse(
            &[decl("color", "red"), decl("margin", "4px")],
            &mut reporter,
        );
        assert!(transitions.is_empty());
        assert!(reporter.is_empty());
    }

    #[test]
    fn invalid_time_reported_as_zero() {
        let mut reporter = Reporter::new();
        let transitions = parse(&[decl("transition-duration", "fast")], &mut reporter);
        assert_eq!(transitions, vec![Transition::default()]);
        assert_eq!(reporter.len(), 1);
    }
}
