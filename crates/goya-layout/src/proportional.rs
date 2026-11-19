//! Opt-in proportional resize geometry (Nebula NEB-002).
//!
//! # Policy
//!
//! Emperor's EMP-M1 rule (GY-EMP-001 §4.2) is "resize without reflow: dirty
//! the frame, keep admitted DIP rectangles". [`ResizePolicy::Fixed`] encodes
//! that rule, is the [`Default`], and is the exact identity: [`place`] returns
//! the input rectangles verbatim and [`Placement::to_design`] returns its
//! input. Nothing in this module changes EMP-M1 behaviour unless a caller
//! explicitly selects [`ResizePolicy::Proportional`].
//!
//! [`ResizePolicy::Proportional`] scales content from its admitted design
//! size towards the configured surface size, optionally preserving aspect
//! ratio, and anchors any leftover space according to an [`Anchor`] and the
//! compositor's [`TiledEdges`]. The tiled state is plain data; this module
//! has no platform dependency.
//!
//! # Units
//!
//! All lengths share one integer unit, the unit of [`Rect`] (Emperor stores
//! DIP multiplied by `goya_wayland::MP_PER_PX`). Scales are integer
//! milli-factors: `1000` is 1x, `2000` is 2x, `1500` is 1.5x.
//!
//! # Rounding (integer only, no floats)
//!
//! Every division rounds toward negative infinity (floor), with 128-bit
//! intermediates so no product can overflow. Results are bit-identical on
//! every host.
//!
//! - Raw axis scale: `floor(configured * 1000 / design)`. Flooring means the
//!   scaled design box never exceeds the configured size unless a
//!   `min_scale_milli` clamp forces it to.
//! - With `preserve_aspect`, both axes use the smaller raw scale.
//! - The scale is then clamped to `[max(min_scale_milli, 1),
//!   max(max_scale_milli, that)]`; a zero minimum is treated as 1 and a
//!   maximum below the minimum is raised to it.
//! - Coordinate map: `f(v) = offset + floor(v * scale / 1000)`.
//! - Leftover space per axis is `gap = configured - floor(design * scale /
//!   1000)`; the offset is `0` (start), `floor(gap / 2)` (centre) or `gap`
//!   (end). A negative gap (content forced larger than the surface by
//!   `min_scale_milli`) overflows symmetrically or towards the far edge.
//!
//! # Guarantees for `Proportional`
//!
//! - **Edges, not sizes.** Each output edge is `f(edge)` of the design edge,
//!   so two rectangles sharing an edge in design space share it exactly after
//!   scaling: there are no 1-unit seams or overlaps.
//! - **Containment.** Every output rectangle lies inside `[0, configured]` on
//!   both axes (edges are clamped to the surface after mapping).
//! - **No invented degeneracy.** An output extent is zero or negative only if
//!   (a) the input extent was, (b) the configured extent is zero, or (c) the
//!   rectangle lies wholly outside the surface because a `min_scale_milli`
//!   clamp made the content overflow. A positive design extent that the
//!   scale would collapse below one unit is widened to one unit (this is the
//!   single case where an exact shared edge may be overlapped by one unit; it
//!   cannot occur when every design extent times the minimum scale is at
//!   least 1000).
//! - **Exact inverse.** [`Placement::to_design`] returns the largest design
//!   coordinate `d` with `f(d) <= c`. Consequently, for every rectangle that
//!   was neither clipped nor widened, a surface point `c` lies in the output
//!   rectangle (half-open) if and only if `to_design(c)` lies in the design
//!   rectangle (half-open). Hit-testing in design space is exact.
//!
//! # Tiling
//!
//! When the surface is tiled against an edge, content is flush with that edge
//! on that axis regardless of [`Anchor`], so there is never a gap between a
//! tiled edge and the content. Tiled against only the right (bottom) edge
//! means end-aligned; tiled against only the left (top) edge, or against
//! both opposite edges, means start-aligned. Untiled axes follow the anchor.

use crate::Rect;

/// Fixed-point denominator of every milli-scale.
pub const SCALE_ONE_MILLI: u32 = 1000;

