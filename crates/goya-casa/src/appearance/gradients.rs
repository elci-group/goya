//! CSS gradient parsing for the appearance transformation stage
//! (directive GOYA-DIS-CASA-001).

use super::colors::{self, Color};
use super::{absolute_px, angle_degrees, length, meaningful, percentage, split_comma};
use crate::geometry::Length;
use goya_css_syntax::{parse_component_values, ComponentValue, Token};

/// The gradient image families this stage supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    Conic,
}

/// A stop position: symbolic percentage or resolved pixels. Percentages
/// stay unresolved because the gradient box is unknown at this stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopPosition {
    Percent(f64),
    Px(f64),
}

/// One color stop; `position` is `None` when the CSS omits it (the
/// position is then interpolated between neighbours by the renderer).
#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    pub color: Color,
    pub position: Option<StopPosition>,
}

/// A parsed gradient image. `angle` is the linear direction or the conic
/// `from` angle in degrees (`None` for radial); `center` is the radial or
/// conic center point (`None` for linear and when CSS uses the default).
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub angle: Option<f64>,
    pub center: Option<(Length, Length)>,
    pub stops: Vec<GradientStop>,
}

/// Parse one computed `*-gradient()` image value.
pub fn parse(value: &str) -> Option<Gradient> {
    let values = parse_component_values(value);
    let meaningful = meaningful(&values);
    match meaningful.as_slice() {
        [single] => from_component(single),
        _ => None,
    }
}

/// Parse a gradient from a single component value (for shorthands such
/// as `background` that embed gradient functions).
pub(crate) fn from_component(value: &ComponentValue) -> Option<Gradient> {
    match value {
        ComponentValue::Function {
            name, arguments, ..
        } => function(name, arguments),
        _ => None,
    }
}

fn function(name: &str, arguments: &[ComponentValue]) -> Option<Gradient> {
    let name = name.to_ascii_lowercase();
    let kind = match name.as_str() {
        "linear-gradient" => GradientKind::Linear,
        "radial-gradient" => GradientKind::Radial,
        "conic-gradient" => GradientKind::Conic,
        _ => return None,
    };
    let sections = split_comma(arguments);
    if sections.is_empty() {
        return None;
    }
    let mut angle = None;
    let mut center = None;
    let mut stops_start = 0;
    let first = meaningful(sections[0]);
    if !first.is_empty() && is_directive(kind, &first) {
        match kind {
            GradientKind::Linear => angle = Some(linear_direction(&first)?),
            GradientKind::Conic => {
                let (from, at) = conic_directive(&first)?;
                angle = Some(from);
                center = at;
            }
            GradientKind::Radial => center = radial_directive(&first)?,
        }
        stops_start = 1;
    }
    // CSS defaults when the directive clause is omitted: linear runs
    // `to bottom`, conic starts `from 0deg`.
    if angle.is_none() {
        angle = match kind {
            GradientKind::Linear => Some(180.0),
            GradientKind::Conic => Some(0.0),
            GradientKind::Radial => None,
        };
    }
    let mut stops = Vec::new();
    for section in &sections[stops_start..] {
        stops.push(stop(section)?);
    }
    // A gradient needs at least two color stops per the CSS grammar.
    if stops.len() < 2 {
        return None;
    }
    Some(Gradient {
        kind,
        angle,
        center,
        stops,
    })
}

/// Whether the first comma-separated section carries the gradient's
/// direction/shape clause rather than a color stop.
fn is_directive(kind: GradientKind, tokens: &[&ComponentValue]) -> bool {
    match kind {
        GradientKind::Linear => match tokens {
            [single] => angle_degrees(single).is_some(),
            [ComponentValue::Token(Token::Ident(name), _), ..] => name.eq_ignore_ascii_case("to"),
            _ => false,
        },
        GradientKind::Conic => tokens.iter().any(|value| {
            matches!(
                value,
                ComponentValue::Token(Token::Ident(name), _)
                    if name.eq_ignore_ascii_case("from") || name.eq_ignore_ascii_case("at")
            )
        }),
        GradientKind::Radial => tokens.iter().any(|value| {
            matches!(
                value,
                ComponentValue::Token(Token::Ident(name), _)
                    if radial_keyword(name) || name.eq_ignore_ascii_case("at")
            )
        }),
    }
}

