//! GY-PROVENANCE/1 — Text provenance and bounded diagnostic lineage.
//!
//! Deep Copy operates on the selected text's provenance graph, not a dump of
//! recent log lines. Bundles are bounded, redacted and consent-aware
//! (GY-CHR-001 §6).

mod redaction;

pub use redaction::{redact, RedactionRecord};

use std::fmt;

/// Contract identifier implemented by this crate.
pub const PROVENANCE_CONTRACT: &str = "GY-PROVENANCE/1";

/// Identifies the originating UI element of a selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceReference {
    /// Stable object id of the source element.
    pub object_id: String,
    /// Component that owns the element.
    pub component: String,
    /// Application state revision at selection time.
    pub state_revision: u64,
    /// Executable module version responsible for the content.
    pub module_version: String,
}

/// A character range within the source object's text content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    /// Byte offset of the selection start (inclusive).
    pub start: usize,
    /// Byte offset of the selection end (exclusive).
    pub end: usize,
}

impl SelectionRange {
    pub fn new(start: usize, end: usize) -> Self {
        SelectionRange {
            start: start.min(end),
            end: end.max(start),
        }
    }

    /// Extract the selected bytes from `text`. Out-of-range offsets are
    /// clamped; the operation never panics.
    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        let start = self.start.min(text.len());
        let end = self.end.clamp(start, text.len());
        // Snap to char boundaries to avoid slicing mid-codepoint.
        let mut start = start;
        let mut end = end;
        while !text.is_char_boundary(start) {
            start -= 1;
        }
        while !text.is_char_boundary(end) && end < text.len() {
            end += 1;
        }
        &text[start..end]
    }
}

/// Correlates operations across modules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraceId(pub String);

/// One causal diagnostic record linked to the selection.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticRecord {
    pub event: String,
    pub component: String,
    pub reason: String,
    pub trace_id: TraceId,
    pub timestamp: String,
}

/// Bounding limits for bundle assembly — Deep Copy must be consent-aware and
/// size-bounded, never an unbounded dump.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeepCopyLimits {
    pub max_bytes: usize,
    pub include_traces: bool,
    pub include_logs: bool,
}

impl Default for DeepCopyLimits {
    fn default() -> Self {
        DeepCopyLimits {
            max_bytes: 8 * 1024,
            include_traces: true,
            include_logs: true,
        }
    }
}

/// The provenance-bearing payload of a Deep Copy.
#[derive(Debug, Clone, PartialEq)]
pub struct DeepCopyBundle {
    /// Exact visible selected content.
    pub text: String,
    pub source: SourceReference,
    pub trace_ids: Vec<TraceId>,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub redactions: Vec<RedactionRecord>,
}

impl fmt::Display for DeepCopyBundle {
    /// Plain-text rendering that remains useful in ordinary editors and
    /// issue trackers (GY-CHR-001 §6).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.text)?;
        writeln!(f)?;
        writeln!(f, "--- GOYA DIAGNOSTIC CONTEXT ---")?;
        writeln!(
            f,
            "Source: {} ({})",
            self.source.object_id, self.source.component
        )?;
        writeln!(f, "State revision: {}", self.source.state_revision)?;
        writeln!(f, "Module version: {}", self.source.module_version)?;
        for d in &self.diagnostics {
            writeln!(f, "Event: {}", d.event)?;
            writeln!(f, "Component: {}", d.component)?;
            writeln!(f, "Reason: {}", d.reason)?;
            writeln!(f, "Trace: {}", d.trace_id.0)?;
            writeln!(f, "Timestamp: {}", d.timestamp)?;
        }
        for r in &self.redactions {
            writeln!(f, "Redacted: {}", r.reason)?;
        }
        write!(f, "--- END CONTEXT ---")
    }
}

/// Errors from bundle assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleError {
    /// The selected text alone exceeds the byte budget.
    SelectionTooLarge,
    /// The principal has not consented to including the requested context.
    ConsentDenied,
}

