//! goya-gva — Graphical Vector Array (`.gva`), contract `GY-GVA/0` (draft).
//! Normative text: `docs/graphics/gva.md`.
//!
//! GVA turns a GIR [`Drawing`] into a deterministic, deduplicated,
//! structure-of-arrays binary inside the ratified GY-GFXC/1 container:
//!
//! | Section | Contents |
//! |---|---|
//! | `HEAD` | Logical canvas size and coordinate unit |
//! | `GEOM` | Deduplicated geometry pool: offset table plus delta/varint-packed paths |
//! | `STYL` | Deduplicated style pool: 16-byte records |
//! | `INST` | One array per field: geometry ref, style ref, e, f, a, b, c, d |
//! | `MODS` | Modulation arrays (opacity) |
//! | `BBOX` | Local bounds per geometry and document bounds per instance |
//! | `HASH` | Optional structural identity per geometry (truncated SHA-256) |
//!
//! Canonicalisation (directive §7.5, steps 1–5):
//! - Geometry is translated so its first point is the origin whenever the
//!   offset folds exactly into the instance translation. Copies of a shape
//!   drawn at different places then share one pool entry.
//! - Pools are ordered by first use in drawing order.
//! - Coordinates are delta-encoded and zigzag/LEB128-packed.
//!
//! Entropy coding (step 6) is not applied yet: the container defines only
//! the stored codec.
//!
//! [`Gva::open`] verifies the container and the structure of every section,
//! but decodes geometry lazily. [`Gva::verify`] decodes everything and
//! checks that the file is in canonical form.

use goya_graphics_container::{
    encode as encode_container, Container, Expect, Format, Limits, Profile, SectionInput, Tag,
};
use goya_graphics_ir::{
    codes, Affine, Drawing, Fill, FillRule, GfxDiag, Item, LineCap, LineJoin, Path, Point, Rect,
    Rgba8, Segment, Stroke, Style,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

/// Contract identifier implemented by this crate.
pub const GVA_CONTRACT: &str = "GY-GVA/0";

pub const BBOX: Tag = Tag::new(b"BBOX");
pub const GEOM: Tag = Tag::new(b"GEOM");
pub const HASH: Tag = Tag::new(b"HASH");
pub const HEAD: Tag = Tag::new(b"HEAD");
pub const INST: Tag = Tag::new(b"INST");
pub const MODS: Tag = Tag::new(b"MODS");
pub const STYL: Tag = Tag::new(b"STYL");
const KNOWN: [Tag; 7] = [BBOX, GEOM, HASH, HEAD, INST, MODS, STYL];

/// Every GVA section is 16-byte aligned, and so is every array inside one.
const ALIGN_LOG2: u8 = 4;
const SUB_HEADER: usize = 16;
/// Bytes per style record.
pub const STYLE_RECORD: usize = 16;
const RECT_RECORD: usize = 16;
const HASH_RECORD: usize = 16;
/// `HEAD.unit` value meaning "millipoints of a point" (Baseline).
pub const UNIT_MILLIPOINT: u16 = 0;
/// `MODS.channels` bit for the opacity channel (the only one in v0).
pub const CHANNEL_OPACITY: u32 = 1;
/// `HASH.algorithm` value: the first 16 bytes of SHA-256 of the encoded path.
pub const HASH_SHA256_128: u32 = 1;

const VERB_MOVE: u8 = 0;
const VERB_LINE: u8 = 1;
const VERB_QUAD: u8 = 2;
const VERB_CUBIC: u8 = 3;
const VERB_CLOSE: u8 = 4;

/// Limits on GVA content, on top of the container's own limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GvaLimits {
    pub container: Limits,
    pub max_instances: u32,
    pub max_geometries: u32,
    pub max_styles: u32,
    pub max_segments_per_path: u32,
    /// Total segments across all *instances* when expanding to a drawing.
    pub max_total_segments: u64,
}

impl Default for GvaLimits {
    fn default() -> GvaLimits {
        GvaLimits {
            container: Limits::default(),
            max_instances: 1 << 20,
            max_geometries: 1 << 18,
            max_styles: 1 << 16,
            max_segments_per_path: 1 << 20,
            max_total_segments: 1 << 24,
        }
    }
}

fn diag(code: u16, detail: impl Into<String>) -> GfxDiag {
    GfxDiag::new(code, detail)
}

fn section_err(tag: Tag, detail: impl std::fmt::Display) -> GfxDiag {
    diag(codes::MALFORMED_SECTION, format!("{tag}: {detail}"))
}

fn from_container(e: goya_graphics_container::GfxError) -> GfxDiag {
    GfxDiag::new(e.kind.code(), e.to_string())
}

fn pad16(n: usize) -> Option<usize> {
    n.checked_add(15).map(|v| v & !15)
}

// ---------------------------------------------------------------------------
// Path codec
// ---------------------------------------------------------------------------

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

fn get_varint(buf: &[u8], pos: &mut usize) -> Result<u64, GfxDiag> {
    let mut v = 0u64;
    let mut shift = 0u32;
    loop {
        let Some(&b) = buf.get(*pos) else {
            return Err(diag(
                codes::MALFORMED_GEOMETRY,
                "path data ends inside a varint",
            ));
        };
        *pos += 1;
        if shift == 63 && b > 1 {
            return Err(diag(codes::MALFORMED_GEOMETRY, "varint overflows 64 bits"));
        }
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            if b == 0 && shift > 0 {
                return Err(diag(
                    codes::NON_MINIMAL_VARINT,
                    "varint has a redundant trailing byte",
                ));
            }
            return Ok(v);
        }
        shift += 7;
        if shift > 63 {
            return Err(diag(codes::MALFORMED_GEOMETRY, "varint is too long"));
        }
    }
}

/// `varint(n) ‖ n verb bytes ‖ zigzag-varint deltas of every point`.
/// Deltas run across the whole path, starting from the origin.
fn encode_path(path: &Path) -> Vec<u8> {
    let segs = path.segments();
    let mut out = Vec::with_capacity(1 + segs.len() * 5);
    put_varint(&mut out, segs.len() as u64);
    for s in segs {
        out.push(match s {
            Segment::MoveTo(_) => VERB_MOVE,
            Segment::LineTo(_) => VERB_LINE,
            Segment::QuadTo(..) => VERB_QUAD,
            Segment::CubicTo(..) => VERB_CUBIC,
            Segment::Close => VERB_CLOSE,
        });
    }
    let mut prev = Point::ORIGIN;
    for p in path.points() {
        put_varint(&mut out, zigzag(i64::from(p.x) - i64::from(prev.x)));
        put_varint(&mut out, zigzag(i64::from(p.y) - i64::from(prev.y)));
        prev = p;
    }
    out
}

