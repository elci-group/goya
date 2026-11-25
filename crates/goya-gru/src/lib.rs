//! goya-gru — Graphical Rendering Unit (`.gru`), contract `GY-GRU/0`
//! (draft). Normative text: `docs/graphics/gru.md`.
//!
//! A GRU is a reusable graphical unit (GOYA-GFX-001 §6). It holds:
//!
//! - deduplicated, immutable geometry, decoded once at load;
//! - instances placed over that geometry;
//! - **typed slots**, the unit's parameters, with declared domains;
//! - **bindings**: bounded, declarative maps from one slot to one channel of
//!   one instance (opacity, fill or stroke colour, translation, scale,
//!   visibility, or a geometry variant);
//! - **invalidation metadata** (`CHNG`): for each slot, the instances and
//!   channels it can touch;
//! - **interaction regions** (`SPAT`) for hit-testing, focus order and
//!   accessibility, without rasterising.
//!
//! The unit never runs on its own. The parent sets slots through
//! [`UnitState::set`], collects the [`Invalidation`] and schedules
//! presentation. Bindings have no loops, recursion, calls or allocation
//! that depends on slot values, so every operation costs O(1), plus table
//! indexing bounded by [`GruLimits`].

mod state;

pub use state::{Invalidation, UnitState};

use goya_graphics_container::{
    encode as encode_container, Container, Expect, Format, Profile, SectionInput, Tag,
};
use goya_graphics_ir::{codes, Affine, Drawing, GfxDiag, Path, Point, Rect, Rgba8, Style};
use goya_gva::{
    encode_geometry_pool, encode_style_pool, parse_style_pool, GeometryPool, Gva, GvaLimits,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Contract identifier implemented by this crate.
pub const GRU_CONTRACT: &str = "GY-GRU/0";

pub const GEOM: Tag = Tag::new(b"GEOM");
pub const HASH: Tag = Tag::new(b"HASH");
pub const HEAD: Tag = Tag::new(b"HEAD");
pub const INST: Tag = Tag::new(b"INST");
pub const META: Tag = Tag::new(b"META");
pub const MODS: Tag = Tag::new(b"MODS");
pub const SLOT: Tag = Tag::new(b"SLOT");
pub const SPAT: Tag = Tag::new(b"SPAT");
pub const STYL: Tag = Tag::new(b"STYL");
pub const TYPE: Tag = Tag::new(b"TYPE");
pub const CHNG: Tag = Tag::new(b"CHNG");
const KNOWN: [Tag; 11] = [
    CHNG, GEOM, HASH, HEAD, INST, META, MODS, SLOT, SPAT, STYL, TYPE,
];
const ALIGN_LOG2: u8 = 4;
/// `HEAD.profile`: the bounded execution profile of GY-GRU/0 (§6.7).
pub const PROFILE_BOUNDED_V0: u16 = 0;
const Q16: i64 = 1 << 16;

/// Channel bits used in `CHNG` masks and [`Invalidation::channels`].
pub mod channel {
    pub const OPACITY: u32 = 1;
    pub const FILL: u32 = 2;
    pub const STROKE: u32 = 4;
    pub const TRANSLATE_X: u32 = 8;
    pub const TRANSLATE_Y: u32 = 16;
    pub const SCALE: u32 = 32;
    pub const VISIBLE: u32 = 64;
    pub const GEOMETRY: u32 = 128;
    /// Channels that change paint only: no geometry, no transform.
    pub const MODULATION: u32 = OPACITY | FILL | STROKE | VISIBLE;
    pub const TRANSFORM: u32 = TRANSLATE_X | TRANSLATE_Y | SCALE;
}

fn diag(code: u16, detail: impl Into<String>) -> GfxDiag {
    GfxDiag::new(code, detail)
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// A slot's type and domain. Scalars are Q16.16 fixed point: the Baseline
/// profile stores no floats.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SlotType {
    Scalar { min: i32, max: i32 },
    Int { min: i32, max: i32 },
    Bool,
    Enum(Vec<String>),
    Colour,
}

/// A slot value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotValue {
    /// Q16.16.
    Scalar(i32),
    Int(i32),
    Bool(bool),
    /// Index into the enum's labels.
    Enum(u16),
    Colour(Rgba8),
}

impl SlotValue {
    /// A Q16.16 scalar from `f64`, rounded half-to-even. Returns `None` if
    /// the value is not finite or out of range.
    pub fn scalar(v: f64) -> Option<SlotValue> {
        let q = (v * Q16 as f64).round_ties_even();
        (q.is_finite() && q >= f64::from(i32::MIN) && q <= f64::from(i32::MAX))
            .then_some(SlotValue::Scalar(q as i32))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Slot {
    pub name: String,
    pub ty: SlotType,
    pub default: SlotValue,
}

/// The channel of one instance that a binding drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// Opacity, 0–255.
    Opacity(u32),
    /// Fill colour. The instance's base style must have a fill.
    FillColour(u32),
    /// Stroke colour. The instance's base style must have a stroke.
    StrokeColour(u32),
    /// Document-space offset in millipoints, applied after the base
    /// transform.
    TranslateX(u32),
    TranslateY(u32),
    /// Uniform Q16.16 scale about `pivot` (in the instance's own
    /// coordinates), applied before the base transform.
    Scale {
        instance: u32,
        pivot: Point,
    },
    /// Visibility. Table maps only.
    Visible(u32),
    /// Geometry variant. Table maps only; entries are paths.
    Geometry(u32),
}

impl Target {
    pub fn instance(&self) -> u32 {
        match *self {
            Target::Opacity(i)
            | Target::FillColour(i)
            | Target::StrokeColour(i)
            | Target::TranslateX(i)
            | Target::TranslateY(i)
            | Target::Visible(i)
            | Target::Geometry(i) => i,
            Target::Scale { instance, .. } => instance,
        }
    }

    pub fn channel(&self) -> u32 {
        match self {
            Target::Opacity(_) => channel::OPACITY,
            Target::FillColour(_) => channel::FILL,
            Target::StrokeColour(_) => channel::STROKE,
            Target::TranslateX(_) => channel::TRANSLATE_X,
            Target::TranslateY(_) => channel::TRANSLATE_Y,
            Target::Scale { .. } => channel::SCALE,
            Target::Visible(_) => channel::VISIBLE,
            Target::Geometry(_) => channel::GEOMETRY,
        }
    }
}

/// A table entry or computed channel value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Number(i32),
    Colour(Rgba8),
    Bool(bool),
    /// Authoring only: a geometry variant in the instance's own coordinates.
    Path(Arc<Path>),
    /// Decoded form of a variant: an index into the geometry pool.
    Geometry(u32),
}

/// How a slot value becomes a channel value. Every map is O(1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Map {
    /// `out = round_half_even(x · mul / 2^16) + add`, where `x` is the raw
    /// slot value (Q16.16 for scalars, plain for ints). Numeric targets
    /// only. The result must stay in the target's range across the whole
    /// domain, which is checked at encode and at load.
    Linear { mul: i32, add: i32 },
    /// Scalar `t` in `[0, 1]` (Q16.16): `from + round_half_even((to −
    /// from) · t / 2^16)` per channel. Colour targets only.
    Mix(Rgba8, Rgba8),
    /// Index by the slot value: `false/true`, the enum index, or
    /// `int − min`. Exactly one entry per domain value.
    Table(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Binding {
    pub slot: u16,
    pub target: Target,
    pub map: Map,
}

/// Accessibility and interaction role of a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Button,
    Toggle,
    Slider,
    Indicator,
    Image,
}

impl Role {
    fn id(self) -> u8 {
        match self {
            Role::Button => 0,
            Role::Toggle => 1,
            Role::Slider => 2,
            Role::Indicator => 3,
            Role::Image => 4,
        }
    }

    fn from_id(id: u8) -> Option<Role> {
        Some(match id {
            0 => Role::Button,
            1 => Role::Toggle,
            2 => Role::Slider,
            3 => Role::Indicator,
            4 => Role::Image,
            _ => return None,
        })
    }
}

/// An interaction region attached to an instance. It follows the
/// instance's current transform and visibility.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Region {
    /// A stable semantic identifier, unique within the unit.
    pub id: String,
    pub role: Role,
    pub instance: u32,
    /// Focus traversal position, unique within the unit.
    pub focus: u16,
    /// `true`: hit only inside the filled shape. `false`: hit anywhere in
    /// the instance's bounds.
    pub precise: bool,
}

/// Everything a GRU is compiled from.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitSource {
    /// The base instances. Items keep their order and become instance ids.
    pub drawing: Drawing,
    pub slots: Vec<Slot>,
    pub bindings: Vec<Binding>,
    pub regions: Vec<Region>,
    /// Non-executable provenance text for `META`.
    pub provenance: Option<String>,
}

