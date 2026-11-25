//! The CPU reference renderer: the conformance oracle for GIR and for every
//! format that decodes to it. It favours being obviously correct and
//! deterministic over being fast, and it is **not** a production backend.
//!
//! Method:
//!
//! - Curves are flattened uniformly, with a step count chosen from the
//!   second-difference bound so that the chord error is at most
//!   [`TOLERANCE_PX`].
//! - Fills are scan-converted at [`SUBSAMPLES`] sub-scanlines per pixel row,
//!   with exact horizontal span coverage, under the nonzero or even-odd
//!   rule.
//! - Strokes are expanded in local space into one quad per segment, plus
//!   disks for round joins and round caps (square caps extend the end
//!   segments). Every polygon is oriented the same way, so a nonzero fill
//!   of the pieces is exactly their union.
//! - Compositing is source-over on premultiplied values in the sRGB-encoded
//!   space. This is the conventional non-linear blending SVG renderers use
//!   by default.
//!
//! Determinism: integer geometry is mapped to device space through the
//! exact constant `a·ox + c·oy + e·2^16`, using coordinates relative to the
//! path's first point, so translating geometry into or out of an instance
//! transform gives bit-identical output. Only `+ − × ÷ sqrt` are used, and
//! Rust never fuses these operations, so output is identical on every IEEE
//! 754 host.

use crate::{codes, Affine, Drawing, FillRule, GfxDiag, LineCap, Path, Rgba8, Segment, FIXED_ONE};

/// Largest chord deviation allowed when flattening curves, in device px.
pub const TOLERANCE_PX: f64 = 0.05;
/// Sub-scanlines per pixel row.
pub const SUBSAMPLES: usize = 16;
/// Cubic Bézier circle constant, 4(√2 − 1)/3.
const KAPPA: f64 = 0.552_284_749_830_793_6;

/// Resource limits enforced while rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderLimits {
    pub max_pixels: u64,
    /// Total polygon edges across the whole drawing.
    pub max_edges: usize,
    /// Most line segments a single curve may be flattened into.
    pub max_curve_steps: u32,
}

impl Default for RenderLimits {
    fn default() -> RenderLimits {
        RenderLimits {
            max_pixels: 1 << 22,
            max_edges: 4_000_000,
            max_curve_steps: 256,
        }
    }
}

/// A rendered image: row-major, 8-bit sRGB with straight alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// The largest difference between two same-sized images in any
    /// premultiplied channel (alpha included), in 8-bit levels. This is
    /// the tolerance metric for comparing renders (directive §5.3).
    /// Straight-alpha RGB is unsuitable because it jumps arbitrarily where
    /// alpha is near zero. Returns `None` if the sizes differ.
    pub fn max_premultiplied_diff(&self, other: &Image) -> Option<u8> {
        if (self.width, self.height) != (other.width, other.height) {
            return None;
        }
        let pm = |px: &[u8]| -> [u32; 4] {
            let a = u32::from(px[3]);
            [
                (u32::from(px[0]) * a + 127) / 255,
                (u32::from(px[1]) * a + 127) / 255,
                (u32::from(px[2]) * a + 127) / 255,
                a,
            ]
        };
        Some(
            self.rgba
                .chunks(4)
                .zip(other.rgba.chunks(4))
                .map(|(x, y)| {
                    let (p, q) = (pm(x), pm(y));
                    (0..4).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0) as u8
                })
                .max()
                .unwrap_or(0),
        )
    }

    /// Netpbm PAM (`RGB_ALPHA`) encoding, for inspection and golden files.
    pub fn to_pam(&self) -> Vec<u8> {
        let mut out = format!(
            "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",
            self.width, self.height
        )
        .into_bytes();
        out.extend_from_slice(&self.rgba);
        out
    }
}

type Pt = (f64, f64);

/// Maps path-local coordinates (relative to the path's first point) to
/// device pixels.
struct DeviceMap {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    kx: f64,
    ky: f64,
    scale: f64,
}

