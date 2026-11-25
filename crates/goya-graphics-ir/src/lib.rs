//! goya-graphics-ir — GIR, Goya's internal graphical intermediate
//! representation (`GY-GIR/0`, draft), and the CPU reference renderer
//! ([`reference`]) that serves as the conformance oracle for every graphics
//! format.
//!
//! GIR is a compiler contract, not a public file format (GOYA-GFX-001 §4).
//! It follows the ratified Baseline profile (`docs/graphics/container.md`
//! §7):
//!
//! - coordinates are signed integer **millipoints** (1 pt = 1000 mp), the
//!   GY-LAYOUT/1 unit, in the item's own local space;
//! - colour is 8-bit sRGB with straight alpha;
//! - transforms are row-vector affine (SVG `matrix(a,b,c,d,e,f)`): the
//!   linear part is Q16.16 fixed point and the translation is millipoints.
//!
//! Every drawable is split into the four parts §4.1 requires: **geometry**
//! (an immutable, shareable [`Path`]), **instance** ([`Affine`]),
//! **modulation** (opacity) and **composition** (item order). Geometry is
//! held in an [`Arc`] so identical shapes are stored once.

use std::fmt;
use std::sync::Arc;

pub mod reference;

/// Contract identifier implemented by this crate.
pub const GIR_CONTRACT: &str = "GY-GIR/0";
/// Millipoints per point.
pub const MP_PER_PT: i32 = 1000;
/// `1.0` in the Q16.16 fixed point used by [`Affine`]'s linear part.
pub const FIXED_ONE: i32 = 1 << 16;

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Stable `GFX-E` codes owned by the graphics crates above the container.
/// The container's own codes (E1xxx, E4001–E4003) live in
/// `goya-graphics-container`.
pub mod codes {
    pub const MALFORMED_GEOMETRY: u16 = 2001;
    pub const NON_MINIMAL_VARINT: u16 = 2002;
    pub const COORDINATE_OVERFLOW: u16 = 2003;
    pub const PRECISION_EXCEEDED: u16 = 2004;
    pub const INVALID_STYLE: u16 = 2005;
    pub const MALFORMED_SECTION: u16 = 2006;
    pub const NON_CANONICAL: u16 = 2007;
    pub const NON_FINITE: u16 = 2008;
    pub const UNSUPPORTED_UNIT: u16 = 2009;
    /// GRU slot values.
    pub const SLOT_OUT_OF_DOMAIN: u16 = 2010;
    pub const SLOT_TYPE_MISMATCH: u16 = 2011;
    pub const GEOMETRY_REF: u16 = 3001;
    pub const STYLE_REF: u16 = 3002;
    pub const COUNT_MISMATCH: u16 = 3003;
    /// GRU typing and dependencies.
    pub const BINDING_TYPE: u16 = 3004;
    pub const BINDING_CONFLICT: u16 = 3005;
    pub const INVALIDATION_MISMATCH: u16 = 3006;
    pub const REGION_INVALID: u16 = 3007;
    pub const UNKNOWN_SLOT: u16 = 3008;
    pub const TOO_MANY_INSTANCES: u16 = 4101;
    pub const TOO_MANY_GEOMETRIES: u16 = 4102;
    pub const TOO_MANY_STYLES: u16 = 4103;
    pub const TOO_MANY_SEGMENTS: u16 = 4104;
    pub const TOO_MANY_SLOTS: u16 = 4105;
    pub const TOO_MANY_BINDINGS: u16 = 4106;
    pub const TABLE_TOO_LONG: u16 = 4107;
    pub const TOO_MANY_REGIONS: u16 = 4108;
    pub const CANVAS_TOO_LARGE: u16 = 4201;
    pub const TOO_MANY_EDGES: u16 = 4202;
    /// GVI eligibility (KFBM) and completeness.
    pub const GVI_NOT_KNOWN: u16 = 5001;
    pub const GVI_NOT_FINITE: u16 = 5002;
    pub const GVI_NOT_BOUNDED: u16 = 5003;
    pub const GVI_NOT_MANAGEABLE: u16 = 5004;
    pub const GVI_INVALID_STATE: u16 = 5005;
    pub const GVI_UNREACHABLE_STATE: u16 = 5006;
    pub const GVI_PARENT_MISMATCH: u16 = 5007;
    pub const GVI_SOURCE_MISMATCH: u16 = 5008;
    pub const GVI_CERTIFICATE_INVALID: u16 = 5009;
    pub const GVI_INVALID_MODEL: u16 = 5010;
}