/// `to <side> [<side>]` to a direction angle. Corner angles use the
/// square-box values (45°/135°/225°/315°); the box is unknown here.
fn linear_direction(tokens: &[&ComponentValue]) -> Option<f64> {
    match tokens {
        [single] => angle_degrees(single),
        [ComponentValue::Token(Token::Ident(first), _), rest @ ..]
            if first.eq_ignore_ascii_case("to") =>
        {
            let mut angle = None;
            for value in rest {
                let ComponentValue::Token(Token::Ident(side), _) = value else {
                    return None;
                };
                let side = side.to_ascii_lowercase();
                let degrees = match side.as_str() {
                    "top" => 0.0,
                    "right" => 90.0,
                    "bottom" => 180.0,
                    "left" => 270.0,
                    _ => return None,
                };
                angle = match angle {
                    None => Some(degrees),
                    // Corner: average the two sides onto the diagonal.
                    Some(existing) => Some((existing + degrees) / 2.0),
                };
            }
            // `to top bottom` and other opposing pairs are invalid.
            if rest.len() == 2 && (angle.unwrap() % 90.0) == 0.0 {
                return None;
            }
            angle
        }
        _ => None,
    }
}

fn radial_keyword(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "circle"
            | "ellipse"
            | "closest-side"
            | "closest-corner"
            | "farthest-side"
            | "farthest-corner"
    )
}

/// Shape/size keywords carry no extra information this stage needs, so
/// they are skipped; an `at` clause yields the center point. Anything
/// else (a size pair, an unknown keyword) is rejected.
fn radial_directive(tokens: &[&ComponentValue]) -> Option<Option<(Length, Length)>> {
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index] {
            ComponentValue::Token(Token::Ident(name), _) if radial_keyword(name) => index += 1,
            ComponentValue::Token(Token::Ident(name), _) if name.eq_ignore_ascii_case("at") => {
                return Some(center_point(&tokens[index + 1..]));
            }
            _ => return None,
        }
    }
    Some(None)
}

/// `from <angle>` and an optional `at <point>`; both are optional.
fn conic_directive(tokens: &[&ComponentValue]) -> Option<(f64, Option<(Length, Length)>)> {
    let mut angle = 0.0;
    let mut center = None;
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index] {
            ComponentValue::Token(Token::Ident(name), _) if name.eq_ignore_ascii_case("from") => {
                angle = angle_degrees(tokens.get(index + 1)?)?;
                index += 2;
            }
            ComponentValue::Token(Token::Ident(name), _) if name.eq_ignore_ascii_case("at") => {
                center = Some(center_point(&tokens[index + 1..])?);
                // The center point consumes the remainder.
                index = tokens.len();
            }
            _ => return None,
        }
    }
    Some((angle, center))
}

/// An `at` clause: exactly one x/y pair. A single coordinate would leave
/// the other axis at the keyword `center`, which has no numeric form.
fn center_point(tokens: &[&ComponentValue]) -> Option<(Length, Length)> {
    if tokens.len() != 2 {
        return None;
    }
    Some((length(tokens[0])?, length(tokens[1])?))
}

fn stop(section: &[ComponentValue]) -> Option<GradientStop> {
    let tokens = meaningful(section);
    match tokens.as_slice() {
        [color_token] => Some(GradientStop {
            color: colors::from_component(color_token)?,
            position: None,
        }),
        [color_token, position_token] => Some(GradientStop {
            color: colors::from_component(color_token)?,
            position: Some(stop_position(position_token)?),
        }),
        _ => None,
    }
}

