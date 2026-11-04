//! goya-watermark — keyed mid-frequency DCT spectral watermarking (GY-WATERMARK/1).
//!
//! Std-only. Embeds a 64-bit payload into mid-frequency coefficients of 8x8
//! DCT blocks. Per block, each of three mid-frequency coefficients is pushed
//! to a target sign derived from one payload bit XORed with a keystream bit
//! keyed by a xorshift64 PRNG. Detection re-derives the keystream, votes on
//! each payload bit across blocks/coefficients, and returns `None` when the
//! agreement confidence is below threshold (e.g. an unwatermarked image).
//!
//! Contract: [`WATERMARK_CONTRACT`].

pub const WATERMARK_CONTRACT: &str = "GY-WATERMARK/1";

/// Number of payload bits carried by the watermark.
pub const PAYLOAD_BITS: usize = 64;

/// Mid-frequency coefficients (row-index, column-index) used per block.
const COEFFS: [(usize, usize); 3] = [(1, 2), (2, 1), (2, 2)];

/// Detection confidence threshold: mean per-bit majority agreement required
/// to report a payload. A watermarked image scores ~1.0; an unwatermarked
/// one scores ~0.5-0.75 (pseudorandom keystream votes).
const CONFIDENCE_THRESHOLD: f64 = 0.85;

/// xorshift64 PRNG used to derive the keystream from the key.
struct Xorshift64(u64);

impl Xorshift64 {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// splitmix64 finalizer: decorrelates a position before seeding the PRNG so
/// neighbouring keystream positions are independent.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Keystream bit for a given (block, coefficient) position. Embed and detect
/// walk positions in the same order, so the stream matches.
///
/// The key is mixed through `mix64` together with the position before seeding,
/// so two keys never produce constant-offset streams (a naive `key ^ mix(pos)`
/// seed leaves the seed difference equal to the key difference at every
/// position, and a linear PRNG then cancels it out). The output bit is the
/// parity of a later PRNG word, which decorrelates well across positions.
fn keystream_bit(key: u64, position: u64) -> bool {
    let seed = mix64(key ^ mix64(position.wrapping_add(0x9E37_79B9_7F4A_7C15))) | 1;
    let mut rng = Xorshift64(seed);
    let mut out = 0u64;
    for _ in 0..3 {
        out = rng.next_u64();
    }
    out.count_ones() % 2 == 1
}

/// Precomputed orthonormal 8-point DCT-II basis rows:
/// `basis[u][x] = c(u) * sqrt(2/8) * cos(pi * (2x + 1) * u / 16)`.
fn dct_basis() -> [[f64; 8]; 8] {
    let mut basis = [[0.0f64; 8]; 8];
    for (u, row) in basis.iter_mut().enumerate() {
        let cu = if u == 0 {
            std::f64::consts::FRAC_1_SQRT_2
        } else {
            1.0
        };
        for (x, cell) in row.iter_mut().enumerate() {
            let angle = std::f64::consts::PI * (2 * x + 1) as f64 * u as f64 / 16.0;
            *cell = cu * (2.0f64 / 8.0).sqrt() * angle.cos();
        }
    }
    basis
}

/// 2D orthonormal DCT-II of an 8x8 block (row-major, 64 f64 samples).
fn dct2d(block: &[f64; 64], basis: &[[f64; 8]; 8]) -> [f64; 64] {
    // Rows: tmp[u][x] = sum_y basis[u][y] * block[y][x].
    let mut tmp = [0.0f64; 64];
    for u in 0..8 {
        for x in 0..8 {
            let mut acc = 0.0;
            for y in 0..8 {
                acc += basis[u][y] * block[y * 8 + x];
            }
            tmp[u * 8 + x] = acc;
        }
    }
    // Columns: out[u][v] = sum_x basis[v][x] * tmp[u][x].
    let mut out = [0.0f64; 64];
    for u in 0..8 {
        for v in 0..8 {
            let mut acc = 0.0;
            for x in 0..8 {
                acc += basis[v][x] * tmp[u * 8 + x];
            }
            out[u * 8 + v] = acc;
        }
    }
    out
}

/// Inverse 2D orthonormal DCT of an 8x8 coefficient block.
fn idct2d(coeffs: &[f64; 64], basis: &[[f64; 8]; 8]) -> [f64; 64] {
    // Columns: tmp[y][v] = sum_u basis[u][y] * coeffs[u][v].
    let mut tmp = [0.0f64; 64];
    for y in 0..8 {
        for v in 0..8 {
            let mut acc = 0.0;
            for u in 0..8 {
                acc += basis[u][y] * coeffs[u * 8 + v];
            }
            tmp[y * 8 + v] = acc;
        }
    }
    // Rows: out[y][x] = sum_v basis[v][x] * tmp[y][v].
    let mut out = [0.0f64; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut acc = 0.0;
            for v in 0..8 {
                acc += basis[v][x] * tmp[y * 8 + v];
            }
            out[y * 8 + x] = acc;
        }
    }
    out
}

