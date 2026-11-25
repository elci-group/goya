//! GVI-O operative spans (`docs/graphics/gvi-operative.md` §2).
//!
//! A column of values (integers or labels) is stored as an ordered list of
//! **spans**. Each span covers a contiguous run of values and is either a
//! literal run or an *operative*: a bounded, closed-form relationship whose
//! every value can be evaluated directly from its local index `k`, with no
//! traversal of earlier values.
//!
//! | Op | Name | Value `k` (0-based within the span) |
//! |---|---|---|
//! | 0 | `LITERAL` | stored explicitly (also serves as `EXCEPT`) |
//! | 1 | `CONST` | `c` |
//! | 2 | `LINE` | `a + k·d` |
//! | 3 | `RATIO` | `a + floor(k·N / D)`, `D ≥ 1` |
//! | 4 | `NUMBERED` (labels) | `prefix ‖ decimal(a + k·d) ‖ suffix` |
//!
//! All arithmetic is checked `i128`, and results must fit `i64`. Decimal
//! formatting is Rust's canonical `i64` form (no leading zeros, `-` for
//! negatives). Encoding is deterministic. Readers re-encode and compare, so
//! a column has exactly one valid byte form.
//!
//! Wire format, everything varint (LEB128, minimal) unless noted:
//! `spans`, then per span `u8 op`, `len`, and the payload. `LITERAL` ints
//! are zigzag values. `LITERAL` labels are `byte_len ‖ bytes`. `CONST` is
//! zigzag `c`. `LINE` is zigzag `a, d`. `RATIO` is zigzag `a, N`, then
//! `D`. `NUMBERED` is `prefix_len ‖ prefix ‖ suffix_len ‖ suffix`, then
//! zigzag `a, d`.

use goya_graphics_ir::{codes, GfxDiag};

const OP_LITERAL: u8 = 0;
const OP_CONST: u8 = 1;
const OP_LINE: u8 = 2;
const OP_RATIO: u8 = 3;
const OP_NUMBERED: u8 = 4;
/// Longest prefix or suffix a `NUMBERED` span may carry.
pub(crate) const MAX_AFFIX: usize = 255;
/// Longest run any candidate scan examines (the compile work bound).
const MAX_SCAN: usize = 1 << 16;