fn stop_position(value: &ComponentValue) -> Option<StopPosition> {
    if let Some(percent) = percentage(value) {
        return Some(StopPosition::Percent(percent));
    }
    absolute_px(value).map(StopPosition::Px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_gradient_with_angle_and_stops() {
        let gradient = parse("linear-gradient(45deg, red, blue)").unwrap();
        assert_eq!(gradient.kind, GradientKind::Linear);
        assert_eq!(gradient.angle, Some(45.0));
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[0].color, Color::opaque(255, 0, 0));
        assert_eq!(gradient.stops[0].position, None);
    }

    #[test]
    fn linear_gradient_direction_keywords() {
        assert_eq!(
            parse("linear-gradient(to right, red, blue)").unwrap().angle,
            Some(90.0)
        );
        assert_eq!(
            parse("linear-gradient(to top, red, blue)").unwrap().angle,
            Some(0.0)
        );
        assert_eq!(
            parse("linear-gradient(to bottom right, red, blue)")
                .unwrap()
                .angle,
            Some(135.0)
        );
        // Default direction is `to bottom`.
        assert_eq!(
            parse("linear-gradient(red, blue)").unwrap().angle,
            Some(180.0)
        );
    }

    #[test]
    fn stop_positions_are_kept_symbolic() {
        let gradient = parse("linear-gradient(to right, red 20%, blue 30px)").unwrap();
        assert_eq!(
            gradient.stops[0].position,
            Some(StopPosition::Percent(20.0))
        );
        assert_eq!(gradient.stops[1].position, Some(StopPosition::Px(30.0)));
    }

    #[test]
    fn radial_gradient_shape_and_center() {
        let gradient = parse("radial-gradient(circle at 30px 40%, red, blue)").unwrap();
        assert_eq!(gradient.kind, GradientKind::Radial);
        assert_eq!(gradient.angle, None);
        assert_eq!(
            gradient.center,
            Some((Length::Px(30.0), Length::Percent(40.0)))
        );
        // Shape keywords without a center still parse.
        assert_eq!(
            parse("radial-gradient(ellipse, red, blue)").unwrap().center,
            None
        );
    }

    #[test]
    fn conic_gradient_from_angle() {
        let gradient = parse("conic-gradient(from 90deg at 50% 50%, red, blue)").unwrap();
        assert_eq!(gradient.kind, GradientKind::Conic);
        assert_eq!(gradient.angle, Some(90.0));
        assert_eq!(
            gradient.center,
            Some((Length::Percent(50.0), Length::Percent(50.0)))
        );
        // Default `from` angle is 0deg.
        assert_eq!(parse("conic-gradient(red, blue)").unwrap().angle, Some(0.0));
    }

    #[test]
    fn functions_colors_and_positions_parse() {
        let gradient = parse("linear-gradient(to right, rgb(255 0 0) 10%, #00f)").unwrap();
        assert_eq!(gradient.stops[0].color, Color::opaque(255, 0, 0));
        assert_eq!(
            gradient.stops[0].position,
            Some(StopPosition::Percent(10.0))
        );
        assert_eq!(gradient.stops[1].color, Color::opaque(0, 0, 255));
        assert_eq!(gradient.stops[1].position, None);
    }

    #[test]
    fn malformed_gradients_are_rejected() {
        assert_eq!(parse("linear-gradient(red)"), None);
        assert_eq!(parse("linear-gradient()"), None);
        assert_eq!(parse("linear-gradient(red, blue) red"), None);
        assert_eq!(parse("linear-gradient(to top bottom, red, blue)"), None);
        assert_eq!(
            parse("radial-gradient(circle 50px at 0 0, red, blue)"),
            None
        );
        assert_eq!(parse("linear-gradient(to right, red 10x, blue)"), None);
        assert_eq!(parse("gradient(red, blue)"), None);
        assert_eq!(parse("red"), None);
    }
}
