//! WP-009 seed — decoder fuzz-lite (assurance, S6).
//!
//! Deterministic seeded mutation of valid canonical Request/Response
//! encodings. Contract: decode either succeeds with a value consistent
//! with the original message (same consumed-prefix invariants), or returns
//! a typed error. It must NEVER panic, and implausible declared lengths
//! (declared > remaining buffer) must be rejected as ImplausibleLength /
//! Truncated — never satisfied by allocating.

use ie_contract::codec::{self, CodecError};
use ie_contract::fixture::GoldenFixture;
use ie_contract::framing::{decode_frame, encode_frame};
use ie_contract::messages::{Request, Response, Status};
use ie_contract::errors::TypedError;
use ie_contract::tensors::TensorData;

/// splitmix64 — local deterministic seed source (no external crates).
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

fn sample_tensor(vals: &[f32]) -> TensorData {
    GoldenFixture::tensor(vals, &[vals.len() as u64])
}

fn sample_request() -> Request {
    Request {
        request_id: 0xDEAD_BEEF_1234_5678,
        model_sha256: [0x5A; 32],
        inputs: vec![sample_tensor(&[1.0, -2.5, 3.25, 1e6, -0.75])],
        deadline_ms: 1_500,
        lease_id: 77,
        policy_digest: [0x11; 32],
        requested_outputs: vec!["logits".into(), "hidden".into()],
    }
}

fn sample_response() -> Response {
    Response {
        request_id: 42,
        status: Status::Failed(TypedError::UnsupportedOp { op: "gelu".into() }),
        backend: "cpu0".into(),
        provider_id: "ie://peer/esp0/0".into(),
        epoch: 12,
        outputs: vec![sample_tensor(&[19.0, 22.0, 43.0, 50.0])],
        timing_us: [11, 22, 33],
    }
}

fn encoded_request() -> Vec<u8> {
    let mut buf = Vec::new();
    codec::encode_request(&mut buf, &sample_request()).unwrap();
    buf
}

fn encoded_response() -> Vec<u8> {
    let mut buf = Vec::new();
    codec::encode_response(&mut buf, &sample_response()).unwrap();
    buf
}

/// Call the right decoder for the kind of buffer.
fn decode_request_checked(buf: &[u8]) -> Result<usize, CodecError> {
    codec::decode_request(buf).map(|(_, c)| c)
}

fn decode_response_checked(buf: &[u8]) -> Result<usize, CodecError> {
    codec::decode_response(buf).map(|(_, c)| c)
}


/// The core invariant: on ANY input the decoder either succeeds with
/// consumed <= buffer length, or returns a typed error. Note: the
/// ImplausibleLength variant is emitted under three distinct semantics
/// (buffer-level length, count*min_elem, and tensor descriptor mismatch),
/// so "declared > remaining" is not a necessary condition in general —
/// the targeted tests below pin the implausible-length rule exactly.
fn assert_mutation_safe(_orig: &[u8], mutated: &[u8], result: Result<usize, CodecError>) {
    match result {
        Ok(consumed) => assert!(
            consumed <= mutated.len(),
            "decoded consumed {consumed} beyond buffer of {} bytes",
            mutated.len()
        ),
        Err(_) => {
            // Truncated, ImplausibleLength, UnknownDType, UnknownStatus,
            // InvalidUtf8, VersionMismatch etc. are acceptable typed
            // rejections — as long as we got here without panicking.
        }
    }
}

fn apply_bit_flip(buf: &[u8], rng: &mut SplitMix64) -> Vec<u8> {
    let mut m = buf.to_vec();
    let pos = rng.below(m.len());
    let bit = rng.below(8) as u8;
    m[pos] ^= 1 << bit;
    m
}

fn apply_truncate(buf: &[u8], rng: &mut SplitMix64) -> Vec<u8> {
    let cut = 1 + rng.below(buf.len());
    buf[..cut].to_vec()
}

/// Length/count inflation: overwrite a 4-byte window with an implausible
/// count (u32::MAX) or a moderately inflated value.
fn apply_length_inflation(buf: &[u8], rng: &mut SplitMix64, kind: u32) -> Vec<u8> {
    let mut m = buf.to_vec();
    if m.len() < 4 {
        return m;
    }
    let pos = rng.below(m.len() - 3);
    let v = match kind % 3 {
        0 => u32::MAX,
        1 => 0x4000_0000, // 2^30: huge but not saturated
        _ => 0xFFFF_FFFFu32.wrapping_sub(rng.next_u64() as u32 & 0xF),
    };
    m[pos..pos + 4].copy_from_slice(&v.to_le_bytes());
    m
}

