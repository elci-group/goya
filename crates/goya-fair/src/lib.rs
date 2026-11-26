//! GY-FAIR/1 — Foreign Application Intermediate Representation.
//!
//! Schema, validation and deterministic serialisation for the Disparates
//! programme. FAIR records what semantic extraction understood about a foreign
//! application, including — explicitly — what it did not understand.
//!
//! This crate intentionally contains no `goya-ir` types: FAIR preserves foreign
//! semantics; the FAIR→Goya boundary is a future contract. See
//! `docs/disparates/contracts/fair-1.md`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// FAIR contract identifier.
pub const FAIR_CONTRACT: &str = "GY-FAIR/1";

/// Top-level FAIR document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairDocument {
    /// Contract identifier; must equal [`FAIR_CONTRACT`].
    pub contract: String,
    /// Foreign project identity: name, framework detections, entry points.
    pub project: ProjectProvenance,
    /// Binding of this document to foreign source bytes.
    pub source: SourceManifest,
    /// Identity of the extractor that produced this document.
    pub extractor: ExtractorIdentity,
    /// Extracted elements, in extractor emission order. Canonical output is
    /// sorted by element ID; storage order is preserved as emitted.
    pub elements: Vec<FairElement>,
}

/// Foreign project identity recorded at discovery time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectProvenance {
    /// Foreign project name (from its own manifest where available).
    pub name: String,
    /// Framework detections, each with the evidence that produced it.
    pub frameworks: Vec<FrameworkDetection>,
    /// Entry points declared or discovered.
    pub entry_points: Vec<String>,
}

/// One framework detection with its evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkDetection {
    /// Detected framework (for example `electron`, `react`).
    pub framework: String,
    /// What produced the detection (for example `dependency electron ^31.0.1`).
    pub evidence: String,
}

/// Hash binding of the foreign source tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceManifest {
    /// Foreign project root, as presented to the extractor.
    pub root: String,
    /// One entry per extracted source file; SHA-256 over file bytes.
    pub files: Vec<SourceFile>,
}

/// One file in the source manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    /// Path relative to the foreign project root.
    pub path: String,
    /// Lowercase hex SHA-256 of the file bytes.
    pub sha256: String,
}

/// Identity of the extractor that produced a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractorIdentity {
    /// Extractor name (for example `goya-disparate-electron`).
    pub name: String,
    /// Extractor version.
    pub version: String,
    /// Statement of the element-ID assignment rule, so IDs are reproducible.
    pub id_rule: String,
}

/// One extracted foreign construct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairElement {
    /// Extractor-assigned stable ID, unique within the document.
    pub id: String,
    /// Element kind from the versioned vocabulary.
    pub kind: ElementKind,
    /// Source provenance.
    pub provenance: ElementProvenance,
    /// Representability classification.
    pub representability: Representability,
    /// Open semantics payload. Records what was understood — including
    /// unresolved and unknown facts, which must never be dropped.
    pub semantics: BTreeMap<String, SemanticsValue>,
}

/// Versioned element-kind vocabulary. Unobserved constructs are `Unknown`
/// with the observed spelling recorded under the `observed` semantics key.
/// The vocabulary grows by contract revision only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    /// UI component or widget declaration.
    Component,
    /// Hook, composable or reactive accessor.
    Hook,
    /// Event handler or interaction callback.
    EventHandler,
    /// Inter-process channel (for example Electron IPC).
    IpcChannel,
    /// Preload/exposure bridge between privilege domains.
    PreloadBridge,
    /// Operating-system API call.
    OsApiCall,
    /// Child process management.
    ChildProcess,
    /// Filesystem access or persistence operation.
    FilesystemOperation,
    /// Window, view or scene lifecycle management.
    WindowLifecycle,
    /// Native addon or binary module.
    NativeModule,
    /// Dynamic module resolution (for example dynamic `require()`).
    DynamicModuleResolution,
    /// Application state store or stateful service.
    StateStore,
    /// Visual declaration, layout or styling rule.
    StyleRule,
    /// Construct outside the versioned vocabulary.
    Unknown,
}

