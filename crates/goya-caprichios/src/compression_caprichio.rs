//! A bounded, process-local RLE codec with a small versioned wire envelope.
//!
//! RLE reduces some repeated-byte inputs and can expand incompressible ones.
//! The codec makes no general compression-ratio or arbitrary-data shrinkage
//! claim, and it provides no checksum, authenticity, or cryptographic security.

use std::fmt;

pub const CONTRACT: &str = "goya.data.compression.rle.v1";
pub const RECIPE: &str = "rust-process-local-rle-v1";
pub const RECIPE_VERSION: &str = "0.1.0";
pub const MAX_OUTPUT_BYTES: u32 = 16 * 1024 * 1024;
pub const MAX_ENCODED_BYTES: u32 = 17 * 1024 * 1024;
const MAGIC: &[u8; 4] = b"GYRL";
const WIRE_VERSION: u8 = 1;
const HEADER_LEN: usize = 9;
const MAX_RUN: usize = 130;
const MAX_LITERAL: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub max_output_bytes: u32,
    /// Includes the 9-byte wire header. Encoding may return `EncodedTooLarge`
    /// when a particular input cannot fit this declared output bound.
    pub max_encoded_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    Browser,
    Node,
    CrossProcess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub pointer_width_bits: u8,
    pub standard_allocator: bool,
    pub byte_slices: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroLimit,
    LimitTooLarge,
    EncodedLimitTooSmall,
    UnsupportedTarget,
    UnsupportedPointerWidth,
    AllocationLimitExceeded,
    MissingStandardAllocator,
    MissingByteSlices,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroLimit => "output and encoded limits must be positive",
            Self::LimitTooLarge => "declared limit exceeds the RLE recipe maximum",
            Self::EncodedLimitTooSmall => "encoded limit must fit the complete wire header",
            Self::UnsupportedTarget => "RLE recipe supports only process-local Rust",
            Self::UnsupportedPointerWidth => "target pointer width must be 32 or 64 bits",
            Self::AllocationLimitExceeded => "declared limit exceeds target Vec allocation bounds",
            Self::MissingStandardAllocator => "target requires a standard allocator",
            Self::MissingByteSlices => "target requires byte-slice support",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompressionError {
    InvalidLimits,
    InputTooLarge { length: usize, maximum: usize },
    EncodedTooLarge { maximum: usize },
    EncodedInputTooLarge { length: usize, maximum: usize },
    AllocationFailed { requested: usize },
    TruncatedHeader,
    InvalidMagic,
    UnsupportedVersion(u8),
    TruncatedLiteral,
    TruncatedRun,
    OutputLengthExceeded { declared: usize, attempted: usize },
    OutputLengthMismatch { declared: usize, actual: usize },
    TrailingData,
}

impl fmt::Display for CompressionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("compression limits are invalid"),
            Self::InputTooLarge { length, maximum } => {
                write!(f, "input length {length} exceeds maximum {maximum}")
            }
            Self::EncodedTooLarge { maximum } => {
                write!(f, "encoded output exceeds maximum {maximum}")
            }
            Self::EncodedInputTooLarge { length, maximum } => {
                write!(f, "encoded input length {length} exceeds maximum {maximum}")
            }
            Self::AllocationFailed { requested } => {
                write!(f, "unable to reserve {requested} bytes for RLE data")
            }
            Self::TruncatedHeader => f.write_str("RLE input is shorter than its header"),
            Self::InvalidMagic => f.write_str("invalid RLE wire magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported RLE wire version {version}")
            }
            Self::TruncatedLiteral => f.write_str("RLE literal token is truncated"),
            Self::TruncatedRun => f.write_str("RLE run token is truncated"),
            Self::OutputLengthExceeded {
                declared,
                attempted,
            } => write!(
                f,
                "RLE token would produce {attempted} bytes beyond declared output length {declared}"
            ),
            Self::OutputLengthMismatch { declared, actual } => write!(
                f,
                "RLE produced {actual} bytes but header declares {declared}"
            ),
            Self::TrailingData => f.write_str("RLE input contains trailing token data"),
        }
    }
}

