//! Source-mapped resolver diagnostics.
//!
//! Codes owned by this crate use the `GY-RESOLVER-` prefix. Codes that begin
//! with `GY-E` are copied from strict GSSH admission so a rejected source
//! keeps the frontend's identity.

use std::fmt;

use goya_lang::SourceRange;

/// One failure a caller can report without inspecting interpreter internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub range: Option<SourceRange>,
}

impl Diagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            range: None,
        }
    }

    pub fn with_range(mut self, range: SourceRange) -> Self {
        self.range = Some(range);
        self
    }
}

pub(crate) fn located(
    code: &'static str,
    message: impl Into<String>,
    range: Option<SourceRange>,
) -> Diagnostic {
    let diagnostic = Diagnostic::new(code, message);
    match range {
        Some(range) => diagnostic.with_range(range),
        None => diagnostic,
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.range {
            Some(range) => write!(
                f,
                "{}: {} (bytes {}..{})",
                self.code, self.message, range.start, range.end
            ),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for Diagnostic {}