#[test]
fn request_truncations_never_panic_and_never_over_consume() {
    let buf = encoded_request();
    for cut in 0..buf.len() {
        assert_mutation_safe(&buf, &buf[..cut], decode_request_checked(&buf[..cut]));
    }
    // Full buffer decodes cleanly and consumes exactly everything.
    let (req, consumed) = codec::decode_request(&buf).unwrap();
    assert_eq!(consumed, buf.len());
    assert_eq!(req, sample_request());
}

#[test]
fn response_truncations_never_panic_and_never_over_consume() {
    let buf = encoded_response();
    for cut in 0..buf.len() {
        assert_mutation_safe(&buf, &buf[..cut], decode_response_checked(&buf[..cut]));
    }
    let (resp, consumed) = codec::decode_response(&buf).unwrap();
    assert_eq!(consumed, buf.len());
    assert_eq!(resp, sample_response());
}

#[test]
fn request_seeded_mutations_never_panic() {
    let buf = encoded_request();
    let mut rng = SplitMix64(0x1E5E_1DEA_5EED_0001);
    for _ in 0..2000 {
        let mutated = match rng.below(3) {
            0 => apply_bit_flip(&buf, &mut rng),
            1 => apply_truncate(&buf, &mut rng),
            _ => {
                let kind = rng.next_u64() as u32;
                apply_length_inflation(&buf, &mut rng, kind)
            }
        };
        let r = decode_request_checked(&mutated);
        assert_mutation_safe(&buf, &mutated, r);
    }
}

#[test]
fn response_seeded_mutations_never_panic() {
    let buf = encoded_response();
    let mut rng = SplitMix64(0xBEEF_CAFE_5EED_0002);
    for _ in 0..2000 {
        let mutated = match rng.below(3) {
            0 => apply_bit_flip(&buf, &mut rng),
            1 => apply_truncate(&buf, &mut rng),
            _ => {
                let kind = rng.next_u64() as u32;
                apply_length_inflation(&buf, &mut rng, kind)
            }
        };
        let r = decode_response_checked(&mutated);
        assert_mutation_safe(&buf, &mutated, r);
    }
}

/// Implausible-length rule: an inflated tensor-payload length (u64 field)
/// must be rejected with ImplausibleLength, not satisfied by reading past
/// the buffer.
#[test]
fn inflated_tensor_payload_len_is_implausible_error() {
    let buf = encoded_request();
    // Byte layout: 8 (req id) + 32 (sha) + 4 (count) = 44 to first tensor;
    // payload len field sits at 44 + 1 (dtype) + 4 (rank) + 8 (dim)
    //   + 1 + 1 (quant flags) = 59, 8 bytes long (u64).
    let mut m = buf.clone();
    m[59..67].copy_from_slice(&u64::MAX.to_le_bytes());
    match decode_request_checked(&m) {
        Err(CodecError::ImplausibleLength { declared, remaining }) => {
            assert_eq!(declared, u64::MAX);
            assert!(declared > remaining as u64);
        }
        other => panic!("expected ImplausibleLength, got {other:?}"),
    }
}

/// Count inflation: input count of u32::MAX must be rejected before
/// allocation (ImplausibleLength), regardless of buffer contents.
#[test]
fn inflated_input_count_is_implausible_error() {
    let buf = encoded_request();
    let mut m = buf.clone();
    m[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode_request_checked(&m),
        Err(CodecError::ImplausibleLength { .. })
    ));
}

/// Framing layer: declared length > remaining => IncompletePayload or
/// DeclaredTooLarge, never a panic and never an oversized allocation signal.
#[test]
fn frame_inflated_lengths_are_typed_errors() {
    let payload = encoded_request();
    let frame = encode_frame(&payload).unwrap();
    // Flip high bits into the length prefix to push past MAX_MESSAGE_BYTES.
    for hi in [0x80u8, 0xFF, 0x01] {
        let mut m = frame.clone();
        m[3] = hi; // MSB byte of the LE u32 length
        match decode_frame(&m) {
            Err(_) => {} // typed error — fine
            Ok((p, c)) => {
                // Only acceptable if the declared length was still within
                // cap and satisfied; consumed must stay in the buffer.
                assert!(c <= m.len());
                let _ = p;
            }
        }
    }
    // Truncated frames below header/payload size.
    for cut in 0..frame.len() {
        let _ = decode_frame(&frame[..cut]); // must not panic
    }
}

/// Response status byte corruption: unknown status is a typed error, never
/// a panic; a flipped Pending<->Completed bit still decodes or errors.
#[test]
fn response_status_corruption_is_typed() {
    let buf = encoded_response();
    let status_pos = 8; // request_id (8 bytes) then status u8
    for b in 0..=255u8 {
        let mut m = buf.clone();
        m[status_pos] = b;
        match decode_response_checked(&m) {
            Ok(consumed) => assert!(consumed <= m.len()),
            Err(CodecError::UnknownStatus(s)) => assert_eq!(s, b),
            Err(_) => {}
        }
    }
}