/// A graphics diagnostic: a stable `GFX-E` code plus detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GfxDiag {
    pub code: u16,
    pub detail: String,
}

impl GfxDiag {
    pub fn new(code: u16, detail: impl Into<String>) -> GfxDiag {
        GfxDiag {
            code,
            detail: detail.into(),
        }
    }

    /// The stable diagnostic code, e.g. `GFX-E2001`.
    pub fn code_str(&self) -> String {
        format!("GFX-E{}", self.code)
    }
}

impl fmt::Display for GfxDiag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GFX-E{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for GfxDiag {}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// A point in millipoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ORIGIN: Point = Point { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    /// `self + (dx, dy)`, or `None` on overflow.
    pub fn checked_offset(self, dx: i32, dy: i32) -> Option<Point> {
        Some(Point::new(self.x.checked_add(dx)?, self.y.checked_add(dy)?))
    }
}

/// One path command. Curves are quadratic and cubic Béziers whose control
/// points come before the end point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Segment {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

/// Axis-aligned bounds in millipoints, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl Rect {
    pub fn intersects(&self, other: &Rect) -> bool {
        self.min_x <= other.max_x
            && other.min_x <= self.max_x
            && self.min_y <= other.max_y
            && other.min_y <= self.max_y
    }
}

/// An immutable path in canonical form (see [`Path::new`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path {
    segments: Vec<Segment>,
}

fn malformed(detail: &str) -> GfxDiag {
    GfxDiag::new(codes::MALFORMED_GEOMETRY, detail)
}

impl Path {
    /// Validate and canonicalise a segment list. Canonical form means:
    ///
    /// 1. the path begins with `MoveTo`;
    /// 2. no `MoveTo` is immediately followed by another `MoveTo` (the
    ///    earlier one is dropped) and the path never ends with `MoveTo`;
    /// 3. a drawing command after `Close` is preceded by an explicit
    ///    `MoveTo` to the closed subpath's start (SVG semantics, made
    ///    explicit);
    /// 4. `Close` never directly follows `Close`.
    ///
    /// A path that draws nothing (empty after these rules) is rejected.
    pub fn new(input: Vec<Segment>) -> Result<Path, GfxDiag> {
        let mut out: Vec<Segment> = Vec::with_capacity(input.len());
        let mut start: Option<Point> = None;
        let mut after_close = false;
        for seg in input {
            match seg {
                Segment::MoveTo(p) => {
                    if let Some(Segment::MoveTo(_)) = out.last() {
                        out.pop();
                    }
                    out.push(seg);
                    start = Some(p);
                    after_close = false;
                }
                Segment::Close => match out.last() {
                    None => return Err(malformed("a path must begin with MoveTo")),
                    Some(Segment::Close) => {}
                    Some(_) => {
                        out.push(seg);
                        after_close = true;
                    }
                },
                _ => {
                    let Some(s) = start else {
                        return Err(malformed("a path must begin with MoveTo"));
                    };
                    if after_close {
                        out.push(Segment::MoveTo(s));
                        after_close = false;
                    }
                    out.push(seg);
                }
            }
        }
        if let Some(Segment::MoveTo(_)) = out.last() {
            out.pop();
        }
        if out.is_empty() {
            return Err(malformed("the path draws nothing"));
        }
        Ok(Path { segments: out })
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// The first `MoveTo` point. Canonical paths always start with one.
    pub fn first_point(&self) -> Point {
        match self.segments[0] {
            Segment::MoveTo(p) => p,
            _ => unreachable!("canonical paths begin with MoveTo"),
        }
    }

    /// Every stored point (end points and control points) in order.
    pub fn points(&self) -> impl Iterator<Item = Point> + '_ {
        self.segments.iter().flat_map(|s| {
            let pts: [Option<Point>; 3] = match *s {
                Segment::MoveTo(p) | Segment::LineTo(p) => [Some(p), None, None],
                Segment::QuadTo(c, p) => [Some(c), Some(p), None],
                Segment::CubicTo(c1, c2, p) => [Some(c1), Some(c2), Some(p)],
                Segment::Close => [None, None, None],
            };
            pts.into_iter().flatten()
        })
    }