/// The execution profile's resource bounds (§6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GruLimits {
    pub gva: GvaLimits,
    pub max_slots: usize,
    pub max_bindings: usize,
    pub max_bindings_per_slot: usize,
    pub max_table: usize,
    pub max_regions: usize,
}

impl Default for GruLimits {
    fn default() -> GruLimits {
        GruLimits {
            gva: GvaLimits::default(),
            max_slots: 1024,
            max_bindings: 16_384,
            max_bindings_per_slot: 256,
            max_table: 256,
            max_regions: 4096,
        }
    }
}

// ---------------------------------------------------------------------------
// Typing (shared by encoder and loader)
// ---------------------------------------------------------------------------

pub(crate) fn round_q16(v: i128) -> i128 {
    let q = v.div_euclid(65536);
    match v.rem_euclid(65536).cmp(&32768) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
    }
}

/// Number of table entries a slot type needs, or `None` if it cannot drive
/// a table.
fn domain_size(ty: &SlotType) -> Option<u64> {
    match ty {
        SlotType::Bool => Some(2),
        SlotType::Enum(labels) => Some(labels.len() as u64),
        SlotType::Int { min, max } => Some((i64::from(*max) - i64::from(*min) + 1) as u64),
        _ => None,
    }
}

/// Check a value against a slot's type and domain.
pub(crate) fn check_value(slot: &Slot, v: &SlotValue) -> Result<(), GfxDiag> {
    let out = |d: String| {
        diag(
            codes::SLOT_OUT_OF_DOMAIN,
            format!("slot {:?}: {d}", slot.name),
        )
    };
    match (&slot.ty, v) {
        (SlotType::Scalar { min, max }, SlotValue::Scalar(x))
        | (SlotType::Int { min, max }, SlotValue::Int(x)) => {
            if x < min || x > max {
                return Err(out(format!("{x} is outside [{min}, {max}]")));
            }
        }
        (SlotType::Enum(labels), SlotValue::Enum(i)) => {
            if usize::from(*i) >= labels.len() {
                return Err(out(format!("enum index {i} is out of range")));
            }
        }
        (SlotType::Bool, SlotValue::Bool(_)) | (SlotType::Colour, SlotValue::Colour(_)) => {}
        (ty, v) => {
            return Err(diag(
                codes::SLOT_TYPE_MISMATCH,
                format!("slot {:?} has type {ty:?}, got {v:?}", slot.name),
            ))
        }
    }
    Ok(())
}

/// Evaluate a typed, validated binding for a valid slot value.
pub(crate) fn evaluate(map: &Map, slot: &Slot, v: &SlotValue) -> Value {
    match map {
        Map::Linear { mul, add } => {
            let x = match v {
                SlotValue::Scalar(x) | SlotValue::Int(x) => *x,
                _ => 0,
            };
            Value::Number((round_q16(i128::from(x) * i128::from(*mul)) + i128::from(*add)) as i32)
        }
        Map::Mix(from, to) => {
            let t = match v {
                SlotValue::Scalar(t) => i128::from(*t),
                _ => 0,
            };
            let lerp = |a: u8, b: u8| {
                (i128::from(a) + round_q16((i128::from(b) - i128::from(a)) * t)) as u8
            };
            Value::Colour(Rgba8::new(
                lerp(from.r, to.r),
                lerp(from.g, to.g),
                lerp(from.b, to.b),
                lerp(from.a, to.a),
            ))
        }
        Map::Table(entries) => {
            let i = match (&slot.ty, v) {
                (_, SlotValue::Bool(b)) => usize::from(*b),
                (_, SlotValue::Enum(i)) => usize::from(*i),
                (SlotType::Int { min, .. }, SlotValue::Int(x)) => {
                    (i64::from(*x) - i64::from(*min)) as usize
                }
                _ => 0,
            };
            entries[i].clone()
        }
    }
}

/// The loaded, typed form of a unit, shared by the encoder (which
/// validates before writing) and the loader.
pub(crate) struct Typed<'s> {
    pub slots: &'s [Slot],
    pub bindings: &'s [Binding],
    pub regions: &'s [Region],
    pub base_styles: Vec<Style>,
    pub geometries: u32,
}

