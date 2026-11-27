//! Animation classification: precomputed vs runtime (directive
//! GOYA-DIS-CASA-001).
//!
//! Purpose: decide, per `@keyframes` rule, whether the whole animation can
//! be precomputed into a [`Trajectory`] at transform time (GVI-A) or must
//! be deferred to the runtime player. Progressive scope: animations whose
//! keyframe values are all interpolable and free of runtime dependencies
//! precompute; `var()`/`env()`/`attr()` custom-property dependencies,
//! `calc()` expressions, percentages (their reference — containing block,
//! font metrics — is a runtime property) and non-interpolable values force
//! [`Classification::Runtime`]. Determinism: declarations are examined in
//! keyframe-offset then BTreeMap order, so the reported reason is the
//! first dependency in that stable order and identical inputs classify
//! identically. Diagnostics: deferral emits `CASA-ANI-002` once per
//! animation with the reason in the message.

use crate::animation::easing::Easing;
use crate::animation::keyframes::{property_kind, Animation, PropertyKind};
use crate::animation::trajectory::{sample_animation, Trajectory};
use crate::diagnostics::codes::ANIMATION_RUNTIME_DEFERRED;
use crate::diagnostics::reporter::Reporter;
use crate::geometry::dimensional;
use crate::geometry::{Context, Length};

/// Why an animation cannot be precomputed at transform time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeReason {
    /// A keyframe value references a custom property via `var()`.
    CustomPropertyDependency { property: String, value: String },
    /// A keyframe value is a `calc()`/`env()`/`attr()` expression.
    CalcDependency { property: String, value: String },
    /// A keyframe percentage resolves against a runtime-only reference.
    UnknownPercentage { property: String, value: String },
    /// A keyframe value cannot be interpolated at all.
    NonInterpolableProperty { property: String, value: String },
    /// The rule has no usable keyframes to precompute.
    EmptyAnimation,
}

impl RuntimeReason {
    /// Human-readable description used in the diagnostic message.
    pub fn description(&self) -> String {
        match self {
            RuntimeReason::CustomPropertyDependency { property, .. } => {
                format!("property {property:?} depends on a custom property (var())")
            }
            RuntimeReason::CalcDependency { property, .. } => {
                format!("property {property:?} uses a calc()/env()/attr() expression")
            }
            RuntimeReason::UnknownPercentage { property, .. } => {
                format!("property {property:?} resolves a percentage against a runtime reference")
            }
            RuntimeReason::NonInterpolableProperty { property, .. } => {
                format!("property {property:?} is not interpolable")
            }
            RuntimeReason::EmptyAnimation => "the rule has no usable keyframes".to_string(),
        }
    }
}

/// The transform-time disposition of one animation.
#[derive(Debug, Clone, PartialEq)]
pub enum Classification {
    /// Fully precomputable: the sampled trajectory is embedded.
    Precomputed(Trajectory),
    /// Deferred to the runtime player, with the reason why.
    Runtime { reason: RuntimeReason },
}

/// Classify an animation with the default (linear) timing function.
/// Use [`classify_with_easing`] when the `animation-timing-function` is
/// already resolved; the classification decision itself does not depend
/// on the easing.
pub fn classify(
    animation: &Animation,
    context: &Context,
    reporter: &mut Reporter,
) -> Classification {
    classify_with_easing(animation, &Easing::Linear, context, reporter)
}

/// Classify an animation through a resolved timing function. Returns
/// [`Classification::Precomputed`] with the sampled trajectory when every
/// keyframe value is interpolable and free of runtime dependencies;
/// otherwise emits `CASA-ANI-002` and returns [`Classification::Runtime`]
/// with the first reason in deterministic (offset, property) order.
pub fn classify_with_easing(
    animation: &Animation,
    easing: &Easing,
    context: &Context,
    reporter: &mut Reporter,
) -> Classification {
    if let Some(reason) = blocking_reason(animation, context) {
        reporter.emit(
            ANIMATION_RUNTIME_DEFERRED,
            format!(
                "animation {:?} is classified for runtime evaluation: {}",
                animation.name,
                reason.description()
            ),
            None,
        );
        return Classification::Runtime { reason };
    }
    Classification::Precomputed(sample_animation(animation, easing, reporter))
}

/// The first reason (in stable keyframe-offset, property order) that
/// blocks precomputation, if any.
fn blocking_reason(animation: &Animation, context: &Context) -> Option<RuntimeReason> {
    if animation.steps.is_empty() {
        return Some(RuntimeReason::EmptyAnimation);
    }
    for step in &animation.steps {
        for (property, value) in &step.declarations {
            if contains_function(value, "var(") {
                return Some(RuntimeReason::CustomPropertyDependency {
                    property: property.clone(),
                    value: value.clone(),
                });
            }
            if contains_function(value, "calc(")
                || contains_function(value, "env(")
                || contains_function(value, "attr(")
            {
                return Some(RuntimeReason::CalcDependency {
                    property: property.clone(),
                    value: value.clone(),
                });
            }
            if matches!(dimensional::parse(value, context), Some(Length::Percent(_))) {
                return Some(RuntimeReason::UnknownPercentage {
                    property: property.clone(),
                    value: value.clone(),
                });
            }
            if property_kind(value, context) == PropertyKind::Discrete {
                return Some(RuntimeReason::NonInterpolableProperty {
                    property: property.clone(),
                    value: value.clone(),
                });
            }
        }
    }
    None
}

