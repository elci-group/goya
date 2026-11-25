//! The live, externally scheduled state of a unit (directive §6.5).
//!
//! When the parent changes a slot:
//!
//! 1. The value is checked against the slot's type and domain.
//! 2. Only that slot's bindings are evaluated.
//! 3. Only the instances listed in `CHNG` are touched. Their geometry is
//!    never rebuilt: a geometry change only switches an `Arc` to another
//!    path that was decoded at load.
//! 4. The affected instances, the channels and a dirty rectangle accumulate
//!    in an [`Invalidation`] for the parent to schedule.
//!
//! An update is atomic: if any affected transform would overflow, nothing
//! changes.

use crate::{
    channel, check_value, codes, evaluate, round_q16, Binding, GfxDiag, Point, Rect, Region, Rgba8,
    SlotValue, Style, Target, Unit, Value,
};
use goya_graphics_ir::{Affine, Drawing, FillRule, Item, Segment, FIXED_ONE};

#[derive(Debug, Clone)]
struct Live {
    geometry: u32,
    opacity: u8,
    fill: Option<Rgba8>,
    stroke: Option<Rgba8>,
    tx: i32,
    ty: i32,
    scale: i32,
    pivot: Point,
    visible: bool,
    transform: Affine,
    style: Style,
}

/// What changed since the last [`UnitState::take_invalidation`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Invalidation {
    /// Affected instances, ascending.
    pub instances: Vec<u32>,
    /// Union of the changed channels (see [`crate::channel`]).
    pub channels: u32,
    /// Document-space rectangle covering every affected instance before
    /// and after the change, or `None` if nothing visible changed.
    pub region: Option<Rect>,
}

impl Invalidation {
    /// True when only paint changed: no transform and no geometry.
    pub fn modulation_only(&self) -> bool {
        self.channels & !channel::MODULATION == 0
    }
}

/// A unit's live state. Create it with [`Unit::instantiate`].
#[derive(Debug, Clone)]
pub struct UnitState<'u> {
    unit: &'u Unit,
    values: Vec<SlotValue>,
    live: Vec<Live>,
    dirty: Vec<bool>,
    pending: Invalidation,
    evaluations: u64,
}

fn apply(live: &mut Live, target: &Target, value: &Value) {
    match (target, value) {
        (Target::Opacity(_), Value::Number(x)) => live.opacity = *x as u8,
        (Target::FillColour(_), Value::Colour(c)) => live.fill = Some(*c),
        (Target::StrokeColour(_), Value::Colour(c)) => live.stroke = Some(*c),
        (Target::TranslateX(_), Value::Number(x)) => live.tx = *x,
        (Target::TranslateY(_), Value::Number(x)) => live.ty = *x,
        (Target::Scale { pivot, .. }, Value::Number(x)) => {
            live.scale = *x;
            live.pivot = *pivot;
        }
        (Target::Visible(_), Value::Bool(b)) => live.visible = *b,
        (Target::Geometry(_), Value::Geometry(g)) => live.geometry = *g,
        _ => {}
    }
}

fn overflow() -> GfxDiag {
    GfxDiag::new(
        codes::COORDINATE_OVERFLOW,
        "resolved transform overflows; the update was not applied",
    )
}

/// Recompute the cached transform and style from the channels.
fn resolve(live: &mut Live, base: &Affine, base_style: &Style) -> Result<(), GfxDiag> {
    let local = if live.scale == FIXED_ONE {
        Affine::IDENTITY
    } else {
        let s = i128::from(live.scale);
        let e = i128::from(live.pivot.x) - round_q16(s * i128::from(live.pivot.x));
        let f = i128::from(live.pivot.y) - round_q16(s * i128::from(live.pivot.y));
        Affine {
            a: live.scale,
            b: 0,
            c: 0,
            d: live.scale,
            e: i32::try_from(e).map_err(|_| overflow())?,
            f: i32::try_from(f).map_err(|_| overflow())?,
        }
    };
    live.transform = local
        .then(base)
        .and_then(|t| t.then(&Affine::translate(live.tx, live.ty)))
        .ok_or_else(overflow)?;
    let mut style = *base_style;
    if let (Some(f), Some(c)) = (&mut style.fill, live.fill) {
        f.colour = c;
    }
    if let (Some(s), Some(c)) = (&mut style.stroke, live.stroke) {
        s.colour = c;
    }
    live.style = style;
    Ok(())
}