    /// Bounds of all stored points (the control-point hull, which contains
    /// the curve).
    pub fn bounds(&self) -> Rect {
        let first = self.first_point();
        let mut r = Rect {
            min_x: first.x,
            min_y: first.y,
            max_x: first.x,
            max_y: first.y,
        };
        for p in self.points() {
            r.min_x = r.min_x.min(p.x);
            r.min_y = r.min_y.min(p.y);
            r.max_x = r.max_x.max(p.x);
            r.max_y = r.max_y.max(p.y);
        }
        r
    }

    /// The same path moved by `(dx, dy)`, or `None` on overflow.
    pub fn translated(&self, dx: i32, dy: i32) -> Option<Path> {
        let mv = |p: Point| p.checked_offset(dx, dy);
        let segments = self
            .segments
            .iter()
            .map(|s| {
                Some(match *s {
                    Segment::MoveTo(p) => Segment::MoveTo(mv(p)?),
                    Segment::LineTo(p) => Segment::LineTo(mv(p)?),
                    Segment::QuadTo(c, p) => Segment::QuadTo(mv(c)?, mv(p)?),
                    Segment::CubicTo(c1, c2, p) => Segment::CubicTo(mv(c1)?, mv(c2)?, mv(p)?),
                    Segment::Close => Segment::Close,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Path { segments })
    }
}

// ---------------------------------------------------------------------------
// Style (paint) and instance
// ---------------------------------------------------------------------------

/// 8-bit sRGB colour with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Rgba8 {
        Rgba8 { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

/// Line joins. Only round joins exist in GY-GIR/0; miter and bevel are
/// planned for a later minor version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineJoin {
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fill {
    pub colour: Rgba8,
    pub rule: FillRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stroke {
    pub colour: Rgba8,
    /// Width in local millipoints; must be positive.
    pub width: u32,
    pub cap: LineCap,
    pub join: LineJoin,
}

/// Paint for one item. Fill is painted before stroke. A style with
/// neither is allowed: it is an invisible but hit-testable region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Style {
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

impl Style {
    pub fn validate(&self) -> Result<(), GfxDiag> {
        if let Some(s) = self.stroke {
            if s.width == 0 {
                return Err(GfxDiag::new(
                    codes::INVALID_STYLE,
                    "stroke width must be positive",
                ));
            }
        }
        Ok(())
    }
}

/// Row-vector affine transform: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
/// `a`–`d` are Q16.16; `e`, `f` are millipoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Affine {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
    pub e: i32,
    pub f: i32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: FIXED_ONE,
        b: 0,
        c: 0,
        d: FIXED_ONE,
        e: 0,
        f: 0,
    };

    pub const fn translate(e: i32, f: i32) -> Affine {
        Affine {
            e,
            f,
            ..Affine::IDENTITY
        }
    }

    /// The exact image of `p`, scaled by 2^16 (that is, in units of
    /// 1/65536 mp). Exact integer arithmetic: no rounding.
    pub fn apply_scaled(&self, p: Point) -> (i128, i128) {
        let (x, y) = (i128::from(p.x), i128::from(p.y));
        (
            i128::from(self.a) * x + i128::from(self.c) * y + (i128::from(self.e) << 16),
            i128::from(self.b) * x + i128::from(self.d) * y + (i128::from(self.f) << 16),
        )
    }

    /// The linear part applied to `(x, y)` (no translation), scaled by 2^16.
    pub fn linear_scaled(&self, x: i32, y: i32) -> (i128, i128) {
        let (x, y) = (i128::from(x), i128::from(y));
        (
            i128::from(self.a) * x + i128::from(self.c) * y,
            i128::from(self.b) * x + i128::from(self.d) * y,
        )
    }

    /// `self` followed by `outer` (`outer ∘ self`): the transform that
    /// applies `self` and then `outer`. Each product is rounded half-to-even
    /// from its exact value, so composing with a pure translation is exact.
    /// Returns `None` on overflow.
    pub fn then(&self, outer: &Affine) -> Option<Affine> {
        let w = |v: i32| i128::from(v);
        let round = |num: i128| -> Option<i32> {
            let q = num.div_euclid(65536);
            let r = num.rem_euclid(65536);
            let q = match r.cmp(&32768) {
                std::cmp::Ordering::Less => q,
                std::cmp::Ordering::Greater => q + 1,
                std::cmp::Ordering::Equal => q + (q & 1),
            };
            i32::try_from(q).ok()
        };
        let (a1, b1, c1, d1, e1, f1) = (
            w(self.a),
            w(self.b),
            w(self.c),
            w(self.d),
            w(self.e),
            w(self.f),
        );
        let (a2, b2, c2, d2) = (w(outer.a), w(outer.b), w(outer.c), w(outer.d));
        let e = i128::from(round(a2 * e1 + c2 * f1)?) + w(outer.e);
        let f = i128::from(round(b2 * e1 + d2 * f1)?) + w(outer.f);
        Some(Affine {
            a: round(a2 * a1 + c2 * b1)?,
            b: round(b2 * a1 + d2 * b1)?,
            c: round(a2 * c1 + c2 * d1)?,
            d: round(b2 * c1 + d2 * d1)?,
            e: i32::try_from(e).ok()?,
            f: i32::try_from(f).ok()?,
        })
    }

    /// The largest column norm of the linear part, as a plain factor. This
    /// bounds how much a local length can grow. Only basic IEEE operations
    /// are used, so the result is deterministic across hosts.
    pub fn max_scale(&self) -> f64 {
        let col = |p: i32, q: i32| {
            let (p, q) = (f64::from(p), f64::from(q));
            (p * p + q * q).sqrt()
        };
        col(self.a, self.b).max(col(self.c, self.d)) / f64::from(FIXED_ONE)
    }
}

/// One drawable: shared geometry, its paint, its placement and its
/// opacity modulation (`255` = opaque).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Item {
    pub path: Arc<Path>,
    pub style: Style,
    pub transform: Affine,
    pub opacity: u8,
}

/// A static vector drawing. Items are painted in order (first is lowest).
/// `width` × `height` is the logical canvas in millipoints.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Drawing {
    pub width: u32,
    pub height: u32,
    pub items: Vec<Item>,
}

// ---------------------------------------------------------------------------
// Quantisation (directive §7.4)
// ---------------------------------------------------------------------------

/// Converts floating-point points to millipoints with round-half-even
/// (Baseline, D3) and reports quantisation error. An encoder must reject
/// input whose error would exceed the declared tolerance instead of
/// silently degrading it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantiser {
    tolerance_pt: f64,
    max_error_pt: f64,
}

