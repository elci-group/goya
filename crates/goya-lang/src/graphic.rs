//! `@graphic` and `@draw` (proposed; live profiles only).
//!
//! A `@graphic` declares a small vector drawing and its *known, finite,
//! bounded* state model in `.gscene`, so a target can precompute every
//! state at build time (GOYA-GFX-001 Phase 5 grammar; README decision 8:
//! authoring stays in `.gscene`). Arachnid compiles each one to a GY-GVI/0
//! inventory. Geometry inside a graphic uses the graphic's own local points
//! on its `canvas`; the graphic is placed in the scene by the cell that
//! draws it, through `position_p`/`extent_p` like any element, so every
//! scene length stays parent-relative.
//!
//! ```text
//! @graphic card {
//!   id: "graphic.card";
//!   canvas: (200, 280);
//!   @shape back { rect: (0, 0, 200, 280, 18); fill: #2b4c7e; }
//!   @shape star { path: "M 100 40 L 120 100 L 60 70 L 140 70 L 80 100 Z"; fill: #f2c14e; }
//!   @dimension face {
//!     step-s: 0.05;
//!     @value down { hide: star; }
//!     @value up { hide: back; }
//!   }
//! }
//! @cell slot { ...; @draw card { face: "up" when state.open is true; face: "down"; } }
//! ```
//!
//! Effects, applied in dimension order and then value order:
//! `hide: sel;`, `scale: sel (sx, sy);` about the canvas centre,
//! `move: sel (dx, dy);`, `fade: sel factor;`, `paint: sel #rrggbb;` (fill),
//! and `morph: shape "path" t;`. A selector is `all` or a list of shapes.
//!
//! A `@draw` rule list chooses one value per dimension: the first rule whose
//! `when` condition holds, and every dimension ends with an unconditional
//! rule. When a dimension declares `step-s`, a change between two values
//! steps through the values between them in declaration order, one every
//! `step-s` seconds.

use crate::parser::SourceRange;
use crate::scene::{Condition, SceneCell};
use std::collections::BTreeSet;

/// Largest canvas side, in points.
pub const MAX_CANVAS_PT: f64 = 4096.0;
/// Most shapes in one graphic.
pub const MAX_SHAPES: usize = 255;
/// Most dimensions in one graphic.
pub const MAX_DIMENSIONS: usize = 8;
/// Most values in one dimension.
pub const MAX_VALUES: usize = 64;
/// Bounds of `step-s`, in seconds.
pub const MIN_STEP_S: f64 = 0.02;
pub const MAX_STEP_S: f64 = 1.0;