impl std::error::Error for CompressionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ir {
    max_output_bytes: u32,
    max_encoded_bytes: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

impl Declaration {
    pub fn normalize(self) -> Result<Ir, Diagnostic> {
        if self.max_output_bytes == 0 || self.max_encoded_bytes == 0 {
            return Err(Diagnostic::ZeroLimit);
        }
        if self.max_output_bytes > MAX_OUTPUT_BYTES || self.max_encoded_bytes > MAX_ENCODED_BYTES {
            return Err(Diagnostic::LimitTooLarge);
        }
        if self.max_encoded_bytes < HEADER_LEN as u32 {
            return Err(Diagnostic::EncodedLimitTooSmall);
        }
        Ok(Ir {
            max_output_bytes: self.max_output_bytes,
            max_encoded_bytes: self.max_encoded_bytes,
        })
    }
}

impl Ir {
    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe={RECIPE}@{RECIPE_VERSION}\nformat=GYRL-v1\nmax_output_bytes={}\nmax_encoded_bytes={}\nheader_bytes={HEADER_LEN}\nrun_token=0x80..0xff:length_3_to_130_then_byte\nliteral_token=0x00..0x7f:length_1_to_128_then_bytes\nrun_threshold=3\ncompression_ratio_guarantee=none\n",
            self.max_output_bytes, self.max_encoded_bytes
        )
    }

    pub fn lower_checked(&self, target: Target, host: HostProfile) -> Result<Artifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !matches!(host.pointer_width_bits, 32 | 64) {
            return Err(Diagnostic::UnsupportedPointerWidth);
        }
        let target_isize_max = match host.pointer_width_bits {
            32 => i32::MAX as u64,
            64 => i64::MAX as u64,
            _ => unreachable!("pointer width validated above"),
        };
        if u64::from(self.max_output_bytes) > target_isize_max
            || u64::from(self.max_encoded_bytes) > target_isize_max
        {
            return Err(Diagnostic::AllocationLimitExceeded);
        }
        if !host.standard_allocator {
            return Err(Diagnostic::MissingStandardAllocator);
        }
        if !host.byte_slices {
            return Err(Diagnostic::MissingByteSlices);
        }
        let canonical_ir = format!(
            "{}target=process_local_rust\ntarget_pointer_width_bits={}\n",
            self.canonical(),
            host.pointer_width_bits
        );
        let source = format!(
            "// {CONTRACT}; recipe={RECIPE}@{RECIPE_VERSION}; no compression ratio is promised.\n\
             pub const MAX_OUTPUT_BYTES: usize = {}usize;\n\
             pub const MAX_ENCODED_BYTES: usize = {}usize;\n\
             pub fn compress(input: &[u8]) -> Result<Vec<u8>, crate::compression_caprichio::CompressionError> {{\n\
                 crate::compression_caprichio::encode(input, MAX_OUTPUT_BYTES, MAX_ENCODED_BYTES)\n\
             }}\n\
             pub fn decompress(encoded: &[u8]) -> Result<Vec<u8>, crate::compression_caprichio::CompressionError> {{\n\
                 crate::compression_caprichio::decode(encoded, MAX_OUTPUT_BYTES, MAX_ENCODED_BYTES)\n\
             }}\n",
            self.max_output_bytes, self.max_encoded_bytes
        );
        Ok(Artifact {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir,
            target_pointer_width_bits: host.pointer_width_bits,
            source,
        })
    }
}

/// Encode opaque bytes as GYRL v1. The nine-byte header is `GYRL`, version 1,
/// and the original byte length as big-endian u32. Payload token bytes with the
/// high bit set encode 3..=130 copies of the next byte; other token bytes encode
/// 1..=128 literal following bytes. RLE can expand data and may return
/// `EncodedTooLarge` under the caller's encoded-output bound.
pub fn encode(
    input: &[u8],
    max_output_bytes: usize,
    max_encoded_bytes: usize,
) -> Result<Vec<u8>, CompressionError> {
    validate_limits(max_output_bytes, max_encoded_bytes)?;
    if input.len() > max_output_bytes {
        return Err(CompressionError::InputTooLarge {
            length: input.len(),
            maximum: max_output_bytes,
        });
    }
    let declared_len = u32::try_from(input.len()).map_err(|_| CompressionError::InputTooLarge {
        length: input.len(),
        maximum: max_output_bytes.min(u32::MAX as usize),
    })?;
    let mut output = Vec::new();
    append_limited(&mut output, MAGIC, max_encoded_bytes)?;
    append_limited(&mut output, &[WIRE_VERSION], max_encoded_bytes)?;
    append_limited(&mut output, &declared_len.to_be_bytes(), max_encoded_bytes)?;

    let mut index = 0;
    while index < input.len() {
        let run = run_length(input, index);
        if run >= 3 {
            append_limited(
                &mut output,
                &[0x80 | (run - 3) as u8, input[index]],
                max_encoded_bytes,
            )?;
            index += run;
            continue;
        }

        let literal_start = index;
        while index < input.len() && index - literal_start < MAX_LITERAL {
            let next_run = run_length(input, index);
            if next_run >= 3 {
                break;
            }
            let remaining = MAX_LITERAL - (index - literal_start);
            index += next_run.min(remaining);
        }
        let literal_len = index - literal_start;
        debug_assert!((1..=MAX_LITERAL).contains(&literal_len));
        append_limited(&mut output, &[(literal_len - 1) as u8], max_encoded_bytes)?;
        append_limited(&mut output, &input[literal_start..index], max_encoded_bytes)?;
    }
    Ok(output)
}