impl Quantiser {
    /// `tolerance_pt` is the largest acceptable error in points. Baseline's
    /// inherent worst case is 0.0005 pt.
    pub fn new(tolerance_pt: f64) -> Quantiser {
        Quantiser {
            tolerance_pt,
            max_error_pt: 0.0,
        }
    }

    pub fn coord(&mut self, pt: f64) -> Result<i32, GfxDiag> {
        if !pt.is_finite() {
            return Err(GfxDiag::new(
                codes::NON_FINITE,
                format!("coordinate {pt} is not finite"),
            ));
        }
        let mp = (pt * f64::from(MP_PER_PT)).round_ties_even();
        if mp < f64::from(i32::MIN) || mp > f64::from(i32::MAX) {
            return Err(GfxDiag::new(
                codes::COORDINATE_OVERFLOW,
                format!("{pt} pt is outside the millipoint range"),
            ));
        }
        let err = (mp / f64::from(MP_PER_PT) - pt).abs();
        if err > self.tolerance_pt {
            return Err(GfxDiag::new(
                codes::PRECISION_EXCEEDED,
                format!(
                    "{pt} pt quantises with error {err:e} pt over tolerance {:e}; use a higher-precision profile",
                    self.tolerance_pt
                ),
            ));
        }
        self.max_error_pt = self.max_error_pt.max(err);
        Ok(mp as i32)
    }

    pub fn point(&mut self, x_pt: f64, y_pt: f64) -> Result<Point, GfxDiag> {
        Ok(Point::new(self.coord(x_pt)?, self.coord(y_pt)?))
    }

