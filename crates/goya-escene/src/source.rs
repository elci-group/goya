//! Source identity, exact byte preservation and spans (ESC-INV-001).
//!
//! The authoritative source is the exact original byte sequence. Spans are
//! zero-based UTF-8 byte offsets, half-open: `[start_byte, end_byte)`.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Zero-based, half-open UTF-8 byte span with declared attribution precision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub source_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub precision: Precision,
}

/// Attribution precision: the engine never fabricates exact locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precision {
    Exact,
    Enclosing,
    Approximate,
}

impl Span {
    /// Construct a span, verifying byte bounds against `source` and that the
    /// endpoints do not split a UTF-8 sequence.
    pub fn new(source: &SourceSnapshot, start: usize, end: usize, precision: Precision) -> Option<Span> {
        if start > end || end > source.bytes.len() {
            return None;
        }
        if std::str::from_utf8(&source.bytes[start..end]).is_err() {
            return None;
        }
        Some(Span { source_id: source.id.clone(), start_byte: start, end_byte: end, precision })
    }

    /// Extract the exact source bytes covered by this span.
    pub fn slice<'a>(&self, source: &'a SourceSnapshot) -> &'a [u8] {
        &source.bytes[self.start_byte..self.end_byte]
    }
}

/// An exact snapshot of the original `.gscene` bytes plus derived identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub id: String,
    pub logical_path: String,
    pub content_base64: String,
    pub byte_length: usize,
    pub digest: String,

    #[serde(skip)]
    pub bytes: Vec<u8>,
}

impl SourceSnapshot {
    /// Capture exact bytes, compute the SHA-256 digest and byte length.
    pub fn capture(id: &str, logical_path: &str, bytes: Vec<u8>) -> SourceSnapshot {
        let digest = sha256_hex(&bytes);
        let byte_length = bytes.len();
        let content_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        SourceSnapshot {
            id: id.to_string(),
            logical_path: logical_path.to_string(),
            content_base64,
            byte_length,
            digest: format!("sha256:{digest}"),
            bytes,
        }
    }

    /// Recover the exact original bytes (ESC-INV-001).
    pub fn recover(&self) -> Result<Vec<u8>, SourceError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.content_base64)
            .map_err(|_| SourceError::InvalidBase64)?;
        if bytes.len() != self.byte_length {
            return Err(SourceError::LengthMismatch {
                declared: self.byte_length,
                actual: bytes.len(),
            });
        }
        if format!("sha256:{}", sha256_hex(&bytes)) != self.digest {
            return Err(SourceError::DigestMismatch);
        }
        Ok(bytes)
    }
}

#[derive(Debug)]
pub enum SourceError {
    InvalidBase64,
    LengthMismatch { declared: usize, actual: usize },
    DigestMismatch,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::InvalidBase64 => write!(f, "source payload is not valid base64"),
            SourceError::LengthMismatch { declared, actual } => {
                write!(f, "source length mismatch: declared {declared}, actual {actual}")
            }
            SourceError::DigestMismatch => write!(f, "source digest mismatch"),
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_is_byte_exact() {
        // Includes a BOM, CRLF line endings and an original emoji in the source.
        let bytes: Vec<u8> = b"\xEF\xBB\xBFblock render {\r\n    resolve() \xF0\x9F\x8C\x88\r\n}\r\n".to_vec();
        let snap = SourceSnapshot::capture("src-1", "Doom.gscene", bytes.clone());
        assert_eq!(snap.recover().unwrap(), bytes);
        assert_eq!(snap.digest.len(), 71); // "sha256:" + 64 hex chars
    }

    #[test]
    fn tampered_payload_fails_integrity() {
        let snap = SourceSnapshot::capture("src-1", "Doom.gscene", b"block {}".to_vec());
        let mut bad = snap.clone();
        bad.content_base64 = base64::engine::general_purpose::STANDARD.encode(b"block }");
        assert!(matches!(bad.recover(), Err(SourceError::LengthMismatch { .. })));
        let mut bad2 = snap.clone();
        bad2.digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
        assert!(matches!(bad2.recover(), Err(SourceError::DigestMismatch)));
    }

    #[test]
    fn span_rejects_utf8_split() {
        let snap = SourceSnapshot::capture("src-1", "x.gscene", "a😀b".as_bytes().to_vec());
        // The emoji occupies bytes 1..5; splitting at 2 must be rejected.
        assert!(Span::new(&snap, 2, 6, Precision::Exact).is_none());
        assert!(Span::new(&snap, 0, 5, Precision::Exact).is_some());
        assert_eq!(Span::new(&snap, 0, 5, Precision::Exact).unwrap().slice(&snap), "a😀".as_bytes());
    }
}