/// Number of complete 8x8 blocks available in the image.
fn block_count(w: u32, h: u32) -> usize {
    (w as usize / 8) * (h as usize / 8)
}

/// Extract the (bx, by)-indexed 8x8 block as f64 luma.
fn get_block(pixels: &[u8], w: u32, bx: usize, by: usize) -> [f64; 64] {
    let w = w as usize;
    let mut block = [0.0f64; 64];
    for y in 0..8 {
        for x in 0..8 {
            block[y * 8 + x] = pixels[(by * 8 + y) * w + (bx * 8 + x)] as f64;
        }
    }
    block
}

/// Write an f64 8x8 block back into the pixel buffer, clamped and rounded.
fn put_block(pixels: &mut [u8], w: u32, bx: usize, by: usize, block: &[f64; 64]) {
    let w = w as usize;
    for y in 0..8 {
        for x in 0..8 {
            let v = block[y * 8 + x].round();
            pixels[(by * 8 + y) * w + (bx * 8 + x)] = if v < 0.0 {
                0
            } else if v > 255.0 {
                255
            } else {
                v as u8
            };
        }
    }
}

/// Embed a 64-bit `payload` into `pixels` (8-bit luma, row-major, `w` x `h`).
///
/// For each complete 8x8 block `b`, the payload bit at index `b % 64` is
/// redundantly encoded into three mid-frequency coefficients: the target sign
/// of coefficient `ci` is `(bit XOR keystream(b, b*3 + ci))`, and any
/// coefficient not already at that sign with margin is moved to
/// `sign * alpha`. The delta applied to a coefficient is therefore at most
/// `alpha + |c|`; on smooth images (mid coefficients near zero) the worst-case
/// per-pixel delta is about `0.75 * alpha` (three coefficients, basis
/// amplitude <= 0.25), satisfying the bounded-distortion requirement
/// `delta <= ceil(alpha) + 2`. Remainder rows/columns are left untouched.
pub fn embed(pixels: &[u8], w: u32, h: u32, key: u64, payload: u64, alpha: f64) -> Vec<u8> {
    let mut out = pixels.to_vec();
    let n_blocks = block_count(w, h);
    if n_blocks == 0 {
        return out;
    }
    let basis = dct_basis();
    let bw = (w as usize / 8).max(1);
    for b in 0..n_blocks {
        let bx = b % bw;
        let by = b / bw;
        let bit = (payload >> (b % PAYLOAD_BITS)) & 1 == 1;
        let block = get_block(pixels, w, bx, by);
        let mut coeffs = dct2d(&block, &basis);
        let mut changed = false;
        for (ci, &(u, v)) in COEFFS.iter().enumerate() {
            let ks = keystream_bit(key, (b * COEFFS.len() + ci) as u64);
            let target_one = bit ^ ks;
            let s = if target_one { 1.0 } else { -1.0 };
            let c = coeffs[u * 8 + v];
            if s * c < alpha {
                coeffs[u * 8 + v] = s * alpha;
                changed = true;
            }
        }
        if changed {
            let watermarked_pixels = idct2d(&coeffs, &basis);
            put_block(&mut out, w, bx, by, &watermarked_pixels);
        }
    }
    out
}

