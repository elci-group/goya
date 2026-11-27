//! CSS clip-path shape parsing for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use super::{length, meaningful};
use crate::geometry::Length;
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// A point inside the clip reference box; coordinates stay symbolic
/// (percentages) or resolve to px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: Length,
    pub y: Length,
}

/// One basic shape from `clip-path`. Radii and vertices keep percentages
/// symbolic. Keyword-only forms (`closest-side`, `round`, fill rules)
/// have no numeric representation and parse as `None`.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipShape {
    Inset {
        top: Length,
        right: Length,
        bottom: Length,
        left: Length,
    },
    Circle {
        radius: Length,
        center: Option<Point>,
    },
    Ellipse {
        rx: Length,
        ry: Length,
        center: Option<Point>,
    },
    Polygon {
        vertices: Vec<Point>,
    },
}

/// Parse one computed `clip-path` value. `none` (and any unsupported or
/// malformed shape) means no clip and returns `None`.
pub fn parse(value: &str) -> Option<ClipShape> {
    let values = parse_component_values(value);
    let tokens = meaningful(&values);
    match tokens.as_slice() {
        [ComponentValue::Token(Token::Ident(name), _)] if name.eq_ignore_ascii_case("none") => None,
        [ComponentValue::Function {
            name, arguments, ..
        }] => {
            let tokens = meaningful(arguments);
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                "inset" => inset(&tokens),
                "circle" => circle(&tokens),
                "ellipse" => ellipse(&tokens),
                "polygon" => polygon(&tokens),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `inset(<length-percentage>{1,4})` with the standard 1-4 value
/// expansion. `round <border-radius>` is not modeled and rejects.
fn inset(tokens: &[&ComponentValue]) -> Option<ClipShape> {
    if tokens
        .iter()
        .any(|value| matches!(value, ComponentValue::Token(Token::Ident(name), _) if name.eq_ignore_ascii_case("round")))
    {
        return None;
    }
    let sides: Vec<Length> = tokens
        .iter()
        .map(|value| length(value))
        .collect::<Option<Vec<_>>>()?;
    let [top, right, bottom, left] = expand(sides)?;
    Some(ClipShape::Inset {
        top,
        right,
        bottom,
        left,
    })
}

/// CSS 1-4 value expansion (top right bottom left).
fn expand<T: Copy>(values: Vec<T>) -> Option<[T; 4]> {
    let values = match values.len() {
        1 => vec![values[0], values[0], values[0], values[0]],
        2 => vec![values[0], values[1], values[0], values[1]],
        3 => vec![values[0], values[1], values[2], values[1]],
        4 => values,
        _ => return None,
    };
    values.try_into().ok()
}

/// Split an argument list on a bare `at` identifier: the tokens before
/// it and the position after it.
fn split_at<'a>(
    tokens: &'a [&'a ComponentValue],
) -> Option<(&'a [&'a ComponentValue], Option<Point>)> {
    for (index, value) in tokens.iter().enumerate() {
        if let ComponentValue::Token(Token::Ident(name), _) = value {
            if name.eq_ignore_ascii_case("at") {
                let point = position(&tokens[index + 1..])?;
                return Some((&tokens[..index], Some(point)));
            }
        }
    }
    Some((tokens, None))
}

/// An `at` clause position: exactly one x/y pair. A single coordinate
/// would leave the other axis at keyword `center`, which has no numeric
/// form here.
fn position(tokens: &[&ComponentValue]) -> Option<Point> {
    if tokens.len() != 2 {
        return None;
    }
    Some(Point {
        x: length(tokens[0])?,
        y: length(tokens[1])?,
    })
}

fn circle(tokens: &[&ComponentValue]) -> Option<ClipShape> {
    let (radius_tokens, center) = split_at(tokens)?;
    let radius = match radius_tokens {
        [] => return None, // default `closest-side` has no numeric form
        [only] => length(only)?,
        _ => return None,
    };
    Some(ClipShape::Circle { radius, center })
}

fn ellipse(tokens: &[&ComponentValue]) -> Option<ClipShape> {
    let (radii_tokens, center) = split_at(tokens)?;
    let [rx, ry] = match radii_tokens.len() {
        2 => [length(radii_tokens[0])?, length(radii_tokens[1])?],
        // Omitted radii default to `closest-side`.
        _ => return None,
    };
    Some(ClipShape::Ellipse { rx, ry, center })
}