/// Width and height in [`Rect`] units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Size {
    pub width: i64,
    pub height: i64,
}

impl Size {
    pub const fn new(width: i64, height: i64) -> Self {
        Self { width, height }
    }
}

/// A point in [`Rect`] units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl Point {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// Where leftover space goes on axes that are not tiled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// Alignment of content within leftover space on one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn horizontal(self) -> Align {
        match self {
            Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => Align::Start,
            Anchor::Top | Anchor::Center | Anchor::Bottom => Align::Middle,
            Anchor::TopRight | Anchor::Right | Anchor::BottomRight => Align::End,
        }
    }

    fn vertical(self) -> Align {
        match self {
            Anchor::TopLeft | Anchor::Top | Anchor::TopRight => Align::Start,
            Anchor::Left | Anchor::Center | Anchor::Right => Align::Middle,
            Anchor::BottomLeft | Anchor::Bottom | Anchor::BottomRight => Align::End,
        }
    }
}

/// Which surface edges the compositor reports as tiled (adjacent to another
/// window or the output edge). Plain data; no platform dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TiledEdges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl TiledEdges {
    /// No tiled edges (a floating window).
    pub const NONE: Self = Self {
        left: false,
        right: false,
        top: false,
        bottom: false,
    };

    /// True when any edge is tiled.
    pub const fn any(self) -> bool {
        self.left || self.right || self.top || self.bottom
    }
}

/// How content responds when the configured surface size differs from the
/// admitted design size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ResizePolicy {
    /// EMP-M1 default: keep admitted rectangles exactly; resize only dirties
    /// the frame. [`place`] is the identity.
    #[default]
    Fixed,
    /// Opt-in: scale content from the design size to the configured size.
    Proportional {
        /// Smallest permitted scale, in milli (`1000` = 1x). `0` acts as `1`.
        min_scale_milli: u32,
        /// Largest permitted scale, in milli. Values below the effective
        /// minimum are raised to it.
        max_scale_milli: u32,
        /// Use one scale for both axes (letterbox) instead of stretching.
        preserve_aspect: bool,
        /// Placement of leftover space on untiled axes.
        anchor: Anchor,
    },
}

/// The integer design-to-surface transform chosen by [`place`].
///
/// `surface = offset + floor(design * scale_milli / 1000)` per axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Transform {
    pub scale_x_milli: u32,
    pub scale_y_milli: u32,
    pub offset_x: i64,
    pub offset_y: i64,
}

impl Transform {
    /// The identity transform used by [`ResizePolicy::Fixed`].
    pub const IDENTITY: Self = Self {
        scale_x_milli: SCALE_ONE_MILLI,
        scale_y_milli: SCALE_ONE_MILLI,
        offset_x: 0,
        offset_y: 0,
    };

    /// Map a design point to the surface (unclipped).
    pub fn to_surface(&self, p: Point) -> Point {
        Point {
            x: sat(forward(p.x, self.scale_x_milli, self.offset_x)),
            y: sat(forward(p.y, self.scale_y_milli, self.offset_y)),
        }
    }

    /// Map a surface point to the largest design point that maps at or
    /// before it on each axis. Exact inverse of [`Transform::to_surface`]
    /// for hit-testing: `to_surface(d).x <= c.x` iff `d.x <= to_design(c).x`.
    pub fn to_design(&self, c: Point) -> Point {
        Point {
            x: sat(inverse(c.x, self.scale_x_milli, self.offset_x)),
            y: sat(inverse(c.y, self.scale_y_milli, self.offset_y)),
        }
    }
}

/// Output of [`place`]: transformed rectangles (same order as the input) and
/// the transform that produced them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub rects: Vec<Rect>,
    pub transform: Transform,
    /// Configured surface size the rectangles were clipped to (negative
    /// inputs are reported as zero). Not applied under `Fixed`.
    pub surface: Size,
}

impl Placement {
    /// Map a surface point (for example a pointer position) back to design
    /// space for hit-testing against the admitted rectangles.
    pub fn to_design(&self, point: Point) -> Point {
        self.transform.to_design(point)
    }
}

