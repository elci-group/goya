//! CSS-to-scene appearance transformation (directive GOYA-DIS-CASA-001).
//!
//! Each submodule parses one family of computed CSS property values into
//! typed scene payloads. Parsing is deterministic and total in the Rust
//! sense: malformed or unsupported values yield `None`/an empty result,
//! never a guess. [`colors`] is the shared leaf every other submodule
//! reuses for color tokens; the helpers below are the shared token
//! plumbing over `goya_css_syntax`.

pub mod backgrounds;
pub mod borders;
pub mod colors;
pub mod deduplication;
pub mod filters;
pub mod gradients;
pub mod masks;
pub mod shadows;
pub mod transforms;

use crate::geometry::Length;
use goya_css_syntax::{ComponentValue, Token};

/// Component values with whitespace stripped, the standard first step of
/// every parser in this module.
pub(crate) fn meaningful(values: &[ComponentValue]) -> Vec<&ComponentValue> {
    values
        .iter()
        .filter(|value| !matches!(value, ComponentValue::Token(Token::Whitespace, _)))
        .collect()
}

/// Split on top-level commas, keeping empty groups so callers can reject
/// them explicitly (`rgb(1, 2, ,)` must not silently lose the gap).
pub(crate) fn split_comma(values: &[ComponentValue]) -> Vec<&[ComponentValue]> {
    split_delim(values, ',')
}

/// Split on a top-level delimiter (`','` or `'/'`), keeping empty groups.
pub(crate) fn split_delim(values: &[ComponentValue], delimiter: char) -> Vec<&[ComponentValue]> {
    let mut groups = Vec::new();
    let mut start = 0;
    for (position, value) in values.iter().enumerate() {
        let hits = match value {
            ComponentValue::Token(Token::Comma, _) if delimiter == ',' => true,
            ComponentValue::Token(Token::Delim(hit), _) if *hit == delimiter => true,
            _ => false,
        };
        if hits {
            groups.push(&values[start..position]);
            start = position + 1;
        }
    }
    groups.push(&values[start..]);
    groups
}

/// A plain `<number>` token value.
pub(crate) fn number(value: &ComponentValue) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Number(numeric), _) => Some(numeric.value),
        _ => None,
    }
}

/// A `<percentage>` token value (0-100 scale, not resolved).
pub(crate) fn percentage(value: &ComponentValue) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Percentage(numeric), _) => Some(numeric.value),
        _ => None,
    }
}

/// An absolute length in px: a unitless zero (the only unitless length CSS
/// allows) or a dimension in a context-free unit. Font-relative and
/// viewport units need a [`crate::geometry::Context`] and are rejected
/// here rather than guessed.
pub(crate) fn absolute_px(value: &ComponentValue) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Number(numeric), _) if numeric.value == 0.0 => Some(0.0),
        ComponentValue::Token(Token::Dimension { number, unit }, _) => {
            let px = match unit.to_ascii_lowercase().as_str() {
                "px" => number.value,
                "pt" => number.value * 96.0 / 72.0,
                "pc" => number.value * 96.0 / 72.0 * 12.0,
                "in" => number.value * 96.0,
                "cm" => number.value * 96.0 / 2.54,
                "mm" => number.value * 96.0 / 25.4,
                "q" => number.value * 96.0 / 101.6,
                _ => return None,
            };
            Some(px)
        }
        _ => None,
    }
}

/// A `<length-percentage>` as a symbolic [`Length`]: absolute units
/// resolve to px, percentages stay percent. Context-relative units are
/// rejected (no context is available at this layer).
pub(crate) fn length(value: &ComponentValue) -> Option<Length> {
    if let Some(px) = absolute_px(value) {
        return Some(Length::Px(px));
    }
    percentage(value).map(Length::Percent)
}

/// An `<angle>` in degrees: a unitless zero or a `deg`/`grad`/`rad`/`turn`
/// dimension.
pub(crate) fn angle_degrees(value: &ComponentValue) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Number(numeric), _) if numeric.value == 0.0 => Some(0.0),
        ComponentValue::Token(Token::Dimension { number, unit }, _) => {
            let degrees = match unit.to_ascii_lowercase().as_str() {
                "deg" => number.value,
                "grad" => number.value * 360.0 / 400.0,
                "rad" => number.value * 180.0 / std::f64::consts::PI,
                "turn" => number.value * 360.0,
                _ => return None,
            };
            Some(degrees)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_css_syntax::parse_component_values;

    fn single(value: &str) -> ComponentValue {
        let values = parse_component_values(value);
        assert_eq!(
            values.len(),
            1,
            "expected one component value for {value:?}"
        );
        values.into_iter().next().unwrap()
    }

    #[test]
    fn helpers_read_tokens() {
        assert_eq!(number(&single("42")), Some(42.0));
        assert_eq!(number(&single("42px")), None);
        assert_eq!(percentage(&single("50%")), Some(50.0));
        assert_eq!(absolute_px(&single("0")), Some(0.0));
        assert_eq!(absolute_px(&single("12px")), Some(12.0));
        assert_eq!(absolute_px(&single("1in")), Some(96.0));
        assert_eq!(absolute_px(&single("12")), None);
        assert_eq!(length(&single("50%")), Some(Length::Percent(50.0)));
        assert_eq!(length(&single("1em")), None);
    }

    #[test]
    fn angles_convert_to_degrees() {
        assert_eq!(angle_degrees(&single("0")), Some(0.0));
        assert_eq!(angle_degrees(&single("90deg")), Some(90.0));
        assert_eq!(angle_degrees(&single("100grad")), Some(90.0));
        assert!((angle_degrees(&single("0.25turn")).unwrap() - 90.0).abs() < 1e-9);
        assert!((angle_degrees(&single("1.5707963rad")).unwrap() - 90.0).abs() < 1e-5);
        assert_eq!(angle_degrees(&single("45")), None);
    }

    #[test]
    fn splitting_keeps_empty_groups() {
        let values = parse_component_values("1, ,2");
        let groups = split_comma(&values);
        assert_eq!(groups.len(), 3);
        assert_eq!(meaningful(groups[1]).len(), 0);

        let values = parse_component_values("red 20% / 0.5");
        let groups = split_delim(&values, '/');
        assert_eq!(groups.len(), 2);
    }
}
