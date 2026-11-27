//! CSS color parsing for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use super::{meaningful, number, percentage, split_comma, split_delim};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// A resolved color: 0-255 channels and an f64 alpha in 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

impl Color {
    /// Fully opaque color from 8-bit channels.
    pub const fn opaque(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 1.0 }
    }
}

/// Parse one computed CSS `<color>` value: named colors, hex notation
/// (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), `rgb()`/`rgba()` in both
/// legacy comma and modern space syntax, and `hsl()`/`hsla()`.
/// `currentcolor`, `calc()` channels and anything malformed return `None`.
pub fn parse_color(value: &str) -> Option<Color> {
    let values = parse_component_values(value);
    let meaningful = meaningful(&values);
    match meaningful.as_slice() {
        [single] => from_component(single),
        _ => None,
    }
}

/// Parse a color from a single component value (token or function), so
/// sibling appearance parsers can consume color tokens without
/// re-stringifying them.
pub(crate) fn from_component(value: &ComponentValue) -> Option<Color> {
    match value {
        ComponentValue::Token(Token::Hash { value: hex, .. }, _) => hex_color(hex),
        ComponentValue::Token(Token::Ident(name), _) => named(name),
        ComponentValue::Function {
            name, arguments, ..
        } => function_color(name, arguments),
        _ => None,
    }
}

fn function_color(name: &str, arguments: &[ComponentValue]) -> Option<Color> {
    match name.to_ascii_lowercase().as_str() {
        "rgb" | "rgba" => rgb(arguments),
        "hsl" | "hsla" => hsl(arguments),
        _ => None,
    }
}

/// Split function arguments into exactly three channel tokens plus the
/// optional alpha, accepting both the legacy comma syntax and the modern
/// space syntax with a `/`-separated alpha.
fn channels_and_alpha(arguments: &[ComponentValue]) -> Option<(Vec<&ComponentValue>, Option<f64>)> {
    let has_comma = arguments
        .iter()
        .any(|value| matches!(value, ComponentValue::Token(Token::Comma, _)));
    if has_comma {
        let groups = split_comma(arguments);
        if !(3..=4).contains(&groups.len()) {
            return None;
        }
        let mut channels = Vec::with_capacity(3);
        for group in &groups[..3] {
            let meaningful = meaningful(group);
            if meaningful.len() != 1 {
                return None;
            }
            channels.push(meaningful[0]);
        }
        let alpha = match groups.get(3) {
            Some(section) => Some(alpha(section)?),
            None => None,
        };
        Some((channels, alpha))
    } else {
        let parts = split_delim(arguments, '/');
        if parts.len() > 2 {
            return None;
        }
        let channels = meaningful(parts[0]);
        if channels.len() != 3 {
            return None;
        }
        let alpha = match parts.get(1) {
            Some(section) => Some(alpha(section)?),
            None => None,
        };
        Some((channels, alpha))
    }
}

fn rgb(arguments: &[ComponentValue]) -> Option<Color> {
    let (channels, alpha) = channels_and_alpha(arguments)?;
    let mut resolved = [0u8; 3];
    for (target, value) in resolved.iter_mut().zip(channels) {
        *target = match value {
            ComponentValue::Token(Token::Number(numeric), _) => numeric.value.clamp(0.0, 255.0),
            ComponentValue::Token(Token::Percentage(numeric), _) => {
                (numeric.value / 100.0 * 255.0).clamp(0.0, 255.0)
            }
            _ => return None,
        }
        .round() as u8;
    }
    Some(Color {
        r: resolved[0],
        g: resolved[1],
        b: resolved[2],
        a: alpha.unwrap_or(1.0),
    })
}

fn hsl(arguments: &[ComponentValue]) -> Option<Color> {
    let (channels, alpha) = channels_and_alpha(arguments)?;
    let hue = hue(channels[0])?;
    // CSS Color 4 allows both percentages and plain numbers for
    // saturation/lightness; both share the 0-100 scale.
    let saturation = saturation_scale(channels[1])?;
    let lightness = saturation_scale(channels[2])?;
    let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);
    Some(Color {
        r,
        g,
        b,
        a: alpha.unwrap_or(1.0),
    })
}

