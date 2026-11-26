//! `.gscene` graphics lowered to GY-GVI/0, shared by every Goya target that
//! draws `@graphic` (Arachnid on webOS, Aragon on Android).
//!
//! * **Lowering.** Shapes become GY-GIR/0 items in declaration order, with
//!   geometry in millipoints on the graphic's canvas. Rectangles with a
//!   corner radius use quadratic corners, ellipses four cubic arcs. Effects
//!   map one to one onto GVI effects: `hide` → `Hide`, `scale` (about the
//!   canvas centre) and `move` → `Transform`, `fade` → `Opacity`, `paint` →
//!   `Restyle` (per shape, keeping its stroke), `morph` → `Morph`. Every
//!   state combination is reachable (`Product`).
//! * **Encoding.** Each graphic is compiled with transforms kept and baked,
//!   each with GVI-D (depth 8), GVI-A (provenance) and GVI-O; GVI's
//!   never-bigger chain picks forms inside each, and the smaller file wins
//!   (ties keep transforms).
//! * **Verification.** The chosen bytes are opened with the reference
//!   decoder (`Inventory::open`) and checked with `verify_against` before
//!   any target packages them.
//! * **Conformance.** [`reference_json`] reports every state as the
//!   reference decoder sees it, for the targets' runtime decoders to match.

use goya_gvi::{
    compile_with, AlbitProfile, Bake, Budget, CompileOptions, Dimension, Effect, GviLimits,
    Inventory, OperativeProfile, Reachability, Selector, StateModel, Value,
};
use goya_graphics_ir::{Affine, Drawing, Fill, FillRule, Item, LineCap, LineJoin, Path, Point, Rgba8, Segment, Stroke, Style};
use goya_gva::GvaLimits;
use goya_lang::graphic::{GraphicDecl, GraphicEffect, Geometry, PathCmd, Selector as Sel};
use sha2::{Digest, Sha256};
use std::sync::Arc;



/// Differential depth Arachnid asks GVI-D for.
pub const DIFF_DEPTH: u32 = 8;

/// One compiled graphic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    pub name: String,
    pub authored_id: String,
    pub parent: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub binding: String,
    /// Canvas in millipoints.
    pub width: u32,
    pub height: u32,
    /// Per dimension: name, value labels, step in milliseconds.
    pub dims: Vec<(String, Vec<String>, Option<u32>)>,
    pub states: usize,
    pub projections: usize,
    /// Encoding chosen: whether transforms are baked.
    pub baked: bool,
    /// Size of the other candidate, for the compilation report.
    pub other_size: usize,
}

fn mp(v: f64) -> i32 {
    (v * 1000.0).round() as i32
}