/// One path command in local points (absolute coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCmd {
    Move(f64, f64),
    Line(f64, f64),
    Quad(f64, f64, f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

/// A shape's geometry.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// SVG path data subset: absolute `M L H V Q C Z`.
    Path(Vec<PathCmd>),
    /// `rect: (x, y, w, h)` or `(x, y, w, h, r)` with corner radius `r`.
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        r: f64,
    },
    /// `ellipse: (cx, cy, rx, ry)`.
    Ellipse { cx: f64, cy: f64, rx: f64, ry: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeDecl {
    pub name: String,
    pub geometry: Geometry,
    /// `#rrggbb` fill.
    pub fill: Option<u32>,
    /// `#rrggbb` stroke, with `stroke-width` in local points.
    pub stroke: Option<u32>,
    pub stroke_width: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    All,
    Shapes(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphicEffect {
    Hide(Selector),
    /// Scale about the canvas centre.
    Scale(Selector, f64, f64),
    Move(Selector, f64, f64),
    /// Multiply opacity by a factor in [0, 1].
    Fade(Selector, f64),
    /// Replace the fill colour (`#rrggbb`); the stroke is kept.
    Paint(Selector, u32),
    /// Interpolate one shape toward a path of the same structure.
    Morph(String, Vec<PathCmd>, f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphicValue {
    pub name: String,
    pub effects: Vec<GraphicEffect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphicDimension {
    pub name: String,
    pub step_s: Option<f64>,
    pub values: Vec<GraphicValue>,
}

/// `@graphic name { ... }`.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicDecl {
    pub name: String,
    pub authored_id: Option<String>,
    /// `canvas: (w, h)` in local points.
    pub canvas: (f64, f64),
    pub shapes: Vec<ShapeDecl>,
    pub dimensions: Vec<GraphicDimension>,
    pub range: SourceRange,
}

/// One `dimension: "value" [when state.k is v];` rule of a `@draw`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawRule {
    pub dimension: String,
    /// The value label; empty for a `dealt` rule.
    pub value: String,
    pub when: Option<Condition>,
    /// `dim: dealt`: the colour (`red`, `green`, `yellow` or `blue`) dealt
    /// to the enclosing `colour-group: deal` button. The rule does not
    /// match while that button has no colour, so it is never final.
    pub dealt: bool,
}

/// The values a dimension needs for a `dealt` rule.
pub const DEALT_VALUES: [&str; 4] = ["red", "green", "yellow", "blue"];

/// `@draw graphic { rules }` inside a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawDecl {
    pub graphic: String,
    pub rules: Vec<DrawRule>,
    pub range: SourceRange,
}

/// Parse SVG path data: absolute `M x y`, `L x y`, `H x`, `V y`,
/// `Q cx cy x y`, `C c1x c1y c2x c2y x y` and `Z`, separated by spaces or
/// commas. A command letter may be followed by several coordinate groups,
/// as in SVG. Returns `None` for anything else, including relative
/// commands.
pub fn parse_path_data(data: &str) -> Option<Vec<PathCmd>> {
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in data.chars() {
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            tokens.push(c.to_string());
        } else if c == ',' || c.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
        } else if c == '-' && !cur.is_empty() && !cur.ends_with(['e', 'E']) {
            tokens.push(std::mem::take(&mut cur));
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    let mut out = Vec::new();
    let mut i = 0;
    let mut cmd: Option<char> = None;
    let mut last = (0.0f64, 0.0f64);
    let num = |t: &str| -> Option<f64> { t.parse::<f64>().ok().filter(|v| v.is_finite()) };
    while i < tokens.len() {
        let t = &tokens[i];
        if t.len() == 1 && t.chars().all(|c| c.is_ascii_alphabetic()) {
            cmd = Some(t.chars().next()?);
            i += 1;
            if cmd == Some('Z') {
                out.push(PathCmd::Close);
                cmd = None;
            }
            continue;
        }
        let take = |i: &mut usize, n: usize| -> Option<Vec<f64>> {
            let v: Option<Vec<f64>> = tokens.get(*i..*i + n)?.iter().map(|t| num(t)).collect();
            *i += n;
            v
        };
        match cmd? {
            'M' => {
                let v = take(&mut i, 2)?;
                out.push(PathCmd::Move(v[0], v[1]));
                last = (v[0], v[1]);
                // Further pairs after M are implicit L, as in SVG.
                cmd = Some('L');
            }
            'L' => {
                let v = take(&mut i, 2)?;
                out.push(PathCmd::Line(v[0], v[1]));
                last = (v[0], v[1]);
            }
            'H' => {
                let v = take(&mut i, 1)?;
                out.push(PathCmd::Line(v[0], last.1));
                last.0 = v[0];
            }
            'V' => {
                let v = take(&mut i, 1)?;
                out.push(PathCmd::Line(last.0, v[0]));
                last.1 = v[0];
            }
            'Q' => {
                let v = take(&mut i, 4)?;
                out.push(PathCmd::Quad(v[0], v[1], v[2], v[3]));
                last = (v[2], v[3]);
            }
            'C' => {
                let v = take(&mut i, 6)?;
                out.push(PathCmd::Cubic(v[0], v[1], v[2], v[3], v[4], v[5]));
                last = (v[4], v[5]);
            }
            _ => return None,
        }
    }
    match out.first() {
        Some(PathCmd::Move(..)) => Some(out),
        _ => None,
    }
}

/// The verb sequence of a path, for morph structure checks.
pub fn path_verbs(cmds: &[PathCmd]) -> Vec<u8> {
    cmds.iter()
        .map(|c| match c {
            PathCmd::Move(..) => 0,
            PathCmd::Line(..) => 1,
            PathCmd::Quad(..) => 2,
            PathCmd::Cubic(..) => 3,
            PathCmd::Close => 4,
        })
        .collect()
}

fn coords(cmds: &[PathCmd]) -> Vec<f64> {
    let mut v = Vec::new();
    for c in cmds {
        match *c {
            PathCmd::Move(x, y) | PathCmd::Line(x, y) => v.extend([x, y]),
            PathCmd::Quad(a, b, x, y) => v.extend([a, b, x, y]),
            PathCmd::Cubic(a, b, c, d, x, y) => v.extend([a, b, c, d, x, y]),
            PathCmd::Close => {}
        }
    }
    v
}

/// Validate graphics and every `@draw` in the tree. Returns the state keys
/// the `@draw` conditions read (they count as used), or a message and the
/// range of the offending declaration.
pub fn validate(
    graphics: &[GraphicDecl],
    root: &SceneCell,
    key_type_ok: &dyn Fn(&Condition) -> bool,
) -> Result<BTreeSet<String>, (String, SourceRange)> {
    for (gi, g) in graphics.iter().enumerate() {
        let bad = |m: String| Err((format!("graphic {}: {m}", g.name), g.range));
        if graphics[..gi].iter().any(|o| o.name == g.name) {
            return bad("declared twice".into());
        }
        if g.authored_id.is_none() {
            return bad("needs an id".into());
        }
        let (w, h) = g.canvas;
        if !(w > 0.0 && h > 0.0 && w <= MAX_CANVAS_PT && h <= MAX_CANVAS_PT) {
            return bad(format!(
                "canvas must be positive and at most {MAX_CANVAS_PT} points"
            ));
        }
        if g.shapes.is_empty() || g.shapes.len() > MAX_SHAPES {
            return bad(format!("needs 1 to {MAX_SHAPES} shapes"));
        }
        for (si, s) in g.shapes.iter().enumerate() {
            if g.shapes[..si].iter().any(|o| o.name == s.name) {
                return bad(format!("shape {} is declared twice", s.name));
            }
            if s.fill.is_none() && s.stroke.is_none() {
                return bad(format!("shape {} needs a fill or a stroke", s.name));
            }
            if s.stroke.is_some() != s.stroke_width.is_some()
                || s.stroke_width
                    .is_some_and(|w| !(w > 0.0 && w <= MAX_CANVAS_PT))
            {
                return bad(format!(
                    "shape {} needs stroke and a positive stroke-width together",
                    s.name
                ));
            }
            let finite = match &s.geometry {
                Geometry::Path(p) => coords(p).iter().all(|v| v.abs() <= 4.0 * MAX_CANVAS_PT),
                Geometry::Rect { x, y, w, h, r } => {
                    *w > 0.0
                        && *h > 0.0
                        && *r >= 0.0
                        && 2.0 * r <= w.min(*h)
                        && [x, y].iter().all(|v| v.abs() <= MAX_CANVAS_PT)
                }
                Geometry::Ellipse { cx, cy, rx, ry } => {
                    *rx > 0.0
                        && *ry > 0.0
                        && [cx, cy, rx, ry].iter().all(|v| v.abs() <= MAX_CANVAS_PT)
                }
            };
            if !finite {
                return bad(format!("shape {} has invalid geometry", s.name));
            }
        }
        if g.dimensions.is_empty() || g.dimensions.len() > MAX_DIMENSIONS {
            return bad(format!("needs 1 to {MAX_DIMENSIONS} dimensions"));
        }
        let shape = |n: &str| g.shapes.iter().find(|s| s.name == n);
        for (di, d) in g.dimensions.iter().enumerate() {
            if g.dimensions[..di].iter().any(|o| o.name == d.name) {
                return bad(format!("dimension {} is declared twice", d.name));
            }
            if d.values.is_empty() || d.values.len() > MAX_VALUES {
                return bad(format!(
                    "dimension {} needs 1 to {MAX_VALUES} values",
                    d.name
                ));
            }
            if d.step_s
                .is_some_and(|s| !(MIN_STEP_S..=MAX_STEP_S).contains(&s))
            {
                return bad(format!(
                    "step-s must lie in [{MIN_STEP_S}, {MAX_STEP_S}] seconds"
                ));
            }
            for (vi, v) in d.values.iter().enumerate() {
                if d.values[..vi].iter().any(|o| o.name == v.name) {
                    return bad(format!("value {}.{} is declared twice", d.name, v.name));
                }
                for e in &v.effects {
                    let sel = match e {
                        GraphicEffect::Hide(s)
                        | GraphicEffect::Scale(s, ..)
                        | GraphicEffect::Move(s, ..)
                        | GraphicEffect::Fade(s, _)
                        | GraphicEffect::Paint(s, _) => Some(s),
                        GraphicEffect::Morph(name, target, t) => {
                            let Some(sh) = shape(name) else {
                                return bad(format!("morph names unknown shape {name}"));
                            };
                            let Geometry::Path(src) = &sh.geometry else {
                                return bad(format!("morph needs shape {name} to be a path"));
                            };
                            if path_verbs(src) != path_verbs(target) || !(0.0..=1.0).contains(t) {
                                return bad(format!(
                                    "morph of {name} needs a path of the same structure and t in [0, 1]"
                                ));
                            }
                            None
                        }
                    };
                    if let Some(Selector::Shapes(names)) = sel {
                        if names.is_empty() {
                            return bad("an effect selects no shape".into());
                        }
                        if let Some(n) = names.iter().find(|n| shape(n).is_none()) {
                            return bad(format!("an effect names unknown shape {n}"));
                        }
                    }
                    let ok = match e {
                        GraphicEffect::Scale(_, x, y) => {
                            x.is_finite() && y.is_finite() && x.abs() <= 16.0 && y.abs() <= 16.0
                        }
                        GraphicEffect::Move(_, x, y) => {
                            x.abs() <= MAX_CANVAS_PT && y.abs() <= MAX_CANVAS_PT
                        }
                        GraphicEffect::Fade(_, f) => (0.0..=1.0).contains(f),
                        _ => true,
                    };
                    if !ok {
                        return bad(format!(
                            "value {}.{} has an effect out of range",
                            d.name, v.name
                        ));
                    }
                }
            }
        }
    }
    let mut keys = BTreeSet::new();
    // Each cell with whether its nearest enclosing button (itself included)
    // deals colours.
    let mut stack = vec![(root, false)];
    while let Some((cell, outer_deals)) = stack.pop() {
        let deals = if cell.kind == crate::scene::CellKind::Button {
            cell.colour_group.as_ref().is_some_and(|g| g.dealt())
        } else {
            outer_deals
        };
        stack.extend(cell.children.iter().map(|c| (c, deals)));
        let Some(draw) = &cell.draw else { continue };
        let bad = |m: String| {
            Err((
                format!("@draw {} on {}: {m}", draw.graphic, cell.name),
                draw.range,
            ))
        };
        let Some(g) = graphics.iter().find(|g| g.name == draw.graphic) else {
            return bad("no such graphic".into());
        };
        for d in &g.dimensions {
            let rules: Vec<&DrawRule> = draw
                .rules
                .iter()
                .filter(|r| r.dimension == d.name)
                .collect();
            match rules.last() {
                Some(r) if r.when.is_none() && !r.dealt => {}
                _ => {
                    return bad(format!(
                        "dimension {} needs a final rule without `when`",
                        d.name
                    ))
                }
            }
            if rules[..rules.len() - 1].iter().any(|r| r.when.is_none() && !r.dealt) {
                return bad(format!(
                    "dimension {} has an unreachable rule after an unconditional one",
                    d.name
                ));
            }
            for r in rules {
                if r.dealt {
                    if !deals {
                        return bad(format!(
                            "dimension {} uses dealt outside a colour-group: deal button",
                            d.name
                        ));
                    }
                    if let Some(missing) = DEALT_VALUES.iter().find(|c| !d.values.iter().any(|v| v.name == **c)) {
                        return bad(format!("dimension {} needs a value \"{missing}\" for dealt", d.name));
                    }
                } else if !d.values.iter().any(|v| v.name == r.value) {
                    return bad(format!("dimension {} has no value \"{}\"", d.name, r.value));
                }
                if let Some(c) = &r.when {
                    if !key_type_ok(c) {
                        return bad(format!(
                            "when compares '{}' with a literal of another type or an undeclared key",
                            c.key
                        ));
                    }
                    keys.insert(c.key.clone());
                }
            }
        }
        if let Some(r) = draw
            .rules
            .iter()
            .find(|r| !g.dimensions.iter().any(|d| d.name == r.dimension))
        {
            return bad(format!("graphic has no dimension {}", r.dimension));
        }
    }
    Ok(keys)
}

// Every number in these declarations comes from `parse_number` or
// `parse_path_data`, which refuse non-finite values, so `PartialEq` is a
// total equivalence on every value the parser produces.
impl Eq for PathCmd {}
impl Eq for Geometry {}
impl Eq for ShapeDecl {}
impl Eq for GraphicEffect {}
impl Eq for GraphicValue {}
impl Eq for GraphicDimension {}
impl Eq for GraphicDecl {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_data_subset() {
        let p = parse_path_data("M 0 0 L 10,0 H 20 V 5 Q 1 2 3 4 C 1 2 3 4 5 6 Z").unwrap();
        assert_eq!(p.len(), 7);
        assert_eq!(p[2], PathCmd::Line(20.0, 0.0));
        assert_eq!(p[3], PathCmd::Line(20.0, 5.0));
        assert_eq!(
            parse_path_data("M 0 0 1 1 2 2").unwrap().len(),
            3,
            "implicit L after M"
        );
        assert_eq!(
            parse_path_data("M-1-2L3-4").unwrap()[1],
            PathCmd::Line(3.0, -4.0)
        );
        assert!(
            parse_path_data("m 0 0 l 1 1").is_none(),
            "relative commands are refused"
        );
        assert!(parse_path_data("L 1 1").is_none(), "must start with M");
        assert!(parse_path_data("M 0 0 L 1").is_none(), "incomplete group");
    }
}
