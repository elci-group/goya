//! Ambient motion on webOS: breathing fills and drifting elements, for
//! burn-in avoidance. Contract `arachnid.motion/0.0.1`.
//!
//! GSSH/3 live admission adds two constructs:
//! * `fill-to` / `fill-period-s` makes a fill breathe between two colours;
//! * `drift-p` / `drift-period-s` makes an element and its subtree orbit
//!   an ellipse;
//! * `appear-s` / `appear-from-p` makes a subtree fade in, and optionally
//!   slide in, whenever it becomes present.
//!
//! **Parent-relative throughout.** Drift half-axes and appear offsets are
//! displacements, so like `position_p` they are fractions of the
//! **parent's** resolved extent. The ontology limit stands in for the
//! root's parent. The emitted pixels come from that compile-time layout.
//!
//! The parser already bounds periods (1 s to 1 h) and fractions. This
//! module checks what only a laid-out frame can show:
//!
//! * **Bounds (`GY-AR-621`).** A node that draws something (fill, stroke,
//!   text, image or button) must stay inside the 1920×1080 ontology limit
//!   at every point of its motion. That motion is the sum of its own drift
//!   and appear offset and its ancestors' drifts and appear offsets. An
//!   undrawn container may cross the edge, because it has no pixels there.
//!   Appear motion is transient and ends at the rest geometry, so it is
//!   not part of the focus or contrast rules below.
//! * **One focus geometry (`GY-AR-633`).** The focus graph is computed once
//!   at compile time. A drift may carry buttons only when it carries every
//!   button, so that it is a common ancestor of all of them. The whole
//!   interface may orbit (pixel shift), but buttons never move relative to
//!   one another.
//! * **Contrast (`GY-AR-622`).** Text is black. When a node's text could
//!   sit over a breathing fill, or over a fill that moves relative to it,
//!   the check samples every colour that background takes. It requires
//!   WCAG 2.x AA contrast against black throughout: 4.5:1, or 3:1 for text
//!   of at least 24 CSS px (WCAG "large text", 18 pt). Text that an image
//!   could move under cannot be verified, so it is refused. Only nodes
//!   present together in some presence configuration are considered.
//!   Text over unchanging backgrounds is not checked; Arachnid has no
//!   general contrast contract yet. A live sky (`sky: local`) can take any
//!   colour, so text over it must be covered by an opaque fill drawn above
//!   the sky that is present with the text, moves with it and contains it.
//!
//! **Emission** uses only compositor-friendly properties. A drift is a
//! `transform` animation on a wrapper around the drifting subtree, which
//! is contiguous in draw order. A breathing fill is an `opacity`
//! cross-fade of a second layer inside the node. That layer is a linear
//! gradient from `fill-to` to `fill`, so every colour shown lies on the
//! `fill`–`fill-to` segment that the contrast check samples. Under
//! `prefers-reduced-motion: reduce`, breathing fills and decorative drifts
//! (drifts carrying no buttons) stop. A drift carrying the interface is
//! burn-in protection, so it continues.

use crate::diagnostics::{Code, Diagnostic};
use goya_lang::{Frame, RectMp, SceneCell};

pub const CONTRACT: &str = "arachnid.motion/0.0.1";

/// WCAG 2.x large-text threshold in CSS px (18 pt).
pub const LARGE_TEXT_PX: i64 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drift {
    /// Half-axes in millipoints.
    pub ax: i64,
    pub ay: i64,
    pub period_ms: u32,
    /// Carries no button: stops under reduced motion.
    pub decorative: bool,
}

/// A fade-in, optionally sliding from (dx, dy) millipoints to rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appear {
    pub dx: i64,
    pub dy: i64,
    pub ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flow {
    pub from: u32,
    pub to: u32,
    pub period_ms: u32,
}

/// Motion per frame node, in preorder (the frame's node order).
#[derive(Clone, Debug, Default)]
pub struct Motion {
    pub drift: Vec<Option<Drift>>,
    pub flow: Vec<Option<Flow>>,
    pub appear: Vec<Option<Appear>>,
    /// One past the last descendant of each node.
    pub subtree_end: Vec<usize>,
}

fn ms(seconds: f64) -> u32 {
    (seconds * 1000.0).round() as u32
}

/// sRGB relative luminance (WCAG 2.x).
pub fn luminance(rgb: u32) -> f64 {
    let lin = |c: u32| {
        let c = c as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin((rgb >> 16) & 255) + 0.7152 * lin((rgb >> 8) & 255) + 0.0722 * lin(rgb & 255)
}

/// Contrast of black text on `rgb`.
pub fn contrast_with_black(rgb: u32) -> f64 {
    (luminance(rgb) + 0.05) / 0.05
}

/// Colour at `t` of the gamma-space blend from `a` to `b`, as the
/// compositor blends an opacity cross-fade.
pub fn mix(a: u32, b: u32, t: f64) -> u32 {
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 255) as f64, ((b >> s) & 255) as f64);
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

