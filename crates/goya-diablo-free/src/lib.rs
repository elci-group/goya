//! Diablo Free — FreeBSD native compiler for Goya.

#![forbid(unsafe_code)]

pub mod cli;
pub mod lower;
pub mod manifest;
pub mod orchestrate;
pub mod target;

/// Diablo Free directive ID.
pub const DIRECTIVE_ID: &str = "GOYA-DIA-TD-001";

/// Diablo Free compiler version.
pub const COMPILER_VERSION: &str = "0.0.1";

/// Diablo Free profile.
pub const PROFILE: &str = "diablo.free.interactive/0.0.1";

/// Re-export Fault from goya-diablos.
pub use goya_diablos::Fault;
pub use goya_diablos::fault::exit;

/// Re-export all public items from target module
pub use target::{
    TargetClass, FREEBSD_TRIPLES, classify_target, is_p0, is_named_blocked,
    host, elf_machine, profile_class, blocked_reason,
    is_freebsd_target, target_class,
};

// Re-export orchestrate module (only public items)
pub use orchestrate::{execute, default_invocation};

// Re-export manifest module
pub use manifest::{freebsd_desktop, freebsd_server, freebsd_headless, freebsd_constrained};

// Re-export pipeline types from goya-diablos
pub use goya_diablos::pipeline::{CommandKind, Invocation, Run};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freebsd_triples_recognized() {
        assert!(is_freebsd_target("x86_64-unknown-freebsd"));
        assert!(is_freebsd_target("aarch64-unknown-freebsd"));
        assert!(is_freebsd_target("i386-unknown-freebsd"));
    }

    #[test]
    fn non_freebsd_not_recognized() {
        assert!(!is_freebsd_target("x86_64-unknown-linux-gnu"));
        assert!(!is_freebsd_target("x86_64-pc-windows-msvc"));
    }
}