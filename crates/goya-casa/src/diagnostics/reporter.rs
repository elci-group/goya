//! Machine-readable diagnostic output (directive §21).
//!
//! A [`Reporter`] collects [`Diagnostic`]s in emission order and renders
//! them either as JSON (machine-readable) or plain text (human-readable).

use crate::diagnostics::codes::{DiagnosticCode, Severity};
use serde::{Deserialize, Serialize};

/// Byte span within a named source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Sheet name or path the span refers to.
    pub source: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Registry code, e.g. `CASA-SYN-001`.
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

#[derive(Debug, Default)]
pub struct Reporter {
    diagnostics: Vec<Diagnostic>,
}

impl Reporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(
        &mut self,
        code: DiagnosticCode,
        message: impl Into<String>,
        location: Option<Location>,
    ) {
        self.diagnostics.push(Diagnostic {
            code: code.code.to_string(),
            severity: code.default_severity,
            message: message.into(),
            location,
        });
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// JSON array of diagnostics, in emission order.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.diagnostics).expect("diagnostic serialization is infallible")
    }

    /// One human-readable line per diagnostic.
    pub fn render_text(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| match &d.location {
                Some(loc) => format!(
                    "{} {}: {} ({}:{}..{})",
                    severity_tag(d.severity),
                    d.code,
                    d.message,
                    loc.source,
                    loc.start,
                    loc.end
                ),
                None => format!("{} {}: {}", severity_tag(d.severity), d.code, d.message),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn severity_tag(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitted_diagnostics_round_trip_through_json() {
        let mut reporter = Reporter::new();
        reporter.emit(
            crate::diagnostics::codes::SYNTAX_INVALID,
            "bad declaration",
            Some(Location {
                source: "test.css".into(),
                start: 3,
                end: 9,
            }),
        );
        reporter.emit(crate::diagnostics::codes::IMPORT_CYCLE, "a -> b -> a", None);

        let json = reporter.to_json();
        let parsed: Vec<Diagnostic> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, reporter.diagnostics());
        assert_eq!(parsed[0].code, "CASA-SYN-001");
        assert_eq!(parsed[0].location.as_ref().unwrap().source, "test.css");
        assert!(parsed[1].location.is_none());
    }

    #[test]
    fn error_detection() {
        let mut reporter = Reporter::new();
        assert!(!reporter.has_errors());
        reporter.emit(crate::diagnostics::codes::IMPORT_UNRESOLVED, "gone", None);
        assert!(reporter.has_errors());
        assert!(reporter
            .render_text()
            .starts_with("error CASA-IMP-002: gone"));
    }
}
