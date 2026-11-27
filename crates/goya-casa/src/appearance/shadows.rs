//! CSS box-shadow parsing for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use super::colors::{self, Color};
use super::{absolute_px, meaningful};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// One box shadow, offsets and radii resolved to px. `color` is `None`
/// for CSS `currentcolor`.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    pub x_offset_px: f64,
    pub y_offset_px: f64,
    pub blur_radius_px: f64,
    pub spread_radius_px: f64,
    pub color: Option<Color>,
    pub inset: bool,
}

/// Parse one computed `box-shadow` value (`<x> <y> <blur>? <spread>?
/// <color>?` with optional leading/trailing `inset`). `none` means no
/// shadow and returns `None`; a comma-separated list is not a single
/// shadow and is rejected.
pub fn parse(value: &str) -> Option<Shadow> {
    let values = parse_component_values(value);
    let meaningful = meaningful(&values);
    if meaningful.is_empty() {
        return None;
    }
    if let [ComponentValue::Token(Token::Ident(name), _)] = meaningful[..] {
        if name.eq_ignore_ascii_case("none") {
            return None;
        }
    }
    let mut inset = false;
    let mut lengths = Vec::new();
    let mut color = None;
    let mut saw_color = false;
    for value in &meaningful {
        if let Some(px) = absolute_px(value) {
            lengths.push(px);
            continue;
        }
        if let Some(color_value) = colors::from_component(value) {
            // A color can appear before or after the lengths, once.
            if saw_color {
                return None;
            }
            saw_color = true;
            color = Some(color_value);
            continue;
        }
        if let ComponentValue::Token(Token::Ident(name), _) = value {
            if name.eq_ignore_ascii_case("inset") && !inset {
                inset = true;
                continue;
            }
        }
        return None;
    }
    if !(2..=4).contains(&lengths.len()) {
        return None;
    }
    Some(Shadow {
        x_offset_px: lengths[0],
        y_offset_px: lengths[1],
        blur_radius_px: lengths.get(2).copied().unwrap_or(0.0),
        spread_radius_px: lengths.get(3).copied().unwrap_or(0.0),
        color,
        inset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_shadow_defaults_radii() {
        let shadow = parse("2px 3px").unwrap();
        assert_eq!(shadow.x_offset_px, 2.0);
        assert_eq!(shadow.y_offset_px, 3.0);
        assert_eq!(shadow.blur_radius_px, 0.0);
        assert_eq!(shadow.spread_radius_px, 0.0);
        assert_eq!(shadow.color, None);
        assert!(!shadow.inset);
    }

    #[test]
    fn blur_spread_and_color_parse() {
        let shadow = parse("2px 2px 4px 1px rgba(0, 0, 0, 0.5)").unwrap();
        assert_eq!(shadow.blur_radius_px, 4.0);
        assert_eq!(shadow.spread_radius_px, 1.0);
        assert_eq!(
            shadow.color,
            Some(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0.5
            })
        );
    }

    #[test]
    fn inset_and_leading_color() {
        let shadow = parse("inset 0 0 5px").unwrap();
        assert!(shadow.inset);
        assert_eq!(shadow.blur_radius_px, 5.0);
        let shadow = parse("red 1px 1px").unwrap();
        assert_eq!(shadow.color, Some(Color::opaque(255, 0, 0)));
        let shadow = parse("1px 1px red").unwrap();
        assert_eq!(shadow.color, Some(Color::opaque(255, 0, 0)));
    }

    #[test]
    fn none_means_no_shadow() {
        assert_eq!(parse("none"), None);
    }

    #[test]
    fn malformed_shadows_are_rejected() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("1px"), None);
        assert_eq!(parse("1px 2px 3px 4px 5px"), None);
        assert_eq!(parse("1px 2px inset inset"), None);
        assert_eq!(parse("1px 2px currentcolor"), None);
        assert_eq!(parse("1px 2px, 3px 3px"), None);
        assert_eq!(parse("auto 2px"), None);
    }
}
