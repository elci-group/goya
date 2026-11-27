//! Deterministic classification engine (§12, ESC-INV-004, ESC-INV-008).
//!
//! Rules map evidence predicates to canonical annotation identifiers. Rule
//! evaluation never depends on natural-language interpretation, neural
//! inference or nondeterministic responses. Where the evidence required by a
//! rule is absent, the rule emits nothing — it never invents thresholds.

use crate::evidence::{payload, Certainty, Evidence, EvidenceKind, Origin};
use crate::model::{annotation_sort_key, Annotation, CaprichioStatus, Scope, Severity, Target};
use crate::registry::{registry, AnnotationClass, DiagnosticId};
use crate::source::SourceSnapshot;
use serde_json::json;

/// A classification rule: predicate over typed evidence, emitting annotations.
#[derive(Debug, Clone)]
pub struct Rule {
    pub rule_id: &'static str,
    pub annotation: DiagnosticId,
    pub required_evidence: &'static [EvidenceKind],
    pub severity: Severity,
    pub scope: Scope,
}

/// The versioned rule registry.
pub struct RuleRegistry {
    pub rules: Vec<Rule>,
    pub version: &'static str,
}

impl RuleRegistry {
    pub fn v1() -> RuleRegistry {
        use AnnotationClass::{C, I, S};
        use EvidenceKind as K;
        let rules = vec![
            // Compilation
            Rule {
                rule_id: "RULE C.NATIVE_LOWERING.v1",
                annotation: DiagnosticId::new(C, "NATIVE_LOWERING"),
                required_evidence: &[K::LoweringDecision],
                severity: Severity::Info,
                scope: Scope::Block,
            },
            Rule {
                rule_id: "RULE C.PRECOMPUTED.v1",
                annotation: DiagnosticId::new(C, "PRECOMPUTED"),
                required_evidence: &[K::LoweringDecision],
                severity: Severity::Info,
                scope: Scope::Expression,
            },
            Rule {
                rule_id: "RULE C.EQUIVALENCE_VERIFIED.v1",
                annotation: DiagnosticId::new(C, "EQUIVALENCE_VERIFIED"),
                required_evidence: &[K::VerificationEquivalence],
                severity: Severity::Positive,
                scope: Scope::Block,
            },
            // Interpretation — memory budget exceeded (§12.2)
            Rule {
                rule_id: "RULE I.MEMORY_BUDGET_EXCEEDED.v1",
                annotation: DiagnosticId::new(I, "RESOURCE_SENSITIVE"),
                required_evidence: &[K::RuntimeMemory],
                severity: Severity::Warning,
                scope: Scope::Block,
            },
            // Interpretation — latency budget exceeded
            Rule {
                rule_id: "RULE I.LATENCY_BUDGET_EXCEEDED.v1",
                annotation: DiagnosticId::new(I, "LATENCY_EXCEEDED"),
                required_evidence: &[K::RuntimeLatency],
                severity: Severity::Warning,
                scope: Scope::Expression,
            },
            // Interpretation — measured efficiency
            Rule {
                rule_id: "RULE I.PERFORMANCE_EFFICIENT.v1",
                annotation: DiagnosticId::new(I, "PERFORMANCE_EFFICIENT"),
                required_evidence: &[K::RuntimeLatency],
                severity: Severity::Positive,
                scope: Scope::Expression,
            },
            // Substitution — applied computational reuse (§12.3)
            Rule {
                rule_id: "RULE S.COMPUTATIONAL_REUSE_APPLIED.v1",
                annotation: DiagnosticId::new(S, "COMPUTATIONAL_REUSE"),
                required_evidence: &[K::CaprichioApplication],
                severity: Severity::Info,
                scope: Scope::Block,
            },
            Rule {
                rule_id: "RULE S.PRECOMPUTATION_APPLIED.v1",
                annotation: DiagnosticId::new(S, "PRECOMPUTATION"),
                required_evidence: &[K::CaprichioApplication],
                severity: Severity::Info,
                scope: Scope::Block,
            },
        ];
        RuleRegistry { rules, version: "escene-rules/1.0.0" }
    }

    pub fn digest(&self) -> String {
        // Canonical digest over rule identities and versions.
        let mut materials = String::new();
        for r in &self.rules {
            materials.push_str(r.rule_id);
            materials.push('\n');
        }
        format!("sha256:{}", crate::source::sha256_hex(materials.as_bytes()))
    }
}

/// A Caprichio application event supplied by the subsystem that actually
/// applied the Caprichio (§11.4): the name is never inferred from performance.
#[derive(Debug, Clone)]
pub struct CaprichioApplication {
    pub id: String,
    pub canonical_name: String,
    pub version: String,
    pub configuration_digest: String,
    pub status: CaprichioStatus,
    pub reason_codes: Vec<String>,
    pub implementation_refs: Vec<String>,
    /// Source span bytes the substitution covers.
    pub span: Option<(usize, usize)>,
}