impl DeviceMap {
    fn new(t: &Affine, path: &Path, px_per_mp: f64) -> DeviceMap {
        let (kx, ky) = t.apply_scaled(path.first_point());
        DeviceMap {
            a: f64::from(t.a),
            b: f64::from(t.b),
            c: f64::from(t.c),
            d: f64::from(t.d),
            kx: kx as f64,
            ky: ky as f64,
            scale: px_per_mp / f64::from(FIXED_ONE),
        }
    }

    fn apply(&self, (x, y): Pt) -> Pt {
        (
            (self.a * x + self.c * y + self.kx) * self.scale,
            (self.b * x + self.d * y + self.ky) * self.scale,
        )
    }
}

struct Subpath {
    pts: Vec<Pt>,
    closed: bool,
}

fn steps(dd: f64, factor: f64, tol: f64, max: u32) -> u32 {
    let n = (factor * dd / tol).sqrt().ceil();
    if n.is_finite() {
        (n as u32).clamp(1, max)
    } else {
        max
    }
}

fn dist((x0, y0): Pt, (x1, y1): Pt, (x2, y2): Pt) -> f64 {
    let (x, y) = (x0 - 2.0 * x1 + x2, y0 - 2.0 * y1 + y2);
    (x * x + y * y).sqrt()
}

fn push_quad(out: &mut Vec<Pt>, p0: Pt, p1: Pt, p2: Pt, tol: f64, max: u32) {
    // Error bound for n uniform steps: |p0 − 2p1 + p2| / (4n²).
    let n = steps(dist(p0, p1, p2), 0.25, tol, max);
    for i in 1..=n {
        let t = f64::from(i) / f64::from(n);
        let mt = 1.0 - t;
        let (w0, w1, w2) = (mt * mt, 2.0 * mt * t, t * t);
        out.push((
            w0 * p0.0 + w1 * p1.0 + w2 * p2.0,
            w0 * p0.1 + w1 * p1.1 + w2 * p2.1,
        ));
    }
}

