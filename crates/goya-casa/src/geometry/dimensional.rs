//! CSS units → proportional constraints (directive §9.1).
//!
//! Parses the length grammar (numbers, dimensions, percentages) against a
//! [`Context`] and resolves it to reference pixels. Absolute units convert
//! directly; font-relative and viewport units resolve through the context
//! at parse time. Percentages stay symbolic in [`Length::Percent`] until a
//! reference (parent width, containing block, ...) is applied. Unknown
//! units and malformed values return `None` so callers can report
//! `CASA-GEO-001` and fall back deterministically.

use super::{Context, Length, Px};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// Points per CSS reference pixel (1pt = 1/72in, 1px = 1/96in).
const PTS_PER_PX: f64 = 72.0 / 96.0;

/// Parse one CSS `<length-percentage>` value. Accepts a unitless zero (the
/// only unitless length CSS allows), dimensions with known units, and
/// percentages. `calc()` is handled by [`super::calc`].
pub fn parse(value: &str, context: &Context) -> Option<Length> {
    let values = parse_component_values(value);
    let meaningful: Vec<&ComponentValue> = values
        .iter()
        .filter(|v| !matches!(v, ComponentValue::Token(Token::Whitespace, _)))
        .collect();
    match meaningful.as_slice() {
        [ComponentValue::Token(Token::Number(number), _)] if number.value == 0.0 => {
            Some(Length::Px(0.0))
        }
        [ComponentValue::Token(Token::Dimension { number, unit, .. }, _)] => {
            dimension_to_px(number.value, unit, context).map(Length::Px)
        }
        [ComponentValue::Token(Token::Percentage(number), _)] => {
            Some(Length::Percent(number.value))
        }
        _ => None,
    }
}

/// Resolve a parsed length to pixels given the reference the percentage
/// applies against (parent width, containing block, ...).
pub fn resolve(length: Length, reference: Px) -> Px {
    match length {
        Length::Px(px) => px,
        Length::Percent(percent) => percent / 100.0 * reference,
    }
}

/// Parse and resolve in one step: `resolve(parse(value)?, reference)`.
pub fn resolve_value(value: &str, reference: Px, context: &Context) -> Option<Px> {
    Some(resolve(parse(value, context)?, reference))
}

/// Convert one dimensioned value to CSS reference pixels. Font-relative
/// and viewport units take their base from the context; `ex`/`ch`
/// approximate the x-height and `0`-glyph width as half the font size
/// without a font metrics engine.
fn dimension_to_px(value: f64, unit: &str, context: &Context) -> Option<Px> {
    match unit.to_ascii_lowercase().as_str() {
        "px" => Some(value),
        "pt" => Some(value / PTS_PER_PX),
        "pc" => Some(value / PTS_PER_PX * 12.0),
        "in" => Some(value * 96.0),
        "cm" => Some(value * 96.0 / 2.54),
        "mm" => Some(value * 96.0 / 25.4),
        "q" => Some(value * 96.0 / 101.6),
        "em" => Some(value * context.font_size),
        "rem" => Some(value * context.root_font_size),
        "ex" | "ch" => Some(value * context.font_size * 0.5),
        "vw" => Some(value / 100.0 * context.viewport_width),
        "vh" => Some(value / 100.0 * context.viewport_height),
        "vmin" => Some(value / 100.0 * context.viewport_width.min(context.viewport_height)),
        "vmax" => Some(value / 100.0 * context.viewport_width.max(context.viewport_height)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    #[test]
    fn absolute_units_convert_to_px() {
        let ctx = context();
        assert_eq!(resolve_value("0", 100.0, &ctx), Some(0.0));
        assert_eq!(resolve_value("12px", 100.0, &ctx), Some(12.0));
        assert!((resolve_value("12pt", 100.0, &ctx).unwrap() - 16.0).abs() < 1e-9);
        assert_eq!(resolve_value("1in", 100.0, &ctx), Some(96.0));
        assert!((resolve_value("2.54cm", 100.0, &ctx).unwrap() - 96.0).abs() < 1e-9);
        assert!((resolve_value("10mm", 100.0, &ctx).unwrap() - 96.0 * 10.0 / 25.4).abs() < 1e-9);
    }

    #[test]
    fn font_and_viewport_units_use_context() {
        let ctx = context();
        assert_eq!(resolve_value("1.5em", 0.0, &ctx), Some(24.0));
        assert_eq!(resolve_value("2rem", 0.0, &ctx), Some(32.0));
        assert_eq!(resolve_value("10vw", 0.0, &ctx), Some(80.0));
        assert_eq!(resolve_value("10vh", 0.0, &ctx), Some(60.0));
        assert_eq!(resolve_value("1vmin", 0.0, &ctx), Some(6.0));
        assert_eq!(resolve_value("1vmax", 0.0, &ctx), Some(8.0));
    }

    #[test]
    fn percentages_resolve_against_reference() {
        let ctx = context();
        assert_eq!(resolve_value("50%", 200.0, &ctx), Some(100.0));
        // Percentage of zero reference collapses to zero, not an error.
        assert_eq!(resolve_value("50%", 0.0, &ctx), Some(0.0));
    }

    #[test]
    fn unitless_nonzero_and_unknown_units_are_unresolvable() {
        let ctx = context();
        assert_eq!(resolve_value("12", 100.0, &ctx), None);
        assert_eq!(resolve_value("10fr", 100.0, &ctx), None);
        assert_eq!(resolve_value("auto", 100.0, &ctx), None);
        // Font-relative units parse through the same entry point.
        assert_eq!(resolve_value("1em", 0.0, &ctx), Some(16.0));
    }
}