fn err(detail: impl Into<String>) -> GfxDiag {
    GfxDiag::new(
        codes::MALFORMED_SECTION,
        format!("operative: {}", detail.into()),
    )
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

pub(crate) fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn varint_len(v: u64) -> usize {
    (64 - v.max(1).leading_zeros() as usize).div_ceil(7)
}

/// A bounds-checked cursor over operative bytes.
pub(crate) struct Cursor<'a> {
    pub buf: &'a [u8],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Cursor<'a> {
        Cursor { buf, pos: 0 }
    }

    pub(crate) fn varint(&mut self) -> Result<u64, GfxDiag> {
        let mut v = 0u64;
        let mut shift = 0u32;
        loop {
            let b = *self
                .buf
                .get(self.pos)
                .ok_or_else(|| err("truncated varint"))?;
            self.pos += 1;
            if shift == 63 && b > 1 {
                return Err(err("varint overflows"));
            }
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                if b == 0 && shift > 0 {
                    return Err(GfxDiag::new(
                        codes::NON_MINIMAL_VARINT,
                        "operative varint is not minimal",
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

    fn signed(&mut self) -> Result<i64, GfxDiag> {
        Ok(unzigzag(self.varint()?))
    }

    pub(crate) fn u8(&mut self) -> Result<u8, GfxDiag> {
        let b = *self.buf.get(self.pos).ok_or_else(|| err("truncated"))?;
        self.pos += 1;
        Ok(b)
    }

    pub(crate) fn bytes(&mut self, n: usize) -> Result<&'a [u8], GfxDiag> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| err("truncated"))?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    /// A span length that must fit the values still owed to the column.
    fn len(&mut self, remaining: usize) -> Result<usize, GfxDiag> {
        let l = self.varint()?;
        if l == 0 || l > remaining as u64 {
            return Err(err("span length is zero or overruns the column"));
        }
        Ok(l as usize)
    }
}

// ---------------------------------------------------------------------------
// Integer columns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntOp {
    Const(i64),
    Line(i64, i64),
    Ratio(i64, i64, u64),
}

fn eval_int(op: IntOp, k: usize) -> Option<i64> {
    let k = k as i128;
    let v = match op {
        IntOp::Const(c) => i128::from(c),
        IntOp::Line(a, d) => i128::from(a) + k * i128::from(d),
        IntOp::Ratio(a, n, den) => i128::from(a) + (k * i128::from(n)).div_euclid(i128::from(den)),
    };
    i64::try_from(v).ok()
}

fn op_cost(op: IntOp, len: usize) -> usize {
    1 + varint_len(len as u64)
        + match op {
            IntOp::Const(c) => varint_len(zigzag(c)),
            IntOp::Line(a, d) => varint_len(zigzag(a)) + varint_len(zigzag(d)),
            IntOp::Ratio(a, n, den) => {
                varint_len(zigzag(a)) + varint_len(zigzag(n)) + varint_len(den)
            }
        }
}

/// How many values from `i` the operative reproduces exactly.
fn run(values: &[i64], i: usize, op: IntOp) -> usize {
    values[i..]
        .iter()
        .take(MAX_SCAN)
        .enumerate()
        .take_while(|&(k, &v)| eval_int(op, k) == Some(v))
        .count()
}

/// The best operative starting at `i`, as `(op, len, bytes saved)`.
fn best_int(values: &[i64], i: usize) -> Option<(IntOp, usize, isize)> {
    let lit = |j: usize| -> usize { values[i..j].iter().map(|&v| varint_len(zigzag(v))).sum() };
    let mut best: Option<(IntOp, usize, isize)> = None;
    let mut consider = |op: IntOp, len: usize| {
        if len < 2 {
            return;
        }
        let saved = lit(i + len) as isize - op_cost(op, len) as isize;
        if saved > 0 && best.is_none_or(|b| saved > b.2) {
            best = Some((op, len, saved));
        }
    };
    let c = IntOp::Const(values[i]);
    consider(c, run(values, i, c));
    if i + 1 < values.len() {
        let d = i128::from(values[i + 1]) - i128::from(values[i]);
        if let Ok(d) = i64::try_from(d) {
            let line = IntOp::Line(values[i], d);
            consider(line, run(values, i, line));
        }
        // RATIO: take the run whose steps are two adjacent integers, and fit
        // its endpoints exactly.
        let step = |k: usize| i128::from(values[k + 1]) - i128::from(values[k]);
        let lo = step(i);
        let mut j = i + 1;
        let (mut min, mut max) = (lo, lo);
        while j + 1 < values.len() && j - i < MAX_SCAN {
            let s = step(j);
            let (nmin, nmax) = (min.min(s), max.max(s));
            if nmax - nmin > 1 {
                break;
            }
            (min, max) = (nmin, nmax);
            j += 1;
        }
        if j - i >= 2 {
            let n = i128::from(values[j]) - i128::from(values[i]);
            if let (Ok(n), den) = (i64::try_from(n), (j - i) as u64) {
                let ratio = IntOp::Ratio(values[i], n, den);
                // Valid only over exactly the span it was fitted to.
                let r = run(values, i, ratio).min(j - i + 1);
                if r == j - i + 1 {
                    consider(ratio, r);
                }
            }
        }
    }
    best
}

/// Encode an integer column canonically.
pub(crate) fn encode_ints(values: &[i64]) -> Vec<u8> {
    let mut spans: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut literal: Vec<i64> = Vec::new();
    let flush = |spans: &mut Vec<(u8, Vec<u8>)>, literal: &mut Vec<i64>| {
        if literal.is_empty() {
            return;
        }
        let mut p = Vec::new();
        put_varint(&mut p, literal.len() as u64);
        for &v in literal.iter() {
            put_varint(&mut p, zigzag(v));
        }
        spans.push((OP_LITERAL, p));
        literal.clear();
    };
    let mut i = 0;
    while i < values.len() {
        match best_int(values, i) {
            Some((op, len, _)) => {
                flush(&mut spans, &mut literal);
                let mut p = Vec::new();
                put_varint(&mut p, len as u64);
                let code = match op {
                    IntOp::Const(c) => {
                        put_varint(&mut p, zigzag(c));
                        OP_CONST
                    }
                    IntOp::Line(a, d) => {
                        put_varint(&mut p, zigzag(a));
                        put_varint(&mut p, zigzag(d));
                        OP_LINE
                    }
                    IntOp::Ratio(a, n, den) => {
                        put_varint(&mut p, zigzag(a));
                        put_varint(&mut p, zigzag(n));
                        put_varint(&mut p, den);
                        OP_RATIO
                    }
                };
                spans.push((code, p));
                i += len;
            }
            None => {
                literal.push(values[i]);
                i += 1;
            }
        }
    }
    flush(&mut spans, &mut literal);
    let mut out = Vec::new();
    put_varint(&mut out, spans.len() as u64);
    for (code, p) in spans {
        out.push(code);
        out.extend_from_slice(&p);
    }
    out
}

/// Decode an integer column of exactly `count` values.
pub(crate) fn decode_ints(c: &mut Cursor<'_>, count: usize) -> Result<Vec<i64>, GfxDiag> {
    let spans = c.varint()?;
    if spans > count as u64 + 1 {
        return Err(err("more spans than values"));
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..spans {
        let op = c.u8()?;
        let len = c.len(count - out.len())?;
        let op = match op {
            OP_LITERAL => {
                for _ in 0..len {
                    out.push(c.signed()?);
                }
                continue;
            }
            OP_CONST => IntOp::Const(c.signed()?),
            OP_LINE => IntOp::Line(c.signed()?, c.signed()?),
            OP_RATIO => {
                let (a, n, den) = (c.signed()?, c.signed()?, c.varint()?);
                if den == 0 {
                    return Err(err("RATIO denominator is zero"));
                }
                IntOp::Ratio(a, n, den)
            }
            other => return Err(err(format!("unknown integer operative {other}"))),
        };
        for k in 0..len {
            out.push(eval_int(op, k).ok_or_else(|| err("operative value overflows i64"))?);
        }
    }
    if out.len() != count {
        return Err(err("spans do not cover the column"));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Label columns
// ---------------------------------------------------------------------------

/// Split a label into `(prefix, number, suffix)` around its last decimal
/// run, if that run is a canonical `i64` decimal.
fn numbered_parts(s: &str) -> Option<(&str, i64, &str)> {
    let b = s.as_bytes();
    let end = b.iter().rposition(u8::is_ascii_digit)? + 1;
    let mut start = end;
    while start > 0 && b[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start > 0 && b[start - 1] == b'-' {
        start -= 1;
    }
    let n: i64 = s[start..end].parse().ok()?;
    (n.to_string() == s[start..end]).then_some((&s[..start], n, &s[end..]))
}

fn numbered(prefix: &str, n: i64, suffix: &str) -> String {
    format!("{prefix}{n}{suffix}")
}

/// Encode a label column canonically.
pub(crate) fn encode_labels(labels: &[&str]) -> Vec<u8> {
    let mut spans: Vec<Vec<u8>> = Vec::new();
    let mut literal: Vec<&str> = Vec::new();
    let flush = |spans: &mut Vec<Vec<u8>>, literal: &mut Vec<&str>| {
        if literal.is_empty() {
            return;
        }
        let mut p = vec![OP_LITERAL];
        put_varint(&mut p, literal.len() as u64);
        for s in literal.iter() {
            put_varint(&mut p, s.len() as u64);
            p.extend_from_slice(s.as_bytes());
        }
        spans.push(p);
        literal.clear();
    };
    let mut i = 0;
    while i < labels.len() {
        let candidate = (i + 1 < labels.len())
            .then(|| numbered_parts(labels[i]))
            .flatten()
            .filter(|(p, _, s)| p.len() <= MAX_AFFIX && s.len() <= MAX_AFFIX)
            .and_then(|(prefix, a, suffix)| {
                let (_, b, _) = numbered_parts(labels[i + 1])?;
                let d = b.checked_sub(a)?;
                let len = labels[i..]
                    .iter()
                    .take(MAX_SCAN)
                    .enumerate()
                    .take_while(|&(k, s)| {
                        (k as i64)
                            .checked_mul(d)
                            .and_then(|x| x.checked_add(a))
                            .map(|n| numbered(prefix, n, suffix))
                            == Some((*s).to_string())
                    })
                    .count();
                let lit: usize = labels[i..i + len]
                    .iter()
                    .map(|s| varint_len(s.len() as u64) + s.len())
                    .sum();
                let cost = 1
                    + varint_len(len as u64)
                    + varint_len(prefix.len() as u64)
                    + prefix.len()
                    + varint_len(suffix.len() as u64)
                    + suffix.len()
                    + varint_len(zigzag(a))
                    + varint_len(zigzag(d));
                (len >= 2 && cost < lit).then_some((prefix, suffix, a, d, len))
            });
        match candidate {
            Some((prefix, suffix, a, d, len)) => {
                flush(&mut spans, &mut literal);
                let mut p = vec![OP_NUMBERED];
                put_varint(&mut p, len as u64);
                put_varint(&mut p, prefix.len() as u64);
                p.extend_from_slice(prefix.as_bytes());
                put_varint(&mut p, suffix.len() as u64);
                p.extend_from_slice(suffix.as_bytes());
                put_varint(&mut p, zigzag(a));
                put_varint(&mut p, zigzag(d));
                spans.push(p);
                i += len;
            }
            None => {
                literal.push(labels[i]);
                i += 1;
            }
        }
    }
    flush(&mut spans, &mut literal);
    let mut out = Vec::new();
    put_varint(&mut out, spans.len() as u64);
    for p in spans {
        out.extend_from_slice(&p);
    }
    out
}

/// Decode a label column of exactly `count` labels. `budget` bounds the
/// total bytes materialised, as a defence against expansion bombs.
pub(crate) fn decode_labels(
    c: &mut Cursor<'_>,
    count: usize,
    budget: &mut usize,
) -> Result<Vec<String>, GfxDiag> {
    let spans = c.varint()?;
    if spans > count as u64 + 1 {
        return Err(err("more spans than labels"));
    }
    let mut out = Vec::with_capacity(count);
    let charge = |n: usize, budget: &mut usize| -> Result<(), GfxDiag> {
        *budget = budget
            .checked_sub(n)
            .ok_or_else(|| err("labels exceed the expansion budget"))?;
        Ok(())
    };
    let text = |b: &[u8]| -> Result<String, GfxDiag> {
        std::str::from_utf8(b)
            .map(str::to_string)
            .map_err(|_| err("label is not UTF-8"))
    };
    for _ in 0..spans {
        let op = c.u8()?;
        let len = c.len(count - out.len())?;
        match op {
            OP_LITERAL => {
                for _ in 0..len {
                    let n = c.varint()? as usize;
                    charge(n, budget)?;
                    out.push(text(c.bytes(n)?)?);
                }
            }
            OP_NUMBERED => {
                let pl = c.varint()? as usize;
                if pl > MAX_AFFIX {
                    return Err(err("NUMBERED prefix is too long"));
                }
                let prefix = text(c.bytes(pl)?)?;
                let sl = c.varint()? as usize;
                if sl > MAX_AFFIX {
                    return Err(err("NUMBERED suffix is too long"));
                }
                let suffix = text(c.bytes(sl)?)?;
                let (a, d) = (c.signed()?, c.signed()?);
                // Preflight: each label is at most prefix + 20 digits + suffix.
                charge(len.saturating_mul(pl + sl + 20), budget)?;
                for k in 0..len {
                    let n = (k as i64)
                        .checked_mul(d)
                        .and_then(|x| x.checked_add(a))
                        .ok_or_else(|| err("NUMBERED value overflows i64"))?;
                    out.push(numbered(&prefix, n, &suffix));
                }
            }
            other => return Err(err(format!("unknown label operative {other}"))),
        }
    }
    if out.len() != count {
        return Err(err("spans do not cover the labels"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(values: &[i64]) -> usize {
        let enc = encode_ints(values);
        let mut c = Cursor::new(&enc);
        assert_eq!(decode_ints(&mut c, values.len()).unwrap(), values);
        assert_eq!(c.pos, enc.len(), "the decoder consumes exactly the column");
        enc.len()
    }

    #[test]
    fn regular_sequences_are_tiny_and_exact() {
        // GO001: constant and repeated.
        assert!(ints(&[7; 1000]) <= 6);
        // GO002: positive and negative progressions.
        assert!(ints(&(0..1000).map(|k| 10 + 5 * k).collect::<Vec<_>>()) <= 8);
        assert!(ints(&(0..1000).map(|k| 3 - 7 * k).collect::<Vec<_>>()) <= 8);
        // GO003: floor(k·65536 / 1023) is reproduced exactly by RATIO.
        let t: Vec<i64> = (0..1024).map(|k| k * 65_536 / 1023).collect();
        assert!(ints(&t) <= 12, "{}", encode_ints(&t).len());
        let neg: Vec<i64> = (0..100).map(|k| (-3 * k as i64).div_euclid(7)).collect();
        ints(&neg);
        ints(&[]);
        ints(&[i64::MIN, i64::MAX, 0]);
    }

    #[test]
    fn partial_spans_keep_surrounding_literals() {
        // GO004 / O05: A–Z where B–W is a progression and the rest is not.
        let mut v = vec![9_999];
        v.extend((0..22).map(|k| 100 + 3 * k));
        v.extend([-5, 77_777, 3]);
        let n = ints(&v);
        assert!(n < v.iter().map(|&x| varint_len(zigzag(x))).sum::<usize>() + 2);
        // GO006: an irregular value inside a progression becomes an
        // exception (a literal span) and is restored exactly.
        let mut w: Vec<i64> = (0..500).map(|k| 4 * k).collect();
        w[250] = -1;
        ints(&w);
        // O11: random values fall back to literals without loss.
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let r: Vec<i64> = (0..300)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as i64 >> 20
            })
            .collect();
        ints(&r);
    }

    #[test]
    fn labels() {
        let names: Vec<String> = (0..1024).map(|k| format!("f{k}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let enc = encode_labels(&refs);
        assert!(enc.len() <= 10, "{}", enc.len());
        let mut budget = 1 << 20;
        assert_eq!(
            decode_labels(&mut Cursor::new(&enc), refs.len(), &mut budget).unwrap(),
            names
        );
        // Mixed: literal, numbered span, literal; and non-canonical digits
        // ("f007") stay literal.
        let mixed = [
            "idle", "tile1", "tile2", "tile3", "f007", "x-3y", "x-2y", "x-1y", "x0y",
        ];
        let enc = encode_labels(&mixed);
        let mut budget = 1 << 20;
        assert_eq!(
            decode_labels(&mut Cursor::new(&enc), mixed.len(), &mut budget).unwrap(),
            mixed
        );
    }

    #[test]
    fn hostile_columns_fail_safely() {
        // GO008: zero denominator; unknown op; spans overrunning the count.
        let bad = [1, OP_RATIO, 2, 0, 2, 0];
        assert!(decode_ints(&mut Cursor::new(&bad), 2).is_err());
        assert!(decode_ints(&mut Cursor::new(&[1, 9, 1, 0]), 1).is_err());
        assert!(decode_ints(&mut Cursor::new(&[1, OP_CONST, 5, 0]), 3).is_err());
        // GO009: overflow fails deterministically.
        let mut over = vec![1, OP_LINE, 3];
        put_varint(&mut over, zigzag(i64::MAX));
        put_varint(&mut over, zigzag(i64::MAX));
        assert!(decode_ints(&mut Cursor::new(&over), 3).is_err());
        // Expansion bomb: a NUMBERED span over many labels with a long
        // prefix is refused by the budget before materialising.
        let mut bomb = vec![1, OP_NUMBERED];
        put_varint(&mut bomb, 60_000);
        put_varint(&mut bomb, 255);
        bomb.extend(std::iter::repeat_n(b'a', 255));
        bomb.extend([0, 0, 2]);
        let mut budget = 1 << 20;
        assert!(decode_labels(&mut Cursor::new(&bomb), 60_000, &mut budget).is_err());
        // GO025: every truncation fails.
        let enc = encode_ints(&(0..50).map(|k| k * k).collect::<Vec<_>>());
        for cut in 0..enc.len() {
            assert!(
                decode_ints(&mut Cursor::new(&enc[..cut]), 50).is_err(),
                "cut at {cut}"
            );
        }
    }
}
