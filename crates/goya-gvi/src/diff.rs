//! Differential geometry (GVI-D, gvi.md §6).
//!
//! A derived geometry, one created by a morph or by baking, may be stored
//! as a **record** instead of a complete path: a deterministic *predictor*
//! built from earlier geometries, plus an exact integer *residual*.
//!
//! | Predictor | Prediction |
//! |---|---|
//! | `Copy(b)` | geometry `b` |
//! | `Extrapolate(a, b)` | `2·b − a` per point (exact for linear sequences such as morph frames) |
//! | `Affine(b, M)` | `b` baked under the linear part `M` (§5: round half-to-even, then translated to the origin) |
//!
//! Reconstruction is exact: `prediction + residual` must reproduce the
//! canonical geometry. Each record carries a 16-byte hash of the result to
//! prove it. Records reference only lower ids, so the graph is acyclic and
//! can be rebuilt in id order. The compiler bounds each record's **depth**
//! (anchor = 0) and its **ancestor count**, and declares both in `COST`.
//! This is the Bounded Differential Traversal contract: a lazy reader
//! rebuilds any geometry from at most `max_ancestors` others.

use crate::{map_points, morph_path, round_q16};
use goya_graphics_ir::{codes, Affine, GfxDiag, Path, Point, Segment};
use goya_gva::encode_path_bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const KIND_COPY: u8 = 0;
const KIND_EXTRAPOLATE: u8 = 1;
const KIND_AFFINE: u8 = 2;
/// GVI-A trajectory sample: `morph_path(a, b, t)` (gvi-albit.md §3).
const KIND_LERP: u8 = 3;
/// GVI-A transform-derived sample: `bake(morph_path(a, b, t), M)`.
const KIND_BAKED_LERP: u8 = 4;
const FLAG_RESIDUAL: u8 = 1;
/// With `FLAG_RESIDUAL`: every residual is −1, 0 or +1 and is packed at
/// 2 bits per coordinate (`00` = 0, `01` = +1, `10` = −1, `11` invalid),
/// four per byte, low bits first, with the final byte zero-padded.
const FLAG_PACKED: u8 = 2;
/// With `FLAG_RESIDUAL`: every residual is in −7..=7 and is packed as a
/// 4-bit two's-complement nibble (`1000` invalid), two per byte, low
/// nibble first, with the final nibble zero-padded.
const FLAG_NIBBLE: u8 = 4;
/// How many earlier same-structure geometries the encoder considers as
/// bases.
const WINDOW: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Predictor {
    Copy(u32),
    Extrapolate(u32, u32),
    Affine(u32, Affine),
    /// GVI-A: sample `t / 65536` of the trajectory from `a` to `b`.
    Lerp(u32, u32, u32),
    /// GVI-A: that sample baked under a linear transform.
    BakedLerp(u32, u32, u32, Affine),
}

impl Predictor {
    pub(crate) fn bases(&self) -> Vec<u32> {
        match *self {
            Predictor::Copy(b) | Predictor::Affine(b, _) => vec![b],
            Predictor::Extrapolate(a, b)
            | Predictor::Lerp(a, b, _)
            | Predictor::BakedLerp(a, b, ..) => {
                vec![a, b]
            }
        }
    }

    /// Whether this is a GVI-A trajectory record.
    pub(crate) fn is_trajectory(&self) -> bool {
        matches!(self, Predictor::Lerp(..) | Predictor::BakedLerp(..))
    }

    fn remap(&self, m: &[u32]) -> Predictor {
        match *self {
            Predictor::Copy(b) => Predictor::Copy(m[b as usize]),
            Predictor::Extrapolate(a, b) => Predictor::Extrapolate(m[a as usize], m[b as usize]),
            Predictor::Affine(b, t) => Predictor::Affine(m[b as usize], t),
            Predictor::Lerp(a, b, t) => Predictor::Lerp(m[a as usize], m[b as usize], t),
            Predictor::BakedLerp(a, b, t, x) => {
                Predictor::BakedLerp(m[a as usize], m[b as usize], t, x)
            }
        }
    }
}

fn err(detail: impl Into<String>) -> GfxDiag {
    GfxDiag::new(codes::GVI_CERTIFICATE_INVALID, detail)
}

pub(crate) fn path_hash(p: &Path) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&Sha256::digest(encode_path_bytes(p))[..16]);
    out
}