fn decode_path(buf: &[u8], limits: &GvaLimits) -> Result<Path, GfxDiag> {
    let mut pos = 0;
    let n = get_varint(buf, &mut pos)?;
    let remaining = (buf.len() - pos) as u64;
    if n == 0 || n > remaining {
        return Err(diag(
            codes::MALFORMED_GEOMETRY,
            format!("segment count {n} does not fit the path data"),
        ));
    }
    if n > u64::from(limits.max_segments_per_path) {
        return Err(diag(
            codes::TOO_MANY_SEGMENTS,
            format!(
                "{n} segments exceeds the per-path limit of {}",
                limits.max_segments_per_path
            ),
        ));
    }
    let n = n as usize; // n <= buf.len()
    let verbs = &buf[pos..pos + n];
    pos += n;
    let mut prev = Point::ORIGIN;
    let mut next_point = |pos: &mut usize| -> Result<Point, GfxDiag> {
        let mut coord = |prev: i32| -> Result<i32, GfxDiag> {
            let delta = unzigzag(get_varint(buf, pos)?);
            i64::from(prev)
                .checked_add(delta)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| {
                    diag(
                        codes::COORDINATE_OVERFLOW,
                        "decoded coordinate leaves the i32 range",
                    )
                })
        };
        let x = coord(prev.x)?;
        let y = coord(prev.y)?;
        prev = Point::new(x, y);
        Ok(prev)
    };
    let mut segs = Vec::with_capacity(n);
    for &v in verbs {
        segs.push(match v {
            VERB_MOVE => Segment::MoveTo(next_point(&mut pos)?),
            VERB_LINE => Segment::LineTo(next_point(&mut pos)?),
            VERB_QUAD => {
                let c = next_point(&mut pos)?;
                Segment::QuadTo(c, next_point(&mut pos)?)
            }
            VERB_CUBIC => {
                let c1 = next_point(&mut pos)?;
                let c2 = next_point(&mut pos)?;
                Segment::CubicTo(c1, c2, next_point(&mut pos)?)
            }
            VERB_CLOSE => Segment::Close,
            other => {
                return Err(diag(
                    codes::MALFORMED_GEOMETRY,
                    format!("unknown verb {other}"),
                ))
            }
        });
    }
    if pos != buf.len() {
        return Err(diag(
            codes::MALFORMED_GEOMETRY,
            "trailing bytes after path data",
        ));
    }
    let path = Path::new(segs.clone())?;
    if path.segments() != segs.as_slice() {
        return Err(diag(
            codes::NON_CANONICAL,
            "stored path is not in canonical form",
        ));
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Style codec
// ---------------------------------------------------------------------------

fn encode_style(s: &Style, out: &mut Vec<u8>) {
    let mut rec = [0u8; STYLE_RECORD];
    if let Some(f) = s.fill {
        rec[0] |= 1;
        rec[1] = match f.rule {
            FillRule::NonZero => 0,
            FillRule::EvenOdd => 1,
        };
        rec[4..8].copy_from_slice(&[f.colour.r, f.colour.g, f.colour.b, f.colour.a]);
    }
    if let Some(k) = s.stroke {
        rec[0] |= 2;
        rec[2] = match k.cap {
            LineCap::Butt => 0,
            LineCap::Round => 1,
            LineCap::Square => 2,
        };
        rec[3] = match k.join {
            LineJoin::Round => 0,
        };
        rec[8..12].copy_from_slice(&[k.colour.r, k.colour.g, k.colour.b, k.colour.a]);
        rec[12..16].copy_from_slice(&k.width.to_le_bytes());
    }
    out.extend_from_slice(&rec);
}

fn decode_style(rec: &[u8]) -> Result<Style, GfxDiag> {
    let bad = |what: &str| diag(codes::INVALID_STYLE, what.to_string());
    let flags = rec[0];
    if flags & !3 != 0 {
        return Err(bad("unknown style flag bits"));
    }
    let has_fill = flags & 1 != 0;
    let has_stroke = flags & 2 != 0;
    let fill = if has_fill {
        let rule = match rec[1] {
            0 => FillRule::NonZero,
            1 => FillRule::EvenOdd,
            _ => return Err(bad("unknown fill rule")),
        };
        Some(Fill {
            colour: Rgba8::new(rec[4], rec[5], rec[6], rec[7]),
            rule,
        })
    } else {
        if rec[1] != 0 || rec[4..8] != [0; 4] {
            return Err(bad("fill fields must be zero without a fill"));
        }
        None
    };
    let stroke = if has_stroke {
        let cap = match rec[2] {
            0 => LineCap::Butt,
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => return Err(bad("unknown line cap")),
        };
        let join = match rec[3] {
            0 => LineJoin::Round,
            _ => return Err(bad("unknown line join")),
        };
        let width = u32::from_le_bytes([rec[12], rec[13], rec[14], rec[15]]);
        Some(Stroke {
            colour: Rgba8::new(rec[8], rec[9], rec[10], rec[11]),
            width,
            cap,
            join,
        })
    } else {
        if rec[2] != 0 || rec[3] != 0 || rec[8..16] != [0; 8] {
            return Err(bad("stroke fields must be zero without a stroke"));
        }
        None
    };
    let style = Style { fill, stroke };
    style.validate()?;
    Ok(style)
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Fold the path's first point into the translation when that is exact, so
/// equal shapes at different places share geometry.
fn normalise(item: &Item) -> (Path, Affine) {
    let o = item.path.first_point();
    if o == Point::ORIGIN {
        return ((*item.path).clone(), item.transform);
    }
    let t = item.transform;
    let (dx, dy) = t.linear_scaled(o.x, o.y);
    let fold = || -> Option<(Path, Affine)> {
        if dx % 65536 != 0 || dy % 65536 != 0 {
            return None;
        }
        let e = i32::try_from(i128::from(t.e) + (dx >> 16)).ok()?;
        let f = i32::try_from(i128::from(t.f) + (dy >> 16)).ok()?;
        let path = item
            .path
            .translated(o.x.checked_neg()?, o.y.checked_neg()?)?;
        Some((path, Affine { e, f, ..t }))
    };
    fold().unwrap_or_else(|| ((*item.path).clone(), t))
}

fn floor_div(v: i128) -> i128 {
    v.div_euclid(65536)
}

fn ceil_div(v: i128) -> i128 {
    -(-v).div_euclid(65536)
}

/// Conservative document-space bounds of an instance, stroke included.
fn instance_bounds(local: &Rect, t: &Affine, style: &Style) -> Result<Rect, GfxDiag> {
    let corners = [
        (local.min_x, local.min_y),
        (local.max_x, local.min_y),
        (local.min_x, local.max_y),
        (local.max_x, local.max_y),
    ];
    let (mut x0, mut y0, mut x1, mut y1) = (i128::MAX, i128::MAX, i128::MIN, i128::MIN);
    for (x, y) in corners {
        let (sx, sy) = t.apply_scaled(Point::new(x, y));
        x0 = x0.min(floor_div(sx));
        y0 = y0.min(floor_div(sy));
        x1 = x1.max(ceil_div(sx));
        y1 = y1.max(ceil_div(sy));
    }
    if let Some(s) = style.stroke {
        // Square caps reach hw·√2 past an end point; joins and round caps
        // reach hw.
        let ext =
            (f64::from(s.width) / 2.0 * std::f64::consts::SQRT_2 * t.max_scale()).ceil() as i128;
        x0 -= ext;
        y0 -= ext;
        x1 += ext;
        y1 += ext;
    }
    let fit = |v: i128| {
        i32::try_from(v).map_err(|_| {
            diag(
                codes::COORDINATE_OVERFLOW,
                "instance bounds leave the i32 range",
            )
        })
    };
    Ok(Rect {
        min_x: fit(x0)?,
        min_y: fit(y0)?,
        max_x: fit(x1)?,
        max_y: fit(y1)?,
    })
}

fn put_rect(out: &mut Vec<u8>, r: &Rect) {
    for v in [r.min_x, r.min_y, r.max_x, r.max_y] {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn sub_header(a: u32, b: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(SUB_HEADER);
    h.extend_from_slice(&a.to_le_bytes());
    h.extend_from_slice(&b.to_le_bytes());
    h.extend_from_slice(&[0u8; 8]);
    h
}

fn pad_to_16(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
}

/// Encoder-side instance row: geometry ref, style ref, transform, opacity.
type InstanceRow = (u32, u32, Affine, u8);

/// Field `k` of an instance row, in `INST` array order.
fn instance_field(r: &InstanceRow, k: usize) -> [u8; 4] {
    let (g, s, t, _) = *r;
    match k {
        0 => g.to_le_bytes(),
        1 => s.to_le_bytes(),
        2 => t.e.to_le_bytes(),
        3 => t.f.to_le_bytes(),
        4 => t.a.to_le_bytes(),
        5 => t.b.to_le_bytes(),
        6 => t.c.to_le_bytes(),
        _ => t.d.to_le_bytes(),
    }
}

/// Encode a drawing. Equal drawings give byte-identical output, and
/// encoding a decoded file reproduces it exactly.
pub fn encode(drawing: &Drawing, limits: &GvaLimits) -> Result<Vec<u8>, GfxDiag> {
    let count = drawing.items.len();
    if count > limits.max_instances as usize {
        return Err(diag(
            codes::TOO_MANY_INSTANCES,
            format!(
                "{count} instances exceeds the limit of {}",
                limits.max_instances
            ),
        ));
    }

    let mut geoms: Vec<Path> = Vec::new();
    let mut geom_index: HashMap<Path, u32> = HashMap::new();
    let mut styles: Vec<Style> = Vec::new();
    let mut style_index: HashMap<Style, u32> = HashMap::new();
    let mut refs: Vec<InstanceRow> = Vec::with_capacity(count);
    for item in &drawing.items {
        item.style.validate()?;
        let (path, t) = normalise(item);
        let g = match geom_index.get(&path) {
            Some(&g) => g,
            None => {
                let g = geoms.len() as u32;
                if g >= limits.max_geometries {
                    return Err(diag(
                        codes::TOO_MANY_GEOMETRIES,
                        "too many distinct geometries",
                    ));
                }
                geom_index.insert(path.clone(), g);
                geoms.push(path);
                g
            }
        };
        let s = match style_index.get(&item.style) {
            Some(&s) => s,
            None => {
                let s = styles.len() as u32;
                if s >= limits.max_styles {
                    return Err(diag(codes::TOO_MANY_STYLES, "too many distinct styles"));
                }
                style_index.insert(item.style, s);
                styles.push(item.style);
                s
            }
        };
        refs.push((g, s, t, item.opacity));
    }
    let n = count as u32;

    // HEAD
    let mut head = Vec::with_capacity(16);
    head.extend_from_slice(&drawing.width.to_le_bytes());
    head.extend_from_slice(&drawing.height.to_le_bytes());
    head.extend_from_slice(&UNIT_MILLIPOINT.to_le_bytes());
    head.extend_from_slice(&[0u8; 6]);

    let (geom, hash) = encode_geometry_pool(&geoms)?;
    let styl = encode_style_pool(&styles);

    // INST: one array per field.
    let mut inst = sub_header(n, 0);
    for field in 0..8 {
        for r in &refs {
            inst.extend_from_slice(&instance_field(r, field));
        }
        pad_to_16(&mut inst);
    }

    // MODS
    let mut mods = sub_header(n, CHANNEL_OPACITY);
    mods.extend(refs.iter().map(|r| r.3));
    pad_to_16(&mut mods);

    // BBOX
    let local: Vec<Rect> = geoms.iter().map(Path::bounds).collect();
    let mut bbox = sub_header(geoms.len() as u32, n);
    for r in &local {
        put_rect(&mut bbox, r);
    }
    for (g, s, t, _) in &refs {
        put_rect(
            &mut bbox,
            &instance_bounds(&local[*g as usize], t, &styles[*s as usize])?,
        );
    }

    let sections = [
        SectionInput::new(HEAD, &head).align_log2(ALIGN_LOG2),
        SectionInput::new(GEOM, &geom).align_log2(ALIGN_LOG2),
        SectionInput::new(STYL, &styl).align_log2(ALIGN_LOG2),
        SectionInput::new(INST, &inst).align_log2(ALIGN_LOG2),
        SectionInput::new(MODS, &mods).align_log2(ALIGN_LOG2),
        SectionInput::new(BBOX, &bbox).align_log2(ALIGN_LOG2),
        SectionInput::new(HASH, &hash)
            .align_log2(ALIGN_LOG2)
            .optional(),
    ];
    encode_container(
        Format::Gva,
        Profile::Baseline,
        0,
        &sections,
        &limits.container,
    )
    .map_err(from_container)
}

// ---------------------------------------------------------------------------
// Shared pool codecs (also used by GVI, directive §10.4)
// ---------------------------------------------------------------------------

/// Encode a geometry pool as its `GEOM` and `HASH` section payloads.
pub fn encode_geometry_pool(paths: &[Path]) -> Result<(Vec<u8>, Vec<u8>), GfxDiag> {
    let encoded: Vec<Vec<u8>> = paths.iter().map(encode_path).collect();
    let mut geom = sub_header(paths.len() as u32, 0);
    let mut offset = 0u64;
    geom.extend_from_slice(&0u32.to_le_bytes());
    for e in &encoded {
        offset += e.len() as u64;
        let off = u32::try_from(offset)
            .map_err(|_| diag(codes::MALFORMED_SECTION, "geometry pool exceeds 4 GiB"))?;
        geom.extend_from_slice(&off.to_le_bytes());
    }
    pad_to_16(&mut geom);
    let mut hash = sub_header(paths.len() as u32, HASH_SHA256_128);
    for e in &encoded {
        geom.extend_from_slice(e);
        hash.extend_from_slice(&Sha256::digest(e)[..HASH_RECORD]);
    }
    pad_to_16(&mut geom);
    Ok((geom, hash))
}

/// Encode a style pool as a `STYL` section payload.
pub fn encode_style_pool(styles: &[Style]) -> Vec<u8> {
    let mut styl = sub_header(styles.len() as u32, 0);
    for s in styles {
        encode_style(s, &mut styl);
    }
    styl
}

/// One style as its 16-byte record (gva.md §4).
pub fn encode_style_record(style: &Style) -> [u8; STYLE_RECORD] {
    let mut v = Vec::with_capacity(STYLE_RECORD);
    encode_style(style, &mut v);
    let mut out = [0u8; STYLE_RECORD];
    out.copy_from_slice(&v);
    out
}

/// Decode and validate one 16-byte style record.
pub fn decode_style_record(rec: &[u8; STYLE_RECORD]) -> Result<Style, GfxDiag> {
    decode_style(rec)
}

/// Parse and validate a `STYL` payload.
pub fn parse_style_pool(styl: &[u8], limits: &GvaLimits) -> Result<Vec<Style>, GfxDiag> {
    let (sc, reserved) = parse_sub_header(STYL, styl)?;
    check_padding(STYL, &reserved.to_le_bytes())?;
    if sc > limits.max_styles {
        return Err(diag(
            codes::TOO_MANY_STYLES,
            format!("{sc} styles exceeds the limit of {}", limits.max_styles),
        ));
    }
    expect_len(
        STYL,
        styl,
        (sc as usize)
            .checked_mul(STYLE_RECORD)
            .and_then(|v| v.checked_add(SUB_HEADER)),
    )?;
    styl[SUB_HEADER..]
        .chunks_exact(STYLE_RECORD)
        .map(decode_style)
        .collect()
}

/// A validated `GEOM` payload. Paths are decoded on demand.
#[derive(Debug, Clone, Copy)]
pub struct GeometryPool<'a> {
    offsets: U32s<'a>,
    blob: &'a [u8],
    limits: GvaLimits,
}

impl<'a> GeometryPool<'a> {
    /// Validate the header, offset table and padding. No path is decoded.
    pub fn parse(geom: &'a [u8], limits: &GvaLimits) -> Result<GeometryPool<'a>, GfxDiag> {
        let (gc, reserved) = parse_sub_header(GEOM, geom)?;
        check_padding(GEOM, &reserved.to_le_bytes())?;
        if gc > limits.max_geometries {
            return Err(diag(
                codes::TOO_MANY_GEOMETRIES,
                format!(
                    "{gc} geometries exceeds the limit of {}",
                    limits.max_geometries
                ),
            ));
        }
        let table_len = (gc as usize + 1) * 4;
        let table_padded = pad16(table_len).ok_or_else(|| section_err(GEOM, "count overflows"))?;
        if geom.len() < SUB_HEADER + table_padded {
            return Err(section_err(GEOM, "offset table runs past the section"));
        }
        let offsets = U32s(&geom[SUB_HEADER..SUB_HEADER + table_len]);
        check_padding(
            GEOM,
            &geom[SUB_HEADER + table_len..SUB_HEADER + table_padded],
        )?;
        let mut prev = None;
        for off in offsets.iter() {
            match prev {
                None if off != 0 => return Err(section_err(GEOM, "first offset must be 0")),
                Some(p) if off <= p => {
                    return Err(section_err(GEOM, "offsets must strictly increase"))
                }
                _ => {}
            }
            prev = Some(off);
        }
        let blob_len = prev.unwrap_or(0) as usize;
        let blob_start = SUB_HEADER + table_padded;
        expect_len(
            GEOM,
            geom,
            pad16(blob_len).and_then(|p| p.checked_add(blob_start)),
        )?;
        check_padding(GEOM, &geom[blob_start + blob_len..])?;
        Ok(GeometryPool {
            offsets,
            blob: &geom[blob_start..blob_start + blob_len],
            limits: *limits,
        })
    }

    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Encoded bytes of path `g`.
    pub fn bytes(&self, g: usize) -> Option<&'a [u8]> {
        let start = self.offsets.get(g)? as usize;
        let end = self.offsets.get(g + 1)? as usize;
        Some(&self.blob[start..end])
    }

    /// Decode path `g`.
    pub fn get(&self, g: usize) -> Result<Path, GfxDiag> {
        let bytes = self
            .bytes(g)
            .ok_or_else(|| diag(codes::GEOMETRY_REF, format!("geometry {g} does not exist")))?;
        decode_path(bytes, &self.limits)
    }
}

// ---------------------------------------------------------------------------
// Decoder
// ---------------------------------------------------------------------------

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// A zero-copy view of a little-endian `u32` array.
#[derive(Debug, Clone, Copy)]
pub struct U32s<'a>(&'a [u8]);

impl<'a> U32s<'a> {
    pub fn len(&self) -> usize {
        self.0.len() / 4
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<u32> {
        (i < self.len()).then(|| u32_at(self.0, i * 4))
    }

    pub fn iter(&self) -> impl Iterator<Item = u32> + 'a {
        self.0
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
    }
}

/// A zero-copy view of a little-endian `i32` array.
#[derive(Debug, Clone, Copy)]
pub struct I32s<'a>(&'a [u8]);

impl<'a> I32s<'a> {
    pub fn len(&self) -> usize {
        self.0.len() / 4
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<i32> {
        (i < self.len()).then(|| u32_at(self.0, i * 4) as i32)
    }

    pub fn iter(&self) -> impl Iterator<Item = i32> + 'a {
        self.0
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
    }
}

/// The instance arrays. Each field is a separate contiguous array, as
/// directive §7.2 requires.
#[derive(Debug, Clone, Copy)]
pub struct Instances<'a> {
    pub geometry: U32s<'a>,
    pub style: U32s<'a>,
    pub e: I32s<'a>,
    pub f: I32s<'a>,
    pub a: I32s<'a>,
    pub b: I32s<'a>,
    pub c: I32s<'a>,
    pub d: I32s<'a>,
    pub opacity: &'a [u8],
}

