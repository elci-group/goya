//! Keyframe value interpolation (directive GOYA-DIS-CASA-001).
//!
//! Purpose: produce the value a property holds between two keyframes at a
//! given progress. Progressive scope: numbers, pixel lengths, percentages
//! and colors interpolate by channel/linear lerp; every other value kind
//! (mismatched units, keywords, malformed input) steps discretely at the
//! halfway point, exactly as CSS discrete animation behaves. Determinism:
//! interpolation is a pure function of its arguments — `f64` lerp and
//! half-away-from-zero rounding only, no time or randomness. Diagnostics:
//! non-interpolable pairs emit `CASA-ANI-001` when a `&mut Reporter` is
//! supplied; malformed values are reported, never guessed at.

use crate::appearance::colors::{parse_color, Color};
use crate::diagnostics::codes::ANIMATION_UNINTERPOLABLE;
use crate::diagnostics::reporter::Reporter;
use crate::geometry::Px;

/// The value of one animated property at one point of the trajectory.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatedValue {
    /// A unitless number (opacity, font-weight factor, ...).
    Number(f64),
    /// An absolute length in CSS reference pixels.
    Length(Px),
    /// A percentage of a runtime reference, kept symbolic.
    Percent(f64),
    /// A color, interpolated channel-wise.
    Color(Color),
    /// A non-interpolable value, held until the halfway point.
    Discrete(String),
}

/// One parsed CSS value, before interpolation.
#[derive(Debug, Clone, PartialEq)]
enum ParsedValue {
    Number(f64),
    Px(f64),
    Percent(f64),
    Color(Color),
    Other(String),
}

impl ParsedValue {
    fn kind(&self) -> u8 {
        match self {
            ParsedValue::Number(_) => 0,
            ParsedValue::Px(_) => 1,
            ParsedValue::Percent(_) => 2,
            ParsedValue::Color(_) => 3,
            ParsedValue::Other(_) => 4,
        }
    }
}

/// Interpolate between two keyframe declaration values at progress `t`
/// (clamped to 0..=1). Non-interpolable or mismatched pairs step
/// discretely: `t < 0.5` yields the `from` value, `t >= 0.5` the `to`
/// value, and `CASA-ANI-001` is emitted when a reporter is passed.
pub fn interpolate(
    from: &str,
    to: &str,
    t: f64,
    reporter: Option<&mut Reporter>,
) -> InterpolatedValue {
    let t = t.clamp(0.0, 1.0);
    let parsed_from = parse_value(from);
    let parsed_to = parse_value(to);
    if parsed_from.kind() != parsed_to.kind() {
        report_uninterpolable(reporter, from, to);
        return discrete_step(from, to, t);
    }
    match (parsed_from, parsed_to) {
        (ParsedValue::Number(a), ParsedValue::Number(b)) => {
            InterpolatedValue::Number(lerp(a, b, t))
        }
        (ParsedValue::Px(a), ParsedValue::Px(b)) => InterpolatedValue::Length(lerp(a, b, t)),
        (ParsedValue::Percent(a), ParsedValue::Percent(b)) => {
            InterpolatedValue::Percent(lerp(a, b, t))
        }
        (ParsedValue::Color(a), ParsedValue::Color(b)) => {
            InterpolatedValue::Color(lerp_color(&a, &b, t))
        }
        _ => {
            report_uninterpolable(reporter, from, to);
            discrete_step(from, to, t)
        }
    }
}