fn same_structure(a: &Path, b: &Path) -> bool {
    a.segments().len() == b.segments().len()
        && a.segments()
            .iter()
            .zip(b.segments())
            .all(|(x, y)| std::mem::discriminant(x) == std::mem::discriminant(y))
}

/// The verb sequence, as a key for grouping candidate bases.
fn structure_key(p: &Path) -> Vec<u8> {
    p.segments()
        .iter()
        .map(|s| match s {
            Segment::MoveTo(_) => 0,
            Segment::LineTo(_) => 1,
            Segment::QuadTo(..) => 2,
            Segment::CubicTo(..) => 3,
            Segment::Close => 4,
        })
        .collect()
}

/// The linear part of `t` applied to `path`, each point rounded half to
/// even, then translated so the first point is the origin. This is exactly
/// the geometry that baking produces.
pub(crate) fn bake_geometry(path: &Path, t: &Affine) -> Option<Path> {
    let image = map_points(path, |_, p| {
        let (x, y) = t.linear_scaled(p.x, p.y);
        Some(Point::new(
            i32::try_from(round_q16(x)).ok()?,
            i32::try_from(round_q16(y)).ok()?,
        ))
    })?;
    let o = image.first_point();
    image.translated(o.x.checked_neg()?, o.y.checked_neg()?)
}

/// The prediction, given access to already-built geometries.
pub(crate) fn predict<'p>(pred: &Predictor, get: &dyn Fn(u32) -> Option<&'p Path>) -> Option<Path> {
    match *pred {
        Predictor::Copy(b) => get(b).cloned(),
        Predictor::Extrapolate(a, b) => {
            let (a, b) = (get(a)?, get(b)?);
            if !same_structure(a, b) {
                return None;
            }
            let ap: Vec<Point> = a.points().collect();
            map_points(b, |i, q| {
                let x = 2 * i64::from(q.x) - i64::from(ap[i].x);
                let y = 2 * i64::from(q.y) - i64::from(ap[i].y);
                Some(Point::new(i32::try_from(x).ok()?, i32::try_from(y).ok()?))
            })
        }
        Predictor::Affine(b, t) => bake_geometry(get(b)?, &t),
        Predictor::Lerp(a, b, t) => morph_path(get(a)?, get(b)?, t),
        Predictor::BakedLerp(a, b, t, m) => bake_geometry(&morph_path(get(a)?, get(b)?, t)?, &m),
    }
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn get_varint(buf: &[u8], pos: &mut usize) -> Result<u64, GfxDiag> {
    let mut v = 0u64;
    let mut shift = 0u32;
    loop {
        let b = *buf
            .get(*pos)
            .ok_or_else(|| err("record ends inside a varint"))?;
        *pos += 1;
        if shift == 63 && b > 1 {
            return Err(err("varint overflows"));
        }
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            if b == 0 && shift > 0 {
                return Err(GfxDiag::new(
                    codes::NON_MINIMAL_VARINT,
                    "record varint is not minimal",
                ));
            }
            return Ok(v);
        }
        shift += 7;
        if shift > 63 {
            return Err(err("varint is too long"));
        }
    }
}

/// `actual − predicted` per coordinate, or `None` if structures differ.
fn residual(actual: &Path, predicted: &Path) -> Option<Vec<i64>> {
    if !same_structure(actual, predicted) {
        return None;
    }
    Some(
        actual
            .points()
            .zip(predicted.points())
            .flat_map(|(a, p)| {
                [
                    i64::from(a.x) - i64::from(p.x),
                    i64::from(a.y) - i64::from(p.y),
                ]
            })
            .collect(),
    )
}