fn rect_at(b: &[u8], off: usize) -> Rect {
    Rect {
        min_x: u32_at(b, off) as i32,
        min_y: u32_at(b, off + 4) as i32,
        max_x: u32_at(b, off + 8) as i32,
        max_y: u32_at(b, off + 12) as i32,
    }
}

/// A validated `.gva` file over borrowed (possibly memory-mapped) bytes.
#[derive(Debug, Clone)]
pub struct Gva<'a> {
    bytes: &'a [u8],
    limits: GvaLimits,
    pub width: u32,
    pub height: u32,
    geom: GeometryPool<'a>,
    styles: Vec<Style>,
    instances: Instances<'a>,
    geom_bounds: &'a [u8],
    inst_bounds: &'a [u8],
    hashes: Option<&'a [u8]>,
}

fn take<'a>(c: &Container<'a>, tag: Tag) -> Result<&'a [u8], GfxDiag> {
    c.section(tag)
        .map_err(from_container)?
        .ok_or_else(|| section_err(tag, "required section is missing"))
}

fn check_padding(tag: Tag, bytes: &[u8]) -> Result<(), GfxDiag> {
    if bytes.iter().any(|&b| b != 0) {
        return Err(section_err(tag, "padding or reserved bytes are not zero"));
    }
    Ok(())
}

