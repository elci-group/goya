//! Differential encoding of observations (GVI-D, directive §12.2).
//!
//! Phase 3 implements a deterministic, **self-verifying** line-based
//! delta: an observation's payload is stored as instructions against one
//! base payload. Every encode verifies its own decode (`encode` refuses
//! to emit a delta that does not reconstruct the target byte for byte),
//! so a `DELTA` encoding is certified exact by verification — never by
//! appearance (§12.3, AVR-C09).
//!
//! Chains (a delta against another delta) are legal but bounded: the
//! reconstruction graph enforces acyclicity and maximum depth (§12.2,
//! AVR-C04/C05); the derivation engine currently bases deltas on anchors
//! only, keeping chains at depth 1.
//!
//! Encoding selection compares the *complete* encoded size — header,
//! instruction stream and overhead — against the raw payload (§12.5);
//! an encoding that does not pay for itself is not selected. Storage
//! reduction alone is explicitly insufficient justification (§8.5): the
//! decode cost is bounded by construction (one linear pass) and the
//! chain depth by the reconstruction graph.

/// Why a delta could not be encoded or applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    /// The delta stream is malformed (§17: untrusted input).
    Malformed,
    /// Applying the delta produced something other than the expected
    /// payload — the certificate claim is void.
    VerificationFailed,
    /// A delta instruction references a base line that does not exist.
    BaseLineOutOfRange { index: u32 },
}

/// Fixed header every delta stream begins with; counted in the §12.5
/// size comparison.
pub const DELTA_HEADER: &[u8] = b"AVR-DELTA1\n";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Instruction {
    /// Copy line `index` from the base payload.
    Keep(u32),
    /// Insert these bytes (one line including its newline).
    Insert(Vec<u8>),
}

fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            lines.push(&bytes[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        lines.push(&bytes[start..]);
    }
    lines
}

/// Longest common subsequence over lines, computed by dynamic
/// programming. Deterministic: ties in backtracking prefer `Keep`
/// (matching lines) over `Insert`, giving a canonical instruction stream.
fn lcs_instructions(base: &[&[u8]], target: &[&[u8]]) -> Vec<Instruction> {
    let n = base.len();
    let m = target.len();
    // lcs[i][j] = LCS length of base[i..] and target[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == target[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut instructions = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if base[i] == target[j] {
            instructions.push(Instruction::Keep(i as u32));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            // Deterministic tie-break: emit an insert of the target line
            // and advance the target (j += 1) — wait, this branch means
            // skipping base[i] is at least as good; the matching insert
            // of target[j] happens in the other branch. Here we skip the
            // base line (nothing to emit).
            i += 1;
        } else {
            instructions.push(Instruction::Insert(target[j].to_vec()));
            j += 1;
        }
    }
    while j < m {
        instructions.push(Instruction::Insert(target[j].to_vec()));
        j += 1;
    }
    instructions
}

fn write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn serialize(instructions: &[Instruction]) -> Vec<u8> {
    let mut out = Vec::from(DELTA_HEADER);
    for instruction in instructions {
        match instruction {
            Instruction::Keep(index) => {
                out.push(b'K');
                write_u32(&mut out, *index);
            }
            Instruction::Insert(line) => {
                out.push(b'I');
                write_u32(&mut out, line.len() as u32);
                out.extend_from_slice(line);
            }
        }
    }
    out
}

fn deserialize(delta: &[u8]) -> Result<Vec<Instruction>, DeltaError> {
    if !delta.starts_with(DELTA_HEADER) {
        return Err(DeltaError::Malformed);
    }
    let mut instructions = Vec::new();
    let mut cursor = DELTA_HEADER.len();
    while cursor < delta.len() {
        match delta[cursor] {
            b'K' => {
                let Some(bytes): Option<&[u8; 4]> = delta.get(cursor + 1..cursor + 5).and_then(|s| s.try_into().ok()) else {
                    return Err(DeltaError::Malformed);
                };
                instructions.push(Instruction::Keep(u32::from_le_bytes(*bytes)));
                cursor += 5;
            }
            b'I' => {
                let Some(bytes): Option<&[u8; 4]> = delta.get(cursor + 1..cursor + 5).and_then(|s| s.try_into().ok()) else {
                    return Err(DeltaError::Malformed);
                };
                let len = u32::from_le_bytes(*bytes) as usize;
                let Some(line) = delta.get(cursor + 5..cursor + 5 + len) else {
                    return Err(DeltaError::Malformed);
                };
                instructions.push(Instruction::Insert(line.to_vec()));
                cursor += 5 + len;
            }
            _ => return Err(DeltaError::Malformed),
        }
    }
    Ok(instructions)
}

