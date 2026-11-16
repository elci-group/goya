//! Small, deterministic implementations for two `data.*` catalog contracts.
//!
//! This module implements a bounded length-prefixed byte envelope and a
//! non-cryptographic FNV-1a checksum. It does not implement general format
//! serialization, compression, or authenticity.

use std::fmt;

const MAGIC: &[u8; 4] = b"GYDB";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 4 + 1 + 4 + 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodecError {
    PayloadTooLarge { length: usize, maximum: usize },
    EncodedSizeOverflow { payload_length: usize },
    AllocationFailed { encoded_size: usize },
    TruncatedHeader,
    InvalidMagic,
    UnsupportedVersion(u8),
    LengthMismatch { declared: usize, actual: usize },
    ChecksumMismatch { expected: u64, actual: u64 },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge { length, maximum } => {
                write!(f, "payload length {length} exceeds maximum {maximum}")
            }
            Self::EncodedSizeOverflow { payload_length } => write!(
                f,
                "encoded size overflows usize for payload length {payload_length}"
            ),
            Self::AllocationFailed { encoded_size } => {
                write!(
                    f,
                    "unable to reserve {encoded_size} bytes for encoded payload"
                )
            }
            Self::TruncatedHeader => f.write_str("input is shorter than the envelope header"),
            Self::InvalidMagic => f.write_str("invalid envelope magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported envelope version {v}"),
            Self::LengthMismatch { declared, actual } => {
                write!(
                    f,
                    "declared payload length {declared} differs from actual {actual}"
                )
            }
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "payload checksum mismatch: expected {expected:016x}, got {actual:016x}"
            ),
        }
    }
}

impl std::error::Error for CodecError {}

/// Compute 64-bit FNV-1a. This detects accidental changes; it is not
/// collision-resistant and must not be used for authentication.
pub fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

/// Encode bytes in a versioned envelope with a bounded u32 length and checksum.
/// The caller supplies a limit so a later decoder can apply the same policy.
pub fn encode(payload: &[u8], maximum: usize) -> Result<Vec<u8>, CodecError> {
    if payload.len() > maximum || payload.len() > u32::MAX as usize {
        return Err(CodecError::PayloadTooLarge {
            length: payload.len(),
            maximum: maximum.min(u32::MAX as usize),
        });
    }
    let capacity = checked_encoded_len(payload.len())?;
    let mut out = reserve_encoded(capacity)?;
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&checksum(payload).to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

fn reserve_encoded(encoded_size: usize) -> Result<Vec<u8>, CodecError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(encoded_size)
        .map_err(|_| CodecError::AllocationFailed { encoded_size })?;
    Ok(output)
}

fn checked_encoded_len(payload_length: usize) -> Result<usize, CodecError> {
    HEADER_LEN
        .checked_add(payload_length)
        .ok_or(CodecError::EncodedSizeOverflow { payload_length })
}

/// Validate and borrow the payload from an encoded envelope without copying.
pub fn decode<'a>(encoded: &'a [u8], maximum: usize) -> Result<&'a [u8], CodecError> {
    if encoded.len() < HEADER_LEN {
        return Err(CodecError::TruncatedHeader);
    }
    if &encoded[..4] != MAGIC {
        return Err(CodecError::InvalidMagic);
    }
    if encoded[4] != VERSION {
        return Err(CodecError::UnsupportedVersion(encoded[4]));
    }
    let declared =
        u32::from_be_bytes(encoded[5..9].try_into().expect("fixed header slice")) as usize;
    if declared > maximum {
        return Err(CodecError::PayloadTooLarge {
            length: declared,
            maximum,
        });
    }
    let actual_len = encoded.len() - HEADER_LEN;
    if declared != actual_len {
        return Err(CodecError::LengthMismatch {
            declared,
            actual: actual_len,
        });
    }
    let expected = u64::from_be_bytes(encoded[9..17].try_into().expect("fixed header slice"));
    let payload = &encoded[HEADER_LEN..];
    let actual = checksum(payload);
    if expected != actual {
        return Err(CodecError::ChecksumMismatch { expected, actual });
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_fnv1a_64_known_vectors() {
        assert_eq!(checksum(b""), 0xcbf29ce484222325);
        assert_eq!(checksum(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(checksum(b"hello"), 0xa430d84680aabd0b);
    }

    #[test]
    fn encoded_size_calculation_rejects_usize_overflow() {
        assert_eq!(checked_encoded_len(0).unwrap(), HEADER_LEN);
        assert_eq!(
            checked_encoded_len(usize::MAX),
            Err(CodecError::EncodedSizeOverflow {
                payload_length: usize::MAX
            })
        );
    }

    #[test]
    fn failed_capacity_reservation_is_reported_instead_of_panicking() {
        assert_eq!(
            reserve_encoded(usize::MAX),
            Err(CodecError::AllocationFailed {
                encoded_size: usize::MAX
            })
        );
    }

    #[test]
    fn binary_envelope_round_trips_arbitrary_bytes_and_empty_payload() {
        for payload in [&b""[..], &[0, 255, 1, 0, 128][..], b"text"] {
            let encoded = encode(payload, 64).unwrap();
            assert_eq!(decode(&encoded, 64).unwrap(), payload);
        }
    }

    #[test]
    fn rejects_invalid_or_oversized_inputs_before_returning_payload() {
        assert_eq!(decode(b"short", 10), Err(CodecError::TruncatedHeader));
        let mut encoded = encode(b"abc", 10).unwrap();
        encoded[0] ^= 1;
        assert_eq!(decode(&encoded, 10), Err(CodecError::InvalidMagic));
        encoded[0] ^= 1;
        encoded[4] = 2;
        assert_eq!(decode(&encoded, 10), Err(CodecError::UnsupportedVersion(2)));
        encoded[4] = VERSION;
        assert!(matches!(
            decode(&encoded, 2),
            Err(CodecError::PayloadTooLarge {
                length: 3,
                maximum: 2
            })
        ));
    }

    #[test]
    fn detects_trailing_truncated_and_corrupted_payloads() {
        let encoded = encode(b"abc", 10).unwrap();
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(matches!(
            decode(&trailing, 10),
            Err(CodecError::LengthMismatch {
                declared: 3,
                actual: 4
            })
        ));
        assert!(matches!(
            decode(&encoded[..encoded.len() - 1], 10),
            Err(CodecError::LengthMismatch {
                declared: 3,
                actual: 2
            })
        ));
        let mut corrupted = encoded;
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(matches!(
            decode(&corrupted, 10),
            Err(CodecError::ChecksumMismatch { .. })
        ));
    }

    /// Run explicitly with `cargo test -p goya-caprichios --lib data_runtime::tests::benchmark_envelope -- --ignored --nocapture`.
    #[test]
    #[ignore = "manual microbenchmark; run explicitly when measuring this implementation"]
    fn benchmark_envelope() {
        use std::time::Instant;
        let payload = vec![0x5a; 1024 * 1024];
        let start = Instant::now();
        let iterations = 100;
        for _ in 0..iterations {
            let encoded = encode(&payload, payload.len()).unwrap();
            assert_eq!(
                decode(&encoded, payload.len()).unwrap().len(),
                payload.len()
            );
        }
        eprintln!(
            "{} encode+decode iterations of 1 MiB in {:?}",
            iterations,
            start.elapsed()
        );
    }
}
