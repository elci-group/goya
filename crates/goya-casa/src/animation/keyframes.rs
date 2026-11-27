//! Keyframe rule parsing (directive GOYA-DIS-CASA-001).
//!
//! Purpose: lift a `@keyframes` IR rule into the typed [`Animation`] model
//! the animation pipeline consumes. Progressive scope: `from`/`to` and
//! percentage offsets (including comma-expanded selectors such as
//! `0%, 100%`) are supported; malformed or out-of-range offsets are
//! reported and skipped, duplicate offsets are merged (later declarations
//! win, matching CSS), and steps are normalised to ascending offset order.
//! Determinism: offsets are `f64` in 0..=1, declarations live in a
//! [`BTreeMap`], and sorting is stable — identical input rules always
//! produce identical `Animation` values. A per-property [`PropertyKind`]
//! helper classifies values as interpolable (length, color, number) or
//! discrete.

use std::collections::BTreeMap;

use crate::appearance::colors::parse_color;
use crate::diagnostics::codes::SYNTAX_INVALID;
use crate::diagnostics::reporter::Reporter;
use crate::geometry::dimensional;
use crate::geometry::{Context, Length};
use crate::ir::casa_ir::{KeyframeStep, KeyframesRule};

/// A parsed `@keyframes` rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// The keyframes name, without the `@keyframes` prefix.
    pub name: String,
    /// Steps in ascending offset order; offsets are in 0..=1.
    pub steps: Vec<AnimationStep>,
}

/// One keyframe: a normalized offset plus its declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationStep {
    /// 0.0 for `from`/`0%`, 1.0 for `to`/`100%`, otherwise `N% / 100`.
    pub offset: f64,
    /// Property name (lowercased) → value as written.
    pub declarations: BTreeMap<String, String>,
}

/// How a property's keyframe values can be animated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    /// Unitless numbers interpolate linearly.
    Number,
    /// Lengths (px and resolved units) and percentages interpolate.
    Length,
    /// Colors interpolate channel-wise.
    Color,
    /// Keywords and anything unparseable steps discretely.
    Discrete,
}

impl PropertyKind {
    /// Whether a pair of values of this kind can be interpolated at all.
    pub fn is_interpolable(self) -> bool {
        self != PropertyKind::Discrete
    }
}

