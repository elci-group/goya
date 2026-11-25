//! The parent state model GVI is compiled from, KFBM eligibility, reachable
//! state enumeration, and the model's canonical binary form (the `STAT` and
//! `TRAN` sections).
//!
//! The model is declarative. Effects come from a closed, typed set, so the
//! compiler knows the semantics of every state ("Known"). No user code ever
//! runs to produce a projection.

use goya_graphics_ir::{codes, Affine, GfxDiag, Path, Style};
use goya_gva::{
    decode_path_bytes, decode_style_record, encode_path_bytes, encode_style_record, GvaLimits,
    STYLE_RECORD,
};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// Which source items an effect applies to. Item ids are indices into the
/// source drawing's item list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selector {
    All,
    /// Strictly ascending item ids.
    Items(Vec<u32>),
}

impl Selector {
    pub fn matches(&self, item: u32) -> bool {
        match self {
            Selector::All => true,
            Selector::Items(ids) => ids.binary_search(&item).is_ok(),
        }
    }
}

/// What a state value does to the selected items. Effects run in order:
/// dimensions in declaration order, then each value's effects in order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Effect {
    /// Apply `Affine` after the item's own transform (`outer ∘ item`),
    /// rounded per `Affine::then`.
    Transform(Selector, Affine),
    /// Multiply opacity: `o' = (o · factor + 127) / 255`.
    Opacity(Selector, u8),
    /// Replace the item's style.
    Restyle(Selector, Style),
    /// Leave the item out of the projection.
    Hide(Selector),
    /// Interpolate the item's current geometry toward `target` at
    /// `t / 65536` (0 ≤ t ≤ 65536; see `morph_path`). `target` is authored
    /// in the source item's own coordinates and must have the same segment
    /// structure as the geometry it morphs. Changing geometry is exactly
    /// the work GVI moves to build time.
    Morph(Selector, Arc<Path>, u32),
}

/// One value of a discrete dimension.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Value {
    pub label: String,
    pub effects: Vec<Effect>,
}

impl Value {
    pub fn new(label: &str) -> Value {
        Value {
            label: label.to_string(),
            effects: Vec::new(),
        }
    }

    pub fn with(mut self, effect: Effect) -> Value {
        self.effects.push(effect);
        self
    }
}

/// A state dimension's domain as the compiler understands it.
#[derive(Debug, Clone, PartialEq)]
pub enum Domain {
    /// A finite, known list of values. Only this kind is GVI-eligible.
    Discrete(Vec<Value>),
    /// A bounded continuous range: Known and Bounded, but not Finite. This
    /// is an SPC candidate.
    Continuous { min: f64, max: f64 },
    /// A value with no enforceable limit.
    Unbounded,
    /// State whose graphical semantics the compiler cannot see, for
    /// example a value computed by arbitrary code.
    Opaque,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    pub name: String,
    pub domain: Domain,
}

impl Dimension {
    pub fn discrete(name: &str, values: Vec<Value>) -> Dimension {
        Dimension {
            name: name.to_string(),
            domain: Domain::Discrete(values),
        }
    }

    fn values(&self) -> &[Value] {
        match &self.domain {
            Domain::Discrete(v) => v,
            _ => &[],
        }
    }
}

/// A permitted state change: dimension `dim` moves to value `to`, from
/// value `from` (or from any value), when every `(dim, value)` guard holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rule {
    pub dim: u16,
    pub from: Option<u16>,
    pub to: u16,
    pub guard: Vec<(u16, u16)>,
}

/// How the reachable state set is derived.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Reachability {
    /// Every combination is reachable except those matching an exclusion.
    /// An exclusion matches when all of its `(dim, value)` pairs hold. Any
    /// reachable state may change to any other.
    Product { exclusions: Vec<Vec<(u16, u16)>> },
    /// States reachable from `initial` through `rules`. Only rule steps are
    /// permitted transitions.
    Transitions { initial: Vec<u16>, rules: Vec<Rule> },
}

/// The parent application's graphical state model for one graphic.
#[derive(Debug, Clone, PartialEq)]
pub struct StateModel {
    /// Identity of the consuming parent (for example a `.gscene` scene id).
    pub parent: String,
    pub dimensions: Vec<Dimension>,
    pub reachability: Reachability,
}

/// Compile-time budgets for "Manageable".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_dimensions: usize,
    pub max_values_per_dimension: usize,
    /// Largest reachable state count.
    pub max_states: u64,
    /// Most candidate states the enumerator may examine.
    pub max_enumeration: u64,
    /// Total instance rows across all distinct projections.
    pub max_rows: u64,
    /// Largest encoded `.gvi`.
    pub max_bytes: u64,
}

impl Default for Budget {
    fn default() -> Budget {
        Budget {
            max_dimensions: 16,
            max_values_per_dimension: 1024,
            max_states: 65_536,
            max_enumeration: 1 << 20,
            max_rows: 1 << 22,
            max_bytes: 16 << 20,
        }
    }
}

/// The KFBM assessment of a model (directive §8.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Assessment {
    pub known: bool,
    pub finite: bool,
    pub bounded: bool,
    /// `None` when not every criterion before it held.
    pub manageable: Option<bool>,
    /// Size of the full Cartesian product, when finite and representable.
    pub domain_size: Option<u64>,
    /// Number of reachable states, when enumeration ran.
    pub reachable: Option<u64>,
    pub reasons: Vec<String>,
}

impl Assessment {
    pub fn eligible(&self) -> bool {
        self.known && self.finite && self.bounded && self.manageable == Some(true)
    }

    /// `Ok` when eligible. Otherwise an error for the first failed criterion,
    /// with the alternative representation the compiler should use.
    pub fn verdict(&self) -> Result<(), GfxDiag> {
        let why = self.reasons.join("; ");
        if !self.known {
            return Err(GfxDiag::new(
                codes::GVI_NOT_KNOWN,
                format!("not Known ({why}); keep this graphic as GRU/GVA and evaluate at runtime"),
            ));
        }
        if !self.bounded {
            return Err(GfxDiag::new(
                codes::GVI_NOT_BOUNDED,
                format!("not Bounded ({why}); keep this graphic as GRU/GVA"),
            ));
        }
        if !self.finite {
            return Err(GfxDiag::new(
                codes::GVI_NOT_FINITE,
                format!(
                    "not Finite ({why}); use SPC if approximation is permitted, otherwise GRU/GVA"
                ),
            ));
        }
        if self.manageable != Some(true) {
            return Err(GfxDiag::new(
                codes::GVI_NOT_MANAGEABLE,
                format!("not Manageable ({why}); use SPC if approximation is permitted, otherwise GRU/GVA"),
            ));
        }
        Ok(())
    }
}