/// `polygon([<fill-rule>,]? <length-percentage>{2}#)`. A fill rule is
/// not modeled and rejects.
fn polygon(tokens: &[&ComponentValue]) -> Option<ClipShape> {
    let mut vertices = Vec::new();
    for group in split_comma_across(tokens) {
        if group.len() != 2 {
            return None;
        }
        vertices.push(Point {
            x: length(group[0])?,
            y: length(group[1])?,
        });
    }
    if vertices.is_empty() {
        return None;
    }
    Some(ClipShape::Polygon { vertices })
}

/// Comma-splitting over an already-`meaningful` token list.
fn split_comma_across<'a>(tokens: &'a [&'a ComponentValue]) -> Vec<&'a [&'a ComponentValue]> {
    let mut groups = Vec::new();
    let mut start = 0;
    for (index, value) in tokens.iter().enumerate() {
        if matches!(value, ComponentValue::Token(Token::Comma, _)) {
            groups.push(&tokens[start..index]);
            start = index + 1;
        }
    }
    groups.push(&tokens[start..]);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_expands_one_to_four_values() {
        let shape = parse("inset(10px)").unwrap();
        assert_eq!(
            shape,
            ClipShape::Inset {
                top: Length::Px(10.0),
                right: Length::Px(10.0),
                bottom: Length::Px(10.0),
                left: Length::Px(10.0)
            }
        );
        let shape = parse("inset(10% 20px)").unwrap();
        assert_eq!(
            shape,
            ClipShape::Inset {
                top: Length::Percent(10.0),
                right: Length::Px(20.0),
                bottom: Length::Percent(10.0),
                left: Length::Px(20.0)
            }
        );
        let shape = parse("inset(1px 2px 3px 4px)").unwrap();
        assert_eq!(
            shape,
            ClipShape::Inset {
                top: Length::Px(1.0),
                right: Length::Px(2.0),
                bottom: Length::Px(3.0),
                left: Length::Px(4.0)
            }
        );
    }

    #[test]
    fn circle_and_ellipse_parse_with_center() {
        let shape = parse("circle(50% at 10px 20px)").unwrap();
        assert_eq!(
            shape,
            ClipShape::Circle {
                radius: Length::Percent(50.0),
                center: Some(Point {
                    x: Length::Px(10.0),
                    y: Length::Px(20.0)
                })
            }
        );
        let shape = parse("circle(5px)").unwrap();
        assert_eq!(
            shape,
            ClipShape::Circle {
                radius: Length::Px(5.0),
                center: None
            }
        );
        let shape = parse("ellipse(10px 20%)").unwrap();
        assert_eq!(
            shape,
            ClipShape::Ellipse {
                rx: Length::Px(10.0),
                ry: Length::Percent(20.0),
                center: None
            }
        );
    }

    #[test]
    fn polygon_collects_vertices() {
        let shape = parse("polygon(0% 0%, 100% 0%, 50% 100%)").unwrap();
        assert_eq!(
            shape,
            ClipShape::Polygon {
                vertices: vec![
                    Point {
                        x: Length::Percent(0.0),
                        y: Length::Percent(0.0)
                    },
                    Point {
                        x: Length::Percent(100.0),
                        y: Length::Percent(0.0)
                    },
                    Point {
                        x: Length::Percent(50.0),
                        y: Length::Percent(100.0)
                    },
                ]
            }
        );
    }

    #[test]
    fn none_and_unmodeled_shapes_reject() {
        assert_eq!(parse("none"), None);
        assert_eq!(parse("circle(closest-side)"), None);
        assert_eq!(parse("circle()"), None);
        assert_eq!(parse("ellipse(10px)"), None);
        assert_eq!(parse("inset(10px round 4px)"), None);
        assert_eq!(parse("polygon(evenodd, 0 0, 1px 1px)"), None);
        assert_eq!(parse("circle(50% at 10px)"), None);
        assert_eq!(parse("path('M0 0')"), None);
    }

    #[test]
    fn malformed_shapes_are_rejected() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("circle(50%) circle(60%)"), None);
        assert_eq!(parse("inset(auto)"), None);
        assert_eq!(parse("polygon(10px)"), None);
        assert_eq!(parse("circle(1em)"), None);
    }
}
