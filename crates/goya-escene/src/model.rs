//! The `.escene` artefact model (§7, §8). Record collections use ordered
//! containers so identical evidence yields byte-identical output (ESC-INV-004).

use crate::evidence::{Certainty, Evidence, EvidenceKind, Origin};
use crate::registry::DiagnosticId;
use crate::source::{Precision, SourceSnapshot, Span};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationMode {
    Compilation,
    Interpretation,
    Hybrid,
    Composite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Complete,
    Partial,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub format: String, // always "escene"
    pub version: String,
    pub registry_version: String,
    pub ruleset_digest: String,
    pub generation_mode: GenerationMode,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub platform: String,
    pub toolchain: String,
    pub toolchain_version: String,
    /// Toolchain is identified separately from the operating platform (§6.5).
    pub execution_model: ExecutionModel,
    pub configuration_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModel {
    Compiled,
    Interpreted,
    Jit,
    Hybrid,
}

/// Per-class observation availability (§7.1 `coverage`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassCoverage {
    pub status: CoverageStatus,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub limitations: Vec<String>,
    /// Dropped evidence is disclosed, never silently lost (ESC-G12).
    #[serde(skip_serializing_if = "is_zero", default)]
    pub dropped_events: u64,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatus {
    Complete,
    Partial,
    Unavailable,
    NotApplicable,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    #[serde(rename = "C")]
    pub c: ClassCoverage,
    #[serde(rename = "I")]
    pub i: ClassCoverage,
    #[serde(rename = "S")]
    pub s: ClassCoverage,
}

impl Coverage {
    pub fn full() -> Coverage {
        let ok = || ClassCoverage { status: CoverageStatus::Complete, limitations: vec![], dropped_events: 0 };
        Coverage { c: ok(), i: ok(), s: ok() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Implementation {
    pub id: String,
    pub kind: ImplementationKind,
    pub producer: String,
    pub description_code: String,
    pub source_spans: Vec<Span>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub generated_symbols: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationKind {
    Native,
    Synthesised,
    Hybrid,
    Fallback,
    Precomputed,
    Eliminated,
}

/// Lifecycle status of a Caprichio: identified ≠ applied (ESC-INV-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaprichioStatus {
    Identified,
    Selected,
    Verified,
    Applied,
    Rejected,
    Reverted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caprichio {
    pub id: String,
    pub canonical_name: String,
    pub version: String,
    pub configuration_digest: String,
    pub status: CaprichioStatus,
    pub reason_codes: Vec<String>,
    pub implementation_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
}

/// Severity is independent of annotation class and emoji (§7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Trace,
    Info,
    Positive,
    Notice,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Expression,
    Statement,
    Block,
    Scene,
    File,
    Application,
    Environment,
}

/// A source-correlated classified finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    /// Deterministic record id, e.g. `ann-0007` (referenced as `I:0007`).
    pub id: String,
    pub class: crate::registry::AnnotationClass,
    pub code: DiagnosticId,
    pub emoji: String,
    pub severity: Severity,
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    pub rule_id: String,
    pub evidence_refs: Vec<String>,
    /// Attribution precision is declared; approximate attribution is never
    /// dressed up as exact (§7.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_precision: Option<Precision>,
}

impl Annotation {
    /// Display tag used in the projection, e.g. `I:0007`.
    pub fn display_tag(&self) -> String {
        let n = self.id.trim_start_matches("ann-");
        format!("{}:{}", match self.class {
            crate::registry::AnnotationClass::C => "C",
            crate::registry::AnnotationClass::I => "I",
            crate::registry::AnnotationClass::S => "S",
        }, n)
    }
}

/// Deterministic identity for an annotation, derived from its content.
pub fn annotation_sort_key(a: &Annotation) -> (usize, usize, u8, String, String) {
    let span_start = a.span.as_ref().map(|s| s.start_byte).unwrap_or(usize::MAX);
    let span_len = a.span.as_ref().map(|s| s.end_byte.saturating_sub(s.start_byte)).unwrap_or(0);
    let class_rank = match a.class {
        crate::registry::AnnotationClass::C => 0u8,
        crate::registry::AnnotationClass::I => 1,
        crate::registry::AnnotationClass::S => 2,
    };
    // §10.3 render order: offset, longest enclosing span first, C/I/S, code, id.
    (span_start, usize::MAX - span_len, class_rank, a.code.0.clone(), a.id.clone())
}

/// The complete artefact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artefact {
    pub header: Header,
    pub source: SourceSnapshot,
    pub target: Target,
    pub coverage: Coverage,
    pub implementations: Vec<Implementation>,
    pub caprichios: Vec<Caprichio>,
    pub annotations: Vec<Annotation>,
    pub evidence: Vec<Evidence>,
    pub projection: Projection,
    pub integrity: Integrity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    pub renderer_version: String,
    pub encoding: String, // "utf-8"
    pub content: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integrity {
    pub canonical_digest: String,
}

/// Evidence snapshot handed to the classifier (§11.1 `EsceneClassifier`).
#[derive(Debug, Clone, Default)]
pub struct EvidenceSnapshot {
    pub records: Vec<Evidence>,
}

impl EvidenceSnapshot {
    pub fn push(&mut self, ev: Evidence) {
        self.records.push(ev);
    }

    pub fn by_kind(&self, kind: EvidenceKind) -> impl Iterator<Item = &Evidence> {
        self.records.iter().filter(move |e| e.kind == kind)
    }

    pub fn by_certainty(&self, c: Certainty) -> impl Iterator<Item = &Evidence> {
        self.records.iter().filter(move |e| e.certainty == c)
    }

    pub fn from_origin(&self, o: Origin) -> impl Iterator<Item = &Evidence> {
        self.records.iter().filter(move |e| e.origin == o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_tag_matches_directive_examples() {
        let ann = Annotation {
            id: "ann-0007".into(),
            class: crate::registry::AnnotationClass::I,
            code: DiagnosticId::new(crate::registry::AnnotationClass::I, "RESOURCE_SENSITIVE"),
            emoji: "🐘".into(),
            severity: Severity::Warning,
            scope: Scope::Block,
            span: None,
            rule_id: "RULE I.MEMORY_BUDGET_EXCEEDED.v1".into(),
            evidence_refs: vec!["ev-02".into()],
            span_precision: None,
        };
        assert_eq!(ann.display_tag(), "I:0007");
    }
}