fn invalid(detail: impl Into<String>) -> GfxDiag {
    GfxDiag::new(codes::GVI_INVALID_MODEL, detail)
}

/// Mixed-radix packing of a state into a `u64` key. Dimension 0 is the most
/// significant digit, so sorting keys sorts states lexicographically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Radix {
    radices: Vec<u64>,
    strides: Vec<u64>,
    pub domain: u64,
}

impl Radix {
    /// `None` if the product does not fit `u64`.
    pub fn new(radices: Vec<u64>) -> Option<Radix> {
        let mut strides = vec![0u64; radices.len()];
        let mut acc = 1u64;
        for i in (0..radices.len()).rev() {
            strides[i] = acc;
            acc = acc.checked_mul(radices[i])?;
        }
        Some(Radix {
            radices,
            strides,
            domain: acc,
        })
    }

    pub fn dims(&self) -> usize {
        self.radices.len()
    }

    pub fn radix(&self, d: usize) -> u64 {
        self.radices[d]
    }

    /// `None` if the state has the wrong arity or a value is out of range.
    pub fn pack(&self, state: &[u16]) -> Option<u64> {
        if state.len() != self.radices.len() {
            return None;
        }
        let mut key = 0u64;
        for (i, &v) in state.iter().enumerate() {
            if u64::from(v) >= self.radices[i] {
                return None;
            }
            key += u64::from(v) * self.strides[i];
        }
        Some(key)
    }

    pub fn unpack(&self, key: u64) -> Vec<u16> {
        self.radices
            .iter()
            .zip(&self.strides)
            .map(|(&r, &s)| ((key / s) % r) as u16)
            .collect()
    }
}

/// Apply `rule` to `state`. Returns the successor, or `None` when the rule
/// does not apply or would not change anything.
pub fn step(rule: &Rule, state: &[u16]) -> Option<Vec<u16>> {
    let d = rule.dim as usize;
    let cur = *state.get(d)?;
    if cur == rule.to || rule.from.is_some_and(|f| f != cur) {
        return None;
    }
    if !rule
        .guard
        .iter()
        .all(|&(gd, gv)| state.get(gd as usize) == Some(&gv))
    {
        return None;
    }
    let mut next = state.to_vec();
    next[d] = rule.to;
    Some(next)
}

pub(crate) fn excluded(exclusions: &[Vec<(u16, u16)>], state: &[u16]) -> bool {
    exclusions
        .iter()
        .any(|ex| ex.iter().all(|&(d, v)| state.get(d as usize) == Some(&v)))
}

impl StateModel {
    pub(crate) fn radix(&self) -> Option<Radix> {
        Radix::new(
            self.dimensions
                .iter()
                .map(|d| d.values().len() as u64)
                .collect(),
        )
    }

    /// Structural validation of a finite model against a source with
    /// `source_items` items.
    pub fn validate(&self, source_items: u32, budget: &Budget) -> Result<(), GfxDiag> {
        if self.parent.is_empty() || self.parent.len() > u16::MAX as usize {
            return Err(invalid("parent id must be 1–65535 bytes"));
        }
        if self.dimensions.is_empty() || self.dimensions.len() > budget.max_dimensions {
            return Err(invalid(format!(
                "1–{} dimensions are allowed",
                budget.max_dimensions
            )));
        }
        let mut names = HashSet::new();
        for dim in &self.dimensions {
            if dim.name.is_empty() || dim.name.len() > u16::MAX as usize || !names.insert(&dim.name)
            {
                return Err(invalid(format!(
                    "dimension name {:?} is empty, too long or duplicated",
                    dim.name
                )));
            }
            let values = match &dim.domain {
                Domain::Discrete(v) => v,
                _ => return Err(invalid(format!("dimension {:?} is not discrete", dim.name))),
            };
            if values.is_empty()
                || values.len() > budget.max_values_per_dimension.min(u16::MAX as usize - 1)
            {
                return Err(invalid(format!(
                    "dimension {:?} has {} values",
                    dim.name,
                    values.len()
                )));
            }
            let mut labels = HashSet::new();
            for v in values {
                if v.label.is_empty()
                    || v.label.len() > u16::MAX as usize
                    || !labels.insert(&v.label)
                {
                    return Err(invalid(format!(
                        "{}: label {:?} is empty, too long or duplicated",
                        dim.name, v.label
                    )));
                }
                if v.effects.len() > u16::MAX as usize {
                    return Err(invalid(format!(
                        "{}={}: too many effects",
                        dim.name, v.label
                    )));
                }
                for e in &v.effects {
                    let sel = match e {
                        Effect::Transform(s, _) | Effect::Opacity(s, _) | Effect::Hide(s) => s,
                        Effect::Morph(s, _, t) => {
                            if *t > 65536 {
                                return Err(invalid(format!(
                                    "{}={}: morph fraction above 1",
                                    dim.name, v.label
                                )));
                            }
                            s
                        }
                        Effect::Restyle(s, style) => {
                            style.validate()?;
                            s
                        }
                    };
                    if let Selector::Items(ids) = sel {
                        if ids.is_empty() || !ids.windows(2).all(|w| w[0] < w[1]) {
                            return Err(invalid(format!(
                                "{}={}: item selectors must be non-empty and strictly ascending",
                                dim.name, v.label
                            )));
                        }
                        if ids.last().is_some_and(|&last| last >= source_items) {
                            return Err(invalid(format!(
                                "{}={}: selects an item the source does not have",
                                dim.name, v.label
                            )));
                        }
                    }
                }
            }
        }
        let radix = self
            .radix()
            .ok_or_else(|| invalid("state domain overflows u64"))?;
        let check_pair = |&(d, v): &(u16, u16)| -> Result<(), GfxDiag> {
            if (d as usize) < radix.dims() && u64::from(v) < radix.radix(d as usize) {
                Ok(())
            } else {
                Err(invalid(format!(
                    "reference to dimension {d} value {v} is out of range"
                )))
            }
        };
        match &self.reachability {
            Reachability::Product { exclusions } => {
                for ex in exclusions {
                    if ex.is_empty() {
                        return Err(invalid("an empty exclusion would exclude every state"));
                    }
                    ex.iter().try_for_each(check_pair)?;
                }
            }
            Reachability::Transitions { initial, rules } => {
                if radix.pack(initial).is_none() {
                    return Err(invalid("initial state is not in the domain"));
                }
                for r in rules {
                    check_pair(&(r.dim, r.to))?;
                    if let Some(f) = r.from {
                        check_pair(&(r.dim, f))?;
                    }
                    r.guard.iter().try_for_each(check_pair)?;
                }
            }
        }
        Ok(())
    }
}

