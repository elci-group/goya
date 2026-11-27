//! Easing function parsing and sampling (directive GOYA-DIS-CASA-001).
//!
//! Purpose: turn the CSS `animation-timing-function` grammar into a typed
//! [`Easing`] value and sample it deterministically. Progressive scope: the
//! CSS keyword easings, `cubic-bezier()` (x coordinates constrained to
//! 0..=1 per CSS) and `steps()` are represented; `linear()` shorthand and
//! other exotic grammars parse to `None` and leave the decision to the
//! caller. Determinism: [`Easing::sample`] is a pure function of its
//! arguments — the bézier is solved by a fixed 48-iteration bisection, so
//! identical inputs always produce bit-identical outputs, with no time,
//! randomness or iteration-order dependence.

/// A CSS timing function.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier { x1: f64, y1: f64, x2: f64, y2: f64 },
    Steps { count: u32, jump_start: bool },
}

impl Easing {
    /// Parse one `animation-timing-function` value. Malformed or
    /// unsupported values return `None`; callers decide whether to fall
    /// back (the CSS initial value is `Easing::Ease`) and report.
    pub fn parse(value: &str) -> Option<Easing> {
        let trimmed = value.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "linear" => return Some(Easing::Linear),
            "ease" => return Some(Easing::Ease),
            "ease-in" => return Some(Easing::EaseIn),
            "ease-out" => return Some(Easing::EaseOut),
            "ease-in-out" => return Some(Easing::EaseInOut),
            _ => {}
        }
        if trimmed.is_empty() {
            return None;
        }
        if let Some(inner) = function_arguments(trimmed, "cubic-bezier") {
            return parse_cubic_bezier(inner);
        }
        if let Some(inner) = function_arguments(trimmed, "steps") {
            return parse_steps(inner);
        }
        None
    }

    /// Sample the easing at progress `t` in 0..=1, returning eased
    /// progress in 0..=1. `t` outside 0..=1 is clamped, so the result is
    /// always in range.
    pub fn sample(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Easing::Linear => t,
            Easing::Ease => cubic_bezier_progress(0.25, 0.1, 0.25, 1.0, t),
            Easing::EaseIn => cubic_bezier_progress(0.42, 0.0, 1.0, 1.0, t),
            Easing::EaseOut => cubic_bezier_progress(0.0, 0.0, 0.58, 1.0, t),
            Easing::EaseInOut => cubic_bezier_progress(0.42, 0.0, 0.58, 1.0, t),
            Easing::CubicBezier { x1, y1, x2, y2 } => cubic_bezier_progress(x1, y1, x2, y2, t),
            Easing::Steps { count, jump_start } => {
                let n = count as f64;
                if t >= 1.0 {
                    return 1.0;
                }
                let scaled = t * n;
                let index = if jump_start {
                    (scaled.floor() + 1.0).min(n)
                } else {
                    scaled.floor().max(0.0)
                };
                index / n
            }
        }
    }
}

/// Extract the argument text of `name(...)` if `value` is exactly that
/// function (case-insensitive), otherwise `None`.
fn function_arguments<'a>(value: &'a str, name: &'a str) -> Option<&'a str> {
    let lower = value.to_ascii_lowercase();
    let rest = lower.strip_prefix(name)?;
    if rest.len() != value.len() - name.len() {
        return None;
    }
    let rest = &value[name.len()..];
    let inner = rest.strip_prefix('(')?;
    inner.strip_suffix(')')
}

fn parse_cubic_bezier(inner: &str) -> Option<Easing> {
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    let numbers: Vec<f64> = parts
        .iter()
        .map(|part| part.trim().parse::<f64>().ok())
        .collect::<Option<Vec<f64>>>()?;
    if numbers.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let [x1, y1, x2, y2] = numbers[..] else {
        return None;
    };
    // CSS requires the x control points to lie in 0..=1; y is unbounded.
    if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
        return None;
    }
    Some(Easing::CubicBezier { x1, y1, x2, y2 })
}

fn parse_steps(inner: &str) -> Option<Easing> {
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.is_empty() || parts.len() > 2 {
        return None;
    }
    let count = parts[0].trim().parse::<u32>().ok()?;
    if count == 0 {
        return None;
    }
    let jump_start = match parts.len() {
        1 => false, // CSS default is `end`.
        _ => match parts[1].trim().to_ascii_lowercase().as_str() {
            "start" | "jump-start" => true,
            "end" | "jump-end" => false,
            _ => return None,
        },
    };
    Some(Easing::Steps { count, jump_start })
}

