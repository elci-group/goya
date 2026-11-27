//! Fault types and exit codes for Diablos compilers.

use std::path::Path;

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

impl std::error::Error for Fault {}

/// Exit codes for Diablos compilers (matching Emperor convention).
pub mod exit {
    /// Every requested check passed.
    pub const SUCCESS: u8 = 0;
    /// Source or user validation failed (bad scene, .. path, unknown triple).
    pub const ADMISSION_FAILED: u8 = 1;
    /// Invocation or toolchain failed (missing flag, unknown command, Cargo failure).
    pub const TOOLCHAIN_FAILED: u8 = 2;
    /// Qualification blocked (missing display, session not qualified, cross-compile).
    pub const BLOCKED: u8 = 3;
    /// Artifact failed verification (bad ELF, hash mismatch, missing RELRO).
    pub const VERIFICATION_FAILED: u8 = 4;
    /// Runtime or conformance failed (oracle/smoke non-zero).
    pub const RUNTIME_FAILED: u8 = 5;
}

/// Construct a Fault for admission failures (exit 1).
pub fn admission_failed(code: &'static str, message: impl Into<String>) -> Fault {
    Fault::new(exit::ADMISSION_FAILED, code, message)
}

/// Construct a Fault for toolchain failures (exit 2).
pub fn toolchain_failed(code: &'static str, message: impl Into<String>) -> Fault {
    Fault::new(exit::TOOLCHAIN_FAILED, code, message)
}

/// Construct a Fault for blocked qualification (exit 3).
pub fn blocked(code: &'static str, message: impl Into<String>) -> Fault {
    Fault::new(exit::BLOCKED, code, message)
}

/// Construct a Fault for verification failures (exit 4).
pub fn verification_failed(code: &'static str, message: impl Into<String>) -> Fault {
    Fault::new(exit::VERIFICATION_FAILED, code, message)
}

/// Construct a Fault for runtime failures (exit 5).
pub fn runtime_failed(code: &'static str, message: impl Into<String>) -> Fault {
    Fault::new(exit::RUNTIME_FAILED, code, message)
}

/// Validate that a path does not contain `..` components.
pub fn validate_no_dotdot(path: &Path, context: &str) -> Result<(), Fault> {
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(admission_failed(
                "INVALID_PATH",
                format!("{context} path contains '..' component: {}", path.display()),
            ));
        }
    }
    Ok(())
}

/// Validate that a string is not empty.
pub fn validate_not_empty(value: &str, field: &str) -> Result<(), Fault> {
    if value.trim().is_empty() {
        return Err(admission_failed(
            "EMPTY_FIELD",
            format!("{field} must not be empty"),
        ));
    }
    Ok(())
}