/// Classify an evidence snapshot into deterministic, evidence-backed
/// annotations (§12). Returns annotations in canonical render order.
pub fn classify(
    source: &SourceSnapshot,
    target: &Target,
    snapshot: &crate::model::EvidenceSnapshot,
    applications: &[CaprichioApplication],
    rules: &RuleRegistry,
) -> Vec<Annotation> {
    let reg = registry();
    let mut annotations = Vec::new();
    let mut counter = 0usize;

    for rule in &rules.rules {
        match rule.annotation.0.as_str() {
            "C.NATIVE_LOWERING" => {
                for ev in snapshot.by_kind(EvidenceKind::LoweringDecision) {
                    if let Some((start, end)) = span_fields(ev) {
                        push(&mut annotations, &reg, &mut counter, rule, source, Some((start, end)), ev);
                    }
                }
            }
            "C.PRECOMPUTED" => {
                for ev in snapshot.by_kind(EvidenceKind::LoweringDecision) {
                    if ev.payload.get("decision").and_then(|d| d.as_str()) == Some("precomputed") {
                        if let Some((start, end)) = span_fields(ev) {
                            push(&mut annotations, &reg, &mut counter, rule, source, Some((start, end)), ev);
                        }
                    }
                }
            }
            "C.EQUIVALENCE_VERIFIED" => {
                for ev in snapshot.by_kind(EvidenceKind::VerificationEquivalence) {
                    if ev.payload.get("result").and_then(|r| r.as_str()) == Some("equivalent")
                        && ev.certainty == Certainty::Verified
                    {
                        let span = span_fields(ev);
                        push(&mut annotations, &reg, &mut counter, rule, source, span, ev);
                    }
                }
            }
            "I.RESOURCE_SENSITIVE" => {
                // §12.2: observed > declared budget. No budget, no annotation —
                // the rule does not invent a universal threshold.
                for ev in snapshot.by_kind(EvidenceKind::RuntimeMemory) {
                    let observed = ev.payload.get("observed_bytes").and_then(|v| v.as_u64());
                    let budget = ev.payload.get("budget_bytes").and_then(|v| v.as_u64());
                    if let (Some(observed), Some(budget)) = (observed, budget) {
                        if observed > budget {
                            if let Some((start, end)) = span_fields(ev) {
                                push(&mut annotations, &reg, &mut counter, rule, source, Some((start, end)), ev);
                            }
                        }
                    }
                }
            }
            "I.LATENCY_EXCEEDED" => {
                for ev in snapshot.by_kind(EvidenceKind::RuntimeLatency) {
                    let observed = ev.payload.get("observed_ns").and_then(|v| v.as_u64());
                    let budget = ev.payload.get("budget_ns").and_then(|v| v.as_u64());
                    if let (Some(observed), Some(budget)) = (observed, budget) {
                        if observed > budget {
                            if let Some((start, end)) = span_fields(ev) {
                                push(&mut annotations, &reg, &mut counter, rule, source, Some((start, end)), ev);
                            }
                        }
                    }
                }
            }
            "I.PERFORMANCE_EFFICIENT" => {
                for ev in snapshot.by_kind(EvidenceKind::RuntimeLatency) {
                    let observed = ev.payload.get("observed_ns").and_then(|v| v.as_u64());
                    let budget = ev.payload.get("budget_ns").and_then(|v| v.as_u64());
                    let samples = ev.sample_count.unwrap_or(0);
                    if let (Some(observed), Some(budget)) = (observed, budget) {
                        if observed <= budget && samples >= 10 && ev.certainty == Certainty::Measured {
                            if let Some((start, end)) = span_fields(ev) {
                                push(&mut annotations, &reg, &mut counter, rule, source, Some((start, end)), ev);
                            }
                        }
                    }
                }
            }
            "S.COMPUTATIONAL_REUSE" | "S.PRECOMPUTATION" => {
                let category = if rule.annotation.0 == "S.PRECOMPUTATION" {
                    "precomputation"
                } else {
                    "computational_reuse"
                };
                for app in applications.iter().filter(|a| a.status == CaprichioStatus::Applied) {
                    // Reference the application evidence the producing
                    // subsystem actually emitted; the classifier never invents
                    // evidence records (ESC-INV-005, ESC-INV-010).
                    let applied_ev = snapshot
                        .by_kind(EvidenceKind::CaprichioApplication)
                        .find(|ev| {
                            ev.payload.get("caprichio_ref").and_then(|v| v.as_str()) == Some(app.id.as_str())
                                && ev.payload.get("status").and_then(|v| v.as_str()) == Some("applied")
                                && ev.payload.get("category").and_then(|v| v.as_str()) == Some(category)
                        });
                    let ev = match applied_ev {
                        Some(ev) => ev.clone(),
                        None => continue,
                    };
                    push(&mut annotations, &reg, &mut counter, rule, source, app.span, &ev);
                }
            }
            _ => {}
        }
        let _ = target;
    }

    // §10.3 canonical ordering.
    annotations.sort_by_key(annotation_sort_key);
    annotations
}