/// Record bytes: `u8 kind, u8 flags, varint bases…, [6 zigzag varints for
/// Affine], hash[16], [residual]`. The residual is either packed 2-bit
/// codes (`FLAG_PACKED`) or one zigzag varint per coordinate.
pub(crate) fn encode_record(pred: &Predictor, hash: &[u8; 16], res: &[i64]) -> Vec<u8> {
    let mut out = Vec::new();
    let nonzero = res.iter().any(|&r| r != 0);
    out.push(match pred {
        Predictor::Copy(_) => KIND_COPY,
        Predictor::Extrapolate(..) => KIND_EXTRAPOLATE,
        Predictor::Affine(..) => KIND_AFFINE,
        Predictor::Lerp(..) => KIND_LERP,
        Predictor::BakedLerp(..) => KIND_BAKED_LERP,
    });
    let packed = nonzero && res.iter().all(|r| (-1..=1).contains(r));
    let nibble = nonzero && !packed && res.iter().all(|r| (-7..=7).contains(r));
    out.push(match (nonzero, packed, nibble) {
        (false, ..) => 0,
        (true, true, _) => FLAG_RESIDUAL | FLAG_PACKED,
        (true, false, true) => FLAG_RESIDUAL | FLAG_NIBBLE,
        (true, false, false) => FLAG_RESIDUAL,
    });
    for b in pred.bases() {
        put_varint(&mut out, u64::from(b));
    }
    if let Predictor::Lerp(_, _, t) | Predictor::BakedLerp(_, _, t, _) = pred {
        put_varint(&mut out, u64::from(*t));
    }
    if let Predictor::Affine(_, t) | Predictor::BakedLerp(_, _, _, t) = pred {
        for v in [t.a, t.b, t.c, t.d, t.e, t.f] {
            put_varint(&mut out, zigzag(i64::from(v)));
        }
    }
    out.extend_from_slice(hash);
    if packed {
        for chunk in res.chunks(4) {
            let mut byte = 0u8;
            for (k, &r) in chunk.iter().enumerate() {
                let code = match r {
                    1 => 1,
                    -1 => 2,
                    _ => 0,
                };
                byte |= code << (2 * k);
            }
            out.push(byte);
        }
    } else if nibble {
        for pair in res.chunks(2) {
            let lo = (pair[0] as u8) & 0xf;
            let hi = pair.get(1).map_or(0, |&r| (r as u8) & 0xf);
            out.push(lo | (hi << 4));
        }
    } else if nonzero {
        for &r in res {
            put_varint(&mut out, zigzag(r));
        }
    }
    out
}

/// A parsed record whose residual is still encoded.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RecordView<'a> {
    pub pred: Predictor,
    pub hash: [u8; 16],
    pub residual: Option<&'a [u8]>,
    pub packed: bool,
    pub nibble: bool,
}