fn union(a: Option<Rect>, b: Rect) -> Rect {
    match a {
        None => b,
        Some(a) => Rect {
            min_x: a.min_x.min(b.min_x),
            min_y: a.min_y.min(b.min_y),
            max_x: a.max_x.max(b.max_x),
            max_y: a.max_y.max(b.max_y),
        },
    }
}

/// Conservative document bounds of `local` under `t`, with stroke reach.
fn doc_bounds(local: &Rect, t: &Affine, style: &Style) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (i128::MAX, i128::MAX, i128::MIN, i128::MIN);
    for (x, y) in [
        (local.min_x, local.min_y),
        (local.max_x, local.min_y),
        (local.min_x, local.max_y),
        (local.max_x, local.max_y),
    ] {
        let (sx, sy) = t.apply_scaled(Point::new(x, y));
        x0 = x0.min(sx.div_euclid(65536));
        y0 = y0.min(sy.div_euclid(65536));
        x1 = x1.max(-(-sx).div_euclid(65536));
        y1 = y1.max(-(-sy).div_euclid(65536));
    }
    if let Some(s) = style.stroke {
        let ext =
            (f64::from(s.width) / 2.0 * std::f64::consts::SQRT_2 * t.max_scale()).ceil() as i128;
        x0 -= ext;
        y0 -= ext;
        x1 += ext;
        y1 += ext;
    }
    let c = |v: i128| v.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32;
    Rect {
        min_x: c(x0),
        min_y: c(y0),
        max_x: c(x1),
        max_y: c(y1),
    }
}

impl<'u> UnitState<'u> {
    pub(crate) fn new(unit: &'u Unit) -> Result<UnitState<'u>, GfxDiag> {
        let mut live: Vec<Live> = (0..unit.instance_count())
            .map(|i| {
                let b = unit.base(i).copied().unwrap_or_else(|| unreachable!());
                let style = unit.styles()[b.style as usize];
                Live {
                    geometry: b.geometry,
                    opacity: b.opacity,
                    fill: style.fill.map(|f| f.colour),
                    stroke: style.stroke.map(|s| s.colour),
                    tx: 0,
                    ty: 0,
                    scale: FIXED_ONE,
                    pivot: Point::ORIGIN,
                    visible: true,
                    transform: b.transform,
                    style,
                }
            })
            .collect();
        let values: Vec<SlotValue> = unit.slots().iter().map(|s| s.default).collect();
        for b in unit.bindings() {
            let slot = &unit.slots()[usize::from(b.slot)];
            let v = evaluate(&b.map, slot, &values[usize::from(b.slot)]);
            apply(&mut live[b.target.instance() as usize], &b.target, &v);
        }
        for (i, l) in live.iter_mut().enumerate() {
            let b = unit.base(i).copied().unwrap_or_else(|| unreachable!());
            resolve(l, &b.transform, &unit.styles()[b.style as usize])?;
        }
        Ok(UnitState {
            unit,
            values,
            dirty: vec![false; live.len()],
            live,
            pending: Invalidation::default(),
            evaluations: unit.bindings().len() as u64,
        })
    }

    pub fn unit(&self) -> &'u Unit {
        self.unit
    }

    pub fn get(&self, slot: u16) -> Option<SlotValue> {
        self.values.get(usize::from(slot)).copied()
    }

    /// Total binding evaluations since instantiation.
    pub fn evaluations(&self) -> u64 {
        self.evaluations
    }

