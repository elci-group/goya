//! Trajectory encoding — the GVI-A precomputed animation representation
//! (directive GOYA-DIS-CASA-001).
//!
//! Purpose: compile an [`Animation`] into the compact, fixed-size sample
//! list a runtime player consumes without re-running the easing or the
//! interpolator. Progressive scope: [`sample_animation`] evaluates the
//! animation at a fixed, deterministic set of times (`i / 20` for
//! 0..=20 samples) through the easing, and drops samples identical to
//! their predecessor so constant spans collapse. Determinism: the sample
//! times, the easing solver and the interpolation are all pure —
//! identical animations always yield byte-identical trajectories. The
//! trajectory keeps `t` values in ascending order with the eased progress
//! baked into the property values; players only lerp between neighbours.

use std::collections::BTreeMap;

use crate::animation::easing::Easing;
use crate::animation::interpolation::{interpolate, InterpolatedValue};
use crate::animation::keyframes::Animation;
use crate::diagnostics::reporter::Reporter;

/// Number of trajectory samples (including both endpoints).
pub const SAMPLE_COUNT: usize = 21;

/// One precomputed point of the animation: raw time `t` in 0..=1 plus the
/// already-eased, already-interpolated property values.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// Raw time in 0..=1, sampled at `i / (SAMPLE_COUNT - 1)`.
    pub t: f64,
    /// Property name → value at this time.
    pub properties: BTreeMap<String, InterpolatedValue>,
}

/// The compact precomputed representation runtime players consume
/// (GVI-A framing): property trajectories sampled on a fixed grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Trajectory {
    /// The timing function the samples were evaluated through.
    pub easing: Easing,
    /// Samples in ascending `t`; identical consecutive samples removed.
    pub samples: Vec<Sample>,
}

impl Trajectory {
    /// The fixed, deterministic sample times (0, 1/20, ..., 1).
    pub fn sample_times() -> Vec<f64> {
        (0..SAMPLE_COUNT)
            .map(|i| i as f64 / (SAMPLE_COUNT - 1) as f64)
            .collect()
    }
}

/// Sample an animation through `easing` at the fixed times. Property
/// values are evaluated with the eased progress mapped onto the keyframe
/// offsets; non-interpolable pairs step discretely and report
/// `CASA-ANI-001` through the reporter.
pub fn sample_animation(
    animation: &Animation,
    easing: &Easing,
    reporter: &mut Reporter,
) -> Trajectory {
    // Per-property (offset, value) series, in ascending offset order.
    let mut series: Vec<(String, Vec<(f64, &str)>)> = Vec::new();
    let mut names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for step in &animation.steps {
        for name in step.declarations.keys() {
            names.insert(name.as_str());
        }
    }
    for name in names {
        let mut points: Vec<(f64, &str)> = animation
            .steps
            .iter()
            .filter_map(|step| {
                step.declarations
                    .get(name)
                    .map(|value| (step.offset, value.as_str()))
            })
            .collect();
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        series.push((name.to_string(), points));
    }

    let mut samples = Vec::new();
    for t in Trajectory::sample_times() {
        let eased = easing.sample(t);
        let mut properties = BTreeMap::new();
        for (name, points) in &series {
            properties.insert(name.clone(), value_at(points, eased, reporter));
        }
        let redundant = samples
            .last()
            .is_some_and(|previous: &Sample| previous.properties == properties);
        if !redundant {
            samples.push(Sample { t, properties });
        }
    }
    Trajectory {
        easing: *easing,
        samples,
    }
}

