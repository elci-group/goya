//! Diablos Core — shared BSD compilation infrastructure for Goya.
//!
//! This crate provides the common compilation pipeline, capability registry,
//! ABI resolution, graphics integration, security synthesis, packaging,
//! and certification infrastructure shared across all Diablo backends.

#![forbid(unsafe_code)]

pub mod abi;
pub mod capabilities;
pub mod certification;
pub mod fault;
pub mod graphics;
pub mod packaging;
pub mod pipeline;
pub mod security;

/// Directive that defined this compiler family's obligations.
pub const DIRECTIVE_ID: &str = "GOYA-DIA-TD-001";

/// Compiler family version. Not a production designation.
pub const COMPILER_VERSION: &str = "0.0.1";

/// Primary Diablo profile implemented by the first vertical slice.
pub const PROFILE: &str = "diablo.free.interactive/0.0.1";

// Re-export Fault and exit codes from fault module.
pub use fault::{Fault, exit};

// Re-export pipeline types for downstream crates.
pub use pipeline::{CommandKind, Invocation, Run, execute};

// Re-export graphics backend types for downstream crates.
pub use crate::graphics::backend::{GraphicsBackend, BackendType, SoftwareBackend, DrmBackend, WaylandBackend, X11Backend, create_backend, select_backend};

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::fault::Fault;
    use super::*;

    #[test]
    fn fault_display() {
        let f = Fault::new(1, "ADMISSION_FAILED", "scene too large");
        assert_eq!(f.to_string(), "ADMISSION_FAILED: scene too large");
    }

    #[test]
    fn sha256_hex_known() {
        assert_eq!(sha256_hex(b"hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }
}