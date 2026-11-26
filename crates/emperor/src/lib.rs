//! Emperor, the Goya Linux compiler.
//!
//! Strategy A lowers one admitted interactive counter to typed Rust and links
//! it with `emperor-runtime`. Strategy B is not implemented. A produced ELF is
//! not a release, a Windows launch, or a state-of-the-art claim.

#![forbid(unsafe_code)]

pub mod cli;
pub mod desktop;
pub mod elf;
pub mod lower;
pub mod orchestrate;
pub mod stage;
pub mod target;

/// Directive that defined this compiler's obligations.
pub const DIRECTIVE_ID: &str = "GY-EMP-001";

/// Compiler version. It is not a production designation.
pub const COMPILER_VERSION: &str = "0.0.1";

/// Linux profile implemented by the first counter.
pub const PROFILE: &str = "emperor.linux.interactive/0.0.1";

/// One compiler fault with a stable exit class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    pub exit: u8,
    pub code: &'static str,
    pub message: String,
}

impl Fault {
    pub fn new(exit: u8, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            exit,
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Fault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
