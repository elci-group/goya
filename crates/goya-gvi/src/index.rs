//! `INDX` encoding (gvi.md §4.2): the reachable state keys, and each
//! state's projection id, in the smallest exact form that still gives
//! constant-time or logarithmic lookup.
//!
//! Keys use the first form that applies, otherwise the smaller of the
//! other two:
//!
//! | Form | Stored | `index_of(key)` |
//! |---|---|---|
//! | identity | nothing (keys are exactly `0..n`) | `key < n` → `key` |
//! | bitmap | one bit per domain key, plus a packed rank sample per 512-bit block | bit test, then sample + at most 8 popcounts |
//! | packed | the sorted keys as a packed column (§4.1) | binary search |
//!
//! Projection ids are omitted when state `i` uses projection `i`, and are
//! otherwise a packed column. The encoding is canonical: a reader decodes
//! the keys and ids, re-encodes them, and rejects any byte difference.

use crate::model::Reader;
use crate::packed::{encode_column, Column};
use goya_graphics_ir::{codes, GfxDiag};

const FORM_IDENTITY: u32 = 0;
const FORM_BITMAP: u32 = 1;
const FORM_PACKED: u32 = 2;
const FORM_MASK: u32 = 3;
const PROJ_IDENTITY: u32 = 4;
const BLOCK_BITS: u64 = 512;
/// Largest domain a bitmap may cover (2^32 bits = 512 MiB). Larger domains
/// use packed keys.
const MAX_BITMAP_DOMAIN: u64 = 1 << 32;

fn bad(detail: &str) -> GfxDiag {
    GfxDiag::new(codes::MALFORMED_SECTION, format!("INDX: {detail}"))
}

fn bitmap_parts(keys: &[u64], domain: u64) -> (Vec<u8>, Vec<u8>) {
    let words = domain.div_ceil(64) as usize;
    let mut bits = vec![0u64; words];
    for &k in keys {
        bits[(k / 64) as usize] |= 1 << (k % 64);
    }
    let blocks = domain.div_ceil(BLOCK_BITS) as usize;
    let mut samples = Vec::with_capacity(blocks);
    let mut rank = 0i64;
    for b in 0..blocks {
        samples.push(rank);
        for w in &bits[b * 8..((b + 1) * 8).min(words)] {
            rank += i64::from(w.count_ones());
        }
    }
    let mut raw = Vec::with_capacity(words * 8);
    for w in bits {
        raw.extend_from_slice(&w.to_le_bytes());
    }
    (encode_column(&samples), raw)
}

/// Encode `INDX` for strictly ascending `keys` inside `domain`, with each
/// state's projection id.
pub(crate) fn encode_indx(keys: &[u64], proj: &[u32], domain: u64) -> Vec<u8> {
    let n = keys.len();
    let identity = keys.iter().enumerate().all(|(i, &k)| k == i as u64);
    let packed = encode_column(&keys.iter().map(|&k| k as i64).collect::<Vec<_>>());
    let (form, payload) = if identity {
        (FORM_IDENTITY, Vec::new())
    } else if domain <= MAX_BITMAP_DOMAIN && domain.div_ceil(8) < packed.len() as u64 {
        // Only build the bitmap when it could win; it is at least
        // domain / 8 bytes.
        let (samples, raw) = bitmap_parts(keys, domain);
        if samples.len() + raw.len() < packed.len() {
            let mut p = samples;
            p.extend_from_slice(&raw);
            (FORM_BITMAP, p)
        } else {
            (FORM_PACKED, packed)
        }
    } else {
        (FORM_PACKED, packed)
    };
    let proj_identity = proj.iter().enumerate().all(|(i, &p)| p == i as u32);
    let mut out = Vec::new();
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(form | if proj_identity { PROJ_IDENTITY } else { 0 }).to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&payload);
    if !proj_identity {
        out.extend_from_slice(&encode_column(
            &proj.iter().map(|&p| i64::from(p)).collect::<Vec<_>>(),
        ));
    }
    while !out.len().is_multiple_of(16) {
        out.push(0);
    }
    out
}

#[derive(Debug, Clone)]
enum Keys<'a> {
    Identity,
    Bitmap {
        samples: Column<'a>,
        words: &'a [u8],
        domain: u64,
    },
    Packed(Column<'a>),
}

/// A validated `INDX`.
#[derive(Debug, Clone)]
pub(crate) struct Index<'a> {
    n: usize,
    keys: Keys<'a>,
    /// `None` when state `i` uses projection `i`.
    proj: Option<Column<'a>>,
}

fn word(words: &[u8], w: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&words[w * 8..w * 8 + 8]);
    u64::from_le_bytes(b)
}