/// Classify one CSS value string into the strongest interpolation kind it
/// supports. Colors win over numbers (named colors), then bare numbers,
/// then dimensioned lengths and percentages; anything else is discrete.
fn parse_value(value: &str) -> ParsedValue {
    let trimmed = value.trim();
    if let Some(color) = parse_color(trimmed) {
        return ParsedValue::Color(color);
    }
    if let Ok(number) = trimmed.parse::<f64>() {
        if number.is_finite() {
            return ParsedValue::Number(number);
        }
    }
    if trimmed.is_empty() {
        return ParsedValue::Other(String::new());
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(number) = lower
        .strip_suffix("px")
        .and_then(|n| n.trim().parse::<f64>().ok())
    {
        if number.is_finite() {
            return ParsedValue::Px(number);
        }
    }
    if let Some(number) = lower
        .strip_suffix('%')
        .and_then(|n| n.trim().parse::<f64>().ok())
    {
        if number.is_finite() {
            return ParsedValue::Percent(number);
        }
    }
    ParsedValue::Other(trimmed.to_string())
}

fn discrete_step(from: &str, to: &str, t: f64) -> InterpolatedValue {
    let held = if t < 0.5 { from } else { to };
    InterpolatedValue::Discrete(held.trim().to_string())
}

fn report_uninterpolable(reporter: Option<&mut Reporter>, from: &str, to: &str) {
    if let Some(reporter) = reporter {
        reporter.emit(
            ANIMATION_UNINTERPOLABLE,
            format!("values {from:?} and {to:?} cannot be interpolated; the property steps at 50%"),
            None,
        );
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_color(from: &Color, to: &Color, t: f64) -> Color {
    Color {
        r: lerp_channel(from.r, to.r, t),
        g: lerp_channel(from.g, to.g, t),
        b: lerp_channel(from.b, to.b, t),
        a: lerp(from.a, to.a, t),
    }
}

fn lerp_channel(from: u8, to: u8, t: f64) -> u8 {
    (f64::from(from) + (f64::from(to) - f64::from(from)) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reported(from: &str, to: &str, t: f64) -> (InterpolatedValue, Reporter) {
        let mut reporter = Reporter::new();
        let value = interpolate(from, to, t, Some(&mut reporter));
        (value, reporter)
    }

    #[test]
    fn numbers_interpolate() {
        assert_eq!(
            interpolate("0", "1", 0.25, None),
            InterpolatedValue::Number(0.25)
        );
        assert_eq!(
            interpolate("2.5", "-2.5", 0.5, None),
            InterpolatedValue::Number(0.0)
        );
    }

    #[test]
    fn lengths_and_percentages_interpolate() {
        assert_eq!(
            interpolate("10px", "30px", 0.5, None),
            InterpolatedValue::Length(20.0)
        );
        assert_eq!(
            interpolate("0%", "100%", 0.75, None),
            InterpolatedValue::Percent(75.0)
        );
    }

    #[test]
    fn colors_interpolate_per_channel() {
        let mid = interpolate("red", "blue", 0.5, None);
        assert_eq!(
            mid,
            InterpolatedValue::Color(Color {
                r: 128,
                g: 0,
                b: 128,
                a: 1.0
            })
        );
        let end = interpolate("#ff0000", "#0000ff", 1.0, None);
        assert_eq!(
            end,
            InterpolatedValue::Color(Color {
                r: 0,
                g: 0,
                b: 255,
                a: 1.0
            })
        );
    }

    #[test]
    fn progress_clamps_to_unit_range() {
        assert_eq!(
            interpolate("0", "10", -1.0, None),
            InterpolatedValue::Number(0.0)
        );
        assert_eq!(
            interpolate("0", "10", 2.0, None),
            InterpolatedValue::Number(10.0)
        );
    }

    #[test]
    fn mismatched_units_step_discretely() {
        let (value, reporter) = reported("10px", "50%", 0.9);
        assert_eq!(value, InterpolatedValue::Discrete("50%".to_string()));
        assert_eq!(reporter.len(), 1);
        assert_eq!(reporter.diagnostics()[0].code, "CASA-ANI-001");
    }

    #[test]
    fn malformed_values_step_and_report() {
        let (value, reporter) = reported("banana", "42", 0.4);
        assert_eq!(value, InterpolatedValue::Discrete("banana".to_string()));
        assert_eq!(reporter.len(), 1);
        let (below_half, _) = reported("banana", "42", 0.4);
        assert_eq!(
            below_half,
            InterpolatedValue::Discrete("banana".to_string())
        );
        let (above_half, _) = reported("banana", "42", 0.6);
        assert_eq!(above_half, InterpolatedValue::Discrete("42".to_string()));
    }

    #[test]
    fn identical_discrete_values_still_step() {
        let (value, reporter) = reported("auto", "auto", 0.2);
        assert_eq!(value, InterpolatedValue::Discrete("auto".to_string()));
        assert_eq!(reporter.len(), 1);
    }

    #[test]
    fn interpolation_is_deterministic() {
        let first = interpolate("10px", "20px", 0.3, None);
        let second = interpolate("10px", "20px", 0.3, None);
        assert_eq!(first, second);
    }
}
