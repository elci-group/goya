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

mod diff;
mod index;
mod lineless;
pub mod model;
mod operative;
mod packed;

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
    assess_and_enumerate, encode_stat, encode_stat_operative, encode_tran, excluded, parse_stat,
    parse_tran, step, Reader, STAT_OPERATIVE,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

/// Contract identifier implemented by this crate.
pub const GVI_CONTRACT: &str = "GY-GVI/0";
/// Compiler identity recorded in `PARN` and folded into the cache key.
pub const COMPILER_ID: &str = "goya-gvi 0.0.1 (GY-GVI/0)";

pub const CERT: Tag = Tag::new(b"CERT");
pub const COST: Tag = Tag::new(b"COST");
pub const DIFF: Tag = Tag::new(b"DIFF");
pub const GEOM: Tag = Tag::new(b"GEOM");
pub const HASH: Tag = Tag::new(b"HASH");
pub const INDX: Tag = Tag::new(b"INDX");
pub const LSTY: Tag = Tag::new(b"LSTY");
pub const PARN: Tag = Tag::new(b"PARN");
pub const PROJ: Tag = Tag::new(b"PROJ");
pub const SRCE: Tag = Tag::new(b"SRCE");
pub const STAT: Tag = Tag::new(b"STAT");
pub const STYL: Tag = Tag::new(b"STYL");
pub const TRAN: Tag = Tag::new(b"TRAN");
const KNOWN: [Tag; 13] = [
    CERT, COST, DIFF, GEOM, HASH, INDX, LSTY, PARN, PROJ, SRCE, STAT, STYL, TRAN,
];
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
/// `CERT.assumptions` bit 3: some geometry is stored as differential
/// records in `DIFF`, bounded by `COST` (§6).
pub const ASSUME_DIFFERENTIAL_GEOMETRY: u32 = 8;
/// `CERT.assumptions` bit 4: styles use the GVI-L palette record section.
pub const ASSUME_LINELESS_STYLES: u32 = 16;
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

/// GVI-A trajectory encoding (`docs/graphics/gvi-albit.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AlbitProfile {
    /// GVI-D only.
    #[default]
    Off,
    /// Trajectory candidates from compiler provenance. Geometries that
    /// have one skip the GVI-D predictor search; the others still use it.
    Provenance,
    /// Trajectory candidates plus the full GVI-D search for every
    /// geometry, keeping the cheapest exact record.
    Exhaustive,
}

impl AlbitProfile {
    fn id(self) -> u8 {
        match self {
            AlbitProfile::Off => 0,
            AlbitProfile::Provenance => 1,
            AlbitProfile::Exhaustive => 2,
        }
    }

    fn from_id(id: u8) -> Option<AlbitProfile> {
        Some(match id {
            0 => AlbitProfile::Off,
            1 => AlbitProfile::Provenance,
            2 => AlbitProfile::Exhaustive,
            _ => return None,
        })
    }
}

/// GVI-O operative encoding (`docs/graphics/gvi-operative.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OperativeProfile {
    /// Literal `STAT` and record-per-entry `DIFF`.
    #[default]
    Off,
    /// Constants, progressions, exact rational progressions and numbered
    /// labels, found by a linear greedy scan with a bounded work budget.
    /// Each section takes the operative form only if it is smaller, and the
    /// file only if it beats the same compilation without GVI-O.
    Fast,
}

/// Encoding choices that change bytes but not semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompileOptions {
    pub bake: Bake,
    /// Largest reconstruction depth for differential geometry (§6). `0`
    /// stores every geometry complete.
    pub max_diff_depth: u32,
    /// GVI-A trajectory records. They live in `DIFF`, so they need
    /// `max_diff_depth ≥ 1`; with depth 0 this setting has no effect.
    pub albit: AlbitProfile,
    /// GVI-O operative encoding of the state model and differential
    /// records.
    pub operative: OperativeProfile,
}

impl From<Bake> for CompileOptions {
    fn from(bake: Bake) -> CompileOptions {
        CompileOptions {
            bake,
            max_diff_depth: 0,
            albit: AlbitProfile::Off,
            operative: OperativeProfile::Off,
        }
    }
}

/// The deepest differential chain an encoder may declare.
pub const MAX_DIFF_DEPTH: u32 = 64;

