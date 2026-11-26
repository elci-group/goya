//! Import SVG artwork as `@graphic` source.
//!
//! `@shape path:` accepts only the absolute subset `M L H V Q C Z` (see
//! [`crate::graphic::parse_path_data`]). Real-world icons use the whole SVG
//! path grammar: relative commands, smooth curves (`S`, `T`), elliptical
//! arcs (`A`), and minified numbers such as `.5.5` or `1-2`. This module
//! reads that grammar, converts arcs to cubic Béziers, fits the drawing into
//! a canvas, and prints a `@graphic` block that `parse_path_data` reads back.
//!
//! Only `<path d>` elements are imported, each with an optional `fill`.
//! A `transform`, `<use>`, a stroke, or a non-hex fill is refused rather than
//! approximated, so an imported drawing is either exact or rejected.

use crate::graphic::PathCmd;

/// An SVG file reduced to what a `@graphic` can hold.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgArt {
    /// `viewBox` as (min x, min y, width, height).
    pub view_box: (f64, f64, f64, f64),
    /// Each `<path>`: its commands in view-box coordinates and its fill.
    pub paths: Vec<(Vec<PathCmd>, Option<u32>)>,
}

/// How to place imported art on a `@graphic` canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub canvas: (f64, f64),
    /// Margin on every side, as a fraction of the shorter canvas side.
    pub inset: f64,
    /// Fill for paths that declare none (SVG's default is black).
    pub fill: u32,
    /// Optional rounded background `(colour, corner radius fraction)`.
    pub background: Option<(u32, f64)>,
}

/// Parse full SVG path data into absolute `M L Q C Z` commands.
pub fn parse_svg_path(d: &str) -> Result<Vec<PathCmd>, String> {
    let mut sc = Scanner {
        s: d.as_bytes(),
        i: 0,
    };
    let mut out = Vec::new();
    let (mut cur, mut start) = ((0.0f64, 0.0f64), (0.0f64, 0.0f64));
    // Reflection points for S and T.
    let mut last_cubic: Option<(f64, f64)> = None;
    let mut last_quad: Option<(f64, f64)> = None;
    let mut cmd: Option<u8> = None;
    loop {
        sc.skip_separators();
        if sc.done() {
            break;
        }
        if let Some(c) = sc.command() {
            cmd = Some(c);
            if c == b'Z' || c == b'z' {
                out.push(PathCmd::Close);
                cur = start;
                last_cubic = None;
                last_quad = None;
                continue;
            }
        } else if cmd.is_none() {
            return Err(format!(
                "path data must start with a command at byte {}",
                sc.i
            ));
        }
        let c = cmd.ok_or("missing command")?;
        let rel = c.is_ascii_lowercase();
        let base = if rel { cur } else { (0.0, 0.0) };
        let p = |sc: &mut Scanner| -> Result<(f64, f64), String> {
            let x = sc.number()?;
            let y = sc.number()?;
            Ok((base.0 + x, base.1 + y))
        };
        let (mut next_cubic, mut next_quad) = (None, None);
        match c.to_ascii_uppercase() {
            b'M' => {
                cur = p(&mut sc)?;
                start = cur;
                out.push(PathCmd::Move(cur.0, cur.1));
                // Further pairs after M are implicit L (l after m).
                cmd = Some(if rel { b'l' } else { b'L' });
            }
            b'L' => {
                cur = p(&mut sc)?;
                out.push(PathCmd::Line(cur.0, cur.1));
            }
            b'H' => {
                let x = sc.number()?;
                cur.0 = if rel { cur.0 + x } else { x };
                out.push(PathCmd::Line(cur.0, cur.1));
            }
            b'V' => {
                let y = sc.number()?;
                cur.1 = if rel { cur.1 + y } else { y };
                out.push(PathCmd::Line(cur.0, cur.1));
            }
            b'C' => {
                let (c1, c2, e) = (p(&mut sc)?, p(&mut sc)?, p(&mut sc)?);
                out.push(PathCmd::Cubic(c1.0, c1.1, c2.0, c2.1, e.0, e.1));
                next_cubic = Some(c2);
                cur = e;
            }
            b'S' => {
                let c1 = reflect(last_cubic, cur);
                let (c2, e) = (p(&mut sc)?, p(&mut sc)?);
                out.push(PathCmd::Cubic(c1.0, c1.1, c2.0, c2.1, e.0, e.1));
                next_cubic = Some(c2);
                cur = e;
            }
            b'Q' => {
                let (q, e) = (p(&mut sc)?, p(&mut sc)?);
                out.push(PathCmd::Quad(q.0, q.1, e.0, e.1));
                next_quad = Some(q);
                cur = e;
            }
            b'T' => {
                let q = reflect(last_quad, cur);
                let e = p(&mut sc)?;
                out.push(PathCmd::Quad(q.0, q.1, e.0, e.1));
                next_quad = Some(q);
                cur = e;
            }
            b'A' => {
                let rx = sc.number()?;
                let ry = sc.number()?;
                let phi = sc.number()?;
                let large = sc.flag()?;
                let sweep = sc.flag()?;
                let e = p(&mut sc)?;
                arc_to_cubics(&mut out, cur, rx, ry, phi, large, sweep, e);
                cur = e;
            }
            other => return Err(format!("unsupported path command '{}'", other as char)),
        }
        last_cubic = next_cubic;
        last_quad = next_quad;
    }
    match out.first() {
        Some(PathCmd::Move(..)) => Ok(out),
        _ => Err("path data is empty or does not start with M".into()),
    }
}