/// Place `rects` (in design space) on a surface of `configured_size`.
///
/// `Fixed` returns `rects` unchanged with [`Transform::IDENTITY`] and does
/// not clip (EMP-M1 keeps admitted rectangles even when the window is
/// smaller). `Proportional` follows the rules in the module documentation.
pub fn place(
    policy: ResizePolicy,
    design_size: Size,
    configured_size: Size,
    tiled: Option<TiledEdges>,
    rects: &[Rect],
) -> Placement {
    let surface = Size::new(configured_size.width.max(0), configured_size.height.max(0));
    let ResizePolicy::Proportional {
        min_scale_milli,
        max_scale_milli,
        preserve_aspect,
        anchor,
    } = policy
    else {
        return Placement {
            rects: rects.to_vec(),
            transform: Transform::IDENTITY,
            surface,
        };
    };

    let lo = min_scale_milli.max(1);
    let hi = max_scale_milli.max(lo);
    let raw_x = raw_scale(design_size.width, surface.width);
    let raw_y = raw_scale(design_size.height, surface.height);
    let (sx, sy) = if preserve_aspect {
        let s = match (raw_x, raw_y) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let s = clamp_scale(s, lo, hi);
        (s, s)
    } else {
        (clamp_scale(raw_x, lo, hi), clamp_scale(raw_y, lo, hi))
    };

    let tiled = tiled.unwrap_or(TiledEdges::NONE);
    let align_x = tiled_align(tiled.left, tiled.right).unwrap_or(anchor.horizontal());
    let align_y = tiled_align(tiled.top, tiled.bottom).unwrap_or(anchor.vertical());
    let offset_x = sat(offset(design_size.width, surface.width, sx, align_x));
    let offset_y = sat(offset(design_size.height, surface.height, sy, align_y));
    let transform = Transform {
        scale_x_milli: sx,
        scale_y_milli: sy,
        offset_x,
        offset_y,
    };

    let rects = rects
        .iter()
        .map(|r| {
            let (x0, x1) = map_span(r.x, r.width, sx, offset_x, surface.width);
            let (y0, y1) = map_span(r.y, r.height, sy, offset_y, surface.height);
            Rect {
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
            }
        })
        .collect();

    Placement {
        rects,
        transform,
        surface,
    }
}

/// `floor(configured * 1000 / design)`, or `None` when the design extent is
/// not positive (the axis carries no scale information).
fn raw_scale(design: i64, configured: i64) -> Option<i128> {
    (design > 0).then(|| (i128::from(configured) * 1000).div_euclid(i128::from(design)))
}

fn clamp_scale(raw: Option<i128>, lo: u32, hi: u32) -> u32 {
    let raw = raw.unwrap_or(i128::from(SCALE_ONE_MILLI));
    // Bounded by u32 limits, so the cast cannot truncate.
    raw.clamp(i128::from(lo), i128::from(hi)) as u32
}

/// Alignment forced by tiled edges on one axis, if any.
fn tiled_align(start: bool, end: bool) -> Option<Align> {
    match (start, end) {
        (false, false) => None,
        (false, true) => Some(Align::End),
        (true, _) => Some(Align::Start),
    }
}

fn scaled(v: i64, scale: u32) -> i128 {
    (i128::from(v) * i128::from(scale)).div_euclid(i128::from(SCALE_ONE_MILLI))
}

fn forward(v: i64, scale: u32, offset: i64) -> i128 {
    i128::from(offset) + scaled(v, scale)
}

/// Largest `d` with `forward(d) <= c`: `floor(((c - offset + 1) * 1000 - 1) / scale)`.
fn inverse(c: i64, scale: u32, offset: i64) -> i128 {
    let scale = i128::from(scale.max(1));
    ((i128::from(c) - i128::from(offset) + 1) * i128::from(SCALE_ONE_MILLI) - 1).div_euclid(scale)
}

fn offset(design: i64, configured: i64, scale: u32, align: Align) -> i128 {
    let content = scaled(design.max(0), scale);
    let gap = i128::from(configured) - content;
    match align {
        Align::Start => 0,
        Align::Middle => gap.div_euclid(2),
        Align::End => gap,
    }
}