pub(crate) fn parse_record(buf: &[u8], own_id: u32) -> Result<RecordView<'_>, GfxDiag> {
    if buf.len() < 2 {
        return Err(err("record is truncated"));
    }
    let (kind, flags) = (buf[0], buf[1]);
    let packing = flags & (FLAG_PACKED | FLAG_NIBBLE);
    if flags & !(FLAG_RESIDUAL | FLAG_PACKED | FLAG_NIBBLE) != 0
        || (packing != 0 && flags & FLAG_RESIDUAL == 0)
        || packing == FLAG_PACKED | FLAG_NIBBLE
    {
        return Err(err("unknown record flags"));
    }
    let mut pos = 2;
    let base = |pos: &mut usize| -> Result<u32, GfxDiag> {
        let b = get_varint(buf, pos)?;
        if b >= u64::from(own_id) {
            return Err(err(format!(
                "record {own_id} references geometry {b}, which is not earlier"
            )));
        }
        Ok(b as u32)
    };
    let pred = match kind {
        KIND_COPY => Predictor::Copy(base(&mut pos)?),
        KIND_EXTRAPOLATE => {
            let a = base(&mut pos)?;
            Predictor::Extrapolate(a, base(&mut pos)?)
        }
        KIND_AFFINE => {
            let b = base(&mut pos)?;
            let mut v = [0i32; 6];
            for x in &mut v {
                *x = i32::try_from(unzigzag(get_varint(buf, &mut pos)?))
                    .map_err(|_| err("affine overflows"))?;
            }
            Predictor::Affine(
                b,
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
        KIND_LERP | KIND_BAKED_LERP => {
            let a = base(&mut pos)?;
            let b = base(&mut pos)?;
            let t = get_varint(buf, &mut pos)?;
            if t > 65_536 {
                return Err(err(format!("trajectory sample {t} is outside [0, 65536]")));
            }
            let t = t as u32;
            if kind == KIND_LERP {
                Predictor::Lerp(a, b, t)
            } else {
                let mut v = [0i32; 6];
                for x in &mut v {
                    *x = i32::try_from(unzigzag(get_varint(buf, &mut pos)?))
                        .map_err(|_| err("affine overflows"))?;
                }
                Predictor::BakedLerp(
                    a,
                    b,
                    t,
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
        }
        other => return Err(err(format!("unknown record kind {other}"))),
    };
    let hash: [u8; 16] = buf
        .get(pos..pos + 16)
        .and_then(|h| h.try_into().ok())
        .ok_or_else(|| err("record hash is truncated"))?;
    pos += 16;
    let residual = if flags & FLAG_RESIDUAL != 0 {
        Some(&buf[pos..])
    } else if pos != buf.len() {
        return Err(err("trailing bytes after a residual-free record"));
    } else {
        None
    };
    Ok(RecordView {
        pred,
        hash,
        residual,
        packed: flags & FLAG_PACKED != 0,
        nibble: flags & FLAG_NIBBLE != 0,
    })
}

/// Rebuild one geometry from its record and its (already rebuilt) bases.
pub(crate) fn reconstruct<'p>(
    rec: &RecordView<'_>,
    get: &dyn Fn(u32) -> Option<&'p Path>,
) -> Result<Path, GfxDiag> {
    let pred =
        predict(&rec.pred, get).ok_or_else(|| err("record predictor cannot be evaluated"))?;
    let out = match rec.residual {
        None => pred,
        Some(stream) => {
            let n = 2 * pred.points().count();
            let res: Vec<i64> = if rec.packed {
                if stream.len() != n.div_ceil(4) {
                    return Err(err("packed residual has the wrong length"));
                }
                let mut out = Vec::with_capacity(n);
                for (k, &byte) in stream.iter().enumerate() {
                    for slot in 0..4 {
                        let code = (byte >> (2 * slot)) & 3;
                        if 4 * k + slot >= n {
                            if code != 0 {
                                return Err(err("packed residual padding is not zero"));
                            }
                            continue;
                        }
                        out.push(match code {
                            0 => 0,
                            1 => 1,
                            2 => -1,
                            _ => return Err(err("invalid packed residual code")),
                        });
                    }
                }
                out
            } else if rec.nibble {
                if stream.len() != n.div_ceil(2) {
                    return Err(err("nibble residual has the wrong length"));
                }
                let mut out = Vec::with_capacity(n);
                for (k, &byte) in stream.iter().enumerate() {
                    for (slot, nib) in [byte & 0xf, byte >> 4].into_iter().enumerate() {
                        if 2 * k + slot >= n {
                            if nib != 0 {
                                return Err(err("nibble residual padding is not zero"));
                            }
                            continue;
                        }
                        if nib == 8 {
                            return Err(err("invalid nibble residual code"));
                        }
                        out.push(i64::from(((nib << 4) as i8) >> 4));
                    }
                }
                out
            } else {
                let mut pos = 0;
                let mut out = Vec::with_capacity(n);
                while pos < stream.len() && out.len() < n {
                    out.push(unzigzag(get_varint(stream, &mut pos)?));
                }
                if out.len() != n || pos != stream.len() {
                    return Err(err("residual length does not match the geometry"));
                }
                out
            };
            let p = map_points(&pred, |i, q| {
                let x = i32::try_from(i64::from(q.x) + res[2 * i]).ok()?;
                let y = i32::try_from(i64::from(q.y) + res[2 * i + 1]).ok()?;
                Some(Point::new(x, y))
            })
            .ok_or_else(|| err("residual leaves the coordinate range"))?;
            if !same_structure(&p, &pred) {
                return Err(err("residual changes the path structure"));
            }
            p
        }
    };
    if path_hash(&out) != rec.hash {
        return Err(err(
            "reconstructed geometry does not match its recorded hash",
        ));
    }
    Ok(out)
}

/// The encoder's decision for a pool.
pub(crate) struct Plan {
    /// New id → old id.
    pub order: Vec<u32>,
    /// Old id → new id.
    pub remap: Vec<u32>,
    pub anchors: usize,
    /// Records for new ids `anchors..`, already remapped.
    pub records: Vec<Vec<u8>>,
    pub max_depth: u32,
    pub max_ancestors: u32,
    pub points: u64,
}

/// A geometry's known trajectory origin (GVI-A, compiler provenance):
/// `morph_path(from, to, t)`, optionally baked under `bake`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Origin {
    pub from: u32,
    pub to: u32,
    pub t: u32,
    pub bake: Option<Affine>,
}

/// Choose, for each derived geometry (old ids `fixed..`), the cheapest
/// exact representation whose depth stays within `max_depth`.
/// `provenance` gives a known bake origin `(base old id, linear part)`.
/// The plan is deterministic.
pub(crate) fn plan(
    pool: &[Path],
    fixed: usize,
    provenance: &HashMap<u32, (u32, Affine)>,
    origins: &HashMap<u32, Origin>,
    search: bool,
    max_depth: u32,
) -> Plan {
    let n = pool.len();
    let mut depth = vec![0u32; n];
    let mut choice: Vec<Option<(Predictor, Vec<u8>)>> = vec![None; n];
    let mut recent: HashMap<Vec<u8>, Vec<u32>> = HashMap::new();
    for (i, p) in pool.iter().enumerate() {
        let key = structure_key(p);
        let cands: Vec<u32> = recent.get(&key).cloned().unwrap_or_default();
        if i >= fixed && max_depth > 0 {
            let complete = encode_path_bytes(p).len();
            let mut best: Option<(usize, Predictor, Vec<u8>, u32)> = None;
            let mut consider = |pred: Predictor| {
                if pred.bases().iter().any(|&b| b as usize >= i) {
                    return;
                }
                let d = 1 + pred
                    .bases()
                    .iter()
                    .map(|&b| depth[b as usize])
                    .max()
                    .unwrap_or(0);
                if d > max_depth {
                    return;
                }
                let get = |id: u32| pool.get(id as usize);
                let Some(predicted) = predict(&pred, &get) else {
                    return;
                };
                let Some(res) = residual(p, &predicted) else {
                    return;
                };
                let bytes = encode_record(&pred, &path_hash(p), &res);
                if best.as_ref().is_none_or(|b| bytes.len() < b.0) {
                    best = Some((bytes.len(), pred, bytes, d));
                }
            };
            // GVI-A: the trajectory candidate from provenance is O(V) to
            // try. With `search` off it replaces the predictor search for
            // geometries that have one.
            let origin = origins.get(&(i as u32));
            if let Some(o) = origin {
                consider(match o.bake {
                    None => Predictor::Lerp(o.from, o.to, o.t),
                    Some(m) => Predictor::BakedLerp(o.from, o.to, o.t, m),
                });
            }
            if let Some(&(b, t)) = provenance.get(&(i as u32)) {
                consider(Predictor::Affine(b, t));
            }
            let skip_search = !search && origin.is_some();
            for (k, &b) in cands.iter().enumerate().rev().filter(|_| !skip_search) {
                consider(Predictor::Copy(b));
                for &a in cands[..k].iter().rev() {
                    consider(Predictor::Extrapolate(a, b));
                }
            }
            // Adaptive anchors: a record costing more than half a complete
            // path is stored complete instead. It then resets depth for
            // the geometries that follow it.
            if let Some((len, pred, bytes, d)) = best {
                if 2 * len <= complete {
                    depth[i] = d;
                    choice[i] = Some((pred, bytes));
                }
            }
        }
        let list = recent.entry(key).or_default();
        list.push(i as u32);
        if list.len() > WINDOW {
            list.remove(0);
        }
    }

    let mut order: Vec<u32> = (0..n as u32)
        .filter(|&i| choice[i as usize].is_none())
        .collect();
    let anchors = order.len();
    order.extend((0..n as u32).filter(|&i| choice[i as usize].is_some()));
    let mut remap = vec![0u32; n];
    for (new, &old) in order.iter().enumerate() {
        remap[old as usize] = new as u32;
    }
    let mut records = Vec::new();
    let mut points = 0u64;
    let mut ancestors: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut max_ancestors = 0u32;
    for &old in &order[anchors..] {
        let (pred, _) = choice[old as usize]
            .clone()
            .unwrap_or_else(|| unreachable!());
        let pred = pred.remap(&remap);
        let p = &pool[old as usize];
        let get = |id: u32| pool.get(order[id as usize] as usize);
        let predicted = predict(&pred, &get).unwrap_or_else(|| unreachable!());
        let res = residual(p, &predicted).unwrap_or_else(|| unreachable!());
        records.push(encode_record(&pred, &path_hash(p), &res));
        points += p.points().count() as u64;
        let new = remap[old as usize] as usize;
        let mut anc: Vec<u32> = Vec::new();
        for b in pred.bases() {
            anc.push(b);
            anc.extend_from_slice(&ancestors[b as usize]);
        }
        anc.sort_unstable();
        anc.dedup();
        max_ancestors = max_ancestors.max(anc.len() as u32);
        ancestors[new] = anc;
    }
    Plan {
        order,
        remap,
        anchors,
        records,
        max_depth: depth.iter().copied().max().unwrap_or(0),
        max_ancestors,
        points,
    }
}

// ---------------------------------------------------------------------------
// GVI-O columnar operative DIFF (gvi-operative.md §4)
// ---------------------------------------------------------------------------

/// `DIFF` byte 4: record-per-entry form with an offset table.
pub(crate) const DIFF_RECORDS: u8 = 0;
/// `DIFF` byte 4: GVI-O columnar operative form.
pub(crate) const DIFF_OPERATIVE: u8 = 1;
const COLUMNS: usize = 12;

impl RecordView<'_> {
    fn kind(&self) -> u8 {
        match self.pred {
            Predictor::Copy(_) => KIND_COPY,
            Predictor::Extrapolate(..) => KIND_EXTRAPOLATE,
            Predictor::Affine(..) => KIND_AFFINE,
            Predictor::Lerp(..) => KIND_LERP,
            Predictor::BakedLerp(..) => KIND_BAKED_LERP,
        }
    }

    fn flags(&self) -> u8 {
        let mut f = 0;
        if self.residual.is_some() {
            f |= FLAG_RESIDUAL;
        }
        if self.packed {
            f |= FLAG_PACKED;
        }
        if self.nibble {
            f |= FLAG_NIBBLE;
        }
        f
    }

    /// Column values: kind, flags, base0, base1, t, the six affine fields
    /// and the residual length. Fields a kind does not use are 0.
    fn columns(&self) -> [i64; COLUMNS] {
        let bases = self.pred.bases();
        let t = match self.pred {
            Predictor::Lerp(_, _, t) | Predictor::BakedLerp(_, _, t, _) => i64::from(t),
            _ => 0,
        };
        let m = match self.pred {
            Predictor::Affine(_, m) | Predictor::BakedLerp(_, _, _, m) => {
                [m.a, m.b, m.c, m.d, m.e, m.f]
            }
            _ => [0; 6],
        };
        [
            i64::from(self.kind()),
            i64::from(self.flags()),
            i64::from(bases[0]),
            bases.get(1).map_or(0, |&b| i64::from(b)),
            t,
            i64::from(m[0]),
            i64::from(m[1]),
            i64::from(m[2]),
            i64::from(m[3]),
            i64::from(m[4]),
            i64::from(m[5]),
            self.residual.map_or(0, |r| r.len() as i64),
        ]
    }
}