fn reflect(ctrl: Option<(f64, f64)>, cur: (f64, f64)) -> (f64, f64) {
    ctrl.map_or(cur, |c| (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1))
}

struct Scanner<'a> {
    s: &'a [u8],
    i: usize,
}

impl Scanner<'_> {
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
    fn skip_separators(&mut self) {
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',')
        {
            self.i += 1;
        }
    }
    fn command(&mut self) -> Option<u8> {
        let c = *self.s.get(self.i)?;
        if c.is_ascii_alphabetic() && c != b'e' && c != b'E' {
            self.i += 1;
            Some(c)
        } else {
            None
        }
    }
    /// One SVG number: sign, digits, at most one '.', optional exponent.
    /// A second '.' or a sign starts the next number, as in `.5.5` or `1-2`.
    fn number(&mut self) -> Result<f64, String> {
        self.skip_separators();
        let start = self.i;
        let s = self.s;
        let mut j = self.i;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let mut digits = 0;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
            digits += 1;
        }
        if j < s.len() && s[j] == b'.' {
            j += 1;
            while j < s.len() && s[j].is_ascii_digit() {
                j += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(format!("expected a number at byte {start}"));
        }
        if j < s.len() && (s[j] == b'e' || s[j] == b'E') {
            let mut k = j + 1;
            if k < s.len() && (s[k] == b'+' || s[k] == b'-') {
                k += 1;
            }
            if k < s.len() && s[k].is_ascii_digit() {
                while k < s.len() && s[k].is_ascii_digit() {
                    k += 1;
                }
                j = k;
            }
        }
        self.i = j;
        std::str::from_utf8(&s[start..j])
            .ok()
            .and_then(|t| t.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("bad number at byte {start}"))
    }
    /// An arc flag is a single '0' or '1' and needs no separator after it.
    fn flag(&mut self) -> Result<bool, String> {
        self.skip_separators();
        match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(format!("expected an arc flag at byte {}", self.i)),
        }
    }
}