    /// Set a slot. The value is validated against the slot's type and
    /// domain (E2010, E2011). Only that slot's bindings run, and only the
    /// instances `CHNG` lists for it are touched. The update is atomic.
    pub fn set(&mut self, slot: u16, value: SlotValue) -> Result<(), GfxDiag> {
        let unit = self.unit;
        let s = unit
            .slots()
            .get(usize::from(slot))
            .ok_or_else(|| GfxDiag::new(codes::UNKNOWN_SLOT, format!("no slot {slot}")))?;
        check_value(s, &value)?;
        if self.values[usize::from(slot)] == value {
            return Ok(());
        }
        let (mask, affected) = &unit.invalidation_table()[usize::from(slot)];
        let mut staged: Vec<Live> = affected
            .iter()
            .map(|&i| self.live[i as usize].clone())
            .collect();
        let bindings: &[Binding] = unit.bindings();
        for &k in &unit.by_slot[usize::from(slot)] {
            let b = &bindings[k];
            let v = evaluate(&b.map, s, &value);
            let at = affected
                .binary_search(&b.target.instance())
                .unwrap_or_else(|_| unreachable!());
            apply(&mut staged[at], &b.target, &v);
        }
        for (l, &i) in staged.iter_mut().zip(affected) {
            let b = unit
                .base(i as usize)
                .copied()
                .unwrap_or_else(|| unreachable!());
            resolve(l, &b.transform, &unit.styles()[b.style as usize])?;
        }
        // Commit.
        for (l, &i) in staged.into_iter().zip(affected) {
            let local = unit
                .local_bounds(i as usize)
                .unwrap_or_else(|| unreachable!());
            let old = &self.live[i as usize];
            if old.visible {
                self.pending.region = Some(union(
                    self.pending.region,
                    doc_bounds(&local, &old.transform, &old.style),
                ));
            }
            if l.visible {
                self.pending.region = Some(union(
                    self.pending.region,
                    doc_bounds(&local, &l.transform, &l.style),
                ));
            }
            self.live[i as usize] = l;
            if !self.dirty[i as usize] {
                self.dirty[i as usize] = true;
                self.pending.instances.push(i);
            }
        }
        self.pending.channels |= mask;
        self.values[usize::from(slot)] = value;
        self.evaluations += unit.by_slot[usize::from(slot)].len() as u64;
        Ok(())
    }

    /// Set a slot by name.
    pub fn set_named(&mut self, name: &str, value: SlotValue) -> Result<(), GfxDiag> {
        let slot = self
            .unit
            .slot_index(name)
            .ok_or_else(|| GfxDiag::new(codes::UNKNOWN_SLOT, format!("no slot {name:?}")))?;
        self.set(slot, value)
    }

    /// Hand the accumulated invalidation to the parent's scheduler.
    pub fn take_invalidation(&mut self) -> Invalidation {
        let mut out = std::mem::take(&mut self.pending);
        out.instances.sort_unstable();
        for &i in &out.instances {
            self.dirty[i as usize] = false;
        }
        out
    }

    /// Instance `i` as presented now, or `None` when hidden.
    pub fn item(&self, i: usize) -> Option<Item> {
        let l = self.live.get(i)?;
        l.visible.then(|| Item {
            path: self.unit.geometry()[l.geometry as usize].clone(),
            style: l.style,
            transform: l.transform,
            opacity: l.opacity,
        })
    }

    /// The whole unit as a drawing.
    pub fn drawing(&self) -> Drawing {
        Drawing {
            width: self.unit.width,
            height: self.unit.height,
            items: (0..self.live.len()).filter_map(|i| self.item(i)).collect(),
        }
    }

    /// Current document bounds of instance `i`, or `None` when hidden.
    pub fn instance_bounds(&self, i: usize) -> Option<Rect> {
        let l = self.live.get(i)?;
        l.visible.then(|| {
            doc_bounds(
                &self.unit.local_bounds(i).unwrap_or_else(|| unreachable!()),
                &l.transform,
                &l.style,
            )
        })
    }

