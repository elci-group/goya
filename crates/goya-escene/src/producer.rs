//! Shared evidence producer contract (§11.1).
//!
//! Producers emit typed events; the shared classifier determines annotations.
//! Producers never decide emoji meaning or final emojification. The disabled
//! path is inert: no buffers, no payloads, no persistence (§11.5).

use crate::evidence::{payload, Certainty, Evidence, EvidenceKind, Origin};
use crate::model::EvidenceSnapshot;
use serde_json::json;

/// Outcome of submitting an event (§11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitOutcome {
    Accepted,
    Aggregated,
    DroppedByBudget,
    Disabled,
}

/// Typed producer events (§11.2–11.4).
#[derive(Debug, Clone)]
pub enum EsceneEvent {
    /// Compiler lifecycle events.
    Compilation(CompilationEvent),
    /// Resolver/runtime lifecycle events.
    Interpretation(InterpretationEvent),
    /// Caprichio lifecycle events.
    Substitution(SubstitutionEvent),
}

/// §11.2 compiler events.
#[derive(Debug, Clone)]
pub enum CompilationEvent {
    SourceAccepted {
        source_id: String,
        byte_length: usize,
    },
    ConstructLowered {
        span: (usize, usize),
        decision: &'static str,
        pass: String,
        implementation_ref: String,
    },
    DependencyResolved {
        name: String,
        status: &'static str,
    },
    ImplementationGenerated {
        implementation_ref: String,
        kind: &'static str,
    },
    TranslationFailed {
        span: Option<(usize, usize)>,
        reason_code: String,
    },
    OutputFinalised {
        output_digest: String,
    },
}

/// §11.3 resolver events (aggregated, never per-call by default).
#[derive(Debug, Clone)]
pub enum InterpretationEvent {
    InvocationAggregated {
        span: (usize, usize),
        invocation_count: u64,
        window_ns: String,
    },
    LatencyObserved {
        span: (usize, usize),
        observed_ns: u64,
        budget_ns: u64,
        sample_count: u64,
    },
    ResourceObserved {
        span: (usize, usize),
        resource: &'static str,
        observed_bytes: u64,
        budget_bytes: u64,
        sample_count: u64,
    },
    ExecutionFailed {
        span: Option<(usize, usize)>,
        reason_code: String,
    },
}

/// §11.4 Caprichio events. Application evidence is emitted where the
/// replacement becomes effective; selection alone is never "applied".
#[derive(Debug, Clone)]
pub enum SubstitutionEvent {
    CandidateIdentified {
        caprichio_ref: String,
        reason_codes: Vec<String>,
    },
    SubstitutionApplied {
        caprichio_ref: String,
        category: &'static str,
        span: Option<(usize, usize)>,
        replacement_implementation_refs: Vec<String>,
    },
    CandidateRejected {
        caprichio_ref: String,
        reason_codes: Vec<String>,
    },
    SubstitutionReverted {
        caprichio_ref: String,
        reason_codes: Vec<String>,
    },
}

