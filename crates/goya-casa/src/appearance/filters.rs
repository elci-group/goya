//! CSS filter list parsing for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use super::colors;
use super::shadows::Shadow;
use super::{absolute_px, angle_degrees, meaningful, number, percentage};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// One filter function. Amounts are the CSS-normalized ratios (100% = 1);
/// a bare `none` value is the empty list.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Blur { radius_px: f64 },
    Brightness { amount: f64 },
    Contrast { amount: f64 },
    Grayscale { amount: f64 },
    Sepia { amount: f64 },
    Saturate { amount: f64 },
    HueRotate { degrees: f64 },
    Invert { amount: f64 },
    Opacity { amount: f64 },
    DropShadow { shadow: Shadow },
}

/// Parse one computed `filter` value: a whitespace-separated list of
/// filter functions, or `none` for the empty list.
pub fn parse(value: &str) -> Option<Vec<Filter>> {
    let values = parse_component_values(value);
    let meaningful = meaningful(&values);
    match meaningful.as_slice() {
        [] => None,
        [ComponentValue::Token(Token::Ident(name), _)] if name.eq_ignore_ascii_case("none") => {
            Some(Vec::new())
        }
        _ => {
            let mut filters = Vec::with_capacity(meaningful.len());
            for value in &meaningful {
                let ComponentValue::Function {
                    name, arguments, ..
                } = value
                else {
                    return None;
                };
                filters.push(function(name, arguments)?);
            }
            Some(filters)
        }
    }
}

fn function(name: &str, arguments: &[ComponentValue]) -> Option<Filter> {
    let tokens = meaningful(arguments);
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "blur" => Some(Filter::Blur {
            radius_px: match tokens.as_slice() {
                [only] => absolute_px(only)?,
                [] => 0.0,
                _ => return None,
            },
        }),
        "hue-rotate" => Some(Filter::HueRotate {
            degrees: match tokens.as_slice() {
                [only] => angle_degrees(only)?,
                [] => 0.0,
                _ => return None,
            },
        }),
        "brightness" | "contrast" | "grayscale" | "sepia" | "saturate" | "invert" | "opacity" => {
            Some(amount_filter(&name, &tokens)?)
        }
        "drop-shadow" => Some(Filter::DropShadow {
            shadow: drop_shadow(&tokens)?,
        }),
        _ => None,
    }
}

/// `<amount>` filters: a number or percentage ratio; omission means the
/// identity amount 1 (0 for `blur`, handled above).
fn amount_filter(name: &str, tokens: &[&ComponentValue]) -> Option<Filter> {
    let amount = match tokens {
        [only] => number(only).or_else(|| percentage(only).map(|percent| percent / 100.0))?,
        [] => 1.0,
        _ => return None,
    };
    Some(match name {
        "brightness" => Filter::Brightness { amount },
        "contrast" => Filter::Contrast { amount },
        "grayscale" => Filter::Grayscale { amount },
        "sepia" => Filter::Sepia { amount },
        "saturate" => Filter::Saturate { amount },
        "invert" => Filter::Invert { amount },
        "opacity" => Filter::Opacity { amount },
        _ => return None,
    })
}

/// `drop-shadow(<x> <y> <blur>? <color>?)`: no spread and never inset,
/// so the box-shadow grammar is reused minus those parts.
fn drop_shadow(tokens: &[&ComponentValue]) -> Option<Shadow> {
    let mut lengths = Vec::new();
    let mut color = None;
    let mut saw_color = false;
    for value in tokens {
        if let Some(px) = absolute_px(value) {
            lengths.push(px);
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
    if !(2..=3).contains(&lengths.len()) {
        return None;
    }
    Some(Shadow {
        x_offset_px: lengths[0],
        y_offset_px: lengths[1],
        blur_radius_px: lengths.get(2).copied().unwrap_or(0.0),
        spread_radius_px: 0.0,
        color,
        inset: false,
    })
}

#[cfg(test)]
mod tests {
    use super::super::colors::Color;
    use super::*;

    #[test]
    fn chain_parses_in_order() {
        let filters = parse("blur(4px) brightness(1.2) hue-rotate(90deg)").unwrap();
        assert_eq!(filters.len(), 3);
        assert_eq!(filters[0], Filter::Blur { radius_px: 4.0 });
        assert_eq!(filters[1], Filter::Brightness { amount: 1.2 });
        assert_eq!(filters[2], Filter::HueRotate { degrees: 90.0 });
    }

    #[test]
    fn amounts_accept_numbers_percentages_and_defaults() {
        assert_eq!(
            parse("grayscale(50%)").unwrap()[0],
            Filter::Grayscale { amount: 0.5 }
        );
        assert_eq!(
            parse("saturate(2)").unwrap()[0],
            Filter::Saturate { amount: 2.0 }
        );
        assert_eq!(
            parse("opacity(0.3)").unwrap()[0],
            Filter::Opacity { amount: 0.3 }
        );
        // Omitted amount is the identity.
        assert_eq!(
            parse("invert()").unwrap()[0],
            Filter::Invert { amount: 1.0 }
        );
        assert_eq!(parse("blur()").unwrap()[0], Filter::Blur { radius_px: 0.0 });
        assert_eq!(
            parse("hue-rotate()").unwrap()[0],
            Filter::HueRotate { degrees: 0.0 }
        );
    }

    #[test]
    fn drop_shadow_maps_to_the_shadow_type() {
        let filter = parse("drop-shadow(2px 3px 4px rgba(0, 0, 0, 0.5))").unwrap();
        assert_eq!(
            filter[0],
            Filter::DropShadow {
                shadow: Shadow {
                    x_offset_px: 2.0,
                    y_offset_px: 3.0,
                    blur_radius_px: 4.0,
                    spread_radius_px: 0.0,
                    color: Some(Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0.5
                    }),
                    inset: false,
                }
            }
        );
        let filter = parse("drop-shadow(1px 1px red)").unwrap();
        assert_eq!(
            filter[0],
            Filter::DropShadow {
                shadow: Shadow {
                    x_offset_px: 1.0,
                    y_offset_px: 1.0,
                    blur_radius_px: 0.0,
                    spread_radius_px: 0.0,
                    color: Some(Color::opaque(255, 0, 0)),
                    inset: false,
                }
            }
        );
    }

    #[test]
    fn none_is_the_empty_list() {
        assert_eq!(parse("none"), Some(Vec::new()));
    }

    #[test]
    fn malformed_filters_are_rejected() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("blur(4px) brightness"), None);
        assert_eq!(parse("blur(1em)"), None);
        // blur takes a length only; amount filters take numbers or
        // percentages only.
        assert_eq!(parse("blur(50%)"), None);
        assert_eq!(parse("brightness(4px)"), None);
        assert_eq!(parse("drop-shadow(1px)"), None);
        assert_eq!(parse("drop-shadow(1px 2px 3px 4px)"), None);
        assert_eq!(parse("translate(1px)"), None);
    }
}
