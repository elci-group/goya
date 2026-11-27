//! Structured evidence records (ESC-INV-005, ESC-INV-009).
//!
//! Every annotation must reference at least one evidence record. Unobserved
//! behaviour is never represented as successful behaviour: where evidence could
//! not be collected, the certainty is `unavailable` or the annotation is
//! `I.UNOBSERVED` / `I.INSUFFICIENT_EVIDENCE`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where the evidence came from. Producers do not classify; the shared
/// classification engine does (§11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Compiler,
    Resolver,
    NativeRuntime,
    Profiler,
    Caprichio,
    Verification,
    Environment,
}

/// Evidence certainty model (§7.5): agents must distinguish verified facts
/// from estimates and hypotheses without natural-language interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Verified,
    Measured,
    Derived,
    Estimated,
    Inferred,
    Unavailable,
}

/// Standard evidence vocabulary (§9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    LoweringDecision,
    DependencyResolution,
    RuntimeInvocation,
    RuntimeLatency,
    RuntimeMemory,
    RuntimeRedundancy,
    RuntimeExposure,
    RuntimeContention,
    CaprichioApplication,
    VerificationEquivalence,
    VerificationCorrectness,
    EnvironmentCapability,
}

/// A single structured observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub producer: String,
    pub origin: Origin,
    pub certainty: Certainty,
    pub kind: EvidenceKind,
    /// Typed per-kind payload; extensible until per-kind schemas are frozen
    /// (§8.2). Keys and values are ordered for canonical serialisation.
    pub payload: BTreeMap<String, serde_json::Value>,
    /// Nanosecond observation window; decimal strings preserve precision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_start_ns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_duration_ns: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_count: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parent_refs: Vec<String>,
}

/// Convenience constructor for payload building.
pub fn payload(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resource_sensitivity_payload_shape() {
        // Illustrative payload from directive §9.2.
        let ev = Evidence {
            id: "ev-02".into(),
            producer: "arachnid-compiler".into(),
            origin: Origin::Profiler,
            certainty: Certainty::Measured,
            kind: EvidenceKind::RuntimeMemory,
            payload: payload(&[
                ("resource", json!("memory")),
                ("metric", json!("peak_attributed_allocation")),
                ("observed_bytes", json!(50_331_648u64)),
                ("budget_bytes", json!(33_554_432u64)),
                ("attribution", json!("instrumented_allocator")),
                ("sample_count", json!(120)),
                ("baseline_available", json!(false)),
            ]),
            window_start_ns: None,
            window_duration_ns: None,
            sample_count: Some(120),
            parent_refs: vec![],
        };
        assert_eq!(ev.payload["observed_bytes"], json!(50_331_648u64));
    }
}