/// SVG 1.1 implementation notes F.6.5/F.6.6: endpoint to centre
/// parameterisation, then one cubic per quarter turn or less.
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(
    out: &mut Vec<PathCmd>,
    p0: (f64, f64),
    rx: f64,
    ry: f64,
    phi_deg: f64,
    large: bool,
    sweep: bool,
    p1: (f64, f64),
) {
    if p0 == p1 {
        return;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        out.push(PathCmd::Line(p1.0, p1.1));
        return;
    }
    let phi = phi_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (p0.0 - p1.0) / 2.0;
    let dy = (p0.1 - p1.1) / 2.0;
    let x1 = cos * dx + sin * dy;
    let y1 = -sin * dx + cos * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut co = (num / den).max(0.0).sqrt();
    if large == sweep {
        co = -co;
    }
    let cxp = co * rx * y1 / ry;
    let cyp = -co * ry * x1 / rx;
    let cx = cos * cxp - sin * cyp + (p0.0 + p1.0) / 2.0;
    let cy = sin * cxp + cos * cyp + (p0.1 + p1.1) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let t1 = angle(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut dt = angle(
        (x1 - cxp) / rx,
        (y1 - cyp) / ry,
        (-x1 - cxp) / rx,
        (-y1 - cyp) / ry,
    );
    let tau = std::f64::consts::TAU;
    if !sweep && dt > 0.0 {
        dt -= tau;
    } else if sweep && dt < 0.0 {
        dt += tau;
    }
    let n = (dt.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = dt / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        (
            cx + rx * c * cos - ry * s * sin,
            cy + rx * c * sin + ry * s * cos,
        )
    };
    let deriv = |t: f64| {
        let (s, c) = t.sin_cos();
        (-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos)
    };
    let mut t = t1;
    for seg in 0..n {
        let a = point(t);
        let da = deriv(t);
        let t2 = t + step;
        let b = if seg + 1 == n { p1 } else { point(t2) };
        let db = deriv(t2);
        out.push(PathCmd::Cubic(
            a.0 + k * da.0,
            a.1 + k * da.1,
            b.0 - k * db.0,
            b.1 - k * db.1,
            b.0,
            b.1,
        ));
        t = t2;
    }
}

/// Read the `viewBox` and every `<path>` of a self-contained SVG file.
pub fn parse_svg(svg: &str) -> Result<SvgArt, String> {
    for refused in [
        "<use",
        "<image",
        "<text",
        "<circle",
        "<rect",
        "<ellipse",
        "<polygon",
        "<polyline",
        "<line",
    ] {
        if svg.contains(refused) {
            return Err(format!(
                "{refused}> elements are not imported; convert them to <path> first"
            ));
        }
    }
    if svg.contains("transform=") {
        return Err("transform attributes are not imported; flatten the drawing first".into());
    }
    if svg.contains("<style") || svg.contains("class=") {
        return Err("CSS styling is not imported; use fill attributes".into());
    }
    let root = tag(svg, "<svg").ok_or("no <svg> element")?;
    let view_box = match attr(root, "viewBox") {
        Some(v) => {
            let n: Vec<f64> = v
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|t| !t.is_empty())
                .map(|t| t.parse::<f64>().map_err(|_| format!("bad viewBox {v:?}")))
                .collect::<Result<_, _>>()?;
            if n.len() != 4 || n[2] <= 0.0 || n[3] <= 0.0 {
                return Err(format!("bad viewBox {v:?}"));
            }
            (n[0], n[1], n[2], n[3])
        }
        None => {
            let num = |k| attr(root, k).and_then(|v| v.trim_end_matches("px").parse::<f64>().ok());
            match (num("width"), num("height")) {
                (Some(w), Some(h)) if w > 0.0 && h > 0.0 => (0.0, 0.0, w, h),
                _ => {
                    return Err(
                        "the <svg> element needs a viewBox or numeric width and height".into(),
                    )
                }
            }
        }
    };
    let root_fill = attr(root, "fill")
        .map(|f| parse_fill(&f))
        .transpose()?
        .flatten();
    let mut paths = Vec::new();
    let mut rest = svg;
    while let Some(t) = tag(rest, "<path") {
        if attr(t, "stroke").is_some_and(|s| s != "none") {
            return Err("stroked paths are not imported; outline the strokes first".into());
        }
        let d = attr(t, "d").ok_or("a <path> has no d attribute")?;
        let fill = match attr(t, "fill") {
            Some(f) => parse_fill(&f)?,
            None => root_fill,
        };
        let cmds = parse_svg_path(&d)?;
        if fill != Some(NO_FILL) {
            paths.push((cmds, fill));
        }
        let end = rest.find(t).map(|p| p + t.len()).unwrap_or(rest.len());
        rest = &rest[end..];
    }
    if paths.is_empty() {
        return Err("no filled <path> elements".into());
    }
    Ok(SvgArt { view_box, paths })
}

/// Marker for `fill="none"`: the path draws nothing and is skipped.
const NO_FILL: u32 = u32::MAX;

/// `#rgb`, `#rrggbb`, `none` (Some(NO_FILL)) or `currentColor` (None).
fn parse_fill(f: &str) -> Result<Option<u32>, String> {
    let f = f.trim();
    if f == "none" {
        return Ok(Some(NO_FILL));
    }
    if f == "currentColor" {
        return Ok(None);
    }
    let hex = f
        .strip_prefix('#')
        .ok_or_else(|| format!("fill {f:?} is not a hex colour"))?;
    let v = u32::from_str_radix(hex, 16).map_err(|_| format!("fill {f:?} is not a hex colour"))?;
    match hex.len() {
        6 => Ok(Some(v)),
        3 => {
            let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
            Ok(Some(((r * 17) << 16) | ((g * 17) << 8) | (b * 17)))
        }
        _ => Err(format!("fill {f:?} is not a hex colour")),
    }
}

/// The text of the first `<name ...>` start tag.
fn tag<'a>(s: &'a str, open: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(p) = s[from..].find(open) {
        let at = from + p;
        let after = s.as_bytes().get(at + open.len()).copied();
        if matches!(after, Some(b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/')) {
            let end = s[at..].find('>')? + at + 1;
            return Some(&s[at..end]);
        }
        from = at + open.len();
    }
    None
}