/// Parse a `@keyframes` IR rule into an [`Animation`]. Malformed or
/// out-of-range offsets and non-monotonic step order are reported via
/// `CASA-SYN-001` and handled deterministically (skip / sort); no path
/// panics or guesses.
pub fn parse_keyframes(rule: &KeyframesRule, reporter: &mut Reporter) -> Animation {
    let mut expanded: Vec<AnimationStep> = Vec::new();
    let mut non_monotonic = false;
    let mut previous_offset: Option<f64> = None;
    for step in &rule.steps {
        for offset in parse_offsets(&step.offsets, reporter) {
            if let Some(previous) = previous_offset {
                if offset < previous {
                    non_monotonic = true;
                }
            }
            previous_offset = Some(offset);
            expanded.push(AnimationStep {
                offset,
                declarations: declarations_of(step),
            });
        }
    }
    if non_monotonic {
        reporter.emit(
            SYNTAX_INVALID,
            format!(
                "keyframes {:?} lists offsets out of ascending order; the steps were reordered",
                rule.name
            ),
            None,
        );
    }
    // Stable sort keeps source order for equal offsets, then merge
    // duplicates with later declarations winning, matching CSS.
    expanded.sort_by(|a, b| {
        a.offset
            .partial_cmp(&b.offset)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut merged: Vec<AnimationStep> = Vec::new();
    for step in expanded {
        match merged.last_mut() {
            Some(last) if last.offset == step.offset => {
                for (name, value) in step.declarations {
                    last.declarations.insert(name, value);
                }
            }
            _ => merged.push(step),
        }
    }
    Animation {
        name: rule.name.clone(),
        steps: merged,
    }
}

/// Expand one selector list (`"from"`, `"0%"`, `"0%, 100%"`) into
/// normalized offsets. Malformed or out-of-range selectors are reported
/// and dropped; if none remain the whole step is skipped.
fn parse_offsets(offsets: &str, reporter: &mut Reporter) -> Vec<f64> {
    let mut parsed = Vec::new();
    for selector in offsets.split(',') {
        let selector = selector.trim();
        let offset = match selector.to_ascii_lowercase().as_str() {
            "from" => Some(0.0),
            "to" => Some(1.0),
            _ => selector
                .strip_suffix('%')
                .and_then(|number| number.trim().parse::<f64>().ok())
                .filter(|number| number.is_finite())
                .map(|number| number / 100.0),
        };
        match offset {
            Some(offset) if (0.0..=1.0).contains(&offset) => parsed.push(offset),
            _ => reporter.emit(
                SYNTAX_INVALID,
                format!(
                    "keyframe selector {selector:?} is not a valid offset in 0..=1; it was skipped"
                ),
                None,
            ),
        }
    }
    parsed
}

fn declarations_of(step: &KeyframeStep) -> BTreeMap<String, String> {
    step.declarations
        .iter()
        .map(|property| {
            (
                property.name.trim().to_ascii_lowercase(),
                property.value.trim().to_string(),
            )
        })
        .collect()
}

/// Classify a keyframe value: colors first (named colors are idents),
/// then bare numbers, then lengths/percentages, otherwise discrete.
pub fn property_kind(value: &str, context: &Context) -> PropertyKind {
    let trimmed = value.trim();
    if parse_color(trimmed).is_some() {
        return PropertyKind::Color;
    }
    if let Ok(number) = trimmed.parse::<f64>() {
        if number.is_finite() {
            return PropertyKind::Number;
        }
    }
    match dimensional::parse(trimmed, context) {
        Some(Length::Px(_)) | Some(Length::Percent(_)) => PropertyKind::Length,
        _ => PropertyKind::Discrete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::casa_ir::DeclaredProperty;

    fn context() -> Context {
        Context {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    fn rule(name: &str, steps: Vec<(&str, Vec<(&str, &str)>)>) -> KeyframesRule {
        KeyframesRule {
            name: name.to_string(),
            steps: steps
                .into_iter()
                .map(|(offsets, declarations)| KeyframeStep {
                    offsets: offsets.to_string(),
                    declarations: declarations
                        .into_iter()
                        .map(|(name, value)| DeclaredProperty {
                            name: name.to_string(),
                            value: value.to_string(),
                            important: false,
                            start: 0,
                            end: 0,
                            category: crate::ir::value_categories::ValueCategory::Declared,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn from_to_parse_to_unit_range() {
        let mut reporter = Reporter::new();
        let animation = parse_keyframes(
            &rule(
                "fade",
                vec![
                    ("from", vec![("opacity", "0")]),
                    ("to", vec![("opacity", "1")]),
                ],
            ),
            &mut reporter,
        );
        assert_eq!(animation.name, "fade");
        assert_eq!(animation.steps.len(), 2);
        assert_eq!(animation.steps[0].offset, 0.0);
        assert_eq!(animation.steps[1].offset, 1.0);
        assert_eq!(animation.steps[0].declarations["opacity"], "0");
        assert_eq!(reporter.len(), 0);
    }

    #[test]
    fn comma_selectors_expand_sharing_declarations() {
        let mut reporter = Reporter::new();
        let animation = parse_keyframes(
            &rule(
                "pulse",
                vec![
                    ("0%, 100%", vec![("transform", "scale(1)")]),
                    ("50%", vec![("transform", "scale(1.5)")]),
                ],
            ),
            &mut reporter,
        );
        let offsets: Vec<f64> = animation.steps.iter().map(|step| step.offset).collect();
        assert_eq!(offsets, vec![0.0, 0.5, 1.0]);
        assert_eq!(
            animation.steps[0].declarations,
            animation.steps[2].declarations
        );
    }

    #[test]
    fn malformed_and_out_of_range_offsets_are_reported_and_skipped() {
        let mut reporter = Reporter::new();
        let animation = parse_keyframes(
            &rule(
                "bad",
                vec![
                    ("banana", vec![("opacity", "0")]),
                    ("-25%", vec![("opacity", "0")]),
                    ("150%", vec![("opacity", "0")]),
                    ("100%", vec![("opacity", "1")]),
                ],
            ),
            &mut reporter,
        );
        assert_eq!(animation.steps.len(), 1);
        assert_eq!(animation.steps[0].offset, 1.0);
        assert_eq!(reporter.len(), 3);
        assert_eq!(reporter.diagnostics()[0].code, "CASA-SYN-001");
    }

    #[test]
    fn non_monotonic_order_is_reported_and_normalized() {
        let mut reporter = Reporter::new();
        let animation = parse_keyframes(
            &rule(
                "swap",
                vec![
                    ("75%", vec![("opacity", "0.75")]),
                    ("25%", vec![("opacity", "0.25")]),
                ],
            ),
            &mut reporter,
        );
        assert_eq!(reporter.len(), 1);
        let offsets: Vec<f64> = animation.steps.iter().map(|step| step.offset).collect();
        assert_eq!(offsets, vec![0.25, 0.75]);
        assert_eq!(animation.steps[0].declarations["opacity"], "0.25");
    }

    #[test]
    fn duplicate_offsets_merge_later_wins() {
        let mut reporter = Reporter::new();
        let animation = parse_keyframes(
            &rule(
                "dup",
                vec![
                    ("0%", vec![("opacity", "0"), ("color", "red")]),
                    ("0%", vec![("opacity", "1")]),
                ],
            ),
            &mut reporter,
        );
        assert_eq!(animation.steps.len(), 1);
        assert_eq!(animation.steps[0].declarations["opacity"], "1");
        assert_eq!(animation.steps[0].declarations["color"], "red");
    }

    #[test]
    fn property_kind_classifies_values() {
        let context = context();
        assert_eq!(property_kind("0.5", &context), PropertyKind::Number);
        assert_eq!(property_kind("12px", &context), PropertyKind::Length);
        assert_eq!(property_kind("50%", &context), PropertyKind::Length);
        assert_eq!(property_kind("red", &context), PropertyKind::Color);
        assert_eq!(property_kind("#aabbcc", &context), PropertyKind::Color);
        assert_eq!(property_kind("auto", &context), PropertyKind::Discrete);
        assert!(PropertyKind::Color.is_interpolable());
        assert!(!PropertyKind::Discrete.is_interpolable());
    }
}