impl Typed<'_> {
    /// Full type and dependency check (§6.2, `TYPE`).
    pub fn check(&self, limits: &GruLimits) -> Result<(), GfxDiag> {
        let n = self.base_styles.len() as u32;
        if self.slots.len() > limits.max_slots {
            return Err(diag(
                codes::TOO_MANY_SLOTS,
                format!("{} slots exceeds {}", self.slots.len(), limits.max_slots),
            ));
        }
        if self.bindings.len() > limits.max_bindings {
            return Err(diag(codes::TOO_MANY_BINDINGS, "too many bindings"));
        }
        if self.regions.len() > limits.max_regions {
            return Err(diag(codes::TOO_MANY_REGIONS, "too many regions"));
        }
        let mut names = std::collections::HashSet::new();
        for s in self.slots {
            if s.name.is_empty() || s.name.len() > u16::MAX as usize || !names.insert(&s.name) {
                return Err(diag(
                    codes::BINDING_TYPE,
                    format!("slot name {:?} is empty, too long or duplicated", s.name),
                ));
            }
            match &s.ty {
                SlotType::Scalar { min, max } | SlotType::Int { min, max } if min > max => {
                    return Err(diag(
                        codes::BINDING_TYPE,
                        format!("slot {:?} has an empty domain", s.name),
                    ));
                }
                SlotType::Enum(labels) => {
                    let mut seen = std::collections::HashSet::new();
                    if labels.is_empty()
                        || labels.len() > u16::MAX as usize
                        || labels
                            .iter()
                            .any(|l| l.is_empty() || l.len() > u16::MAX as usize || !seen.insert(l))
                    {
                        return Err(diag(
                            codes::BINDING_TYPE,
                            format!("slot {:?} has invalid enum labels", s.name),
                        ));
                    }
                }
                _ => {}
            }
            check_value(s, &s.default)?;
        }
        let mut per_slot = vec![0usize; self.slots.len()];
        let mut claimed = std::collections::HashSet::new();
        for (k, b) in self.bindings.iter().enumerate() {
            let bad = |d: &str| diag(codes::BINDING_TYPE, format!("binding {k}: {d}"));
            let slot = self.slots.get(usize::from(b.slot)).ok_or_else(|| {
                diag(
                    codes::UNKNOWN_SLOT,
                    format!("binding {k} names missing slot {}", b.slot),
                )
            })?;
            per_slot[usize::from(b.slot)] += 1;
            if per_slot[usize::from(b.slot)] > limits.max_bindings_per_slot {
                return Err(diag(
                    codes::TOO_MANY_BINDINGS,
                    format!("slot {:?} drives too many bindings", slot.name),
                ));
            }
            let inst = b.target.instance();
            if inst >= n {
                return Err(bad("targets a missing instance"));
            }
            if !claimed.insert((inst, b.target.channel())) {
                return Err(diag(
                    codes::BINDING_CONFLICT,
                    format!(
                        "binding {k}: instance {inst} channel {:#x} already has a binding",
                        b.target.channel()
                    ),
                ));
            }
            let style = &self.base_styles[inst as usize];
            match b.target {
                Target::FillColour(_) if style.fill.is_none() => {
                    return Err(bad("fill colour on an instance without a fill"))
                }
                Target::StrokeColour(_) if style.stroke.is_none() => {
                    return Err(bad("stroke colour on an instance without a stroke"))
                }
                _ => {}
            }
            let numeric_range = |t: &Target| -> Option<(i64, i64)> {
                match t {
                    Target::Opacity(_) => Some((0, 255)),
                    Target::TranslateX(_) | Target::TranslateY(_) => {
                        Some((i64::from(i32::MIN), i64::from(i32::MAX)))
                    }
                    Target::Scale { .. } => Some((0, i64::from(i32::MAX))),
                    _ => None,
                }
            };
            match &b.map {
                Map::Linear { mul, add } => {
                    let (min, max) = match slot.ty {
                        SlotType::Scalar { min, max } | SlotType::Int { min, max } => (min, max),
                        _ => return Err(bad("linear maps need a scalar or int slot")),
                    };
                    let range = numeric_range(&b.target)
                        .ok_or_else(|| bad("linear maps need a numeric target"))?;
                    for x in [min, max] {
                        let y = round_q16(i128::from(x) * i128::from(*mul)) + i128::from(*add);
                        if y < i128::from(range.0) || y > i128::from(range.1) {
                            return Err(bad(&format!(
                                "output {y} leaves the target's range at slot value {x}"
                            )));
                        }
                    }
                }
                Map::Mix(..) => {
                    match slot.ty {
                        SlotType::Scalar { min, max } if min >= 0 && i64::from(max) <= Q16 => {}
                        _ => return Err(bad("mix maps need a scalar slot within [0, 1]")),
                    }
                    if !matches!(b.target, Target::FillColour(_) | Target::StrokeColour(_)) {
                        return Err(bad("mix maps need a colour target"));
                    }
                }
                Map::Table(entries) => {
                    let size = domain_size(&slot.ty)
                        .ok_or_else(|| bad("tables need a bool, enum or int slot"))?;
                    if size > limits.max_table as u64 || entries.len() > limits.max_table {
                        return Err(diag(
                            codes::TABLE_TOO_LONG,
                            format!("binding {k}: table over {}", limits.max_table),
                        ));
                    }
                    if entries.len() as u64 != size {
                        return Err(bad(&format!(
                            "table has {} entries for {size} slot values",
                            entries.len()
                        )));
                    }
                    for e in entries {
                        let ok = match (&b.target, e) {
                            (Target::FillColour(_) | Target::StrokeColour(_), Value::Colour(_)) => {
                                true
                            }
                            (Target::Visible(_), Value::Bool(_)) => true,
                            (Target::Geometry(_), Value::Geometry(g)) => *g < self.geometries,
                            (t, Value::Number(x)) => numeric_range(t)
                                .is_some_and(|(lo, hi)| (lo..=hi).contains(&i64::from(*x))),
                            _ => false,
                        };
                        if !ok {
                            return Err(bad(&format!("table entry {e:?} does not fit the target")));
                        }
                    }
                }
            }
            if matches!(b.target, Target::Visible(_) | Target::Geometry(_))
                && !matches!(b.map, Map::Table(_))
            {
                return Err(bad("visibility and geometry are driven by tables only"));
            }
        }
        let mut ids = std::collections::HashSet::new();
        let mut focus = std::collections::HashSet::new();
        for r in self.regions {
            if r.id.is_empty() || r.id.len() > u16::MAX as usize || !ids.insert(&r.id) {
                return Err(diag(
                    codes::REGION_INVALID,
                    format!("region id {:?} is empty, too long or duplicated", r.id),
                ));
            }
            if !focus.insert(r.focus) {
                return Err(diag(
                    codes::REGION_INVALID,
                    format!("region {:?}: focus position {} is taken", r.id, r.focus),
                ));
            }
            if r.instance >= n {
                return Err(diag(
                    codes::REGION_INVALID,
                    format!("region {:?} names a missing instance", r.id),
                ));
            }
            if r.precise && self.base_styles[r.instance as usize].fill.is_none() {
                return Err(diag(
                    codes::REGION_INVALID,
                    format!("region {:?}: precise hits need a filled shape", r.id),
                ));
            }
        }
        Ok(())
    }

    /// Per slot: (channel mask, sorted affected instances). This is `CHNG`.
    pub fn invalidation(&self) -> Vec<(u32, Vec<u32>)> {
        let mut out: Vec<(u32, Vec<u32>)> = vec![(0, Vec::new()); self.slots.len()];
        for b in self.bindings {
            let e = &mut out[usize::from(b.slot)];
            e.0 |= b.target.channel();
            e.1.push(b.target.instance());
        }
        for e in &mut out {
            e.1.sort_unstable();
            e.1.dedup();
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

fn pad16(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn sub_header(out: &mut Vec<u8>, a: u32, b: u32) {
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
}

fn put_rect(out: &mut Vec<u8>, r: &Rect) {
    for v in [r.min_x, r.min_y, r.max_x, r.max_y] {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        min_x: a.min_x.min(b.min_x),
        min_y: a.min_y.min(b.min_y),
        max_x: a.max_x.max(b.max_x),
        max_y: a.max_y.max(b.max_y),
    }
}

const SLOT_SCALAR: u8 = 0;
const SLOT_INT: u8 = 1;
const SLOT_BOOL: u8 = 2;
const SLOT_ENUM: u8 = 3;
const SLOT_COLOUR: u8 = 4;
const MAP_LINEAR: u8 = 0;
const MAP_MIX: u8 = 1;
const MAP_TABLE: u8 = 2;

fn target_kind(t: &Target) -> u8 {
    match t {
        Target::Opacity(_) => 0,
        Target::FillColour(_) => 1,
        Target::StrokeColour(_) => 2,
        Target::TranslateX(_) => 3,
        Target::TranslateY(_) => 4,
        Target::Scale { .. } => 5,
        Target::Visible(_) => 6,
        Target::Geometry(_) => 7,
    }
}

/// Compile a unit. The output is deterministic: equal sources give equal
/// bytes.
pub fn encode(src: &UnitSource, limits: &GruLimits) -> Result<Vec<u8>, GfxDiag> {
    let gva_bytes = goya_gva::encode(&src.drawing, &limits.gva)?;
    let gva = Gva::open(&gva_bytes, &limits.gva)?;
    let mut pool: Vec<Path> = (0..gva.geometry_count())
        .map(|g| gva.geometry(g))
        .collect::<Result<_, _>>()?;
    let mut pool_index: HashMap<Path, u32> = pool
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), i as u32))
        .collect();
    let v = gva.instances();
    let n = gva.instance_count();
    let base_styles: Vec<Style> = (0..n)
        .map(|i| gva.styles()[v.style.get(i).unwrap_or(0) as usize])
        .collect();

    // Per-instance frame change from source to canonical (origin folding).
    let mut delta = Vec::with_capacity(n);
    for i in 0..n {
        let c = pool[v.geometry.get(i).unwrap_or(0) as usize].first_point();
        let s = src.drawing.items[i].path.first_point();
        let d = (
            i32::try_from(i64::from(c.x) - i64::from(s.x)),
            i32::try_from(i64::from(c.y) - i64::from(s.y)),
        );
        match d {
            (Ok(x), Ok(y)) => delta.push((x, y)),
            _ => {
                return Err(diag(
                    codes::COORDINATE_OVERFLOW,
                    "instance frame is out of range",
                ))
            }
        }
    }

    // Resolve authoring forms: variant paths into pool indices and pivots
    // into canonical frames.
    let mut bindings = Vec::with_capacity(src.bindings.len());
    for (k, b) in src.bindings.iter().enumerate() {
        let inst = b.target.instance() as usize;
        let (dx, dy) = *delta.get(inst).ok_or_else(|| {
            diag(
                codes::BINDING_TYPE,
                format!("binding {k} targets a missing instance"),
            )
        })?;
        let target = match b.target {
            Target::Scale { instance, pivot } => Target::Scale {
                instance,
                pivot: pivot.checked_offset(dx, dy).ok_or_else(|| {
                    diag(codes::COORDINATE_OVERFLOW, "scale pivot is out of range")
                })?,
            },
            t => t,
        };
        let map = match &b.map {
            Map::Table(entries) => Map::Table(
                entries
                    .iter()
                    .map(|e| match e {
                        Value::Path(p) => {
                            let p = p.translated(dx, dy).ok_or_else(|| {
                                diag(
                                    codes::COORDINATE_OVERFLOW,
                                    "geometry variant is out of range",
                                )
                            })?;
                            let g = match pool_index.get(&p) {
                                Some(&g) => g,
                                None => {
                                    let g = pool.len() as u32;
                                    if g >= limits.gva.max_geometries {
                                        return Err(diag(
                                            codes::TOO_MANY_GEOMETRIES,
                                            "too many geometry variants",
                                        ));
                                    }
                                    pool_index.insert(p.clone(), g);
                                    pool.push(p);
                                    g
                                }
                            };
                            Ok(Value::Geometry(g))
                        }
                        Value::Geometry(_) => Err(diag(
                            codes::BINDING_TYPE,
                            format!("binding {k}: author geometry variants as Value::Path"),
                        )),
                        other => Ok(other.clone()),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            m => m.clone(),
        };
        bindings.push(Binding {
            slot: b.slot,
            target,
            map,
        });
    }
    let typed = Typed {
        slots: &src.slots,
        bindings: &bindings,
        regions: &src.regions,
        base_styles,
        geometries: pool.len() as u32,
    };
    typed.check(limits)?;

    // Local bounds per instance: the union over every geometry it can show.
    let mut bounds: Vec<Rect> = (0..n)
        .map(|i| pool[v.geometry.get(i).unwrap_or(0) as usize].bounds())
        .collect();
    for b in &bindings {
        if let (Target::Geometry(i), Map::Table(entries)) = (&b.target, &b.map) {
            for e in entries {
                if let Value::Geometry(g) = e {
                    bounds[*i as usize] = union(bounds[*i as usize], pool[*g as usize].bounds());
                }
            }
        }
    }

    // HEAD
    let mut head = Vec::new();
    head.extend_from_slice(&src.drawing.width.to_le_bytes());
    head.extend_from_slice(&src.drawing.height.to_le_bytes());
    head.extend_from_slice(&0u16.to_le_bytes());
    head.extend_from_slice(&PROFILE_BOUNDED_V0.to_le_bytes());
    head.extend_from_slice(&[0u8; 4]);

    // INST (GVA layout) and MODS (opacity)
    let mut inst = Vec::new();
    sub_header(&mut inst, n as u32, 0);
    for field in 0..8 {
        for i in 0..n {
            let t = gva.transform(i).unwrap_or(Affine::IDENTITY);
            let word = match field {
                0 => v.geometry.get(i).unwrap_or(0),
                1 => v.style.get(i).unwrap_or(0),
                2 => t.e as u32,
                3 => t.f as u32,
                4 => t.a as u32,
                5 => t.b as u32,
                6 => t.c as u32,
                _ => t.d as u32,
            };
            inst.extend_from_slice(&word.to_le_bytes());
        }
        pad16(&mut inst);
    }
    let mut mods = Vec::new();
    sub_header(&mut mods, n as u32, channel::OPACITY);
    mods.extend_from_slice(v.opacity);
    pad16(&mut mods);

    // TYPE
    let mut ty = Vec::new();
    sub_header(&mut ty, src.slots.len() as u32, 0);
    for s in &src.slots {
        put_str(&mut ty, &s.name);
        match &s.ty {
            SlotType::Scalar { min, max } | SlotType::Int { min, max } => {
                ty.push(if matches!(s.ty, SlotType::Scalar { .. }) {
                    SLOT_SCALAR
                } else {
                    SLOT_INT
                });
                ty.extend_from_slice(&min.to_le_bytes());
                ty.extend_from_slice(&max.to_le_bytes());
            }
            SlotType::Bool => ty.push(SLOT_BOOL),
            SlotType::Colour => ty.push(SLOT_COLOUR),
            SlotType::Enum(labels) => {
                ty.push(SLOT_ENUM);
                ty.extend_from_slice(&(labels.len() as u16).to_le_bytes());
                for l in labels {
                    put_str(&mut ty, l);
                }
            }
        }
        match s.default {
            SlotValue::Scalar(x) | SlotValue::Int(x) => ty.extend_from_slice(&x.to_le_bytes()),
            SlotValue::Bool(b) => ty.push(u8::from(b)),
            SlotValue::Enum(i) => ty.extend_from_slice(&i.to_le_bytes()),
            SlotValue::Colour(c) => ty.extend_from_slice(&[c.r, c.g, c.b, c.a]),
        }
    }
    pad16(&mut ty);

    // SLOT
    let mut slot = Vec::new();
    sub_header(&mut slot, bindings.len() as u32, 0);
    for b in &bindings {
        slot.extend_from_slice(&b.slot.to_le_bytes());
        slot.push(target_kind(&b.target));
        slot.push(match b.map {
            Map::Linear { .. } => MAP_LINEAR,
            Map::Mix(..) => MAP_MIX,
            Map::Table(_) => MAP_TABLE,
        });
        slot.extend_from_slice(&b.target.instance().to_le_bytes());
        if let Target::Scale { pivot, .. } = b.target {
            slot.extend_from_slice(&pivot.x.to_le_bytes());
            slot.extend_from_slice(&pivot.y.to_le_bytes());
        }
        match &b.map {
            Map::Linear { mul, add } => {
                slot.extend_from_slice(&mul.to_le_bytes());
                slot.extend_from_slice(&add.to_le_bytes());
            }
            Map::Mix(a, c) => slot.extend_from_slice(&[a.r, a.g, a.b, a.a, c.r, c.g, c.b, c.a]),
            Map::Table(entries) => {
                slot.extend_from_slice(&(entries.len() as u16).to_le_bytes());
                for e in entries {
                    let word = match e {
                        Value::Number(x) => x.to_le_bytes(),
                        Value::Colour(c) => [c.r, c.g, c.b, c.a],
                        Value::Bool(b) => [u8::from(*b), 0, 0, 0],
                        Value::Geometry(g) => g.to_le_bytes(),
                        Value::Path(_) => unreachable!("resolved above"),
                    };
                    slot.extend_from_slice(&word);
                }
            }
        }
    }
    pad16(&mut slot);

    // CHNG
    let mut chng = Vec::new();
    let inval = typed.invalidation();
    sub_header(&mut chng, inval.len() as u32, 0);
    for (mask, insts) in &inval {
        chng.extend_from_slice(&mask.to_le_bytes());
        chng.extend_from_slice(&(insts.len() as u32).to_le_bytes());
        for i in insts {
            chng.extend_from_slice(&i.to_le_bytes());
        }
    }
    pad16(&mut chng);

    // SPAT
    let mut spat = Vec::new();
    sub_header(&mut spat, n as u32, src.regions.len() as u32);
    for b in &bounds {
        put_rect(&mut spat, b);
    }
    for r in &src.regions {
        put_str(&mut spat, &r.id);
        spat.push(r.role.id());
        spat.push(u8::from(r.precise));
        spat.extend_from_slice(&r.focus.to_le_bytes());
        spat.extend_from_slice(&r.instance.to_le_bytes());
    }
    pad16(&mut spat);

    let (geom, hash) = encode_geometry_pool(&pool)?;
    let styl = encode_style_pool(gva.styles());
    let mut meta = Vec::new();
    if let Some(p) = &src.provenance {
        meta.extend_from_slice(&(p.len() as u32).to_le_bytes());
        meta.extend_from_slice(&[0u8; 12]);
        meta.extend_from_slice(p.as_bytes());
        pad16(&mut meta);
    }
    let mut sections = vec![
        SectionInput::new(CHNG, &chng).align_log2(ALIGN_LOG2),
        SectionInput::new(GEOM, &geom).align_log2(ALIGN_LOG2),
        SectionInput::new(HASH, &hash)
            .align_log2(ALIGN_LOG2)
            .optional(),
        SectionInput::new(HEAD, &head).align_log2(ALIGN_LOG2),
        SectionInput::new(INST, &inst).align_log2(ALIGN_LOG2),
        SectionInput::new(MODS, &mods).align_log2(ALIGN_LOG2),
        SectionInput::new(SLOT, &slot).align_log2(ALIGN_LOG2),
        SectionInput::new(SPAT, &spat).align_log2(ALIGN_LOG2),
        SectionInput::new(STYL, &styl).align_log2(ALIGN_LOG2),
        SectionInput::new(TYPE, &ty).align_log2(ALIGN_LOG2),
    ];
    if src.provenance.is_some() {
        sections.push(
            SectionInput::new(META, &meta)
                .align_log2(ALIGN_LOG2)
                .optional(),
        );
    }
    encode_container(
        Format::Gru,
        Profile::Baseline,
        0,
        &sections,
        &limits.gva.container,
    )
    .map_err(|e| diag(e.kind.code(), e.to_string()))
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    tag: Tag,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8], tag: Tag) -> Reader<'a> {
        Reader { buf, pos: 0, tag }
    }

    fn err(&self, d: &str) -> GfxDiag {
        diag(codes::MALFORMED_SECTION, format!("{}: {d}", self.tag))
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], GfxDiag> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| self.err("truncated"))?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, GfxDiag> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, GfxDiag> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, GfxDiag> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32, GfxDiag> {
        Ok(self.u32()? as i32)
    }

    fn rgba(&mut self) -> Result<Rgba8, GfxDiag> {
        let b = self.bytes(4)?;
        Ok(Rgba8::new(b[0], b[1], b[2], b[3]))
    }

    fn string(&mut self) -> Result<String, GfxDiag> {
        let n = usize::from(self.u16()?);
        let b = self.bytes(n)?;
        std::str::from_utf8(b)
            .map(str::to_string)
            .map_err(|_| self.err("string is not UTF-8"))
    }

    fn zeros(&mut self, n: usize) -> Result<(), GfxDiag> {
        if self.bytes(n)?.iter().any(|&b| b != 0) {
            return Err(self.err("reserved or padding bytes are not zero"));
        }
        Ok(())
    }

    fn sub_header(&mut self) -> Result<(u32, u32), GfxDiag> {
        let a = self.u32()?;
        let b = self.u32()?;
        self.zeros(8)?;
        Ok((a, b))
    }

    fn pad(&mut self) -> Result<(), GfxDiag> {
        let n = (16 - self.pos % 16) % 16;
        self.zeros(n)
    }

    fn finish(&mut self) -> Result<(), GfxDiag> {
        self.pad()?;
        if self.pos != self.buf.len() {
            return Err(self.err("unexpected trailing bytes"));
        }
        Ok(())
    }
}