/// Rebuild a record from its columns, with the same checks as
/// [`parse_record`]: known kind and flags, earlier bases, `t ≤ 65536`, and
/// unused fields exactly zero.
fn record_from_columns<'a>(
    c: &[i64; COLUMNS],
    hash: [u8; 16],
    residual: Option<&'a [u8]>,
    own_id: u32,
) -> Result<RecordView<'a>, GfxDiag> {
    let kind = u8::try_from(c[0]).map_err(|_| err("record kind out of range"))?;
    let flags = u8::try_from(c[1]).map_err(|_| err("record flags out of range"))?;
    let packing = flags & (FLAG_PACKED | FLAG_NIBBLE);
    if flags & !(FLAG_RESIDUAL | FLAG_PACKED | FLAG_NIBBLE) != 0
        || (packing != 0 && flags & FLAG_RESIDUAL == 0)
        || packing == FLAG_PACKED | FLAG_NIBBLE
    {
        return Err(err("unknown record flags"));
    }
    if (flags & FLAG_RESIDUAL != 0) != residual.is_some() {
        return Err(err("residual length disagrees with the residual flag"));
    }
    let base = |v: i64| -> Result<u32, GfxDiag> {
        match u32::try_from(v) {
            Ok(b) if b < own_id => Ok(b),
            _ => Err(err(format!(
                "record {own_id} references geometry {v}, which is not earlier"
            ))),
        }
    };
    let affine = || -> Result<Affine, GfxDiag> {
        let f = |k: usize| i32::try_from(c[5 + k]).map_err(|_| err("affine overflows"));
        Ok(Affine {
            a: f(0)?,
            b: f(1)?,
            c: f(2)?,
            d: f(3)?,
            e: f(4)?,
            f: f(5)?,
        })
    };
    let t = || -> Result<u32, GfxDiag> {
        match u32::try_from(c[4]) {
            Ok(t) if t <= 65_536 => Ok(t),
            _ => Err(err(format!(
                "trajectory sample {} is outside [0, 65536]",
                c[4]
            ))),
        }
    };
    let pred = match kind {
        KIND_COPY => Predictor::Copy(base(c[2])?),
        KIND_EXTRAPOLATE => Predictor::Extrapolate(base(c[2])?, base(c[3])?),
        KIND_AFFINE => Predictor::Affine(base(c[2])?, affine()?),
        KIND_LERP => Predictor::Lerp(base(c[2])?, base(c[3])?, t()?),
        KIND_BAKED_LERP => Predictor::BakedLerp(base(c[2])?, base(c[3])?, t()?, affine()?),
        other => return Err(err(format!("unknown record kind {other}"))),
    };
    let rec = RecordView {
        pred,
        hash,
        residual,
        packed: flags & FLAG_PACKED != 0,
        nibble: flags & FLAG_NIBBLE != 0,
    };
    // Unused fields must be zero, so each record has one encoding.
    if rec.columns() != *c {
        return Err(err("a field the record kind does not use is not zero"));
    }
    Ok(rec)
}

