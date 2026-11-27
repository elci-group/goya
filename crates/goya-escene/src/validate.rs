//! Mandatory semantic validation (§8.2).
//!
//! JSON Schema conformance alone does not establish ESCENE conformance; the
//! validator enforces the semantic invariants that cannot be expressed in
//! schema: source recovery, span bounds, reference integrity, identity
//! uniqueness, class/code agreement, emoji registry agreement, evidence
//! sufficiency, projection reproducibility, applied-Caprichio provenance,
//! coverage disclosure and canonical digest reproduction.

use crate::model::{Artefact, CaprichioStatus, CoverageStatus};
use crate::registry::DiagnosticId;
use crate::render::{render_projection, RenderProfile, RENDERER_VERSION};
use crate::rules::RuleRegistry;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub domain: &'static str,
    pub message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.domain, self.message)
    }
}

impl std::error::Error for Violation {}

/// Validate an artefact. An empty result means the artefact conforms.
pub fn validate(artefact: &Artefact, rules: &RuleRegistry) -> Result<(), Vec<Violation>> {
    validate_inner(artefact, rules)
}

fn validate_inner(artefact: &Artefact, rules: &RuleRegistry) -> Result<(), Vec<Violation>> {
    let mut v: Vec<Violation> = Vec::new();

    // Source recovery (ESC-INV-001).
    if let Err(e) = artefact.source.recover() {
        v.push(Violation { domain: "source_recovery", message: e.to_string() });
    }

    // Header identity.
    if artefact.header.format != "escene" || artefact.header.version != crate::model::FORMAT_VERSION {
        v.push(Violation {
            domain: "header",
            message: format!(
                "unexpected format/version: {}/{}",
                artefact.header.format, artefact.header.version
            ),
        });
    }
    if artefact.header.ruleset_digest != rules.digest() {
        v.push(Violation {
            domain: "header",
            message: "ruleset_digest does not match the classification rule registry".into(),
        });
    }

    // Spans within source bounds, start ≤ end (checked again here against the
    // recovered byte length, not just the declared one).
    let source_len = artefact.source.byte_length;
    for ann in &artefact.annotations {
        if let Some(span) = &ann.span {
            if span.start_byte > span.end_byte || span.end_byte > source_len {
                v.push(Violation {
                    domain: "source_spans",
                    message: format!("annotation {} span {}..{} out of bounds", ann.id, span.start_byte, span.end_byte),
                });
            }
        }
        // Class matches the diagnostic code prefix.
        match DiagnosticId::parse(&ann.code.0) {
            Some((class, _)) if class as i32 == ann.class as i32 => {}
            Some((class, _)) => v.push(Violation {
                domain: "annotation_class",
                message: format!("{} declares class mismatch: {:?}", ann.id, class),
            }),
            None => v.push(Violation {
                domain: "annotation_class",
                message: format!("{} has malformed diagnostic code {}", ann.id, ann.code.0),
            }),
        }
        // Emoji matches the declared registry version.
        if let Some(entry) = crate::registry::registry().lookup(&ann.code) {
            if entry.emoji.0 != ann.emoji {
                v.push(Violation {
                    domain: "emoji_registry",
                    message: format!(
                        "{} emoji {:?} does not match registry {:?}",
                        ann.id, ann.emoji, entry.emoji.0
                    ),
                });
            }
        } else {
            v.push(Violation {
                domain: "emoji_registry",
                message: format!("{} code {} unknown to registry {}", ann.id, ann.code.0, artefact.header.registry_version),
            });
        }
        // Evidence sufficiency: every annotation references ≥ 1 evidence record.
        if ann.evidence_refs.is_empty() {
            v.push(Violation {
                domain: "evidence_sufficiency",
                message: format!("annotation {} carries no evidence references", ann.id),
            });
        }
    }

    // Reference integrity and identity uniqueness.
    let evidence_ids: BTreeSet<&str> = artefact.evidence.iter().map(|e| e.id.as_str()).collect();
    let impl_ids: BTreeSet<&str> = artefact.implementations.iter().map(|i| i.id.as_str()).collect();
    for ann in &artefact.annotations {
        for r in &ann.evidence_refs {
            if !evidence_ids.contains(r.as_str()) {
                v.push(Violation {
                    domain: "reference_integrity",
                    message: format!("annotation {} references missing evidence {}", ann.id, r),
                });
            }
        }
    }
    for cap in &artefact.caprichios {
        for r in &cap.evidence_refs {
            if !evidence_ids.contains(r.as_str()) {
                v.push(Violation {
                    domain: "reference_integrity",
                    message: format!("caprichio {} references missing evidence {}", cap.id, r),
                });
            }
        }
        for r in &cap.implementation_refs {
            if !impl_ids.contains(r.as_str()) {
                v.push(Violation {
                    domain: "reference_integrity",
                    message: format!("caprichio {} references missing implementation {}", cap.id, r),
                });
            }
        }
    }

    check_unique(&artefact.evidence.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), "identity_uniqueness", "evidence", &mut v);
    check_unique(&artefact.annotations.iter().map(|a| a.id.clone()).collect::<Vec<_>>(), "identity_uniqueness", "annotation", &mut v);
    check_unique(&artefact.implementations.iter().map(|i| i.id.clone()).collect::<Vec<_>>(), "identity_uniqueness", "implementation", &mut v);
    check_unique(&artefact.caprichios.iter().map(|c| c.id.clone()).collect::<Vec<_>>(), "identity_uniqueness", "caprichio", &mut v);

    // Caprichio status: applied annotations must reference evidence of actual
    // application; a Caprichio recorded as applied must carry application
    // evidence with status "applied" (ESC-INV-010, ESC-G06).
    for cap in &artefact.caprichios {
        if cap.status == CaprichioStatus::Applied {
            let has_applied_evidence = cap.evidence_refs.iter().any(|r| {
                artefact.evidence.iter().any(|e| {
                    e.id == *r && e.payload.get("status").and_then(|s| s.as_str()) == Some("applied")
                })
            });
            if !has_applied_evidence {
                v.push(Violation {
                    domain: "caprichio_status",
                    message: format!("caprichio {} is marked applied without application evidence", cap.id),
                });
            }
        }
    }

    // Coverage: dropped observations must be disclosed alongside limitations.
    for (name, c) in [("C", &artefact.coverage.c), ("I", &artefact.coverage.i), ("S", &artefact.coverage.s)] {
        if c.dropped_events > 0 && c.limitations.is_empty() {
            v.push(Violation {
                domain: "coverage",
                message: format!("class {name} dropped {} events without disclosure", c.dropped_events),
            });
        }
        if c.status == CoverageStatus::NotApplicable && c.dropped_events > 0 {
            v.push(Violation {
                domain: "coverage",
                message: format!("class {name} is not applicable yet reports dropped events"),
            });
        }
    }

    // Projection: must reproduce deterministic rendering of source + annotations.
    if artefact.projection.renderer_version != RENDERER_VERSION {
        v.push(Violation {
            domain: "projection",
            message: format!("renderer version {} != {}", artefact.projection.renderer_version, RENDERER_VERSION),
        });
    }
    let expected = render_projection(&artefact.source, &artefact.annotations, RenderProfile::Standard)
        .map_err(|e| vec![Violation { domain: "projection", message: e }])?;
    if artefact.projection.content != expected {
        v.push(Violation {
            domain: "projection",
            message: "projection content does not match deterministic rendering".into(),
        });
    }
    let expected_digest = format!("sha256:{}", crate::source::sha256_hex(expected.as_bytes()));
    if artefact.projection.digest != expected_digest {
        v.push(Violation {
            domain: "projection",
            message: "projection digest mismatch".into(),
        });
    }

    // Canonical digest reproduction (self field excluded from its own hash).
    let canonical = canonical_bytes_without_digest(artefact);
    let expected = format!("sha256:{}", crate::source::sha256_hex(&canonical));
    if artefact.integrity.canonical_digest != expected {
        v.push(Violation {
            domain: "canonical_digest",
            message: "canonical content does not reproduce the declared digest".into(),
        });
    }

    if v.is_empty() {
        Ok(())
    } else {
        Err(v)
    }
}

fn check_unique(ids: &[String], domain: &'static str, kind: &str, v: &mut Vec<Violation>) {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            v.push(Violation { domain, message: format!("duplicate {kind} id {id}") });
        }
    }
}

/// Serialise the artefact for digesting with the canonical_digest field blanked.
fn canonical_bytes_without_digest(artefact: &Artefact) -> Vec<u8> {
    let mut clone = artefact.clone();
    clone.integrity.canonical_digest = String::new();
    crate::canonical::canonical_json(&clone)
        .expect("artefact serialisation cannot fail for in-memory values")
        .into_bytes()
}

/// Produce the canonical bytes of a complete artefact.
pub fn canonical_bytes(artefact: &Artefact) -> Result<Vec<u8>, String> {
    crate::canonical::canonical_json(artefact).map(|s| s.into_bytes()).map_err(|e| e.to_string())
}