/// Convert an event into its evidence record(s). Conversion is total and
/// deterministic; no evidence is invented for missing fields — callers that
/// lack a value simply do not emit the event.
pub fn event_to_evidence(id: &str, producer: &str, event: &EsceneEvent) -> Vec<Evidence> {
    let mk = |origin: Origin,
              certainty: Certainty,
              kind: EvidenceKind,
              pairs: &[(&str, serde_json::Value)]| Evidence {
        id: id.to_string(),
        producer: producer.to_string(),
        origin,
        certainty,
        kind,
        payload: payload(pairs),
        window_start_ns: None,
        window_duration_ns: None,
        sample_count: None,
        parent_refs: vec![],
    };
    match event {
        EsceneEvent::Compilation(CompilationEvent::ConstructLowered {
            span,
            decision,
            pass,
            implementation_ref,
        }) => vec![mk(
            Origin::Compiler,
            Certainty::Verified,
            EvidenceKind::LoweringDecision,
            &[
                ("decision", json!(decision)),
                ("pass", json!(pass)),
                ("implementation_ref", json!(implementation_ref)),
                ("span_start_byte", json!(span.0)),
                ("span_end_byte", json!(span.1)),
            ],
        )],
        EsceneEvent::Compilation(CompilationEvent::DependencyResolved { name, status }) => vec![mk(
            Origin::Compiler,
            Certainty::Verified,
            EvidenceKind::DependencyResolution,
            &[("dependency", json!(name)), ("status", json!(status))],
        )],
        EsceneEvent::Compilation(CompilationEvent::OutputFinalised { output_digest }) => vec![mk(
            Origin::Compiler,
            Certainty::Verified,
            EvidenceKind::EnvironmentCapability,
            &[("output_digest", json!(output_digest))],
        )],
        EsceneEvent::Interpretation(InterpretationEvent::LatencyObserved {
            span,
            observed_ns,
            budget_ns,
            sample_count,
        }) => vec![mk(
            Origin::Resolver,
            Certainty::Measured,
            EvidenceKind::RuntimeLatency,
            &[
                ("observed_ns", json!(observed_ns)),
                ("budget_ns", json!(budget_ns)),
                ("span_start_byte", json!(span.0)),
                ("span_end_byte", json!(span.1)),
            ],
        )
        .with_sample_count(*sample_count)],
        EsceneEvent::Interpretation(InterpretationEvent::ResourceObserved {
            span,
            resource,
            observed_bytes,
            budget_bytes,
            sample_count,
        }) => vec![mk(
            Origin::Profiler,
            Certainty::Measured,
            EvidenceKind::RuntimeMemory,
            &[
                ("resource", json!(resource)),
                ("observed_bytes", json!(observed_bytes)),
                ("budget_bytes", json!(budget_bytes)),
                ("span_start_byte", json!(span.0)),
                ("span_end_byte", json!(span.1)),
            ],
        )
        .with_sample_count(*sample_count)],
        EsceneEvent::Interpretation(InterpretationEvent::InvocationAggregated {
            span,
            invocation_count,
            window_ns,
        }) => vec![mk(
            Origin::Resolver,
            Certainty::Measured,
            EvidenceKind::RuntimeInvocation,
            &[
                ("invocation_count", json!(invocation_count)),
                ("window_ns", json!(window_ns)),
                ("span_start_byte", json!(span.0)),
                ("span_end_byte", json!(span.1)),
            ],
        )],
        EsceneEvent::Substitution(SubstitutionEvent::SubstitutionApplied {
            caprichio_ref,
            category,
            span,
            replacement_implementation_refs,
        }) => {
            let mut pairs = vec![
                ("caprichio_ref", json!(caprichio_ref)),
                ("status", json!("applied")),
                ("category", json!(category)),
            ];
            if let Some((s, e)) = span {
                pairs.push(("span_start_byte", json!(s)));
                pairs.push(("span_end_byte", json!(e)));
            }
            vec![mk(
                Origin::Caprichio,
                Certainty::Verified,
                EvidenceKind::CaprichioApplication,
                &pairs,
            )
            .with_refs(replacement_implementation_refs.clone())]
        }
        // Identified/rejected/reverted events carry no classification evidence
        // yet; they surface through Caprichio records, not annotations.
        EsceneEvent::Compilation(CompilationEvent::SourceAccepted { .. })
        | EsceneEvent::Compilation(CompilationEvent::ImplementationGenerated { .. })
        | EsceneEvent::Compilation(CompilationEvent::TranslationFailed { .. })
        | EsceneEvent::Interpretation(InterpretationEvent::ExecutionFailed { .. })
        | EsceneEvent::Substitution(_) => vec![],
    }
}

impl Evidence {
    fn with_sample_count(mut self, n: u64) -> Evidence {
        self.sample_count = Some(n);
        self
    }

    fn with_refs(mut self, refs: Vec<String>) -> Evidence {
        self.parent_refs = refs;
        self
    }
}

/// Bounded evidence collector (§17.2): accepts events, aggregates under a
/// byte budget, and discloses every dropped event.
#[derive(Debug)]
pub struct Collector {
    pub producer: String,
    pub buffer_bytes: usize,
    pub enabled: bool,
    snapshot: EvidenceSnapshot,
    next_id: u64,
    dropped_events: u64,
    used_bytes: usize,
}