    /// The topmost interaction region under `p` (document millipoints),
    /// computed from geometry, not pixels (§6.6). Precise regions test
    /// the filled shape under the region's fill rule, with curves
    /// subdivided into 16 chords. Other regions test bounds.
    pub fn hit_test(&self, p: Point) -> Option<&'u Region> {
        let mut order: Vec<&Region> = self.unit.regions().iter().collect();
        order.sort_by_key(|r| std::cmp::Reverse(r.instance));
        order.into_iter().find(|r| {
            let i = r.instance as usize;
            let Some(b) = self.instance_bounds(i) else {
                return false;
            };
            if p.x < b.min_x || p.x > b.max_x || p.y < b.min_y || p.y > b.max_y {
                return false;
            }
            !r.precise || self.inside_fill(i, p)
        })
    }

    fn inside_fill(&self, i: usize, p: Point) -> bool {
        let l = &self.live[i];
        let Some(fill) = l.style.fill else {
            return false;
        };
        let path = &self.unit.geometry()[l.geometry as usize];
        let map = |q: Point| {
            let (x, y) = l.transform.apply_scaled(q);
            (x as f64 / 65536.0, y as f64 / 65536.0)
        };
        let mut polys: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut cur = (0.0, 0.0);
        for seg in path.segments() {
            match *seg {
                Segment::MoveTo(q) => {
                    cur = map(q);
                    polys.push(vec![cur]);
                }
                Segment::LineTo(q) => {
                    cur = map(q);
                    if let Some(poly) = polys.last_mut() {
                        poly.push(cur);
                    }
                }
                Segment::QuadTo(c, q) => {
                    let (p0, p1, p2) = (cur, map(c), map(q));
                    if let Some(poly) = polys.last_mut() {
                        for k in 1..=16 {
                            let t = f64::from(k) / 16.0;
                            let m = 1.0 - t;
                            poly.push((
                                m * m * p0.0 + 2.0 * m * t * p1.0 + t * t * p2.0,
                                m * m * p0.1 + 2.0 * m * t * p1.1 + t * t * p2.1,
                            ));
                        }
                    }
                    cur = p2;
                }
                Segment::CubicTo(c1, c2, q) => {
                    let (p0, p1, p2, p3) = (cur, map(c1), map(c2), map(q));
                    if let Some(poly) = polys.last_mut() {
                        for k in 1..=16 {
                            let t = f64::from(k) / 16.0;
                            let m = 1.0 - t;
                            let (w0, w1, w2, w3) =
                                (m * m * m, 3.0 * m * m * t, 3.0 * m * t * t, t * t * t);
                            poly.push((
                                w0 * p0.0 + w1 * p1.0 + w2 * p2.0 + w3 * p3.0,
                                w0 * p0.1 + w1 * p1.1 + w2 * p2.1 + w3 * p3.1,
                            ));
                        }
                    }
                    cur = p3;
                }
                Segment::Close => {}
            }
        }
        let (px, py) = (f64::from(p.x), f64::from(p.y));
        let mut winding = 0i32;
        for poly in &polys {
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                if (a.1 <= py) != (b.1 <= py) {
                    let x = a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1);
                    if x > px {
                        winding += if b.1 > a.1 { 1 } else { -1 };
                    }
                }
            }
        }
        match fill.rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        }
    }

    /// Visible regions in focus order: the traversal for remote controls
    /// and assistive technology.
    pub fn focus_order(&self) -> Vec<&'u Region> {
        let mut out: Vec<&Region> = self
            .unit
            .regions()
            .iter()
            .filter(|r| self.live[r.instance as usize].visible)
            .collect();
        out.sort_by_key(|r| r.focus);
        out
    }

    /// The next visible region after `id` in focus order, wrapping around.
    pub fn focus_next(&self, id: &str) -> Option<&'u Region> {
        let order = self.focus_order();
        let at = order.iter().position(|r| r.id == id)?;
        order.get((at + 1) % order.len()).copied()
    }
}