fn push(
    annotations: &mut Vec<Annotation>,
    reg: &crate::registry::Registry,
    counter: &mut usize,
    rule: &Rule,
    source: &SourceSnapshot,
    span_bytes: Option<(usize, usize)>,
    ev: &Evidence,
) {
    *counter += 1;
    let span = span_bytes
        .and_then(|(s, e)| crate::source::Span::new(source, s, e, crate::source::Precision::Exact));
    annotations.push(Annotation {
        id: format!("ann-{counter:04}"),
        class: rule.annotation.class().expect("registry id carries class"),
        code: rule.annotation.clone(),
        emoji: reg.emoji_for(&rule.annotation).map(|e| e.0.to_string()).unwrap_or_default(),
        severity: rule.severity,
        scope: rule.scope,
        span,
        rule_id: rule.rule_id.to_string(),
        evidence_refs: vec![ev.id.clone()],
        span_precision: Some(crate::source::Precision::Exact),
    });
}

fn span_fields(ev: &Evidence) -> Option<(usize, usize)> {
    let start = ev.payload.get("span_start_byte").and_then(|v| v.as_u64())? as usize;
    let end = ev.payload.get("span_end_byte").and_then(|v| v.as_u64())? as usize;
    Some((start, end))
}

/// Deterministic rule-evaluation environment handed to `classify`.
pub fn snapshot_with_applications(applications: &[CaprichioApplication]) -> crate::model::EvidenceSnapshot {
    let mut snap = crate::model::EvidenceSnapshot::default();
    for app in applications {
        snap.push(Evidence {
            id: format!("ev-{}", app.id),
            producer: "caprichio".into(),
            origin: Origin::Caprichio,
            certainty: Certainty::Verified,
            kind: EvidenceKind::CaprichioApplication,
            payload: payload(&[
                ("caprichio_ref", json!(app.id)),
                ("status", json!("applied")),
            ]),
            window_start_ns: None,
            window_duration_ns: None,
            sample_count: None,
            parent_refs: vec![],
        });
    }
    snap
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceSnapshot;
    use serde_json::json;

    fn source() -> SourceSnapshot {
        SourceSnapshot::capture("src-1", "Doom.gscene", b"block render_world {\n    resolve_geometry()\n}\n".to_vec())
    }

    fn target() -> Target {
        Target {
            id: "arachnid".into(),
            platform: "webOS".into(),
            toolchain: "goya-arachnid".into(),
            toolchain_version: "0.1.0".into(),
            execution_model: crate::model::ExecutionModel::Compiled,
            configuration_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
            platform_version: None,
            architecture: None,
        }
    }

    #[test]
    fn memory_budget_exceeded_emits_resource_sensitive() {
        let src = source();
        let mut snap = crate::model::EvidenceSnapshot::default();
        snap.push(Evidence {
            id: "ev-02".into(),
            producer: "profiler".into(),
            origin: Origin::Profiler,
            certainty: Certainty::Measured,
            kind: EvidenceKind::RuntimeMemory,
            payload: payload(&[
                ("resource", json!("memory")),
                ("observed_bytes", json!(50_331_648u64)),
                ("budget_bytes", json!(33_554_432u64)),
                ("span_start_byte", json!(0)),
                ("span_end_byte", json!(17)),
            ]),
            window_start_ns: None,
            window_duration_ns: None,
            sample_count: Some(120),
            parent_refs: vec![],
        });
        let out = classify(&src, &target(), &snap, &[], &RuleRegistry::v1());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].code.0, "I.RESOURCE_SENSITIVE");
        assert_eq!(out[0].emoji, "🐘");
        assert_eq!(out[0].severity, Severity::Warning);
    }

    #[test]
    fn no_budget_means_no_annotation() {
        // §12.2: without a declared budget the rule invents no threshold.
        let src = source();
        let mut snap = crate::model::EvidenceSnapshot::default();
        snap.push(Evidence {
            id: "ev-03".into(),
            producer: "profiler".into(),
            origin: Origin::Profiler,
            certainty: Certainty::Measured,
            kind: EvidenceKind::RuntimeMemory,
            payload: payload(&[
                ("resource", json!("memory")),
                ("observed_bytes", json!(u64::MAX / 2)),
                ("span_start_byte", json!(0)),
                ("span_end_byte", json!(17)),
            ]),
            window_start_ns: None,
            window_duration_ns: None,
            sample_count: Some(1),
            parent_refs: vec![],
        });
        let out = classify(&src, &target(), &snap, &[], &RuleRegistry::v1());
        assert!(out.is_empty());
    }

    #[test]
    fn applied_caprichio_only() {
        // An identified-but-unapplied Caprichio yields no substitution
        // annotation (ESC-INV-010).
        let src = source();
        let apps = vec![CaprichioApplication {
            id: "cap-0001".into(),
            canonical_name: "geometry-reuse".into(),
            version: "0.2.0".into(),
            configuration_digest: "sha256:1111111111111111111111111111111111111111111111111111111111111111".into(),
            status: CaprichioStatus::Identified,
            reason_codes: vec!["FINITE_STATE_SPACE".into()],
            implementation_refs: vec![],
            span: Some((22, 39)),
        }];
        let snap = snapshot_with_applications(&apps);
        let out = classify(&src, &target(), &snap, &apps, &RuleRegistry::v1());
        assert!(out.is_empty(), "identified Caprichio must not be reported as applied");
    }
}