/// The build cache key (directive §8.6): source digest, parent binding,
/// state schema (inside the binding), compilation profile, encoding options
/// and compiler version. If any of them changes, the inventory must be
/// regenerated.
pub fn cache_key(
    source: &Drawing,
    model: &StateModel,
    budget: &Budget,
    gva_limits: &GvaLimits,
    options: impl Into<CompileOptions>,
) -> Result<[u8; 32], GfxDiag> {
    let options = options.into();
    let gva = goya_gva::encode(source, gva_limits)?;
    let binding = model.binding(source.items.len() as u32, budget)?;
    Ok(sha256(&[
        COMPILER_ID.as_bytes(),
        &Profile::Baseline.id().to_le_bytes(),
        &[options.bake.id()],
        &options.max_diff_depth.to_le_bytes(),
        &[options.albit.id()],
        &[u8::from(options.operative == OperativeProfile::Fast)],
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
    /// GVI-A provenance: the last morph that produced `path`, as
    /// `(from, to in the canonical frame, t)`.
    origin: Option<(Arc<Path>, Arc<Path>, u32)>,
}

/// An encoded projection row: item, geometry, style index, transform,
/// opacity.
type ProjRow = (u32, u32, u32, Affine, u8);

/// `round_half_even(v / 65536)`.
pub(crate) fn round_q16(v: i128) -> i128 {
    let q = v.div_euclid(65536);
    match v.rem_euclid(65536).cmp(&32768) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
    }
}

/// Rebuild `path` with every stored point replaced by `f(index, point)`.
pub(crate) fn map_points(
    path: &Path,
    mut f: impl FnMut(usize, Point) -> Option<Point>,
) -> Option<Path> {
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
    let geometry = diff::bake_geometry(path, t)?;
    let (x, y) = t.apply_scaled(path.first_point());
    let o = Point::new(
        i32::try_from(round_q16(x)).ok()?,
        i32::try_from(round_q16(y)).ok()?,
    );
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
                    let morphed = Arc::new(morph_path(&r.path, &target, *t).ok_or_else(mismatch)?);
                    r.origin = Some((r.path.clone(), Arc::new(target), *t));
                    r.path = morphed;
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

/// The sorted key list as `u64` little-endian bytes: the input to the
/// certificate's coverage digest, whatever form `INDX` uses.
fn key_list_bytes(keys: &[u64]) -> Vec<u8> {
    keys.iter().flat_map(|k| k.to_le_bytes()).collect()
}

/// The id of `path` in the pool, inserting it if new.
fn ensure_geometry(
    pool: &mut Vec<Path>,
    index: &mut HashMap<Path, u32>,
    path: &Path,
    limits: &GvaLimits,
) -> Result<u32, GfxDiag> {
    if let Some(&g) = index.get(path) {
        return Ok(g);
    }
    let g = pool.len() as u32;
    if g >= limits.max_geometries {
        return Err(diag(
            codes::TOO_MANY_GEOMETRIES,
            "projections need too many distinct geometries",
        ));
    }
    index.insert(path.clone(), g);
    pool.push(path.clone());
    Ok(g)
}

/// `PROJ` (gvi.md §4): a pool of distinct rows in first-use order, a list
/// of row ids per projection (omitted when it is the identity), and
/// bit-packed columns.
/// `PROJ` flags bit 0: the row list is the identity.
const PROJ_IDENTITY: u32 = 1;
/// `PROJ` flags bit 1: columns may use GVI-O operative spans.
const PROJ_OPERATIVE: u32 = 2;

pub(crate) fn encode_proj(projections: &[Vec<ProjRow>], operative: bool) -> Vec<u8> {
    let mut pool: Vec<ProjRow> = Vec::new();
    let mut index: HashMap<ProjRow, u32> = HashMap::new();
    let mut list: Vec<i64> = Vec::new();
    let mut starts: Vec<i64> = vec![0];
    for p in projections {
        for r in p {
            let id = *index.entry(*r).or_insert_with(|| {
                pool.push(*r);
                (pool.len() - 1) as u32
            });
            list.push(i64::from(id));
        }
        starts.push(list.len() as i64);
    }
    let identity = list.len() == pool.len();
    let mut out = Vec::new();
    out.extend_from_slice(&(projections.len() as u32).to_le_bytes());
    out.extend_from_slice(&(pool.len() as u32).to_le_bytes());
    out.extend_from_slice(&(list.len() as u32).to_le_bytes());
    let flags =
        if identity { PROJ_IDENTITY } else { 0 } | if operative { PROJ_OPERATIVE } else { 0 };
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&packed::encode_column_with(&starts, operative));
    if !identity {
        out.extend_from_slice(&packed::encode_column_with(&list, operative));
    }
    for field in 0..10 {
        let col: Vec<i64> = pool
            .iter()
            .map(|r| match field {
                0 => i64::from(r.0),
                1 => i64::from(r.1),
                2 => i64::from(r.2),
                3 => i64::from(r.3.e),
                4 => i64::from(r.3.f),
                5 => i64::from(r.3.a),
                6 => i64::from(r.3.b),
                7 => i64::from(r.3.c),
                8 => i64::from(r.3.d),
                _ => i64::from(r.4),
            })
            .collect();
        out.extend_from_slice(&packed::encode_column_with(&col, operative));
    }
    pad16(&mut out);
    out
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
    options: impl Into<CompileOptions>,
) -> Result<Vec<u8>, GfxDiag> {
    let CompileOptions {
        bake,
        max_diff_depth,
        albit,
        operative,
    } = options.into();
    let albit_on = albit != AlbitProfile::Off && max_diff_depth > 0;
    if max_diff_depth > MAX_DIFF_DEPTH {
        return Err(diag(
            codes::GVI_INVALID_MODEL,
            format!("differential depth {max_diff_depth} exceeds {MAX_DIFF_DEPTH}"),
        ));
    }
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
            origin: None,
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
    // Known bake origins: baked geometry id → (unbaked geometry id, linear part).
    let mut provenance: HashMap<u32, (u32, Affine)> = HashMap::new();
    // GVI-A trajectory origins: geometry id → its morph endpoints and t.
    let mut origins: HashMap<u32, diff::Origin> = HashMap::new();
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
            // GVI-A: give the trajectory's endpoints earlier ids than its
            // samples, so a sample record can reference them directly.
            let origin_ids = match (&r.origin, albit_on) {
                (Some((from, to, t)), true) => Some((
                    ensure_geometry(&mut pool, &mut pool_index, from, gva_limits)?,
                    ensure_geometry(&mut pool, &mut pool_index, to, gva_limits)?,
                    *t,
                )),
                _ => None,
            };
            // For differential encoding, make sure the unbaked geometry
            // has an earlier id than its baked forms, so the exact Affine
            // predictor can refer to it. A trajectory sample does not need
            // it.
            if baked.is_some()
                && max_diff_depth > 0
                && origin_ids.is_none()
                && !pool_index.contains_key(&*r.path)
            {
                if pool.len() as u32 >= gva_limits.max_geometries {
                    return Err(diag(
                        codes::TOO_MANY_GEOMETRIES,
                        "projections need too many distinct geometries",
                    ));
                }
                pool_index.insert((*r.path).clone(), pool.len() as u32);
                pool.push((*r.path).clone());
            }
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
                    if let Some((from, to, t)) = origin_ids {
                        origins.insert(
                            g,
                            diff::Origin {
                                from,
                                to,
                                t,
                                bake: baked.is_some().then_some(Affine {
                                    e: 0,
                                    f: 0,
                                    ..r.transform
                                }),
                            },
                        );
                    }
                    if baked.is_some() {
                        if let Some(&base) = pool_index.get(&*r.path) {
                            let linear = Affine {
                                e: 0,
                                f: 0,
                                ..r.transform
                            };
                            provenance.insert(g, (base, linear));
                        }
                    }
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
    // The parent binding is always over the canonical literal model; the
    // stored STAT may be its operative form (gvi-operative.md §3).
    let literal_stat = encode_stat(model);
    let tran = encode_tran(model);
    let binding = binding_digest(&model.parent, &literal_stat, &tran);
    let mut stat = literal_stat;

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
    let indx = index::encode_indx(&keys, &state_proj, radix.domain);
    let keys_bytes = key_list_bytes(&keys);

    // Differential geometry (§6): choose records, then renumber so anchors
    // come first and every record references earlier ids only.
    let plan = (max_diff_depth > 0).then(|| {
        diff::plan(
            &pool,
            paths.len(),
            &provenance,
            &origins,
            albit != AlbitProfile::Provenance,
            max_diff_depth,
        )
    });
    if let Some(plan) = &plan {
        for p in &mut projections {
            for r in p.iter_mut() {
                r.1 = plan.remap[r.1 as usize];
            }
        }
    }

    let proj = encode_proj(&projections, operative != OperativeProfile::Off);

    let method = match model.reachability {
        Reachability::Product { .. } => METHOD_PRODUCT,
        Reachability::Transitions { .. } => METHOD_TRANSITION_CLOSURE,
    };
    let bake_flag = match bake {
        Bake::None => 0,
        Bake::Transforms => ASSUME_BAKED_TRANSFORMS,
    } | if plan.is_some() {
        ASSUME_DIFFERENTIAL_GEOMETRY
    } else {
        0
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

    let (anchors, diff_section, cost) = match &plan {
        None => (pool, None, None),
        Some(plan) => {
            let anchors: Vec<Path> = plan.order[..plan.anchors]
                .iter()
                .map(|&old| pool[old as usize].clone())
                .collect();
            let mut d = Vec::new();
            d.extend_from_slice(&(plan.records.len() as u32).to_le_bytes());
            d.extend_from_slice(&[0u8; 12]);
            let mut off = 0u32;
            d.extend_from_slice(&off.to_le_bytes());
            for rec in &plan.records {
                off += rec.len() as u32;
                d.extend_from_slice(&off.to_le_bytes());
            }
            pad16(&mut d);
            for rec in &plan.records {
                d.extend_from_slice(rec);
            }
            pad16(&mut d);
            // GVI-O: the columnar operative form, if smaller.
            if operative != OperativeProfile::Off {
                let views: Vec<diff::RecordView<'_>> = plan
                    .records
                    .iter()
                    .enumerate()
                    .map(|(k, rec)| diff::parse_record(rec, (plan.anchors + k) as u32))
                    .collect::<Result<_, _>>()?;
                let op = diff::encode_diff_operative(&views);
                if op.len() < d.len() {
                    d = op;
                }
            }
            let mut c = Vec::with_capacity(32);
            c.extend_from_slice(&max_diff_depth.to_le_bytes());
            c.extend_from_slice(&plan.max_depth.to_le_bytes());
            c.extend_from_slice(&plan.max_ancestors.to_le_bytes());
            c.extend_from_slice(&(plan.anchors as u32).to_le_bytes());
            c.extend_from_slice(&plan.points.to_le_bytes());
            c.push(if albit_on { albit.id() } else { 0 });
            c.extend_from_slice(&[0u8; 7]);
            (anchors, Some(d), Some(c))
        }
    };
    // GVI-O: the operative STAT, whose REF targets point into the final
    // anchor order, if smaller.
    if operative != OperativeProfile::Off {
        let op = encode_stat_operative(model, &anchors);
        if op.len() < stat.len() {
            stat = op;
        }
    }
    let (geom, hash) = encode_geometry_pool(&anchors)?;
    let styl = encode_style_pool(&styles);
    // GVI-L: the palette form, only when it is strictly shorter.
    let lsty = lineless::encode(&styles).filter(|l| l.len().div_ceil(16) < styl.len().div_ceil(16));
    if lsty.is_some() {
        let assumptions = u32_at(&cert, 72) | ASSUME_LINELESS_STYLES;
        cert[72..76].copy_from_slice(&assumptions.to_le_bytes());
    }
    let mut sections = vec![
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
        SectionInput::new(TRAN, &tran).align_log2(ALIGN_LOG2),
    ];
    if let Some(l) = &lsty {
        sections.push(SectionInput::new(LSTY, l).align_log2(ALIGN_LOG2));
    } else {
        sections.push(SectionInput::new(STYL, &styl).align_log2(ALIGN_LOG2));
    }
    if let (Some(d), Some(c)) = (&diff_section, &cost) {
        sections.push(SectionInput::new(DIFF, d).align_log2(ALIGN_LOG2));
        sections.push(SectionInput::new(COST, c).align_log2(ALIGN_LOG2));
    }
    let out = encode_container(
        Format::Gvi,
        Profile::Baseline,
        0,
        &sections,
        &gva_limits.container,
    )
    .map_err(|e| diag(e.kind.code(), e.to_string()))?;
    // Differential layout is used only when it makes the file smaller;
    // otherwise the complete layout is emitted. The choice is
    // deterministic, so recompiling with the same options reproduces it.
    let out = if plan.is_some() {
        let complete = compile_with(
            source,
            model,
            budget,
            gva_limits,
            CompileOptions {
                bake,
                max_diff_depth: 0,
                albit: AlbitProfile::Off,
                operative,
            },
        )?;
        if complete.len() <= out.len() {
            complete
        } else {
            out
        }
    } else {
        out
    };
    // GVI-A never-bigger (gvi-albit.md §5): the trajectory candidate must
    // beat the same compilation without it, or that compilation is
    // returned byte for byte.
    let out = if albit_on {
        let without = compile_with(
            source,
            model,
            budget,
            gva_limits,
            CompileOptions {
                bake,
                max_diff_depth,
                albit: AlbitProfile::Off,
                operative,
            },
        )?;
        if without.len() <= out.len() {
            without
        } else {
            out
        }
    } else {
        out
    };
    // GVI-O never-bigger (gvi-operative.md §5): the whole file must beat
    // the same compilation without operatives, or that compilation is
    // returned byte for byte.
    let out = if operative != OperativeProfile::Off {
        let without = compile_with(
            source,
            model,
            budget,
            gva_limits,
            CompileOptions {
                bake,
                max_diff_depth,
                albit,
                operative: OperativeProfile::Off,
            },
        )?;
        if without.len() <= out.len() {
            without
        } else {
            out
        }
    } else {
        out
    };
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
    /// Deepest differential chain accepted (§6).
    pub max_diff_depth: u32,
    /// Most ancestors any differential record may have.
    pub max_diff_ancestors: usize,
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
            max_diff_depth: MAX_DIFF_DEPTH,
            max_diff_ancestors: 4096,
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
    /// `INDX`: reachable keys and their projection ids (§4.2).
    index: index::Index<'a>,
    /// `PROJ`: projection start offsets into the row-id list.
    starts: packed::Column<'a>,
    /// Row ids per reference, or `None` when the list is the identity.
    row_list: Option<packed::Column<'a>>,
    /// Row pool columns: item, geometry, style, e, f, a, b, c, d, opacity.
    cols: Vec<packed::Column<'a>>,
    /// Native copies of the tables under expanded residency (§9.1).
    expanded: Option<Box<ExpandedTables>>,
    geom: GeometryPool<'a>,
    /// Differential records for geometry ids `geom.len()..` (§6).
    records: Vec<diff::RecordView<'a>>,
    /// `COST`: `(declared depth bound, max depth, max ancestors, reconstruction points)`.
    cost: Option<(u32, u32, u32, u64)>,
    /// The GVI-A profile recorded in `COST` (Off without `DIFF`).
    albit: AlbitProfile,
    /// Whether any section is stored in GVI-O operative form.
    operative: OperativeProfile,
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
        // Reconstruct the model first (bounded by the open limits and the
        // operative expansion budgets), then check the binding over its
        // canonical literal form, whatever form is stored.
        // An operative STAT may reference GEOM anchors (REF), so decode
        // them first. Literal STAT needs none.
        let stat_operative = stat.get(8) == Some(&STAT_OPERATIVE);
        let ref_anchors: Vec<Path> = if stat_operative {
            let pool = GeometryPool::parse(take(GEOM)?, &limits.gva)?;
            (0..pool.len())
                .map(|g| pool.get(g))
                .collect::<Result<_, _>>()?
        } else {
            Vec::new()
        };
        let dims = parse_stat(stat, limits.max_dimensions, &limits.gva, &ref_anchors)?;
        let reach = parse_tran(tran, dims.len())?;
        let canonical = StateModel {
            parent: String::new(),
            dimensions: dims.clone(),
            reachability: reach.clone(),
        };
        let literal_stat = encode_stat(&canonical);
        if binding_digest(&parent, &literal_stat, tran) != binding {
            return Err(cert_err(
                "stored state model does not match its parent binding",
            ));
        }
        // The stored model must be canonical in its own form: a path table
        // without duplicates or unused entries, in first-use order, and an
        // operative form exactly as the encoder emits it.
        let expected = if stat_operative {
            encode_stat_operative(&canonical, &ref_anchors)
        } else {
            literal_stat
        };
        if expected != stat || encode_tran(&canonical) != tran {
            return Err(diag(
                codes::NON_CANONICAL,
                "stored state model is not in canonical form",
            ));
        }

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
        // CERT bit 4 decides between STYL and LSTY (GVI-L).
        let certificate = parse_cert(take(CERT)?)?;
        let styles = match (
            certificate.assumptions & ASSUME_LINELESS_STYLES != 0,
            c.section(LSTY)
                .map_err(|e| diag(e.kind.code(), e.to_string()))?,
            c.section(STYL)
                .map_err(|e| diag(e.kind.code(), e.to_string()))?,
        ) {
            (true, Some(data), None) => lineless::decode(data, &limits.gva)?,
            (false, None, Some(data)) => parse_style_pool(data, &limits.gva)?,
            _ => return Err(cert_err("exactly one declared style section is required")),
        };

        // INDX
        let (index, all_keys, _) =
            index::Index::parse(take(INDX)?, radix.domain, limits.max_states)?;

        // PROJ
        let projb = take(PROJ)?;
        let mut r = Reader::new(projb, "PROJ");
        let np = r.u32()?;
        let nrows = r.u32()?;
        let total = r.u32()?;
        let flags = r.u32()?;
        if flags & !(PROJ_IDENTITY | PROJ_OPERATIVE) != 0 {
            return Err(malformed(PROJ, "unknown flags"));
        }
        if np > limits.max_projections || nrows > limits.max_rows || total > limits.max_rows {
            return Err(diag(
                codes::GVI_NOT_MANAGEABLE,
                "projection table exceeds the open limits",
            ));
        }
        let proj_operative = flags & PROJ_OPERATIVE != 0;
        let starts = packed::Column::parse_with(&mut r, np as usize + 1, proj_operative)?;
        let row_list = if flags & PROJ_IDENTITY != 0 {
            if total != nrows {
                return Err(malformed(
                    PROJ,
                    "an identity row list needs one reference per row",
                ));
            }
            None
        } else {
            Some(packed::Column::parse_with(
                &mut r,
                total as usize,
                proj_operative,
            )?)
        };
        let mut cols = Vec::with_capacity(10);
        for _ in 0..10 {
            cols.push(packed::Column::parse_with(
                &mut r,
                nrows as usize,
                proj_operative,
            )?);
        }
        r.finish()?;

        let differential = certificate.assumptions & ASSUME_DIFFERENTIAL_GEOMETRY != 0;
        let present = |tag: Tag| -> Result<Option<&'a [u8]>, GfxDiag> {
            c.section(tag)
                .map_err(|e| diag(e.kind.code(), e.to_string()))
        };
        let (records, cost, albit, diff_operative) = match (differential, present(DIFF)?, present(COST)?) {
            (false, None, None) => (Vec::new(), None, AlbitProfile::Off, false),
            (true, Some(d), Some(cst)) => {
                let (records, cost, albit) = parse_differential(d, cst, geom.len(), limits)?;
                (records, Some(cost), albit, d.get(4) == Some(&diff::DIFF_OPERATIVE))
            }
            _ => {
                return Err(cert_err(
                    "DIFF and COST must be present exactly when the certificate declares differential geometry",
                ))
            }
        };
        let inv = Inventory {
            bytes,
            parent,
            compiler,
            binding: ParentBinding(binding),
            source_digest,
            source_items,
            width: u32_at(srce, 40),
            height: u32_at(srce, 44),
            certificate,
            dims,
            reach,
            radix,
            index,
            starts,
            row_list,
            cols,
            expanded: None,
            geom,
            records,
            cost,
            albit,
            operative: if stat_operative || diff_operative || proj_operative {
                OperativeProfile::Fast
            } else {
                OperativeProfile::Off
            },
            styles,
        };
        // Inventories with operative PROJ columns are always expanded, so
        // the packed read path never sees a decoded column.
        let mut inv = inv;
        if inv.cols.iter().any(packed::Column::is_operative)
            || inv
                .row_list
                .as_ref()
                .is_some_and(packed::Column::is_operative)
            || inv.starts.is_operative()
        {
            inv.set_residency(Residency::Expanded);
        }
        inv.check_structure(np, total, &all_keys)?;
        inv.check_completeness(limits, &all_keys)?;
        Ok(inv)
    }

    fn proj_of(&self, i: usize) -> u32 {
        match self.expanded.as_ref().and_then(|x| x.proj.as_ref()) {
            Some(p) => p[i],
            None => self.index.proj_of(i),
        }
    }

    fn start(&self, p: usize) -> u32 {
        match &self.expanded {
            Some(x) => x.starts[p],
            None => self.starts.get(p) as u32,
        }
    }

    /// Pool row of reference `r`.
    fn row_id(&self, r: usize) -> usize {
        match &self.expanded {
            Some(x) => x.row_list.as_ref().map_or(r, |l| l[r] as usize),
            None => match &self.row_list {
                None => r,
                Some(l) => l.get(r) as usize,
            },
        }
    }

    fn row_u32(&self, field: usize, row: usize) -> u32 {
        match (&self.expanded, field) {
            (Some(x), 0) => x.item[row],
            (Some(x), 1) => x.geometry[row],
            (Some(x), 2) => x.style[row],
            _ => self.cols[field].get(row) as u32,
        }
    }

    fn row_opacity(&self, row: usize) -> u8 {
        match &self.expanded {
            Some(x) => x.opacity[row],
            None => self.cols[9].get(row) as u8,
        }
    }

    fn row_transform(&self, row: usize) -> Affine {
        match &self.expanded {
            Some(x) => x.transform[row],
            None => self.packed_transform(row),
        }
    }

    fn packed_transform(&self, row: usize) -> Affine {
        let f = |k: usize| self.cols[k].get(row) as i32;
        Affine {
            e: f(3),
            f: f(4),
            a: f(5),
            b: f(6),
            c: f(7),
            d: f(8),
        }
    }

    pub fn state_count(&self) -> usize {
        self.index.len()
    }

    /// Choose how the packed `PROJ` and `INDX` tables are held in memory
    /// (§9.1). `Expanded` unpacks them into native arrays: faster reads,
    /// more heap. `Packed`, the default, reads straight from the file
    /// bytes. Results are identical either way.
    pub fn set_residency(&mut self, residency: Residency) {
        self.expanded = match residency {
            // Operative PROJ columns have no packed form: such inventories
            // stay expanded.
            Residency::Packed
                if self.cols.iter().any(packed::Column::is_operative)
                    || self.starts.is_operative()
                    || self
                        .row_list
                        .as_ref()
                        .is_some_and(packed::Column::is_operative) =>
            {
                return
            }
            Residency::Packed => None,
            Residency::Expanded => {
                let col_u32 = |c: &packed::Column<'_>| -> Vec<u32> {
                    c.values().into_iter().map(|v| v as u32).collect()
                };
                let n = self.cols[0].len();
                Some(Box::new(ExpandedTables {
                    starts: col_u32(&self.starts),
                    row_list: self.row_list.as_ref().map(col_u32),
                    item: col_u32(&self.cols[0]),
                    geometry: col_u32(&self.cols[1]),
                    style: col_u32(&self.cols[2]),
                    transform: (0..n)
                        .map(|r| {
                            let f = |k: usize| self.cols[k].at(r) as i32;
                            Affine {
                                e: f(3),
                                f: f(4),
                                a: f(5),
                                b: f(6),
                                c: f(7),
                                d: f(8),
                            }
                        })
                        .collect(),
                    opacity: self.cols[9].values().into_iter().map(|v| v as u8).collect(),
                    proj: self.index.proj_ids(),
                    keys: self.index.packed_keys(),
                }))
            }
        };
    }

    pub fn residency(&self) -> Residency {
        if self.expanded.is_some() {
            Residency::Expanded
        } else {
            Residency::Packed
        }
    }

    /// Memory held by the `PROJ` and `INDX` tables: `(packed bytes
    /// borrowed from the file, heap bytes)`. Heap covers dictionaries and
    /// the expanded tables, if present.
    pub fn table_residency(&self) -> (usize, usize) {
        let mut cols: Vec<&packed::Column<'a>> = self.cols.iter().collect();
        cols.push(&self.starts);
        if let Some(l) = &self.row_list {
            cols.push(l);
        }
        cols.extend(self.index.columns());
        let (file, mut heap) = cols.iter().fold((0, 0), |(p, h), c| {
            (p + c.packed_bytes(), h + c.heap_bytes())
        });
        if let Some(x) = &self.expanded {
            heap += x.heap_bytes();
        }
        (file, heap)
    }

    pub fn projection_count(&self) -> usize {
        self.starts.len() - 1
    }

    /// The source drawing's canvas in millipoints (`SRCE` width, height).
    pub fn canvas(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn dimensions(&self) -> &[Dimension] {
        &self.dims
    }

    pub fn reachability(&self) -> &Reachability {
        &self.reach
    }

    /// Index, projection and row references, plus the certificate figures.
    fn check_structure(&self, np: u32, total: u32, keys: &[u64]) -> Result<(), GfxDiag> {
        let n = self.state_count();
        let mut used = vec![false; np as usize];
        // Keys were checked ascending and inside the domain by Index::parse.
        for i in 0..n {
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
        // Row-id list: in range, each new row first used in pool order, and
        // the identity form used exactly when possible.
        let nrows = self.cols[0].len();
        if let Some(list) = &self.row_list {
            if list.len() <= nrows {
                return Err(diag(
                    codes::NON_CANONICAL,
                    "PROJ stores an identity row list explicitly",
                ));
            }
            let mut next = 0usize;
            for k in 0..list.len() {
                let id = list.at(k);
                if id < 0 || id as usize > next || id as usize >= nrows {
                    return Err(diag(
                        codes::NON_CANONICAL,
                        "PROJ row ids are not in first-use order",
                    ));
                }
                if id as usize == next {
                    next += 1;
                }
            }
            if next != nrows {
                return Err(diag(codes::NON_CANONICAL, "PROJ holds an unreferenced row"));
            }
        }
        let mut distinct = std::collections::HashSet::with_capacity(nrows);
        for row in 0..nrows {
            let tuple: Vec<i64> = self.cols.iter().map(|c| c.at(row)).collect();
            if !distinct.insert(tuple) {
                return Err(diag(codes::NON_CANONICAL, "PROJ holds a duplicate row"));
            }
            for f in 3..9 {
                if i32::try_from(self.cols[f].at(row)).is_err() {
                    return Err(malformed(PROJ, "transform value is out of range"));
                }
            }
            if !(0..=255).contains(&self.cols[9].at(row)) {
                return Err(malformed(PROJ, "opacity is out of range"));
            }
            for f in 0..3 {
                if !(0..=i64::from(u32::MAX)).contains(&self.cols[f].at(row)) {
                    return Err(malformed(PROJ, "reference is out of range"));
                }
            }
        }
        let (gc, sc) = (self.geometry_count() as u32, self.styles.len() as u32);
        for row in 0..nrows {
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
        let known = required
            | ASSUME_BAKED_TRANSFORMS
            | ASSUME_DIFFERENTIAL_GEOMETRY
            | ASSUME_LINELESS_STYLES;
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
            || c.coverage_digest != sha256(&[&key_list_bytes(keys)])
        {
            return Err(cert_err("certificate figures do not match the inventory"));
        }
        Ok(())
    }

    fn contains(&self, key: u64) -> bool {
        self.index_of(key).is_some()
    }

    fn index_of(&self, key: u64) -> Option<usize> {
        match self.expanded.as_ref().and_then(|x| x.keys.as_ref()) {
            Some(keys) => keys.binary_search(&key).ok(),
            None => self.index.index_of(key),
        }
    }

    /// Re-derive completeness from the stored model, using a different
    /// argument from the compiler's enumeration:
    ///
    /// - **Product:** every index key avoids every exclusion, and the count
    ///   equals the domain size minus the excluded combinations.
    /// - **Transitions:** the initial state is indexed, the index is closed
    ///   under every rule, and every indexed state is reached by walking
    ///   rules from the initial state within the index.
    fn check_completeness(&self, limits: &GviLimits, keys: &[u64]) -> Result<(), GfxDiag> {
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
                    let state = self.radix.unpack(keys[i]);
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
    /// Decode every geometry once, for presentation: anchors directly, and
    /// differential records in id order from their already-built bases.
    /// Each record's hash proves its reconstruction exact.
    pub fn decode_geometry(&self) -> Result<Vec<Arc<Path>>, GfxDiag> {
        let mut paths: Vec<Arc<Path>> = Vec::with_capacity(self.geometry_count());
        for g in 0..self.geom.len() {
            paths.push(Arc::new(self.geom.get(g)?));
        }
        let mut points = 0u64;
        for rec in &self.records {
            let p = {
                let get = |id: u32| paths.get(id as usize).map(|a| &**a);
                diff::reconstruct(rec, &get)?
            };
            points += p.points().count() as u64;
            paths.push(Arc::new(p));
        }
        if self.cost.is_some_and(|c| c.3 != points) {
            return Err(cert_err("COST misstates the reconstruction work"));
        }
        Ok(paths)
    }

    /// Total geometries: complete anchors plus differential records.
    pub fn geometry_count(&self) -> usize {
        self.geom.len() + self.records.len()
    }

    /// How geometry is stored (§6).
    pub fn geometry_layout(&self) -> GeometryLayout {
        let (depth_bound, max_depth, max_ancestors, points) = self.cost.unwrap_or((0, 0, 0, 0));
        GeometryLayout {
            anchors: self.geom.len(),
            records: self.records.len(),
            depth_bound,
            max_depth,
            max_ancestors,
            reconstruction_points: points,
            trajectory_records: self
                .records
                .iter()
                .filter(|r| r.pred.is_trajectory())
                .count(),
            albit: self.albit,
        }
    }

    /// Rebuild one geometry lazily, touching only its ancestors. This costs
    /// at most `max_ancestors + 1` geometries (the Bounded Differential
    /// Traversal contract).
    pub fn geometry(&self, g: u32) -> Result<Path, GfxDiag> {
        let anchors = self.geom.len() as u32;
        if g < anchors {
            return self.geom.get(g as usize);
        }
        let mut need = vec![g];
        let mut stack = vec![g];
        while let Some(id) = stack.pop() {
            if id >= anchors {
                let rec = self.records.get((id - anchors) as usize).ok_or_else(|| {
                    diag(codes::GEOMETRY_REF, format!("geometry {id} does not exist"))
                })?;
                for b in rec.pred.bases() {
                    if !need.contains(&b) {
                        need.push(b);
                        stack.push(b);
                    }
                }
            }
        }
        need.sort_unstable();
        let mut built: HashMap<u32, Path> = HashMap::with_capacity(need.len());
        for id in need {
            let p = if id < anchors {
                self.geom.get(id as usize)?
            } else {
                let get = |b: u32| built.get(&b);
                diff::reconstruct(&self.records[(id - anchors) as usize], &get)?
            };
            built.insert(id, p);
        }
        built
            .remove(&g)
            .ok_or_else(|| diag(codes::GEOMETRY_REF, format!("geometry {g} does not exist")))
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
        let options = CompileOptions {
            bake,
            max_diff_depth: self.cost.map_or(0, |c| c.0),
            albit: self.albit,
            operative: self.operative,
        };
        if compile_with(source, model, budget, gva_limits, options)? != self.bytes {
            return Err(diag(
                codes::NON_CANONICAL,
                "inventory differs from the canonical compilation",
            ));
        }
        Ok(())
    }
}

/// `PROJ` and `INDX` unpacked into native arrays (§9.1).
#[derive(Debug, Clone)]
struct ExpandedTables {
    starts: Vec<u32>,
    row_list: Option<Vec<u32>>,
    item: Vec<u32>,
    geometry: Vec<u32>,
    style: Vec<u32>,
    transform: Vec<Affine>,
    opacity: Vec<u8>,
    /// `None` when projection ids are the identity.
    proj: Option<Vec<u32>>,
    /// `Some` only when keys are a packed list; identity and bitmap keys
    /// are already O(1).
    keys: Option<Vec<u64>>,
}

impl ExpandedTables {
    fn heap_bytes(&self) -> usize {
        4 * (self.starts.len()
            + self.row_list.as_ref().map_or(0, Vec::len)
            + self.item.len()
            + self.geometry.len()
            + self.style.len()
            + self.proj.as_ref().map_or(0, Vec::len))
            + std::mem::size_of::<Affine>() * self.transform.len()
            + self.opacity.len()
            + 8 * self.keys.as_ref().map_or(0, Vec::len)
    }
}

/// How the packed `PROJ` and `INDX` tables are held in memory (§9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Residency {
    /// Read straight from the (possibly memory-mapped) file bytes.
    Packed,
    /// Unpack every column into native arrays at the cost of heap memory.
    Expanded,
}

/// How an inventory's geometry is stored (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryLayout {
    /// Complete geometries in `GEOM`.
    pub anchors: usize,
    /// Differential records in `DIFF`.
    pub records: usize,
    /// The declared Bounded Differential Traversal depth (0 = none).
    pub depth_bound: u32,
    pub max_depth: u32,
    /// Most geometries any single lazy reconstruction touches, besides
    /// the target itself.
    pub max_ancestors: u32,
    /// Points rebuilt when every record is reconstructed.
    pub reconstruction_points: u64,
    /// How many records are GVI-A trajectory samples.
    pub trajectory_records: usize,
    /// The GVI-A profile the inventory was compiled with.
    pub albit: AlbitProfile,
}

/// `COST` fields: declared depth bound, max depth, max ancestors and
/// reconstruction points.
type Cost = (u32, u32, u32, u64);

/// Parse `DIFF` and `COST`, and check the record graph against the declared
/// bounds: bases are earlier ids, and depth and ancestor counts match `COST`
/// and stay within the open limits.
#[allow(clippy::type_complexity)]
fn parse_differential<'a>(
    d: &'a [u8],
    cost: &[u8],
    anchors: usize,
    limits: &GviLimits,
) -> Result<(Vec<diff::RecordView<'a>>, Cost, AlbitProfile), GfxDiag> {
    if cost.len() != 32 || cost[25..32] != [0; 7] {
        return Err(malformed(COST, "wrong length or non-zero reserved bytes"));
    }
    let albit =
        AlbitProfile::from_id(cost[24]).ok_or_else(|| malformed(COST, "unknown GVI-A profile"))?;
    let (bound, max_depth, max_anc, cost_anchors) = (
        u32_at(cost, 0),
        u32_at(cost, 4),
        u32_at(cost, 8),
        u32_at(cost, 12),
    );
    let points = u64_at(cost, 16);
    if bound == 0 || bound > MAX_DIFF_DEPTH.min(limits.max_diff_depth) || max_depth > bound {
        return Err(cert_err(format!(
            "differential depth bound {bound} is not acceptable"
        )));
    }
    if cost_anchors as usize != anchors {
        return Err(cert_err("COST anchor count disagrees with GEOM"));
    }
    let n = u32_at(d.get(..4).ok_or_else(|| malformed(DIFF, "truncated"))?, 0) as usize;
    if anchors + n > limits.gva.max_geometries as usize {
        return Err(diag(codes::TOO_MANY_GEOMETRIES, "too many geometries"));
    }
    let parsed: Vec<diff::RecordView<'a>> = match d.get(4) {
        Some(&diff::DIFF_OPERATIVE) => {
            let records = diff::parse_diff_operative(d, anchors, n)?;
            if diff::encode_diff_operative(&records) != d {
                return Err(diag(
                    codes::NON_CANONICAL,
                    "operative DIFF is not canonically encoded",
                ));
            }
            records
        }
        Some(&diff::DIFF_RECORDS) => {
            let mut r = Reader::new(d, "DIFF");
            r.u32()?;
            r.zeros(12)?;
            let table = r.bytes((n + 1) * 4)?;
            r.zeros((16 - ((n + 1) * 4) % 16) % 16)?;
            let offsets: Vec<usize> = table
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize)
                .collect();
            if offsets[0] != 0 || offsets.windows(2).any(|w| w[1] <= w[0]) {
                return Err(malformed(
                    DIFF,
                    "record offsets must start at 0 and increase",
                ));
            }
            let blob = r.bytes(offsets[n])?;
            r.finish()?;
            (0..n)
                .map(|k| {
                    diff::parse_record(&blob[offsets[k]..offsets[k + 1]], (anchors + k) as u32)
                })
                .collect::<Result<_, _>>()?
        }
        _ => return Err(malformed(DIFF, "unknown DIFF encoding")),
    };
    let mut records = Vec::with_capacity(n);
    let mut depth: Vec<u32> = vec![0; anchors + n];
    let mut ancestors: Vec<Vec<u32>> = vec![Vec::new(); anchors + n];
    let (mut seen_depth, mut seen_anc) = (0u32, 0u32);
    for (k, rec) in parsed.into_iter().enumerate() {
        let id = (anchors + k) as u32;
        if rec.pred.is_trajectory() && albit == AlbitProfile::Off {
            return Err(cert_err(
                "a GVI-A trajectory record in an inventory that declares none",
            ));
        }
        let bases = rec.pred.bases();
        depth[id as usize] = 1 + bases.iter().map(|&b| depth[b as usize]).max().unwrap_or(0);
        let mut anc: Vec<u32> = Vec::new();
        for &b in &bases {
            anc.push(b);
            anc.extend_from_slice(&ancestors[b as usize]);
        }
        anc.sort_unstable();
        anc.dedup();
        if anc.len() > limits.max_diff_ancestors {
            return Err(cert_err("a record has more ancestors than the open limit"));
        }
        seen_depth = seen_depth.max(depth[id as usize]);
        seen_anc = seen_anc.max(anc.len() as u32);
        ancestors[id as usize] = anc;
        records.push(rec);
    }
    if seen_depth != max_depth || seen_anc != max_anc || seen_depth > bound {
        return Err(cert_err(
            "COST misstates the differential depth or ancestor bound",
        ));
    }
    Ok((records, (bound, max_depth, max_anc, points), albit))
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
        self.inv.row_u32(field, self.inv.row_id(self.start + i))
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
        self.inv.row_transform(self.inv.row_id(self.start + i))
    }

    pub fn opacity(&self, i: usize) -> u8 {
        self.inv.row_opacity(self.inv.row_id(self.start + i))
    }

    /// Build the drawing for presentation from pre-decoded geometry. The
    /// residency is checked once per drawing, not once per field.
    pub fn drawing(&self, paths: &[Arc<Path>]) -> Drawing {
        let inv = self.inv;
        let items = match &inv.expanded {
            Some(x) => (self.start..self.end)
                .map(|k| {
                    let r = x.row_list.as_ref().map_or(k, |l| l[k] as usize);
                    Item {
                        path: paths[x.geometry[r] as usize].clone(),
                        style: inv.styles[x.style[r] as usize],
                        transform: x.transform[r],
                        opacity: x.opacity[r],
                    }
                })
                .collect(),
            None => (self.start..self.end)
                .map(|k| {
                    // One row-id lookup per row, then direct column reads.
                    let r = match &inv.row_list {
                        None => k,
                        Some(l) => l.get(k) as usize,
                    };
                    Item {
                        path: paths[inv.cols[1].get(r) as usize].clone(),
                        style: inv.styles[inv.cols[2].get(r) as usize],
                        transform: inv.packed_transform(r),
                        opacity: inv.cols[9].get(r) as u8,
                    }
                })
                .collect(),
        };
        Drawing {
            width: inv.width,
            height: inv.height,
            items,
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

    #[test]
    fn lineless_style_pool_compiles_and_opens() {
        let values = (1..=100)
            .map(|width| {
                let mut style = source().items[2].style;
                style.stroke.as_mut().unwrap().width = width;
                Value::new(&format!("width-{width}"))
                    .with(Effect::Restyle(Selector::Items(vec![2]), style))
            })
            .collect();
        let model = StateModel {
            parent: "scene://lineless".into(),
            dimensions: vec![Dimension::discrete("width", values)],
            reachability: Reachability::Product { exclusions: vec![] },
        };
        let bytes = compile(&source(), &model, &bud(), &gl()).unwrap();
        let inv = Inventory::open(&bytes, &model.binding(4, &bud()).unwrap(), &lim()).unwrap();
        assert_ne!(inv.certificate.assumptions & ASSUME_LINELESS_STYLES, 0);
        assert_eq!(inv.styles().len(), 104);
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
        let domain = Radix::new(
            Inventory::open(
                bytes,
                &ParentBinding(c.section(PARN).unwrap().unwrap()[..32].try_into().unwrap()),
                &lim(),
            )
            .unwrap()
            .dimensions()
            .iter()
            .map(|dim| match &dim.domain {
                Domain::Discrete(v) => v.len() as u64,
                _ => 1,
            })
            .collect(),
        )
        .unwrap()
        .domain;
        let (_, keys, ids) = index::Index::parse(d, domain, u32::MAX).unwrap();
        let n = keys.len();
        let kept: Vec<(u64, u32)> = keys
            .into_iter()
            .zip(ids)
            .filter(|&(k, _)| k != key)
            .collect();
        assert_eq!(kept.len(), n - 1, "key {key} was not indexed");
        let ks: Vec<u64> = kept.iter().map(|k| k.0).collect();
        let ps: Vec<u32> = kept.iter().map(|k| k.1).collect();
        let indx = index::encode_indx(&ks, &ps, domain);
        let digest = sha256(&[&key_list_bytes(&ks)]);
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

    /// Every projection's rows, read back through the public accessors.
    fn projection_rows(inv: &Inventory<'_>) -> Vec<Vec<ProjRow>> {
        (0..inv.projection_count())
            .map(|p| {
                let (s, e) = (inv.start(p) as usize, inv.start(p + 1) as usize);
                let view = Projection {
                    inv,
                    id: p as u32,
                    start: s,
                    end: e,
                };
                (0..view.len())
                    .map(|i| {
                        (
                            view.item(i),
                            view.geometry(i),
                            inv.row_u32(2, inv.row_id(s + i)),
                            view.transform(i),
                            view.opacity(i),
                        )
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn residency_does_not_change_results() {
        for m in [product_model(), transitions_model()] {
            let bytes = compile(&source(), &m, &bud(), &gl()).unwrap();
            let mut inv = Inventory::open(&bytes, &binding(&m), &lim()).unwrap();
            let paths = inv.decode_geometry().unwrap();
            let radix = Radix::new(
                m.dimensions
                    .iter()
                    .map(|d| match &d.domain {
                        Domain::Discrete(v) => v.len() as u64,
                        _ => 1,
                    })
                    .collect(),
            )
            .unwrap();
            let snapshot = |inv: &Inventory<'_>| -> Vec<Option<Drawing>> {
                (0..radix.domain)
                    .map(|k| {
                        inv.resolve(&radix.unpack(k))
                            .ok()
                            .map(|p| p.drawing(&paths))
                    })
                    .collect()
            };
            let packed = snapshot(&inv);
            let (_, heap_packed) = inv.table_residency();
            inv.set_residency(Residency::Expanded);
            assert_eq!(snapshot(&inv), packed);
            let (_, heap_expanded) = inv.table_residency();
            assert!(heap_expanded > heap_packed);
            inv.set_residency(Residency::Packed);
            assert_eq!(inv.table_residency().1, heap_packed);
            assert_eq!(snapshot(&inv), packed);
        }
    }

    #[test]
    fn proj_round_trips_and_rejects_non_canonical_tables() {
        let m = product_model();
        let base = compile(&source(), &m, &bud(), &gl()).unwrap();
        let inv = Inventory::open(&base, &binding(&m), &lim()).unwrap();
        let rows = projection_rows(&inv);
        // Re-encoding what was read reproduces PROJ exactly.
        let c = Container::open(
            &base,
            &Expect {
                format: Format::Gvi,
                known_sections: &KNOWN,
            },
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(encode_proj(&rows, false), c.section(PROJ).unwrap().unwrap());
        // Shared rows are pooled: the backdrop row appears in all 24
        // projections but is stored once.
        assert!(inv.cols[0].len() < rows.iter().map(Vec::len).sum::<usize>());
        // Byte 12 of the header is the identity flag; this model shares rows,
        // so it needs an explicit row list.
        assert_eq!(encode_proj(&rows, false)[12], 0);
        let mut reordered = rows.clone();
        reordered.swap(0, 1);
        let swapped = encode_proj(&reordered, false);
        let tampered = rebuild(&base, &mut |tag, d| {
            if tag == PROJ {
                *d = swapped.clone();
            }
        });
        // Swapping projections keeps the table canonical but makes INDX
        // point at the wrong appearances; verify_against notices.
        let inv2 = Inventory::open(&tampered, &binding(&m), &lim()).unwrap();
        assert_eq!(
            inv2.verify_against(&source(), &m, &bud(), &gl())
                .unwrap_err()
                .code,
            codes::NON_CANONICAL
        );
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
        // Projection rows pointing at a missing style, re-encoded
        // canonically so only the reference check can object.
        let inv = Inventory::open(&base, &binding(&m), &lim()).unwrap();
        let mut rows = projection_rows(&inv);
        rows[0][0].2 = 999;
        let forged = encode_proj(&rows, false);
        let dangling = rebuild(&base, &mut |tag, d| {
            if tag == PROJ {
                *d = forged.clone();
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

    // -----------------------------------------------------------------------
    // Differential geometry (GVI-D)
    // -----------------------------------------------------------------------

    /// A closed path of 32 cubics with end points at `radius(i)` around
    /// (50 pt, 50 pt). Star and circle share this structure, so they morph.
    fn ring(radius: impl Fn(usize) -> f64) -> Path {
        let p = |i: usize| {
            let a = (i % 32) as f64 / 32.0 * std::f64::consts::TAU;
            let r = radius(i % 32);
            pt(
                (50_000.0 + r * a.cos()).round() as i32,
                (50_000.0 + r * a.sin()).round() as i32,
            )
        };
        let mut segs = vec![Segment::MoveTo(p(0))];
        for i in 0..32 {
            let (a, b) = (p(i), p(i + 1));
            let c1 = pt(a.x + (b.x - a.x) / 3, a.y + (b.y - a.y) / 3);
            let c2 = pt(a.x + 2 * (b.x - a.x) / 3, a.y + 2 * (b.y - a.y) / 3);
            segs.push(Segment::CubicTo(c1, c2, b));
        }
        segs.push(Segment::Close);
        Path::new(segs).unwrap()
    }

    fn ring_source() -> Drawing {
        Drawing {
            width: 100_000,
            height: 100_000,
            items: vec![item(
                ring(|i| if i % 2 == 0 { 40_000.0 } else { 22_000.0 }),
                fill(BLUE),
            )],
        }
    }

    /// 12 morph frames × 3 relative sizes: 36 states.
    fn ring_model() -> StateModel {
        let circle = Arc::new(ring(|_| 32_000.0));
        let frames = (0..12u32)
            .map(|k| {
                Value::new(&format!("f{k}")).with(Effect::Morph(
                    Selector::All,
                    circle.clone(),
                    k * 65_536 / 11,
                ))
            })
            .collect();
        let scale = |q: i32| {
            let e = 50_000 - (i64::from(q) * 50_000 / i64::from(FIXED_ONE)) as i32;
            Affine {
                a: q,
                b: 0,
                c: 0,
                d: q,
                e,
                f: e,
            }
        };
        StateModel {
            parent: "scene://loader".into(),
            dimensions: vec![
                Dimension::discrete("frame", frames),
                Dimension::discrete(
                    "size",
                    vec![
                        Value::new("small")
                            .with(Effect::Transform(Selector::All, scale(FIXED_ONE / 2))),
                        Value::new("medium")
                            .with(Effect::Transform(Selector::All, scale(FIXED_ONE * 3 / 4))),
                        Value::new("full"),
                    ],
                ),
            ],
            reachability: Reachability::Product { exclusions: vec![] },
        }
    }

    fn ring_binding(m: &StateModel) -> ParentBinding {
        m.binding(1, &bud()).unwrap()
    }

    fn opts(bake: Bake, depth: u32) -> CompileOptions {
        CompileOptions {
            bake,
            max_diff_depth: depth,
            albit: AlbitProfile::Off,
            operative: OperativeProfile::Off,
        }
    }

    #[test]
    fn differential_inventories_resolve_identically() {
        let m = ring_model();
        let radix = Radix::new(vec![12, 3]).unwrap();
        let mut used = 0;
        for bake in [Bake::None, Bake::Transforms] {
            let full = compile_with(&ring_source(), &m, &bud(), &gl(), bake).unwrap();
            let fi = Inventory::open(&full, &ring_binding(&m), &lim()).unwrap();
            let fp = fi.decode_geometry().unwrap();
            for depth in [1, 4, 8] {
                let bytes =
                    compile_with(&ring_source(), &m, &bud(), &gl(), opts(bake, depth)).unwrap();
                let di = Inventory::open(&bytes, &ring_binding(&m), &lim()).unwrap();
                let layout = di.geometry_layout();
                assert!(layout.max_depth <= depth, "{layout:?}");
                assert_eq!(layout.anchors + layout.records, fi.geometry_count());
                // Never larger; identical to the complete layout when the
                // differential one would not be smaller.
                assert!(
                    bytes.len() <= full.len(),
                    "{bake:?}/{depth}: {} > {}",
                    bytes.len(),
                    full.len()
                );
                if layout.records == 0 {
                    assert_eq!(bytes, full);
                }
                used += layout.records;
                let dp = di.decode_geometry().unwrap();
                for key in 0..radix.domain {
                    let s = radix.unpack(key);
                    assert_eq!(
                        di.resolve(&s).unwrap().drawing(&dp),
                        fi.resolve(&s).unwrap().drawing(&fp),
                        "{bake:?}/{depth} state {s:?}"
                    );
                }
                // Lazy reconstruction equals eager reconstruction.
                for g in 0..di.geometry_count() as u32 {
                    assert_eq!(di.geometry(g).unwrap(), *dp[g as usize]);
                }
                di.verify_against(&ring_source(), &m, &bud(), &gl())
                    .unwrap();
            }
        }
        assert!(used > 0, "no configuration used differential records");
        assert_eq!(
            compile_with(
                &ring_source(),
                &m,
                &bud(),
                &gl(),
                opts(Bake::None, MAX_DIFF_DEPTH + 1)
            )
            .unwrap_err()
            .code,
            codes::GVI_INVALID_MODEL
        );
    }

    // -----------------------------------------------------------------------
    // GVI-A trajectory records
    // -----------------------------------------------------------------------

    fn albit(bake: Bake, depth: u32, profile: AlbitProfile) -> CompileOptions {
        CompileOptions {
            bake,
            max_diff_depth: depth,
            albit: profile,
            operative: OperativeProfile::Off,
        }
    }

    /// The ring model with frame fractions given explicitly.
    fn ring_model_with(fractions: &[u32]) -> StateModel {
        let mut m = ring_model();
        let circle = Arc::new(ring(|_| 32_000.0));
        m.dimensions[0] = Dimension::discrete(
            "frame",
            fractions
                .iter()
                .enumerate()
                .map(|(k, &t)| {
                    Value::new(&format!("f{k}")).with(Effect::Morph(
                        Selector::All,
                        circle.clone(),
                        t,
                    ))
                })
                .collect(),
        );
        m
    }

    fn all_drawings(inv: &Inventory<'_>, m: &StateModel) -> Vec<Option<Drawing>> {
        let paths = inv.decode_geometry().unwrap();
        let radix = Radix::new(
            m.dimensions
                .iter()
                .map(|d| match &d.domain {
                    Domain::Discrete(v) => v.len() as u64,
                    _ => 1,
                })
                .collect(),
        )
        .unwrap();
        (0..radix.domain)
            .map(|k| {
                inv.resolve(&radix.unpack(k))
                    .ok()
                    .map(|p| p.drawing(&paths))
            })
            .collect()
    }

    #[test]
    fn trajectory_records_are_exact_and_directly_addressed() {
        // Uniform and eased (nonuniform) fractions.
        let uniform: Vec<u32> = (0..12u32).map(|k| k * 65_536 / 11).collect();
        let eased: Vec<u32> = (0..12u64).map(|k| (65_536 * k * k / 121) as u32).collect();
        for fractions in [uniform, eased] {
            let m = ring_model_with(&fractions);
            let b = ring_binding(&m);
            for bake in [Bake::None, Bake::Transforms] {
                let plain = compile_with(&ring_source(), &m, &bud(), &gl(), bake).unwrap();
                let reference = all_drawings(&Inventory::open(&plain, &b, &lim()).unwrap(), &m);
                let gvid = compile_with(
                    &ring_source(),
                    &m,
                    &bud(),
                    &gl(),
                    albit(bake, 8, AlbitProfile::Off),
                )
                .unwrap();
                for profile in [AlbitProfile::Provenance, AlbitProfile::Exhaustive] {
                    let bytes =
                        compile_with(&ring_source(), &m, &bud(), &gl(), albit(bake, 8, profile))
                            .unwrap();
                    // Never bigger than GVI-D with the same depth.
                    assert!(
                        bytes.len() <= gvid.len(),
                        "{bake:?} {profile:?}: {} > {}",
                        bytes.len(),
                        gvid.len()
                    );
                    let inv = Inventory::open(&bytes, &b, &lim()).unwrap();
                    let layout = inv.geometry_layout();
                    assert!(
                        layout.trajectory_records > 0,
                        "{bake:?} {profile:?}: {layout:?}"
                    );
                    assert_eq!(layout.albit, profile);
                    // Every state equals the plain inventory.
                    assert_eq!(all_drawings(&inv, &m), reference, "{bake:?} {profile:?}");
                    // Lazy equals eager.
                    let eager = inv.decode_geometry().unwrap();
                    for g in 0..inv.geometry_count() as u32 {
                        assert_eq!(inv.geometry(g).unwrap(), *eager[g as usize]);
                    }
                    inv.verify_against(&ring_source(), &m, &bud(), &gl())
                        .unwrap();
                    if profile == AlbitProfile::Provenance {
                        // Direct addressing: every sample needs only its
                        // two endpoints, whatever its frame.
                        assert!(layout.max_ancestors <= 2, "{layout:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn trajectories_never_make_files_bigger() {
        // Every frame morphs fully onto a different target: there is no
        // shared trajectory, so GVI-A must return the GVI-D bytes exactly.
        let m = {
            let mut m = ring_model();
            m.dimensions[0] = Dimension::discrete(
                "frame",
                (0..6u32)
                    .map(|k| {
                        let target = Arc::new(ring(move |i| {
                            20_000.0 + f64::from((i as u32 * 7 + k * 13) % 17) * 1_000.0
                        }));
                        Value::new(&format!("f{k}")).with(Effect::Morph(
                            Selector::All,
                            target,
                            65_536,
                        ))
                    })
                    .collect(),
            );
            m
        };
        for bake in [Bake::None, Bake::Transforms] {
            let gvid = compile_with(
                &ring_source(),
                &m,
                &bud(),
                &gl(),
                albit(bake, 8, AlbitProfile::Off),
            )
            .unwrap();
            let a = compile_with(
                &ring_source(),
                &m,
                &bud(),
                &gl(),
                albit(bake, 8, AlbitProfile::Provenance),
            )
            .unwrap();
            assert_eq!(
                a, gvid,
                "{bake:?}: GVI-A without benefit must be byte-identical to GVI-D"
            );
        }
        // Depth 0 disables records, so GVI-A has no effect.
        let m = ring_model();
        assert_eq!(
            compile_with(
                &ring_source(),
                &m,
                &bud(),
                &gl(),
                albit(Bake::None, 0, AlbitProfile::Provenance)
            )
            .unwrap(),
            compile(&ring_source(), &m, &bud(), &gl()).unwrap()
        );
    }

    #[test]
    fn trajectory_tampering_is_caught() {
        let m = ring_model();
        let base = compile_with(
            &ring_source(),
            &m,
            &bud(),
            &gl(),
            albit(Bake::None, 8, AlbitProfile::Provenance),
        )
        .unwrap();
        let b = ring_binding(&m);
        assert!(
            Inventory::open(&base, &b, &lim())
                .unwrap()
                .geometry_layout()
                .trajectory_records
                > 0
        );
        // COST claims no GVI-A profile while DIFF holds trajectory records.
        let undeclared = rebuild(&base, &mut |tag, s| {
            if tag == COST {
                s[24] = 0;
            }
        });
        assert_eq!(
            Inventory::open(&undeclared, &b, &lim()).unwrap_err().code,
            codes::GVI_CERTIFICATE_INVALID
        );
        // An unknown profile id.
        let unknown = rebuild(&base, &mut |tag, s| {
            if tag == COST {
                s[24] = 9;
            }
        });
        assert_eq!(
            Inventory::open(&unknown, &b, &lim()).unwrap_err().code,
            codes::MALFORMED_SECTION
        );
        // A sample address outside [0, 65536]: find the first trajectory
        // record and replace its t varint with 65537.
        let c = Container::open(
            &base,
            &Expect {
                format: Format::Gvi,
                known_sections: &KNOWN,
            },
            &Limits::default(),
        )
        .unwrap();
        let d = c.section(DIFF).unwrap().unwrap().to_vec();
        let n = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
        let offs: Vec<usize> = (0..=n)
            .map(|k| u32::from_le_bytes(d[16 + 4 * k..20 + 4 * k].try_into().unwrap()) as usize)
            .collect();
        let blob_at = 16 + ((n + 1) * 4).div_ceil(16) * 16;
        let k = (0..n)
            .find(|&k| d[blob_at + offs[k]] == 3)
            .expect("a lerp record");
        let rec = &d[blob_at + offs[k]..blob_at + offs[k + 1]];
        // kind, flags, two single-byte varint bases (small ids), then t.
        assert!(rec[2] < 0x80 && rec[3] < 0x80);
        let mut t_len = 1;
        while rec[4 + t_len - 1] & 0x80 != 0 {
            t_len += 1;
        }
        let mut forged = rec[..4].to_vec();
        forged.extend_from_slice(&[0x81, 0x80, 0x04]); // 65537
        forged.extend_from_slice(&rec[4 + t_len..]);
        let mut records: Vec<Vec<u8>> = (0..n)
            .map(|j| d[blob_at + offs[j]..blob_at + offs[j + 1]].to_vec())
            .collect();
        records[k] = forged;
        let mut nd = Vec::new();
        nd.extend_from_slice(&(n as u32).to_le_bytes());
        nd.extend_from_slice(&[0u8; 12]);
        let mut off = 0u32;
        nd.extend_from_slice(&off.to_le_bytes());
        for r in &records {
            off += r.len() as u32;
            nd.extend_from_slice(&off.to_le_bytes());
        }
        pad16(&mut nd);
        for r in &records {
            nd.extend_from_slice(r);
        }
        pad16(&mut nd);
        let bad_t = rebuild(&base, &mut |tag, s| {
            if tag == DIFF {
                *s = nd.clone();
            }
        });
        assert_eq!(
            Inventory::open(&bad_t, &b, &lim()).unwrap_err().code,
            codes::GVI_CERTIFICATE_INVALID
        );
    }

    // -----------------------------------------------------------------------
    // GVI-O operative encoding
    // -----------------------------------------------------------------------

    fn full(
        bake: Bake,
        depth: u32,
        albit: AlbitProfile,
        operative: OperativeProfile,
    ) -> CompileOptions {
        CompileOptions {
            bake,
            max_diff_depth: depth,
            albit,
            operative,
        }
    }

    fn section_of(bytes: &[u8], tag: Tag) -> Vec<u8> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gvi,
                known_sections: &KNOWN,
            },
            &Limits::default(),
        )
        .unwrap();
        c.section(tag)
            .unwrap()
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    }

    #[test]
    fn operative_inventories_are_exact_and_smaller() {
        // 48 uniform frames × 3 sizes: regular fractions and labels.
        let fractions: Vec<u32> = (0..48u32).map(|k| k * 65_536 / 47).collect();
        let m = ring_model_with(&fractions);
        let b = ring_binding(&m);
        for (bake, albit) in [
            (Bake::None, AlbitProfile::Off),
            (Bake::None, AlbitProfile::Provenance),
            (Bake::Transforms, AlbitProfile::Provenance),
        ] {
            // The reference is the plain inventory with the same bake policy.
            let plain = compile_with(&ring_source(), &m, &bud(), &gl(), bake).unwrap();
            let reference = all_drawings(&Inventory::open(&plain, &b, &lim()).unwrap(), &m);
            let base = compile_with(
                &ring_source(),
                &m,
                &bud(),
                &gl(),
                full(bake, 8, albit, OperativeProfile::Off),
            )
            .unwrap();
            let op = compile_with(
                &ring_source(),
                &m,
                &bud(),
                &gl(),
                full(bake, 8, albit, OperativeProfile::Fast),
            )
            .unwrap();
            assert!(
                op.len() < base.len(),
                "{bake:?} {albit:?}: {} ≥ {}",
                op.len(),
                base.len()
            );
            // Each section shrinks or stays.
            for tag in [STAT, DIFF] {
                assert!(
                    section_of(&op, tag).len() <= section_of(&base, tag).len(),
                    "{tag:?}"
                );
            }
            assert_eq!(section_of(&op, STAT)[8], model::STAT_OPERATIVE);
            // GO015: the same parent binding opens both.
            let inv = Inventory::open(&op, &b, &lim()).unwrap();
            assert_eq!(
                inv.binding,
                Inventory::open(&base, &b, &lim()).unwrap().binding
            );
            // GO013: every state equals the plain inventory.
            assert_eq!(all_drawings(&inv, &m), reference, "{bake:?} {albit:?}");
            // GO017: packed and expanded residency agree.
            let mut x = Inventory::open(&op, &b, &lim()).unwrap();
            x.set_residency(Residency::Expanded);
            assert_eq!(all_drawings(&x, &m), reference);
            inv.verify_against(&ring_source(), &m, &bud(), &gl())
                .unwrap();
            // The model decodes to exactly the compiled model.
            assert_eq!(inv.dimensions(), m.dimensions.as_slice());
        }
    }

    #[test]
    fn operatives_never_make_files_bigger() {
        // GO021: the status-icon model has five short irregular labels and
        // mixed effects; whatever GVI-O finds, the file is never bigger,
        // and if nothing helps it is byte-identical.
        let m = product_model();
        let off = compile(&source(), &m, &bud(), &gl()).unwrap();
        let on = compile_with(
            &source(),
            &m,
            &bud(),
            &gl(),
            full(Bake::None, 0, AlbitProfile::Off, OperativeProfile::Fast),
        )
        .unwrap();
        assert!(on.len() <= off.len());
        if on.len() == off.len() {
            assert_eq!(on, off);
        }
        let inv = Inventory::open(&on, &binding(&m), &lim()).unwrap();
        assert_eq!(
            all_drawings(&inv, &m),
            all_drawings(&Inventory::open(&off, &binding(&m), &lim()).unwrap(), &m)
        );
        // Random labels and fractions: no regularity, so identical bytes.
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let circle = Arc::new(ring(|_| 32_000.0));
        let mut rm = ring_model();
        rm.dimensions[0] = Dimension::discrete(
            "frame",
            (0..6)
                .map(|_| {
                    let label = format!("{:x}", next());
                    Value::new(&label).with(Effect::Morph(
                        Selector::All,
                        circle.clone(),
                        (next() % 65_537) as u32,
                    ))
                })
                .collect(),
        );
        let off = compile_with(
            &ring_source(),
            &rm,
            &bud(),
            &gl(),
            full(
                Bake::None,
                8,
                AlbitProfile::Provenance,
                OperativeProfile::Off,
            ),
        )
        .unwrap();
        let on = compile_with(
            &ring_source(),
            &rm,
            &bud(),
            &gl(),
            full(
                Bake::None,
                8,
                AlbitProfile::Provenance,
                OperativeProfile::Fast,
            ),
        )
        .unwrap();
        assert!(on.len() <= off.len());
        if on.len() == off.len() {
            assert_eq!(on, off);
        }
    }

    #[test]
    fn operative_projection_columns() {
        // A 32 × 32 translation grid over a one-item source: PROJ columns
        // e = base + 100·x (repeated progressions) and f = base + 100·y
        // (constant runs). With several items the row pool interleaves
        // them, which plain spans cannot express (gvi-operative.md §7).
        let axis = |name: &str, x: bool| {
            Dimension::discrete(
                name,
                (0..32i32)
                    .map(|k| {
                        let t = if x {
                            Affine::translate(k * 100, 0)
                        } else {
                            Affine::translate(0, k * 100)
                        };
                        Value::new(&format!("{name}{k}")).with(Effect::Transform(Selector::All, t))
                    })
                    .collect(),
            )
        };
        let m = StateModel {
            parent: "scene://grid".into(),
            dimensions: vec![axis("x", true), axis("y", false)],
            reachability: Reachability::Product { exclusions: vec![] },
        };
        let b = ring_binding(&m);
        let off = compile(&ring_source(), &m, &bud(), &gl()).unwrap();
        let on = compile_with(
            &ring_source(),
            &m,
            &bud(),
            &gl(),
            full(Bake::None, 0, AlbitProfile::Off, OperativeProfile::Fast),
        )
        .unwrap();
        let p_off = section_of(&off, PROJ).len();
        let p_on = section_of(&on, PROJ).len();
        assert!(p_on * 4 < p_off, "PROJ {p_on} vs {p_off}");
        let reference = all_drawings(&Inventory::open(&off, &b, &lim()).unwrap(), &m);
        let mut inv = Inventory::open(&on, &b, &lim()).unwrap();
        // Operative columns have no packed form: the inventory is expanded
        // and stays expanded.
        assert_eq!(inv.residency(), Residency::Expanded);
        inv.set_residency(Residency::Packed);
        assert_eq!(inv.residency(), Residency::Expanded);
        assert_eq!(all_drawings(&inv, &m), reference);
        inv.verify_against(&ring_source(), &m, &bud(), &gl())
            .unwrap();
        // An operative column in a PROJ that did not opt in is rejected.
        let forged = rebuild(&on, &mut |tag, s| {
            if tag == PROJ {
                s[12] &= !(PROJ_OPERATIVE as u8);
            }
        });
        assert!(Inventory::open(&forged, &b, &lim()).is_err());
    }

    #[test]
    fn operative_tampering_is_caught() {
        let fractions: Vec<u32> = (0..48u32).map(|k| k * 65_536 / 47).collect();
        let m = ring_model_with(&fractions);
        let b = ring_binding(&m);
        let op = compile_with(
            &ring_source(),
            &m,
            &bud(),
            &gl(),
            full(
                Bake::None,
                8,
                AlbitProfile::Provenance,
                OperativeProfile::Fast,
            ),
        )
        .unwrap();
        assert_eq!(
            section_of(&op, DIFF)[4],
            diff::DIFF_OPERATIVE,
            "this model should take the operative DIFF"
        );
        // A stored model that decodes to a different model breaks the
        // parent binding, whatever its form: flip the first label span's
        // numbering step.
        let mut forged = 0;
        for i in 16..section_of(&op, STAT).len() {
            let tampered = rebuild(&op, &mut |tag, s| {
                if tag == STAT && i < s.len() {
                    s[i] ^= 1;
                }
            });
            match Inventory::open(&tampered, &b, &lim()) {
                Ok(_) => panic!("STAT byte {i} flip went unnoticed"),
                Err(e) => {
                    assert!(
                        matches!(
                            e.code,
                            codes::GVI_CERTIFICATE_INVALID
                                | codes::NON_CANONICAL
                                | codes::MALFORMED_SECTION
                                | codes::NON_MINIMAL_VARINT
                                | codes::GVI_INVALID_MODEL
                                | codes::MALFORMED_GEOMETRY
                                | codes::GVI_PARENT_MISMATCH
                        ),
                        "byte {i}: {e}"
                    );
                    forged += 1;
                }
            }
        }
        assert!(forged > 0);
        // Every DIFF byte flip fails at open or at reconstruction.
        let d = section_of(&op, DIFF);
        for i in 0..d.len() {
            let tampered = rebuild(&op, &mut |tag, s| {
                if tag == DIFF {
                    s[i] ^= 0x10;
                }
            });
            let detected = match Inventory::open(&tampered, &b, &lim()) {
                Err(_) => true,
                Ok(inv) => inv.decode_geometry().is_err(),
            };
            assert!(detected, "DIFF byte {i} flip went unnoticed");
        }
    }

    #[test]
    fn differential_tampering_is_caught() {
        let m = ring_model();
        let base =
            compile_with(&ring_source(), &m, &bud(), &gl(), opts(Bake::Transforms, 4)).unwrap();
        let section = |bytes: &[u8], tag: Tag| -> Vec<u8> {
            let c = Container::open(
                bytes,
                &Expect {
                    format: Format::Gvi,
                    known_sections: &KNOWN,
                },
                &Limits::default(),
            )
            .unwrap();
            c.section(tag).unwrap().unwrap().to_vec()
        };
        let d = section(&base, DIFF);
        let n = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize;
        let blob_at = 16 + ((n + 1) * 4).div_ceil(16) * 16;
        let rec0 = blob_at; // first record starts the blob

        // A flipped hash byte: open succeeds (structure is sound), but
        // reconstruction proves the mismatch.
        let bad_hash = rebuild(&base, &mut |tag, s| {
            if tag == DIFF {
                let kind = s[rec0];
                // kind, flags, then 1 (copy/affine) or 2 (extrapolate) varint
                // bases; for affine, six more varints. Flip the last byte of
                // the record, which is the hash or residual either way.
                let end = 16
                    + ((n + 1) * 4).div_ceil(16) * 16
                    + u32::from_le_bytes(s[20..24].try_into().unwrap()) as usize;
                let _ = kind;
                s[end - 1] ^= 0x40;
            }
        });
        let inv = Inventory::open(&bad_hash, &ring_binding(&m), &lim()).unwrap();
        assert_eq!(
            inv.decode_geometry().unwrap_err().code,
            codes::GVI_CERTIFICATE_INVALID
        );

        // COST that understates the depth.
        let lying = rebuild(&base, &mut |tag, s| {
            if tag == COST {
                s[4] = s[4].wrapping_sub(1);
            }
        });
        assert_eq!(
            Inventory::open(&lying, &ring_binding(&m), &lim())
                .unwrap_err()
                .code,
            codes::GVI_CERTIFICATE_INVALID
        );

        // DIFF without the certificate declaring it.
        let undeclared = rebuild(&base, &mut |tag, s| {
            if tag == CERT {
                s[72] &= !(ASSUME_DIFFERENTIAL_GEOMETRY as u8);
            }
        });
        assert_eq!(
            Inventory::open(&undeclared, &ring_binding(&m), &lim())
                .unwrap_err()
                .code,
            codes::GVI_CERTIFICATE_INVALID
        );

        // A record whose base is not earlier than itself.
        let forward = rebuild(&base, &mut |tag, s| {
            if tag == DIFF {
                s[rec0 + 2] = 0x7f;
            }
        });
        assert_eq!(
            Inventory::open(&forward, &ring_binding(&m), &lim())
                .unwrap_err()
                .code,
            codes::GVI_CERTIFICATE_INVALID
        );
    }

    #[test]
    fn lineless_and_operative_compose() {
        // A model whose many restyles make the GVI-L palette win, compiled
        // with GVI-O on: both encodings must hold, and the result must
        // match the plain inventory state for state.
        let values = (1..=100)
            .map(|width| {
                let mut style = source().items[2].style;
                style.stroke.as_mut().unwrap().width = width;
                Value::new(&format!("width-{width}"))
                    .with(Effect::Restyle(Selector::Items(vec![2]), style))
            })
            .collect();
        let m = StateModel {
            parent: "scene://lineless".into(),
            dimensions: vec![Dimension::discrete("width", values)],
            reachability: Reachability::Product { exclusions: vec![] },
        };
        let b = m.binding(4, &bud()).unwrap();
        let plain = compile(&source(), &m, &bud(), &gl()).unwrap();
        let op = compile_with(
            &source(),
            &m,
            &bud(),
            &gl(),
            full(Bake::None, 0, AlbitProfile::Off, OperativeProfile::Fast),
        )
        .unwrap();
        assert!(op.len() < plain.len(), "{} ≥ {}", op.len(), plain.len());
        let inv = Inventory::open(&op, &b, &lim()).unwrap();
        assert_ne!(
            inv.certificate.assumptions & ASSUME_LINELESS_STYLES,
            0,
            "LSTY still chosen"
        );
        assert_eq!(
            section_of(&op, STAT)[8],
            model::STAT_OPERATIVE,
            "operative STAT still chosen"
        );
        assert_eq!(
            all_drawings(&inv, &m),
            all_drawings(&Inventory::open(&plain, &b, &lim()).unwrap(), &m)
        );
        inv.verify_against(&source(), &m, &bud(), &gl()).unwrap();
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
        let tags = [
            CERT, COST, DIFF, GEOM, INDX, PARN, PROJ, SRCE, STAT, STYL, TRAN,
        ];
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