/// Map the span `[start, start + extent)` and clip it to `[0, limit]`.
fn map_span(start: i64, extent: i64, scale: u32, offset: i64, limit: i64) -> (i64, i64) {
    let end = i128::from(start) + i128::from(extent);
    let a = forward(start, scale, offset);
    let b = i128::from(offset) + (end * i128::from(scale)).div_euclid(i128::from(SCALE_ONE_MILLI));
    let limit_w = i128::from(limit);
    if extent > 0 && a == b && limit_w > 0 && (0..=limit_w).contains(&a) {
        // Sub-unit collapse of a positive extent: widen to one unit.
        return if a < limit_w {
            (sat(a), sat(a + 1))
        } else {
            (limit - 1, limit)
        };
    }
    (sat(a.clamp(0, limit_w)), sat(b.clamp(0, limit_w)))
}

fn sat(v: i128) -> i64 {
    v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i64, y: i64, width: i64, height: i64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn prop(preserve_aspect: bool, anchor: Anchor) -> ResizePolicy {
        ResizePolicy::Proportional {
            min_scale_milli: 1,
            max_scale_milli: u32::MAX,
            preserve_aspect,
            anchor,
        }
    }

    fn contained(p: &Placement) {
        for rect in &p.rects {
            assert!(rect.x >= 0 && rect.y >= 0, "{rect:?}");
            assert!(
                rect.x + rect.width <= p.surface.width,
                "{rect:?} {:?}",
                p.surface
            );
            assert!(
                rect.y + rect.height <= p.surface.height,
                "{rect:?} {:?}",
                p.surface
            );
        }
    }

    const COLS: [Rect; 3] = [
        Rect {
            x: 0,
            y: 0,
            width: 333,
            height: 400,
        },
        Rect {
            x: 333,
            y: 0,
            width: 334,
            height: 400,
        },
        Rect {
            x: 667,
            y: 0,
            width: 333,
            height: 400,
        },
    ];
    const COLS_DESIGN: Size = Size::new(1000, 400);

    #[test]
    fn default_is_fixed() {
        assert_eq!(ResizePolicy::default(), ResizePolicy::Fixed);
        assert_eq!(Anchor::default(), Anchor::TopLeft);
        assert_eq!(TiledEdges::default(), TiledEdges::NONE);
    }

    #[test]
    fn fixed_is_identity() {
        let rects = [r(10, 20, 30, 40), r(-5, 900, 5000, 0), r(1, 1, -3, 7)];
        let tiled = TiledEdges {
            left: true,
            right: false,
            top: true,
            bottom: true,
        };
        for configured in [Size::new(0, 0), Size::new(50, 50), Size::new(4000, 3000)] {
            for t in [None, Some(tiled)] {
                let p = place(
                    ResizePolicy::Fixed,
                    Size::new(100, 100),
                    configured,
                    t,
                    &rects,
                );
                assert_eq!(p.rects, rects);
                assert_eq!(p.transform, Transform::IDENTITY);
                for pt in [Point::new(0, 0), Point::new(-7, 13), Point::new(99, 4999)] {
                    assert_eq!(p.to_design(pt), pt);
                }
            }
        }
    }

    #[test]
    fn proportional_at_design_size_is_identity() {
        for aspect in [false, true] {
            let p = place(
                prop(aspect, Anchor::Center),
                COLS_DESIGN,
                COLS_DESIGN,
                None,
                &COLS,
            );
            assert_eq!(p.rects, COLS);
            assert_eq!(p.transform, Transform::IDENTITY);
        }
    }

    #[test]
    fn exact_two_x() {
        let rects = [r(0, 0, 100, 50), r(100, 50, 100, 50), r(37, 11, 3, 1)];
        for aspect in [false, true] {
            let p = place(
                prop(aspect, Anchor::Center),
                Size::new(200, 100),
                Size::new(400, 200),
                None,
                &rects,
            );
            assert_eq!(p.transform.scale_x_milli, 2000);
            assert_eq!(p.transform.scale_y_milli, 2000);
            assert_eq!((p.transform.offset_x, p.transform.offset_y), (0, 0));
            assert_eq!(
                p.rects,
                vec![r(0, 0, 200, 100), r(200, 100, 200, 100), r(74, 22, 6, 2)]
            );
        }
    }

    #[test]
    fn non_integer_scales_floor() {
        // 300 -> 1000: floor(1000000/300) = 3333 milli; 200 -> 500: 2500.
        let rects = [r(0, 0, 100, 100), r(100, 100, 200, 100), r(7, 3, 1, 1)];
        let p = place(
            prop(false, Anchor::TopLeft),
            Size::new(300, 200),
            Size::new(1000, 500),
            None,
            &rects,
        );
        assert_eq!(p.transform.scale_x_milli, 3333);
        assert_eq!(p.transform.scale_y_milli, 2500);
        // x edges: floor(100*3.333)=333, floor(300*3.333)=999.
        // y edges: 0, 250, 500.  7*3.333=23.331 -> 23, 8*3.333=26.664 -> 26.
        // 3*2.5=7.5 -> 7, 4*2.5=10.
        assert_eq!(
            p.rects,
            vec![r(0, 0, 333, 250), r(333, 250, 666, 250), r(23, 7, 3, 3)]
        );
        contained(&p);

        // 1.5x exactly, odd coordinates floor: 3*1.5 = 4.5 -> 4, 5*1.5 = 7.5 -> 7.
        let p = place(
            prop(true, Anchor::TopLeft),
            Size::new(10, 10),
            Size::new(15, 15),
            None,
            &[r(3, 3, 2, 2)],
        );
        assert_eq!(p.transform.scale_x_milli, 1500);
        assert_eq!(p.rects, vec![r(4, 4, 3, 3)]);
    }

    #[test]
    fn preserve_aspect_letterboxes_by_anchor() {
        // 400x300 into 800x900: s = min(2000, 3000) = 2000, content 800x600,
        // vertical gap 300.
        let design = Size::new(400, 300);
        let conf = Size::new(800, 900);
        let rects = [r(0, 0, 400, 300)];
        let cases = [
            (Anchor::TopLeft, 0),
            (Anchor::Center, 150),
            (Anchor::Left, 150),
            (Anchor::Bottom, 300),
            (Anchor::BottomRight, 300),
        ];
        for (anchor, oy) in cases {
            let p = place(prop(true, anchor), design, conf, None, &rects);
            assert_eq!(p.transform.scale_x_milli, 2000);
            assert_eq!(p.transform.scale_y_milli, 2000);
            assert_eq!(p.transform.offset_x, 0, "{anchor:?}");
            assert_eq!(p.transform.offset_y, oy, "{anchor:?}");
            assert_eq!(p.rects, vec![r(0, oy, 800, 600)]);
            contained(&p);
        }
        // Pillarbox: 400x300 into 1000x600: s = min(2500, 2000) = 2000,
        // content 800x600, horizontal gap 200.
        let p = place(
            prop(true, Anchor::Center),
            design,
            Size::new(1000, 600),
            None,
            &rects,
        );
        assert_eq!((p.transform.offset_x, p.transform.offset_y), (100, 0));
        let p = place(
            prop(true, Anchor::Right),
            design,
            Size::new(1000, 600),
            None,
            &rects,
        );
        assert_eq!((p.transform.offset_x, p.transform.offset_y), (200, 0));
        // Odd gaps centre with floor: 400x300 into 801x900 gives
        // floor(801000/400) = 2002 milli; content floor(800.8) = 800 by
        // floor(600.6) = 600; gaps 1 and 300 centre to 0 and 150.
        let p = place(
            prop(true, Anchor::Center),
            design,
            Size::new(801, 900),
            None,
            &rects,
        );
        assert_eq!(p.transform.scale_x_milli, 2002);
        assert_eq!((p.transform.offset_x, p.transform.offset_y), (0, 150));
        assert_eq!(p.rects, vec![r(0, 150, 800, 600)]);
        contained(&p);
    }

    #[test]
    fn stretch_without_aspect_fills_both_axes() {
        let p = place(
            prop(false, Anchor::Center),
            Size::new(400, 300),
            Size::new(800, 900),
            None,
            &[r(0, 0, 400, 300)],
        );
        assert_eq!(p.rects, vec![r(0, 0, 800, 900)]);
    }

    #[test]
    fn max_clamp_leaves_anchored_gap() {
        let policy = ResizePolicy::Proportional {
            min_scale_milli: 500,
            max_scale_milli: 2000,
            preserve_aspect: true,
            anchor: Anchor::Center,
        };
        let p = place(
            policy,
            Size::new(100, 100),
            Size::new(1000, 500),
            None,
            &[r(0, 0, 100, 100)],
        );
        assert_eq!(p.transform.scale_x_milli, 2000);
        assert_eq!((p.transform.offset_x, p.transform.offset_y), (400, 150));
        assert_eq!(p.rects, vec![r(400, 150, 200, 200)]);
        contained(&p);
    }

    #[test]
    fn min_clamp_overflows_and_clips() {
        let policy = ResizePolicy::Proportional {
            min_scale_milli: 500,
            max_scale_milli: 2000,
            preserve_aspect: false,
            anchor: Anchor::Center,
        };
        // 1000 -> 100 would be 100 milli; clamped to 500 => content 500 wide,
        // gap -400, centred offset -200; visible design window is [400, 600).
        let rects = [
            r(0, 0, 400, 1000),
            r(400, 0, 200, 1000),
            r(600, 0, 400, 1000),
            r(450, 450, 100, 100),
        ];
        let p = place(
            policy,
            Size::new(1000, 1000),
            Size::new(100, 100),
            None,
            &rects,
        );
        assert_eq!(p.transform.scale_x_milli, 500);
        assert_eq!(p.transform.offset_x, -200);
        contained(&p);
        // Wholly outside rects clip to zero width (documented exception).
        assert_eq!(p.rects[0], r(0, 0, 0, 100));
        assert_eq!(p.rects[1], r(0, 0, 100, 100));
        assert_eq!(p.rects[2], r(100, 0, 0, 100));
        assert_eq!(p.rects[3], r(25, 25, 50, 50));
        assert_eq!(p.to_design(Point::new(50, 50)), Point::new(501, 501));
    }

    #[test]
    fn scale_bounds_are_normalised() {
        // min 0 acts as 1; max below min is raised to min.
        let p = place(
            ResizePolicy::Proportional {
                min_scale_milli: 0,
                max_scale_milli: 0,
                preserve_aspect: true,
                anchor: Anchor::TopLeft,
            },
            Size::new(1000, 1000),
            Size::new(0, 0),
            None,
            &[r(0, 0, 1000, 1000)],
        );
        assert_eq!(p.transform.scale_x_milli, 1);
        let p = place(
            ResizePolicy::Proportional {
                min_scale_milli: 3000,
                max_scale_milli: 1000,
                preserve_aspect: true,
                anchor: Anchor::TopLeft,
            },
            Size::new(10, 10),
            Size::new(100, 100),
            None,
            &[],
        );
        assert_eq!(p.transform.scale_x_milli, 3000);
    }

    #[test]
    fn tiled_edges_anchor_flush() {
        // 400x300 into 1000x900 (aspect): s = min(2500, 3000) = 2500,
        // content 1000x750, vertical gap 150; anchor Center would give 75.
        let design = Size::new(400, 300);
        let conf = Size::new(1000, 900);
        let rects = [r(0, 0, 400, 300)];
        let t = |left, right, top, bottom| TiledEdges {
            left,
            right,
            top,
            bottom,
        };
        let cases = [
            (None, 75),
            (Some(TiledEdges::NONE), 75),
            (Some(t(true, false, false, false)), 75),
            (Some(t(false, false, true, false)), 0),
            (Some(t(false, false, false, true)), 150),
            (Some(t(false, false, true, true)), 0),
            (Some(t(true, true, true, false)), 0),
        ];
        for (tiled, oy) in cases {
            let p = place(prop(true, Anchor::Center), design, conf, tiled, &rects);
            assert_eq!(p.transform.offset_y, oy, "{tiled:?}");
            contained(&p);
        }
        // Horizontal: 400x300 into 1000x600: s = 2000, content 800 wide, gap 200.
        let conf = Size::new(1000, 600);
        let cases = [
            (None, 100),
            (Some(t(true, false, false, false)), 0),
            (Some(t(false, true, false, false)), 200),
            (Some(t(true, true, false, false)), 0),
        ];
        for (tiled, ox) in cases {
            for (anchor, untiled_ox) in [
                (Anchor::Center, 100),
                (Anchor::Right, 200),
                (Anchor::TopLeft, 0),
            ] {
                let p = place(prop(true, anchor), design, conf, tiled, &rects);
                let expect = if tiled.is_some() { ox } else { untiled_ox };
                assert_eq!(p.transform.offset_x, expect, "{tiled:?} {anchor:?}");
                if tiled.is_some_and(|t| t.left) {
                    assert_eq!(p.rects[0].x, 0, "flush with tiled left edge");
                }
                if tiled.is_some_and(|t| t.right && !t.left) {
                    assert_eq!(
                        p.rects[0].x + p.rects[0].width,
                        conf.width,
                        "flush with tiled right edge"
                    );
                }
            }
        }
        assert!(t(false, false, false, true).any());
        assert!(!TiledEdges::NONE.any());
    }

    fn check_columns(p: &Placement) {
        contained(p);
        let [a, b, c] = [p.rects[0], p.rects[1], p.rects[2]];
        assert_eq!(a.x + a.width, b.x, "seam a|b {p:?}");
        assert_eq!(b.x + b.width, c.x, "seam b|c {p:?}");
        assert_eq!(a.x, p.transform.offset_x);
        for rect in [a, b, c] {
            assert!(rect.width > 0 && rect.height > 0, "{p:?}");
            assert_eq!(rect.y, a.y);
            assert_eq!(rect.height, a.height);
        }
    }

    #[test]
    fn shared_edges_survive_width_sweep() {
        for width in 100..=2000 {
            for (aspect, height) in [(false, 400), (false, 37), (true, 400), (true, 2000)] {
                for anchor in [Anchor::TopLeft, Anchor::Center, Anchor::BottomRight] {
                    let p = place(
                        prop(aspect, anchor),
                        COLS_DESIGN,
                        Size::new(width, height),
                        None,
                        &COLS,
                    );
                    check_columns(&p);
                    if !aspect {
                        // Stretch fills to within one unit-per-1000 of the surface.
                        let right = p.rects[2].x + p.rects[2].width;
                        assert!(width - right <= width / 1000 + 1, "{width}: {right}");
                    }
                }
            }
        }
    }

    #[test]
    fn hit_test_inverse_round_trips() {
        let mut checked = 0;
        for width in 100..=2000 {
            for aspect in [false, true] {
                let conf = Size::new(width, 300 + width % 97);
                let p = place(prop(aspect, Anchor::Center), COLS_DESIGN, conf, None, &COLS);
                for (design, out) in COLS.iter().zip(&p.rects) {
                    // Centre of the output rect maps back inside the design rect.
                    let c = Point::new(out.x + out.width / 2, out.y + out.height / 2);
                    let d = p.to_design(c);
                    assert!(
                        d.x >= design.x && d.x < design.x + design.width,
                        "{width} {c:?} -> {d:?} not in {design:?}"
                    );
                    assert!(
                        d.y >= design.y && d.y < design.y + design.height,
                        "{width} {c:?} -> {d:?} not in {design:?}"
                    );
                    // Centre of the design rect maps forward inside the output rect.
                    let dc = Point::new(design.x + design.width / 2, design.y + design.height / 2);
                    let s = p.transform.to_surface(dc);
                    assert!(
                        s.x >= out.x && s.x < out.x + out.width,
                        "{width} {dc:?} -> {s:?} not in {out:?}"
                    );
                    assert!(
                        s.y >= out.y && s.y < out.y + out.height,
                        "{width} {dc:?} -> {s:?} not in {out:?}"
                    );
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 1901 * 2 * 3);
    }

    #[test]
    fn hit_test_is_exact_on_every_surface_pixel() {
        for width in [101, 250, 333, 999, 1001, 1777] {
            for aspect in [false, true] {
                let p = place(
                    prop(aspect, Anchor::Center),
                    COLS_DESIGN,
                    Size::new(width, 211),
                    None,
                    &COLS,
                );
                let y = p.rects[0].y;
                for x in 0..width {
                    let d = p.to_design(Point::new(x, y));
                    for (design, out) in COLS.iter().zip(&p.rects) {
                        let in_out = x >= out.x && x < out.x + out.width;
                        let in_design = d.x >= design.x && d.x < design.x + design.width;
                        assert_eq!(in_out, in_design, "width {width} x {x} -> {d:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn zero_and_negative_sizes_do_not_panic() {
        let rects = [r(0, 0, 10, 10), r(5, 5, 0, 0), r(-3, -3, 1, 1)];
        let sizes = [
            Size::new(0, 0),
            Size::new(0, 100),
            Size::new(-5, -5),
            Size::new(100, 0),
        ];
        for conf in sizes {
            for design in sizes.iter().copied().chain([Size::new(10, 10)]) {
                for aspect in [false, true] {
                    let p = place(prop(aspect, Anchor::Center), design, conf, None, &rects);
                    contained(&p);
                    for rect in &p.rects {
                        assert!(rect.width >= 0 && rect.height >= 0);
                    }
                    let _ = p.to_design(Point::new(0, 0));
                    let _ = p.to_design(Point::new(i64::MAX, i64::MIN));
                }
            }
        }
        let p = place(
            prop(true, Anchor::Center),
            Size::new(10, 10),
            Size::new(0, 0),
            None,
            &rects,
        );
        assert!(p.rects.iter().all(|r| *r
            == Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0
            }));
    }

    #[test]
    fn extreme_values_saturate_without_panic() {
        let policy = ResizePolicy::Proportional {
            min_scale_milli: u32::MAX,
            max_scale_milli: u32::MAX,
            preserve_aspect: false,
            anchor: Anchor::BottomRight,
        };
        let rects = [
            r(i64::MIN, i64::MIN, i64::MAX, i64::MAX),
            r(i64::MAX, 0, i64::MAX, 1),
        ];
        let p = place(
            policy,
            Size::new(i64::MAX, i64::MAX),
            Size::new(i64::MAX, 1),
            None,
            &rects,
        );
        contained(&p);
    }

    #[test]
    fn sub_unit_extents_widen_to_one() {
        // 10 -> 3: floor(3000/10) = 300 milli. A 1-wide rect at 0 maps to
        // [0, 0.3) and is widened to one unit; zero-width input stays zero.
        let rects = [r(0, 0, 1, 1), r(9, 9, 1, 1), r(4, 4, 0, 0), r(0, 0, 10, 10)];
        let p = place(
            prop(true, Anchor::TopLeft),
            Size::new(10, 10),
            Size::new(3, 3),
            None,
            &rects,
        );
        assert_eq!(
            p.rects,
            vec![r(0, 0, 1, 1), r(2, 2, 1, 1), r(1, 1, 0, 0), r(0, 0, 3, 3)]
        );
        contained(&p);
    }

    #[test]
    fn negative_design_coordinates_floor() {
        // -3 * 1.5 = -4.5 floors to -5, then clips to 0; the inverse floors too.
        let p = place(
            prop(true, Anchor::TopLeft),
            Size::new(10, 10),
            Size::new(15, 15),
            None,
            &[r(-3, -3, 6, 6)],
        );
        assert_eq!(p.rects, vec![r(0, 0, 4, 4)]);
        assert_eq!(
            p.transform.to_surface(Point::new(-3, -1)),
            Point::new(-5, -2)
        );
        assert_eq!(p.to_design(Point::new(-5, -2)), Point::new(-3, -1));
    }

    #[test]
    fn deterministic() {
        let a = place(
            prop(true, Anchor::Center),
            COLS_DESIGN,
            Size::new(1234, 567),
            None,
            &COLS,
        );
        let b = place(
            prop(true, Anchor::Center),
            COLS_DESIGN,
            Size::new(1234, 567),
            None,
            &COLS,
        );
        assert_eq!(a, b);
    }
}
