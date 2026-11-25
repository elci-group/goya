//! goya-gvi — Graphical Vector Inventory (`.gvi`), contract `GY-GVI/0`
//! (draft). Normative text: `docs/graphics/gvi.md`.
//!
//! A GVI is a **parent-scoped, precomputed projection inventory**
//! (GOYA-GFX-001 §8). The compiler takes a source graphic and the parent's
//! declarative state model. It checks KFBM eligibility, enumerates every
//! reachable state, resolves each one's projection at build time, merges
//! identical projections, and records a completeness certificate. At
//! runtime:
//!
//! ```text
//! runtime work = state validation + index lookup (binary search) + presentation
//! ```
//!
//! No transform, opacity or style is evaluated at runtime, and the source
//! SVG or GVA is never re-read. A state outside the domain is rejected
//! (`GFX-E5005`), and so is a valid but unreachable state (`GFX-E5006`).
//! Neither ever falls back silently to another projection.

pub mod model;

pub use model::{
    Assessment, Budget, Dimension, Domain, Effect, Radix, Reachability, Rule, Selector, StateModel,
    Value,
};

use goya_graphics_container::{
    encode as encode_container, Container, Expect, Format, Profile, SectionInput, Tag,
};
use goya_graphics_ir::{
    codes, Affine, Drawing, GfxDiag, Item, Path, Point, Segment, Style, FIXED_ONE,
};
use goya_gva::{
    encode_geometry_pool, encode_style_pool, parse_style_pool, GeometryPool, Gva, GvaLimits,
};
use model::{
    assess_and_enumerate, encode_stat, encode_tran, excluded, parse_stat, parse_tran, step, Reader,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

/// Contract identifier implemented by this crate.
pub const GVI_CONTRACT: &str = "GY-GVI/0";
/// Compiler identity recorded in `PARN` and folded into the cache key.
pub const COMPILER_ID: &str = "goya-gvi 0.0.1 (GY-GVI/0)";

pub const CERT: Tag = Tag::new(b"CERT");
pub const GEOM: Tag = Tag::new(b"GEOM");
pub const HASH: Tag = Tag::new(b"HASH");
pub const INDX: Tag = Tag::new(b"INDX");
pub const PARN: Tag = Tag::new(b"PARN");
pub const PROJ: Tag = Tag::new(b"PROJ");
pub const SRCE: Tag = Tag::new(b"SRCE");
pub const STAT: Tag = Tag::new(b"STAT");
pub const STYL: Tag = Tag::new(b"STYL");
pub const TRAN: Tag = Tag::new(b"TRAN");
const KNOWN: [Tag; 10] = [CERT, GEOM, HASH, INDX, PARN, PROJ, SRCE, STAT, STYL, TRAN];
const ALIGN_LOG2: u8 = 4;

/// `CERT.method`: completeness by product enumeration minus exclusions.
pub const METHOD_PRODUCT: u8 = 0;
/// `CERT.method`: completeness by closure of the transition rules from the
/// initial state.
pub const METHOD_TRANSITION_CLOSURE: u8 = 1;
/// `CERT.assumptions` bit 0: effects are the closed GY-GVI/0 set.
pub const ASSUME_CLOSED_EFFECTS: u32 = 1;
/// `CERT.assumptions` bit 1: unreachable states are rejected at runtime.
pub const ASSUME_REJECT_UNREACHABLE: u32 = 2;
/// `CERT.assumptions` bit 2: transforms were baked into geometry (§5).
pub const ASSUME_BAKED_TRANSFORMS: u32 = 4;
const CERT_LEN: usize = 80;

fn diag(code: u16, detail: impl Into<String>) -> GfxDiag {
    GfxDiag::new(code, detail)
}

fn cert_err(detail: impl Into<String>) -> GfxDiag {
    diag(codes::GVI_CERTIFICATE_INVALID, detail)
}

fn malformed(tag: Tag, detail: impl std::fmt::Display) -> GfxDiag {
    diag(codes::MALFORMED_SECTION, format!("{tag}: {detail}"))
}

fn pad16(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

// ---------------------------------------------------------------------------
// Parent binding and cache identity (directive §8.6)
// ---------------------------------------------------------------------------

/// The digest that binds an inventory to its parent's graphical semantics:
/// SHA-256 over the parent id and the canonical `STAT` and `TRAN` payloads.
/// A consumer must present the binding it expects before an inventory will
/// open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParentBinding(pub [u8; 32]);

fn binding_digest(parent: &str, stat: &[u8], tran: &[u8]) -> [u8; 32] {
    sha256(&[
        b"GY-GVI/0 parent binding\0",
        &(parent.len() as u64).to_le_bytes(),
        parent.as_bytes(),
        &(stat.len() as u64).to_le_bytes(),
        stat,
        &(tran.len() as u64).to_le_bytes(),
        tran,
    ])
}

impl StateModel {
    /// The parent binding of a valid, finite model.
    pub fn binding(&self, source_items: u32, budget: &Budget) -> Result<ParentBinding, GfxDiag> {
        self.validate(source_items, budget)?;
        Ok(ParentBinding(binding_digest(
            &self.parent,
            &encode_stat(self),
            &encode_tran(self),
        )))
    }
}

/// KFBM assessment without compiling (for diagnostics and Quinta-style
/// size forecasting).
pub fn assess(model: &StateModel, budget: &Budget) -> Assessment {
    assess_and_enumerate(model, budget).0
}

/// Whether transforms are baked into projection geometry (gvi.md §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bake {
    /// Rows keep their full transform; presentation applies it.
    None,
    /// Each row's linear transform is applied to its geometry at build
    /// time, so the row carries only a translation. Strokes bake only
    /// under similarity transforms (uniform scale, rotation, reflection),
    /// which scale the width exactly; other stroked rows keep their
    /// transform.
    Transforms,
}

impl Bake {
    fn id(self) -> u8 {
        match self {
            Bake::None => 0,
            Bake::Transforms => 1,
        }
    }
}

/// The build cache key (directive §8.6): source digest, parent binding,
/// state schema (inside the binding), compilation profile, bake policy and
/// compiler version. If any of them changes, the inventory must be
/// regenerated.
pub fn cache_key(
    source: &Drawing,
    model: &StateModel,
    budget: &Budget,
    gva_limits: &GvaLimits,
    bake: Bake,
) -> Result<[u8; 32], GfxDiag> {
    let gva = goya_gva::encode(source, gva_limits)?;
    let binding = model.binding(source.items.len() as u32, budget)?;
    Ok(sha256(&[
        COMPILER_ID.as_bytes(),
        &Profile::Baseline.id().to_le_bytes(),
        &[bake.id()],
        &sha256(&[&gva]),
        &binding.0,
    ]))
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

/// A working instance row while projecting.
#[derive(Clone)]
struct Row {
    item: u32,
    path: Arc<Path>,
    /// Canonical minus source first point: converts morph targets, which
    /// are authored in the source item's frame, into the canonical frame.
    delta: (i32, i32),
    style: Style,
    transform: Affine,
    opacity: u8,
    hidden: bool,
}

/// An encoded projection row: item, geometry, style index, transform,
/// opacity.
type ProjRow = (u32, u32, u32, Affine, u8);

/// `round_half_even(v / 65536)`.
fn round_q16(v: i128) -> i128 {
    let q = v.div_euclid(65536);
    match v.rem_euclid(65536).cmp(&32768) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
    }
}

/// Rebuild `path` with every stored point replaced by `f(index, point)`.
fn map_points(path: &Path, mut f: impl FnMut(usize, Point) -> Option<Point>) -> Option<Path> {
    let mut i = 0;
    let mut next = |p: Point| {
        let q = f(i, p);
        i += 1;
        q
    };
    let mut segs = Vec::with_capacity(path.segments().len());
    for s in path.segments() {
        segs.push(match *s {
            Segment::MoveTo(p) => Segment::MoveTo(next(p)?),
            Segment::LineTo(p) => Segment::LineTo(next(p)?),
            Segment::QuadTo(c, p) => Segment::QuadTo(next(c)?, next(p)?),
            Segment::CubicTo(c1, c2, p) => Segment::CubicTo(next(c1)?, next(c2)?, next(p)?),
            Segment::Close => Segment::Close,
        });
    }
    Path::new(segs).ok()
}

/// Interpolate `from` toward `to` at `t / 65536` (gvi.md §1, `Morph`):
/// `p + round_half_even((q − p) · t / 65536)` per stored point. The paths
/// must have identical segment structure; returns `None` otherwise or on
/// overflow. Integer-exact, so build and runtime agree bit for bit.
pub fn morph_path(from: &Path, to: &Path, t: u32) -> Option<Path> {
    let same_shape = from.segments().len() == to.segments().len()
        && from
            .segments()
            .iter()
            .zip(to.segments())
            .all(|(a, b)| std::mem::discriminant(a) == std::mem::discriminant(b));
    if !same_shape || t > 65536 {
        return None;
    }
    let targets: Vec<Point> = to.points().collect();
    let t = i128::from(t);
    map_points(from, |i, p| {
        let q = targets[i];
        let lerp = |a: i32, b: i32| {
            let v = i128::from(a) + round_q16((i128::from(b) - i128::from(a)) * t);
            i32::try_from(v).ok()
        };
        Some(Point::new(lerp(p.x, q.x)?, lerp(p.y, q.y)?))
    })
}

/// Bake the linear part of `t` into `path` (§5). Returns the new geometry,
/// normalised so its first point is the origin, the residual translation,
/// and the style (stroke width scaled). `None` if baking is not exact
/// enough or not needed.
fn bake_row(path: &Path, t: &Affine, style: &Style) -> Option<(Path, Affine, Style)> {
    if (t.a, t.b, t.c, t.d) == (FIXED_ONE, 0, 0, FIXED_ONE) {
        return None;
    }
    let mut style = *style;
    if let Some(stroke) = &mut style.stroke {
        let similarity = (t.a == t.d && t.b == -t.c) || (t.a == -t.d && t.b == t.c);
        if !similarity {
            return None;
        }
        let (a, b) = (f64::from(t.a), f64::from(t.b));
        let w = (f64::from(stroke.width) * (a * a + b * b).sqrt() / f64::from(FIXED_ONE))
            .round_ties_even();
        if !(1.0..=f64::from(u32::MAX)).contains(&w) {
            return None;
        }
        stroke.width = w as u32;
    }
    let image = map_points(path, |_, p| {
        let (x, y) = t.apply_scaled(p);
        Some(Point::new(
            i32::try_from(round_q16(x)).ok()?,
            i32::try_from(round_q16(y)).ok()?,
        ))
    })?;
    let o = image.first_point();
    let geometry = image.translated(o.x.checked_neg()?, o.y.checked_neg()?)?;
    Some((geometry, Affine::translate(o.x, o.y), style))
}

fn apply(rows: &mut [Row], effects: &[Effect]) -> Result<(), GfxDiag> {
    for e in effects {
        for r in rows.iter_mut() {
            match e {
                Effect::Transform(sel, t) if sel.matches(r.item) => {
                    r.transform = r.transform.then(t).ok_or_else(|| {
                        diag(
                            codes::COORDINATE_OVERFLOW,
                            format!("transform effect overflows on item {}", r.item),
                        )
                    })?;
                }
                Effect::Opacity(sel, f) if sel.matches(r.item) => {
                    r.opacity = ((u32::from(r.opacity) * u32::from(*f) + 127) / 255) as u8;
                }
                Effect::Restyle(sel, s) if sel.matches(r.item) => r.style = *s,
                Effect::Hide(sel) if sel.matches(r.item) => r.hidden = true,
                Effect::Morph(sel, target, t) if sel.matches(r.item) => {
                    let mismatch = || {
                        diag(
                            codes::GVI_INVALID_MODEL,
                            format!(
                                "morph target for item {} does not match its geometry's segment structure",
                                r.item
                            ),
                        )
                    };
                    let target = target
                        .translated(r.delta.0, r.delta.1)
                        .ok_or_else(mismatch)?;
                    r.path = Arc::new(morph_path(&r.path, &target, *t).ok_or_else(mismatch)?);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn not_manageable(detail: String) -> GfxDiag {
    diag(
        codes::GVI_NOT_MANAGEABLE,
        format!(
            "not Manageable ({detail}); use SPC if approximation is permitted, otherwise GRU/GVA"
        ),
    )
}

/// Compile a parent-bound inventory without baking. See [`compile_with`].
pub fn compile(
    source: &Drawing,
    model: &StateModel,
    budget: &Budget,
    gva_limits: &GvaLimits,
) -> Result<Vec<u8>, GfxDiag> {
    compile_with(source, model, budget, gva_limits, Bake::None)
}

/// Compile a parent-bound inventory. Fails with an E50xx diagnostic, and a
/// recommended alternative representation, when the model is not
/// KFBM-eligible. A partial inventory is never produced.
pub fn compile_with(
    source: &Drawing,
    model: &StateModel,
    budget: &Budget,
    gva_limits: &GvaLimits,
    bake: Bake,
) -> Result<Vec<u8>, GfxDiag> {
    let (assessment, keys) = assess_and_enumerate(model, budget);
    assessment.verdict()?;
    let keys = keys.ok_or_else(|| not_manageable("enumeration did not complete".into()))?;
    let items = u32::try_from(source.items.len())
        .map_err(|_| diag(codes::TOO_MANY_INSTANCES, "too many source items"))?;
    model.validate(items, budget)?;
    let radix = model
        .radix()
        .ok_or_else(|| not_manageable("domain overflows u64".into()))?;

    // Canonical source: GVA's normalised geometry and pools.
    let gva_bytes = goya_gva::encode(source, gva_limits)?;
    let gva = Gva::open(&gva_bytes, gva_limits)?;
    let paths: Vec<Arc<Path>> = (0..gva.geometry_count())
        .map(|g| gva.geometry(g).map(Arc::new))
        .collect::<Result<_, _>>()?;
    let mut pool: Vec<Path> = paths.iter().map(|p| (**p).clone()).collect();
    let mut pool_index: HashMap<Path, u32> = pool
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), i as u32))
        .collect();
    let mut styles: Vec<Style> = gva.styles().to_vec();
    let mut style_index: HashMap<Style, u32> = styles
        .iter()
        .enumerate()
        .map(|(i, s)| (*s, i as u32))
        .collect();
    let v = gva.instances();
    let mut base = Vec::with_capacity(gva.instance_count());
    for i in 0..gva.instance_count() {
        let path = paths[v.geometry.get(i).unwrap_or(0) as usize].clone();
        let (c, s) = (path.first_point(), source.items[i].path.first_point());
        let delta = (
            i32::try_from(i64::from(c.x) - i64::from(s.x)),
            i32::try_from(i64::from(c.y) - i64::from(s.y)),
        );
        let (Ok(dx), Ok(dy)) = delta else {
            return Err(diag(
                codes::COORDINATE_OVERFLOW,
                "source item is out of range",
            ));
        };
        base.push(Row {
            item: i as u32,
            path,
            delta: (dx, dy),
            style: styles[v.style.get(i).unwrap_or(0) as usize],
            transform: gva.transform(i).unwrap_or(Affine::IDENTITY),
            opacity: v.opacity[i],
            hidden: false,
        });
    }

    // Resolve every reachable state.
    let dims: Vec<&[Value]> = model
        .dimensions
        .iter()
        .map(|d| match &d.domain {
            Domain::Discrete(v) => v.as_slice(),
            _ => &[],
        })
        .collect();
    let mut projections: Vec<Vec<ProjRow>> = Vec::new();
    let mut proj_index: HashMap<Vec<ProjRow>, u32> = HashMap::new();
    let mut state_proj: Vec<u32> = Vec::with_capacity(keys.len());
    let mut total_rows = 0u64;
    for &key in &keys {
        let state = radix.unpack(key);
        let mut rows = base.clone();
        for (d, &val) in state.iter().enumerate() {
            apply(&mut rows, &dims[d][val as usize].effects)?;
        }
        let mut proj = Vec::with_capacity(rows.len());
        for r in rows.iter().filter(|r| !r.hidden) {
            let baked = match bake {
                Bake::None => None,
                Bake::Transforms => bake_row(&r.path, &r.transform, &r.style),
            };
            let (path, transform, style) = match &baked {
                Some((p, t, s)) => (p, *t, *s),
                None => (&*r.path, r.transform, r.style),
            };
            let g = match pool_index.get(path) {
                Some(&g) => g,
                None => {
                    let g = pool.len() as u32;
                    if g >= gva_limits.max_geometries {
                        return Err(diag(
                            codes::TOO_MANY_GEOMETRIES,
                            "projections need too many distinct geometries",
                        ));
                    }
                    pool_index.insert(path.clone(), g);
                    pool.push(path.clone());
                    g
                }
            };
            let s = match style_index.get(&style) {
                Some(&s) => s,
                None => {
                    let s = styles.len() as u32;
                    if s >= gva_limits.max_styles {
                        return Err(diag(
                            codes::TOO_MANY_STYLES,
                            "projections need too many distinct styles",
                        ));
                    }
                    style_index.insert(style, s);
                    styles.push(style);
                    s
                }
            };
            proj.push((r.item, g, s, transform, r.opacity));
        }
        let id = match proj_index.get(&proj) {
            Some(&id) => id,
            None => {
                total_rows += proj.len() as u64;
                if total_rows > budget.max_rows {
                    return Err(not_manageable(format!(
                        "more than {} projection rows",
                        budget.max_rows
                    )));
                }
                let id = projections.len() as u32;
                proj_index.insert(proj.clone(), id);
                projections.push(proj);
                id
            }
        };
        state_proj.push(id);
    }

    // Sections.
    let stat = encode_stat(model);
    let tran = encode_tran(model);
    let binding = binding_digest(&model.parent, &stat, &tran);

    let mut parn = Vec::new();
    parn.extend_from_slice(&binding);
    parn.extend_from_slice(&(model.parent.len() as u32).to_le_bytes());
    parn.extend_from_slice(&(COMPILER_ID.len() as u32).to_le_bytes());
    parn.extend_from_slice(&[0u8; 8]);
    parn.extend_from_slice(model.parent.as_bytes());
    parn.extend_from_slice(COMPILER_ID.as_bytes());
    pad16(&mut parn);

    let mut srce = Vec::with_capacity(48);
    srce.extend_from_slice(&sha256(&[&gva_bytes]));
    srce.extend_from_slice(&items.to_le_bytes());
    srce.extend_from_slice(&(paths.len() as u32).to_le_bytes());
    srce.extend_from_slice(&source.width.to_le_bytes());
    srce.extend_from_slice(&source.height.to_le_bytes());

    let n = keys.len();
    let mut indx = Vec::new();
    indx.extend_from_slice(&(n as u32).to_le_bytes());
    indx.extend_from_slice(&[0u8; 12]);
    let keys_start = indx.len();
    for k in &keys {
        indx.extend_from_slice(&k.to_le_bytes());
    }
    let keys_bytes = indx[keys_start..].to_vec();
    pad16(&mut indx);
    for p in &state_proj {
        indx.extend_from_slice(&p.to_le_bytes());
    }
    pad16(&mut indx);

    let total = total_rows as u32;
    let mut proj = Vec::new();
    proj.extend_from_slice(&(projections.len() as u32).to_le_bytes());
    proj.extend_from_slice(&total.to_le_bytes());
    proj.extend_from_slice(&[0u8; 8]);
    let mut start = 0u32;
    proj.extend_from_slice(&start.to_le_bytes());
    for p in &projections {
        start += p.len() as u32;
        proj.extend_from_slice(&start.to_le_bytes());
    }
    pad16(&mut proj);
    let all_rows: Vec<&ProjRow> = projections.iter().flatten().collect();
    for field in 0..9 {
        for r in &all_rows {
            match field {
                0 => proj.extend_from_slice(&r.0.to_le_bytes()),
                1 => proj.extend_from_slice(&r.1.to_le_bytes()),
                2 => proj.extend_from_slice(&r.2.to_le_bytes()),
                3 => proj.extend_from_slice(&r.3.e.to_le_bytes()),
                4 => proj.extend_from_slice(&r.3.f.to_le_bytes()),
                5 => proj.extend_from_slice(&r.3.a.to_le_bytes()),
                6 => proj.extend_from_slice(&r.3.b.to_le_bytes()),
                7 => proj.extend_from_slice(&r.3.c.to_le_bytes()),
                _ => proj.extend_from_slice(&r.3.d.to_le_bytes()),
            }
        }
        pad16(&mut proj);
    }
    proj.extend(all_rows.iter().map(|r| r.4));
    pad16(&mut proj);

    let method = match model.reachability {
        Reachability::Product { .. } => METHOD_PRODUCT,
        Reachability::Transitions { .. } => METHOD_TRANSITION_CLOSURE,
    };
    let bake_flag = match bake {
        Bake::None => 0,
        Bake::Transforms => ASSUME_BAKED_TRANSFORMS,
    };
    let mut cert = vec![0u8; CERT_LEN];
    cert[0] = method;
    cert[4..8].copy_from_slice(&(model.dimensions.len() as u32).to_le_bytes());
    cert[8..16].copy_from_slice(&radix.domain.to_le_bytes());
    cert[16..24].copy_from_slice(&(n as u64).to_le_bytes());
    cert[24..32].copy_from_slice(&(projections.len() as u64).to_le_bytes());
    cert[32..40].copy_from_slice(&((n - projections.len()) as u64).to_le_bytes());
    cert[40..72].copy_from_slice(&sha256(&[&keys_bytes]));
    cert[72..76].copy_from_slice(
        &(ASSUME_CLOSED_EFFECTS | ASSUME_REJECT_UNREACHABLE | bake_flag).to_le_bytes(),
    );

    let (geom, hash) = encode_geometry_pool(&pool)?;
    let styl = encode_style_pool(&styles);
    let sections = [
        SectionInput::new(CERT, &cert).align_log2(ALIGN_LOG2),
        SectionInput::new(GEOM, &geom).align_log2(ALIGN_LOG2),
        SectionInput::new(HASH, &hash)
            .align_log2(ALIGN_LOG2)
            .optional(),
        SectionInput::new(INDX, &indx).align_log2(ALIGN_LOG2),
        SectionInput::new(PARN, &parn).align_log2(ALIGN_LOG2),
        SectionInput::new(PROJ, &proj).align_log2(ALIGN_LOG2),
        SectionInput::new(SRCE, &srce).align_log2(ALIGN_LOG2),
        SectionInput::new(STAT, &stat).align_log2(ALIGN_LOG2),
        SectionInput::new(STYL, &styl).align_log2(ALIGN_LOG2),
        SectionInput::new(TRAN, &tran).align_log2(ALIGN_LOG2),
    ];
    let out = encode_container(
        Format::Gvi,
        Profile::Baseline,
        0,
        &sections,
        &gva_limits.container,
    )
    .map_err(|e| diag(e.kind.code(), e.to_string()))?;
    if out.len() as u64 > budget.max_bytes {
        return Err(not_manageable(format!(
            "inventory is {} bytes, over the {}-byte budget",
            out.len(),
            budget.max_bytes
        )));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// Limits enforced when opening an inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GviLimits {
    pub gva: GvaLimits,
    pub max_dimensions: usize,
    pub max_states: u32,
    pub max_projections: u32,
    pub max_rows: u32,
    /// Rule applications allowed while re-checking completeness at open.
    pub max_check_steps: u64,
}

impl Default for GviLimits {
    fn default() -> GviLimits {
        GviLimits {
            gva: GvaLimits::default(),
            max_dimensions: 16,
            max_states: 1 << 20,
            max_projections: 1 << 20,
            max_rows: 1 << 24,
            max_check_steps: 1 << 24,
        }
    }
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn u64_at(b: &[u8], off: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[off..off + 8]);
    u64::from_le_bytes(a)
}

/// The decoded certificate (directive §8.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    pub method: u8,
    pub dimensions: u32,
    pub domain_size: u64,
    pub reachable: u64,
    pub projections: u64,
    /// States that share a projection with an earlier state.
    pub merged_states: u64,
    /// SHA-256 of the sorted reachable key list.
    pub coverage_digest: [u8; 32],
    pub assumptions: u32,
}

/// A validated, parent-verified, certificate-checked inventory.
#[derive(Debug, Clone)]
pub struct Inventory<'a> {
    bytes: &'a [u8],
    pub parent: String,
    pub compiler: String,
    pub binding: ParentBinding,
    pub source_digest: [u8; 32],
    pub source_items: u32,
    pub width: u32,
    pub height: u32,
    pub certificate: Certificate,
    dims: Vec<Dimension>,
    reach: Reachability,
    radix: Radix,
    keys: &'a [u8],
    key_proj: &'a [u8],
    starts: &'a [u8],
    rows: [&'a [u8]; 9],
    opacity: &'a [u8],
    geom: GeometryPool<'a>,
    styles: Vec<Style>,
}

/// One resolved projection: a zero-copy slice of the row arrays.
#[derive(Debug, Clone, Copy)]
pub struct Projection<'i, 'a> {
    inv: &'i Inventory<'a>,
    pub id: u32,
    start: usize,
    end: usize,
}

impl<'a> Inventory<'a> {
    /// Open and fully validate an inventory for the parent whose binding is
    /// `expected`. This checks the container, every section, every
    /// reference, the parent binding, the certificate's figures, and
    /// completeness, re-derived independently from `STAT`/`TRAN` by
    /// closure or product checking. Only then can the inventory be used.
    pub fn open(
        bytes: &'a [u8],
        expected: &ParentBinding,
        limits: &GviLimits,
    ) -> Result<Inventory<'a>, GfxDiag> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gvi,
                known_sections: &KNOWN,
            },
            &limits.gva.container,
        )
        .map_err(|e| diag(e.kind.code(), e.to_string()))?;
        let take = |tag: Tag| -> Result<&'a [u8], GfxDiag> {
            c.section(tag)
                .map_err(|e| diag(e.kind.code(), e.to_string()))?
                .ok_or_else(|| malformed(tag, "required section is missing"))
        };

        // PARN, STAT, TRAN: binding first, before trusting anything else.
        let parn = take(PARN)?;
        let mut r = Reader::new(parn, "PARN");
        let mut binding = [0u8; 32];
        binding.copy_from_slice(r.bytes(32)?);
        if ParentBinding(binding) != *expected {
            return Err(diag(
                codes::GVI_PARENT_MISMATCH,
                "inventory was compiled for different parent semantics; regenerate it",
            ));
        }
        let plen = r.u32()? as usize;
        let clen = r.u32()? as usize;
        r.zeros(8)?;
        let parent = String::from_utf8(r.bytes(plen)?.to_vec())
            .map_err(|_| malformed(PARN, "parent id is not UTF-8"))?;
        let compiler = String::from_utf8(r.bytes(clen)?.to_vec())
            .map_err(|_| malformed(PARN, "compiler id is not UTF-8"))?;
        r.finish()?;
        let stat = take(STAT)?;
        let tran = take(TRAN)?;
        if binding_digest(&parent, stat, tran) != binding {
            return Err(cert_err(
                "stored state model does not match its parent binding",
            ));
        }
        let dims = parse_stat(stat, limits.max_dimensions, &limits.gva)?;
        let reach = parse_tran(tran, dims.len())?;

        // SRCE
        let srce = take(SRCE)?;
        if srce.len() != 48 {
            return Err(malformed(
                SRCE,
                format!("{} bytes, expected 48", srce.len()),
            ));
        }
        let mut source_digest = [0u8; 32];
        source_digest.copy_from_slice(&srce[..32]);
        let source_items = u32_at(srce, 32);
        let source_geoms = u32_at(srce, 36);

        let model = StateModel {
            parent: parent.clone(),
            dimensions: dims.clone(),
            reachability: reach.clone(),
        };
        let budget = Budget {
            max_dimensions: limits.max_dimensions,
            max_values_per_dimension: u16::MAX as usize - 1,
            ..Budget::default()
        };
        model.validate(source_items, &budget)?;
        let radix = model
            .radix()
            .ok_or_else(|| cert_err("state domain overflows u64"))?;

        // Pools
        let geom = GeometryPool::parse(take(GEOM)?, &limits.gva)?;
        if geom.len() < source_geoms as usize {
            return Err(diag(
                codes::COUNT_MISMATCH,
                "GEOM holds fewer geometries than the source",
            ));
        }
        let styles = parse_style_pool(take(STYL)?, &limits.gva)?;

        // INDX
        let indx = take(INDX)?;
        let mut r = Reader::new(indx, "INDX");
        let n = r.u32()?;
        r.zeros(12)?;
        if n > limits.max_states {
            return Err(diag(
                codes::GVI_NOT_MANAGEABLE,
                format!("{n} states exceeds the limit of {}", limits.max_states),
            ));
        }
        let n_us = n as usize;
        let keys = r.bytes(n_us * 8)?;
        r.zeros((16 - (n_us * 8) % 16) % 16)?;
        let key_proj = r.bytes(n_us * 4)?;
        r.finish()?;

        // PROJ
        let projb = take(PROJ)?;
        let mut r = Reader::new(projb, "PROJ");
        let np = r.u32()?;
        let total = r.u32()?;
        r.zeros(8)?;
        if np > limits.max_projections || total > limits.max_rows {
            return Err(diag(
                codes::GVI_NOT_MANAGEABLE,
                "projection table exceeds the open limits",
            ));
        }
        let (np_us, total_us) = (np as usize, total as usize);
        let starts_len = (np_us + 1) * 4;
        let starts = r.bytes(starts_len)?;
        r.zeros((16 - starts_len % 16) % 16)?;
        let arr_pad = (16 - (total_us * 4) % 16) % 16;
        let mut rows: [&[u8]; 9] = [&[]; 9];
        for slot in rows.iter_mut() {
            *slot = r.bytes(total_us * 4)?;
            r.zeros(arr_pad)?;
        }
        let opacity = r.bytes(total_us)?;
        r.finish()?;

        let inv = Inventory {
            bytes,
            parent,
            compiler,
            binding: ParentBinding(binding),
            source_digest,
            source_items,
            width: u32_at(srce, 40),
            height: u32_at(srce, 44),
            certificate: parse_cert(take(CERT)?)?,
            dims,
            reach,
            radix,
            keys,
            key_proj,
            starts,
            rows,
            opacity,
            geom,
            styles,
        };
        inv.check_structure(np, total)?;
        inv.check_completeness(limits)?;
        Ok(inv)
    }

    fn key(&self, i: usize) -> u64 {
        u64_at(self.keys, i * 8)
    }

    fn proj_of(&self, i: usize) -> u32 {
        u32_at(self.key_proj, i * 4)
    }

    fn start(&self, p: usize) -> u32 {
        u32_at(self.starts, p * 4)
    }

    fn row_u32(&self, field: usize, row: usize) -> u32 {
        u32_at(self.rows[field], row * 4)
    }

    pub fn state_count(&self) -> usize {
        self.keys.len() / 8
    }

    pub fn projection_count(&self) -> usize {
        self.starts.len() / 4 - 1
    }

    pub fn dimensions(&self) -> &[Dimension] {
        &self.dims
    }

    pub fn reachability(&self) -> &Reachability {
        &self.reach
    }

    /// Index, projection and row references, plus the certificate figures.
    fn check_structure(&self, np: u32, total: u32) -> Result<(), GfxDiag> {
        let n = self.state_count();
        let mut prev: Option<u64> = None;
        let mut used = vec![false; np as usize];
        for i in 0..n {
            let k = self.key(i);
            if prev.is_some_and(|p| p >= k) || k >= self.radix.domain {
                return Err(cert_err(
                    "index keys must be strictly ascending and inside the domain",
                ));
            }
            prev = Some(k);
            let p = self.proj_of(i);
            let slot = used.get_mut(p as usize).ok_or_else(|| {
                diag(
                    codes::GEOMETRY_REF,
                    format!("state {i} references missing projection {p}"),
                )
            })?;
            *slot = true;
        }
        if used.iter().any(|u| !u) {
            return Err(cert_err("a projection is not referenced by any state"));
        }
        if self.start(0) != 0 || self.start(np as usize) != total {
            return Err(malformed(
                PROJ,
                "projection ranges do not cover the row arrays",
            ));
        }
        for p in 0..np as usize {
            if self.start(p) > self.start(p + 1) {
                return Err(malformed(PROJ, "projection ranges must not decrease"));
            }
        }
        let (gc, sc) = (self.geom.len() as u32, self.styles.len() as u32);
        for row in 0..total as usize {
            if self.row_u32(0, row) >= self.source_items {
                return Err(diag(
                    codes::GEOMETRY_REF,
                    "projection row names a missing source item",
                ));
            }
            if self.row_u32(1, row) >= gc {
                return Err(diag(
                    codes::GEOMETRY_REF,
                    "projection row references a missing geometry",
                ));
            }
            if self.row_u32(2, row) >= sc {
                return Err(diag(
                    codes::STYLE_REF,
                    "projection row references a missing style",
                ));
            }
        }
        let c = &self.certificate;
        let required = ASSUME_CLOSED_EFFECTS | ASSUME_REJECT_UNREACHABLE;
        let known = required | ASSUME_BAKED_TRANSFORMS;
        if c.assumptions & required != required || c.assumptions & !known != 0 {
            return Err(cert_err("unknown or missing certificate assumptions"));
        }
        let method = match self.reach {
            Reachability::Product { .. } => METHOD_PRODUCT,
            Reachability::Transitions { .. } => METHOD_TRANSITION_CLOSURE,
        };
        if c.method != method
            || c.dimensions as usize != self.dims.len()
            || c.domain_size != self.radix.domain
            || c.reachable != n as u64
            || c.projections != u64::from(np)
            || c.merged_states != (n as u64).saturating_sub(u64::from(np))
            || c.coverage_digest != sha256(&[self.keys])
        {
            return Err(cert_err("certificate figures do not match the inventory"));
        }
        Ok(())
    }

    fn contains(&self, key: u64) -> bool {
        self.index_of(key).is_some()
    }

    fn index_of(&self, key: u64) -> Option<usize> {
        let (mut lo, mut hi) = (0usize, self.state_count());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.key(mid).cmp(&key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid),
            }
        }
        None
    }

    /// Re-derive completeness from the stored model, using a different
    /// argument from the compiler's enumeration:
    ///
    /// - **Product:** every index key avoids every exclusion, and the count
    ///   equals the domain size minus the excluded combinations.
    /// - **Transitions:** the initial state is indexed, the index is closed
    ///   under every rule, and every indexed state is reached by walking
    ///   rules from the initial state within the index.
    fn check_completeness(&self, limits: &GviLimits) -> Result<(), GfxDiag> {
        let n = self.state_count();
        match &self.reach {
            Reachability::Product { exclusions } => {
                if self.radix.domain > limits.max_check_steps {
                    return Err(cert_err("domain too large to check"));
                }
                let mut expected = 0u64;
                for key in 0..self.radix.domain {
                    let ex = excluded(exclusions, &self.radix.unpack(key));
                    let present = self.contains(key);
                    if ex == present {
                        return Err(cert_err(if ex {
                            format!("excluded state {key} is indexed")
                        } else {
                            format!("reachable state {key} is missing from the index")
                        }));
                    }
                    expected += u64::from(!ex);
                }
                if expected != n as u64 {
                    return Err(cert_err("index size does not match the product"));
                }
            }
            Reachability::Transitions { initial, rules } => {
                let start = self
                    .radix
                    .pack(initial)
                    .ok_or_else(|| cert_err("initial state is outside the domain"))?;
                let Some(start_i) = self.index_of(start) else {
                    return Err(cert_err("initial state is missing from the index"));
                };
                let mut visited = vec![false; n];
                visited[start_i] = true;
                let mut stack = vec![start_i];
                let mut steps = 0u64;
                while let Some(i) = stack.pop() {
                    let state = self.radix.unpack(self.key(i));
                    for rule in rules {
                        steps += 1;
                        if steps > limits.max_check_steps {
                            return Err(cert_err("completeness check exceeds its step budget"));
                        }
                        if let Some(next) = step(rule, &state) {
                            let k = self
                                .radix
                                .pack(&next)
                                .ok_or_else(|| cert_err("rule leaves the domain"))?;
                            let j = self.index_of(k).ok_or_else(|| {
                                cert_err(format!("reachable state {k} is missing from the index"))
                            })?;
                            if !visited[j] {
                                visited[j] = true;
                                stack.push(j);
                            }
                        }
                    }
                }
                if visited.iter().any(|v| !v) {
                    return Err(cert_err("the index holds a state that is not reachable"));
                }
            }
        }
        Ok(())
    }

    /// Build a state from `(dimension, label)` pairs. Every dimension must
    /// be given exactly once.
    pub fn state(&self, assignments: &[(&str, &str)]) -> Result<Vec<u16>, GfxDiag> {
        let invalid = |d: String| diag(codes::GVI_INVALID_STATE, d);
        let mut state = vec![None; self.dims.len()];
        for &(name, label) in assignments {
            let d = self
                .dims
                .iter()
                .position(|d| d.name == name)
                .ok_or_else(|| invalid(format!("no dimension {name:?}")))?;
            let Domain::Discrete(values) = &self.dims[d].domain else {
                return Err(invalid(format!("{name} is not discrete")));
            };
            let v = values
                .iter()
                .position(|v| v.label == label)
                .ok_or_else(|| invalid(format!("{name} has no value {label:?}")))?;
            if state[d].replace(v as u16).is_some() {
                return Err(invalid(format!("{name} is assigned twice")));
            }
        }
        state
            .into_iter()
            .enumerate()
            .map(|(d, v)| {
                v.ok_or_else(|| invalid(format!("{} is not assigned", self.dims[d].name)))
            })
            .collect()
    }

    /// Resolve the precomputed projection for `state`. The work is state
    /// validation plus a binary search; nothing is recomputed.
    pub fn resolve(&self, state: &[u16]) -> Result<Projection<'_, 'a>, GfxDiag> {
        let key = self.radix.pack(state).ok_or_else(|| {
            diag(
                codes::GVI_INVALID_STATE,
                format!("state {state:?} is outside the declared domain"),
            )
        })?;
        let i = self.index_of(key).ok_or_else(|| {
            diag(
                codes::GVI_UNREACHABLE_STATE,
                format!("state {state:?} is valid but not reachable under the parent's model; it has no projection"),
            )
        })?;
        let id = self.proj_of(i);
        Ok(Projection {
            inv: self,
            id,
            start: self.start(id as usize) as usize,
            end: self.start(id as usize + 1) as usize,
        })
    }

    /// Whether the parent's model permits changing directly from `from` to
    /// `to`. Both states must be reachable.
    pub fn permits(&self, from: &[u16], to: &[u16]) -> Result<bool, GfxDiag> {
        self.resolve(from)?;
        self.resolve(to)?;
        Ok(match &self.reach {
            Reachability::Product { .. } => true,
            Reachability::Transitions { rules, .. } => {
                rules.iter().any(|r| step(r, from).as_deref() == Some(to))
            }
        })
    }

    /// Decode every geometry once, for presentation.
    pub fn decode_geometry(&self) -> Result<Vec<Arc<Path>>, GfxDiag> {
        (0..self.geom.len())
            .map(|g| self.geom.get(g).map(Arc::new))
            .collect()
    }

    pub fn styles(&self) -> &[Style] {
        &self.styles
    }

    /// Check that this inventory is exactly what compiling `source` under
    /// `model` produces.
    pub fn verify_against(
        &self,
        source: &Drawing,
        model: &StateModel,
        budget: &Budget,
        gva_limits: &GvaLimits,
    ) -> Result<(), GfxDiag> {
        let gva = goya_gva::encode(source, gva_limits)?;
        if sha256(&[&gva]) != self.source_digest {
            return Err(diag(
                codes::GVI_SOURCE_MISMATCH,
                "inventory was compiled from a different source",
            ));
        }
        if model.binding(source.items.len() as u32, budget)? != self.binding {
            return Err(diag(
                codes::GVI_PARENT_MISMATCH,
                "inventory was compiled for a different model",
            ));
        }
        let bake = if self.certificate.assumptions & ASSUME_BAKED_TRANSFORMS != 0 {
            Bake::Transforms
        } else {
            Bake::None
        };
        if compile_with(source, model, budget, gva_limits, bake)? != self.bytes {
            return Err(diag(
                codes::NON_CANONICAL,
                "inventory differs from the canonical compilation",
            ));
        }
        Ok(())
    }
}