/// Element source provenance. Origins are attributed to the most precise
/// available enclosing location; fallback origins are marked, not fabricated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementProvenance {
    /// Path relative to the foreign project root.
    pub file: String,
    /// 1-based line number.
    pub line: u64,
    /// Start of byte span, end-exclusive UTF-8; present only when known.
    pub span_start: Option<u64>,
    /// End of byte span, end-exclusive UTF-8; present only when known.
    pub span_end: Option<u64>,
    /// True when the origin is a fallback rather than the precise site.
    pub fallback_origin: bool,
}

/// Representability classification: an evidence progression, not a percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepresentabilityStatus {
    /// Foreign construct identified and recorded.
    Extracted,
    /// Existing Goya semantics can express it.
    Representable,
    /// Native Goya source generated.
    Translated,
    /// Behavioural equivalence demonstrated. Unreachable under this revision.
    Verified,
    /// Explicit intervention necessary.
    RemediationRequired,
    /// Required Goya capability absent.
    Unrepresentable,
    /// Insufficient evidence to establish semantics.
    Undetermined,
}

/// Representability record for one element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Representability {
    /// Classification state.
    pub status: RepresentabilityStatus,
    /// Machine-readable reason code (uppercase snake case). Required for every
    /// status except [`RepresentabilityStatus::Extracted`].
    pub reason: Option<String>,
    /// Free-form human detail; the code stays register-bound.
    pub detail: Option<String>,
}

/// A value in the open semantics payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SemanticsValue {
    /// String value.
    Str(String),
    /// Integer value.
    Int(i64),
    /// Boolean value.
    Bool(bool),
    /// Ordered list of values.
    List(Vec<SemanticsValue>),
    /// Map of values; keys are sorted in canonical output.
    Map(BTreeMap<String, SemanticsValue>),
}

/// A structural validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Element ID the failure concerns, or `None` for document-level failures.
    pub element: Option<String>,
    /// Machine-readable failure code.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