fn colour(rgb: u32) -> Rgba8 {
    Rgba8::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

pub fn path_from(cmds: &[PathCmd]) -> Option<Path> {
    let p = |x: f64, y: f64| Point::new(mp(x), mp(y));
    let segs = cmds
        .iter()
        .map(|c| match *c {
            PathCmd::Move(x, y) => Segment::MoveTo(p(x, y)),
            PathCmd::Line(x, y) => Segment::LineTo(p(x, y)),
            PathCmd::Quad(a, b, x, y) => Segment::QuadTo(p(a, b), p(x, y)),
            PathCmd::Cubic(a, b, c, d, x, y) => Segment::CubicTo(p(a, b), p(c, d), p(x, y)),
            PathCmd::Close => Segment::Close,
        })
        .collect();
    Path::new(segs).ok()
}

/// Geometry in local points as path commands.
pub fn geometry_cmds(g: &Geometry) -> Vec<PathCmd> {
    match *g {
        Geometry::Path(ref p) => p.clone(),
        Geometry::Rect { x, y, w, h, r } if r > 0.0 => vec![
            PathCmd::Move(x + r, y),
            PathCmd::Line(x + w - r, y),
            PathCmd::Quad(x + w, y, x + w, y + r),
            PathCmd::Line(x + w, y + h - r),
            PathCmd::Quad(x + w, y + h, x + w - r, y + h),
            PathCmd::Line(x + r, y + h),
            PathCmd::Quad(x, y + h, x, y + h - r),
            PathCmd::Line(x, y + r),
            PathCmd::Quad(x, y, x + r, y),
            PathCmd::Close,
        ],
        Geometry::Rect { x, y, w, h, .. } => vec![
            PathCmd::Move(x, y),
            PathCmd::Line(x + w, y),
            PathCmd::Line(x + w, y + h),
            PathCmd::Line(x, y + h),
            PathCmd::Close,
        ],
        Geometry::Ellipse { cx, cy, rx, ry } => {
            // Four cubic arcs; k is the standard quarter-circle constant.
            let k = 0.552_284_749_830_793_4;
            let (kx, ky) = (k * rx, k * ry);
            vec![
                PathCmd::Move(cx + rx, cy),
                PathCmd::Cubic(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry),
                PathCmd::Cubic(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy),
                PathCmd::Cubic(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry),
                PathCmd::Cubic(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy),
                PathCmd::Close,
            ]
        }
    }
}

fn style_of(fill: Option<u32>, stroke: Option<u32>, width: Option<f64>) -> Style {
    Style {
        fill: fill.map(|c| Fill { colour: colour(c), rule: FillRule::NonZero }),
        stroke: stroke.map(|c| Stroke {
            colour: colour(c),
            width: mp(width.unwrap_or(1.0)).max(1) as u32,
            cap: LineCap::Round,
            join: LineJoin::Round,
        }),
    }
}

fn q16(v: f64) -> i32 {
    (v * 65536.0).round() as i32
}

/// Lower one graphic to its source drawing and state model.
pub fn lower(scene: &str, g: &GraphicDecl) -> Result<(Drawing, StateModel), String> {
    let mut items = Vec::new();
    for s in &g.shapes {
        let path = path_from(&geometry_cmds(&s.geometry))
            .ok_or_else(|| format!("shape {} has no drawable geometry", s.name))?;
        items.push(Item {
            path: Arc::new(path),
            style: style_of(s.fill, s.stroke, s.stroke_width),
            transform: Affine::IDENTITY,
            opacity: 255,
        });
    }
    let index = |n: &str| g.shapes.iter().position(|s| s.name == n).map(|i| i as u32);
    let sel = |s: &Sel| -> Selector {
        match s {
            Sel::All => Selector::All,
            Sel::Shapes(names) => {
                let mut ids: Vec<u32> = names.iter().filter_map(|n| index(n)).collect();
                ids.sort_unstable();
                ids.dedup();
                Selector::Items(ids)
            }
        }
    };
    let (cx, cy) = (mp(g.canvas.0 / 2.0), mp(g.canvas.1 / 2.0));
    let mut dims = Vec::new();
    for d in &g.dimensions {
        let mut values = Vec::new();
        for v in &d.values {
            let mut value = Value::new(&v.name);
            for e in &v.effects {
                match e {
                    GraphicEffect::Hide(s) => value = value.with(Effect::Hide(sel(s))),
                    GraphicEffect::Scale(s, sx, sy) => {
                        let (a, dd) = (q16(*sx), q16(*sy));
                        let t = Affine {
                            a,
                            b: 0,
                            c: 0,
                            d: dd,
                            e: cx - (f64::from(cx) * sx).round() as i32,
                            f: cy - (f64::from(cy) * sy).round() as i32,
                        };
                        value = value.with(Effect::Transform(sel(s), t));
                    }
                    GraphicEffect::Move(s, dx, dy) => {
                        let t = Affine { e: mp(*dx), f: mp(*dy), ..Affine::IDENTITY };
                        value = value.with(Effect::Transform(sel(s), t));
                    }
                    GraphicEffect::Fade(s, f) => {
                        value = value.with(Effect::Opacity(sel(s), (f * 255.0).round() as u8))
                    }
                    GraphicEffect::Paint(s, rgb) => {
                        let Selector::Items(ids) = sel(s) else {
                            for (i, shape) in g.shapes.iter().enumerate() {
                                let style = style_of(Some(*rgb), shape.stroke, shape.stroke_width);
                                value = value.with(Effect::Restyle(Selector::Items(vec![i as u32]), style));
                            }
                            continue;
                        };
                        for i in ids {
                            let shape = &g.shapes[i as usize];
                            let style = style_of(Some(*rgb), shape.stroke, shape.stroke_width);
                            value = value.with(Effect::Restyle(Selector::Items(vec![i]), style));
                        }
                    }
                    GraphicEffect::Morph(shape, target, t) => {
                        let i = index(shape).ok_or_else(|| format!("unknown shape {shape}"))?;
                        let target = path_from(target).ok_or_else(|| format!("morph target of {shape} is not drawable"))?;
                        value = value.with(Effect::Morph(
                            Selector::Items(vec![i]),
                            Arc::new(target),
                            (t * 65536.0).round() as u32,
                        ));
                    }
                }
            }
            values.push(value);
        }
        dims.push(Dimension::discrete(&d.name, values));
    }
    let drawing = Drawing { width: mp(g.canvas.0) as u32, height: mp(g.canvas.1) as u32, items };
    let model = StateModel {
        parent: format!("{scene}#{}", g.authored_id.as_deref().unwrap_or(&g.name)),
        dimensions: dims,
        reachability: Reachability::Product { exclusions: vec![] },
    };
    Ok((drawing, model))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compile, choose and verify one graphic.
pub fn compile(scene: &str, g: &GraphicDecl) -> Result<Compiled, String> {
    let kept = compile_encoding(scene, g, false)?;
    let baked = compile_encoding(scene, g, true)?;
    Ok(if baked.bytes.len() < kept.bytes.len() {
        Compiled { other_size: kept.bytes.len(), ..baked }
    } else {
        Compiled { other_size: baked.bytes.len(), ..kept }
    })
}

/// Compile and verify one graphic with transforms kept or baked.
pub fn compile_encoding(scene: &str, g: &GraphicDecl, bake: bool) -> Result<Compiled, String> {
    let (drawing, model) = lower(scene, g)?;
    let budget = Budget::default();
    let gva = GvaLimits::default();
    let options = CompileOptions {
        bake: if bake { Bake::Transforms } else { Bake::None },
        max_diff_depth: DIFF_DEPTH,
        albit: AlbitProfile::Provenance,
        operative: OperativeProfile::Fast,
    };
    let bytes = compile_with(&drawing, &model, &budget, &gva, options).map_err(|e| e.to_string())?;
    let (is_baked, other) = (bake, 0);
    let binding = model
        .binding(drawing.items.len() as u32, &budget)
        .map_err(|e| e.to_string())?;
    let inv = Inventory::open(&bytes, &binding, &GviLimits::default()).map_err(|e| e.to_string())?;
    inv.verify_against(&drawing, &model, &budget, &gva).map_err(|e| e.to_string())?;
    let dims = g
        .dimensions
        .iter()
        .map(|d| {
            (
                d.name.clone(),
                d.values.iter().map(|v| v.name.clone()).collect(),
                d.step_s.map(|s| (s * 1000.0).round() as u32),
            )
        })
        .collect();
    Ok(Compiled {
        name: g.name.clone(),
        authored_id: g.authored_id.clone().unwrap_or_else(|| g.name.clone()),
        parent: model.parent.clone(),
        sha256: hex(&Sha256::digest(&bytes)),
        binding: hex(&binding.0),
        width: drawing.width,
        height: drawing.height,
        dims,
        states: inv.state_count(),
        projections: inv.projection_count(),
        baked: is_baked,
        other_size: other,
        bytes,
    })
}

/// Every state of a compiled graphic as the reference decoder sees it, as
/// JSON: `[[key, [[item, [segments...], style, [a,b,c,d,e,f], opacity]...]]...]`.
/// The runtime decoder's conformance test compares against this.
pub fn reference_json(c: &Compiled, g: &GraphicDecl, scene: &str) -> Result<String, String> {
    let (drawing, model) = lower(scene, g)?;
    let radices: Vec<usize> = c.dims.iter().map(|d| d.1.len()).collect();
    reference_json_for(&c.bytes, &drawing, &model, &radices)
}

/// [`reference_json`] for any inventory; unreachable states are left out.
pub fn reference_json_for(
    bytes: &[u8],
    drawing: &Drawing,
    model: &StateModel,
    radices: &[usize],
) -> Result<String, String> {
    let binding = model.binding(drawing.items.len() as u32, &Budget::default()).map_err(|e| e.to_string())?;
    let inv = Inventory::open(bytes, &binding, &GviLimits::default()).map_err(|e| e.to_string())?;
    let paths = inv.decode_geometry().map_err(|e| e.to_string())?;
    let total: usize = radices.iter().product();
    let mut states = Vec::new();
    for key in 0..total {
        let mut rest = key;
        let mut state = vec![0u16; radices.len()];
        for d in (0..radices.len()).rev() {
            state[d] = (rest % radices[d]) as u16;
            rest /= radices[d];
        }
        let Ok(p) = inv.resolve(&state) else { continue };
        let mut items = Vec::new();
        for i in 0..p.len() {
            let path = &paths[p.geometry(i) as usize];
            let segs: Vec<String> = path
                .segments()
                .iter()
                .map(|s| match *s {
                    Segment::MoveTo(q) => format!("[0,{},{}]", q.x, q.y),
                    Segment::LineTo(q) => format!("[1,{},{}]", q.x, q.y),
                    Segment::QuadTo(a, q) => format!("[2,{},{},{},{}]", a.x, a.y, q.x, q.y),
                    Segment::CubicTo(a, b, q) => format!("[3,{},{},{},{},{},{}]", a.x, a.y, b.x, b.y, q.x, q.y),
                    Segment::Close => "[4]".to_string(),
                })
                .collect();
            let st = p.style(i);
            let fill = st.fill.map_or("null".into(), |f| format!("[{},{},{},{},{}]", f.colour.r, f.colour.g, f.colour.b, f.colour.a, matches!(f.rule, FillRule::EvenOdd) as u8));
            let stroke = st.stroke.map_or("null".into(), |k| format!("[{},{},{},{},{},{}]", k.colour.r, k.colour.g, k.colour.b, k.colour.a, k.width, k.cap as u8));
            let t = p.transform(i);
            items.push(format!(
                "[{},[{}],[{fill},{stroke}],[{},{},{},{},{},{}],{}]",
                p.item(i),
                segs.join(","),
                t.a, t.b, t.c, t.d, t.e, t.f,
                p.opacity(i)
            ));
        }
        states.push(format!("[{key},[{}]]", items.join(",")));
    }
    Ok(format!("[{}]", states.join(",\n")))
}


/// One conformance vector: an inventory and what the reference decoder
/// reports for every reachable state ([`reference_json_for`]). Targets
/// check their runtime decoders against these.
#[derive(Debug, Clone)]
pub struct Vector {
    pub label: String,
    pub bytes: Vec<u8>,
    pub radices: Vec<usize>,
    pub sha256: String,
    pub reference: String,
}

/// A small graphic exercising every effect kind.
pub const STRESS: &str = r#"@ontology stress {
  @graphic badge {
id: "graphic.badge";
canvas: (120, 120);
@shape disc { ellipse: (60, 60, 50, 50); fill: #2b4c7e; stroke: #f4f1ea; stroke-width: 4; }
@shape tick { path: "M 30 62 L 52 84 L 92 40 L 86 34 L 52 72 L 36 56 Z"; fill: #ffffff; }
@shape edge { rect: (10, 10, 100, 100, 12); stroke: #e0a81e; stroke-width: 3; }
@dimension tint {
  @value a { paint: disc #2b4c7e; } @value b { paint: disc #d64550; } @value c { paint: disc #2e9e4f; }
  @value d { paint: disc #8e5cc8; } @value e { paint: disc #f08a24; } @value f { paint: disc #3a3a3a; }
}
@dimension grow {
  step-s: 0.05;
  @value g0 { morph: tick "M 40 60 L 55 75 L 85 45 L 80 40 L 55 66 L 44 56 Z" 0.0; }
  @value g1 { morph: tick "M 40 60 L 55 75 L 85 45 L 80 40 L 55 66 L 44 56 Z" 0.25; scale: edge (0.9, 0.9); }
  @value g2 { morph: tick "M 40 60 L 55 75 L 85 45 L 80 40 L 55 66 L 44 56 Z" 0.5; scale: edge (0.8, 0.8); fade: edge 0.5; }
  @value g3 { morph: tick "M 40 60 L 55 75 L 85 45 L 80 40 L 55 66 L 44 56 Z" 0.75; move: all (3, -2); }
  @value g4 { morph: tick "M 40 60 L 55 75 L 85 45 L 80 40 L 55 66 L 44 56 Z" 1.0; hide: edge; }
}
  }
  @cell root { id: "root"; @cell b { id: "b"; position_p: (0, 0, 0); extent_p: (0.1, 0.1, 0); @draw badge { tint: "a"; grow: "g0"; } } }
}"#;

fn vector(label: String, bytes: Vec<u8>, drawing: &Drawing, model: &StateModel) -> Vector {
    let radices: Vec<usize> = model
        .dimensions
        .iter()
        .map(|d| match &d.domain {
            goya_gvi::Domain::Discrete(v) => v.len(),
            _ => 0,
        })
        .collect();
    let reference = reference_json_for(&bytes, drawing, model, &radices).expect("reference decoder opens its own output");
    Vector { label, sha256: hex(&Sha256::digest(&bytes)), bytes, radices, reference }
}

/// Conformance vectors: both encodings of each `(scene source, graphic
/// name)` given, of [`STRESS`]'s badge, and inventories built through the
/// GVI API that cover what the grammar cannot author: bitmap and packed
/// index keys, GVI-L palettes, record-form `DIFF`, and frame-of-reference
/// and dictionary columns (GVI-O off).
pub fn conformance_vectors(graphics: &[(&str, &str)]) -> Vec<Vector> {
    let parse = |src: &str, name: &str| {
        let parsed = goya_lang::parse_strict_gssh4_live(src).expect("vector scenes are admitted");
        let g = parsed.declarations.graphics.iter().find(|g| g.name == name).expect("named graphic").clone();
        (parsed.scene.name.clone(), g)
    };
    let mut out = Vec::new();
    let mut cases: Vec<(String, GraphicDecl)> = graphics.iter().map(|(s, n)| parse(s, n)).collect();
    cases.push(parse(STRESS, "badge"));
    for (scene, g) in &cases {
        let (drawing, model) = lower(scene, g).expect("vector graphics lower");
        for bake in [false, true] {
            let c = compile_encoding(scene, g, bake).expect("vector graphics compile");
            out.push(vector(format!("{} (bake {bake})", g.name), c.bytes, &drawing, &model));
        }
    }
    let (scene, badge) = parse(STRESS, "badge");
    let (drawing, mut model) = lower(&scene, &badge).expect("badge lowers");
    let mut tint = match &model.dimensions[0].domain {
        goya_gvi::Domain::Discrete(v) => v.clone(),
        _ => Vec::new(),
    };
    // Few colours, many widths: GVI-L's palette beats GVA's records.
    let palette = [(43, 76, 126), (214, 69, 80), (46, 158, 79), (142, 92, 200)];
    for k in 0..60u32 {
        let (r, g, b) = palette[k as usize % palette.len()];
        let style = Style {
            fill: Some(Fill { colour: Rgba8::new(r, g, b, 255), rule: FillRule::NonZero }),
            stroke: Some(Stroke { colour: Rgba8::new(244, 241, 234, 255), width: 1000 + 100 * k, cap: LineCap::Round, join: LineJoin::Round }),
        };
        tint.push(Value::new(&format!("t{k}")).with(Effect::Restyle(Selector::Items(vec![0]), style)));
    }
    model.dimensions[0] = Dimension::discrete("tint", tint);
    // A morph-heavy dimension: differential geometry pays off without GVI-O.
    let swell = |rx: f64, ry: f64| Arc::new(path_from(&geometry_cmds(&Geometry::Ellipse { cx: 60.0, cy: 60.0, rx, ry })).expect("ellipse path"));
    let wave = (0..12u32)
        .map(|k| Value::new(&format!("w{k}")).with(Effect::Morph(Selector::Items(vec![0]), swell(40.0 + 2.0 * f64::from(k), 52.0 - f64::from(k)), 65536 * k / 11)))
        .collect();
    model.dimensions.push(Dimension::discrete("wave", wave));
    // Strided keys in a large domain: packed keys beat a bitmap.
    let sparse: Vec<Vec<(u16, u16)>> = (1..12u16).map(|v| vec![(2, v)]).chain((1..5u16).map(|g| vec![(1, g)])).collect();
    let bitmap: Vec<Vec<(u16, u16)>> = (0..40u16).map(|v| vec![(0, v), (1, v % 5)]).collect();
    for (name, exclusions) in [("product", vec![]), ("bitmap", bitmap), ("packed", sparse)] {
        model.reachability = Reachability::Product { exclusions };
        for operative in [OperativeProfile::Off, OperativeProfile::Fast] {
            let options = CompileOptions { bake: Bake::Transforms, max_diff_depth: DIFF_DEPTH, albit: AlbitProfile::Provenance, operative };
            let bytes = compile_with(&drawing, &model, &Budget::default(), &GvaLimits::default(), options).expect("api vectors compile");
            out.push(vector(format!("api {name} ({operative:?})"), bytes, &drawing, &model));
        }
    }
    out
}