/// A hue: any `<number>` (degrees implied) or an angle dimension.
fn hue(value: &ComponentValue) -> Option<f64> {
    match value {
        ComponentValue::Token(Token::Number(numeric), _) => Some(numeric.value),
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

fn saturation_scale(value: &ComponentValue) -> Option<f64> {
    number(value).or_else(|| percentage(value))
}

/// An alpha value: `<number>` 0..1 or a percentage. Out-of-range values
/// clamp (CSS behavior), malformed shapes return `None`.
fn alpha(values: &[ComponentValue]) -> Option<f64> {
    let meaningful = meaningful(values);
    match meaningful.as_slice() {
        [ComponentValue::Token(Token::Number(numeric), _)] => Some(numeric.value.clamp(0.0, 1.0)),
        [ComponentValue::Token(Token::Percentage(numeric), _)] => {
            Some((numeric.value / 100.0).clamp(0.0, 1.0))
        }
        _ => None,
    }
}

fn hex_color(hex: &str) -> Option<Color> {
    let mut digits = Vec::with_capacity(hex.len());
    for character in hex.chars() {
        digits.push(character.to_digit(16)? as u8);
    }
    let (r, g, b, a) = match digits.len() {
        3 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, 255u8),
        4 => (
            digits[0] * 17,
            digits[1] * 17,
            digits[2] * 17,
            digits[3] * 17,
        ),
        6 => (
            digits[0] << 4 | digits[1],
            digits[2] << 4 | digits[3],
            digits[4] << 4 | digits[5],
            255,
        ),
        8 => (
            digits[0] << 4 | digits[1],
            digits[2] << 4 | digits[3],
            digits[4] << 4 | digits[5],
            digits[6] << 4 | digits[7],
        ),
        _ => return None,
    };
    Some(Color {
        r,
        g,
        b,
        a: f64::from(a) / 255.0,
    })
}

fn named(name: &str) -> Option<Color> {
    // CSS basic color keywords plus their common aliases; `transparent`
    // is fully transparent black. `currentcolor` is context-dependent
    // and deliberately not resolved here.
    let color = match name.to_ascii_lowercase().as_str() {
        "black" => Color::opaque(0, 0, 0),
        "silver" => Color::opaque(192, 192, 192),
        "gray" | "grey" => Color::opaque(128, 128, 128),
        "white" => Color::opaque(255, 255, 255),
        "maroon" => Color::opaque(128, 0, 0),
        "red" => Color::opaque(255, 0, 0),
        "purple" => Color::opaque(128, 0, 128),
        "fuchsia" | "magenta" => Color::opaque(255, 0, 255),
        "green" => Color::opaque(0, 128, 0),
        "lime" => Color::opaque(0, 255, 0),
        "olive" => Color::opaque(128, 128, 0),
        "yellow" => Color::opaque(255, 255, 0),
        "navy" => Color::opaque(0, 0, 128),
        "blue" => Color::opaque(0, 0, 255),
        "teal" => Color::opaque(0, 128, 128),
        "aqua" | "cyan" => Color::opaque(0, 255, 255),
        "orange" => Color::opaque(255, 165, 0),
        "gold" => Color::opaque(255, 215, 0),
        "pink" => Color::opaque(255, 192, 203),
        "brown" => Color::opaque(165, 42, 42),
        "indigo" => Color::opaque(75, 0, 130),
        "violet" => Color::opaque(238, 130, 238),
        "transparent" => Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        },
        _ => return None,
    };
    Some(color)
}