/// Parse a 16-byte sub-header; returns its two counts after checking the
/// reserved tail.
fn parse_sub_header(tag: Tag, s: &[u8]) -> Result<(u32, u32), GfxDiag> {
    if s.len() < SUB_HEADER {
        return Err(section_err(tag, "shorter than its 16-byte header"));
    }
    check_padding(tag, &s[8..16])?;
    Ok((u32_at(s, 0), u32_at(s, 4)))
}

fn expect_len(tag: Tag, s: &[u8], want: Option<usize>) -> Result<(), GfxDiag> {
    match want {
        Some(w) if w == s.len() => Ok(()),
        Some(w) => Err(section_err(tag, format!("{} bytes, expected {w}", s.len()))),
        None => Err(section_err(tag, "declared counts overflow")),
    }
}

impl<'a> Gva<'a> {
    pub fn open(bytes: &'a [u8], limits: &GvaLimits) -> Result<Gva<'a>, GfxDiag> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gva,
                known_sections: &KNOWN,
            },
            &limits.container,
        )
        .map_err(from_container)?;

        // HEAD
        let head = take(&c, HEAD)?;
        expect_len(HEAD, head, Some(16))?;
        let unit = u16::from_le_bytes([head[8], head[9]]);
        if unit != UNIT_MILLIPOINT {
            return Err(diag(
                codes::UNSUPPORTED_UNIT,
                format!("coordinate unit {unit}"),
            ));
        }
        check_padding(HEAD, &head[10..16])?;

        // INST
        let inst = take(&c, INST)?;
        let (n, reserved) = parse_sub_header(INST, inst)?;
        check_padding(INST, &reserved.to_le_bytes())?;
        if n > limits.max_instances {
            return Err(diag(
                codes::TOO_MANY_INSTANCES,
                format!(
                    "{n} instances exceeds the limit of {}",
                    limits.max_instances
                ),
            ));
        }
        let n_us = n as usize;
        let arr = pad16(n_us * 4).ok_or_else(|| section_err(INST, "count overflows"))?;
        expect_len(
            INST,
            inst,
            arr.checked_mul(8).and_then(|v| v.checked_add(SUB_HEADER)),
        )?;
        let array = |k: usize| {
            let start = SUB_HEADER + k * arr;
            (
                &inst[start..start + n_us * 4],
                &inst[start + n_us * 4..start + arr],
            )
        };
        for k in 0..8 {
            check_padding(INST, array(k).1)?;
        }

        // GEOM
        let geom = GeometryPool::parse(take(&c, GEOM)?, limits)?;
        let gc = geom.len() as u32;
        let gc_us = gc as usize;

        // STYL
        let styles = parse_style_pool(take(&c, STYL)?, limits)?;
        let sc = styles.len() as u32;

        // MODS
        let mods = take(&c, MODS)?;
        let (mn, channels) = parse_sub_header(MODS, mods)?;
        if mn != n {
            return Err(diag(
                codes::COUNT_MISMATCH,
                format!("MODS has {mn} entries for {n} instances"),
            ));
        }
        if channels != CHANNEL_OPACITY {
            return Err(section_err(
                MODS,
                format!("unsupported channel set {channels:#x}"),
            ));
        }
        expect_len(MODS, mods, pad16(n_us).map(|p| p + SUB_HEADER))?;
        let opacity = &mods[SUB_HEADER..SUB_HEADER + n_us];
        check_padding(MODS, &mods[SUB_HEADER + n_us..])?;

        // BBOX
        let bbox = take(&c, BBOX)?;
        let (bg, bi) = parse_sub_header(BBOX, bbox)?;
        if bg != gc || bi != n {
            return Err(diag(
                codes::COUNT_MISMATCH,
                format!("BBOX covers {bg} geometries / {bi} instances, file has {gc} / {n}"),
            ));
        }
        expect_len(BBOX, bbox, Some(SUB_HEADER + (gc_us + n_us) * RECT_RECORD))?;
        let geom_bounds = &bbox[SUB_HEADER..SUB_HEADER + gc_us * RECT_RECORD];
        let inst_bounds = &bbox[SUB_HEADER + gc_us * RECT_RECORD..];

        // HASH (optional)
        let hashes = match c.section(HASH).map_err(from_container)? {
            None => None,
            Some(h) => {
                let (hc, alg) = parse_sub_header(HASH, h)?;
                if hc != gc {
                    return Err(diag(
                        codes::COUNT_MISMATCH,
                        format!("HASH has {hc} entries for {gc} geometries"),
                    ));
                }
                if alg != HASH_SHA256_128 {
                    return Err(section_err(HASH, format!("unknown algorithm {alg}")));
                }
                expect_len(HASH, h, Some(SUB_HEADER + gc_us * HASH_RECORD))?;
                Some(&h[SUB_HEADER..])
            }
        };

        let instances = Instances {
            geometry: U32s(array(0).0),
            style: U32s(array(1).0),
            e: I32s(array(2).0),
            f: I32s(array(3).0),
            a: I32s(array(4).0),
            b: I32s(array(5).0),
            c: I32s(array(6).0),
            d: I32s(array(7).0),
            opacity,
        };
        if let Some(i) = instances.geometry.iter().position(|g| g >= gc) {
            return Err(diag(
                codes::GEOMETRY_REF,
                format!("instance {i} references a missing geometry"),
            ));
        }
        if let Some(i) = instances.style.iter().position(|s| s >= sc) {
            return Err(diag(
                codes::STYLE_REF,
                format!("instance {i} references a missing style"),
            ));
        }

        Ok(Gva {
            bytes,
            limits: *limits,
            width: u32_at(head, 0),
            height: u32_at(head, 4),
            geom,
            styles,
            instances,
            geom_bounds,
            inst_bounds,
            hashes,
        })
    }

    pub fn instance_count(&self) -> usize {
        self.instances.geometry.len()
    }

    pub fn geometry_count(&self) -> usize {
        self.geom.len()
    }

    pub fn styles(&self) -> &[Style] {
        &self.styles
    }

    /// Zero-copy instance and modulation arrays.
    pub fn instances(&self) -> &Instances<'a> {
        &self.instances
    }

    pub fn transform(&self, i: usize) -> Option<Affine> {
        let v = &self.instances;
        Some(Affine {
            a: v.a.get(i)?,
            b: v.b.get(i)?,
            c: v.c.get(i)?,
            d: v.d.get(i)?,
            e: v.e.get(i)?,
            f: v.f.get(i)?,
        })
    }

    /// Encoded bytes of geometry `g`.
    fn geometry_bytes(&self, g: usize) -> Option<&'a [u8]> {
        self.geom.bytes(g)
    }

    /// Decode one geometry on demand.
    pub fn geometry(&self, g: usize) -> Result<Path, GfxDiag> {
        let bytes = self
            .geometry_bytes(g)
            .ok_or_else(|| diag(codes::GEOMETRY_REF, format!("geometry {g} does not exist")))?;
        decode_path(bytes, &self.limits)
    }

    pub fn geometry_bounds(&self, g: usize) -> Option<Rect> {
        (g < self.geometry_count()).then(|| rect_at(self.geom_bounds, g * RECT_RECORD))
    }

    /// Conservative document bounds of instance `i`, stroke included.
    pub fn instance_bounds(&self, i: usize) -> Option<Rect> {
        (i < self.instance_count()).then(|| rect_at(self.inst_bounds, i * RECT_RECORD))
    }

    /// Instances whose bounds intersect `area`, in paint order. This is a
    /// spatial query answered from `BBOX`, without decoding geometry or
    /// rasterising.
    pub fn instances_intersecting(&self, area: Rect) -> impl Iterator<Item = usize> + '_ {
        (0..self.instance_count())
            .filter(move |&i| rect_at(self.inst_bounds, i * RECT_RECORD).intersects(&area))
    }

    /// Structural identity of geometry `g` (from `HASH`), if present.
    pub fn geometry_identity(&self, g: usize) -> Option<[u8; 16]> {
        let h = self.hashes?;
        let rec = h.get(g * HASH_RECORD..(g + 1) * HASH_RECORD)?;
        let mut out = [0u8; 16];
        out.copy_from_slice(rec);
        Some(out)
    }

    /// Expand to a GIR drawing. Each geometry is decoded once and shared by
    /// all its instances.
    pub fn to_drawing(&self) -> Result<Drawing, GfxDiag> {
        let mut decoded: Vec<Option<Arc<Path>>> = vec![None; self.geometry_count()];
        let mut total = 0u64;
        let mut items = Vec::with_capacity(self.instance_count());
        for i in 0..self.instance_count() {
            let g = self.instances.geometry.get(i).unwrap_or(0) as usize;
            let path = match &decoded[g] {
                Some(p) => p.clone(),
                None => {
                    let p = Arc::new(self.geometry(g)?);
                    decoded[g] = Some(p.clone());
                    p
                }
            };
            total += path.segments().len() as u64;
            if total > self.limits.max_total_segments {
                return Err(diag(
                    codes::TOO_MANY_SEGMENTS,
                    format!(
                        "expanding exceeds {} total segments",
                        self.limits.max_total_segments
                    ),
                ));
            }
            let s = self.instances.style.get(i).unwrap_or(0) as usize;
            items.push(Item {
                path,
                style: self.styles[s],
                transform: self.transform(i).unwrap_or(Affine::IDENTITY),
                opacity: self.instances.opacity[i],
            });
        }
        Ok(Drawing {
            width: self.width,
            height: self.height,
            items,
        })
    }

    /// Full verification: decode every geometry, check `HASH`, and check
    /// that the file is exactly what the canonical encoder would produce.
    pub fn verify(&self) -> Result<(), GfxDiag> {
        for g in 0..self.geometry_count() {
            self.geometry(g)?;
            if let (Some(id), Some(bytes)) = (self.geometry_identity(g), self.geometry_bytes(g)) {
                if Sha256::digest(bytes)[..HASH_RECORD] != id {
                    return Err(diag(
                        codes::NON_CANONICAL,
                        format!("HASH entry {g} does not match its geometry"),
                    ));
                }
            }
        }
        let again = encode(&self.to_drawing()?, &self.limits)?;
        if again != self.bytes {
            return Err(diag(
                codes::NON_CANONICAL,
                "file is not in canonical GVA form",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_graphics_ir::reference::{render, RenderLimits};
    use goya_graphics_ir::FIXED_ONE;

    fn pt(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    /// A circle of radius `r` mp centred at `(cx, cy)`, as four cubics.
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

    fn frame(x0: i32, y0: i32, x1: i32, y1: i32, inset: i32) -> Path {
        use Segment::*;
        Path::new(vec![
            MoveTo(pt(x0, y0)),
            LineTo(pt(x1, y0)),
            QuadTo(pt(x1 + inset, y0), pt(x1 + inset, y0 + inset)),
            LineTo(pt(x1 + inset, y1)),
            LineTo(pt(x0, y1)),
            Close,
            MoveTo(pt(x0 + inset, y0 + inset)),
            LineTo(pt(x1 - inset, y0 + inset)),
            LineTo(pt(x1 - inset, y1 - inset)),
            LineTo(pt(x0 + inset, y1 - inset)),
            Close,
        ])
        .unwrap()
    }

    fn check(x: i32, y: i32) -> Path {
        use Segment::*;
        Path::new(vec![
            MoveTo(pt(x, y)),
            LineTo(pt(x + 3000, y + 3000)),
            LineTo(pt(x + 9000, y - 4000)),
        ])
        .unwrap()
    }

    fn filled(colour: Rgba8, rule: FillRule) -> Style {
        Style {
            fill: Some(Fill { colour, rule }),
            stroke: None,
        }
    }

    fn stroked(colour: Rgba8, width: u32, cap: LineCap) -> Style {
        Style {
            fill: None,
            stroke: Some(Stroke {
                colour,
                width,
                cap,
                join: LineJoin::Round,
            }),
        }
    }

    fn item(path: Path, style: Style, transform: Affine, opacity: u8) -> Item {
        Item {
            path: Arc::new(path),
            style,
            transform,
            opacity,
        }
    }

    const BLUE: Rgba8 = Rgba8::new(42, 130, 255, 255);
    const WHITE: Rgba8 = Rgba8::new(255, 255, 255, 255);
    const GREY: Rgba8 = Rgba8::new(60, 60, 70, 200);

    /// A 24 pt status icon: framed background, filled indicator, check mark.
    fn icon() -> Drawing {
        Drawing {
            width: 24_000,
            height: 24_000,
            items: vec![
                item(
                    frame(1000, 1000, 22_000, 23_000, 1500),
                    filled(GREY, FillRule::EvenOdd),
                    Affine::IDENTITY,
                    255,
                ),
                item(
                    circle(12_000, 12_000, 7_000),
                    filled(BLUE, FillRule::NonZero),
                    Affine::IDENTITY,
                    230,
                ),
                item(
                    check(7_500, 12_500),
                    stroked(WHITE, 1_800, LineCap::Round),
                    Affine::IDENTITY,
                    255,
                ),
            ],
        }
    }

    /// `cols × rows` copies of the indicator, each drawn at absolute
    /// coordinates as an SVG export would write them.
    fn grid(cols: i32, rows: i32) -> Drawing {
        let mut items = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                let (cx, cy) = (i * 10_000 + 5_000, j * 10_000 + 5_000);
                let style = if (i + j) % 2 == 0 {
                    filled(BLUE, FillRule::NonZero)
                } else {
                    filled(GREY, FillRule::NonZero)
                };
                items.push(item(circle(cx, cy, 4_000), style, Affine::IDENTITY, 255));
            }
        }
        Drawing {
            width: cols as u32 * 10_000,
            height: rows as u32 * 10_000,
            items,
        }
    }

    fn lim() -> GvaLimits {
        GvaLimits::default()
    }

    #[test]
    fn round_trip_renders_bit_identically() {
        let d = icon();
        let bytes = encode(&d, &lim()).unwrap();
        let gva = Gva::open(&bytes, &lim()).unwrap();
        gva.verify().unwrap();
        let back = gva.to_drawing().unwrap();
        let rl = RenderLimits::default();
        for scale in [1.0, 2.5, 4.0] {
            assert_eq!(
                render(&d, scale, &rl).unwrap(),
                render(&back, scale, &rl).unwrap(),
                "scale {scale}"
            );
        }
        assert_eq!(
            encode(&back, &lim()).unwrap(),
            bytes,
            "encoding is idempotent"
        );
    }

    #[test]
    fn encoding_is_deterministic() {
        assert_eq!(
            encode(&icon(), &lim()).unwrap(),
            encode(&icon(), &lim()).unwrap()
        );
        assert_eq!(
            encode(&grid(7, 5), &lim()).unwrap(),
            encode(&grid(7, 5), &lim()).unwrap()
        );
    }

    #[test]
    fn repeated_geometry_is_stored_once() {
        let d = grid(20, 10);
        let bytes = encode(&d, &lim()).unwrap();
        let gva = Gva::open(&bytes, &lim()).unwrap();
        assert_eq!(gva.instance_count(), 200);
        assert_eq!(
            gva.geometry_count(),
            1,
            "absolute copies fold into one origin-anchored path"
        );
        assert_eq!(gva.styles().len(), 2);
        gva.verify().unwrap();
        let rl = RenderLimits::default();
        assert_eq!(
            render(&d, 1.0, &rl).unwrap(),
            render(&gva.to_drawing().unwrap(), 1.0, &rl).unwrap()
        );
    }

    #[test]
    fn inexact_fold_keeps_geometry_in_place() {
        // Half scale with an odd anchor: folding would need a fractional
        // millipoint, so the geometry must stay where it is.
        let half = Affine {
            a: FIXED_ONE / 2,
            d: FIXED_ONE / 2,
            ..Affine::IDENTITY
        };
        let d = Drawing {
            width: 10_000,
            height: 10_000,
            items: vec![item(
                circle(5_001, 5_001, 3_000),
                filled(BLUE, FillRule::NonZero),
                half,
                255,
            )],
        };
        let bytes = encode(&d, &lim()).unwrap();
        let gva = Gva::open(&bytes, &lim()).unwrap();
        assert_eq!(gva.geometry(0).unwrap().first_point(), pt(8_001, 5_001));
        assert_eq!(gva.transform(0), Some(half));
        gva.verify().unwrap();
    }

    #[test]
    fn structure_of_arrays_access() {
        let mut d = grid(3, 2);
        d.items[4].opacity = 17;
        // A shear whose effect on the anchor is a whole number of millipoints
        // (8192·29000 / 65536 = 3625), so folding stays exact.
        d.items[5].transform = Affine {
            b: 8_192,
            ..Affine::IDENTITY
        };
        let bytes = encode(&d, &lim()).unwrap();
        let gva = Gva::open(&bytes, &lim()).unwrap();
        let v = gva.instances();
        // Translation now carries each copy's position.
        let xs: Vec<i32> = v.e.iter().collect();
        let ys: Vec<i32> = v.f.iter().collect();
        assert_eq!(xs, vec![9_000, 19_000, 29_000, 9_000, 19_000, 29_000]);
        assert_eq!(ys, vec![5_000, 5_000, 5_000, 15_000, 15_000, 18_625]);
        assert_eq!(v.opacity, &[255, 255, 255, 255, 17, 255]);
        assert_eq!(v.geometry.iter().collect::<Vec<_>>(), vec![0; 6]);
        assert_eq!(v.style.iter().collect::<Vec<_>>(), vec![0, 1, 0, 1, 0, 1]);
        assert_eq!(v.b.get(5), Some(8_192));
        assert_eq!(v.a.get(6), None);
        assert!(gva.geometry_identity(0).is_some());
    }

    #[test]
    fn spatial_query_without_rasterising() {
        let bytes = encode(&grid(4, 4), &lim()).unwrap();
        let gva = Gva::open(&bytes, &lim()).unwrap();
        // A probe in the middle of cell (2, 1) hits exactly that instance.
        let probe = Rect {
            min_x: 25_000,
            min_y: 15_000,
            max_x: 25_000,
            max_y: 15_000,
        };
        assert_eq!(
            gva.instances_intersecting(probe).collect::<Vec<_>>(),
            vec![6]
        );
        let b = gva.instance_bounds(6).unwrap();
        assert_eq!(
            (b.min_x, b.min_y, b.max_x, b.max_y),
            (21_000, 11_000, 29_000, 19_000)
        );
        // Stroke width widens bounds conservatively.
        let s = encode(
            &Drawing {
                width: 20_000,
                height: 20_000,
                items: vec![item(
                    check(5_000, 10_000),
                    stroked(WHITE, 2_000, LineCap::Square),
                    Affine::IDENTITY,
                    255,
                )],
            },
            &lim(),
        )
        .unwrap();
        let g = Gva::open(&s, &lim()).unwrap();
        let r = g.instance_bounds(0).unwrap();
        assert!(
            r.min_x <= 5_000 - 1_415 && r.max_x >= 14_000 + 1_415,
            "{r:?}"
        );
    }

    #[test]
    fn golden_fixture_and_render() {
        // Pins the GVA byte layout and the reference renderer's output.
        // Regenerate only with a spec change:
        //   GOYA_GFX_BLESS=1 cargo test -p goya-gva golden
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("fixtures")
            .join("graphics")
            .join("gva");
        let bytes = encode(&icon(), &lim()).unwrap();
        let image = render(
            &Gva::open(&bytes, &lim()).unwrap().to_drawing().unwrap(),
            4.0,
            &RenderLimits::default(),
        )
        .unwrap();
        let image_digest: String = Sha256::digest(&image.rgba)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if std::env::var_os("GOYA_GFX_BLESS").is_some() {
            std::fs::write(dir.join("icon.gva"), &bytes).unwrap();
            std::fs::write(dir.join("icon.x4.pam"), image.to_pam()).unwrap();
            std::fs::write(dir.join("icon.x4.sha256"), format!("{image_digest}\n")).unwrap();
        }
        let golden = std::fs::read(dir.join("icon.gva"))
            .expect("golden fixture missing; see comment to bless");
        assert_eq!(bytes, golden, "GVA byte layout changed");
        let want = std::fs::read_to_string(dir.join("icon.x4.sha256")).unwrap();
        assert_eq!(image_digest, want.trim(), "reference rendering changed");
    }

    // ---------------------------------------------------------------------------
    // Hostile content inside a valid container
    // ---------------------------------------------------------------------------

    /// Re-wrap a GVA file after editing one section's payload, so the
    /// container's digests stay valid and only GVA-level checks can object.
    fn rebuild(bytes: &[u8], tag: Tag, edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let c = Container::open(
            bytes,
            &Expect {
                format: Format::Gva,
                known_sections: &KNOWN,
            },
            &Limits::default(),
        )
        .unwrap();
        let mut owned: Vec<(Tag, bool, Vec<u8>)> = c
            .entries()
            .iter()
            .map(|e| {
                (
                    e.tag,
                    e.mandatory,
                    c.section(e.tag).unwrap().unwrap().to_vec(),
                )
            })
            .collect();
        let mut edit = Some(edit);
        owned.retain_mut(|(t, _, data)| {
            if *t == tag {
                (edit.take().unwrap())(data);
                !data.is_empty()
            } else {
                true
            }
        });
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
            Format::Gva,
            Profile::Baseline,
            0,
            &inputs,
            &Limits::default(),
        )
        .unwrap()
    }

    fn open_code(bytes: &[u8]) -> u16 {
        Gva::open(bytes, &lim())
            .expect_err("expected rejection")
            .code
    }

    #[test]
    fn reference_and_count_violations() {
        let base = encode(&grid(3, 2), &lim()).unwrap();
        let geom_ref = rebuild(&base, INST, |d| {
            d[16..20].copy_from_slice(&9u32.to_le_bytes())
        });
        assert_eq!(open_code(&geom_ref), codes::GEOMETRY_REF);
        // Six instances: each array is 32 bytes, so the style array starts at 48.
        let style_ref = rebuild(&base, INST, |d| {
            d[48..52].copy_from_slice(&9u32.to_le_bytes())
        });
        assert_eq!(open_code(&style_ref), codes::STYLE_REF);
        let mods = rebuild(&base, MODS, |d| d[0] = 5);
        assert_eq!(open_code(&mods), codes::COUNT_MISMATCH);
        let bbox = rebuild(&base, BBOX, |d| d[4] = 1);
        assert_eq!(open_code(&bbox), codes::COUNT_MISMATCH);
        let short = rebuild(&base, INST, |d| d.truncate(d.len() - 16));
        assert_eq!(open_code(&short), codes::MALFORMED_SECTION);
        let missing = rebuild(&base, BBOX, |d| d.clear());
        assert_eq!(open_code(&missing), codes::MALFORMED_SECTION);
        let reserved = rebuild(&base, HEAD, |d| d[15] = 1);
        assert_eq!(open_code(&reserved), codes::MALFORMED_SECTION);
        let unit = rebuild(&base, HEAD, |d| d[8] = 1);
        assert_eq!(open_code(&unit), codes::UNSUPPORTED_UNIT);
        let style = rebuild(&base, STYL, |d| d[16] |= 0x80);
        assert_eq!(open_code(&style), codes::INVALID_STYLE);
        let zero_width = rebuild(&encode(&icon(), &lim()).unwrap(), STYL, |d| {
            // Third style is the check-mark stroke; zero its width.
            d[16 + 2 * 16 + 12..16 + 2 * 16 + 16].fill(0);
        });
        assert_eq!(open_code(&zero_width), codes::INVALID_STYLE);
        let offsets = rebuild(&base, GEOM, |d| d[20] = 0);
        assert_eq!(open_code(&offsets), codes::MALFORMED_SECTION);
    }

    /// A GEOM section holding exactly one hand-encoded path.
    fn geom_section(path: &[u8]) -> Vec<u8> {
        let mut g = sub_header(1, 0);
        g.extend_from_slice(&0u32.to_le_bytes());
        g.extend_from_slice(&(path.len() as u32).to_le_bytes());
        pad_to_16(&mut g);
        g.extend_from_slice(path);
        pad_to_16(&mut g);
        g
    }

    #[test]
    fn hostile_geometry_is_rejected_on_decode() {
        use Segment::*;
        let line = Path::new(vec![MoveTo(Point::ORIGIN), LineTo(pt(1, 0))]).unwrap();
        let base = encode(
            &Drawing {
                width: 1000,
                height: 1000,
                items: vec![item(
                    line,
                    stroked(WHITE, 100, LineCap::Butt),
                    Affine::IDENTITY,
                    255,
                )],
            },
            &lim(),
        )
        .unwrap();
        let cases: [(&[u8], u16); 6] = [
            // segments=2, M L, (0,0) with a non-minimal 0x80 0x00, then (+1, 0).
            (&[2, 0, 1, 0x80, 0x00, 0, 2, 0], codes::NON_MINIMAL_VARINT),
            (&[2, 0, 9, 0, 0, 2, 0], codes::MALFORMED_GEOMETRY),
            (&[2, 1, 1, 0, 0, 2, 0], codes::MALFORMED_GEOMETRY),
            (&[2, 0, 1, 0, 0, 2, 0, 7], codes::MALFORMED_GEOMETRY),
            (&[100, 0, 1, 0, 0, 2, 0], codes::MALFORMED_GEOMETRY),
            // MoveTo, MoveTo, LineTo: valid but not canonical.
            (&[3, 0, 0, 1, 0, 0, 0, 0, 2, 0], codes::NON_CANONICAL),
        ];
        for (path, code) in cases {
            let bad = rebuild(&base, GEOM, |d| *d = geom_section(path));
            let bad = rebuild(&bad, HASH, |d| d.clear());
            let gva = Gva::open(&bad, &lim()).unwrap();
            assert_eq!(gva.geometry(0).unwrap_err().code, code, "path {path:?}");
            assert_eq!(gva.verify().unwrap_err().code, code);
        }
        // Coordinates that leave the i32 range.
        let mut far = vec![2, 0, 1];
        put_varint(&mut far, zigzag(i64::from(i32::MAX)));
        far.push(0);
        put_varint(&mut far, zigzag(10));
        far.push(0);
        let bad = rebuild(
            &rebuild(&base, GEOM, |d| *d = geom_section(&far)),
            HASH,
            |d| d.clear(),
        );
        assert_eq!(
            Gva::open(&bad, &lim())
                .unwrap()
                .geometry(0)
                .unwrap_err()
                .code,
            codes::COORDINATE_OVERFLOW
        );
    }

    #[test]
    fn non_canonical_pools_fail_verification() {
        let base = encode(&grid(2, 1), &lim()).unwrap();
        // Swap the two styles and the instance style refs: renders the same,
        // but pools are no longer in first-use order.
        let swapped = rebuild(&base, STYL, |d| {
            let (a, b) = (d[16..32].to_vec(), d[32..48].to_vec());
            d[16..32].copy_from_slice(&b);
            d[32..48].copy_from_slice(&a);
        });
        let swapped = rebuild(&swapped, INST, |d| {
            // style array is the second array, at 16 + 16.
            d[32..36].copy_from_slice(&1u32.to_le_bytes());
            d[36..40].copy_from_slice(&0u32.to_le_bytes());
        });
        let gva = Gva::open(&swapped, &lim()).unwrap();
        let rl = RenderLimits::default();
        assert_eq!(
            render(&gva.to_drawing().unwrap(), 1.0, &rl).unwrap(),
            render(&grid(2, 1), 1.0, &rl).unwrap()
        );
        assert_eq!(gva.verify().unwrap_err().code, codes::NON_CANONICAL);
        // A wrong HASH entry is also caught.
        let bad_hash = rebuild(&base, HASH, |d| d[16] ^= 1);
        assert_eq!(
            Gva::open(&bad_hash, &lim())
                .unwrap()
                .verify()
                .unwrap_err()
                .code,
            codes::NON_CANONICAL
        );
    }

    #[test]
    fn limits_apply() {
        let bytes = encode(&grid(4, 4), &lim()).unwrap();
        let few = GvaLimits {
            max_instances: 3,
            ..lim()
        };
        assert_eq!(
            Gva::open(&bytes, &few).unwrap_err().code,
            codes::TOO_MANY_INSTANCES
        );
        assert_eq!(
            encode(&grid(4, 4), &few).unwrap_err().code,
            codes::TOO_MANY_INSTANCES
        );
        let expand = GvaLimits {
            max_total_segments: 20,
            ..lim()
        };
        let gva = Gva::open(&bytes, &expand).unwrap();
        assert_eq!(gva.to_drawing().unwrap_err().code, codes::TOO_MANY_SEGMENTS);
        let segs = GvaLimits {
            max_segments_per_path: 3,
            ..lim()
        };
        assert_eq!(
            Gva::open(&bytes, &segs)
                .unwrap()
                .geometry(0)
                .unwrap_err()
                .code,
            codes::TOO_MANY_SEGMENTS
        );
    }

    #[test]
    fn empty_drawing() {
        let d = Drawing {
            width: 0,
            height: 0,
            items: vec![],
        };
        let bytes = encode(&d, &lim()).unwrap();
        let gva = Gva::open(&bytes, &lim()).unwrap();
        gva.verify().unwrap();
        assert_eq!(gva.to_drawing().unwrap(), d);
    }

    /// Deterministic fuzzing of section payloads behind valid container
    /// digests: open, verify, expand and render must never panic.
    #[test]
    fn payload_fuzz_never_panics() {
        let base = encode(&icon(), &lim()).unwrap();
        let tags = [HEAD, GEOM, STYL, INST, MODS, BBOX, HASH];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let rl = RenderLimits {
            max_pixels: 64 * 64,
            max_edges: 100_000,
            max_curve_steps: 64,
        };
        for _ in 0..3_000 {
            let tag = tags[(next() % tags.len() as u64) as usize];
            let r1 = next();
            let r2 = next();
            let r3 = next();
            let mutated = rebuild(&base, tag, |d| {
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
            if let Ok(gva) = Gva::open(&mutated, &lim()) {
                let _ = gva.verify();
                if let Ok(d) = gva.to_drawing() {
                    let _ = render(&d, 64.0 / 24.0, &rl);
                }
            }
        }
    }
}