/// Progress along a cubic bézier curve with P0=(0,0), P3=(1,1), solved by
/// bisection on the x component. A fixed iteration count keeps the result
/// bit-identical across runs and platforms.
fn cubic_bezier_progress(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if bezier_component(x1, x2, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let u = 0.5 * (lo + hi);
    bezier_component(y1, y2, u)
}

/// One component of the cubic bézier at parameter `u`, with P0=0, P3=1.
fn bezier_component(a: f64, b: f64, u: f64) -> f64 {
    let v = 1.0 - u;
    3.0 * v * v * u * a + 3.0 * v * u * u * b + u * u * u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_easings_parse() {
        assert_eq!(Easing::parse("linear"), Some(Easing::Linear));
        assert_eq!(Easing::parse(" ease-in "), Some(Easing::EaseIn));
        assert_eq!(Easing::parse("ease-in-out"), Some(Easing::EaseInOut));
    }

    #[test]
    fn malformed_values_reject() {
        assert_eq!(Easing::parse("banana"), None);
        assert_eq!(Easing::parse(""), None);
        assert_eq!(Easing::parse("cubic-bezier(0.1, 0.2)"), None);
        // x control points must lie in 0..=1.
        assert_eq!(Easing::parse("cubic-bezier(-1, 0, 2, 1)"), None);
        assert_eq!(Easing::parse("steps(0)"), None);
    }

    #[test]
    fn cubic_bezier_and_steps_parse() {
        assert_eq!(
            Easing::parse("cubic-bezier(0.25, 0.1, 0.25, 1.0)"),
            Some(Easing::CubicBezier {
                x1: 0.25,
                y1: 0.1,
                x2: 0.25,
                y2: 1.0
            })
        );
        assert_eq!(
            Easing::parse("steps(4)"),
            Some(Easing::Steps {
                count: 4,
                jump_start: false
            })
        );
        assert_eq!(
            Easing::parse("steps(3, jump-start)"),
            Some(Easing::Steps {
                count: 3,
                jump_start: true
            })
        );
    }

    #[test]
    fn sample_endpoints_hold() {
        let easings = [
            Easing::Linear,
            Easing::Ease,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::CubicBezier {
                x1: 0.3,
                y1: -2.0,
                x2: 0.7,
                y2: 3.0,
            },
            Easing::Steps {
                count: 5,
                jump_start: false,
            },
        ];
        for easing in easings {
            assert_eq!(easing.sample(0.0), 0.0, "{easing:?} at 0");
            assert_eq!(easing.sample(1.0), 1.0, "{easing:?} at 1");
            assert_eq!(easing.sample(-1.0), 0.0, "{easing:?} clamped low");
            assert_eq!(easing.sample(2.0), 1.0, "{easing:?} clamped high");
        }
        // `steps(n, start)` jumps to 1/n at t = 0 by design; the upper
        // clamp and the t = 1 endpoint still hold.
        let jump_start = Easing::Steps {
            count: 5,
            jump_start: true,
        };
        assert_eq!(jump_start.sample(0.0), 0.2);
        assert_eq!(jump_start.sample(1.0), 1.0);
        assert_eq!(jump_start.sample(2.0), 1.0);
    }

    #[test]
    fn symmetric_easing_is_symmetric() {
        // cubic-bezier(0.42, 0, 0.58, 1) (ease-in-out) maps 0.5 to 0.5 and
        // satisfies sample(t) + sample(1 - t) = 1.
        let easing = Easing::EaseInOut;
        assert!((easing.sample(0.5) - 0.5).abs() < 1e-9);
        for i in 1..10 {
            let t = i as f64 / 10.0;
            assert!((easing.sample(t) + easing.sample(1.0 - t) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn steps_sample_jumps() {
        let end = Easing::Steps {
            count: 4,
            jump_start: false,
        };
        assert_eq!(end.sample(0.0), 0.0);
        assert_eq!(end.sample(0.1), 0.0);
        assert_eq!(end.sample(0.3), 0.25);
        assert_eq!(end.sample(0.99), 0.75);
        let start = Easing::Steps {
            count: 4,
            jump_start: true,
        };
        assert_eq!(start.sample(0.0), 0.25);
        assert_eq!(start.sample(0.3), 0.5);
    }

    #[test]
    fn bezier_sample_matches_keyword_within_tolerance() {
        // ease == cubic-bezier(0.25, 0.1, 0.25, 1).
        let keyword = Easing::Ease;
        let explicit = Easing::CubicBezier {
            x1: 0.25,
            y1: 0.1,
            x2: 0.25,
            y2: 1.0,
        };
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            assert!((keyword.sample(t) - explicit.sample(t)).abs() < 1e-12);
        }
    }

    #[test]
    fn sampling_is_deterministic() {
        let easing = Easing::CubicBezier {
            x1: 0.1,
            y1: 0.9,
            x2: 0.9,
            y2: 0.2,
        };
        let first: Vec<f64> = (0..=20).map(|i| easing.sample(i as f64 / 20.0)).collect();
        let second: Vec<f64> = (0..=20).map(|i| easing.sample(i as f64 / 20.0)).collect();
        assert_eq!(first, second);
    }
}