/// Case-insensitive search for a function token such as `var(`.
fn contains_function(value: &str, marker: &str) -> bool {
    value.to_ascii_lowercase().contains(marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::keyframes::parse_keyframes;
    use crate::ir::casa_ir::{DeclaredProperty, KeyframeStep, KeyframesRule};
    use crate::ir::value_categories::ValueCategory;

    fn context() -> Context {
        Context {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    fn animation(steps: Vec<(&str, Vec<(&str, &str)>)>) -> Animation {
        parse_keyframes(
            &KeyframesRule {
                name: "test".to_string(),
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
                                category: ValueCategory::Declared,
                            })
                            .collect(),
                    })
                    .collect(),
            },
            &mut Reporter::new(),
        )
    }

    #[test]
    fn interpolable_animation_precomputes() {
        let mut reporter = Reporter::new();
        let fade = animation(vec![
            ("from", vec![("opacity", "0"), ("margin-left", "0px")]),
            ("to", vec![("opacity", "1"), ("margin-left", "32px")]),
        ]);
        let classification = classify(&fade, &context(), &mut reporter);
        match classification {
            Classification::Precomputed(trajectory) => {
                assert!(!trajectory.samples.is_empty());
            }
            other => panic!("expected Precomputed, got {other:?}"),
        }
        assert_eq!(reporter.len(), 0);
    }

    #[test]
    fn var_dependency_defers_with_diagnostic() {
        let mut reporter = Reporter::new();
        let themed = animation(vec![
            ("from", vec![("color", "red")]),
            ("to", vec![("color", "var(--accent)")]),
        ]);
        let classification = classify(&themed, &context(), &mut reporter);
        assert_eq!(
            classification,
            Classification::Runtime {
                reason: RuntimeReason::CustomPropertyDependency {
                    property: "color".to_string(),
                    value: "var(--accent)".to_string(),
                }
            }
        );
        assert_eq!(reporter.len(), 1);
        assert_eq!(reporter.diagnostics()[0].code, "CASA-ANI-002");
    }

    #[test]
    fn calc_dependency_defers() {
        let mut reporter = Reporter::new();
        let computed = animation(vec![
            ("from", vec![("margin-left", "0px")]),
            ("to", vec![("margin-left", "calc(1em + 4px)")]),
        ]);
        let classification = classify(&computed, &context(), &mut reporter);
        assert!(matches!(
            classification,
            Classification::Runtime {
                reason: RuntimeReason::CalcDependency { .. }
            }
        ));
        assert_eq!(reporter.len(), 1);
    }

    #[test]
    fn percentages_defer_to_runtime() {
        let mut reporter = Reporter::new();
        let relative = animation(vec![
            ("from", vec![("translate", "0px")]),
            ("to", vec![("translate", "50%")]),
        ]);
        let classification = classify(&relative, &context(), &mut reporter);
        assert!(matches!(
            classification,
            Classification::Runtime {
                reason: RuntimeReason::UnknownPercentage { .. }
            }
        ));
    }

    #[test]
    fn non_interpolable_property_defers() {
        let mut reporter = Reporter::new();
        let toggling = animation(vec![
            ("from", vec![("display", "none")]),
            ("to", vec![("display", "block")]),
        ]);
        let classification = classify(&toggling, &context(), &mut reporter);
        assert_eq!(
            classification,
            Classification::Runtime {
                reason: RuntimeReason::NonInterpolableProperty {
                    property: "display".to_string(),
                    value: "none".to_string(),
                }
            }
        );
    }

    #[test]
    fn empty_animation_defers() {
        let mut reporter = Reporter::new();
        let empty = animation(vec![]);
        let classification = classify(&empty, &context(), &mut reporter);
        assert_eq!(
            classification,
            Classification::Runtime {
                reason: RuntimeReason::EmptyAnimation
            }
        );
    }

    #[test]
    fn default_easing_matches_linear_explicitly() {
        let mut reporter = Reporter::new();
        let fade = animation(vec![
            ("from", vec![("opacity", "0")]),
            ("to", vec![("opacity", "1")]),
        ]);
        let default = classify(&fade, &context(), &mut reporter);
        let mut second = Reporter::new();
        let explicit = classify_with_easing(&fade, &Easing::Linear, &context(), &mut second);
        assert_eq!(default, explicit);
    }
}