/// The property value at eased progress `progress`, given the property's
/// (offset, value) points in ascending offset order. Progress before the
/// first or after the last offset holds the boundary value; between two
/// offsets the pair interpolates on the local segment.
fn value_at(points: &[(f64, &str)], progress: f64, reporter: &mut Reporter) -> InterpolatedValue {
    let (from_offset, from) = points[0];
    if progress <= from_offset || points.len() == 1 {
        return interpolate(from, from, 0.0, None);
    }
    let (to_offset, to) = points[points.len() - 1];
    if progress >= to_offset {
        return interpolate(to, to, 1.0, None);
    }
    let mut upper = 1;
    while points[upper].0 < progress {
        upper += 1;
    }
    let (start_offset, start) = points[upper - 1];
    let (end_offset, end) = points[upper];
    let local = if end_offset > start_offset {
        (progress - start_offset) / (end_offset - start_offset)
    } else {
        1.0
    };
    interpolate(start, end, local, Some(reporter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::keyframes::parse_keyframes;
    use crate::ir::casa_ir::{DeclaredProperty, KeyframeStep, KeyframesRule};
    use crate::ir::value_categories::ValueCategory;

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
    fn fixed_sample_times_are_deterministic() {
        let times = Trajectory::sample_times();
        assert_eq!(times.len(), SAMPLE_COUNT);
        assert_eq!(times[0], 0.0);
        assert_eq!(times[SAMPLE_COUNT - 1], 1.0);
        assert_eq!(times, Trajectory::sample_times());
    }

    #[test]
    fn linear_animation_samples_endpoints() {
        let mut reporter = Reporter::new();
        let fade = animation(vec![
            ("from", vec![("opacity", "0")]),
            ("to", vec![("opacity", "1")]),
        ]);
        let trajectory = sample_animation(&fade, &Easing::Linear, &mut reporter);
        assert_eq!(trajectory.samples.len(), SAMPLE_COUNT);
        let first = &trajectory.samples[0];
        assert_eq!(first.properties["opacity"], InterpolatedValue::Number(0.0));
        let last = trajectory.samples.last().unwrap();
        assert_eq!(last.properties["opacity"], InterpolatedValue::Number(1.0));
        let mid = &trajectory.samples[SAMPLE_COUNT / 2];
        assert_eq!(mid.properties["opacity"], InterpolatedValue::Number(0.5));
    }

    #[test]
    fn constant_animation_collapses_to_one_sample() {
        let mut reporter = Reporter::new();
        let flat = animation(vec![
            ("from", vec![("opacity", "0.5")]),
            ("to", vec![("opacity", "0.5")]),
        ]);
        let trajectory = sample_animation(&flat, &Easing::Linear, &mut reporter);
        assert_eq!(trajectory.samples.len(), 1);
        assert_eq!(
            trajectory.samples[0].properties["opacity"],
            InterpolatedValue::Number(0.5)
        );
    }

    #[test]
    fn easing_bakes_into_samples() {
        let mut reporter = Reporter::new();
        let fade = animation(vec![
            ("from", vec![("opacity", "0")]),
            ("to", vec![("opacity", "1")]),
        ]);
        let linear = sample_animation(&fade, &Easing::Linear, &mut reporter);
        let stepped = sample_animation(
            &fade,
            &Easing::Steps {
                count: 4,
                jump_start: false,
            },
            &mut reporter,
        );
        // A stepped easing yields few distinct values, so dedup compresses.
        assert!(stepped.samples.len() < linear.samples.len());
        // Eased progress at t = 0.3 for linear is exactly 0.3.
        let sample = linear.samples.iter().find(|s| s.t == 0.3).unwrap();
        assert_eq!(sample.properties["opacity"], InterpolatedValue::Number(0.3));
    }

    #[test]
    fn progress_holds_outside_keyframe_range() {
        let mut reporter = Reporter::new();
        let partial = animation(vec![
            ("25%", vec![("opacity", "0.2")]),
            ("75%", vec![("opacity", "0.8")]),
        ]);
        let trajectory = sample_animation(&partial, &Easing::Linear, &mut reporter);
        let first = &trajectory.samples[0];
        assert_eq!(first.properties["opacity"], InterpolatedValue::Number(0.2));
        let last = trajectory.samples.last().unwrap();
        assert_eq!(last.properties["opacity"], InterpolatedValue::Number(0.8));
    }

    #[test]
    fn multi_property_trajectories_keep_all_properties() {
        let mut reporter = Reporter::new();
        let both = animation(vec![
            ("from", vec![("opacity", "0"), ("color", "red")]),
            ("to", vec![("opacity", "1"), ("color", "blue")]),
        ]);
        let trajectory = sample_animation(&both, &Easing::Linear, &mut reporter);
        let mid = &trajectory.samples[SAMPLE_COUNT / 2];
        assert_eq!(mid.properties["opacity"], InterpolatedValue::Number(0.5));
        assert_eq!(
            mid.properties["color"],
            InterpolatedValue::Color(crate::appearance::colors::Color {
                r: 128,
                g: 0,
                b: 128,
                a: 1.0
            })
        );
    }
}