    /// The largest error seen so far, in points.
    pub fn max_error_pt(&self) -> f64 {
        self.max_error_pt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn path_canonicalisation() {
        use Segment::*;
        let path = Path::new(vec![
            MoveTo(p(9, 9)),
            MoveTo(p(0, 0)),
            LineTo(p(10, 0)),
            LineTo(p(10, 10)),
            Close,
            Close,
            LineTo(p(5, 5)),
            MoveTo(p(1, 1)),
        ])
        .unwrap();
        assert_eq!(
            path.segments(),
            &[
                MoveTo(p(0, 0)),
                LineTo(p(10, 0)),
                LineTo(p(10, 10)),
                Close,
                MoveTo(p(0, 0)),
                LineTo(p(5, 5)),
            ]
        );
        // Canonicalisation is idempotent.
        assert_eq!(Path::new(path.segments().to_vec()).unwrap(), path);
        assert_eq!(path.first_point(), p(0, 0));
        assert_eq!(
            path.bounds(),
            Rect {
                min_x: 0,
                min_y: 0,
                max_x: 10,
                max_y: 10
            }
        );
    }

    #[test]
    fn path_rejections() {
        use Segment::*;
        for bad in [
            vec![],
            vec![LineTo(p(1, 1))],
            vec![Close],
            vec![MoveTo(p(1, 1))],
        ] {
            assert_eq!(Path::new(bad).unwrap_err().code, codes::MALFORMED_GEOMETRY);
        }
        let edge = Path::new(vec![MoveTo(p(i32::MAX, 0)), LineTo(p(0, 0))]).unwrap();
        assert!(edge.translated(1, 0).is_none());
        assert!(edge.translated(-1, 0).is_some());
    }

    #[test]
    fn quantiser() {
        let mut q = Quantiser::new(0.001);
        assert_eq!(q.coord(1.0).unwrap(), 1000);
        // 1/16 pt and 3/16 pt are exact halves of a millipoint: ties to even.
        assert_eq!(q.coord(0.0625).unwrap(), 62);
        assert_eq!(q.coord(0.1875).unwrap(), 188);
        assert_eq!(q.coord(-2.5004).unwrap(), -2500);
        assert!(q.max_error_pt() > 0.0003);
        let mut strict = Quantiser::new(1e-6);
        assert_eq!(
            strict.coord(0.12345).unwrap_err().code,
            codes::PRECISION_EXCEEDED
        );
        assert_eq!(q.coord(f64::NAN).unwrap_err().code, codes::NON_FINITE);
        assert_eq!(q.coord(3e6).unwrap_err().code, codes::COORDINATE_OVERFLOW);
    }

    #[test]
    fn affine_is_exact() {
        let t = Affine {
            a: FIXED_ONE / 2,
            b: 0,
            c: 0,
            d: 2 * FIXED_ONE,
            e: 7,
            f: -3,
        };
        assert_eq!(
            t.apply_scaled(p(3, 5)),
            ((3 * 32768) + (7 << 16), (5 * 131072) - (3 << 16))
        );
        assert_eq!(Affine::IDENTITY.max_scale(), 1.0);
        // Composition: translation is exact; scale then translate.
        let mv = Affine::translate(100, -50);
        assert_eq!(
            t.then(&mv).unwrap(),
            Affine {
                e: 107,
                f: -53,
                ..t
            }
        );
        let x2 = Affine {
            a: 2 * FIXED_ONE,
            d: 2 * FIXED_ONE,
            ..Affine::IDENTITY
        };
        let st = Affine::translate(10, 20).then(&x2).unwrap();
        assert_eq!((st.a, st.e, st.f), (2 * FIXED_ONE, 20, 40));
        assert_eq!(Affine::IDENTITY.then(&t).unwrap(), t);
        // Half-to-even on the exact product: 0.5 · 3 mp = 1.5 → 2, 0.5 · 5 → 2.
        let half = Affine {
            a: FIXED_ONE / 2,
            d: FIXED_ONE / 2,
            ..Affine::IDENTITY
        };
        assert_eq!(Affine::translate(3, 5).then(&half).unwrap().e, 2);
        assert_eq!(Affine::translate(3, 5).then(&half).unwrap().f, 2);
        assert!(Affine::translate(i32::MAX, 0)
            .then(&Affine::translate(1, 0))
            .is_none());
        assert_eq!(t.max_scale(), 2.0);
        assert!(Style {
            fill: None,
            stroke: Some(Stroke {
                colour: Rgba8::new(0, 0, 0, 255),
                width: 0,
                cap: LineCap::Butt,
                join: LineJoin::Round
            })
        }
        .validate()
        .is_err());
    }
}