/// Assemble a Deep Copy bundle from a selection and its provenance context.
/// Redaction is applied first; the byte budget is enforced on the final
/// rendered form (truncating diagnostics before text).
pub fn build_bundle(
    text: &str,
    source: SourceReference,
    mut diagnostics: Vec<DiagnosticRecord>,
    limits: &DeepCopyLimits,
) -> Result<DeepCopyBundle, BundleError> {
    if !limits.include_traces {
        for d in &mut diagnostics {
            d.trace_id = TraceId(String::new());
        }
    }
    if !limits.include_logs {
        diagnostics.clear();
    }

    let (text, redactions) = redact(text);

    // Enforce the budget by dropping diagnostics oldest-last until the
    // rendered bundle fits; if even the bare text does not fit, fail loudly.
    let mut bundle = DeepCopyBundle {
        text,
        source,
        trace_ids: diagnostics.iter().map(|d| d.trace_id.clone()).collect(),
        diagnostics,
        redactions,
    };
    if bundle.text.len() > limits.max_bytes {
        return Err(BundleError::SelectionTooLarge);
    }
    while bundle.diagnostics.len() > 1 && bundle.render_len() > limits.max_bytes {
        bundle.diagnostics.remove(0);
        bundle.trace_ids.truncate(bundle.diagnostics.len());
    }
    if bundle.render_len() > limits.max_bytes {
        bundle.diagnostics.clear();
        bundle.trace_ids.clear();
    }
    Ok(bundle)
}

impl DeepCopyBundle {
    /// Length of the plain-text rendering in bytes.
    pub fn render_len(&self) -> usize {
        self.to_string().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceReference {
        SourceReference {
            object_id: "text-node-7".into(),
            component: "goya-module".into(),
            state_revision: 42,
            module_version: "0.1.0".into(),
        }
    }

    fn diag(event: &str) -> DiagnosticRecord {
        DiagnosticRecord {
            event: event.into(),
            component: "goya-module".into(),
            reason: "state_schema_mismatch".into(),
            trace_id: TraceId(format!("gy-{event}")),
            timestamp: "2026-11-04T02:00:00Z".into(),
        }
    }

    #[test]
    fn selection_clamps_and_snaps_to_boundaries() {
        let text = "héllo wörld";
        let sel = SelectionRange::new(0, 100);
        assert_eq!(sel.slice(text), text);
        let mid = SelectionRange::new(1, 2); // inside 'é'
        assert_eq!(mid.slice(text), "é");
    }

    #[test]
    fn bundle_renders_context_block() {
        let b = build_bundle(
            "Module compilation failed.",
            source(),
            vec![diag("module.swap.rejected")],
            &DeepCopyLimits::default(),
        )
        .unwrap();
        let s = b.to_string();
        assert!(s.starts_with("Module compilation failed."));
        assert!(s.contains("--- GOYA DIAGNOSTIC CONTEXT ---"));
        assert!(s.contains("Trace: gy-module.swap.rejected"));
        assert!(s.ends_with("--- END CONTEXT ---"));
    }

    #[test]
    fn budget_truncates_diagnostics_not_text() {
        let diags: Vec<_> = (0..50).map(|i| diag(&format!("event.{i}"))).collect();
        let b = build_bundle(
            "keep me",
            source(),
            diags,
            &DeepCopyLimits {
                max_bytes: 600,
                ..DeepCopyLimits::default()
            },
        )
        .unwrap();
        assert!(b.render_len() <= 600, "rendered {} bytes", b.render_len());
        assert_eq!(b.text, "keep me");
    }

    #[test]
    fn oversized_selection_fails_loudly() {
        let err = build_bundle(
            &"x".repeat(100),
            source(),
            vec![],
            &DeepCopyLimits {
                max_bytes: 10,
                ..DeepCopyLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(err, BundleError::SelectionTooLarge);
    }

    #[test]
    fn secrets_are_redacted() {
        let b = build_bundle(
            "token=sk-live-abcdefghijklmnop123456 ok",
            source(),
            vec![],
            &DeepCopyLimits::default(),
        )
        .unwrap();
        assert!(!b.text.contains("sk-live-abcdefghijklmnop123456"));
        assert!(!b.redactions.is_empty());
    }

    #[test]
    fn logs_can_be_excluded() {
        let b = build_bundle(
            "text",
            source(),
            vec![diag("e")],
            &DeepCopyLimits {
                include_logs: false,
                ..DeepCopyLimits::default()
            },
        )
        .unwrap();
        assert!(b.diagnostics.is_empty());
    }
}
