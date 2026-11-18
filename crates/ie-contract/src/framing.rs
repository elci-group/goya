//! Canonical bounded framing (IE-X01): 4-byte little-endian length prefix
//! over an authenticated stream. Oversized messages are rejected **before
//! allocation** — the decoder never reserves memory proportional to an
//! unvalidated length field.

pub const MAX_MESSAGE_BYTES: u32 = 16 * 1024 * 1024;

/// Encode one frame: length prefix + payload.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() as u64 > MAX_MESSAGE_BYTES as u64 {
        return Err(FrameError::DeclaredTooLarge(payload.len() as u32));
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Fewer than 4 bytes of header available; caller should buffer more.
    IncompleteHeader,
    /// Declared length exceeds MAX_MESSAGE_BYTES — rejected before any
    /// allocation, and the peer MUST be treated as violating the contract.
    DeclaredTooLarge(u32),
    /// Fewer payload bytes than declared; caller should buffer more.
    IncompletePayload { declared: u32, available: usize },
}

/// Decode one frame from `buf`. Returns the payload slice and the total
/// consumed byte count (prefix + payload). Never allocates on the error
/// paths: header and size checks happen on the borrowed slice only.
pub fn decode_frame(buf: &[u8]) -> Result<(&[u8], usize), FrameError> {
    if buf.len() < 4 {
        return Err(FrameError::IncompleteHeader);
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if len as u64 > MAX_MESSAGE_BYTES as u64 {
        return Err(FrameError::DeclaredTooLarge(len));
    }
    let end = 4usize
        .checked_add(len as usize)
        .ok_or(FrameError::DeclaredTooLarge(len))?;
    if buf.len() < end {
        return Err(FrameError::IncompletePayload { declared: len, available: buf.len() - 4 });
    }
    Ok((&buf[4..end], end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let frame = encode_frame(b"hello").unwrap();
        let (payload, consumed) = decode_frame(&frame).unwrap();
        assert_eq!(payload, b"hello");
        assert_eq!(consumed, 9);
    }

    #[test]
    fn oversized_declared_rejected_before_allocation() {
        // Declared length far above the cap; buffer itself is tiny.
        let malicious = (u32::MAX - 1).to_le_bytes();
        assert_eq!(
            decode_frame(&malicious),
            Err(FrameError::DeclaredTooLarge(u32::MAX - 1))
        );
    }

    #[test]
    fn incomplete_paths_do_not_panic() {
        assert_eq!(decode_frame(&[1, 2]), Err(FrameError::IncompleteHeader));
        let frame = encode_frame(b"abcdef").unwrap();
        assert_eq!(
            decode_frame(&frame[..6]),
            Err(FrameError::IncompletePayload { declared: 6, available: 2 })
        );
    }

    #[test]
    fn exact_max_allowed_off_by_one_rejected() {
        let frame = encode_frame(&vec![0u8; MAX_MESSAGE_BYTES as usize]).unwrap();
        assert_eq!(decode_frame(&frame).unwrap().0.len(), MAX_MESSAGE_BYTES as usize);
        assert!(encode_frame(&vec![0u8; MAX_MESSAGE_BYTES as usize + 1]).is_err());
    }
}
