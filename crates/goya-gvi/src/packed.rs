//! Bit-packed columns with O(1) random access (gvi.md §4, `PROJ`).
//!
//! A column of integers is stored in one of two forms, whichever is
//! smaller (ties go to frame-of-reference):
//!
//! - **Frame of reference:** `value = base + packed`. `packed` uses the
//!   fewest bits that hold `max − min`, so a constant column takes 0 bits.
//! - **Dictionary:** an ascending table of the distinct values, plus each
//!   row's index packed in the fewest bits that hold `entries − 1`.
//!
//! Column layout: `u8 mode (0 frame of reference, 1 dictionary)`,
//! `u8 width (0–32 bits)`, `u16 entries` (0 for frame of reference),
//! 4 zero bytes, `i64 base` (0 for dictionary), then `entries × i64`, then
//! `ceil(len · width / 8)` bytes of LSB-first packed values, with zero
//! padding bits.
//!
//! The encoding is canonical: a reader re-encodes every column it loads and
//! rejects any byte difference. Reading one value costs a shift, a mask
//! and at most one table index, with no decoding pass.

use crate::model::Reader;
use goya_graphics_ir::{codes, GfxDiag};

const MODE_FOR: u8 = 0;
const MODE_DICT: u8 = 1;
/// GVI-O: the column is operative spans (`operative.rs`), decoded once at
/// parse. Allowed only when the section opts in.
const MODE_OPERATIVE: u8 = 2;
const HEADER: usize = 16;
const MAX_WIDTH: u32 = 32;
const MAX_ENTRIES: usize = u16::MAX as usize;

fn bits(x: u64) -> u32 {
    64 - x.leading_zeros()
}

fn pack(out: &mut Vec<u8>, codes: impl Iterator<Item = u64>, n: usize, width: u32) {
    let start = out.len();
    out.resize(start + (n * width as usize).div_ceil(8), 0);
    if width == 0 {
        return;
    }
    for (i, v) in codes.enumerate() {
        let bit = i * width as usize;
        for b in 0..width as usize {
            if (v >> b) & 1 == 1 {
                let at = bit + b;
                out[start + at / 8] |= 1 << (at % 8);
            }
        }
    }
}

/// Encode `values` canonically. Callers keep each column within a 2^32
/// range (u32 ids, i32 coordinates, u8 opacity), so widths never exceed 32.
pub(crate) fn encode_column(values: &[i64]) -> Vec<u8> {
    encode_column_with(values, false)
}

/// As [`encode_column`], but with `operative` also considering operative
/// spans (GVI-O), which are used if strictly smallest.
pub(crate) fn encode_column_with(values: &[i64], operative: bool) -> Vec<u8> {
    let packed = encode_packed(values);
    if operative {
        let spans = crate::operative::encode_ints(values);
        if HEADER + spans.len() < packed.len() {
            let mut out = vec![MODE_OPERATIVE, 0, 0, 0];
            out.extend_from_slice(&[0u8; 12]);
            out.extend_from_slice(&spans);
            return out;
        }
    }
    packed
}