/// Detect a watermark in `pixels` (8-bit luma, row-major, `w` x `h`).
///
/// Re-derives the keystream, estimates each payload bit by majority vote over
/// the three mid-frequency coefficient signs of every block whose index maps
/// to that bit, and returns the payload when mean majority agreement meets
/// [`CONFIDENCE_THRESHOLD`]. Returns `None` for unwatermarked images or when
/// no blocks are present.
pub fn detect(pixels: &[u8], w: u32, h: u32, key: u64) -> Option<u64> {
    let n_blocks = block_count(w, h);
    if n_blocks == 0 {
        return None;
    }
    let basis = dct_basis();
    let bw = (w as usize / 8).max(1);
    // votes[i] = (votes for bit==1, total votes) per payload bit.
    let mut votes = [(0u32, 0u32); PAYLOAD_BITS];
    for b in 0..n_blocks {
        let bx = b % bw;
        let by = b / bw;
        let block = get_block(pixels, w, bx, by);
        let coeffs = dct2d(&block, &basis);
        for (ci, &(u, v)) in COEFFS.iter().enumerate() {
            let ks = keystream_bit(key, (b * COEFFS.len() + ci) as u64);
            let sign_one = coeffs[u * 8 + v] > 0.0;
            let bit_est = sign_one ^ ks;
            let slot = b % PAYLOAD_BITS;
            if bit_est {
                votes[slot].0 += 1;
            }
            votes[slot].1 += 1;
        }
    }
    let mut payload = 0u64;
    let mut total_agreement = 0.0f64;
    for (i, &(ones, total)) in votes.iter().enumerate() {
        if total == 0 {
            // This bit was never encoded (too few blocks): fail closed.
            return None;
        }
        let ones = f64::from(ones);
        let total = f64::from(total);
        let bit = ones >= total / 2.0;
        if bit {
            payload |= 1u64 << i;
        }
        total_agreement += ones.max(total - ones) / total;
    }
    let confidence = total_agreement / PAYLOAD_BITS as f64;
    if confidence >= CONFIDENCE_THRESHOLD {
        Some(payload)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic 64x64 gradient test image (row-major, wrapping ramp).
    fn gradient() -> (Vec<u8>, u32, u32) {
        let w = 64;
        let h = 64;
        let pixels: Vec<u8> = (0..(w * h)).map(|i| ((i * 4) % 256) as u8).collect();
        (pixels, w, h)
    }

    fn perturb(pixels: &[u8], delta: i16) -> Vec<u8> {
        pixels
            .iter()
            .map(|&p| (i16::from(p) + delta).clamp(0, 255) as u8)
            .collect()
    }

    #[test]
    fn round_trip_recovers_payload() {
        let (pixels, w, h) = gradient();
        let key = 0xDEAD_BEEF_CAFE_F00Du64;
        let payload = 0x1234_5678_9ABC_DEF0u64;
        let marked = embed(&pixels, w, h, key, payload, 12.0);
        assert_eq!(detect(&marked, w, h, key), Some(payload));
    }

    #[test]
    fn robust_to_mild_perturbation() {
        let (pixels, w, h) = gradient();
        let key = 42u64;
        let payload = 0x0F1E_2D3C_4B5A_6978u64;
        let marked = embed(&pixels, w, h, key, payload, 16.0);
        let noisy = perturb(&marked, 2);
        assert_eq!(detect(&noisy, w, h, key), Some(payload));
    }

    #[test]
    fn clean_image_detects_none() {
        let (pixels, w, h) = gradient();
        assert_eq!(detect(&pixels, w, h, 7u64), None);
    }

    #[test]
    fn embed_is_deterministic() {
        let (pixels, w, h) = gradient();
        let a = embed(&pixels, w, h, 99u64, 0xABCDu64, 10.0);
        let b = embed(&pixels, w, h, 99u64, 0xABCDu64, 10.0);
        assert_eq!(a, b);
    }

    #[test]
    fn embedding_is_bounded_distortion() {
        let (pixels, w, h) = gradient();
        let alpha = 8.0f64;
        let marked = embed(&pixels, w, h, 5u64, 0xFFFF_FFFF_FFFF_FFFFu64, alpha);
        let bound = (alpha.ceil() as i64) + 2;
        for (orig, new) in pixels.iter().zip(marked.iter()) {
            let d = i64::from(*orig) - i64::from(*new);
            assert!(
                d.abs() <= bound,
                "pixel delta {} exceeds bound {}",
                d,
                bound
            );
        }
    }

    #[test]
    fn wrong_key_fails_detection() {
        let (pixels, w, h) = gradient();
        let payload = 0x1111_2222_3333_4444u64;
        let marked = embed(&pixels, w, h, 123u64, payload, 14.0);
        assert_ne!(detect(&marked, w, h, 456u64), Some(payload));
    }

    #[test]
    fn dct_idct_round_trip() {
        let basis = dct_basis();
        let mut block = [0.0f64; 64];
        for (i, v) in block.iter_mut().enumerate() {
            *v = ((i * 7) % 256) as f64;
        }
        let coeffs = dct2d(&block, &basis);
        let restored = idct2d(&coeffs, &basis);
        for i in 0..64 {
            assert!((block[i] - restored[i]).abs() < 1e-6, "index {} drifted", i);
        }
    }

    #[test]
    fn too_small_image_detects_none_and_embeds_nothing() {
        let pixels = vec![128u8; 7 * 7];
        assert_eq!(detect(&pixels, 7, 7, 1u64), None);
        assert_eq!(embed(&pixels, 7, 7, 1u64, 42u64, 8.0), pixels);
    }
}