/// An attribute value from a start tag, with either quote style.
fn attr(tag: &str, name: &str) -> Option<String> {
    let b = tag.as_bytes();
    let mut from = 0;
    while let Some(p) = tag[from..].find(name) {
        let at = from + p;
        let before_ok = at > 0 && b[at - 1].is_ascii_whitespace();
        let mut j = at + name.len();
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if before_ok && b.get(j) == Some(&b'=') {
            j += 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            let q = *b.get(j)?;
            if q == b'"' || q == b'\'' {
                let end = tag[j + 1..].find(q as char)? + j + 1;
                return Some(tag[j + 1..end].to_string());
            }
        }
        from = at + name.len();
    }
    None
}

/// Map commands through `x' = sx·x + dx`, `y' = sy·y + dy`.
pub fn transform(cmds: &[PathCmd], s: f64, dx: f64, dy: f64) -> Vec<PathCmd> {
    let x = |v: f64| s * v + dx;
    let y = |v: f64| s * v + dy;
    cmds.iter()
        .map(|c| match *c {
            PathCmd::Move(a, b) => PathCmd::Move(x(a), y(b)),
            PathCmd::Line(a, b) => PathCmd::Line(x(a), y(b)),
            PathCmd::Quad(a, b, c, d) => PathCmd::Quad(x(a), y(b), x(c), y(d)),
            PathCmd::Cubic(a, b, c, d, e, f) => PathCmd::Cubic(x(a), y(b), x(c), y(d), x(e), y(f)),
            PathCmd::Close => PathCmd::Close,
        })
        .collect()
}

fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    let s = format!("{r:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Print commands in the absolute subset `parse_path_data` reads.
pub fn format_path(cmds: &[PathCmd]) -> String {
    let mut out: Vec<String> = Vec::new();
    for c in cmds {
        out.push(match *c {
            PathCmd::Move(a, b) => format!("M {} {}", num(a), num(b)),
            PathCmd::Line(a, b) => format!("L {} {}", num(a), num(b)),
            PathCmd::Quad(a, b, c, d) => format!("Q {} {} {} {}", num(a), num(b), num(c), num(d)),
            PathCmd::Cubic(a, b, c, d, e, f) => {
                format!(
                    "C {} {} {} {} {} {}",
                    num(a),
                    num(b),
                    num(c),
                    num(d),
                    num(e),
                    num(f)
                )
            }
            PathCmd::Close => "Z".into(),
        });
    }
    out.join(" ")
}

/// Render `art` as a `@graphic` block: centred, scaled uniformly to fit the
/// canvas minus the inset, one `@shape` per path, and a single-valued
/// `@dimension look` so a `@draw` can select it (`look: "plain"`).
pub fn to_graphic(art: &SvgArt, name: &str, id: &str, place: &Placement) -> Result<String, String> {
    let (cw, ch) = place.canvas;
    if !(cw > 0.0 && ch > 0.0 && (0.0..0.5).contains(&place.inset)) {
        return Err("canvas must be positive and inset in [0, 0.5)".into());
    }
    let (vx, vy, vw, vh) = art.view_box;
    let margin = place.inset * cw.min(ch);
    let s = ((cw - 2.0 * margin) / vw).min((ch - 2.0 * margin) / vh);
    let dx = (cw - s * vw) / 2.0 - s * vx;
    let dy = (ch - s * vh) / 2.0 - s * vy;
    let mut out = format!(
        "  @graphic {name} {{\n    id: \"{id}\";\n    canvas: ({}, {});\n",
        num(cw),
        num(ch)
    );
    if let Some((bg, r)) = place.background {
        let radius = (r.clamp(0.0, 0.5) * cw.min(ch)).floor();
        out.push_str(&format!(
            "    @shape tile {{ rect: (0, 0, {}, {}, {}); fill: #{bg:06x}; }}\n",
            num(cw),
            num(ch),
            num(radius)
        ));
    }
    let many = art.paths.len() > 1;
    for (k, (cmds, fill)) in art.paths.iter().enumerate() {
        let path = format_path(&transform(cmds, s, dx, dy));
        crate::graphic::parse_path_data(&path).ok_or("converted path does not read back")?;
        let shape = if many {
            format!("mark_{}", k + 1)
        } else {
            "mark".into()
        };
        out.push_str(&format!(
            "    @shape {shape} {{ path: \"{path}\"; fill: #{:06x}; }}\n",
            fill.unwrap_or(place.fill)
        ));
    }
    out.push_str("    @dimension look { @value plain { } }\n  }\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minified_numbers_split_like_svg() {
        let c = parse_svg_path("M.5.5l1-2.5.25e1-1").unwrap();
        assert_eq!(
            c,
            vec![
                PathCmd::Move(0.5, 0.5),
                PathCmd::Line(1.5, -2.0),
                PathCmd::Line(4.0, -3.0)
            ]
        );
    }

    #[test]
    fn relative_and_shorthand_commands_become_absolute() {
        let c = parse_svg_path("m10 10h5v5H10zM0 0c1 0 2 1 2 2s1 2 2 2q1 0 1 1t1 1").unwrap();
        assert_eq!(c[1], PathCmd::Line(15.0, 10.0));
        assert_eq!(c[2], PathCmd::Line(15.0, 15.0));
        assert_eq!(c[3], PathCmd::Line(10.0, 15.0));
        assert_eq!(c[4], PathCmd::Close);
        assert_eq!(c[6], PathCmd::Cubic(1.0, 0.0, 2.0, 1.0, 2.0, 2.0));
        // S reflects the previous second control point (2,1) about (2,2).
        assert_eq!(c[7], PathCmd::Cubic(2.0, 3.0, 3.0, 4.0, 4.0, 4.0));
        assert_eq!(c[8], PathCmd::Quad(5.0, 4.0, 5.0, 5.0));
        // T reflects (5,4) about (5,5).
        assert_eq!(c[9], PathCmd::Quad(5.0, 6.0, 6.0, 6.0));
    }

    #[test]
    fn arcs_become_cubics_ending_exactly_on_the_endpoint() {
        // A full circle as two half arcs, with compact flags "0 01" style.
        let c = parse_svg_path("M2 12a10 10 0 0120 0A10 10 0 012 12z").unwrap();
        let cubics: Vec<_> = c
            .iter()
            .filter(|c| matches!(c, PathCmd::Cubic(..)))
            .collect();
        assert_eq!(cubics.len(), 4, "{c:?}");
        let PathCmd::Cubic(.., x, y) = *cubics[1] else {
            unreachable!()
        };
        assert_eq!((x, y), (22.0, 12.0));
        // The quarter-way point lies on the circle (top, y = 2).
        let PathCmd::Cubic(.., x, y) = *cubics[0] else {
            unreachable!()
        };
        assert!((x - 12.0).abs() < 1e-9 && (y - 2.0).abs() < 1e-9, "{x} {y}");
    }

    #[test]
    fn graphic_output_reads_back_and_is_centred() {
        let svg = r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>T</title><path d="M0 0h24v24H0z"/></svg>"#;
        let art = parse_svg(svg).unwrap();
        let g = to_graphic(
            &art,
            "t",
            "graphic.t",
            &Placement {
                canvas: (200.0, 100.0),
                inset: 0.1,
                fill: 0xff0000,
                background: Some((0x112233, 0.2)),
            },
        )
        .unwrap();
        assert!(g.contains("canvas: (200, 100);"), "{g}");
        assert!(
            g.contains("rect: (0, 0, 200, 100, 20); fill: #112233;"),
            "{g}"
        );
        // 24 units into 80 points, centred horizontally: x from 60 to 140.
        assert!(
            g.contains(r#"path: "M 60 10 L 140 10 L 140 90 L 60 90 Z"; fill: #ff0000;"#),
            "{g}"
        );
        let src = format!("@ontology t {{\n{g}}}");
        assert!(src.contains("@dimension look"));
    }

    #[test]
    fn unsupported_svg_is_refused_not_approximated() {
        for svg in [
            r#"<svg viewBox="0 0 1 1"><g transform="scale(2)"><path d="M0 0L1 1"/></g></svg>"#,
            r#"<svg viewBox="0 0 1 1"><circle cx="1" cy="1" r="1"/></svg>"#,
            r##"<svg viewBox="0 0 1 1"><path d="M0 0L1 1" stroke="#000"/></svg>"##,
            r#"<svg viewBox="0 0 1 1"><path d="M0 0L1 1" fill="red"/></svg>"#,
        ] {
            assert!(parse_svg(svg).is_err(), "{svg}");
        }
        let art = parse_svg(r##"<svg viewBox="0 0 2 2"><path d="M0 0H1" fill="none"/><path d="M0 0H2V2z" fill="#abc"/></svg>"##).unwrap();
        assert_eq!(art.paths.len(), 1);
        assert_eq!(art.paths[0].1, Some(0xaabbcc));
    }
}