/// Standard HSL → sRGB conversion; inputs are hue in degrees and
/// saturation/lightness on the 0-100 scale, output channels 0-255.
fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> (u8, u8, u8) {
    let hue = (hue.rem_euclid(360.0)) / 360.0;
    let saturation = (saturation / 100.0).clamp(0.0, 1.0);
    let lightness = (lightness / 100.0).clamp(0.0, 1.0);
    if saturation == 0.0 {
        let channel = (lightness * 255.0).round() as u8;
        return (channel, channel, channel);
    }
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    let to_channel = |t: f64| {
        let t = t.rem_euclid(1.0);
        let value = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (value * 255.0).round() as u8
    };
    (
        to_channel(hue + 1.0 / 3.0),
        to_channel(hue),
        to_channel(hue - 1.0 / 3.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(value: &str) -> Color {
        parse_color(value).unwrap_or_else(|| panic!("{value:?} should parse"))
    }

    #[test]
    fn named_colors_resolve() {
        assert_eq!(color("red"), Color::opaque(255, 0, 0));
        assert_eq!(color("RED"), Color::opaque(255, 0, 0));
        assert_eq!(color("lime"), Color::opaque(0, 255, 0));
        assert_eq!(color("cyan"), Color::opaque(0, 255, 255));
        assert_eq!(color("grey"), color("gray"));
        assert_eq!(color("transparent").a, 0.0);
    }

    #[test]
    fn hex_notation_is_fully_supported() {
        assert_eq!(color("#f00"), Color::opaque(255, 0, 0));
        assert_eq!(color("#ff0000"), Color::opaque(255, 0, 0));
        assert_eq!(color("#f00f"), Color::opaque(255, 0, 0));
        assert_eq!(color("#ff0000ff"), Color::opaque(255, 0, 0));
        assert_eq!(color("#00ff0080").a, 128.0 / 255.0);
        // Wrong digit counts and non-hex digits are rejected.
        assert_eq!(parse_color("#ff"), None);
        assert_eq!(parse_color("#ff000"), None);
        assert_eq!(parse_color("#zzzzzz"), None);
    }

    #[test]
    fn rgb_accepts_comma_and_space_syntax() {
        assert_eq!(color("rgb(255, 0, 0)"), Color::opaque(255, 0, 0));
        assert_eq!(color("rgb(255 0 0)"), Color::opaque(255, 0, 0));
        assert_eq!(color("rgba(0, 0, 255, 0.5)").a, 0.5);
        assert_eq!(color("rgb(0 0 255 / 0.5)").a, 0.5);
        assert_eq!(color("rgb(0 0 255 / 50%)").a, 0.5);
        assert_eq!(color("rgb(100%, 0%, 0%)"), Color::opaque(255, 0, 0));
        // Out-of-range channels clamp instead of failing.
        assert_eq!(color("rgb(300, -10, 0)"), Color::opaque(255, 0, 0));
    }

    #[test]
    fn hsl_resolves_to_rgb() {
        assert_eq!(color("hsl(0, 100%, 50%)"), Color::opaque(255, 0, 0));
        assert_eq!(color("hsl(120, 100%, 50%)"), Color::opaque(0, 255, 0));
        assert_eq!(color("hsl(240, 100%, 50%)"), Color::opaque(0, 0, 255));
        assert_eq!(color("hsl(120 50% 50% / 0.5)").a, 0.5);
        // Hue wraps: 480deg == 120deg (green); saturation zero gives an
        // exact gray.
        assert_eq!(color("hsl(480, 100%, 50%)"), Color::opaque(0, 255, 0));
        assert_eq!(color("hsl(90, 0%, 50%)"), Color::opaque(128, 128, 128));
    }

    #[test]
    fn malformed_colors_are_rejected() {
        assert_eq!(parse_color("currentcolor"), None);
        assert_eq!(parse_color("not-a-color"), None);
        assert_eq!(parse_color(""), None);
        assert_eq!(parse_color("rgb(1, 2)"), None);
        assert_eq!(parse_color("rgb(1, 2, 3,)"), None);
        assert_eq!(parse_color("rgb(1 2 3 0.5)"), None);
        assert_eq!(parse_color("rgb(auto, 0, 0)"), None);
        assert_eq!(parse_color("hsl(120, 100%, 50%, 0.5, 9)"), None);
        assert_eq!(parse_color("red blue"), None);
    }
}