fn push_cubic(out: &mut Vec<Pt>, p0: Pt, p1: Pt, p2: Pt, p3: Pt, tol: f64, max: u32) {
    // Error bound for n uniform steps: 3·max(second differences) / (4n²).
    let n = steps(dist(p0, p1, p2).max(dist(p1, p2, p3)), 0.75, tol, max);
    for i in 1..=n {
        let t = f64::from(i) / f64::from(n);
        let mt = 1.0 - t;
        let (w0, w1, w2, w3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
        out.push((
            w0 * p0.0 + w1 * p1.0 + w2 * p2.0 + w3 * p3.0,
            w0 * p0.1 + w1 * p1.1 + w2 * p2.1 + w3 * p3.1,
        ));
    }
}

/// Flatten `path` into polylines. Coordinates are taken relative to the
/// path's first point and passed through `map` before subdivision.
fn flatten(path: &Path, map: &dyn Fn(Pt) -> Pt, tol: f64, max: u32) -> Vec<Subpath> {
    let o = path.first_point();
    let local = |p: crate::Point| {
        map((
            (i64::from(p.x) - i64::from(o.x)) as f64,
            (i64::from(p.y) - i64::from(o.y)) as f64,
        ))
    };
    let mut out: Vec<Subpath> = Vec::new();
    for seg in path.segments() {
        match *seg {
            Segment::MoveTo(p) => out.push(Subpath {
                pts: vec![local(p)],
                closed: false,
            }),
            Segment::Close => {
                if let Some(sp) = out.last_mut() {
                    sp.closed = true;
                }
            }
            _ => {
                // Canonical paths always have a current subpath here.
                let Some(sp) = out.last_mut() else { continue };
                let cur = *sp.pts.last().unwrap_or(&(0.0, 0.0));
                match *seg {
                    Segment::LineTo(p) => sp.pts.push(local(p)),
                    Segment::QuadTo(c, p) => {
                        push_quad(&mut sp.pts, cur, local(c), local(p), tol, max)
                    }
                    Segment::CubicTo(c1, c2, p) => {
                        push_cubic(&mut sp.pts, cur, local(c1), local(c2), local(p), tol, max)
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

fn circle((cx, cy): Pt, r: f64, tol: f64, max: u32) -> Vec<Pt> {
    let k = r * KAPPA;
    let mut pts = vec![(cx + r, cy)];
    let quarter = [
        ((cx + r, cy + k), (cx + k, cy + r), (cx, cy + r)),
        ((cx - k, cy + r), (cx - r, cy + k), (cx - r, cy)),
        ((cx - r, cy - k), (cx - k, cy - r), (cx, cy - r)),
        ((cx + k, cy - r), (cx + r, cy - k), (cx + r, cy)),
    ];
    for (c1, c2, p) in quarter {
        let cur = *pts.last().unwrap_or(&(cx + r, cy));
        push_cubic(&mut pts, cur, c1, c2, p, tol, max);
    }
    pts.pop(); // last point repeats the first
    pts
}

fn orient_positive(poly: &mut [Pt]) {
    let n = poly.len();
    let mut area2 = 0.0;
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        area2 += x0 * y1 - x1 * y0;
    }
    if area2 < 0.0 {
        poly.reverse();
    }
}

/// Expand polylines (in local units) into same-orientation polygons whose
/// union is the stroke outline.
fn stroke_polygons(
    subpaths: &[Subpath],
    hw: f64,
    cap: LineCap,
    tol: f64,
    max: u32,
) -> Vec<Vec<Pt>> {
    let mut polys = Vec::new();
    for sp in subpaths {
        let mut pts: Vec<Pt> = Vec::with_capacity(sp.pts.len());
        for &p in &sp.pts {
            if pts.last() != Some(&p) {
                pts.push(p);
            }
        }
        if sp.closed && pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        let n = pts.len();
        if n == 1 {
            // Zero-length subpath: only a round cap paints a dot.
            if cap == LineCap::Round {
                polys.push(circle(pts[0], hw, tol, max));
            }
            continue;
        }
        let seg_count = if sp.closed { n } else { n - 1 };
        for i in 0..seg_count {
            let (mut p, mut q) = (pts[i], pts[(i + 1) % n]);
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let len = (dx * dx + dy * dy).sqrt();
            let (ux, uy) = (dx / len, dy / len);
            if !sp.closed && cap == LineCap::Square {
                if i == 0 {
                    p = (p.0 - ux * hw, p.1 - uy * hw);
                }
                if i == seg_count - 1 {
                    q = (q.0 + ux * hw, q.1 + uy * hw);
                }
            }
            let (nx, ny) = (-uy * hw, ux * hw);
            polys.push(vec![
                (p.0 + nx, p.1 + ny),
                (q.0 + nx, q.1 + ny),
                (q.0 - nx, q.1 - ny),
                (p.0 - nx, p.1 - ny),
            ]);
        }
        let joins = if sp.closed { 0..n } else { 1..n - 1 };
        for i in joins {
            polys.push(circle(pts[i], hw, tol, max));
        }
        if !sp.closed && cap == LineCap::Round {
            polys.push(circle(pts[0], hw, tol, max));
            polys.push(circle(pts[n - 1], hw, tol, max));
        }
    }
    for p in &mut polys {
        orient_positive(p);
    }
    polys
}

struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    dir: i32,
}

/// Coverage in `[0, 1]` per pixel for closed polygons under `rule`.
fn rasterize(
    polys: &[Vec<Pt>],
    rule: FillRule,
    w: usize,
    h: usize,
    edge_budget: &mut usize,
) -> Result<Vec<f32>, GfxDiag> {
    let mut edges = Vec::new();
    for poly in polys {
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if a.1 == b.1 {
                continue;
            }
            if *edge_budget == 0 {
                return Err(GfxDiag::new(
                    codes::TOO_MANY_EDGES,
                    "drawing exceeds the edge budget",
                ));
            }
            *edge_budget -= 1;
            edges.push(if a.1 < b.1 {
                Edge {
                    x0: a.0,
                    y0: a.1,
                    x1: b.0,
                    y1: b.1,
                    dir: 1,
                }
            } else {
                Edge {
                    x0: b.0,
                    y0: b.1,
                    x1: a.0,
                    y1: a.1,
                    dir: -1,
                }
            });
        }
    }
    let mut cov = vec![0f32; w * h];
    if edges.is_empty() {
        return Ok(cov);
    }
    edges.sort_by(|p, q| p.y0.total_cmp(&q.y0));
    let first_row = edges[0].y0.floor().max(0.0) as usize;
    let mut next = 0;
    let mut active: Vec<&Edge> = Vec::new();
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    let mut row_acc = vec![0f64; w];
    let sub_weight = 1.0 / SUBSAMPLES as f64;
    for row in first_row..h {
        let top = row as f64;
        while next < edges.len() && edges[next].y0 < top + 1.0 {
            active.push(&edges[next]);
            next += 1;
        }
        active.retain(|e| e.y1 > top);
        if active.is_empty() {
            if next == edges.len() {
                break;
            }
            continue;
        }
        row_acc.iter_mut().for_each(|v| *v = 0.0);
        for k in 0..SUBSAMPLES {
            let sy = top + (k as f64 + 0.5) * sub_weight;
            crossings.clear();
            for e in &active {
                if e.y0 <= sy && sy < e.y1 {
                    let x = e.x0 + (sy - e.y0) * (e.x1 - e.x0) / (e.y1 - e.y0);
                    crossings.push((x, e.dir));
                }
            }
            crossings.sort_by(|p, q| p.0.total_cmp(&q.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                let inside = match rule {
                    FillRule::NonZero => winding != 0,
                    FillRule::EvenOdd => winding % 2 != 0,
                };
                if inside {
                    add_span(&mut row_acc, pair[0].0, pair[1].0, sub_weight);
                }
            }
        }
        let out = &mut cov[row * w..(row + 1) * w];
        for (o, &v) in out.iter_mut().zip(&row_acc) {
            *o = v.min(1.0) as f32;
        }
    }
    Ok(cov)
}

fn add_span(acc: &mut [f64], xa: f64, xb: f64, weight: f64) {
    let w = acc.len() as f64;
    let (xa, xb) = (xa.clamp(0.0, w), xb.clamp(0.0, w));
    if xb <= xa {
        return;
    }
    let p0 = xa.floor() as usize;
    let p1 = (xb.ceil() as usize).min(acc.len());
    for (px, slot) in acc.iter_mut().enumerate().take(p1).skip(p0) {
        let l = px as f64;
        let overlap = xb.min(l + 1.0) - xa.max(l);
        if overlap > 0.0 {
            *slot += overlap * weight;
        }
    }
}

fn composite(acc: &mut [[f32; 4]], cov: &[f32], colour: Rgba8, opacity: u8) {
    let base = f32::from(colour.a) / 255.0 * f32::from(opacity) / 255.0;
    let src = [
        f32::from(colour.r) / 255.0,
        f32::from(colour.g) / 255.0,
        f32::from(colour.b) / 255.0,
        1.0,
    ];
    for (dst, &c) in acc.iter_mut().zip(cov) {
        let alpha = c * base;
        if alpha <= 0.0 {
            continue;
        }
        for ch in 0..4 {
            dst[ch] = src[ch] * alpha + dst[ch] * (1.0 - alpha);
        }
    }
}

/// Render `drawing` at `px_per_pt` device pixels per point.
pub fn render(drawing: &Drawing, px_per_pt: f64, limits: &RenderLimits) -> Result<Image, GfxDiag> {
    if !(px_per_pt.is_finite() && px_per_pt > 0.0) {
        return Err(GfxDiag::new(
            codes::NON_FINITE,
            "px_per_pt must be finite and positive",
        ));
    }
    let px_per_mp = px_per_pt / 1000.0;
    let wf = (f64::from(drawing.width) * px_per_mp).ceil();
    let hf = (f64::from(drawing.height) * px_per_mp).ceil();
    if wf * hf > limits.max_pixels as f64 || wf > f64::from(u32::MAX) || hf > f64::from(u32::MAX) {
        return Err(GfxDiag::new(
            codes::CANVAS_TOO_LARGE,
            format!(
                "{wf}×{hf} px exceeds the limit of {} pixels",
                limits.max_pixels
            ),
        ));
    }
    let (w, h) = (wf as usize, hf as usize);
    let mut acc = vec![[0f32; 4]; w * h];
    let mut budget = limits.max_edges;
    let max = limits.max_curve_steps;

    for item in &drawing.items {
        let dev = DeviceMap::new(&item.transform, &item.path, px_per_mp);
        if let Some(fill) = item.style.fill {
            let polys: Vec<Vec<Pt>> = flatten(&item.path, &|p| dev.apply(p), TOLERANCE_PX, max)
                .into_iter()
                .map(|sp| sp.pts)
                .collect();
            let cov = rasterize(&polys, fill.rule, w, h, &mut budget)?;
            composite(&mut acc, &cov, fill.colour, item.opacity);
        }
        if let Some(stroke) = item.style.stroke {
            let growth = item.transform.max_scale() * px_per_mp;
            if growth > 0.0 {
                let tol = TOLERANCE_PX / growth;
                let local = flatten(&item.path, &|p| p, tol, max);
                let hw = f64::from(stroke.width) / 2.0;
                let mut polys = stroke_polygons(&local, hw, stroke.cap, tol, max);
                for poly in &mut polys {
                    for p in poly.iter_mut() {
                        *p = dev.apply(*p);
                    }
                }
                let cov = rasterize(&polys, FillRule::NonZero, w, h, &mut budget)?;
                composite(&mut acc, &cov, stroke.colour, item.opacity);
            }
        }
    }

    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in &acc {
        let a = px[3];
        if a <= 0.0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for &v in &px[..3] {
            rgba.push((v / a * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        rgba.push((a * 255.0).round().clamp(0.0, 255.0) as u8);
    }
    Ok(Image {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Fill, Item, LineJoin, Point, Stroke, Style};
    use std::sync::Arc;

    const RED: Rgba8 = Rgba8::new(255, 0, 0, 255);
    const BLUE: Rgba8 = Rgba8::new(0, 0, 255, 255);

    fn rect_path(x0: i32, y0: i32, x1: i32, y1: i32) -> Path {
        use Segment::*;
        Path::new(vec![
            MoveTo(Point::new(x0, y0)),
            LineTo(Point::new(x1, y0)),
            LineTo(Point::new(x1, y1)),
            LineTo(Point::new(x0, y1)),
            Close,
        ])
        .unwrap()
    }

    fn fill(colour: Rgba8, rule: FillRule) -> Style {
        Style {
            fill: Some(Fill { colour, rule }),
            stroke: None,
        }
    }

    fn item(path: Path, style: Style, transform: Affine) -> Item {
        Item {
            path: Arc::new(path),
            style,
            transform,
            opacity: 255,
        }
    }

    fn drawing(w_pt: u32, h_pt: u32, items: Vec<Item>) -> Drawing {
        Drawing {
            width: w_pt * 1000,
            height: h_pt * 1000,
            items,
        }
    }

    fn render1(d: &Drawing) -> Image {
        render(d, 1.0, &RenderLimits::default()).unwrap()
    }

    #[test]
    fn pixel_aligned_square_is_exact() {
        let d = drawing(
            4,
            4,
            vec![item(
                rect_path(1000, 1000, 3000, 3000),
                fill(RED, FillRule::NonZero),
                Affine::IDENTITY,
            )],
        );
        let img = render1(&d);
        assert_eq!((img.width, img.height), (4, 4));
        for y in 0..4 {
            for x in 0..4 {
                let inside = (1..3).contains(&x) && (1..3).contains(&y);
                let want = if inside {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 0, 0]
                };
                assert_eq!(img.pixel(x, y), want, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn half_pixel_coverage() {
        let d = drawing(
            2,
            1,
            vec![item(
                rect_path(500, 0, 1000, 1000),
                fill(RED, FillRule::NonZero),
                Affine::IDENTITY,
            )],
        );
        let img = render1(&d);
        assert_eq!(img.pixel(0, 0), [255, 0, 0, 128]);
        assert_eq!(img.pixel(1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn fill_rules_differ_on_nested_same_direction_contours() {
        let mut segs = rect_path(0, 0, 4000, 4000).segments().to_vec();
        segs.extend_from_slice(rect_path(1000, 1000, 3000, 3000).segments());
        let nested = Path::new(segs).unwrap();
        let nz = render1(&drawing(
            4,
            4,
            vec![item(
                nested.clone(),
                fill(RED, FillRule::NonZero),
                Affine::IDENTITY,
            )],
        ));
        let eo = render1(&drawing(
            4,
            4,
            vec![item(nested, fill(RED, FillRule::EvenOdd), Affine::IDENTITY)],
        ));
        assert_eq!(nz.pixel(2, 2)[3], 255);
        assert_eq!(eo.pixel(2, 2)[3], 0);
        assert_eq!(eo.pixel(0, 0)[3], 255);
    }

    fn stroke(width: u32, cap: LineCap) -> Style {
        Style {
            fill: None,
            stroke: Some(Stroke {
                colour: BLUE,
                width,
                cap,
                join: LineJoin::Round,
            }),
        }
    }

    fn hline() -> Path {
        use Segment::*;
        Path::new(vec![
            MoveTo(Point::new(2000, 5000)),
            LineTo(Point::new(8000, 5000)),
        ])
        .unwrap()
    }

    #[test]
    fn stroke_caps() {
        let render_cap = |cap| {
            render1(&drawing(
                10,
                10,
                vec![item(hline(), stroke(2000, cap), Affine::IDENTITY)],
            ))
        };
        let butt = render_cap(LineCap::Butt);
        // Butt: exactly x 2..8, y 4..6.
        assert_eq!(butt.pixel(2, 4), [0, 0, 255, 255]);
        assert_eq!(butt.pixel(7, 5), [0, 0, 255, 255]);
        assert_eq!(butt.pixel(1, 4)[3], 0);
        assert_eq!(butt.pixel(8, 5)[3], 0);
        assert_eq!(butt.pixel(4, 3)[3], 0);
        let square = render_cap(LineCap::Square);
        assert_eq!(square.pixel(1, 4)[3], 255);
        assert_eq!(square.pixel(8, 5)[3], 255);
        assert_eq!(square.pixel(0, 4)[3], 0);
        let round = render_cap(LineCap::Round);
        let a = round.pixel(1, 4)[3];
        assert!(
            a > 100 && a < 255,
            "round cap partially covers the corner pixel ({a})"
        );
    }

    #[test]
    fn mirrored_stroke_keeps_union() {
        // A negative determinant flips every piece's orientation equally,
        // so joins and segments must still add up rather than cancel.
        use Segment::*;
        let corner = Path::new(vec![
            MoveTo(Point::new(2000, 2000)),
            LineTo(Point::new(8000, 2000)),
            LineTo(Point::new(8000, 8000)),
        ])
        .unwrap();
        let mirror = Affine {
            a: -FIXED_ONE,
            e: 10_000,
            ..Affine::IDENTITY
        };
        let plain = render1(&drawing(
            10,
            10,
            vec![item(
                corner.clone(),
                stroke(2000, LineCap::Butt),
                Affine::IDENTITY,
            )],
        ));
        let flipped = render1(&drawing(
            10,
            10,
            vec![item(corner, stroke(2000, LineCap::Butt), mirror)],
        ));
        for y in 0..10 {
            for x in 0..10 {
                assert_eq!(plain.pixel(x, y), flipped.pixel(9 - x, y), "pixel {x},{y}");
            }
        }
        assert_eq!(plain.pixel(7, 2)[3], 255, "join region is solid");
    }

    #[test]
    fn opacity_and_order() {
        let mut top = item(
            rect_path(0, 0, 2000, 2000),
            fill(BLUE, FillRule::NonZero),
            Affine::IDENTITY,
        );
        top.opacity = 128;
        let d = drawing(
            2,
            2,
            vec![
                item(
                    rect_path(0, 0, 2000, 2000),
                    fill(RED, FillRule::NonZero),
                    Affine::IDENTITY,
                ),
                top,
            ],
        );
        let px = render1(&d).pixel(0, 0);
        assert_eq!(px[3], 255);
        assert!(
            px[0] > 120 && px[0] < 135 && px[2] > 120 && px[2] < 135,
            "{px:?}"
        );
    }

    #[test]
    fn translation_placement_is_bit_identical() {
        // Geometry at (3,3) pt with identity vs. geometry at the origin
        // placed by translation: device coordinates are computed exactly the
        // same way, so the images must match bit for bit.
        use Segment::*;
        let curve = |x: i32, y: i32| {
            Path::new(vec![
                MoveTo(Point::new(x, y)),
                CubicTo(
                    Point::new(x + 3000, y - 2000),
                    Point::new(x + 5000, y + 4000),
                    Point::new(x + 6000, y + 1000),
                ),
                QuadTo(Point::new(x + 2000, y + 5000), Point::new(x, y)),
                Close,
            ])
            .unwrap()
        };
        let style = Style {
            fill: Some(Fill {
                colour: RED,
                rule: FillRule::EvenOdd,
            }),
            stroke: Some(Stroke {
                colour: BLUE,
                width: 700,
                cap: LineCap::Round,
                join: LineJoin::Round,
            }),
        };
        let scaled = |e, f| Affine {
            a: 98_304,
            b: 8_192,
            c: -16_384,
            d: 73_728,
            e,
            f,
        };
        let a = render(
            &drawing(12, 12, vec![item(curve(3000, 3000), style, scaled(0, 0))]),
            3.0,
            &RenderLimits::default(),
        )
        .unwrap();
        // Same image: origin geometry, translation folded into e/f.
        let (dx, dy) = scaled(0, 0).linear_scaled(3000, 3000);
        assert_eq!((dx % 65536, dy % 65536), (0, 0));
        let b = render(
            &drawing(
                12,
                12,
                vec![item(
                    curve(0, 0),
                    style,
                    scaled((dx >> 16) as i32, (dy >> 16) as i32),
                )],
            ),
            3.0,
            &RenderLimits::default(),
        )
        .unwrap();
        assert_eq!(a, b);
        assert!(a.rgba.chunks(4).any(|p| p[3] == 255));
    }

    #[test]
    fn limits_are_enforced() {
        let d = drawing(
            100,
            100,
            vec![item(
                rect_path(0, 0, 1000, 1000),
                fill(RED, FillRule::NonZero),
                Affine::IDENTITY,
            )],
        );
        let small = RenderLimits {
            max_pixels: 100,
            ..RenderLimits::default()
        };
        assert_eq!(
            render(&d, 1.0, &small).unwrap_err().code,
            codes::CANVAS_TOO_LARGE
        );
        let few = RenderLimits {
            max_edges: 1,
            ..RenderLimits::default()
        };
        assert_eq!(
            render(&d, 1.0, &few).unwrap_err().code,
            codes::TOO_MANY_EDGES
        );
        assert_eq!(
            render(&d, f64::NAN, &RenderLimits::default())
                .unwrap_err()
                .code,
            codes::NON_FINITE
        );
    }

    #[test]
    fn premultiplied_diff_ignores_invisible_colour() {
        let img = |rgba: Vec<u8>| Image {
            width: 2,
            height: 1,
            rgba,
        };
        let a = img(vec![0, 0, 0, 0, 255, 0, 0, 255]);
        let b = img(vec![0, 0, 255, 1, 254, 0, 0, 255]);
        // Pixel 0: colour differs by 255 but alpha only by 1.
        assert_eq!(a.max_premultiplied_diff(&b), Some(1));
        assert_eq!(a.max_premultiplied_diff(&a), Some(0));
        let c = Image {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        assert_eq!(a.max_premultiplied_diff(&c), None);
    }

    #[test]
    fn deterministic_and_pam() {
        let d = drawing(
            8,
            8,
            vec![item(
                rect_path(500, 700, 7300, 6100),
                fill(RED, FillRule::NonZero),
                Affine::IDENTITY,
            )],
        );
        let a = render(&d, 2.0, &RenderLimits::default()).unwrap();
        assert_eq!(a, render(&d, 2.0, &RenderLimits::default()).unwrap());
        let pam = a.to_pam();
        assert!(pam.starts_with(b"P7\nWIDTH 16\n"));
        assert_eq!(
            pam.len() - pam.windows(7).position(|w| w == b"ENDHDR\n").unwrap() - 7,
            16 * 16 * 4
        );
    }
}
