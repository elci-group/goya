//! Condition evaluation for conditional group rules (directive §13):
//! `@media` and `@supports` subsets. Unevaluable conditions are reported
//! and treated as non-matching — never silently guessed.

use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// The environmental state conditions are evaluated against (§13.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Environment {
    /// Viewport width in CSS px, when known.
    pub viewport_width: Option<f64>,
    /// Viewport height in CSS px, when known.
    pub viewport_height: Option<f64>,
    pub media_type: MediaType,
    /// `prefers-color-scheme`, when the platform exposes it.
    pub color_scheme: Option<ColorScheme>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaType {
    #[default]
    Screen,
    Print,
    Speech,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// Evaluate a `@media` condition. `Err` names the first feature this
/// evaluator cannot judge; the caller excludes the rule and reports.
pub fn evaluate_media(condition: &str, environment: &Environment) -> Result<bool, String> {
    let values = parse_component_values(condition);
    eval_or_list(&values, environment)
}

/// `a, b` is an OR of ANDed terms; `and`/`or` idents and `not` prefix,
/// where `not` negates its whole AND group.
fn eval_or_list(values: &[ComponentValue], env: &Environment) -> Result<bool, String> {
    for group in split_on(values, is_or_separator) {
        let (negate, rest) = match group.first() {
            Some(value) if is_ident(value, "not") => (true, &group[1..]),
            _ => (false, group),
        };
        let mut matched = true;
        let mut empty = true;
        for term in split_on(rest, |value| is_ident(value, "and")) {
            if term.iter().all(is_whitespace) {
                continue;
            }
            empty = false;
            if !eval_term(term, env)? {
                matched = false;
            }
        }
        if empty {
            continue;
        }
        if matched != negate {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_or_separator(value: &ComponentValue) -> bool {
    is_ident(value, "or") || matches!(value, ComponentValue::Token(Token::Comma, _))
}

fn is_ident(value: &ComponentValue, name: &str) -> bool {
    matches!(value, ComponentValue::Token(Token::Ident(id), _) if id.eq_ignore_ascii_case(name))
}

fn is_whitespace(value: &ComponentValue) -> bool {
    matches!(value, ComponentValue::Token(Token::Whitespace, _))
}

/// One term: a media type, or a parenthesised feature.
fn eval_term(term: &[ComponentValue], env: &Environment) -> Result<bool, String> {
    let mut matched = false;
    for value in term {
        match value {
            ComponentValue::Token(Token::Whitespace, _) => {}
            ComponentValue::Token(Token::Ident(name), _) => {
                if name.eq_ignore_ascii_case("only") {
                    continue;
                }
                if matched {
                    return Err(format!("multiple media types in one term: {name}"));
                }
                matched = eval_media_type(name, env)?;
            }
            ComponentValue::Block(block) if block.open == '(' => {
                if matched {
                    return Err("media type combined with a feature in one term".to_string());
                }
                matched = eval_feature(&block.contents, env)?;
            }
            other => {
                return Err(format!("unsupported media condition token: {other:?}"));
            }
        }
    }
    Ok(matched)
}

fn eval_media_type(name: &str, env: &Environment) -> Result<bool, String> {
    match name.to_ascii_lowercase().as_str() {
        "all" => Ok(true),
        "screen" => Ok(env.media_type == MediaType::Screen),
        "print" => Ok(env.media_type == MediaType::Print),
        "speech" => Ok(env.media_type == MediaType::Speech),
        other => Err(format!("unsupported media type: {other}")),
    }
}

/// One `(feature: value)` condition inside its parens.
fn eval_feature(contents: &[ComponentValue], env: &Environment) -> Result<bool, String> {
    let meaningful: Vec<&ComponentValue> = contents.iter().filter(|v| !is_whitespace(v)).collect();
    let Some((ComponentValue::Token(Token::Ident(name), _), rest)) = meaningful.split_first()
    else {
        return Err("media feature without a name".to_string());
    };
    let feature = name.to_ascii_lowercase();
    let value = match rest.first() {
        Some(ComponentValue::Token(Token::Colon, _)) => &rest[1..],
        None => {
            // Boolean context: `(feature)` — true for supported features.
            return matches!(
                feature.as_str(),
                "width" | "height" | "prefers-color-scheme"
            )
            .then_some(true)
            .ok_or_else(|| format!("unsupported media feature: {feature}"));
        }
        Some(other) => return Err(format!("malformed media feature: {feature}: {other:?}")),
    };
    match feature.as_str() {
        "width" => compare_width("width", value, env, &|a, b| a == b),
        "height" => compare_width("height", value, env, &|a, b| a == b),
        "min-width" => compare_width("width", value, env, &|a, b| a >= b),
        "max-width" => compare_width("width", value, env, &|a, b| a <= b),
        "min-height" => compare_width("height", value, env, &|a, b| a >= b),
        "max-height" => compare_width("height", value, env, &|a, b| a <= b),
        "prefers-color-scheme" => {
            let expected = single_ident(value, &feature)?;
            let Some(scheme) = env.color_scheme else {
                return Ok(false); // feature known, platform exposes nothing
            };
            Ok(matches!(
                (expected.as_str(), scheme),
                ("light", ColorScheme::Light) | ("dark", ColorScheme::Dark)
            ))
        }
        other => Err(format!("unsupported media feature: {other}")),
    }
}

fn compare_width(
    axis: &str,
    value: &[&ComponentValue],
    env: &Environment,
    predicate: &dyn Fn(f64, f64) -> bool,
) -> Result<bool, String> {
    let viewport = match axis {
        "width" => env.viewport_width,
        _ => env.viewport_height,
    };
    let Some(viewport) = viewport else {
        return Ok(false); // known feature, unknown environment
    };
    let meaningful: Vec<&ComponentValue> = value
        .iter()
        .copied()
        .filter(|v| !is_whitespace(v))
        .collect();
    let Some((ComponentValue::Token(Token::Dimension { number, unit }, _), rest)) =
        meaningful.split_first()
    else {
        return Err(format!(
            "media feature value must be a px dimension: {axis}"
        ));
    };
    if !rest.is_empty() || !unit.eq_ignore_ascii_case("px") {
        return Err(format!("unsupported media value unit: {unit}"));
    }
    Ok(predicate(viewport, number.value))
}

fn single_ident(value: &[&ComponentValue], feature: &str) -> Result<String, String> {
    let meaningful: Vec<&ComponentValue> = value
        .iter()
        .copied()
        .filter(|v| !is_whitespace(v))
        .collect();
    match meaningful.as_slice() {
        [ComponentValue::Token(Token::Ident(name), _)] => Ok(name.to_ascii_lowercase()),
        _ => Err(format!("malformed value for media feature: {feature}")),
    }
}

/// Split a component-value slice on a top-level predicate, dropping the
/// separator tokens. `parse_component_values` keeps blocks intact, so
/// commas inside `( ... )` never split.
fn split_on(
    values: &[ComponentValue],
    predicate: impl Fn(&ComponentValue) -> bool,
) -> Vec<&[ComponentValue]> {
    let mut groups = Vec::new();
    let mut start = 0;
    for (index, value) in values.iter().enumerate() {
        if predicate(value) {
            groups.push(&values[start..index]);
            start = index + 1;
        }
    }
    groups.push(&values[start..]);
    groups
}

/// Evaluate a `@supports` condition: `(property: value)` existence
/// checks against the known property set, combined with `and`, `or`,
/// `not` and nested parentheses per CSS Conditional 3.
pub fn evaluate_supports(condition: &str) -> Result<bool, String> {
    let values = parse_component_values(condition);
    eval_supports_or(&values)
}

fn eval_supports_or(values: &[ComponentValue]) -> Result<bool, String> {
    let mut matched = false;
    let mut empty = true;
    for group in split_on(values, is_supports_or_separator) {
        if group.iter().all(is_whitespace) {
            continue;
        }
        empty = false;
        if eval_supports_and(group)? {
            matched = true;
        }
    }
    if empty {
        return Err("empty @supports condition".to_string());
    }
    Ok(matched)
}

fn is_supports_or_separator(value: &ComponentValue) -> bool {
    is_ident(value, "or") || matches!(value, ComponentValue::Token(Token::Comma, _))
}

fn eval_supports_and(values: &[ComponentValue]) -> Result<bool, String> {
    let mut matched = true;
    let mut empty = true;
    for term in split_on(values, |value| is_ident(value, "and")) {
        if term.iter().all(is_whitespace) {
            continue;
        }
        empty = false;
        let (negate, rest) = match term.iter().position(|value| !is_whitespace(value)) {
            Some(position) if is_ident(&term[position], "not") => (true, &term[position + 1..]),
            _ => (false, term),
        };
        if eval_supports_term(rest)? == negate {
            matched = false;
        }
    }
    if empty {
        return Err("empty @supports condition".to_string());
    }
    Ok(matched)
}

/// One `@supports` term: a parenthesised property check, a `selector()`
/// query, or a nested condition.
fn eval_supports_term(term: &[ComponentValue]) -> Result<bool, String> {
    let meaningful: Vec<&ComponentValue> = term.iter().filter(|v| !is_whitespace(v)).collect();
    match meaningful.as_slice() {
        [ComponentValue::Block(block), ..] if block.open == '(' => {
            let inner: Vec<&ComponentValue> = block
                .contents
                .iter()
                .filter(|v| !is_whitespace(v))
                .collect();
            match inner.as_slice() {
                [ComponentValue::Token(Token::Ident(name), _), ComponentValue::Token(Token::Colon, _), ..] => {
                    Ok(crate::cascade::inheritance::is_known_property(name))
                }
                [ComponentValue::Function { name, .. }, ..]
                    if name.eq_ignore_ascii_case("selector") =>
                {
                    // Selector support queries are always supported here:
                    // the matching engine drives them.
                    Ok(true)
                }
                // A nested condition recurses through the same grammar.
                [single] => eval_supports_or(std::slice::from_ref(single)),
                _ => Err("unsupported @supports condition term".to_string()),
            }
        }
        _ => Err("unsupported @supports condition term".to_string()),
    }
}

/// Report an unevaluable condition as a diagnostic and treat the rule as
/// non-matching (the safe default for scene conversion).
pub fn report_unevaluable(reporter: &mut Reporter, kind: &str, condition: &str) {
    reporter.emit(
        codes::CASCADE_UNEVALUABLE,
        format!("{kind} condition not evaluated, rule excluded: {condition}"),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Environment {
        Environment {
            viewport_width: Some(800.0),
            viewport_height: Some(600.0),
            media_type: MediaType::Screen,
            color_scheme: Some(ColorScheme::Dark),
        }
    }

    #[test]
    fn width_features_match_against_viewport() {
        assert!(evaluate_media("(min-width: 400px)", &env()).unwrap());
        assert!(!evaluate_media("(min-width: 900px)", &env()).unwrap());
        assert!(evaluate_media("(max-width: 800px)", &env()).unwrap());
        assert!(!evaluate_media("(max-height: 100px)", &env()).unwrap());
        assert!(evaluate_media("(width: 800px)", &env()).unwrap());
    }

    #[test]
    fn non_px_units_error() {
        assert!(evaluate_media("(min-width: 40em)", &env()).is_err());
    }

    #[test]
    fn media_type_and_color_scheme() {
        assert!(evaluate_media("screen", &env()).unwrap());
        assert!(!evaluate_media("print", &env()).unwrap());
        assert!(evaluate_media("print, screen", &env()).unwrap());
        assert!(evaluate_media("all and (min-width: 100px)", &env()).unwrap());
        assert!(evaluate_media("(prefers-color-scheme: dark)", &env()).unwrap());
        assert!(!evaluate_media("(prefers-color-scheme: light)", &env()).unwrap());
    }

    #[test]
    fn boolean_algebra_in_conditions() {
        assert!(evaluate_media("(min-width: 100px) and (max-width: 900px)", &env()).unwrap());
        assert!(!evaluate_media("(min-width: 100px) and (max-width: 500px)", &env()).unwrap());
        assert!(evaluate_media(
            "(min-width: 1000px), (min-width: 100px) and (max-width: 900px)",
            &env()
        )
        .unwrap());
        assert!(evaluate_media("not screen and (min-width: 900px)", &env()).unwrap());
        assert!(evaluate_media("not print", &env()).unwrap());
    }

    #[test]
    fn unknown_features_error_not_guess() {
        assert!(evaluate_media("(grid: 1)", &env()).is_err());
    }

    #[test]
    fn unknown_environment_makes_features_false() {
        let unknown = Environment::default();
        assert!(!evaluate_media("(min-width: 100px)", &unknown).unwrap());
        assert!(!evaluate_media("(prefers-color-scheme: dark)", &unknown).unwrap());
    }

    #[test]
    fn supports_checks_known_properties() {
        assert!(evaluate_supports("(color: red)").unwrap());
        assert!(evaluate_supports("(display: grid)").unwrap());
        assert!(!evaluate_supports("(not-a-property: red)").unwrap());
        assert!(evaluate_supports("(color: red) and (display: grid)").unwrap());
        assert!(!evaluate_supports("(color: red) and (nope: 1)").unwrap());
    }
}