/// Decode a GYRL v1 envelope while enforcing both encoded-input and decoded
/// output limits. The decoded allocation is reserved fallibly before writes.
pub fn decode(
    encoded: &[u8],
    max_output_bytes: usize,
    max_encoded_bytes: usize,
) -> Result<Vec<u8>, CompressionError> {
    validate_limits(max_output_bytes, max_encoded_bytes)?;
    if encoded.len() > max_encoded_bytes {
        return Err(CompressionError::EncodedInputTooLarge {
            length: encoded.len(),
            maximum: max_encoded_bytes,
        });
    }
    if encoded.len() < HEADER_LEN {
        return Err(CompressionError::TruncatedHeader);
    }
    if &encoded[..4] != MAGIC {
        return Err(CompressionError::InvalidMagic);
    }
    if encoded[4] != WIRE_VERSION {
        return Err(CompressionError::UnsupportedVersion(encoded[4]));
    }
    let declared =
        u32::from_be_bytes(encoded[5..9].try_into().expect("fixed header slice")) as usize;
    if declared > max_output_bytes {
        return Err(CompressionError::InputTooLarge {
            length: declared,
            maximum: max_output_bytes,
        });
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(declared)
        .map_err(|_| CompressionError::AllocationFailed {
            requested: declared,
        })?;

    let mut index = HEADER_LEN;
    while index < encoded.len() {
        if output.len() == declared {
            return Err(CompressionError::TrailingData);
        }
        let token = encoded[index];
        index += 1;
        if token & 0x80 != 0 {
            let Some(&byte) = encoded.get(index) else {
                return Err(CompressionError::TruncatedRun);
            };
            index += 1;
            let count = usize::from(token & 0x7f) + 3;
            let attempted =
                output
                    .len()
                    .checked_add(count)
                    .ok_or(CompressionError::OutputLengthExceeded {
                        declared,
                        attempted: usize::MAX,
                    })?;
            if attempted > declared {
                return Err(CompressionError::OutputLengthExceeded {
                    declared,
                    attempted,
                });
            }
            output.resize(attempted, byte);
        } else {
            let count = usize::from(token) + 1;
            let end = index
                .checked_add(count)
                .ok_or(CompressionError::TruncatedLiteral)?;
            if end > encoded.len() {
                return Err(CompressionError::TruncatedLiteral);
            }
            let attempted =
                output
                    .len()
                    .checked_add(count)
                    .ok_or(CompressionError::OutputLengthExceeded {
                        declared,
                        attempted: usize::MAX,
                    })?;
            if attempted > declared {
                return Err(CompressionError::OutputLengthExceeded {
                    declared,
                    attempted,
                });
            }
            output.extend_from_slice(&encoded[index..end]);
            index = end;
        }
    }
    if output.len() != declared {
        return Err(CompressionError::OutputLengthMismatch {
            declared,
            actual: output.len(),
        });
    }
    Ok(output)
}

fn validate_limits(
    max_output_bytes: usize,
    max_encoded_bytes: usize,
) -> Result<(), CompressionError> {
    if max_output_bytes == 0
        || max_output_bytes > MAX_OUTPUT_BYTES as usize
        || max_encoded_bytes < HEADER_LEN
        || max_encoded_bytes > MAX_ENCODED_BYTES as usize
    {
        return Err(CompressionError::InvalidLimits);
    }
    Ok(())
}

fn append_limited(
    output: &mut Vec<u8>,
    bytes: &[u8],
    maximum: usize,
) -> Result<(), CompressionError> {
    let new_len = output
        .len()
        .checked_add(bytes.len())
        .ok_or(CompressionError::EncodedTooLarge { maximum })?;
    if new_len > maximum {
        return Err(CompressionError::EncodedTooLarge { maximum });
    }
    output
        .try_reserve(bytes.len())
        .map_err(|_| CompressionError::AllocationFailed { requested: new_len })?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn run_length(input: &[u8], start: usize) -> usize {
    let byte = input[start];
    let limit = (input.len() - start).min(MAX_RUN);
    let mut length = 1;
    while length < limit && input[start + length] == byte {
        length += 1;
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        Declaration {
            max_output_bytes: 4096,
            max_encoded_bytes: 8192,
        }
    }

    fn host32() -> HostProfile {
        HostProfile {
            pointer_width_bits: 32,
            standard_allocator: true,
            byte_slices: true,
        }
    }

    #[test]
    fn rejects_invalid_limits_and_unsupported_targets_or_hosts() {
        assert_eq!(
            Declaration {
                max_output_bytes: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroLimit)
        );
        assert_eq!(
            Declaration {
                max_encoded_bytes: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroLimit)
        );
        assert_eq!(
            Declaration {
                max_encoded_bytes: 8,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::EncodedLimitTooSmall)
        );
        assert_eq!(
            Declaration {
                max_output_bytes: MAX_OUTPUT_BYTES + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::LimitTooLarge)
        );
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.lower_checked(Target::Browser, host32()),
            Err(Diagnostic::UnsupportedTarget)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    pointer_width_bits: 16,
                    ..host32()
                }
            ),
            Err(Diagnostic::UnsupportedPointerWidth)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    standard_allocator: false,
                    ..host32()
                }
            ),
            Err(Diagnostic::MissingStandardAllocator)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    byte_slices: false,
                    ..host32()
                }
            ),
            Err(Diagnostic::MissingByteSlices)
        );
    }

    #[test]
    fn wire_format_is_deterministic_and_round_trips_compressible_and_expanding_inputs() {
        let limits = declaration().normalize().unwrap();
        let host = host32();
        let a = limits
            .lower_checked(Target::ProcessLocalRust, host)
            .unwrap();
        assert_eq!(
            a,
            limits
                .lower_checked(Target::ProcessLocalRust, host)
                .unwrap()
        );
        assert_eq!(a.contract, CONTRACT);
        assert!(a.canonical_ir.contains("format=GYRL-v1\n"));

        let run_input = [b'A', b'A', b'A', b'B'];
        let run_encoded = encode(&run_input, 16, 64).unwrap();
        assert_eq!(&run_encoded[..9], b"GYRL\x01\0\0\0\x04");
        assert_eq!(&run_encoded[9..], &[0x80, b'A', 0x00, b'B']);
        assert_eq!(decode(&run_encoded, 16, 64).unwrap(), run_input);

        let literals: Vec<u8> = (0..=127).collect();
        let literal_encoded = encode(&literals, 128, 256).unwrap();
        assert!(
            literal_encoded.len() > literals.len(),
            "RLE must not claim all data shrinks"
        );
        assert_eq!(decode(&literal_encoded, 128, 256).unwrap(), literals);
        assert_eq!(decode(&encode(b"", 16, 64).unwrap(), 16, 64).unwrap(), b"");
    }

    #[test]
    fn run_and_literal_tokens_cover_exact_length_boundaries() {
        let cases: &[(usize, &[u8])] = &[
            (2, &[0x01, b'R', b'R']),
            (3, &[0x80, b'R']),
            (129, &[0xfe, b'R']),
            (130, &[0xff, b'R']),
            (131, &[0xff, b'R', 0x00, b'R']),
            (132, &[0xff, b'R', 0x01, b'R', b'R']),
            (133, &[0xff, b'R', 0x80, b'R']),
            (260, &[0xff, b'R', 0xff, b'R']),
        ];
        for &(length, expected_payload) in cases {
            let input = vec![b'R'; length];
            let encoded = encode(&input, length.max(1), MAX_ENCODED_BYTES as usize).unwrap();
            assert_eq!(
                &encoded[HEADER_LEN..],
                expected_payload,
                "unexpected tokenization for run length {length}"
            );
            assert_eq!(
                decode(&encoded, length.max(1), MAX_ENCODED_BYTES as usize).unwrap(),
                input
            );
        }

        for length in [1, 127, 128, 129, 255, 256, 257] {
            let input: Vec<_> = (0..length).map(|index| (index % 251) as u8).collect();
            let encoded = encode(&input, length.max(1), MAX_ENCODED_BYTES as usize).unwrap();
            assert_eq!(
                decode(&encoded, length.max(1), MAX_ENCODED_BYTES as usize).unwrap(),
                input
            );
        }
        let literal_128: Vec<_> = (0..128).map(|value| value as u8).collect();
        let encoded = encode(&literal_128, 128, 256).unwrap();
        assert_eq!(encoded[HEADER_LEN], 0x7f);
        assert_eq!(&encoded[HEADER_LEN + 1..], literal_128);
    }

    #[test]
    fn varied_bounded_inputs_round_trip_and_encode_deterministically() {
        let max_encoded = MAX_ENCODED_BYTES as usize;
        let mut state = 0x9e37_79b9u32;
        for length in 0..=512 {
            let mut input = Vec::with_capacity(length);
            for index in 0..length {
                // Deterministic, varied bytes with occasional short repeated
                // regions to cover both literal and run token paths.
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let byte = if index % 37 < 4 {
                    (index / 37) as u8
                } else {
                    (state >> 24) as u8
                };
                input.push(byte);
            }
            let max_output = length.max(1);
            let encoded = encode(&input, max_output, max_encoded).unwrap();
            assert_eq!(
                decode(&encoded, max_output, max_encoded).unwrap(),
                input,
                "round trip failed at input length {length}"
            );
            assert_eq!(
                encode(&input, max_output, max_encoded).unwrap(),
                encoded,
                "encoding changed at input length {length}"
            );
        }
    }

    #[test]
    fn rejects_input_and_encoded_limits_and_encoded_expansion() {
        assert_eq!(
            encode(b"12345", 4, 64),
            Err(CompressionError::InputTooLarge {
                length: 5,
                maximum: 4
            })
        );
        assert_eq!(
            encode(b"abc", 4, 9),
            Err(CompressionError::EncodedTooLarge { maximum: 9 })
        );
        let encoded = encode(b"abc", 4, 64).unwrap();
        assert_eq!(
            decode(&encoded, 4, 10),
            Err(CompressionError::EncodedInputTooLarge {
                length: encoded.len(),
                maximum: 10
            })
        );
    }

    #[test]
    fn rejects_malformed_versions_lengths_tokens_overflow_and_trailing_bytes() {
        assert_eq!(
            decode(b"short", 16, 64),
            Err(CompressionError::TruncatedHeader)
        );
        let valid = encode(b"abc", 16, 64).unwrap();
        let mut bad_magic = valid.clone();
        bad_magic[0] ^= 1;
        assert_eq!(
            decode(&bad_magic, 16, 64),
            Err(CompressionError::InvalidMagic)
        );
        let mut bad_version = valid.clone();
        bad_version[4] = 2;
        assert_eq!(
            decode(&bad_version, 16, 64),
            Err(CompressionError::UnsupportedVersion(2))
        );
        let mut short_literal = valid.clone();
        short_literal.pop();
        assert_eq!(
            decode(&short_literal, 16, 64),
            Err(CompressionError::TruncatedLiteral)
        );
        let mut short_run = valid[..9].to_vec();
        short_run[8] = 1;
        short_run.extend_from_slice(&[0x80]);
        assert_eq!(
            decode(&short_run, 16, 64),
            Err(CompressionError::TruncatedRun)
        );
        let mut short_output = valid.clone();
        short_output[8] = 4;
        assert!(matches!(
            decode(&short_output, 16, 64),
            Err(CompressionError::OutputLengthMismatch {
                declared: 4,
                actual: 3
            })
        ));
        let mut over_output = valid.clone();
        over_output[8] = 2;
        assert!(matches!(
            decode(&over_output, 16, 64),
            Err(CompressionError::OutputLengthExceeded {
                declared: 2,
                attempted: 3
            })
        ));
        let mut trailing = valid.clone();
        trailing.push(0);
        assert_eq!(
            decode(&trailing, 16, 64),
            Err(CompressionError::TrailingData)
        );

        let mut declared_empty_with_token = encode(b"", 16, 64).unwrap();
        declared_empty_with_token.extend_from_slice(&[0x00, b'x']);
        assert_eq!(
            decode(&declared_empty_with_token, 16, 64),
            Err(CompressionError::TrailingData)
        );

        let mut truncated_max_literal = valid[..HEADER_LEN].to_vec();
        truncated_max_literal[8] = 128;
        truncated_max_literal.push(0x7f);
        assert_eq!(
            decode(&truncated_max_literal, 128, 64),
            Err(CompressionError::TruncatedLiteral)
        );

        let mut max_run_over_declared = valid[..HEADER_LEN].to_vec();
        max_run_over_declared[8] = 129;
        max_run_over_declared.extend_from_slice(&[0xff, b'x']);
        assert_eq!(
            decode(&max_run_over_declared, 129, 64),
            Err(CompressionError::OutputLengthExceeded {
                declared: 129,
                attempted: 130
            })
        );
    }

    #[test]
    fn hard_limits_accept_exact_header_and_reject_over_limit_without_large_allocations() {
        let exact_header = encode(b"", 1, HEADER_LEN).unwrap();
        assert_eq!(exact_header.len(), HEADER_LEN);
        assert_eq!(decode(&exact_header, 1, HEADER_LEN).unwrap(), b"");
        assert_eq!(
            encode(b"x", 1, HEADER_LEN),
            Err(CompressionError::EncodedTooLarge {
                maximum: HEADER_LEN
            })
        );

        assert_eq!(
            validate_limits(0, HEADER_LEN),
            Err(CompressionError::InvalidLimits)
        );
        assert_eq!(
            validate_limits(1, HEADER_LEN - 1),
            Err(CompressionError::InvalidLimits)
        );
        assert_eq!(
            validate_limits(MAX_OUTPUT_BYTES as usize + 1, HEADER_LEN),
            Err(CompressionError::InvalidLimits)
        );
        assert_eq!(
            validate_limits(1, MAX_ENCODED_BYTES as usize + 1),
            Err(CompressionError::InvalidLimits)
        );

        let encoded = encode(b"x", 1, 32).unwrap();
        assert_eq!(
            decode(&encoded, 1, encoded.len() - 1),
            Err(CompressionError::EncodedInputTooLarge {
                length: encoded.len(),
                maximum: encoded.len() - 1
            })
        );
        let mut too_much_output = encode(b"", 1, 32).unwrap();
        too_much_output[5..9].copy_from_slice(&2u32.to_be_bytes());
        assert_eq!(
            decode(&too_much_output, 1, 32),
            Err(CompressionError::InputTooLarge {
                length: 2,
                maximum: 1
            })
        );
    }

    #[test]
    fn emitted_factory_compiles_and_runs_against_this_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, host32())
            .unwrap();
        let module =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/compression_caprichio.rs");
        let harness = format!(
            "#[path = {module:?}] mod compression_caprichio;\n{}\nfn main() {{\n\
             let input = vec![b'Q'; 333];\n\
             let packed = compress(&input).expect(\"compress\");\n\
             assert!(packed.len() < input.len());\n\
             assert_eq!(decompress(&packed).expect(\"decompress\"), input);\n\
             assert!(matches!(compress(&vec![0u8; 4097]), Err(compression_caprichio::CompressionError::InputTooLarge {{ .. }})));\n\
             }}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let binary =
            std::env::temp_dir().join(format!("goya-rle-factory-{}-{nonce}", std::process::id()));
        let mut compiler = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&binary)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let output = compiler.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "generated RLE factory did not compile:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let run = Command::new(&binary).output().unwrap();
        let _ = std::fs::remove_file(binary);
        assert!(
            run.status.success(),
            "generated RLE factory failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    /// Manual local codec benchmark: run/repeated data and literal-heavy data; no speed threshold.
    #[test]
    #[ignore = "manual RLE codec microbenchmark"]
    fn benchmark_rle_codec() {
        use std::time::Instant;
        let repeated = vec![0x5a; 1024 * 1024];
        let literals: Vec<u8> = (0..1024 * 1024).map(|index| (index % 251) as u8).collect();
        for (name, input) in [("repeated", &repeated), ("literal-heavy", &literals)] {
            let start = Instant::now();
            let mut encoded_bytes = 0usize;
            for _ in 0..20 {
                let encoded =
                    encode(input, MAX_OUTPUT_BYTES as usize, MAX_ENCODED_BYTES as usize).unwrap();
                encoded_bytes = encoded.len();
                assert_eq!(
                    decode(
                        &encoded,
                        MAX_OUTPUT_BYTES as usize,
                        MAX_ENCODED_BYTES as usize
                    )
                    .unwrap(),
                    *input
                );
                std::hint::black_box(&encoded);
            }
            eprintln!("20 RLE encode/decode iterations of {name} 1 MiB; last output {encoded_bytes} bytes; elapsed {:?}", start.elapsed());
        }
    }
}