impl<'a> Index<'a> {
    /// Parse and validate. Also returns the decoded keys and projection
    /// ids, which open uses for its certificate and completeness checks.
    #[allow(clippy::type_complexity)]
    pub(crate) fn parse(
        buf: &'a [u8],
        domain: u64,
        max_states: u32,
    ) -> Result<(Index<'a>, Vec<u64>, Vec<u32>), GfxDiag> {
        let mut r = Reader::new(buf, "INDX");
        let n = r.u32()?;
        let flags = r.u32()?;
        r.zeros(8)?;
        if n > max_states {
            return Err(GfxDiag::new(
                codes::GVI_NOT_MANAGEABLE,
                format!("{n} states exceeds the limit of {max_states}"),
            ));
        }
        if flags & !(FORM_MASK | PROJ_IDENTITY) != 0 {
            return Err(bad("unknown flags"));
        }
        let n = n as usize;
        let (keys, decoded): (Keys<'a>, Vec<u64>) = match flags & FORM_MASK {
            FORM_IDENTITY => (Keys::Identity, (0..n as u64).collect()),
            FORM_BITMAP => {
                if domain > MAX_BITMAP_DOMAIN {
                    return Err(bad("bitmap over a domain that is too large"));
                }
                let samples = Column::parse(&mut r, domain.div_ceil(BLOCK_BITS) as usize)?;
                let words = r.bytes(domain.div_ceil(64) as usize * 8)?;
                let mut decoded = Vec::with_capacity(n);
                for w in 0..words.len() / 8 {
                    let mut x = word(words, w);
                    while x != 0 {
                        if decoded.len() == n {
                            return Err(bad("bitmap holds more keys than declared"));
                        }
                        decoded.push(w as u64 * 64 + u64::from(x.trailing_zeros()));
                        x &= x - 1;
                    }
                }
                (
                    Keys::Bitmap {
                        samples,
                        words,
                        domain,
                    },
                    decoded,
                )
            }
            FORM_PACKED => {
                let col = Column::parse(&mut r, n)?;
                let decoded = (0..n).map(|i| col.get(i) as u64).collect();
                (Keys::Packed(col), decoded)
            }
            _ => return Err(bad("unknown key form")),
        };
        let (proj, ids): (Option<Column<'a>>, Vec<u32>) = if flags & PROJ_IDENTITY != 0 {
            (None, (0..n as u32).collect())
        } else {
            let col = Column::parse(&mut r, n)?;
            let mut ids = Vec::with_capacity(n);
            for i in 0..n {
                ids.push(u32::try_from(col.get(i)).map_err(|_| bad("projection id out of range"))?);
            }
            (Some(col), ids)
        };
        r.finish()?;
        if decoded.len() != n {
            return Err(bad("key count disagrees with the header"));
        }
        if decoded.windows(2).any(|w| w[0] >= w[1]) || decoded.last().is_some_and(|&k| k >= domain)
        {
            return Err(GfxDiag::new(
                codes::GVI_CERTIFICATE_INVALID,
                "index keys must be strictly ascending and inside the domain",
            ));
        }
        if encode_indx(&decoded, &ids, domain) != buf {
            return Err(GfxDiag::new(
                codes::NON_CANONICAL,
                "INDX is not canonically encoded",
            ));
        }
        Ok((Index { n, keys, proj }, decoded, ids))
    }

    pub(crate) fn len(&self) -> usize {
        self.n
    }

    /// The sorted keys, if they are stored as a packed list.
    pub(crate) fn packed_keys(&self) -> Option<Vec<u64>> {
        match &self.keys {
            Keys::Packed(c) => Some(c.values().into_iter().map(|k| k as u64).collect()),
            _ => None,
        }
    }

    /// The projection ids, unless they are the identity.
    pub(crate) fn proj_ids(&self) -> Option<Vec<u32>> {
        self.proj
            .as_ref()
            .map(|c| c.values().into_iter().map(|p| p as u32).collect())
    }