fn parse_cert(b: &[u8]) -> Result<Certificate, GfxDiag> {
    if b.len() != CERT_LEN || b[1..4] != [0; 3] || b[76..80] != [0; 4] {
        return Err(malformed(CERT, "wrong length or non-zero reserved bytes"));
    }
    let mut coverage_digest = [0u8; 32];
    coverage_digest.copy_from_slice(&b[40..72]);
    Ok(Certificate {
        method: b[0],
        dimensions: u32_at(b, 4),
        domain_size: u64_at(b, 8),
        reachable: u64_at(b, 16),
        projections: u64_at(b, 24),
        merged_states: u64_at(b, 32),
        coverage_digest,
        assumptions: u32_at(b, 72),
    })
}

impl<'i, 'a> Projection<'i, 'a> {
    /// Number of visible instances.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    fn u32(&self, field: usize, i: usize) -> u32 {
        self.inv.row_u32(field, self.start + i)
    }

    /// Source item id of row `i`. Hit-testing maps back through this, so
    /// interaction semantics stay exact.
    pub fn item(&self, i: usize) -> u32 {
        self.u32(0, i)
    }

    pub fn geometry(&self, i: usize) -> u32 {
        self.u32(1, i)
    }

    pub fn style(&self, i: usize) -> &'i Style {
        &self.inv.styles[self.u32(2, i) as usize]
    }

    pub fn transform(&self, i: usize) -> Affine {
        let f = |k| self.u32(k, i) as i32;
        Affine {
            e: f(3),
            f: f(4),
            a: f(5),
            b: f(6),
            c: f(7),
            d: f(8),
        }
    }

    pub fn opacity(&self, i: usize) -> u8 {
        self.inv.opacity[self.start + i]
    }

    /// Build the drawing for presentation from pre-decoded geometry.
    pub fn drawing(&self, paths: &[Arc<Path>]) -> Drawing {
        Drawing {
            width: self.inv.width,
            height: self.inv.height,
            items: (0..self.len())
                .map(|i| Item {
                    path: paths[self.geometry(i) as usize].clone(),
                    style: *self.style(i),
                    transform: self.transform(i),
                    opacity: self.opacity(i),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_graphics_container::Limits;
    use goya_graphics_ir::reference::{render, RenderLimits};
    use goya_graphics_ir::{
        Fill, FillRule, LineCap, LineJoin, Point, Rgba8, Segment, Stroke, FIXED_ONE,
    };

    fn pt(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    fn circle(cx: i32, cy: i32, r: i32) -> Path {
        use Segment::*;
        let k = (f64::from(r) * 0.552_284_749_830_793_6).round() as i32;
        Path::new(vec![
            MoveTo(pt(cx + r, cy)),
            CubicTo(pt(cx + r, cy + k), pt(cx + k, cy + r), pt(cx, cy + r)),
            CubicTo(pt(cx - k, cy + r), pt(cx - r, cy + k), pt(cx - r, cy)),
            CubicTo(pt(cx - r, cy - k), pt(cx - k, cy - r), pt(cx, cy - r)),
            CubicTo(pt(cx + k, cy - r), pt(cx + r, cy - k), pt(cx + r, cy)),
            Close,
        ])
        .unwrap()
    }

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Path {
        use Segment::*;
        Path::new(vec![
            MoveTo(pt(x0, y0)),
            LineTo(pt(x1, y0)),
            LineTo(pt(x1, y1)),
            LineTo(pt(x0, y1)),
            Close,
        ])
        .unwrap()
    }

    fn check() -> Path {
        use Segment::*;
        Path::new(vec![
            MoveTo(pt(7_500, 12_500)),
            LineTo(pt(10_500, 15_500)),
            LineTo(pt(16_500, 8_500)),
        ])
        .unwrap()
    }

    fn fill(c: Rgba8) -> Style {
        Style {
            fill: Some(Fill {
                colour: c,
                rule: FillRule::NonZero,
            }),
            stroke: None,
        }
    }

    const GREY: Rgba8 = Rgba8::new(90, 90, 100, 255);
    const BLUE: Rgba8 = Rgba8::new(42, 130, 255, 255);
    const AMBER: Rgba8 = Rgba8::new(255, 170, 0, 255);
    const RED: Rgba8 = Rgba8::new(220, 40, 40, 255);
    const WHITE: Rgba8 = Rgba8::new(255, 255, 255, 255);

    fn item(path: Path, style: Style) -> Item {
        Item {
            path: Arc::new(path),
            style,
            transform: Affine::IDENTITY,
            opacity: 255,
        }
    }

    /// Items: 0 backdrop, 1 indicator, 2 check mark, 3 warning bar.
    fn source() -> Drawing {
        Drawing {
            width: 24_000,
            height: 24_000,
            items: vec![
                item(
                    rect(0, 0, 24_000, 24_000),
                    fill(Rgba8::new(30, 30, 36, 255)),
                ),
                item(circle(12_000, 12_000, 8_000), fill(GREY)),
                item(
                    check(),
                    Style {
                        fill: None,
                        stroke: Some(Stroke {
                            colour: WHITE,
                            width: 1_800,
                            cap: LineCap::Round,
                            join: LineJoin::Round,
                        }),
                    },
                ),
                item(rect(11_000, 7_000, 13_000, 14_000), fill(WHITE)),
            ],
        }
    }

    /// Scale by `num/den` about the 24 pt icon's centre (12 pt, 12 pt).
    fn scale_about_centre(q16: i32) -> Affine {
        let e = 12_000 - (i64::from(q16) * 12_000 / i64::from(FIXED_ONE)) as i32;
        Affine {
            a: q16,
            b: 0,
            c: 0,
            d: q16,
            e,
            f: e,
        }
    }

    fn status() -> Dimension {
        let items = |v: &[u32]| Selector::Items(v.to_vec());
        Dimension::discrete(
            "status",
            vec![
                Value::new("idle").with(Effect::Hide(items(&[2, 3]))),
                Value::new("offline").with(Effect::Hide(items(&[2, 3]))),
                Value::new("active")
                    .with(Effect::Restyle(items(&[1]), fill(BLUE)))
                    .with(Effect::Hide(items(&[3]))),
                Value::new("warning")
                    .with(Effect::Restyle(items(&[1]), fill(AMBER)))
                    .with(Effect::Hide(items(&[2]))),
                Value::new("error")
                    .with(Effect::Restyle(items(&[1]), fill(RED)))
                    .with(Effect::Hide(items(&[2]))),
            ],
        )
    }

    /// Size is relative to the parent-assigned extent, not absolute pixels.
    fn size() -> Dimension {
        Dimension::discrete(
            "size",
            vec![
                Value::new("compact").with(Effect::Transform(
                    Selector::All,
                    scale_about_centre(FIXED_ONE / 2),
                )),
                Value::new("regular").with(Effect::Transform(
                    Selector::All,
                    scale_about_centre(FIXED_ONE * 3 / 4),
                )),
                Value::new("full"),
            ],
        )
    }

    fn emphasis() -> Dimension {
        Dimension::discrete(
            "emphasis",
            vec![
                Value::new("dim").with(Effect::Opacity(Selector::All, 128)),
                Value::new("normal"),
            ],
        )
    }

    fn product_model() -> StateModel {
        StateModel {
            parent: "scene://dashboard/status_icon".into(),
            dimensions: vec![status(), size(), emphasis()],
            reachability: Reachability::Product { exclusions: vec![] },
        }
    }

    fn bud() -> Budget {
        Budget::default()
    }

    fn gl() -> GvaLimits {
        GvaLimits::default()
    }

    fn lim() -> GviLimits {
        GviLimits::default()
    }

    fn binding(m: &StateModel) -> ParentBinding {
        m.binding(source().items.len() as u32, &bud()).unwrap()
    }

    /// Independent dynamic evaluation: what a runtime without GVI would compute
    /// for `state`, starting from the same canonical (GVA-normalised) source.
    fn dynamic(model: &StateModel, state: &[u16]) -> Drawing {
        let gva_bytes = goya_gva::encode(&source(), &gl()).unwrap();
        let canon = Gva::open(&gva_bytes, &gl()).unwrap().to_drawing().unwrap();
        let mut items: Vec<(u32, Item, bool)> = canon
            .items
            .into_iter()
            .enumerate()
            .map(|(i, it)| (i as u32, it, false))
            .collect();
        for (d, &v) in state.iter().enumerate() {
            let Domain::Discrete(values) = &model.dimensions[d].domain else {
                unreachable!()
            };
            for e in &values[v as usize].effects {
                for (id, it, hidden) in items.iter_mut() {
                    match e {
                        Effect::Transform(s, t) if s.matches(*id) => {
                            it.transform = it.transform.then(t).unwrap()
                        }
                        Effect::Opacity(s, f) if s.matches(*id) => {
                            it.opacity = ((u32::from(it.opacity) * u32::from(*f) + 127) / 255) as u8
                        }
                        Effect::Restyle(s, st) if s.matches(*id) => it.style = *st,
                        Effect::Hide(s) if s.matches(*id) => *hidden = true,
                        Effect::Morph(s, target, t) if s.matches(*id) => {
                            // Independent lerp in the canonical frame.
                            let src_first = source().items[*id as usize].path.first_point();
                            let canon_first = it.path.first_point();
                            let (dx, dy) =
                                (canon_first.x - src_first.x, canon_first.y - src_first.y);
                            let to: Vec<Point> = target.points().collect();
                            let mut k = 0;
                            let lerp = |a: i32, b: i32| -> i32 {
                                let num = (i64::from(b) - i64::from(a)) * i64::from(*t);
                                let (q, r) = (num.div_euclid(65536), num.rem_euclid(65536));
                                let q = if r > 32768 || (r == 32768 && q & 1 == 1) {
                                    q + 1
                                } else {
                                    q
                                };
                                (i64::from(a) + q) as i32
                            };
                            let segs: Vec<Segment> = it
                                .path
                                .segments()
                                .iter()
                                .map(|seg| {
                                    let mut mv = |p: Point| {
                                        let q = to[k];
                                        k += 1;
                                        Point::new(lerp(p.x, q.x + dx), lerp(p.y, q.y + dy))
                                    };
                                    match *seg {
                                        Segment::MoveTo(p) => Segment::MoveTo(mv(p)),
                                        Segment::LineTo(p) => Segment::LineTo(mv(p)),
                                        Segment::QuadTo(c, p) => Segment::QuadTo(mv(c), mv(p)),
                                        Segment::CubicTo(a, b, p) => {
                                            Segment::CubicTo(mv(a), mv(b), mv(p))
                                        }
                                        Segment::Close => Segment::Close,
                                    }
                                })
                                .collect();
                            it.path = Arc::new(Path::new(segs).unwrap());
                        }
                        _ => {}
                    }
                }
            }
        }
        Drawing {
            width: 24_000,
            height: 24_000,
            items: items
                .into_iter()
                .filter(|(_, _, h)| !h)
                .map(|(_, it, _)| it)
                .collect(),
        }
    }

    #[test]
    fn every_state_resolves_to_its_dynamic_result() {
        let m = product_model();
        let bytes = compile(&source(), &m, &bud(), &gl()).unwrap();
        let inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
        assert_eq!(inv.state_count(), 5 * 3 * 2);
        let paths = inv.decode_geometry().unwrap();
        let rl = RenderLimits::default();
        let radix = Radix::new(vec![5, 3, 2]).unwrap();
        for key in 0..30 {
            let state = radix.unpack(key);
            let proj = inv.resolve(&state).unwrap();
            let pre = proj.drawing(&paths);
            assert_eq!(pre, dynamic(&m, &state), "state {state:?}");
            assert_eq!(
                render(&pre, 2.0, &rl).unwrap(),
                render(&dynamic(&m, &state), 2.0, &rl).unwrap()
            );
        }
    }

    #[test]
    fn identical_appearances_are_merged() {
        let m = product_model();
        let bytes = compile(&source(), &m, &bud(), &gl()).unwrap();
        let inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
        // idle and offline look the same, so 6 of 30 states share projections.
        assert_eq!(inv.projection_count(), 24);
        let c = &inv.certificate;
        assert_eq!(
            (c.reachable, c.projections, c.merged_states, c.domain_size),
            (30, 24, 6, 30)
        );
        assert_eq!(c.method, METHOD_PRODUCT);
        let idle = inv
            .state(&[("status", "idle"), ("size", "full"), ("emphasis", "normal")])
            .unwrap();
        let offline = inv
            .state(&[
                ("status", "offline"),
                ("size", "full"),
                ("emphasis", "normal"),
            ])
            .unwrap();
        assert_eq!(
            inv.resolve(&idle).unwrap().id,
            inv.resolve(&offline).unwrap().id
        );
        // Hidden items are absent; rows keep their source item ids.
        let p = inv.resolve(&idle).unwrap();
        assert_eq!(
            (0..p.len()).map(|i| p.item(i)).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn deterministic_and_golden() {
        let m = product_model();
        let a = compile(&source(), &m, &bud(), &gl()).unwrap();
        assert_eq!(a, compile(&source(), &m, &bud(), &gl()).unwrap());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("fixtures")
            .join("graphics")
            .join("gvi")
            .join("status_icon.gvi");
        // Regenerate only with a spec change: GOYA_GFX_BLESS=1 cargo test -p goya-gvi golden
        if std::env::var_os("GOYA_GFX_BLESS").is_some() {
            std::fs::write(&path, &a).unwrap();
        }
        assert_eq!(
            a,
            std::fs::read(&path).expect("golden fixture missing"),
            "GVI byte layout changed"
        );
        let inv = Inventory::open(&a, &binding(&m), &lim()).unwrap();
        inv.verify_against(&source(), &m, &bud(), &gl()).unwrap();
    }

    fn transitions_model() -> StateModel {
        // Starts idle at full size; may become active, then warning or error;
        // only error may shrink to compact.
        StateModel {
            parent: "scene://dashboard/status_icon".into(),
            dimensions: vec![status(), size()],
            reachability: Reachability::Transitions {
                initial: vec![0, 2],
                rules: vec![
                    Rule {
                        dim: 0,
                        from: Some(0),
                        to: 2,
                        guard: vec![],
                    },
                    Rule {
                        dim: 0,
                        from: Some(2),
                        to: 3,
                        guard: vec![],
                    },
                    Rule {
                        dim: 0,
                        from: Some(2),
                        to: 4,
                        guard: vec![],
                    },
                    Rule {
                        dim: 0,
                        from: Some(3),
                        to: 2,
                        guard: vec![],
                    },
                    Rule {
                        dim: 1,
                        from: Some(2),
                        to: 0,
                        guard: vec![(0, 4)],
                    },
                ],
            },
        }
    }

    #[test]
    fn transitions_reachability_and_rejection() {
        let m = transitions_model();
        let bytes = compile(&source(), &m, &bud(), &gl()).unwrap();
        let inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
        // idle/full, active/full, warning/full, error/full, error/compact.
        assert_eq!(inv.state_count(), 5);
        assert_eq!(inv.certificate.method, METHOD_TRANSITION_CLOSURE);
        let s = |st: &str, sz: &str| inv.state(&[("status", st), ("size", sz)]).unwrap();
        inv.resolve(&s("error", "compact")).unwrap();
        // Valid in the domain, but never reachable: rejected, not substituted.
        assert_eq!(
            inv.resolve(&s("offline", "full")).unwrap_err().code,
            codes::GVI_UNREACHABLE_STATE
        );
        assert_eq!(
            inv.resolve(&s("warning", "compact")).unwrap_err().code,
            codes::GVI_UNREACHABLE_STATE
        );
        assert_eq!(
            inv.resolve(&[9, 0]).unwrap_err().code,
            codes::GVI_INVALID_STATE
        );
        assert_eq!(
            inv.resolve(&[0]).unwrap_err().code,
            codes::GVI_INVALID_STATE
        );
        assert!(inv
            .permits(&s("idle", "full"), &s("active", "full"))
            .unwrap());
        assert!(!inv
            .permits(&s("idle", "full"), &s("error", "full"))
            .unwrap());
        assert!(inv
            .permits(&s("error", "full"), &s("error", "compact"))
            .unwrap());
        assert_eq!(
            inv.state(&[("status", "idle")]).unwrap_err().code,
            codes::GVI_INVALID_STATE
        );
        assert_eq!(
            inv.state(&[("status", "idle"), ("size", "huge")])
                .unwrap_err()
                .code,
            codes::GVI_INVALID_STATE
        );
    }

    #[test]
    fn parent_binding_is_enforced() {
        let m = product_model();
        let bytes = compile(&source(), &m, &bud(), &gl()).unwrap();
        // Another parent using the same source cannot consume this inventory.
        let mut other = m.clone();
        other.parent = "scene://settings/status_icon".into();
        assert_eq!(
            Inventory::open(&bytes, &binding(&other), &lim())
                .unwrap_err()
                .code,
            codes::GVI_PARENT_MISMATCH
        );
        // Nor can the same parent after its state semantics change.
        let mut changed = m.clone();
        changed.reachability = Reachability::Product {
            exclusions: vec![vec![(0, 1)]],
        };
        assert_eq!(
            Inventory::open(&bytes, &binding(&changed), &lim())
                .unwrap_err()
                .code,
            codes::GVI_PARENT_MISMATCH
        );
        // The cache key moves with source, model and compiler inputs.
        let k1 = cache_key(&source(), &m, &bud(), &gl(), Bake::None).unwrap();
        assert_eq!(
            k1,
            cache_key(&source(), &m, &bud(), &gl(), Bake::None).unwrap()
        );
        assert_ne!(
            k1,
            cache_key(&source(), &changed, &bud(), &gl(), Bake::None).unwrap()
        );
        let mut src2 = source();
        src2.items[0].opacity = 200;
        assert_ne!(k1, cache_key(&src2, &m, &bud(), &gl(), Bake::None).unwrap());
        assert_ne!(
            k1,
            cache_key(&source(), &m, &bud(), &gl(), Bake::Transforms).unwrap()
        );
        // verify_against notices a different source.
        let inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
        assert_eq!(
            inv.verify_against(&src2, &m, &bud(), &gl())
                .unwrap_err()
                .code,
            codes::GVI_SOURCE_MISMATCH
        );
    }

    #[test]
    fn ineligible_models_are_refused() {
        let mut m = product_model();
        m.dimensions.push(Dimension {
            name: "brightness".into(),
            domain: Domain::Continuous { min: 0.0, max: 1.0 },
        });
        let e = compile(&source(), &m, &bud(), &gl()).unwrap_err();
        assert_eq!(e.code, codes::GVI_NOT_FINITE);
        assert!(e.detail.contains("SPC"), "{e}");
        m.dimensions[3].domain = Domain::Opaque;
        assert_eq!(
            compile(&source(), &m, &bud(), &gl()).unwrap_err().code,
            codes::GVI_NOT_KNOWN
        );
        let tight = Budget {
            max_states: 29,
            ..bud()
        };
        assert_eq!(
            compile(&source(), &product_model(), &tight, &gl())
                .unwrap_err()
                .code,
            codes::GVI_NOT_MANAGEABLE
        );
        let small = Budget {
            max_bytes: 1_000,
            ..bud()
        };
        assert_eq!(
            compile(&source(), &product_model(), &small, &gl())
                .unwrap_err()
                .code,
            codes::GVI_NOT_MANAGEABLE
        );
        let a = assess(&product_model(), &bud());
        assert!(a.eligible());
        assert_eq!((a.domain_size, a.reachable), (Some(30), Some(30)));
    }

    // ---------------------------------------------------------------------------
    // Tampering behind valid container digests
    // ---------------------------------------------------------------------------

    fn rebuild(bytes: &[u8], edits: &mut dyn FnMut(Tag, &mut Vec<u8>)) -> Vec<u8> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gvi,
                known_sections: &KNOWN,
            },
            &Limits::default(),
        )
        .unwrap();
        let owned: Vec<(Tag, bool, Vec<u8>)> = c
            .entries()
            .iter()
            .map(|e| {
                let mut d = c.section(e.tag).unwrap().unwrap().to_vec();
                edits(e.tag, &mut d);
                (e.tag, e.mandatory, d)
            })
            .collect();
        let inputs: Vec<SectionInput<'_>> = owned
            .iter()
            .map(|(t, m, d)| {
                let s = SectionInput::new(*t, d).align_log2(ALIGN_LOG2);
                if *m {
                    s
                } else {
                    s.optional()
                }
            })
            .collect();
        encode_container(
            Format::Gvi,
            Profile::Baseline,
            0,
            &inputs,
            &Limits::default(),
        )
        .unwrap()
    }

    /// Remove `key` from the index and patch the certificate to match.
    fn drop_state(bytes: &[u8], key: u64) -> Vec<u8> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gvi,
                known_sections: &KNOWN,
            },
            &Limits::default(),
        )
        .unwrap();
        let d = c.section(INDX).unwrap().unwrap();
        let n = u32::from_le_bytes([d[0], d[1], d[2], d[3]]) as usize;
        let proj_at = 16 + (n * 8).div_ceil(16) * 16;
        let kept: Vec<(u64, u32)> = (0..n)
            .map(|i| {
                (
                    u64::from_le_bytes(d[16 + i * 8..24 + i * 8].try_into().unwrap()),
                    u32::from_le_bytes(d[proj_at + i * 4..proj_at + i * 4 + 4].try_into().unwrap()),
                )
            })
            .filter(|&(k, _)| k != key)
            .collect();
        assert_eq!(kept.len(), n - 1, "key {key} was not indexed");
        let mut indx = Vec::new();
        indx.extend_from_slice(&(kept.len() as u32).to_le_bytes());
        indx.extend_from_slice(&[0u8; 12]);
        for (k, _) in &kept {
            indx.extend_from_slice(&k.to_le_bytes());
        }
        let digest = sha256(&[&indx[16..]]);
        pad16(&mut indx);
        for (_, p) in &kept {
            indx.extend_from_slice(&p.to_le_bytes());
        }
        pad16(&mut indx);
        rebuild(bytes, &mut |tag, sec| match tag {
            INDX => *sec = indx.clone(),
            CERT => {
                let reachable = kept.len() as u64;
                let projections = u64::from_le_bytes(sec[24..32].try_into().unwrap());
                sec[16..24].copy_from_slice(&reachable.to_le_bytes());
                sec[32..40].copy_from_slice(&(reachable - projections).to_le_bytes());
                sec[40..72].copy_from_slice(&digest);
            }
            _ => {}
        })
    }

    fn open_code(bytes: &[u8], m: &StateModel) -> u16 {
        Inventory::open(bytes, &binding(m), &lim())
            .expect_err("expected rejection")
            .code
    }

    #[test]
    fn certificate_tampering_is_caught() {
        let m = transitions_model();
        let base = compile(&source(), &m, &bud(), &gl()).unwrap();

        // Drop a reachable state whose projection is shared with another state
        // and patch every figure that mentions it. Only the independent
        // completeness check can object.
        let mut shared = m.clone();
        if let Reachability::Transitions { rules, .. } = &mut shared.reachability {
            rules.push(Rule {
                dim: 0,
                from: Some(0),
                to: 1,
                guard: vec![],
            });
        }
        let shared_bytes = compile(&source(), &shared, &bud(), &gl()).unwrap();
        // offline/full = 1·3 + 2; it looks exactly like idle/full.
        let forged = drop_state(&shared_bytes, 5);
        let err = Inventory::open(&forged, &binding(&shared), &lim()).unwrap_err();
        assert_eq!(err.code, codes::GVI_CERTIFICATE_INVALID, "{err}");
        assert!(err.detail.contains("missing from the index"), "{err}");
        // Same for product reachability.
        let pm = product_model();
        let pbytes = compile(&source(), &pm, &bud(), &gl()).unwrap();
        let forged = drop_state(&pbytes, 6 + 2 * 2 + 1); // offline/full/normal
        let err = Inventory::open(&forged, &binding(&pm), &lim()).unwrap_err();
        assert_eq!(err.code, codes::GVI_CERTIFICATE_INVALID, "{err}");

        // Figures that disagree with the index.
        let lying = rebuild(&base, &mut |tag, d| {
            if tag == CERT {
                d[32] ^= 1;
            }
        });
        assert_eq!(open_code(&lying, &m), codes::GVI_CERTIFICATE_INVALID);
        let unknown_assumption = rebuild(&base, &mut |tag, d| {
            if tag == CERT {
                d[72] |= 8;
            }
        });
        assert_eq!(
            open_code(&unknown_assumption, &m),
            codes::GVI_CERTIFICATE_INVALID
        );

        // A stored model edited after compilation no longer matches its binding.
        let edited = rebuild(&base, &mut |tag, d| {
            if tag == TRAN {
                d[8] ^= 1;
            }
        });
        assert_eq!(open_code(&edited, &m), codes::GVI_CERTIFICATE_INVALID);

        // Projection rows pointing at missing styles.
        let dangling = rebuild(&base, &mut |tag, d| {
            if tag == PROJ {
                let np = u32::from_le_bytes([d[0], d[1], d[2], d[3]]) as usize;
                let total = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
                let starts = 16 + ((np + 1) * 4).div_ceil(16) * 16;
                let arr = (total * 4).div_ceil(16) * 16;
                let style0 = starts + 2 * arr;
                d[style0..style0 + 4].copy_from_slice(&999u32.to_le_bytes());
            }
        });
        assert_eq!(open_code(&dangling, &m), codes::STYLE_REF);
    }

    // -----------------------------------------------------------------------
    // Baked projections and morphing
    // -----------------------------------------------------------------------

    /// A rounded square with the circle's segment structure, authored in the
    /// source item's own coordinates (centred on 12 pt, 12 pt).
    fn squircle() -> Arc<Path> {
        use Segment::*;
        let (c, r, k) = (12_000, 7_000, 2_000);
        Arc::new(
            Path::new(vec![
                MoveTo(pt(c + r, c)),
                CubicTo(pt(c + r, c + r - k), pt(c + r - k, c + r), pt(c, c + r)),
                CubicTo(pt(c - r + k, c + r), pt(c - r, c + r - k), pt(c - r, c)),
                CubicTo(pt(c - r, c - r + k), pt(c - r + k, c - r), pt(c, c - r)),
                CubicTo(pt(c + r - k, c - r), pt(c + r, c - r + k), pt(c + r, c)),
                Close,
            ])
            .unwrap(),
        )
    }

    fn morph_model() -> StateModel {
        let shape = (0..=4u32)
            .map(|k| {
                Value::new(&format!("t{k}")).with(Effect::Morph(
                    Selector::Items(vec![1]),
                    squircle(),
                    k * 16_384,
                ))
            })
            .collect();
        StateModel {
            parent: "scene://dashboard/status_icon".into(),
            dimensions: vec![status(), size(), Dimension::discrete("shape", shape)],
            reachability: Reachability::Product { exclusions: vec![] },
        }
    }

    fn max_channel_diff(
        a: &goya_graphics_ir::reference::Image,
        b: &goya_graphics_ir::reference::Image,
    ) -> u8 {
        a.max_premultiplied_diff(b).unwrap()
    }

    #[test]
    fn morph_states_match_dynamic_interpolation() {
        let m = morph_model();
        let bytes = compile(&source(), &m, &bud(), &gl()).unwrap();
        let inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
        let paths = inv.decode_geometry().unwrap();
        let radix = Radix::new(vec![5, 3, 5]).unwrap();
        for key in 0..radix.domain {
            let state = radix.unpack(key);
            assert_eq!(
                inv.resolve(&state).unwrap().drawing(&paths),
                dynamic(&m, &state),
                "state {state:?}"
            );
        }
        // t = 0 reuses the source geometry; the other four fractions add one
        // geometry each, shared across every status and size state.
        assert_eq!(paths.len(), 4 + 4);
        let full = inv
            .state(&[("status", "idle"), ("size", "full"), ("shape", "t4")])
            .unwrap();
        let p = inv.resolve(&full).unwrap();
        let morphed = &paths[p.geometry(1) as usize];
        // The source circle starts at (20 pt, 12 pt) and is origin-folded.
        let expect = squircle().translated(-20_000, -12_000).unwrap();
        assert_eq!(**morphed, expect, "t = 1 lands exactly on the target");
        inv.verify_against(&source(), &m, &bud(), &gl()).unwrap();
    }

    #[test]
    fn morph_rejects_mismatched_targets() {
        let mut m = morph_model();
        let Domain::Discrete(values) = &mut m.dimensions[2].domain else {
            unreachable!()
        };
        values[1].effects = vec![Effect::Morph(
            Selector::Items(vec![1]),
            Arc::new(check()),
            100,
        )];
        assert_eq!(
            compile(&source(), &m, &bud(), &gl()).unwrap_err().code,
            codes::GVI_INVALID_MODEL
        );
        let Domain::Discrete(values) = &mut m.dimensions[2].domain else {
            unreachable!()
        };
        values[1].effects = vec![Effect::Morph(Selector::Items(vec![1]), squircle(), 70_000)];
        assert_eq!(
            compile(&source(), &m, &bud(), &gl()).unwrap_err().code,
            codes::GVI_INVALID_MODEL
        );
        assert!(morph_path(&circle(0, 0, 10), &check(), 0).is_none());
    }

    #[test]
    fn baked_projections_carry_only_translations() {
        let m = morph_model();
        let plain = compile(&source(), &m, &bud(), &gl()).unwrap();
        let baked = compile_with(&source(), &m, &bud(), &gl(), Bake::Transforms).unwrap();
        let pi = Inventory::open(&plain, &binding(&m), &lim()).unwrap();
        let bi = Inventory::open(&baked, &binding(&m), &lim()).unwrap();
        assert_eq!(
            bi.certificate.assumptions & ASSUME_BAKED_TRANSFORMS,
            ASSUME_BAKED_TRANSFORMS
        );
        bi.verify_against(&source(), &m, &bud(), &gl()).unwrap();
        let (pp, bp) = (pi.decode_geometry().unwrap(), bi.decode_geometry().unwrap());
        let rl = RenderLimits::default();
        let radix = Radix::new(vec![5, 3, 5]).unwrap();
        let mut worst = 0u8;
        for key in 0..radix.domain {
            let state = radix.unpack(key);
            let (a, b) = (pi.resolve(&state).unwrap(), bi.resolve(&state).unwrap());
            assert_eq!(a.len(), b.len());
            for i in 0..b.len() {
                let t = b.transform(i);
                assert_eq!(
                    (t.a, t.b, t.c, t.d),
                    (FIXED_ONE, 0, 0, FIXED_ONE),
                    "row {i} of {state:?} is not translation-only"
                );
                assert_eq!(a.item(i), b.item(i), "semantic item ids survive baking");
            }
            let ra = render(&a.drawing(&pp), 4.0, &rl).unwrap();
            let rb = render(&b.drawing(&bp), 4.0, &rl).unwrap();
            worst = worst.max(max_channel_diff(&ra, &rb));
        }
        // Baking rounds each point to the nearest millipoint (≤ 0.0005 pt),
        // which can move edge coverage by at most one 8-bit level.
        assert!(
            worst <= 1,
            "baked rendering drifted by {worst} premultiplied levels"
        );
        assert!(bp.len() > pp.len());
    }

    #[test]
    fn non_similarity_strokes_stay_unbaked() {
        let squash = Affine {
            a: FIXED_ONE,
            b: 0,
            c: 0,
            d: FIXED_ONE / 2,
            e: 0,
            f: 6_000,
        };
        let m = StateModel {
            parent: "scene://x".into(),
            dimensions: vec![Dimension::discrete(
                "squash",
                vec![
                    Value::new("no"),
                    Value::new("yes").with(Effect::Transform(Selector::All, squash)),
                ],
            )],
            reachability: Reachability::Product { exclusions: vec![] },
        };
        let bytes = compile_with(&source(), &m, &bud(), &gl(), Bake::Transforms).unwrap();
        let inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
        let p = inv.resolve(&[1]).unwrap();
        assert_eq!(p.len(), 4);
        for i in 0..p.len() {
            let t = p.transform(i);
            let linear_identity = (t.a, t.b, t.c, t.d) == (FIXED_ONE, 0, 0, FIXED_ONE);
            // Item 2 is the stroked check mark: squashing would distort its
            // stroke, so it keeps the transform. Fills bake.
            assert_eq!(
                linear_identity,
                p.item(i) != 2,
                "row for item {}",
                p.item(i)
            );
        }
    }

    #[test]
    fn product_completeness_check() {
        let m = StateModel {
            reachability: Reachability::Product {
                exclusions: vec![vec![(0, 1), (2, 0)]],
            },
            ..product_model()
        };
        let bytes = compile(&source(), &m, &bud(), &gl()).unwrap();
        let inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
        assert_eq!(inv.state_count(), 27);
        let off_dim = inv
            .state(&[("status", "offline"), ("size", "full"), ("emphasis", "dim")])
            .unwrap();
        assert_eq!(
            inv.resolve(&off_dim).unwrap_err().code,
            codes::GVI_UNREACHABLE_STATE
        );
    }

    #[test]
    fn payload_fuzz_never_panics() {
        let m = transitions_model();
        let b = binding(&m);
        let base = compile(&source(), &m, &bud(), &gl()).unwrap();
        let tags = [CERT, GEOM, INDX, PARN, PROJ, SRCE, STAT, STYL, TRAN];
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..2_000 {
            let target = tags[(next() % tags.len() as u64) as usize];
            let (r1, r2, r3) = (next(), next(), next());
            let mutated = rebuild(&base, &mut |tag, d| {
                if tag != target || d.is_empty() {
                    return;
                }
                for k in 0..(1 + r1 % 3) {
                    let i = ((r2 >> (k * 16)) % d.len() as u64) as usize;
                    d[i] = (r3 >> (k * 8)) as u8;
                }
                if r1 % 5 == 0 {
                    d.truncate((r2 % d.len() as u64) as usize);
                }
            });
            // Try both the true binding and the one recomputed from the mutated
            // model, so checks past the binding gate are exercised too.
            let mut bindings = vec![b];
            if let Ok(c) = Container::open(
                &mutated,
                &Expect {
                    format: Format::Gvi,
                    known_sections: &KNOWN,
                },
                &Limits::default(),
            ) {
                if let Ok(Some(p)) = c.section(PARN) {
                    if p.len() >= 32 {
                        bindings.push(ParentBinding(p[..32].try_into().unwrap()));
                    }
                }
            }
            for bind in bindings {
                if let Ok(inv) = Inventory::open(&mutated, &bind, &lim()) {
                    let paths = inv.decode_geometry();
                    let radix = Radix::new(vec![5, 3]).unwrap();
                    for key in 0..15 {
                        if let Ok(p) = inv.resolve(&radix.unpack(key)) {
                            if let Ok(paths) = &paths {
                                if !p.is_empty()
                                    && (0..p.len()).all(|i| (p.geometry(i) as usize) < paths.len())
                                {
                                    let _ = p.drawing(paths);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