/// Assess KFBM eligibility and, when the first three criteria hold,
/// enumerate the reachable states. Returns the sorted reachable keys
/// alongside the assessment.
pub(crate) fn assess_and_enumerate(
    model: &StateModel,
    budget: &Budget,
) -> (Assessment, Option<Vec<u64>>) {
    let mut a = Assessment {
        known: true,
        finite: true,
        bounded: true,
        manageable: None,
        domain_size: None,
        reachable: None,
        reasons: Vec::new(),
    };
    for d in &model.dimensions {
        match &d.domain {
            Domain::Discrete(_) => {}
            Domain::Continuous { min, max } => {
                a.finite = false;
                if !(min.is_finite() && max.is_finite() && min <= max) {
                    a.bounded = false;
                    a.reasons.push(format!(
                        "{}: continuous range has no valid finite limits",
                        d.name
                    ));
                } else {
                    a.reasons
                        .push(format!("{}: continuous range [{min}, {max}]", d.name));
                }
            }
            Domain::Unbounded => {
                a.finite = false;
                a.bounded = false;
                a.reasons.push(format!("{}: unbounded", d.name));
            }
            Domain::Opaque => {
                a.known = false;
                a.finite = false;
                a.bounded = false;
                a.reasons
                    .push(format!("{}: semantics are opaque to the compiler", d.name));
            }
        }
    }
    if !(a.known && a.finite && a.bounded) {
        return (a, None);
    }
    let Some(radix) = model.radix() else {
        a.manageable = Some(false);
        a.reasons.push("state domain overflows u64".into());
        return (a, None);
    };
    a.domain_size = Some(radix.domain);
    match enumerate(model, &radix, budget) {
        Ok(keys) => {
            a.reachable = Some(keys.len() as u64);
            a.manageable = Some(true);
            (a, Some(keys))
        }
        Err(reason) => {
            a.manageable = Some(false);
            a.reasons.push(reason);
            (a, None)
        }
    }
}