/// Encode `target` as a delta against `base`.
///
/// Returns the complete encoded stream (header included), verified to
/// reconstruct `target` byte for byte, or `None` when the delta would not
/// reconstruct (never emitted unverified) or the payload exceeds `u32`
/// line indexing.
pub fn encode(base: &[u8], target: &[u8]) -> Option<Vec<u8>> {
    if base.len() > u32::MAX as usize || target.len() > u32::MAX as usize {
        return None;
    }
    let base_lines = split_lines(base);
    let target_lines = split_lines(target);
    let delta = serialize(&lcs_instructions(&base_lines, &target_lines));
    // Self-verification (AVR-C09 discipline): only emit what decodes.
    if apply(base, &delta) == Ok(target.to_vec()) {
        Some(delta)
    } else {
        None
    }
}

/// Reconstruct the target payload from `base` and a delta stream.
pub fn apply(base: &[u8], delta: &[u8]) -> Result<Vec<u8>, DeltaError> {
    let instructions = deserialize(delta)?;
    let base_lines = split_lines(base);
    let mut out = Vec::with_capacity(base.len());
    for instruction in &instructions {
        match instruction {
            Instruction::Keep(index) => {
                let line = base_lines
                    .get(*index as usize)
                    .ok_or(DeltaError::BaseLineOutOfRange { index: *index })?;
                out.extend_from_slice(line);
            }
            Instruction::Insert(line) => out.extend_from_slice(line),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg(lines: &[&str]) -> Vec<u8> {
        lines.iter().flat_map(|l| format!("{l}\n").into_bytes()).collect()
    }

    #[test]
    fn delta_round_trips_byte_for_byte() {
        let base = svg(&["<defs>", "<linearGradient id=\"g1\">", "</linearGradient>", "</defs>", "<polygon points=\"1,2 3,4\"/>"]);
        let target = svg(&["<defs>", "<linearGradient id=\"g1\">", "</linearGradient>", "</defs>", "<polygon points=\"9,8 7,6\"/>", "<polygon points=\"5,5 5,6\"/>"]);
        let delta = encode(&base, &target).expect("similar payloads delta-compress");
        assert_eq!(apply(&base, &delta), Ok(target.clone()));
        assert!(delta.len() < target.len(), "delta must be smaller for similar payloads");
        assert!(delta.starts_with(DELTA_HEADER), "§12.5: header counted in the comparison");
    }

    #[test]
    fn delta_of_dissimilar_payloads_is_not_selected() {
        let base = svg(&["<a>"]);
        let target = svg(&["<completely>", "<different>", "<content>", "<here>"]);
        let delta = encode(&base, &target).expect("encode still succeeds");
        // The caller's cost comparison (§12.5) must reject it.
        assert!(delta.len() >= target.len());
    }

    #[test]
    fn encoding_is_deterministic() {
        let base = svg(&["<x>", "<y>", "<z>"]);
        let target = svg(&["<x>", "<w>", "<z>", "<extra>"]);
        let a = encode(&base, &target).expect("a");
        let b = encode(&base, &target).expect("b");
        assert_eq!(a, b);
    }

    #[test]
    fn empty_and_degenerate_payloads_round_trip() {
        for (base, target) in [
            (Vec::new(), b"one line\n".to_vec()),
            (b"only base\n".to_vec(), Vec::new()),
            (Vec::new(), Vec::new()),
            (b"no newline tail".to_vec(), b"no newline tail too".to_vec()),
        ] {
            let delta = encode(&base, &target).expect("degenerate round trip");
            assert_eq!(apply(&base, &delta), Ok(target.clone()));
        }
    }

    #[test]
    fn malformed_deltas_are_rejected() {
        assert_eq!(apply(b"base\n", b"garbage"), Err(DeltaError::Malformed));
        assert_eq!(apply(b"base\n", b"AVR-DELTA1\nX"), Err(DeltaError::Malformed));
        // Truncated Keep instruction.
        assert_eq!(apply(b"base\n", b"AVR-DELTA1\nK\x01"), Err(DeltaError::Malformed));
        // Keep out of range (AVR-C08 referential integrity discipline).
        let mut d = Vec::from(DELTA_HEADER);
        d.push(b'K');
        d.extend_from_slice(&99u32.to_le_bytes());
        assert_eq!(
            apply(b"base\n", &d),
            Err(DeltaError::BaseLineOutOfRange { index: 99 })
        );
    }

    #[test]
    fn identical_target_is_a_pure_keep_stream() {
        let base = svg(&["<p/>", "<q/>"]);
        let delta = encode(&base, &base.clone()).expect("identical");
        assert_eq!(apply(&base, &delta), Ok(base.clone()));
        assert_eq!(delta.len(), DELTA_HEADER.len() + 2 * 5);
    }
}