/// A base instance as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseInstance {
    pub geometry: u32,
    pub style: u32,
    pub transform: Affine,
    pub opacity: u8,
}

/// A loaded, fully validated unit. All geometry is decoded exactly once,
/// here. Nothing after load parses or rebuilds geometry.
#[derive(Debug, Clone)]
pub struct Unit {
    pub width: u32,
    pub height: u32,
    pub provenance: Option<String>,
    paths: Vec<Arc<Path>>,
    styles: Vec<Style>,
    base: Vec<BaseInstance>,
    slots: Vec<Slot>,
    bindings: Vec<Binding>,
    /// Binding indices per slot.
    by_slot: Vec<Vec<usize>>,
    invalidation: Vec<(u32, Vec<u32>)>,
    local_bounds: Vec<Rect>,
    regions: Vec<Region>,
}

fn take<'a>(c: &Container<'a>, tag: Tag) -> Result<&'a [u8], GfxDiag> {
    c.section(tag)
        .map_err(|e| diag(e.kind.code(), e.to_string()))?
        .ok_or_else(|| {
            diag(
                codes::MALFORMED_SECTION,
                format!("{tag}: required section is missing"),
            )
        })
}

impl Unit {
    pub fn open(bytes: &[u8], limits: &GruLimits) -> Result<Unit, GfxDiag> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gru,
                known_sections: &KNOWN,
            },
            &limits.gva.container,
        )
        .map_err(|e| diag(e.kind.code(), e.to_string()))?;

        let mut r = Reader::new(take(&c, HEAD)?, HEAD);
        let width = r.u32()?;
        let height = r.u32()?;
        if r.u16()? != 0 {
            return Err(diag(codes::UNSUPPORTED_UNIT, "unsupported coordinate unit"));
        }
        if r.u16()? != PROFILE_BOUNDED_V0 {
            return Err(diag(
                codes::BINDING_TYPE,
                "unknown execution profile; this unit needs a newer runtime",
            ));
        }
        r.zeros(4)?;
        r.finish()?;

        let geom = GeometryPool::parse(take(&c, GEOM)?, &limits.gva)?;
        let paths: Vec<Arc<Path>> = (0..geom.len())
            .map(|g| geom.get(g).map(Arc::new))
            .collect::<Result<_, _>>()?;
        let styles = parse_style_pool(take(&c, STYL)?, &limits.gva)?;

        let inst = take(&c, INST)?;
        let mut r = Reader::new(inst, INST);
        let (n, zero) = r.sub_header()?;
        if zero != 0 {
            return Err(r.err("reserved field is not zero"));
        }
        if n > limits.gva.max_instances {
            return Err(diag(codes::TOO_MANY_INSTANCES, format!("{n} instances")));
        }
        let n_us = n as usize;
        let mut arrays: Vec<Vec<u32>> = Vec::with_capacity(8);
        for _ in 0..8 {
            let raw = r.bytes(n_us * 4)?;
            arrays.push(
                raw.chunks_exact(4)
                    .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
                    .collect(),
            );
            r.pad()?;
        }
        r.finish()?;
        let mut m = Reader::new(take(&c, MODS)?, MODS);
        let (mn, ch) = m.sub_header()?;
        if mn != n || ch != channel::OPACITY {
            return Err(diag(codes::COUNT_MISMATCH, "MODS does not match INST"));
        }
        let opacity = m.bytes(n_us)?.to_vec();
        m.finish()?;
        let mut base = Vec::with_capacity(n_us);
        for i in 0..n_us {
            let (g, s) = (arrays[0][i], arrays[1][i]);
            if g as usize >= paths.len() {
                return Err(diag(
                    codes::GEOMETRY_REF,
                    format!("instance {i} references a missing geometry"),
                ));
            }
            if s as usize >= styles.len() {
                return Err(diag(
                    codes::STYLE_REF,
                    format!("instance {i} references a missing style"),
                ));
            }
            base.push(BaseInstance {
                geometry: g,
                style: s,
                transform: Affine {
                    e: arrays[2][i] as i32,
                    f: arrays[3][i] as i32,
                    a: arrays[4][i] as i32,
                    b: arrays[5][i] as i32,
                    c: arrays[6][i] as i32,
                    d: arrays[7][i] as i32,
                },
                opacity: opacity[i],
            });
        }

        // TYPE
        let mut r = Reader::new(take(&c, TYPE)?, TYPE);
        let (sc, zero) = r.sub_header()?;
        if zero != 0 {
            return Err(r.err("reserved field is not zero"));
        }
        if sc as usize > limits.max_slots {
            return Err(diag(codes::TOO_MANY_SLOTS, format!("{sc} slots")));
        }
        let mut slots = Vec::with_capacity(sc as usize);
        for _ in 0..sc {
            let name = r.string()?;
            let kind = r.u8()?;
            let ty = match kind {
                SLOT_SCALAR | SLOT_INT => {
                    let (min, max) = (r.i32()?, r.i32()?);
                    if kind == SLOT_SCALAR {
                        SlotType::Scalar { min, max }
                    } else {
                        SlotType::Int { min, max }
                    }
                }
                SLOT_BOOL => SlotType::Bool,
                SLOT_COLOUR => SlotType::Colour,
                SLOT_ENUM => {
                    let k = usize::from(r.u16()?);
                    let mut labels = Vec::with_capacity(k.min(r.buf.len()));
                    for _ in 0..k {
                        labels.push(r.string()?);
                    }
                    SlotType::Enum(labels)
                }
                other => {
                    return Err(diag(
                        codes::BINDING_TYPE,
                        format!("unknown slot kind {other}"),
                    ))
                }
            };
            let default = match ty {
                SlotType::Scalar { .. } => SlotValue::Scalar(r.i32()?),
                SlotType::Int { .. } => SlotValue::Int(r.i32()?),
                SlotType::Bool => match r.u8()? {
                    0 => SlotValue::Bool(false),
                    1 => SlotValue::Bool(true),
                    _ => return Err(r.err("bool default is not 0 or 1")),
                },
                SlotType::Enum(_) => SlotValue::Enum(r.u16()?),
                SlotType::Colour => SlotValue::Colour(r.rgba()?),
            };
            slots.push(Slot { name, ty, default });
        }
        r.finish()?;

        // SLOT
        let mut r = Reader::new(take(&c, SLOT)?, SLOT);
        let (bc, zero) = r.sub_header()?;
        if zero != 0 {
            return Err(r.err("reserved field is not zero"));
        }
        if bc as usize > limits.max_bindings {
            return Err(diag(codes::TOO_MANY_BINDINGS, format!("{bc} bindings")));
        }
        let mut bindings = Vec::with_capacity(bc as usize);
        for _ in 0..bc {
            let slot = r.u16()?;
            let tk = r.u8()?;
            let mk = r.u8()?;
            let i = r.u32()?;
            let target = match tk {
                0 => Target::Opacity(i),
                1 => Target::FillColour(i),
                2 => Target::StrokeColour(i),
                3 => Target::TranslateX(i),
                4 => Target::TranslateY(i),
                5 => Target::Scale {
                    instance: i,
                    pivot: Point::new(r.i32()?, r.i32()?),
                },
                6 => Target::Visible(i),
                7 => Target::Geometry(i),
                other => {
                    return Err(diag(
                        codes::BINDING_TYPE,
                        format!("unknown target kind {other}"),
                    ))
                }
            };
            let map = match mk {
                MAP_LINEAR => Map::Linear {
                    mul: r.i32()?,
                    add: r.i32()?,
                },
                MAP_MIX => Map::Mix(r.rgba()?, r.rgba()?),
                MAP_TABLE => {
                    let k = usize::from(r.u16()?);
                    if k > limits.max_table {
                        return Err(diag(codes::TABLE_TOO_LONG, format!("table of {k}")));
                    }
                    let mut entries = Vec::with_capacity(k);
                    for _ in 0..k {
                        let w = r.bytes(4)?;
                        entries.push(match target {
                            Target::FillColour(_) | Target::StrokeColour(_) => {
                                Value::Colour(Rgba8::new(w[0], w[1], w[2], w[3]))
                            }
                            Target::Visible(_) => match w {
                                [0, 0, 0, 0] => Value::Bool(false),
                                [1, 0, 0, 0] => Value::Bool(true),
                                _ => return Err(r.err("bool entry is not 0 or 1")),
                            },
                            Target::Geometry(_) => {
                                Value::Geometry(u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
                            }
                            _ => Value::Number(i32::from_le_bytes([w[0], w[1], w[2], w[3]])),
                        });
                    }
                    Map::Table(entries)
                }
                other => {
                    return Err(diag(
                        codes::BINDING_TYPE,
                        format!("unknown map kind {other}"),
                    ))
                }
            };
            bindings.push(Binding { slot, target, map });
        }
        r.finish()?;

        // SPAT
        let mut r = Reader::new(take(&c, SPAT)?, SPAT);
        let (sn, rc) = r.sub_header()?;
        if sn != n {
            return Err(diag(codes::COUNT_MISMATCH, "SPAT does not match INST"));
        }
        if rc as usize > limits.max_regions {
            return Err(diag(codes::TOO_MANY_REGIONS, format!("{rc} regions")));
        }
        let mut local_bounds = Vec::with_capacity(n_us);
        for _ in 0..n_us {
            let rect = Rect {
                min_x: r.i32()?,
                min_y: r.i32()?,
                max_x: r.i32()?,
                max_y: r.i32()?,
            };
            if rect.min_x > rect.max_x || rect.min_y > rect.max_y {
                return Err(r.err("inverted bounds"));
            }
            local_bounds.push(rect);
        }
        let mut regions = Vec::with_capacity(rc as usize);
        for _ in 0..rc {
            let id = r.string()?;
            let role = Role::from_id(r.u8()?)
                .ok_or_else(|| diag(codes::REGION_INVALID, "unknown region role"))?;
            let precise = match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(r.err("precise flag is not 0 or 1")),
            };
            let focus = r.u16()?;
            let instance = r.u32()?;
            regions.push(Region {
                id,
                role,
                instance,
                focus,
                precise,
            });
        }
        r.finish()?;

        let provenance = match c
            .section(META)
            .map_err(|e| diag(e.kind.code(), e.to_string()))?
        {
            None => None,
            Some(m) => {
                let mut r = Reader::new(m, META);
                let len = r.u32()? as usize;
                r.zeros(12)?;
                let text = std::str::from_utf8(r.bytes(len)?)
                    .map_err(|_| r.err("provenance is not UTF-8"))?
                    .to_string();
                r.finish()?;
                Some(text)
            }
        };

        let base_styles: Vec<Style> = base.iter().map(|b| styles[b.style as usize]).collect();
        let typed = Typed {
            slots: &slots,
            bindings: &bindings,
            regions: &regions,
            base_styles,
            geometries: paths.len() as u32,
        };
        typed.check(limits)?;

        // CHNG must equal what the bindings imply.
        let invalidation = typed.invalidation();
        let mut r = Reader::new(take(&c, CHNG)?, CHNG);
        let (cn, zero) = r.sub_header()?;
        if cn != sc || zero != 0 {
            return Err(diag(
                codes::INVALIDATION_MISMATCH,
                "CHNG does not cover every slot",
            ));
        }
        for (mask, insts) in &invalidation {
            let m = r.u32()?;
            let k = r.u32()? as usize;
            if m != *mask || k != insts.len() {
                return Err(diag(
                    codes::INVALIDATION_MISMATCH,
                    "CHNG disagrees with the bindings",
                ));
            }
            for &i in insts {
                if r.u32()? != i {
                    return Err(diag(
                        codes::INVALIDATION_MISMATCH,
                        "CHNG disagrees with the bindings",
                    ));
                }
            }
        }
        r.finish()?;

        // Stored bounds must cover every geometry each instance can show.
        for (i, b) in base.iter().enumerate() {
            let mut need = paths[b.geometry as usize].bounds();
            for bd in &bindings {
                if let (Target::Geometry(t), Map::Table(entries)) = (&bd.target, &bd.map) {
                    if *t as usize == i {
                        for e in entries {
                            if let Value::Geometry(g) = e {
                                need = union(need, paths[*g as usize].bounds());
                            }
                        }
                    }
                }
            }
            if union(need, local_bounds[i]) != local_bounds[i] {
                return Err(diag(
                    codes::INVALIDATION_MISMATCH,
                    format!("SPAT bounds of instance {i} are too small"),
                ));
            }
        }

        let mut by_slot = vec![Vec::new(); slots.len()];
        for (k, b) in bindings.iter().enumerate() {
            by_slot[usize::from(b.slot)].push(k);
        }
        Ok(Unit {
            width,
            height,
            provenance,
            paths,
            styles,
            base,
            slots,
            bindings,
            by_slot,
            invalidation,
            local_bounds,
            regions,
        })
    }

    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    pub fn slot_index(&self, name: &str) -> Option<u16> {
        self.slots
            .iter()
            .position(|s| s.name == name)
            .map(|i| i as u16)
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn instance_count(&self) -> usize {
        self.base.len()
    }

    pub fn base(&self, i: usize) -> Option<&BaseInstance> {
        self.base.get(i)
    }

    /// The decoded geometry pool (base geometry plus variants).
    pub fn geometry(&self) -> &[Arc<Path>] {
        &self.paths
    }

    pub fn styles(&self) -> &[Style] {
        &self.styles
    }

    /// `CHNG`: per slot, the channel mask and the instances it can touch.
    pub fn invalidation_table(&self) -> &[(u32, Vec<u32>)] {
        &self.invalidation
    }

    pub fn local_bounds(&self, i: usize) -> Option<Rect> {
        self.local_bounds.get(i).copied()
    }

    /// A fresh runtime state with every slot at its default.
    pub fn instantiate(&self) -> Result<UnitState<'_>, GfxDiag> {
        UnitState::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_graphics_container::Limits;
    use goya_graphics_ir::reference::{render, RenderLimits};
    use goya_graphics_ir::{Fill, FillRule, Item, Segment, FIXED_ONE};

    fn pt(x: i32, y: i32) -> Point {
        Point::new(x, y)
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

    fn triangle() -> Path {
        use Segment::*;
        Path::new(vec![
            MoveTo(pt(107_000, 26_000)),
            LineTo(pt(114_000, 30_000)),
            LineTo(pt(107_000, 34_000)),
            Close,
        ])
        .unwrap()
    }

    fn bars() -> Path {
        let mut segs = rect(106_500, 26_000, 109_000, 34_000).segments().to_vec();
        segs.extend_from_slice(rect(111_000, 26_000, 113_500, 34_000).segments());
        Path::new(segs).unwrap()
    }

    fn fill(r: u8, g: u8, b: u8) -> Style {
        Style {
            fill: Some(Fill {
                colour: Rgba8::new(r, g, b, 255),
                rule: FillRule::NonZero,
            }),
            stroke: None,
        }
    }

    fn item(path: Path, style: Style) -> Item {
        Item {
            path: Arc::new(path),
            style,
            transform: Affine::IDENTITY,
            opacity: 255,
        }
    }

    const GREEN: Rgba8 = Rgba8::new(40, 200, 90, 255);
    const AMBER: Rgba8 = Rgba8::new(255, 170, 0, 255);
    const RED: Rgba8 = Rgba8::new(220, 40, 40, 255);

    /// Slots: 0–3 level (scalar 0..1), 4–7 status (enum), 8 dim (scalar),
    /// 9 editable (bool), 10 mode (enum), 11 hover (scalar).
    /// Items per gauge g: 4g track, 4g+1 bar, 4g+2 lamp, 4g+3 label;
    /// 16 button, 17 glyph.
    fn dashboard() -> UnitSource {
        let mut items = Vec::new();
        for g in 0..4 {
            let x0 = g * 25_000;
            items.push(item(
                rect(x0 + 2_000, 2_000, x0 + 8_000, 40_000),
                fill(50, 54, 62),
            ));
            items.push(item(
                rect(x0 + 3_000, 3_000, x0 + 7_000, 39_000),
                fill(42, 130, 255),
            ));
            items.push(item(circle(x0 + 5_000, 46_000, 2_500), fill(120, 120, 120)));
            items.push(item(
                rect(x0 + 1_000, 50_000, x0 + 9_000, 52_000),
                fill(230, 230, 235),
            ));
        }
        items.push(item(circle(110_000, 30_000, 6_000), fill(70, 70, 80)));
        items.push(item(triangle(), fill(255, 255, 255)));
        let q = |v: f64| match SlotValue::scalar(v) {
            Some(SlotValue::Scalar(x)) => x,
            _ => unreachable!(),
        };
        let mut slots = Vec::new();
        for g in 0..4 {
            slots.push(Slot {
                name: format!("level{g}"),
                ty: SlotType::Scalar {
                    min: 0,
                    max: q(1.0),
                },
                default: SlotValue::Scalar(q(0.5)),
            });
        }
        for g in 0..4 {
            slots.push(Slot {
                name: format!("status{g}"),
                ty: SlotType::Enum(vec!["ok".into(), "warn".into(), "fault".into()]),
                default: SlotValue::Enum(0),
            });
        }
        slots.push(Slot {
            name: "dim".into(),
            ty: SlotType::Scalar {
                min: 0,
                max: q(1.0),
            },
            default: SlotValue::Scalar(0),
        });
        slots.push(Slot {
            name: "editable".into(),
            ty: SlotType::Bool,
            default: SlotValue::Bool(true),
        });
        slots.push(Slot {
            name: "mode".into(),
            ty: SlotType::Enum(vec!["run".into(), "pause".into()]),
            default: SlotValue::Enum(0),
        });
        slots.push(Slot {
            name: "hover".into(),
            ty: SlotType::Scalar {
                min: 0,
                max: q(1.0),
            },
            default: SlotValue::Scalar(0),
        });
        let mut bindings = Vec::new();
        for g in 0..4u32 {
            let x0 = g as i32 * 25_000;
            bindings.push(Binding {
                slot: g as u16,
                target: Target::Scale {
                    instance: 4 * g + 1,
                    pivot: pt(x0 + 5_000, 39_000),
                },
                // 0.1 + 0.9 · level
                map: Map::Linear {
                    mul: q(0.9),
                    add: q(0.1),
                },
            });
            bindings.push(Binding {
                slot: 4 + g as u16,
                target: Target::FillColour(4 * g + 2),
                map: Map::Table(vec![
                    Value::Colour(GREEN),
                    Value::Colour(AMBER),
                    Value::Colour(RED),
                ]),
            });
            bindings.push(Binding {
                slot: 8,
                target: Target::Opacity(4 * g + 3),
                // 255 − 155 · dim
                map: Map::Linear {
                    mul: -155,
                    add: 255,
                },
            });
        }
        for i in [16, 17] {
            bindings.push(Binding {
                slot: 9,
                target: Target::Visible(i),
                map: Map::Table(vec![Value::Bool(false), Value::Bool(true)]),
            });
        }
        bindings.push(Binding {
            slot: 10,
            target: Target::Geometry(17),
            map: Map::Table(vec![
                Value::Path(Arc::new(triangle())),
                Value::Path(Arc::new(bars())),
            ]),
        });
        bindings.push(Binding {
            slot: 11,
            target: Target::FillColour(16),
            map: Map::Mix(Rgba8::new(70, 70, 80, 255), Rgba8::new(42, 130, 255, 255)),
        });
        let mut regions: Vec<Region> = (0..4u32)
            .map(|g| Region {
                id: format!("gauge{g}"),
                role: Role::Slider,
                instance: 4 * g,
                focus: g as u16,
                precise: false,
            })
            .collect();
        regions.push(Region {
            id: "reset".into(),
            role: Role::Button,
            instance: 16,
            focus: 10,
            precise: true,
        });
        UnitSource {
            drawing: Drawing {
                width: 120_000,
                height: 60_000,
                items,
            },
            slots,
            bindings,
            regions,
            provenance: Some("goya-gru tests".into()),
        }
    }

    fn lim() -> GruLimits {
        GruLimits::default()
    }

    fn unit() -> Unit {
        Unit::open(&encode(&dashboard(), &lim()).unwrap(), &lim()).unwrap()
    }

    fn scalar(v: f64) -> SlotValue {
        SlotValue::scalar(v).unwrap()
    }

    #[test]
    fn identical_geometry_is_stored_once() {
        let u = unit();
        // Track, bar, lamp and label are each one geometry shared by four
        // gauges, plus the button, the triangle and the bars variant.
        assert_eq!(u.geometry().len(), 7);
        assert_eq!(u.instance_count(), 18);
        assert_eq!(u.provenance.as_deref(), Some("goya-gru tests"));
    }

    #[test]
    fn modulation_only_update_rebuilds_no_geometry() {
        let u = unit();
        let mut st = u.instantiate().unwrap();
        let before: Vec<Option<Item>> = (0..18).map(|i| st.item(i)).collect();
        st.take_invalidation();
        let evals = st.evaluations();
        st.set_named("status2", SlotValue::Enum(2)).unwrap();
        let inv = st.take_invalidation();
        assert_eq!(inv.instances, vec![10]);
        assert_eq!(inv.channels, channel::FILL);
        assert!(inv.modulation_only());
        assert_eq!(st.evaluations() - evals, 1, "only the one binding ran");
        for (i, a) in before.iter().enumerate() {
            let (a, b) = (a.as_ref().unwrap(), st.item(i).unwrap());
            assert!(Arc::ptr_eq(&a.path, &b.path), "geometry of {i} was rebuilt");
            assert_eq!(a.transform, b.transform);
            if i != 10 {
                assert_eq!(*a, b, "instance {i} changed");
            }
        }
        assert_eq!(st.item(10).unwrap().style.fill.unwrap().colour, RED);
        // The dirty region covers the lamp only.
        let r = inv.region.unwrap();
        assert!(
            r.min_x >= 50_000 + 2_500 - 1 && r.max_x <= 50_000 + 7_500 + 1,
            "{r:?}"
        );
        // Setting the same value again is free.
        st.set_named("status2", SlotValue::Enum(2)).unwrap();
        assert_eq!(st.take_invalidation(), Invalidation::default());
    }

    #[test]
    fn transform_and_geometry_updates_are_local() {
        let u = unit();
        let mut st = u.instantiate().unwrap();
        st.take_invalidation();
        st.set_named("level1", scalar(1.0)).unwrap();
        let inv = st.take_invalidation();
        assert_eq!(
            (inv.instances.clone(), inv.channels),
            (vec![5], channel::SCALE)
        );
        assert!(!inv.modulation_only());
        // Full level: scale 1.0, the bar fills its track.
        let b = st.instance_bounds(5).unwrap();
        assert_eq!(
            (b.min_x, b.min_y, b.max_x, b.max_y),
            (28_000, 3_000, 32_000, 39_000)
        );

        let glyph = st.item(17).unwrap().path;
        st.set_named("mode", SlotValue::Enum(1)).unwrap();
        let inv = st.take_invalidation();
        assert_eq!((inv.instances, inv.channels), (vec![17], channel::GEOMETRY));
        let paused = st.item(17).unwrap().path;
        assert!(!Arc::ptr_eq(&glyph, &paused));
        assert!(
            u.geometry().iter().any(|g| Arc::ptr_eq(g, &paused)),
            "variant comes from the load-time pool"
        );
        st.set_named("mode", SlotValue::Enum(0)).unwrap();
        assert!(Arc::ptr_eq(&glyph, &st.item(17).unwrap().path));
    }

    #[test]
    fn invalid_slot_values_are_rejected_deterministically() {
        let u = unit();
        let mut st = u.instantiate().unwrap();
        let before = st.drawing();
        let cases = [
            ("level0", scalar(1.5), codes::SLOT_OUT_OF_DOMAIN),
            ("level0", SlotValue::Int(1), codes::SLOT_TYPE_MISMATCH),
            ("status0", SlotValue::Enum(3), codes::SLOT_OUT_OF_DOMAIN),
            ("editable", SlotValue::Enum(0), codes::SLOT_TYPE_MISMATCH),
        ];
        for (name, v, code) in cases {
            assert_eq!(
                st.set_named(name, v).unwrap_err().code,
                code,
                "{name} = {v:?}"
            );
        }
        assert_eq!(
            st.set_named("nope", SlotValue::Bool(true))
                .unwrap_err()
                .code,
            codes::UNKNOWN_SLOT
        );
        assert_eq!(
            st.set(99, SlotValue::Bool(true)).unwrap_err().code,
            codes::UNKNOWN_SLOT
        );
        assert_eq!(st.drawing(), before, "rejected updates change nothing");
    }

    #[test]
    fn incremental_state_equals_fresh_evaluation() {
        let u = unit();
        let mut a = u.instantiate().unwrap();
        let steps = [
            ("level0", scalar(0.25)),
            ("status3", SlotValue::Enum(1)),
            ("dim", scalar(0.5)),
            ("hover", scalar(0.75)),
            ("mode", SlotValue::Enum(1)),
            ("level0", scalar(0.8)),
            ("editable", SlotValue::Bool(false)),
        ];
        for (n, v) in steps {
            a.set_named(n, v).unwrap();
        }
        // Same final values applied in reverse order to a fresh state.
        let mut b = u.instantiate().unwrap();
        for (n, v) in steps.iter().rev() {
            if *n != "level0" || *v == scalar(0.8) {
                b.set_named(n, *v).unwrap();
            }
        }
        assert_eq!(a.drawing(), b.drawing());
        let rl = RenderLimits::default();
        assert_eq!(
            render(&a.drawing(), 2.0, &rl).unwrap(),
            render(&b.drawing(), 2.0, &rl).unwrap()
        );
        // Mix: hover 0.75 between (70,70,80) and (42,130,255).
        a.set_named("editable", SlotValue::Bool(true)).unwrap();
        assert_eq!(
            a.item(16).unwrap().style.fill.unwrap().colour,
            Rgba8::new(49, 115, 211, 255)
        );
        // Linear: dim 0.5 → round_half_even(−77.5) + 255 = −78 + 255 = 177.
        assert_eq!(a.item(3).unwrap().opacity, 177);
    }

    #[test]
    fn visionless_interaction() {
        let u = unit();
        let mut st = u.instantiate().unwrap();
        // Inside gauge 2's track bounds.
        assert_eq!(
            st.hit_test(pt(55_000, 20_000)).map(|r| r.id.as_str()),
            Some("gauge2")
        );
        // Button centre hits; its bounding-box corner misses (precise circle).
        assert_eq!(
            st.hit_test(pt(110_000, 30_000)).map(|r| r.id.as_str()),
            Some("reset")
        );
        assert_eq!(st.hit_test(pt(104_500, 24_500)), None);
        let order: Vec<&str> = st.focus_order().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(order, vec!["gauge0", "gauge1", "gauge2", "gauge3", "reset"]);
        assert_eq!(
            st.focus_next("reset").map(|r| r.id.as_str()),
            Some("gauge0")
        );
        // Hidden instances leave hit-testing and focus.
        st.set_named("editable", SlotValue::Bool(false)).unwrap();
        assert_eq!(st.hit_test(pt(110_000, 30_000)), None);
        assert!(st.focus_order().iter().all(|r| r.id != "reset"));
        // Regions follow transforms: a shrunken bar does not change the track
        // region, but the bounds query reflects the new scale.
        st.set_named("level0", scalar(0.0)).unwrap();
        let b = st.instance_bounds(1).unwrap();
        assert!(b.max_y - b.min_y < 4_000, "{b:?}");
    }

    #[test]
    fn type_errors_are_actionable() {
        let base = dashboard();
        let check = |edit: &dyn Fn(&mut UnitSource), code: u16| {
            let mut s = base.clone();
            edit(&mut s);
            let e = encode(&s, &lim()).unwrap_err();
            assert_eq!(e.code, code, "{e}");
            assert!(!e.detail.is_empty());
        };
        check(
            &|s| s.bindings[0].map = Map::Mix(GREEN, RED),
            codes::BINDING_TYPE,
        );
        check(
            &|s| {
                s.bindings.push(Binding {
                    slot: 8,
                    target: Target::Opacity(3),
                    map: Map::Linear { mul: 0, add: 1 },
                })
            },
            codes::BINDING_CONFLICT,
        );
        check(
            &|s| s.bindings[1].map = Map::Table(vec![Value::Colour(GREEN)]),
            codes::BINDING_TYPE,
        );
        check(
            &|s| s.bindings[2].map = Map::Linear { mul: 0, add: 300 },
            codes::BINDING_TYPE,
        );
        check(
            &|s| s.bindings[1].target = Target::StrokeColour(2),
            codes::BINDING_TYPE,
        );
        check(&|s| s.bindings[0].slot = 77, codes::UNKNOWN_SLOT);
        check(&|s| s.regions[1].focus = 0, codes::REGION_INVALID);
        check(
            &|s| {
                s.regions.push(Region {
                    id: "x".into(),
                    role: Role::Image,
                    instance: 99,
                    focus: 50,
                    precise: false,
                })
            },
            codes::REGION_INVALID,
        );
        check(
            &|s| s.slots[0].default = scalar(2.0),
            codes::SLOT_OUT_OF_DOMAIN,
        );
        check(
            &|s| {
                s.bindings.push(Binding {
                    slot: 9,
                    target: Target::Visible(0),
                    map: Map::Linear { mul: 0, add: 1 },
                })
            },
            codes::BINDING_TYPE,
        );
        let tiny = GruLimits {
            max_table: 2,
            ..lim()
        };
        assert_eq!(
            encode(&base, &tiny).unwrap_err().code,
            codes::TABLE_TOO_LONG
        );
    }

    #[test]
    fn deterministic_and_golden() {
        let a = encode(&dashboard(), &lim()).unwrap();
        assert_eq!(a, encode(&dashboard(), &lim()).unwrap());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("fixtures")
            .join("graphics")
            .join("gru")
            .join("dashboard.gru");
        // Regenerate only with a spec change: GOYA_GFX_BLESS=1 cargo test -p goya-gru golden
        if std::env::var_os("GOYA_GFX_BLESS").is_some() {
            std::fs::write(&path, &a).unwrap();
        }
        assert_eq!(
            a,
            std::fs::read(&path).expect("golden fixture missing"),
            "GRU byte layout changed"
        );
    }

    #[test]
    fn transform_overflow_is_atomic() {
        let mut src = dashboard();
        src.drawing.items.push(item(
            rect(2_100_000_000, 0, 2_100_001_000, 1_000),
            fill(1, 2, 3),
        ));
        src.slots.push(Slot {
            name: "shift".into(),
            ty: SlotType::Int {
                min: 0,
                max: 100_000_000,
            },
            default: SlotValue::Int(0),
        });
        src.bindings.push(Binding {
            slot: 12,
            target: Target::TranslateX(18),
            map: Map::Linear {
                mul: FIXED_ONE,
                add: 0,
            },
        });
        let u = Unit::open(&encode(&src, &lim()).unwrap(), &lim()).unwrap();
        let mut st = u.instantiate().unwrap();
        let before = st.drawing();
        assert_eq!(
            st.set_named("shift", SlotValue::Int(100_000_000))
                .unwrap_err()
                .code,
            codes::COORDINATE_OVERFLOW
        );
        assert_eq!(st.drawing(), before);
        assert_eq!(st.get(12), Some(SlotValue::Int(0)));
        st.set_named("shift", SlotValue::Int(1_000)).unwrap();
    }

    // ---------------------------------------------------------------------------
    // Tampering behind valid container digests
    // ---------------------------------------------------------------------------

    fn rebuild(bytes: &[u8], target: Tag, edit: &mut dyn FnMut(&mut Vec<u8>)) -> Vec<u8> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gru,
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
                if e.tag == target {
                    edit(&mut d);
                }
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
            Format::Gru,
            Profile::Baseline,
            0,
            &inputs,
            &Limits::default(),
        )
        .unwrap()
    }

    #[test]
    fn tampering_is_caught() {
        let base = encode(&dashboard(), &lim()).unwrap();
        // CHNG: claim slot 0 touches a different instance.
        let chng = rebuild(&base, CHNG, &mut |d| d[24] ^= 1);
        assert_eq!(
            Unit::open(&chng, &lim()).unwrap_err().code,
            codes::INVALIDATION_MISMATCH
        );
        // SPAT: shrink instance 0's bounds.
        let spat = rebuild(&base, SPAT, &mut |d| d[16] = d[16].wrapping_add(100));
        assert_eq!(
            Unit::open(&spat, &lim()).unwrap_err().code,
            codes::INVALIDATION_MISMATCH
        );
        // HEAD: unknown execution profile.
        let head = rebuild(&base, HEAD, &mut |d| d[10] = 1);
        assert_eq!(
            Unit::open(&head, &lim()).unwrap_err().code,
            codes::BINDING_TYPE
        );
    }

    #[test]
    fn payload_fuzz_never_panics() {
        let base = encode(&dashboard(), &lim()).unwrap();
        let tags = [CHNG, GEOM, HEAD, INST, META, MODS, SLOT, SPAT, STYL, TYPE];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..3_000 {
            let tag = tags[(next() % tags.len() as u64) as usize];
            let (r1, r2, r3) = (next(), next(), next());
            let mutated = rebuild(&base, tag, &mut |d| {
                if d.is_empty() {
                    return;
                }
                for k in 0..(1 + r1 % 3) {
                    let i = ((r2 >> (k * 16)) % d.len() as u64) as usize;
                    d[i] = (r3 >> (k * 8)) as u8;
                }
                if r1 % 7 == 0 {
                    d.truncate((r2 % d.len() as u64) as usize);
                }
            });
            if let Ok(u) = Unit::open(&mutated, &lim()) {
                if let Ok(mut st) = u.instantiate() {
                    for (k, s) in u.slots().iter().enumerate() {
                        let v = match &s.ty {
                            SlotType::Scalar { max, .. } => SlotValue::Scalar(*max),
                            SlotType::Int { min, .. } => SlotValue::Int(*min),
                            SlotType::Bool => SlotValue::Bool(r1 & 1 == 0),
                            SlotType::Enum(l) => {
                                SlotValue::Enum((l.len() as u16).saturating_sub(1))
                            }
                            SlotType::Colour => SlotValue::Colour(Rgba8::new(1, 2, 3, 4)),
                        };
                        let _ = st.set(k as u16, v);
                    }
                    let _ = st.drawing();
                    let _ = st.hit_test(pt(50_000, 20_000));
                    let _ = st.take_invalidation();
                }
            }
        }
    }
}