/// The reachable keys in ascending order, or why enumeration was refused.
fn enumerate(model: &StateModel, radix: &Radix, budget: &Budget) -> Result<Vec<u64>, String> {
    let too_many = || format!("more than {} reachable states", budget.max_states);
    match &model.reachability {
        Reachability::Product { exclusions } => {
            if radix.domain > budget.max_enumeration {
                return Err(format!(
                    "product domain of {} exceeds the enumeration budget of {}",
                    radix.domain, budget.max_enumeration
                ));
            }
            let mut keys = Vec::new();
            for key in 0..radix.domain {
                if !excluded(exclusions, &radix.unpack(key)) {
                    if keys.len() as u64 >= budget.max_states {
                        return Err(too_many());
                    }
                    keys.push(key);
                }
            }
            Ok(keys)
        }
        Reachability::Transitions { initial, rules } => {
            let start = radix
                .pack(initial)
                .ok_or("initial state is not in the domain")?;
            let mut seen = HashSet::from([start]);
            let mut queue = VecDeque::from([start]);
            let mut work = 0u64;
            while let Some(key) = queue.pop_front() {
                let state = radix.unpack(key);
                for rule in rules {
                    work += 1;
                    if work > budget.max_enumeration {
                        return Err(format!(
                            "reachability search exceeds {} rule steps",
                            budget.max_enumeration
                        ));
                    }
                    if let Some(next) = step(rule, &state) {
                        let k = radix.pack(&next).ok_or("a rule leaves the domain")?;
                        if seen.insert(k) {
                            if seen.len() as u64 > budget.max_states {
                                return Err(too_many());
                            }
                            queue.push_back(k);
                        }
                    }
                }
            }
            let mut keys: Vec<u64> = seen.into_iter().collect();
            keys.sort_unstable();
            Ok(keys)
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical binary form (STAT and TRAN payloads)
// ---------------------------------------------------------------------------

const EFFECT_TRANSFORM: u8 = 0;
const EFFECT_OPACITY: u8 = 1;
const EFFECT_RESTYLE: u8 = 2;
const EFFECT_HIDE: u8 = 3;
const EFFECT_MORPH: u8 = 4;
const MODE_PRODUCT: u8 = 0;
const MODE_TRANSITIONS: u8 = 1;
const ANY: u16 = u16::MAX;

fn pad16(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Morph targets in first-use order, each stored once.
fn morph_targets(model: &StateModel) -> Vec<&Path> {
    let mut out: Vec<&Path> = Vec::new();
    for d in &model.dimensions {
        for v in d.values() {
            for e in &v.effects {
                if let Effect::Morph(_, target, _) = e {
                    if !out.contains(&&**target) {
                        out.push(target);
                    }
                }
            }
        }
    }
    out
}

/// `STAT`: morph-target path table, dimensions, value labels and effects.
/// The model must already be validated.
pub(crate) fn encode_stat(model: &StateModel) -> Vec<u8> {
    let targets = morph_targets(model);
    let mut out = Vec::new();
    out.extend_from_slice(&(model.dimensions.len() as u32).to_le_bytes());
    out.extend_from_slice(&(targets.len() as u32).to_le_bytes());
    out.push(STAT_LITERAL);
    out.extend_from_slice(&[0u8; 7]);
    for t in &targets {
        let bytes = encode_path_bytes(t);
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&bytes);
    }
    for d in &model.dimensions {
        put_str(&mut out, &d.name);
        let values = d.values();
        out.extend_from_slice(&(values.len() as u16).to_le_bytes());
        for v in values {
            put_str(&mut out, &v.label);
            out.extend_from_slice(&(v.effects.len() as u16).to_le_bytes());
            for e in &v.effects {
                let (kind, sel) = match e {
                    Effect::Transform(s, _) => (EFFECT_TRANSFORM, s),
                    Effect::Opacity(s, _) => (EFFECT_OPACITY, s),
                    Effect::Restyle(s, _) => (EFFECT_RESTYLE, s),
                    Effect::Hide(s) => (EFFECT_HIDE, s),
                    Effect::Morph(s, _, _) => (EFFECT_MORPH, s),
                };
                out.push(kind);
                match sel {
                    Selector::All => out.push(0),
                    Selector::Items(ids) => {
                        out.push(1);
                        out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
                        for id in ids {
                            out.extend_from_slice(&id.to_le_bytes());
                        }
                    }
                }
                match e {
                    Effect::Transform(_, t) => {
                        for v in [t.a, t.b, t.c, t.d, t.e, t.f] {
                            out.extend_from_slice(&v.to_le_bytes());
                        }
                    }
                    Effect::Opacity(_, f) => out.push(*f),
                    Effect::Restyle(_, s) => out.extend_from_slice(&encode_style_record(s)),
                    Effect::Hide(_) => {}
                    Effect::Morph(_, target, t) => {
                        let index = targets.iter().position(|p| **p == **target).unwrap_or(0);
                        out.extend_from_slice(&t.to_le_bytes());
                        out.extend_from_slice(&(index as u32).to_le_bytes());
                    }
                }
            }
        }
    }
    pad16(&mut out);
    out
}

/// `STAT` byte 8: the literal per-value encoding (the canonical form, and the
/// input to the parent binding).
pub(crate) const STAT_LITERAL: u8 = 0;
/// `STAT` byte 8: GVI-O operative encoding (gvi-operative.md §3).
pub(crate) const STAT_OPERATIVE: u8 = 1;
/// Most effect fields an operative `STAT` may materialise, against
/// expansion bombs.
const MAX_OPERATIVE_FIELDS: usize = 1 << 24;
/// Most label bytes an operative `STAT` may materialise.
const MAX_LABEL_BYTES: usize = 16 << 20;

fn effect_kind(e: &Effect) -> u8 {
    match e {
        Effect::Transform(..) => EFFECT_TRANSFORM,
        Effect::Opacity(..) => EFFECT_OPACITY,
        Effect::Restyle(..) => EFFECT_RESTYLE,
        Effect::Hide(_) => EFFECT_HIDE,
        Effect::Morph(..) => EFFECT_MORPH,
    }
}

fn effect_selector(e: &Effect) -> &Selector {
    match e {
        Effect::Transform(s, _)
        | Effect::Opacity(s, _)
        | Effect::Restyle(s, _)
        | Effect::Hide(s)
        | Effect::Morph(s, ..) => s,
    }
}

/// Number of payload integers each effect kind carries.
fn payload_len(kind: u8) -> usize {
    match kind {
        EFFECT_TRANSFORM => 6,
        EFFECT_OPACITY => 1,
        EFFECT_RESTYLE => STYLE_RECORD,
        EFFECT_MORPH => 2,
        _ => 0,
    }
}

/// An effect's shape: kind, selector kind (0 all, 1 items) and selector
/// length.
fn signature(e: &Effect) -> (u8, u8, usize) {
    match effect_selector(e) {
        Selector::All => (effect_kind(e), 0, 0),
        Selector::Items(ids) => (effect_kind(e), 1, ids.len()),
    }
}

/// The effect's selector ids, then its payload, as integers.
fn effect_fields(e: &Effect, targets: &[&Path]) -> Vec<i64> {
    let mut out: Vec<i64> = match effect_selector(e) {
        Selector::All => Vec::new(),
        Selector::Items(ids) => ids.iter().map(|&i| i64::from(i)).collect(),
    };
    match e {
        Effect::Transform(_, t) => out.extend([t.a, t.b, t.c, t.d, t.e, t.f].map(i64::from)),
        Effect::Opacity(_, f) => out.push(i64::from(*f)),
        Effect::Restyle(_, st) => out.extend(encode_style_record(st).map(i64::from)),
        Effect::Hide(_) => {}
        Effect::Morph(_, target, t) => {
            out.push(i64::from(*t));
            out.push(targets.iter().position(|p| **p == **target).unwrap_or(0) as i64);
        }
    }
    out
}

/// Rebuild an effect from its shape and fields.
fn effect_from(
    kind: u8,
    sel_kind: u8,
    fields: &[i64],
    targets: &[Arc<Path>],
) -> Result<Effect, GfxDiag> {
    let bad = |d: &str| invalid(format!("operative STAT: {d}"));
    let (sel, payload) = match sel_kind {
        0 => (Selector::All, fields),
        1 => {
            let n = fields.len() - payload_len(kind);
            let ids = fields[..n]
                .iter()
                .map(|&v| u32::try_from(v).map_err(|_| bad("selector id out of range")))
                .collect::<Result<Vec<_>, _>>()?;
            (Selector::Items(ids), &fields[n..])
        }
        _ => return Err(bad("unknown selector kind")),
    };
    let i32_at =
        |k: usize| i32::try_from(payload[k]).map_err(|_| bad("transform value out of range"));
    Ok(match kind {
        EFFECT_TRANSFORM => Effect::Transform(
            sel,
            Affine {
                a: i32_at(0)?,
                b: i32_at(1)?,
                c: i32_at(2)?,
                d: i32_at(3)?,
                e: i32_at(4)?,
                f: i32_at(5)?,
            },
        ),
        EFFECT_OPACITY => Effect::Opacity(
            sel,
            u8::try_from(payload[0]).map_err(|_| bad("opacity out of range"))?,
        ),
        EFFECT_RESTYLE => {
            let mut rec = [0u8; STYLE_RECORD];
            for (b, &v) in rec.iter_mut().zip(payload) {
                *b = u8::try_from(v).map_err(|_| bad("style byte out of range"))?;
            }
            Effect::Restyle(sel, decode_style_record(&rec)?)
        }
        EFFECT_HIDE => Effect::Hide(sel),
        EFFECT_MORPH => {
            let t = u32::try_from(payload[0]).map_err(|_| bad("morph fraction out of range"))?;
            let idx = usize::try_from(payload[1]).map_err(|_| bad("morph target out of range"))?;
            let target = targets
                .get(idx)
                .ok_or_else(|| bad("morph target is not in the path table"))?;
            Effect::Morph(sel, target.clone(), t)
        }
        _ => return Err(bad("unknown effect kind")),
    })
}

/// `STAT` in GVI-O operative form. The header and path table are as in the
/// literal form (with byte 8 = 1). Each dimension then follows as a varint
/// stream: name, value count, a label column, and either uniform effect
/// columns or literal per-value effects.
/// The `REF` operative for a morph target: the lowest-id `GEOM` anchor that
/// equals the target up to a translation, as `(anchor id, dx, dy)` with
/// `target = anchor.translated(dx, dy)`.
fn target_ref(target: &Path, anchors: &[Path]) -> Option<(u32, i32, i32)> {
    let t0 = target.first_point();
    anchors.iter().enumerate().find_map(|(g, a)| {
        let a0 = a.first_point();
        let dx = i32::try_from(i64::from(t0.x) - i64::from(a0.x)).ok()?;
        let dy = i32::try_from(i64::from(t0.y) - i64::from(a0.y)).ok()?;
        (a.translated(dx, dy).as_ref() == Some(target)).then_some((g as u32, dx, dy))
    })
}

/// `STAT` in GVI-O operative form. The header is as in the literal form,
/// with byte 8 = 1. Each morph target is then either a literal path
/// (`u8 0`, `u32 len`, bytes) or a `REF` to a `GEOM` anchor (`u8 1`,
/// varint id, zigzag dx, zigzag dy), using the lowest matching anchor id.
/// Each dimension then follows as a varint stream: name, value count, a
/// label column, and either uniform effect columns or literal per-value
/// effects.
pub(crate) fn encode_stat_operative(model: &StateModel, anchors: &[Path]) -> Vec<u8> {
    use crate::operative::{encode_ints, encode_labels, put_varint};
    let targets = morph_targets(model);
    let mut out = Vec::new();
    out.extend_from_slice(&(model.dimensions.len() as u32).to_le_bytes());
    out.extend_from_slice(&(targets.len() as u32).to_le_bytes());
    out.push(STAT_OPERATIVE);
    out.extend_from_slice(&[0u8; 7]);
    for t in &targets {
        match target_ref(t, anchors) {
            Some((g, dx, dy)) => {
                out.push(1);
                put_varint(&mut out, u64::from(g));
                put_varint(
                    &mut out,
                    ((i64::from(dx) << 1) ^ (i64::from(dx) >> 63)) as u64,
                );
                put_varint(
                    &mut out,
                    ((i64::from(dy) << 1) ^ (i64::from(dy) >> 63)) as u64,
                );
            }
            None => {
                let bytes = encode_path_bytes(t);
                out.push(0);
                out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                out.extend_from_slice(&bytes);
            }
        }
    }
    for d in &model.dimensions {
        put_varint(&mut out, d.name.len() as u64);
        out.extend_from_slice(d.name.as_bytes());
        let values = d.values();
        put_varint(&mut out, values.len() as u64);
        let labels: Vec<&str> = values.iter().map(|v| v.label.as_str()).collect();
        out.extend_from_slice(&encode_labels(&labels));
        let sigs: Vec<Vec<(u8, u8, usize)>> = values
            .iter()
            .map(|v| v.effects.iter().map(signature).collect())
            .collect();
        let uniform = sigs.windows(2).all(|w| w[0] == w[1]);
        out.push(u8::from(uniform));
        if uniform {
            let sig = sigs.first().cloned().unwrap_or_default();
            put_varint(&mut out, sig.len() as u64);
            for &(kind, sel_kind, sel_len) in &sig {
                out.push(kind);
                out.push(sel_kind);
                if sel_kind == 1 {
                    put_varint(&mut out, sel_len as u64);
                }
            }
            // One integer column per field: values down, fields across.
            let rows: Vec<Vec<i64>> = values
                .iter()
                .map(|v| {
                    v.effects
                        .iter()
                        .flat_map(|e| effect_fields(e, &targets))
                        .collect()
                })
                .collect();
            let width = rows.first().map_or(0, Vec::len);
            for f in 0..width {
                let col: Vec<i64> = rows.iter().map(|r| r[f]).collect();
                out.extend_from_slice(&encode_ints(&col));
            }
        } else {
            for v in values {
                put_varint(&mut out, v.effects.len() as u64);
                for e in &v.effects {
                    let (kind, sel_kind, sel_len) = signature(e);
                    out.push(kind);
                    out.push(sel_kind);
                    if sel_kind == 1 {
                        put_varint(&mut out, sel_len as u64);
                    }
                    for x in effect_fields(e, &targets) {
                        put_varint(&mut out, ((x << 1) ^ (x >> 63)) as u64);
                    }
                }
            }
        }
    }
    pad16(&mut out);
    out
}

fn parse_dims_operative(
    c: &mut crate::operative::Cursor<'_>,
    n: usize,
    targets: &[Arc<Path>],
) -> Result<Vec<Dimension>, GfxDiag> {
    use crate::operative::{decode_ints, decode_labels};
    let bad = |d: &str| invalid(format!("operative STAT: {d}"));
    let mut label_budget = MAX_LABEL_BYTES;
    let mut field_budget = MAX_OPERATIVE_FIELDS;
    let charge = |n: usize, budget: &mut usize| -> Result<(), GfxDiag> {
        *budget = budget
            .checked_sub(n)
            .ok_or_else(|| bad("exceeds the expansion budget"))?;
        Ok(())
    };
    let mut dims = Vec::with_capacity(n);
    for _ in 0..n {
        let name_len = c.varint()? as usize;
        if name_len > u16::MAX as usize {
            return Err(bad("dimension name is too long"));
        }
        let name = std::str::from_utf8(c.bytes(name_len)?)
            .map_err(|_| bad("name is not UTF-8"))?
            .to_string();
        let count = c.varint()? as usize;
        if count >= u16::MAX as usize {
            return Err(bad("too many values"));
        }
        let labels = decode_labels(c, count, &mut label_budget)?;
        let mut values: Vec<Value> = labels
            .into_iter()
            .map(|label| Value {
                label,
                effects: Vec::new(),
            })
            .collect();
        match c.u8()? {
            1 => {
                let ec = c.varint()? as usize;
                let mut sig = Vec::with_capacity(ec.min(c.buf.len()));
                let mut width = 0usize;
                for _ in 0..ec {
                    let kind = c.u8()?;
                    if kind > EFFECT_MORPH {
                        return Err(bad("unknown effect kind"));
                    }
                    let sel_kind = c.u8()?;
                    let sel_len = match sel_kind {
                        0 => 0,
                        1 => c.varint()? as usize,
                        _ => return Err(bad("unknown selector kind")),
                    };
                    let w = sel_len
                        .checked_add(payload_len(kind))
                        .ok_or_else(|| bad("width overflows"))?;
                    width = width.checked_add(w).ok_or_else(|| bad("width overflows"))?;
                    charge(w.saturating_mul(count), &mut field_budget)?;
                    sig.push((kind, sel_kind, w));
                }
                let cols: Vec<Vec<i64>> = (0..width)
                    .map(|_| decode_ints(c, count))
                    .collect::<Result<_, _>>()?;
                for (v, value) in values.iter_mut().enumerate() {
                    let mut f = 0;
                    for &(kind, sel_kind, w) in &sig {
                        let fields: Vec<i64> = (f..f + w).map(|x| cols[x][v]).collect();
                        value
                            .effects
                            .push(effect_from(kind, sel_kind, &fields, targets)?);
                        f += w;
                    }
                }
            }
            0 => {
                for value in values.iter_mut() {
                    let ec = c.varint()? as usize;
                    charge(ec, &mut field_budget)?;
                    for _ in 0..ec {
                        let kind = c.u8()?;
                        if kind > EFFECT_MORPH {
                            return Err(bad("unknown effect kind"));
                        }
                        let sel_kind = c.u8()?;
                        let sel_len = match sel_kind {
                            0 => 0,
                            1 => c.varint()? as usize,
                            _ => return Err(bad("unknown selector kind")),
                        };
                        let w = sel_len
                            .checked_add(payload_len(kind))
                            .ok_or_else(|| bad("width overflows"))?;
                        charge(w, &mut field_budget)?;
                        let mut fields = Vec::with_capacity(w);
                        for _ in 0..w {
                            let u = c.varint()?;
                            fields.push(((u >> 1) as i64) ^ -((u & 1) as i64));
                        }
                        value
                            .effects
                            .push(effect_from(kind, sel_kind, &fields, targets)?);
                    }
                }
            }
            _ => return Err(bad("unknown dimension layout")),
        }
        dims.push(Dimension {
            name,
            domain: Domain::Discrete(values),
        });
    }
    Ok(dims)
}

/// `TRAN`: reachability declaration.
pub(crate) fn encode_tran(model: &StateModel) -> Vec<u8> {
    let mut out = Vec::new();
    let pair = |out: &mut Vec<u8>, (d, v): (u16, u16)| {
        out.extend_from_slice(&d.to_le_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    };
    match &model.reachability {
        Reachability::Product { exclusions } => {
            out.extend_from_slice(&[MODE_PRODUCT, 0, 0, 0]);
            out.extend_from_slice(&(exclusions.len() as u32).to_le_bytes());
            for ex in exclusions {
                out.extend_from_slice(&(ex.len() as u16).to_le_bytes());
                for &p in ex {
                    pair(&mut out, p);
                }
            }
        }
        Reachability::Transitions { initial, rules } => {
            out.extend_from_slice(&[MODE_TRANSITIONS, 0, 0, 0]);
            out.extend_from_slice(&(rules.len() as u32).to_le_bytes());
            for v in initial {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for r in rules {
                out.extend_from_slice(&r.dim.to_le_bytes());
                out.extend_from_slice(&r.from.unwrap_or(ANY).to_le_bytes());
                out.extend_from_slice(&r.to.to_le_bytes());
                out.extend_from_slice(&(r.guard.len() as u16).to_le_bytes());
                for &p in &r.guard {
                    pair(&mut out, p);
                }
            }
        }
    }
    pad16(&mut out);
    out
}

/// A bounds-checked little-endian reader over untrusted bytes.
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8], what: &'static str) -> Reader<'a> {
        Reader { buf, pos: 0, what }
    }

    /// The current position, for [`Reader::since`].
    pub(crate) fn mark(&self) -> usize {
        self.pos
    }

    /// The bytes not yet consumed.
    pub(crate) fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }

    /// The bytes consumed since `mark`.
    pub(crate) fn since(&self, mark: usize) -> &'a [u8] {
        &self.buf[mark..self.pos]
    }

    fn err(&self, detail: &str) -> GfxDiag {
        GfxDiag::new(codes::MALFORMED_SECTION, format!("{}: {detail}", self.what))
    }

    pub(crate) fn bytes(&mut self, n: usize) -> Result<&'a [u8], GfxDiag> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.buf.len());
        let end = end.ok_or_else(|| self.err("truncated"))?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, GfxDiag> {
        Ok(self.bytes(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, GfxDiag> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, GfxDiag> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, GfxDiag> {
        Ok(self.u32()? as i32)
    }

    fn string(&mut self) -> Result<String, GfxDiag> {
        let n = self.u16()? as usize;
        let b = self.bytes(n)?;
        std::str::from_utf8(b)
            .map(str::to_string)
            .map_err(|_| self.err("string is not UTF-8"))
    }

    pub(crate) fn zeros(&mut self, n: usize) -> Result<(), GfxDiag> {
        if self.bytes(n)?.iter().any(|&b| b != 0) {
            return Err(self.err("reserved bytes are not zero"));
        }
        Ok(())
    }

    /// The rest must be zero padding that ends on a 16-byte boundary.
    pub(crate) fn finish(&mut self) -> Result<(), GfxDiag> {
        let rest = self.buf.len() - self.pos;
        if rest >= 16 || !self.buf.len().is_multiple_of(16) {
            return Err(self.err("unexpected trailing bytes"));
        }
        self.zeros(rest)
    }
}

/// Parse `STAT` into discrete dimensions. Lengths and counts are checked
/// against the bytes actually present before anything is allocated.
/// Parse `STAT` in either form. `anchors` are the decoded `GEOM` anchors,
/// which operative `REF` targets resolve against (empty for literal `STAT`).
pub(crate) fn parse_stat(
    buf: &[u8],
    max_dims: usize,
    gva_limits: &GvaLimits,
    anchors: &[Path],
) -> Result<Vec<Dimension>, GfxDiag> {
    let mut r = Reader::new(buf, "STAT");
    let n = r.u32()? as usize;
    let paths = r.u32()? as usize;
    let encoding = r.u8()?;
    r.zeros(7)?;
    if encoding > STAT_OPERATIVE {
        return Err(invalid(format!("unknown STAT encoding {encoding}")));
    }
    if n > max_dims {
        return Err(invalid(format!(
            "{n} dimensions exceeds the limit of {max_dims}"
        )));
    }
    let mut targets: Vec<Arc<Path>> = Vec::with_capacity(paths.min(buf.len() / 4));
    for _ in 0..paths {
        if encoding == STAT_OPERATIVE {
            match r.u8()? {
                0 => {}
                1 => {
                    let mut c = crate::operative::Cursor::new(&r.buf[r.pos..]);
                    let g = c.varint()?;
                    let mut signed = || -> Result<i32, GfxDiag> {
                        let u = c.varint()?;
                        i32::try_from(((u >> 1) as i64) ^ -((u & 1) as i64))
                            .map_err(|_| invalid("REF offset out of range"))
                    };
                    let (dx, dy) = (signed()?, signed()?);
                    r.pos += c.pos;
                    let anchor = usize::try_from(g)
                        .ok()
                        .and_then(|g| anchors.get(g))
                        .ok_or_else(|| invalid("REF names a missing anchor"))?;
                    let target = anchor
                        .translated(dx, dy)
                        .ok_or_else(|| invalid("REF translation overflows"))?;
                    targets.push(Arc::new(target));
                    continue;
                }
                _ => return Err(invalid("unknown path-table entry")),
            }
        }
        let len = r.u32()? as usize;
        targets.push(Arc::new(decode_path_bytes(r.bytes(len)?, gva_limits)?));
    }
    if encoding == STAT_OPERATIVE {
        let mut c = crate::operative::Cursor::new(&r.buf[r.pos..]);
        let dims = parse_dims_operative(&mut c, n, &targets)?;
        r.pos += c.pos;
        r.finish()?;
        return Ok(dims);
    }
    let mut dims = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.string()?;
        let vc = r.u16()? as usize;
        let mut values = Vec::with_capacity(vc.min(buf.len()));
        for _ in 0..vc {
            let label = r.string()?;
            let ec = r.u16()? as usize;
            let mut effects = Vec::with_capacity(ec.min(buf.len()));
            for _ in 0..ec {
                let kind = r.u8()?;
                let sel = match r.u8()? {
                    0 => Selector::All,
                    1 => {
                        let k = r.u32()? as usize;
                        let raw = r.bytes(
                            k.checked_mul(4)
                                .ok_or_else(|| invalid("selector overflows"))?,
                        )?;
                        Selector::Items(
                            raw.chunks_exact(4)
                                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                                .collect(),
                        )
                    }
                    other => return Err(invalid(format!("unknown selector kind {other}"))),
                };
                effects.push(match kind {
                    EFFECT_TRANSFORM => {
                        let mut v = [0i32; 6];
                        for x in &mut v {
                            *x = r.i32()?;
                        }
                        Effect::Transform(
                            sel,
                            Affine {
                                a: v[0],
                                b: v[1],
                                c: v[2],
                                d: v[3],
                                e: v[4],
                                f: v[5],
                            },
                        )
                    }
                    EFFECT_OPACITY => Effect::Opacity(sel, r.u8()?),
                    EFFECT_RESTYLE => {
                        let mut rec = [0u8; STYLE_RECORD];
                        rec.copy_from_slice(r.bytes(STYLE_RECORD)?);
                        Effect::Restyle(sel, decode_style_record(&rec)?)
                    }
                    EFFECT_HIDE => Effect::Hide(sel),
                    EFFECT_MORPH => {
                        let t = r.u32()?;
                        let index = r.u32()? as usize;
                        let target = targets.get(index).ok_or_else(|| {
                            invalid(format!("morph target {index} is not in the path table"))
                        })?;
                        Effect::Morph(sel, target.clone(), t)
                    }
                    other => return Err(invalid(format!("unknown effect kind {other}"))),
                });
            }
            values.push(Value { label, effects });
        }
        dims.push(Dimension {
            name,
            domain: Domain::Discrete(values),
        });
    }
    r.finish()?;
    Ok(dims)
}

/// Parse `TRAN` for a model with `dims` dimensions.
pub(crate) fn parse_tran(buf: &[u8], dims: usize) -> Result<Reachability, GfxDiag> {
    let mut r = Reader::new(buf, "TRAN");
    let mode = r.u8()?;
    r.zeros(3)?;
    let count = r.u32()? as usize;
    let pairs = |r: &mut Reader<'_>, n: usize| -> Result<Vec<(u16, u16)>, GfxDiag> {
        let raw = r.bytes(n * 4)?;
        Ok(raw
            .chunks_exact(4)
            .map(|c| {
                (
                    u16::from_le_bytes([c[0], c[1]]),
                    u16::from_le_bytes([c[2], c[3]]),
                )
            })
            .collect())
    };
    let reach = match mode {
        MODE_PRODUCT => {
            let mut exclusions = Vec::with_capacity(count.min(buf.len()));
            for _ in 0..count {
                let n = r.u16()? as usize;
                exclusions.push(pairs(&mut r, n)?);
            }
            Reachability::Product { exclusions }
        }
        MODE_TRANSITIONS => {
            let mut initial = Vec::with_capacity(dims);
            for _ in 0..dims {
                initial.push(r.u16()?);
            }
            let mut rules = Vec::with_capacity(count.min(buf.len()));
            for _ in 0..count {
                let dim = r.u16()?;
                let from = r.u16()?;
                let to = r.u16()?;
                let n = r.u16()? as usize;
                rules.push(Rule {
                    dim,
                    from: (from != ANY).then_some(from),
                    to,
                    guard: pairs(&mut r, n)?,
                });
            }
            Reachability::Transitions { initial, rules }
        }
        other => return Err(invalid(format!("unknown reachability mode {other}"))),
    };
    r.finish()?;
    Ok(reach)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_graphics_ir::{Fill, FillRule, Rgba8};

    fn colours() -> Dimension {
        let red = Style {
            fill: Some(Fill {
                colour: Rgba8::new(255, 0, 0, 255),
                rule: FillRule::NonZero,
            }),
            stroke: None,
        };
        Dimension::discrete(
            "status",
            vec![
                Value::new("idle"),
                Value::new("error").with(Effect::Restyle(Selector::Items(vec![1]), red)),
            ],
        )
    }

    fn sizes() -> Dimension {
        Dimension::discrete(
            "size",
            vec![
                Value::new("s"),
                Value::new("m").with(Effect::Transform(Selector::All, Affine::translate(5, 5))),
                Value::new("l")
                    .with(Effect::Opacity(Selector::All, 128))
                    .with(Effect::Hide(Selector::Items(vec![0]))),
            ],
        )
    }

    fn model(reach: Reachability) -> StateModel {
        StateModel {
            parent: "scene://test".into(),
            dimensions: vec![colours(), sizes()],
            reachability: reach,
        }
    }

    #[test]
    fn radix_round_trip() {
        let r = Radix::new(vec![2, 3, 4]).unwrap();
        assert_eq!(r.domain, 24);
        for k in 0..24 {
            assert_eq!(r.pack(&r.unpack(k)), Some(k));
        }
        assert_eq!(r.pack(&[1, 2, 3]), Some(23));
        assert_eq!(r.pack(&[2, 0, 0]), None);
        assert_eq!(r.pack(&[0, 0]), None);
        assert!(Radix::new(vec![u64::MAX, 2]).is_none());
    }

    #[test]
    fn product_with_exclusions() {
        let m = model(Reachability::Product {
            exclusions: vec![vec![(0, 1), (1, 2)]],
        });
        let (a, keys) = assess_and_enumerate(&m, &Budget::default());
        assert!(a.eligible(), "{a:?}");
        assert_eq!(a.domain_size, Some(6));
        assert_eq!(keys.unwrap(), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn transitions_reach_only_what_rules_allow() {
        // From (idle, s): size steps up one at a time while idle; error is
        // reachable only from s, and an errored icon never grows.
        let m = model(Reachability::Transitions {
            initial: vec![0, 0],
            rules: vec![
                Rule {
                    dim: 1,
                    from: Some(0),
                    to: 1,
                    guard: vec![(0, 0)],
                },
                Rule {
                    dim: 1,
                    from: Some(1),
                    to: 2,
                    guard: vec![(0, 0)],
                },
                Rule {
                    dim: 0,
                    from: None,
                    to: 1,
                    guard: vec![(1, 0)],
                },
            ],
        });
        let (a, keys) = assess_and_enumerate(&m, &Budget::default());
        assert!(a.eligible());
        // (idle,s)=0 (idle,m)=1 (idle,l)=2 (error,s)=3; nothing else.
        assert_eq!(keys.unwrap(), vec![0, 1, 2, 3]);
        assert_eq!(a.reachable, Some(4));
    }

    #[test]
    fn kfbm_failures() {
        let mut m = model(Reachability::Product { exclusions: vec![] });
        m.dimensions.push(Dimension {
            name: "brightness".into(),
            domain: Domain::Continuous { min: 0.0, max: 1.0 },
        });
        let (a, _) = assess_and_enumerate(&m, &Budget::default());
        assert!(a.known && a.bounded && !a.finite);
        assert_eq!(a.verdict().unwrap_err().code, codes::GVI_NOT_FINITE);
        assert!(a.verdict().unwrap_err().detail.contains("SPC"));

        m.dimensions[2].domain = Domain::Unbounded;
        let (a, _) = assess_and_enumerate(&m, &Budget::default());
        assert_eq!(a.verdict().unwrap_err().code, codes::GVI_NOT_BOUNDED);

        m.dimensions[2].domain = Domain::Opaque;
        let (a, _) = assess_and_enumerate(&m, &Budget::default());
        assert_eq!(a.verdict().unwrap_err().code, codes::GVI_NOT_KNOWN);

        let m = model(Reachability::Product { exclusions: vec![] });
        let tight = Budget {
            max_states: 5,
            ..Budget::default()
        };
        let (a, _) = assess_and_enumerate(&m, &tight);
        assert_eq!(a.manageable, Some(false));
        assert_eq!(a.verdict().unwrap_err().code, codes::GVI_NOT_MANAGEABLE);
    }

    #[test]
    fn validation() {
        let b = Budget::default();
        let ok = model(Reachability::Product { exclusions: vec![] });
        ok.validate(2, &b).unwrap();
        assert_eq!(
            ok.validate(1, &b).unwrap_err().code,
            codes::GVI_INVALID_MODEL
        );
        let mut dup = ok.clone();
        dup.dimensions[1].name = "status".into();
        assert!(dup.validate(2, &b).is_err());
        let bad_rule = model(Reachability::Transitions {
            initial: vec![0, 0],
            rules: vec![Rule {
                dim: 1,
                from: None,
                to: 9,
                guard: vec![],
            }],
        });
        assert!(bad_rule.validate(2, &b).is_err());
        let bad_init = model(Reachability::Transitions {
            initial: vec![0, 3],
            rules: vec![],
        });
        assert!(bad_init.validate(2, &b).is_err());
    }

    #[test]
    fn binary_model_round_trip() {
        for reach in [
            Reachability::Product {
                exclusions: vec![vec![(0, 1), (1, 2)]],
            },
            Reachability::Transitions {
                initial: vec![0, 1],
                rules: vec![Rule {
                    dim: 0,
                    from: None,
                    to: 1,
                    guard: vec![(1, 1)],
                }],
            },
        ] {
            let m = model(reach);
            let stat = encode_stat(&m);
            let tran = encode_tran(&m);
            assert_eq!(stat.len() % 16, 0);
            assert_eq!(
                parse_stat(&stat, 16, &GvaLimits::default(), &[]).unwrap(),
                m.dimensions
            );
            assert_eq!(parse_tran(&tran, 2).unwrap(), m.reachability);
            // Every truncation is rejected.
            for n in 0..stat.len() {
                assert!(
                    parse_stat(&stat[..n], 16, &GvaLimits::default(), &[]).is_err(),
                    "STAT cut at {n}"
                );
            }
            for n in 0..tran.len() {
                assert!(parse_tran(&tran[..n], 2).is_err(), "TRAN cut at {n}");
            }
        }
    }
}