    /// Packed columns the index owns (for residency accounting).
    pub(crate) fn columns(&self) -> Vec<&Column<'a>> {
        let mut out: Vec<&Column<'a>> = Vec::new();
        match &self.keys {
            Keys::Identity => {}
            Keys::Bitmap { samples, .. } => out.push(samples),
            Keys::Packed(c) => out.push(c),
        }
        if let Some(p) = &self.proj {
            out.push(p);
        }
        out
    }

    #[inline]
    pub(crate) fn proj_of(&self, i: usize) -> u32 {
        match &self.proj {
            None => i as u32,
            Some(c) => c.get(i) as u32,
        }
    }

    /// Position of `key` among the reachable states, if it is reachable.
    #[inline]
    pub(crate) fn index_of(&self, key: u64) -> Option<usize> {
        match &self.keys {
            Keys::Identity => (key < self.n as u64).then_some(key as usize),
            Keys::Bitmap {
                samples,
                words,
                domain,
            } => {
                if key >= *domain {
                    return None;
                }
                let (w, bit) = ((key / 64) as usize, key % 64);
                let here = word(words, w);
                if (here >> bit) & 1 == 0 {
                    return None;
                }
                let block = (key / BLOCK_BITS) as usize;
                let mut rank = samples.get(block) as usize;
                for ww in block * 8..w {
                    rank += word(words, ww).count_ones() as usize;
                }
                Some(rank + (here & ((1u64 << bit) - 1)).count_ones() as usize)
            }
            Keys::Packed(col) => {
                let (mut lo, mut hi) = (0usize, self.n);
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    match (col.get(mid) as u64).cmp(&key) {
                        std::cmp::Ordering::Less => lo = mid + 1,
                        std::cmp::Ordering::Greater => hi = mid,
                        std::cmp::Ordering::Equal => return Some(mid),
                    }
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(keys: &[u64], proj: &[u32], domain: u64, form: u32) -> usize {
        let enc = encode_indx(keys, proj, domain);
        assert_eq!(
            u32::from_le_bytes(enc[4..8].try_into().unwrap()) & FORM_MASK,
            form,
            "form for {keys:?}"
        );
        let (idx, k, p) = Index::parse(&enc, domain, u32::MAX).unwrap();
        assert_eq!((k.as_slice(), p.as_slice()), (keys, proj));
        for key in 0..domain.min(5_000) {
            assert_eq!(
                idx.index_of(key),
                keys.binary_search(&key).ok(),
                "key {key}"
            );
        }
        for (i, &want) in proj.iter().enumerate() {
            assert_eq!(idx.proj_of(i), want);
        }
        enc.len()
    }

    #[test]
    fn forms() {
        // Full product: identity keys and ids, header only.
        let all: Vec<u64> = (0..192).collect();
        let ids: Vec<u32> = (0..192).collect();
        assert_eq!(check(&all, &ids, 192, FORM_IDENTITY), 16);
        // A dense set with holes (product minus exclusions): bitmap.
        let holey: Vec<u64> = (0..4_000).filter(|k| k % 7 != 3).collect();
        let p: Vec<u32> = (0..holey.len() as u32).map(|i| i % 5).collect();
        check(&holey, &p, 4_000, FORM_BITMAP);
        // A sparse set in a huge domain (transition reachability): packed.
        let sparse: Vec<u64> = (0..50).map(|i| i * 1_000_003 + 17).collect();
        let q: Vec<u32> = (0..50).collect();
        check(&sparse, &q, 1 << 40, FORM_PACKED);
        // A prefix 0..n inside a larger domain is still the identity.
        check(&[0, 1, 2], &[0, 0, 1], 10, FORM_IDENTITY);
    }

    #[test]
    fn non_canonical_and_malformed_are_rejected() {
        let holey: Vec<u64> = (0..4_000).filter(|k| k % 7 != 3).collect();
        let p: Vec<u32> = vec![0; holey.len()];
        let enc = encode_indx(&holey, &p, 4_000);
        let mut past = enc.clone();
        // Rewrite as packed form with the same keys: valid bytes, wrong form.
        let mut packed = Vec::new();
        packed.extend_from_slice(&(holey.len() as u32).to_le_bytes());
        packed.extend_from_slice(&FORM_PACKED.to_le_bytes());
        packed.extend_from_slice(&[0u8; 8]);
        packed.extend_from_slice(&encode_column(
            &holey.iter().map(|&k| k as i64).collect::<Vec<_>>(),
        ));
        packed.extend_from_slice(&encode_column(&vec![0; holey.len()]));
        while packed.len() % 16 != 0 {
            packed.push(0);
        }
        assert_eq!(
            Index::parse(&packed, 4_000, u32::MAX).unwrap_err().code,
            codes::NON_CANONICAL
        );
        // Unknown flags, and too many states.
        past[4] |= 0x40;
        assert!(Index::parse(&past, 4_000, u32::MAX).is_err());
        assert_eq!(
            Index::parse(&enc, 4_000, 10).unwrap_err().code,
            codes::GVI_NOT_MANAGEABLE
        );
        // Every truncation fails.
        for cut in 0..enc.len() {
            assert!(
                Index::parse(&enc[..cut], 4_000, u32::MAX).is_err(),
                "cut at {cut}"
            );
        }
        // Identity keys beyond the domain are rejected.
        let id = encode_indx(&[0, 1, 2], &[0, 1, 2], 3);
        assert!(Index::parse(&id, 2, u32::MAX).is_err());
    }
}