impl FairDocument {
    /// Structural validation per GY-FAIR/1 invariants.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if self.contract != FAIR_CONTRACT {
            errors.push(ValidationError {
                element: None,
                code: "CONTRACT_MISMATCH".into(),
                message: format!("contract field must be {FAIR_CONTRACT}, found {:?}", self.contract),
            });
        }
        if self.source.files.is_empty() {
            errors.push(ValidationError {
                element: None,
                code: "MISSING_SOURCE_MANIFEST".into(),
                message: "a FAIR document requires a non-empty source manifest".into(),
            });
        }
        for file in &self.source.files {
            if !is_lower_hex_sha256(&file.sha256) {
                errors.push(ValidationError {
                    element: None,
                    code: "BAD_SOURCE_DIGEST".into(),
                    message: format!("file {:?} digest is not lowercase hex SHA-256", file.path),
                });
            }
        }
        if self.extractor.id_rule.trim().is_empty() {
            errors.push(ValidationError {
                element: None,
                code: "MISSING_ID_RULE".into(),
                message: "extractor must state its element-ID assignment rule".into(),
            });
        }
        let mut seen = std::collections::BTreeSet::new();
        for element in &self.elements {
            if element.id.trim().is_empty() {
                errors.push(ValidationError {
                    element: None,
                    code: "EMPTY_ELEMENT_ID".into(),
                    message: "element IDs must be non-empty".into(),
                });
                continue;
            }
            if !seen.insert(element.id.clone()) {
                errors.push(ValidationError {
                    element: Some(element.id.clone()),
                    code: "DUPLICATE_ELEMENT_ID".into(),
                    message: format!("element ID {:?} appears more than once", element.id),
                });
            }
            if element.kind == ElementKind::Unknown
                && !matches!(element.semantics.get("observed"), Some(SemanticsValue::Str(s)) if !s.is_empty())
            {
                errors.push(ValidationError {
                    element: Some(element.id.clone()),
                    code: "UNKNOWN_KIND_UNRECORDED".into(),
                    message: "kind `unknown` requires the observed spelling under semantics `observed`".into(),
                });
            }
            let p = &element.provenance;
            if p.line == 0 {
                errors.push(ValidationError {
                    element: Some(element.id.clone()),
                    code: "ZERO_LINE".into(),
                    message: "provenance lines are 1-based".into(),
                });
            }
            match (p.span_start, p.span_end) {
                (None, None) => {}
                (Some(s), Some(e)) if s < e => {}
                _ => errors.push(ValidationError {
                    element: Some(element.id.clone()),
                    code: "BAD_SPAN".into(),
                    message: "spans are end-exclusive UTF-8; both bounds present with start < end, or neither".into(),
                }),
            }
            let r = &element.representability;
            let reason_required = r.status != RepresentabilityStatus::Extracted;
            match &r.reason {
                Some(code) if is_reason_code(code) => {}
                Some(code) => errors.push(ValidationError {
                    element: Some(element.id.clone()),
                    code: "BAD_REASON_CODE".into(),
                    message: format!(
                        "reason code {code:?} is not uppercase snake case from the versioned register"
                    ),
                }),
                None if reason_required => errors.push(ValidationError {
                    element: Some(element.id.clone()),
                    code: "MISSING_REASON_CODE".into(),
                    message: format!("status {:?} requires a reason code", r.status),
                }),
                None => {}
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Deterministic canonical serialisation: elements sorted by ID, sorted
    /// map keys (via `BTreeMap`), fixed field order, trailing newline.
    pub fn canonical_json(&self) -> String {
        let mut sorted = self.clone();
        sorted.elements.sort_by(|a, b| a.id.cmp(&b.id));
        let mut out = serde_json::to_string_pretty(&sorted).expect("FAIR serialises");
        out.push('\n');
        out
    }

    /// Derived coverage counts per representability status. Derived, never
    /// persisted as authoritative: the element states are the unit of truth.
    pub fn coverage_summary(&self) -> BTreeMap<RepresentabilityStatus, usize> {
        let mut counts: BTreeMap<RepresentabilityStatus, usize> = BTreeMap::new();
        for element in &self.elements {
            *counts.entry(element.representability.status).or_default() += 1;
        }
        counts
    }
}

/// Uppercase snake case, no leading digit, non-empty.
fn is_reason_code(code: &str) -> bool {
    !code.is_empty()
        && code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !code.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn is_lower_hex_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use RepresentabilityStatus::*;

    fn manifest() -> SourceManifest {
        SourceManifest {
            root: "fixtures/sample-app".into(),
            files: vec![SourceFile {
                path: "src/main.ts".into(),
                sha256: "a".repeat(64),
            }],
        }
    }

    fn extractor() -> ExtractorIdentity {
        ExtractorIdentity {
            name: "goya-disparate-electron".into(),
            version: "0.0.1".into(),
            id_rule: "sequential per discovery order, prefixed by subsystem".into(),
        }
    }

    fn element(id: &str, status: RepresentabilityStatus, reason: Option<&str>) -> FairElement {
        FairElement {
            id: id.into(),
            kind: ElementKind::Component,
            provenance: ElementProvenance {
                file: "src/main.ts".into(),
                line: 74,
                span_start: None,
                span_end: None,
                fallback_origin: false,
            },
            representability: Representability {
                status,
                reason: reason.map(Into::into),
                detail: None,
            },
            semantics: BTreeMap::new(),
        }
    }

    fn doc(elements: Vec<FairElement>) -> FairDocument {
        FairDocument {
            contract: FAIR_CONTRACT.into(),
            project: ProjectProvenance {
                name: "sample-app".into(),
                frameworks: vec![FrameworkDetection {
                    framework: "electron".into(),
                    evidence: "dependency electron ^31.0.1".into(),
                }],
                entry_points: vec!["src/main/index.ts".into()],
            },
            source: manifest(),
            extractor: extractor(),
            elements,
        }
    }

    #[test]
    fn valid_document_passes() {
        assert_eq!(doc(vec![element("ui-0001", Extracted, None)]).validate(), Ok(()));
    }

    #[test]
    fn non_extracted_status_requires_reason() {
        let errors = doc(vec![element("ui-0001", Unrepresentable, None)])
            .validate()
            .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "MISSING_REASON_CODE"));
    }

    #[test]
    fn undetermined_requires_reason() {
        let errors = doc(vec![element("node-0042", Undetermined, None)])
            .validate()
            .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "MISSING_REASON_CODE"));
    }

    #[test]
    fn reason_codes_must_be_register_form() {
        let errors = doc(vec![element("ui-0001", Unrepresentable, Some("no capability"))])
            .validate()
            .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "BAD_REASON_CODE"));
    }

    #[test]
    fn duplicate_ids_rejected() {
        let errors = doc(vec![
            element("ui-0001", Extracted, None),
            element("ui-0001", Extracted, None),
        ])
        .validate()
        .unwrap_err();
        assert!(errors.iter().any(|e| e.code == "DUPLICATE_ELEMENT_ID"));
    }

    #[test]
    fn unknown_kind_requires_observed_spelling() {
        let mut e = element("x-0001", Undetermined, Some("UNRESOLVED_CONSTRUCT"));
        e.kind = ElementKind::Unknown;
        let errors = doc(vec![e.clone()]).validate().unwrap_err();
        assert!(errors.iter().any(|e| e.code == "UNKNOWN_KIND_UNRECORDED"));
        e.semantics
            .insert("observed".into(), SemanticsValue::Str("WidgetFactory".into()));
        assert_eq!(doc(vec![e]).validate(), Ok(()));
    }

    #[test]
    fn half_open_span_rejected() {
        let mut e = element("ui-0001", Extracted, None);
        e.provenance.span_start = Some(10);
        let errors = doc(vec![e]).validate().unwrap_err();
        assert!(errors.iter().any(|e| e.code == "BAD_SPAN"));
    }

    #[test]
    fn missing_manifest_rejected() {
        let mut d = doc(vec![]);
        d.source.files.clear();
        let errors = d.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.code == "MISSING_SOURCE_MANIFEST"));
    }

    #[test]
    fn canonical_output_is_sorted_and_stable() {
        let a = doc(vec![element("ui-0002", Extracted, None), element("ui-0001", Extracted, None)]);
        let b = doc(vec![element("ui-0001", Extracted, None), element("ui-0002", Extracted, None)]);
        assert_eq!(a.canonical_json(), b.canonical_json());
        assert!(a.canonical_json().ends_with('\n'));
    }

    #[test]
    fn dynamic_require_is_preserved_not_dropped() {
        let mut e = element(
            "node-0042",
            Undetermined,
            Some("DYNAMIC_REQUIRE_UNRESOLVED"),
        );
        e.kind = ElementKind::DynamicModuleResolution;
        e.semantics.insert(
            "resolution".into(),
            SemanticsValue::Str("unresolved".into()),
        );
        let d = doc(vec![e]);
        assert_eq!(d.validate(), Ok(()));
        let json = d.canonical_json();
        assert!(json.contains("dynamic_module_resolution"));
        assert!(json.contains("DYNAMIC_REQUIRE_UNRESOLVED"));
    }

    #[test]
    fn coverage_summary_counts_states() {
        let d = doc(vec![
            element("a", Extracted, None),
            element("b", Undetermined, Some("X")),
            element("c", Undetermined, Some("Y")),
        ]);
        assert_eq!(d.coverage_summary().get(&Undetermined), Some(&2));
    }

    #[test]
    fn fixture_round_trips() {
        let raw = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/example.fair.json"
        ))
        .expect("fixture present");
        let parsed: FairDocument = serde_json::from_str(&raw).expect("fixture parses");
        assert_eq!(parsed.validate(), Ok(()));
        let canonical = parsed.canonical_json();
        let reparsed: FairDocument = serde_json::from_str(&canonical).expect("canonical parses");
        assert_eq!(parsed, reparsed);
        assert_eq!(parsed.canonical_json(), canonical);
    }
}