/// `DIFF` in columnar operative form: `u32 n`, `u8 encoding = 1`, 11 zero
/// bytes, then twelve operative integer columns, `n × hash[16]`, the
/// concatenated residuals, and zero padding to 16 bytes.
pub(crate) fn encode_diff_operative(records: &[RecordView<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(records.len() as u32).to_le_bytes());
    out.push(DIFF_OPERATIVE);
    out.extend_from_slice(&[0u8; 11]);
    let rows: Vec<[i64; COLUMNS]> = records.iter().map(RecordView::columns).collect();
    for col in 0..COLUMNS {
        let values: Vec<i64> = rows.iter().map(|r| r[col]).collect();
        out.extend_from_slice(&crate::operative::encode_ints(&values));
    }
    for r in records {
        out.extend_from_slice(&r.hash);
    }
    for r in records {
        if let Some(res) = r.residual {
            out.extend_from_slice(res);
        }
    }
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
    out
}

/// Parse a columnar operative `DIFF` whose records are geometry ids
/// `anchors..anchors + n`. The caller checks that it re-encodes exactly.
pub(crate) fn parse_diff_operative(
    buf: &[u8],
    anchors: usize,
    max_records: usize,
) -> Result<Vec<RecordView<'_>>, GfxDiag> {
    if buf.len() < 16 || buf[4] != DIFF_OPERATIVE || buf[5..16].iter().any(|&b| b != 0) {
        return Err(err("bad operative DIFF header"));
    }
    let n = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if n > max_records {
        return Err(err("too many records"));
    }
    let mut c = crate::operative::Cursor::new(&buf[16..]);
    let cols: Vec<Vec<i64>> = (0..COLUMNS)
        .map(|_| crate::operative::decode_ints(&mut c, n))
        .collect::<Result<_, _>>()?;
    let hashes = c.bytes(
        n.checked_mul(16)
            .ok_or_else(|| err("hash block overflows"))?,
    )?;
    let mut residual_at = 16 + c.pos;
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let row: [i64; COLUMNS] = std::array::from_fn(|col| cols[col][k]);
        let len = usize::try_from(row[COLUMNS - 1]).map_err(|_| err("negative residual length"))?;
        let residual = if len > 0 {
            let end = residual_at
                .checked_add(len)
                .filter(|&e| e <= buf.len())
                .ok_or_else(|| err("residual stream truncated"))?;
            let r = &buf[residual_at..end];
            residual_at = end;
            Some(r)
        } else if row[1] & i64::from(FLAG_RESIDUAL) != 0 {
            // A residual flag with an empty stream is still a residual.
            Some(&buf[residual_at..residual_at])
        } else {
            None
        };
        let mut hash = [0u8; 16];
        hash.copy_from_slice(&hashes[k * 16..k * 16 + 16]);
        out.push(record_from_columns(
            &row,
            hash,
            residual,
            (anchors + k) as u32,
        )?);
    }
    Ok(out)
}
