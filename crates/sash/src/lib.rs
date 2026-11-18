//! Sash, the Goya Windows compiler.
//!
//! The canonical scene contract is strict GY-LANG admission (`GSSH/1`).
//! `goya-ir` is not a second source of window meaning. This crate forbids
//! unsafe code; the generated application contains the Win32 adapter.

#![forbid(unsafe_code)]

pub mod admit;
pub mod cli;
pub mod emit;
pub mod interactive;
pub mod ir_codegen;
pub mod ir_lower;
pub mod orchestrate;
pub mod pe;
pub mod report;
pub mod windows_target;

use std::fmt;

/// Directive that defined this compiler's obligations.
pub const DIRECTIVE_ID: &str = "SASH-ENT-001";

/// Admitted machine target for the first static-window proof.
pub const TARGET_MSVC_X64: &str = "x86_64-pc-windows-msvc";

/// Distribution profile implemented by the first proof.
pub const PROFILE_PORTABLE: &str = "portable-native";

/// SHA-256 of bytes, lowercase hex.
pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex_encode(&Sha256::digest(bytes))
}

/// One compiler diagnostic. Origins are half-open UTF-8 byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub stage: &'static str,
    pub message: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub remediation: &'static str,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Diagnostic {
    pub fn new(
        code: &'static str,
        stage: &'static str,
        message: impl Into<String>,
        remediation: &'static str,
    ) -> Self {
        Self {
            code,
            stage,
            message: message.into(),
            start: None,
            end: None,
            remediation,
        }
    }

    pub fn at(mut self, start: usize, end: usize) -> Self {
        self.start = Some(start);
        self.end = Some(end);
        self
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "code": self.code,
            "stage": self.stage,
            "severity": "error",
            "message": self.message,
            "origin": match (self.start, self.end) {
                (Some(start), Some(end)) => serde_json::json!({"start": start, "end": end}),
                _ => serde_json::Value::Null,
            },
            "remediation": self.remediation,
        })
    }
}
