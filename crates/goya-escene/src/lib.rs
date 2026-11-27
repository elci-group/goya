//! goya-escene — deterministic intent-realisation artefact (GOYA-ESCENE-001).
//!
//! An `.escene` artefact is a deterministic, source-preserving, target-specific
//! record of how Goya realised developer intent: compilation (C), interpretation
//! (I) and substitution (S) annotations backed by structured evidence.
//!
//! Invariants implemented here (ESC-INV-001..013): the artefact carries the exact
//! source bytes, is never required to run an application, is generated from
//! canonical evidence through versioned rules without neural inference, and
//! distinguishes honest unobservation (`I.UNOBSERVED`) from success.

pub mod canonical;
pub mod evidence;
pub mod model;
pub mod registry;
pub mod render;
pub mod rules;
pub mod source;
pub mod build;
pub mod validate;

pub use canonical::canonical_json;
pub use evidence::{Certainty, Evidence, EvidenceKind, Origin};
pub use model::{
    Artefact, Caprichio, CaprichioStatus, Coverage, EvidenceSnapshot, Header, Implementation,
    Severity, Status, Target, GenerationMode, FORMAT_VERSION,
};
pub use registry::{registry, AnnotationClass, DiagnosticId, Emoji, Registry, REGISTRY_VERSION};
pub use render::{render_projection, RenderProfile, RENDERER_VERSION};
pub use rules::{classify, CaprichioApplication, RuleRegistry};
pub use source::{Precision, SourceSnapshot, Span};
pub use build::Builder;
pub use validate::validate;