fn encode_packed(values: &[i64]) -> Vec<u8> {
    let n = values.len();
    let (min, max) = values
        .iter()
        .fold((i64::MAX, i64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let (min, max) = if n == 0 { (0, 0) } else { (min, max) };
    let for_width = bits((i128::from(max) - i128::from(min)) as u64);
    let for_size = HEADER + (n * for_width as usize).div_ceil(8);
    let mut dict: Vec<i64> = values.to_vec();
    dict.sort_unstable();
    dict.dedup();
    let dict_width = bits(dict.len().saturating_sub(1) as u64);
    let dict_size = HEADER + 8 * dict.len() + (n * dict_width as usize).div_ceil(8);
    let use_dict = n > 0 && dict.len() <= MAX_ENTRIES && dict_size < for_size;

    let mut out = Vec::with_capacity(for_size.min(dict_size));
    if use_dict {
        out.extend_from_slice(&[MODE_DICT, dict_width as u8]);
        out.extend_from_slice(&(dict.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&0i64.to_le_bytes());
        for v in &dict {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let idx = values
            .iter()
            .map(|v| dict.binary_search(v).unwrap_or(0) as u64);
        pack(&mut out, idx, n, dict_width);
    } else {
        out.extend_from_slice(&[MODE_FOR, for_width as u8, 0, 0]);
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&min.to_le_bytes());
        let offs = values
            .iter()
            .map(|&v| (i128::from(v) - i128::from(min)) as u64);
        pack(&mut out, offs, n, for_width);
    }
    out
}

/// A validated column over borrowed bytes.
#[derive(Debug, Clone)]
pub(crate) struct Column<'a> {
    len: usize,
    width: u32,
    mask: u64,
    base: i64,
    dict: Vec<i64>,
    bits: &'a [u8],
    /// Values of an operative column, decoded at parse.
    decoded: Option<Vec<i64>>,
}

fn bad(detail: &str) -> GfxDiag {
    GfxDiag::new(codes::MALFORMED_SECTION, format!("PROJ column: {detail}"))
}

impl<'a> Column<'a> {
    /// Parse a column of `len` values and check that it is canonical.
    pub(crate) fn parse(r: &mut Reader<'a>, len: usize) -> Result<Column<'a>, GfxDiag> {
        Column::parse_with(r, len, false)
    }

    /// Parse, also accepting operative columns when `operative` is set.
    pub(crate) fn parse_with(
        r: &mut Reader<'a>,
        len: usize,
        operative: bool,
    ) -> Result<Column<'a>, GfxDiag> {
        let mark = r.mark();
        let mode = r.u8()?;
        let width = u32::from(r.u8()?);
        let entries = usize::from(r.u16()?);
        r.zeros(4)?;
        let base = i64::from_le_bytes(r.bytes(8)?.try_into().map_err(|_| bad("truncated"))?);
        if width > MAX_WIDTH {
            return Err(bad("width above 32 bits"));
        }
        let mut dict = Vec::new();
        if mode == MODE_OPERATIVE {
            if !operative || width != 0 || entries != 0 || base != 0 {
                return Err(bad(
                    "operative column where none is allowed, or inconsistent header",
                ));
            }
            let rest = r.rest();
            let mut c = crate::operative::Cursor::new(rest);
            let values = crate::operative::decode_ints(&mut c, len)?;
            r.bytes(c.pos)?;
            if encode_column_with(&values, true) != r.since(mark) {
                return Err(GfxDiag::new(
                    codes::NON_CANONICAL,
                    "PROJ column is not canonically encoded",
                ));
            }
            return Ok(Column {
                len,
                width: 0,
                mask: 0,
                base: 0,
                dict,
                bits: &[],
                decoded: Some(values),
            });
        }
        match mode {
            MODE_FOR if entries == 0 => {}
            MODE_DICT if entries > 0 && base == 0 => {
                let raw = r.bytes(entries * 8)?;
                dict = raw
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
                    .collect();
            }
            _ => return Err(bad("unknown mode or inconsistent header")),
        }
        let nbytes = len
            .checked_mul(width as usize)
            .map(|b| b.div_ceil(8))
            .ok_or_else(|| bad("length overflows"))?;
        let bits = r.bytes(nbytes)?;
        let col = Column {
            len,
            width,
            mask: if width == 0 { 0 } else { (1u64 << width) - 1 },
            base,
            dict,
            bits,
            decoded: None,
        };
        // Canonical form: every index valid, and re-encoding reproduces the
        // bytes exactly (minimal width, ascending dictionary, the smaller
        // form, zero padding).
        let mut values = Vec::with_capacity(len);
        for i in 0..len {
            let raw = col.raw(i);
            values.push(if col.dict.is_empty() {
                base.checked_add(raw as i64)
                    .ok_or_else(|| bad("value overflows"))?
            } else {
                *col.dict
                    .get(raw as usize)
                    .ok_or_else(|| bad("dictionary index out of range"))?
            });
        }
        if encode_column_with(&values, operative) != r.since(mark) {
            return Err(GfxDiag::new(
                codes::NON_CANONICAL,
                "PROJ column is not canonically encoded",
            ));
        }
        Ok(col)
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    fn raw(&self, i: usize) -> u64 {
        if self.width == 0 {
            return 0;
        }
        let bit = i * self.width as usize;
        let (byte, shift) = (bit / 8, bit % 8);
        // Fast path: one unaligned 8-byte load. Width ≤ 32 and shift ≤ 7,
        // so the value always lies inside it.
        if let Some(chunk) = self.bits.get(byte..byte + 8) {
            let mut w = [0u8; 8];
            w.copy_from_slice(chunk);
            return (u64::from_le_bytes(w) >> shift) & self.mask;
        }
        let mut word = 0u64;
        for k in 0..5 {
            word |= u64::from(*self.bits.get(byte + k).unwrap_or(&0)) << (8 * k);
        }
        (word >> shift) & self.mask
    }

    /// Every value, in order (for expanded residency, gvi.md §9.1).
    pub(crate) fn values(&self) -> Vec<i64> {
        (0..self.len).map(|i| self.at(i)).collect()
    }

    /// Value `i` for any column kind (not the hot path).
    pub(crate) fn at(&self, i: usize) -> i64 {
        match &self.decoded {
            Some(v) => v[i],
            None => self.get(i),
        }
    }

    /// Whether this column was stored as GVI-O operative spans.
    pub(crate) fn is_operative(&self) -> bool {
        self.decoded.is_some()
    }

    /// Heap bytes this column holds beyond the file bytes it borrows.
    pub(crate) fn heap_bytes(&self) -> usize {
        8 * self.dict.len() + 8 * self.decoded.as_ref().map_or(0, Vec::len)
    }

    /// Bytes of the packed representation borrowed from the file.
    pub(crate) fn packed_bytes(&self) -> usize {
        self.bits.len()
    }

    /// Value `i`. The column was validated at parse, so this cannot fail.
    #[inline]
    pub(crate) fn get(&self, i: usize) -> i64 {
        // Hot path: packed columns only. Operative columns are read through
        // `at` (validation and expansion); inventories holding them are
        // always expanded, so they never reach here.
        debug_assert!(
            self.decoded.is_none(),
            "operative column read on the packed path"
        );
        let raw = self.raw(i);
        if self.dict.is_empty() {
            self.base + raw as i64
        } else {
            self.dict[raw as usize]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(values: &[i64]) -> usize {
        let enc = encode_column(values);
        let mut r = Reader::new(&enc, "test");
        let col = Column::parse(&mut r, values.len()).unwrap();
        for (i, &v) in values.iter().enumerate() {
            assert_eq!(col.get(i), v, "value {i}");
        }
        assert_eq!(col.values(), values);
        enc.len()
    }

    #[test]
    fn forms_and_sizes() {
        // Constant column: header only.
        assert_eq!(round_trip(&[7; 1000]), HEADER);
        // Small range: frame of reference, 3 bits each.
        assert_eq!(
            round_trip(&(0..800).map(|i| 100 + i % 8).collect::<Vec<_>>()),
            HEADER + 300
        );
        // Three far-apart values: dictionary, 2 bits each.
        let far: Vec<i64> = (0..1000)
            .map(|i| [0, 98_304, -2_000_000_000][i % 3])
            .collect();
        assert_eq!(round_trip(&far), HEADER + 24 + 250);
        // Full i32 range.
        round_trip(&[i64::from(i32::MIN), i64::from(i32::MAX), 0, -1]);
        round_trip(&[]);
        round_trip(&[u32::MAX as i64, 0]);
    }

    #[test]
    fn non_canonical_columns_are_rejected() {
        let enc = encode_column(&[1, 2, 3]);
        // Wider than necessary.
        let mut wide = enc.clone();
        wide[1] = 8;
        wide.truncate(HEADER);
        wide.extend_from_slice(&[0, 1, 2]);
        let mut r = Reader::new(&wide, "t");
        assert!(Column::parse(&mut r, 3).is_err());
        // Non-zero padding bit.
        let mut pad = enc.clone();
        *pad.last_mut().unwrap() |= 0x80;
        let mut r = Reader::new(&pad, "t");
        assert!(Column::parse(&mut r, 3).is_err());
        // Dictionary index out of range.
        let far: Vec<i64> = (0..20).map(|i| [0, 1 << 30, -(1 << 30)][i % 3]).collect();
        let mut d = encode_column(&far);
        assert_eq!(d[0], MODE_DICT);
        // First index (2 bits, right after the 3-entry table) becomes 3.
        d[HEADER + 24] |= 0b11;
        let mut r = Reader::new(&d, "t");
        assert!(Column::parse(&mut r, 20).is_err());
        // Truncation.
        for n in 0..enc.len() {
            let mut r = Reader::new(&enc[..n], "t");
            assert!(Column::parse(&mut r, 3).is_err(), "cut at {n}");
        }
    }
}
