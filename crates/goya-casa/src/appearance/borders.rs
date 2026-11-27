//! CSS border shorthand parsing for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use super::colors::{self, Color};
use super::{absolute_px, meaningful};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// The border paint styles CSS defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Hidden,
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

/// A parsed border: resolved width in px, the paint style, and the color
/// (`None` means CSS `currentcolor`, which the caller resolves against
/// the element's own color).
#[derive(Debug, Clone, PartialEq)]
pub struct Border {
    pub width_px: f64,
    pub style: BorderStyle,
    pub color: Option<Color>,
}

/// Parse one computed `border` shorthand value (`<line-width> ||
/// <line-style> || <color>`, components in any order, each at most
/// once). Omitted components fall back to the CSS initial values
/// (medium / none / currentcolor).
pub fn parse(value: &str) -> Option<Border> {
    let values = parse_component_values(value);
    let meaningful = meaningful(&values);
    if meaningful.is_empty() {
        return None;
    }
    let mut width = 3.0; // medium, the usual UA default
    let mut style = BorderStyle::None;
    let mut color = None;
    let mut saw_width = false;
    let mut saw_style = false;
    let mut saw_color = false;
    for value in &meaningful {
        if let Some(style_value) = style_keyword(value) {
            if saw_style {
                return None;
            }
            saw_style = true;
            style = style_value;
            continue;
        }
        if let Some(width_value) = width_px(value) {
            if saw_width {
                return None;
            }
            saw_width = true;
            width = width_value;
            continue;
        }
        if let Some(color_value) = colors::from_component(value) {
            if saw_color {
                return None;
            }
            saw_color = true;
            color = Some(color_value);
            continue;
        }
        return None;
    }
    Some(Border {
        width_px: width,
        style,
        color,
    })
}

/// A `<line-style>` keyword.
fn style_keyword(value: &ComponentValue) -> Option<BorderStyle> {
    let ComponentValue::Token(Token::Ident(name), _) = value else {
        return None;
    };
    match name.to_ascii_lowercase().as_str() {
        "none" => Some(BorderStyle::None),
        "hidden" => Some(BorderStyle::Hidden),
        "solid" => Some(BorderStyle::Solid),
        "dashed" => Some(BorderStyle::Dashed),
        "dotted" => Some(BorderStyle::Dotted),
        "double" => Some(BorderStyle::Double),
        "groove" => Some(BorderStyle::Groove),
        "ridge" => Some(BorderStyle::Ridge),
        "inset" => Some(BorderStyle::Inset),
        "outset" => Some(BorderStyle::Outset),
        _ => None,
    }
}

/// A `<line-width>`: a non-negative absolute length or the `thin` /
/// `medium` / `thick` keywords (mapped to the usual UA pixel values).
fn width_px(value: &ComponentValue) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Ident(name), _) => match name.to_ascii_lowercase().as_str() {
            "thin" => Some(1.0),
            "medium" => Some(3.0),
            "thick" => Some(5.0),
            _ => None,
        },
        _ => {
            let px = absolute_px(value)?;
            (px >= 0.0).then_some(px)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_shorthand_parses() {
        let border = parse("2px solid black").unwrap();
        assert_eq!(border.width_px, 2.0);
        assert_eq!(border.style, BorderStyle::Solid);
        assert_eq!(border.color, Some(Color::opaque(0, 0, 0)));
    }

    #[test]
    fn components_in_any_order() {
        let border = parse("black 2px solid").unwrap();
        assert_eq!(border, parse("2px solid black").unwrap());
        let border = parse("solid #f00").unwrap();
        assert_eq!(border.style, BorderStyle::Solid);
        assert_eq!(border.color, Some(Color::opaque(255, 0, 0)));
    }

    #[test]
    fn omissions_fall_back_to_initial_values() {
        let border = parse("dashed").unwrap();
        assert_eq!(border.width_px, 3.0);
        assert_eq!(border.style, BorderStyle::Dashed);
        assert_eq!(border.color, None);
        let border = parse("4px").unwrap();
        assert_eq!(border.style, BorderStyle::None);
        assert_eq!(border.width_px, 4.0);
    }

    #[test]
    fn keyword_widths_map_to_pixels() {
        assert_eq!(parse("thin solid").unwrap().width_px, 1.0);
        assert_eq!(parse("medium solid").unwrap().width_px, 3.0);
        assert_eq!(parse("thick solid").unwrap().width_px, 5.0);
        assert_eq!(parse("0 solid").unwrap().width_px, 0.0);
        assert_eq!(parse("1in solid").unwrap().width_px, 96.0);
    }

    #[test]
    fn negative_widths_are_invalid() {
        assert_eq!(parse("-2px solid"), None);
    }

    #[test]
    fn malformed_shorthands_are_rejected() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("solid dashed"), None);
        assert_eq!(parse("2px 3px solid"), None);
        assert_eq!(parse("2px solid red red"), None);
        assert_eq!(parse("1em solid"), None);
        assert_eq!(parse("groovy"), None);
        assert_eq!(parse("2px solid red blue"), None);
    }
}