fn inside(r: &RectMp, limit: &RectMp) -> bool {
    r.x >= limit.x
        && r.y >= limit.y
        && r.x as i128 + r.width as i128 <= limit.x as i128 + limit.width as i128
        && r.y as i128 + r.height as i128 <= limit.y as i128 + limit.height as i128
}

fn expand(r: &RectMp, ax: i64, ay: i64) -> RectMp {
    RectMp {
        x: r.x - ax,
        y: r.y - ay,
        width: r.width + 2 * ax,
        height: r.height + 2 * ay,
    }
}

fn contains(outer: &RectMp, inner: &RectMp) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x as i128 + outer.width as i128 >= inner.x as i128 + inner.width as i128
        && outer.y as i128 + outer.height as i128 >= inner.y as i128 + inner.height as i128
}

fn intersects(a: &RectMp, b: &RectMp) -> bool {
    (a.x as i128) < b.x as i128 + b.width as i128
        && (b.x as i128) < a.x as i128 + a.width as i128
        && (a.y as i128) < b.y as i128 + b.height as i128
        && (b.y as i128) < a.y as i128 + a.height as i128
}

/// Build motion data and check it against the laid-out frame.
pub fn analyse(
    cells: &[&SceneCell],
    parents: &[Option<usize>],
    frame: &Frame,
    limit: &RectMp,
    actionable_nodes: &[usize],
    co_present: &dyn Fn(usize, usize) -> bool,
) -> (Motion, Vec<Diagnostic>) {
    let n = cells.len();
    let mut subtree_end: Vec<usize> = (1..=n).collect();
    for i in (0..n).rev() {
        if let Some(p) = parents[i] {
            subtree_end[p] = subtree_end[p].max(subtree_end[i]);
        }
    }
    let carries = |i: usize, node: usize| node >= i && node < subtree_end[i];
    let mut diags = Vec::new();
    let mut drift = vec![None; n];
    let mut flow = vec![None; n];
    let mut appear = vec![None; n];
    for i in 0..n {
        let c = cells[i];
        // Displacements are fractions of the parent's extent.
        let r = parents[i].map_or(*limit, |p| frame.nodes[p].rect);
        if let Some(s) = c.appear_s {
            let v = c.appear_from_p.unwrap_or_default();
            appear[i] = Some(Appear {
                dx: (r.width as f64 * v.x).round() as i64,
                dy: (r.height as f64 * v.y).round() as i64,
                ms: ms(s),
            });
        }
        if let (Some(v), Some(p)) = (c.drift_p, c.drift_period_s) {
            let carried = actionable_nodes.iter().filter(|&&a| carries(i, a)).count();
            if carried > 0 && carried < actionable_nodes.len() {
                diags.push(Diagnostic::new(
                    Code::IndependentMotion,
                    format!(
                        "{} drifts {carried} of {} buttons; a drift must carry all buttons or none, or the compiled focus graph would be wrong",
                        frame.nodes[i].name,
                        actionable_nodes.len()
                    ),
                ));
            }
            drift[i] = Some(Drift {
                ax: (r.width as f64 * v.x).round() as i64,
                ay: (r.height as f64 * v.y).round() as i64,
                period_ms: ms(p),
                decorative: carried == 0,
            });
        }
        if let (Some(fill), Some(to), Some(p)) = (c.fill, c.fill_to, c.fill_period_s) {
            flow[i] = Some(Flow {
                from: fill & 0xFF_FFFF,
                to: to & 0xFF_FFFF,
                period_ms: ms(p),
            });
        }
    }
    // Total sway of each node: its own drift plus every ancestor's, and the
    // same with appear offsets added, for bounds only.
    let mut sway = vec![(0i64, 0i64); n];
    // Reach per side: (left, right, up, down). A drift sways both ways; an
    // appear slide covers only the stretch from its offset to rest.
    let mut reach = vec![(0i64, 0i64, 0i64, 0i64); n];
    let mut movers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        let (mut ax, mut ay, mut m) = parents[i].map_or((0, 0, Vec::new()), |p| (sway[p].0, sway[p].1, movers[p].clone()));
        if let Some(d) = drift[i] {
            ax += d.ax;
            ay += d.ay;
            m.push(i);
        }
        sway[i] = (ax, ay);
        movers[i] = m;
        let (mut l, mut r, mut u, mut d) = parents[i].map_or((0, 0, 0, 0), |p| reach[p]);
        if let Some(dr) = drift[i] {
            l += dr.ax;
            r += dr.ax;
            u += dr.ay;
            d += dr.ay;
        }
        if let Some(a) = appear[i] {
            l += (-a.dx).max(0);
            r += a.dx.max(0);
            u += (-a.dy).max(0);
            d += a.dy.max(0);
        }
        reach[i] = (l, r, u, d);
    }
    let drawn = |i: usize| {
        let c = cells[i];
        c.fill.is_some()
            || c.stroke.is_some()
            || c.text.is_some()
            || c.text_from.is_some()
            || c.image_from.is_some()
            || actionable_nodes.contains(&i)
    };
    for (i, &(l, r, u, d)) in reach.iter().enumerate() {
        if (l, r, u, d) != (0, 0, 0, 0) && drawn(i) {
            let at = frame.nodes[i].rect;
            let swept = RectMp {
                x: at.x - l,
                y: at.y - u,
                width: at.width + l + r,
                height: at.height + u + d,
            };
            let (ax, ay) = (l.max(r), u.max(d));
            if !inside(&swept, limit) {
                diags.push(Diagnostic::new(
                    Code::MotionOutsideLimit,
                    format!(
                        "{} leaves the 1920×1080 ontology limit while it moves; give it a margin of at least {}×{} pt",
                        frame.nodes[i].name,
                        (ax + 999) / 1000,
                        (ay + 999) / 1000
                    ),
                ));
            }
        }
    }
    // Contrast for text whose background breathes or moves under it.
    for i in 0..n {
        let c = cells[i];
        if c.text.is_none() && c.text_from.is_none() {
            continue;
        }
        let text_rect = expand(&frame.nodes[i].rect, sway[i].0, sway[i].1);
        // A live sky takes any colour, so text may never sit on it: an
        // opaque fill drawn above the sky, present with the text and moving
        // with it, must cover the text entirely.
        for s in (0..i).filter(|&s| cells[s].sky.is_some()) {
            if !co_present(i, s) || !intersects(&expand(&frame.nodes[s].rect, sway[s].0, sway[s].1), &text_rect) {
                continue;
            }
            let covered = (s + 1..=i).any(|k| {
                cells[k].fill.is_some()
                    && cells[k].sky.is_none()
                    && (k == i || co_present(i, k))
                    && movers[k] == movers[i]
                    && contains(&frame.nodes[k].rect, &frame.nodes[i].rect)
            });
            if !covered {
                diags.push(Diagnostic::new(
                    Code::TextContrast,
                    format!(
                        "text of {} would sit on the live sky {}; give it, or a cell around it, a fill",
                        frame.nodes[i].name, frame.nodes[s].name
                    ),
                ));
            }
        }
        let mut candidates = Vec::new();
        let mut dynamic = false;
        for j in 0..=i {
            let cj = cells[j];
            if (cj.fill.is_none() && cj.image_from.is_none()) || cj.sky.is_some() {
                continue;
            }
            // Nodes never present together never overlap on screen.
            if j != i && !co_present(i, j) {
                continue;
            }
            let relative = movers[j] != movers[i];
            // A breathing fill never changes an image's pixels; only an
            // image moving under the text changes what the text sits on.
            if cj.image_from.is_some() && !relative {
                continue;
            }
            let rel_x = if relative { sway[j].0 + sway[i].0 } else { 0 };
            let rel_y = if relative { sway[j].1 + sway[i].1 } else { 0 };
            let under = expand(&frame.nodes[j].rect, rel_x, rel_y);
            if !intersects(&under, &frame.nodes[i].rect) && !intersects(&under, &text_rect) {
                continue;
            }
            dynamic |= relative || flow[j].is_some();
            candidates.push(j);
        }
        if !dynamic {
            continue;
        }
        let px = frame.nodes[i].text_size_mp.map_or(16, |mp| mp / 1000);
        let need = if px >= LARGE_TEXT_PX { 3.0 } else { 4.5 };
        for &j in &candidates {
            if cells[j].image_from.is_some() {
                diags.push(Diagnostic::new(
                    Code::TextContrast,
                    format!(
                        "text of {} can pass over the image {} while things move; its contrast cannot be verified",
                        frame.nodes[i].name, frame.nodes[j].name
                    ),
                ));
                continue;
            }
            let colours: Vec<u32> = match (cells[j].fill, flow[j]) {
                (_, Some(f)) => (0..=32).map(|k| mix(f.from, f.to, k as f64 / 32.0)).collect(),
                (Some(fill), None) => vec![fill & 0xFF_FFFF],
                (None, None) => Vec::new(),
            };
            if let Some(worst) = colours
                .iter()
                .copied()
                .min_by(|a, b| contrast_with_black(*a).total_cmp(&contrast_with_black(*b)))
            {
                let ratio = contrast_with_black(worst);
                if ratio < need {
                    diags.push(Diagnostic::new(
                        Code::TextContrast,
                        format!(
                            "black text of {} reaches {ratio:.2}:1 over {} (#{worst:06x}); WCAG AA needs {need}:1 for {px} px text",
                            frame.nodes[i].name, frame.nodes[j].name
                        ),
                    ));
                }
            }
        }
    }
    (
        Motion {
            drift,
            flow,
            appear,
            subtree_end,
        },
        diags,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast_with_black(0xFFFFFF) - 21.0).abs() < 1e-9);
        assert!((contrast_with_black(0x000000) - 1.0).abs() < 1e-9);
        // #767676 is the familiar 4.54:1 grey against white; against black it is 4.62.
        assert!((contrast_with_black(0x767676) - 4.62).abs() < 0.01);
    }

    #[test]
    fn mix_walks_the_segment() {
        assert_eq!(mix(0x000000, 0xFF0080, 0.0), 0x000000);
        assert_eq!(mix(0x000000, 0xFF0080, 1.0), 0xFF0080);
        assert_eq!(mix(0x000000, 0xFF0080, 0.5), 0x800040);
    }
}