impl Collector {
    /// A disabled collector is inert: `emit` always returns `Disabled` and no
    /// allocation occurs beyond the (zero-capacity) struct itself.
    pub fn disabled(producer: &str) -> Collector {
        Collector {
            producer: producer.to_string(),
            buffer_bytes: 0,
            enabled: false,
            snapshot: EvidenceSnapshot::default(),
            next_id: 0,
            dropped_events: 0,
            used_bytes: 0,
        }
    }

    pub fn enabled(producer: &str, buffer_bytes: usize) -> Collector {
        Collector {
            producer: producer.to_string(),
            buffer_bytes,
            enabled: true,
            snapshot: EvidenceSnapshot::default(),
            next_id: 0,
            dropped_events: 0,
            used_bytes: 0,
        }
    }

    /// Submit an event. Budget exhaustion drops the event and counts it for
    /// coverage disclosure — never silent loss (ESC-G12).
    pub fn emit(&mut self, event: &EsceneEvent) -> EmitOutcome {
        if !self.enabled {
            return EmitOutcome::Disabled;
        }
        let records = event_to_evidence(&format!("ev-{:04}", self.next_id + 1), &self.producer, event);
        if records.is_empty() {
            return EmitOutcome::Aggregated;
        }
        let size: usize = serde_json::to_vec(&records).map(|v| v.len()).unwrap_or(usize::MAX);
        if self.used_bytes + size > self.buffer_bytes {
            self.dropped_events += 1;
            return EmitOutcome::DroppedByBudget;
        }
        self.next_id += records.len() as u64;
        self.used_bytes += size;
        for r in records {
            self.snapshot.push(r);
        }
        EmitOutcome::Accepted
    }

    pub fn snapshot(&self) -> &EvidenceSnapshot {
        &self.snapshot
    }

    pub fn dropped_events(&self) -> u64 {
        self.dropped_events
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_collector_is_inert() {
        let mut c = Collector::disabled("x");
        let out = c.emit(&EsceneEvent::Interpretation(InterpretationEvent::LatencyObserved {
            span: (0, 10),
            observed_ns: 5,
            budget_ns: 10,
            sample_count: 1,
        }));
        assert_eq!(out, EmitOutcome::Disabled);
        assert_eq!(c.snapshot().records.len(), 0);
        assert_eq!(c.used_bytes(), 0);
    }

    #[test]
    fn budget_exhaustion_drops_and_discloses() {
        let mut c = Collector::enabled("x", 512); // small budget
        let mut outcomes = Vec::new();
        for _ in 0..4 {
            outcomes.push(c.emit(&EsceneEvent::Interpretation(InterpretationEvent::ResourceObserved {
                span: (0, 10),
                resource: "memory",
                observed_bytes: 100,
                budget_bytes: 50,
                sample_count: 3,
            })));
        }
        assert!(outcomes.contains(&EmitOutcome::Accepted));
        assert!(outcomes.contains(&EmitOutcome::DroppedByBudget));
        assert!(c.dropped_events() > 0, "drops must be counted for coverage disclosure");
    }

    #[test]
    fn applied_substitution_carries_application_evidence() {
        let mut c = Collector::enabled("gvi", 4096);
        c.emit(&EsceneEvent::Substitution(SubstitutionEvent::SubstitutionApplied {
            caprichio_ref: "cap-0001".into(),
            category: "computational_reuse",
            span: Some((0, 10)),
            replacement_implementation_refs: vec!["impl-2".into()],
        }));
        let ev = &c.snapshot().records[0];
        assert_eq!(ev.payload["status"], json!("applied"));
        assert_eq!(ev.payload["caprichio_ref"], json!("cap-0001"));
        // Identified-only events produce no application evidence.
        let mut c2 = Collector::enabled("gvi", 4096);
        let out = c2.emit(&EsceneEvent::Substitution(SubstitutionEvent::CandidateIdentified {
            caprichio_ref: "cap-0002".into(),
            reason_codes: vec!["MAY_HELP".into()],
        }));
        assert_eq!(out, EmitOutcome::Aggregated);
        assert!(c2.snapshot().records.is_empty());
    }
}
