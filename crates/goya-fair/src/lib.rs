//! GY-FAIR/2 — Foreign Application Intermediate Representation.
//!
//! Schema, validation and deterministic serialisation for the Disparates
//! programme. FAIR records what semantic extraction understood about a foreign
//! application, including — explicitly — what it did not understand.
//!
//! FAIR/2 adds typed relationships between elements, four element kinds, a
//! project record (entry points and framework detections) and a closed
//! reason-code register. It is additive: FAIR/1 documents still deserialise,
//! validate under their own rules and serialise to the same bytes.
//!
//! This crate intentionally contains no `goya-ir` types: FAIR preserves foreign
//! semantics; the FAIR→Goya boundary is a future contract. See
//! `docs/disparates/contracts/fair-2.md`.

mod register;

pub use register::{is_registered, REASON_CODES};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Current FAIR contract identifier.
pub const FAIR_CONTRACT: &str = "GY-FAIR/2";
/// The previous revision, still accepted by [`FairDocument::validate`].
pub const FAIR_1_CONTRACT: &str = "GY-FAIR/1";

/// Top-level FAIR document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairDocument {
    /// Contract identifier: [`FAIR_CONTRACT`] or [`FAIR_1_CONTRACT`].
    pub contract: String,
    /// Binding of this document to foreign source bytes.
    pub source: SourceManifest,
    /// Identity of the extractor that produced this document.
    pub extractor: ExtractorIdentity,
    /// Entry points and framework detections (FAIR/2). Omitted when empty.
    #[serde(default, skip_serializing_if = "FairProject::is_empty")]
    pub project: FairProject,
    /// Extracted elements, in extractor emission order. Canonical output is
    /// sorted by element ID; storage order is preserved as emitted.
    pub elements: Vec<FairElement>,
    /// Typed links between elements (FAIR/2). Canonical output is sorted by
    /// relationship ID. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<FairRelationship>,
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

/// Document-level project record (FAIR/2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairProject {
    /// Files the foreign runtime starts from, with their role.
    #[serde(default)]
    pub entry_points: Vec<EntryPoint>,
    /// Frameworks detected, each with the elements that show it.
    #[serde(default)]
    pub framework_detections: Vec<FrameworkDetection>,
}

impl FairProject {
    /// True when the record holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entry_points.is_empty() && self.framework_detections.is_empty()
    }
}

/// One entry point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryPoint {
    /// Path relative to the foreign project root; must be in the manifest.
    pub path: String,
    /// Role in the foreign runtime (for example `main`, `preload`, `renderer`).
    pub role: String,
    /// Element IDs that establish this entry point. Never empty.
    pub evidence: Vec<String>,
}

/// One framework detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkDetection {
    /// Framework name (for example `electron`).
    pub framework: String,
    /// Declared version or range, when the source states one.
    pub version: Option<String>,
    /// Element IDs that show the framework. Never empty.
    pub evidence: Vec<String>,
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
    /// FAIR/2: a static module import (`require('x')`, `import … from 'x'`).
    ModuleImport,
    /// FAIR/2: a function or method kept as a unit of application logic.
    LogicUnit,
    /// FAIR/2: a call made through a preload or exposure bridge.
    BridgeCall,
    /// FAIR/2: the package manifest (for example `package.json`).
    PackageManifest,
    /// Construct outside the versioned vocabulary.
    Unknown,
}

impl ElementKind {
    /// True for kinds introduced by FAIR/2, which FAIR/1 documents may not use.
    pub fn is_fair2(self) -> bool {
        matches!(
            self,
            Self::ModuleImport | Self::LogicUnit | Self::BridgeCall | Self::PackageManifest
        )
    }
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
    /// status except [`RepresentabilityStatus::Extracted`]. Under FAIR/2 it must
    /// be in [`REASON_CODES`].
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

/// A typed link between two elements (FAIR/2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairRelationship {
    /// Extractor-assigned ID, unique among relationships.
    pub id: String,
    /// Element ID the relationship starts from.
    pub subject: String,
    /// Relation from the versioned vocabulary.
    pub relation: Relation,
    /// Element ID the relationship points to.
    pub object: String,
    /// How the relationship is known.
    pub basis: Basis,
    /// Element or relationship IDs it rests on. Required when `basis` is
    /// `derived`; may be empty otherwise.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// The source site that shows the relationship.
    pub provenance: ElementProvenance,
}

/// Versioned relation vocabulary. Grows by contract revision only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// The subject structurally contains the object (markup nesting).
    Contains,
    /// The subject calls or sends to the object.
    Invokes,
    /// The subject handles events or requests of the object.
    Handles,
    /// The subject makes the object available across a boundary.
    Exposes,
    /// The subject reads the object's value.
    Reads,
    /// The subject changes the object's value.
    Mutates,
    /// The subject loads the object (a page, script or module).
    Loads,
    /// The subject styles the object.
    Styles,
}

/// How a relationship is known. FAIR/2 admits no inferred relationships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Seen directly at one source site.
    Extracted,
    /// Follows deterministically from the elements or relationships in `evidence`.
    Derived,
    /// Recorded during behavioural replay (Disparates Phase 3).
    Observed,
}

/// A structural validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Element or relationship ID the failure concerns, or `None` for
    /// document-level failures.
    pub element: Option<String>,
    /// Machine-readable failure code.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

fn error(errors: &mut Vec<ValidationError>, element: Option<&str>, code: &str, message: String) {
    errors.push(ValidationError {
        element: element.map(str::to_string),
        code: code.into(),
        message,
    });
}

fn check_provenance(errors: &mut Vec<ValidationError>, id: &str, p: &ElementProvenance) {
    if p.line == 0 {
        error(errors, Some(id), "ZERO_LINE", "provenance lines are 1-based".into());
    }
    match (p.span_start, p.span_end) {
        (None, None) => {}
        (Some(s), Some(e)) if s < e => {}
        _ => error(
            errors,
            Some(id),
            "BAD_SPAN",
            "spans are end-exclusive UTF-8; both bounds present with start < end, or neither".into(),
        ),
    }
}

impl FairDocument {
    /// True if this is a FAIR/2 document.
    pub fn is_fair2(&self) -> bool {
        self.contract == FAIR_CONTRACT
    }

    /// Structural validation. FAIR/2 documents are checked against every
    /// FAIR/2 rule. FAIR/1 documents are checked against FAIR/1 rules and
    /// may not use any FAIR/2 feature.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        let fair2 = self.is_fair2();
        if !fair2 && self.contract != FAIR_1_CONTRACT {
            error(
                &mut errors,
                None,
                "CONTRACT_MISMATCH",
                format!(
                    "contract field must be {FAIR_CONTRACT} or {FAIR_1_CONTRACT}, found {:?}",
                    self.contract
                ),
            );
        }
        if self.source.files.is_empty() {
            error(
                &mut errors,
                None,
                "MISSING_SOURCE_MANIFEST",
                "a FAIR document requires a non-empty source manifest".into(),
            );
        }
        for file in &self.source.files {
            if !is_lower_hex_sha256(&file.sha256) {
                error(
                    &mut errors,
                    None,
                    "BAD_SOURCE_DIGEST",
                    format!("file {:?} digest is not lowercase hex SHA-256", file.path),
                );
            }
        }
        if self.extractor.id_rule.trim().is_empty() {
            error(
                &mut errors,
                None,
                "MISSING_ID_RULE",
                "extractor must state its element-ID assignment rule".into(),
            );
        }
        let mut element_ids = BTreeSet::new();
        for element in &self.elements {
            if element.id.trim().is_empty() {
                error(&mut errors, None, "EMPTY_ELEMENT_ID", "element IDs must be non-empty".into());
                continue;
            }
            let id = element.id.as_str();
            if !element_ids.insert(id) {
                error(
                    &mut errors,
                    Some(id),
                    "DUPLICATE_ELEMENT_ID",
                    format!("element ID {id:?} appears more than once"),
                );
            }
            if !fair2 && element.kind.is_fair2() {
                error(
                    &mut errors,
                    Some(id),
                    "FAIR2_FEATURE_IN_FAIR1",
                    format!("kind {:?} needs {FAIR_CONTRACT}", element.kind),
                );
            }
            if element.kind == ElementKind::Unknown
                && !matches!(element.semantics.get("observed"), Some(SemanticsValue::Str(s)) if !s.is_empty())
            {
                error(
                    &mut errors,
                    Some(id),
                    "UNKNOWN_KIND_UNRECORDED",
                    "kind `unknown` requires the observed spelling under semantics `observed`".into(),
                );
            }
            check_provenance(&mut errors, id, &element.provenance);
            let r = &element.representability;
            let reason_required = r.status != RepresentabilityStatus::Extracted;
            match &r.reason {
                Some(code) if !is_reason_code(code) => error(
                    &mut errors,
                    Some(id),
                    "BAD_REASON_CODE",
                    format!("reason code {code:?} is not uppercase snake case"),
                ),
                Some(code) if fair2 && !is_registered(code) => error(
                    &mut errors,
                    Some(id),
                    "UNREGISTERED_REASON_CODE",
                    format!("reason code {code:?} is not in the FAIR/2 register"),
                ),
                Some(_) => {}
                None if reason_required => error(
                    &mut errors,
                    Some(id),
                    "MISSING_REASON_CODE",
                    format!("status {:?} requires a reason code", r.status),
                ),
                None => {}
            }
        }

        if !fair2 && (!self.relationships.is_empty() || !self.project.is_empty()) {
            error(
                &mut errors,
                None,
                "FAIR2_FEATURE_IN_FAIR1",
                format!("relationships and the project record need {FAIR_CONTRACT}"),
            );
        }

        let relationship_ids: BTreeSet<&str> = self.relationships.iter().map(|r| r.id.as_str()).collect();
        let mut seen = BTreeSet::new();
        for rel in &self.relationships {
            let id = rel.id.as_str();
            if id.trim().is_empty() {
                error(&mut errors, None, "EMPTY_RELATIONSHIP_ID", "relationship IDs must be non-empty".into());
                continue;
            }
            if !seen.insert(id) {
                error(
                    &mut errors,
                    Some(id),
                    "DUPLICATE_RELATIONSHIP_ID",
                    format!("relationship ID {id:?} appears more than once"),
                );
            }
            for (end, target) in [("subject", &rel.subject), ("object", &rel.object)] {
                if !element_ids.contains(target.as_str()) {
                    error(
                        &mut errors,
                        Some(id),
                        "DANGLING_RELATIONSHIP",
                        format!("{end} {target:?} is not an element ID"),
                    );
                }
            }
            if rel.basis == Basis::Derived && rel.evidence.is_empty() {
                error(
                    &mut errors,
                    Some(id),
                    "MISSING_EVIDENCE",
                    "a derived relationship must name the elements or relationships it rests on".into(),
                );
            }
            for e in &rel.evidence {
                if e == id || !(element_ids.contains(e.as_str()) || relationship_ids.contains(e.as_str())) {
                    error(
                        &mut errors,
                        Some(id),
                        "DANGLING_EVIDENCE",
                        format!("evidence {e:?} is not another element or relationship ID"),
                    );
                }
            }
            check_provenance(&mut errors, id, &rel.provenance);
        }

        let manifest: BTreeSet<&str> = self.source.files.iter().map(|f| f.path.as_str()).collect();
        for entry in &self.project.entry_points {
            let label = format!("entry point {:?}", entry.path);
            if !manifest.contains(entry.path.as_str()) {
                error(&mut errors, None, "ENTRY_POINT_NOT_IN_MANIFEST", format!("{label} is not in the source manifest"));
            }
            if entry.role.trim().is_empty() {
                error(&mut errors, None, "EMPTY_ROLE", format!("{label} has no role"));
            }
            check_evidence(&mut errors, &label, &entry.evidence, &element_ids);
        }
        for detection in &self.project.framework_detections {
            let label = format!("framework detection {:?}", detection.framework);
            if detection.framework.trim().is_empty() {
                error(&mut errors, None, "EMPTY_FRAMEWORK", "framework names must be non-empty".into());
            }
            check_evidence(&mut errors, &label, &detection.evidence, &element_ids);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Converts a valid FAIR/1 document to FAIR/2. Fails if any reason code
    /// is outside the FAIR/2 register, or if the result does not validate.
    pub fn upgrade(mut self) -> Result<FairDocument, Vec<ValidationError>> {
        if self.contract != FAIR_1_CONTRACT {
            return Err(vec![ValidationError {
                element: None,
                code: "CONTRACT_MISMATCH".into(),
                message: format!("only {FAIR_1_CONTRACT} documents can be upgraded"),
            }]);
        }
        self.validate()?;
        self.contract = FAIR_CONTRACT.into();
        self.validate()?;
        Ok(self)
    }

    /// Deterministic canonical serialisation: elements and relationships
    /// sorted by ID, entry points by (path, role), detections by framework,
    /// sorted map keys (via `BTreeMap`), fixed field order, trailing newline.
    /// An empty project record and an empty relationship list are omitted, so
    /// a FAIR/1 document serialises to the same bytes as under FAIR/1.
    pub fn canonical_json(&self) -> String {
        let mut sorted = self.clone();
        sorted.elements.sort_by(|a, b| a.id.cmp(&b.id));
        sorted.relationships.sort_by(|a, b| a.id.cmp(&b.id));
        sorted
            .project
            .entry_points
            .sort_by(|a, b| (&a.path, &a.role).cmp(&(&b.path, &b.role)));
        sorted
            .project
            .framework_detections
            .sort_by(|a, b| a.framework.cmp(&b.framework));
        for list in sorted
            .project
            .entry_points
            .iter_mut()
            .map(|e| &mut e.evidence)
            .chain(sorted.project.framework_detections.iter_mut().map(|d| &mut d.evidence))
            .chain(sorted.relationships.iter_mut().map(|r| &mut r.evidence))
        {
            list.sort();
        }
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

fn check_evidence(errors: &mut Vec<ValidationError>, label: &str, evidence: &[String], ids: &BTreeSet<&str>) {
    if evidence.is_empty() {
        error(errors, None, "MISSING_EVIDENCE", format!("{label} names no evidence"));
    }
    for e in evidence {
        if !ids.contains(e.as_str()) {
            error(errors, None, "DANGLING_EVIDENCE", format!("{label}: evidence {e:?} is not an element ID"));
        }
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

    fn provenance() -> ElementProvenance {
        ElementProvenance {
            file: "src/main.ts".into(),
            line: 74,
            span_start: None,
            span_end: None,
            fallback_origin: false,
        }
    }

    fn element(id: &str, status: RepresentabilityStatus, reason: Option<&str>) -> FairElement {
        FairElement {
            id: id.into(),
            kind: ElementKind::Component,
            provenance: provenance(),
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
            source: manifest(),
            extractor: extractor(),
            project: FairProject::default(),
            elements,
            relationships: Vec::new(),
        }
    }

    fn doc1(elements: Vec<FairElement>) -> FairDocument {
        FairDocument {
            contract: FAIR_1_CONTRACT.into(),
            ..doc(elements)
        }
    }

    fn rel(id: &str, subject: &str, object: &str, basis: Basis, evidence: &[&str]) -> FairRelationship {
        FairRelationship {
            id: id.into(),
            subject: subject.into(),
            relation: Relation::Invokes,
            object: object.into(),
            basis,
            evidence: evidence.iter().map(|e| e.to_string()).collect(),
            provenance: provenance(),
        }
    }

    fn codes(result: Result<(), Vec<ValidationError>>) -> Vec<String> {
        result.err().unwrap_or_default().into_iter().map(|e| e.code).collect()
    }

    // ---- FAIR/1 rules, unchanged ------------------------------------------

    #[test]
    fn valid_document_passes() {
        assert_eq!(doc(vec![element("ui-0001", Extracted, None)]).validate(), Ok(()));
        assert_eq!(doc1(vec![element("ui-0001", Extracted, None)]).validate(), Ok(()));
    }

    #[test]
    fn non_extracted_status_requires_reason() {
        let errors = codes(doc(vec![element("ui-0001", Unrepresentable, None)]).validate());
        assert!(errors.contains(&"MISSING_REASON_CODE".to_string()));
    }

    #[test]
    fn undetermined_requires_reason() {
        let errors = codes(doc(vec![element("node-0042", Undetermined, None)]).validate());
        assert!(errors.contains(&"MISSING_REASON_CODE".to_string()));
    }

    #[test]
    fn reason_codes_must_be_register_form() {
        for d in [
            doc(vec![element("ui-0001", Unrepresentable, Some("no capability"))]),
            doc1(vec![element("ui-0001", Unrepresentable, Some("no capability"))]),
        ] {
            assert!(codes(d.validate()).contains(&"BAD_REASON_CODE".to_string()));
        }
    }

    #[test]
    fn duplicate_ids_rejected() {
        let errors = codes(
            doc(vec![element("ui-0001", Extracted, None), element("ui-0001", Extracted, None)]).validate(),
        );
        assert!(errors.contains(&"DUPLICATE_ELEMENT_ID".to_string()));
    }

    #[test]
    fn unknown_kind_requires_observed_spelling() {
        let mut e = element("x-0001", Undetermined, Some("NO_FAIR_KIND"));
        e.kind = ElementKind::Unknown;
        assert!(codes(doc(vec![e.clone()]).validate()).contains(&"UNKNOWN_KIND_UNRECORDED".to_string()));
        e.semantics
            .insert("observed".into(), SemanticsValue::Str("WidgetFactory".into()));
        assert_eq!(doc(vec![e]).validate(), Ok(()));
    }

    #[test]
    fn half_open_span_rejected() {
        let mut e = element("ui-0001", Extracted, None);
        e.provenance.span_start = Some(10);
        assert!(codes(doc(vec![e]).validate()).contains(&"BAD_SPAN".to_string()));
    }

    #[test]
    fn missing_manifest_rejected() {
        let mut d = doc(vec![]);
        d.source.files.clear();
        assert!(codes(d.validate()).contains(&"MISSING_SOURCE_MANIFEST".to_string()));
    }

    #[test]
    fn unknown_contract_rejected() {
        let mut d = doc(vec![]);
        d.contract = "GY-FAIR/3".into();
        assert!(codes(d.validate()).contains(&"CONTRACT_MISMATCH".to_string()));
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
        let mut e = element("node-0042", Undetermined, Some("DYNAMIC_REQUIRE_UNRESOLVED"));
        e.kind = ElementKind::DynamicModuleResolution;
        e.semantics
            .insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
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
            element("b", Undetermined, Some("PARSE_FAILED")),
            element("c", Undetermined, Some("NO_FAIR_KIND")),
        ]);
        assert_eq!(d.coverage_summary().get(&Undetermined), Some(&2));
    }

    // ---- FAIR/1 compatibility ---------------------------------------------

    #[test]
    fn fair1_fixture_round_trips_byte_for_byte() {
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/example.fair.json"))
            .expect("fixture present");
        let parsed: FairDocument = serde_json::from_str(&raw).expect("fixture parses");
        assert_eq!(parsed.contract, FAIR_1_CONTRACT);
        assert_eq!(parsed.validate(), Ok(()));
        // The FAIR/1 bytes are unchanged by the FAIR/2 schema.
        assert_eq!(parsed.canonical_json(), raw);
    }

    #[test]
    fn fair1_keeps_form_only_reason_codes() {
        let d = doc1(vec![element("b", Undetermined, Some("SOMETHING_LOCAL"))]);
        assert_eq!(d.validate(), Ok(()));
        let upgraded = d.upgrade();
        assert!(codes(upgraded.map(|_| ())).contains(&"UNREGISTERED_REASON_CODE".to_string()));
    }

    #[test]
    fn fair1_documents_cannot_use_fair2_features() {
        let mut e = element("m", Extracted, None);
        e.kind = ElementKind::ModuleImport;
        assert!(codes(doc1(vec![e]).validate()).contains(&"FAIR2_FEATURE_IN_FAIR1".to_string()));
        let mut d = doc1(vec![element("a", Extracted, None), element("b", Extracted, None)]);
        d.relationships.push(rel("r", "a", "b", Basis::Extracted, &[]));
        assert!(codes(d.validate()).contains(&"FAIR2_FEATURE_IN_FAIR1".to_string()));
    }

    #[test]
    fn fair1_upgrades_to_fair2() {
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/example.fair.json"))
            .expect("fixture present");
        let parsed: FairDocument = serde_json::from_str(&raw).expect("fixture parses");
        let upgraded = parsed.upgrade().expect("upgrades");
        assert_eq!(upgraded.contract, FAIR_CONTRACT);
        assert!(doc(vec![]).upgrade().is_err(), "FAIR/2 is not upgraded again");
    }

    // ---- FAIR/2 -----------------------------------------------------------

    #[test]
    fn fair2_requires_registered_reason_codes() {
        let d = doc(vec![element("b", Undetermined, Some("SOMETHING_LOCAL"))]);
        assert!(codes(d.validate()).contains(&"UNREGISTERED_REASON_CODE".to_string()));
    }

    #[test]
    fn register_is_sorted_unique_and_well_formed() {
        assert!(REASON_CODES.windows(2).all(|w| w[0].0 < w[1].0), "register must be sorted and unique");
        assert!(REASON_CODES.iter().all(|(c, m)| is_reason_code(c) && !m.is_empty()));
    }

    #[test]
    fn register_matches_its_document() {
        let doc = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/disparates/contracts/reason-codes.md"
        ))
        .expect("register document present");
        let documented: Vec<&str> = doc
            .lines()
            .filter_map(|l| l.strip_prefix("| `"))
            .filter_map(|l| l.split('`').next())
            .collect();
        let coded: Vec<&str> = REASON_CODES.iter().map(|(c, _)| *c).collect();
        assert_eq!(documented, coded);
    }

    #[test]
    fn relationships_must_point_at_elements() {
        let mut d = doc(vec![element("a", Extracted, None)]);
        d.relationships.push(rel("r", "a", "missing", Basis::Extracted, &[]));
        assert!(codes(d.validate()).contains(&"DANGLING_RELATIONSHIP".to_string()));
    }

    #[test]
    fn derived_relationships_need_existing_evidence() {
        let mut d = doc(vec![element("a", Extracted, None), element("b", Extracted, None)]);
        d.relationships.push(rel("r1", "a", "b", Basis::Derived, &[]));
        assert!(codes(d.validate()).contains(&"MISSING_EVIDENCE".to_string()));
        d.relationships[0].evidence = vec!["r1".into()];
        assert!(codes(d.validate()).contains(&"DANGLING_EVIDENCE".to_string()), "self-evidence refused");
        d.relationships.push(rel("r0", "b", "a", Basis::Extracted, &[]));
        d.relationships[0].evidence = vec!["r0".into(), "a".into()];
        assert_eq!(d.validate(), Ok(()));
    }

    #[test]
    fn duplicate_relationship_ids_rejected() {
        let mut d = doc(vec![element("a", Extracted, None), element("b", Extracted, None)]);
        d.relationships.push(rel("r", "a", "b", Basis::Extracted, &[]));
        d.relationships.push(rel("r", "b", "a", Basis::Extracted, &[]));
        assert!(codes(d.validate()).contains(&"DUPLICATE_RELATIONSHIP_ID".to_string()));
    }

    #[test]
    fn project_record_is_checked() {
        let mut d = doc(vec![element("a", Extracted, None)]);
        d.project.entry_points.push(EntryPoint {
            path: "src/other.ts".into(),
            role: "".into(),
            evidence: vec![],
        });
        d.project.framework_detections.push(FrameworkDetection {
            framework: "electron".into(),
            version: None,
            evidence: vec!["nope".into()],
        });
        let errors = codes(d.validate());
        for code in ["ENTRY_POINT_NOT_IN_MANIFEST", "EMPTY_ROLE", "MISSING_EVIDENCE", "DANGLING_EVIDENCE"] {
            assert!(errors.contains(&code.to_string()), "{code} not raised");
        }
    }

    #[test]
    fn fair2_canonical_output_sorts_everything() {
        let base = || {
            let mut d = doc(vec![element("a", Extracted, None), element("b", Extracted, None)]);
            d.project.entry_points.push(EntryPoint {
                path: "src/main.ts".into(),
                role: "main".into(),
                evidence: vec!["b".into(), "a".into()],
            });
            d
        };
        let mut x = base();
        x.relationships.push(rel("r2", "a", "b", Basis::Derived, &["b", "a"]));
        x.relationships.push(rel("r1", "b", "a", Basis::Extracted, &[]));
        let mut y = base();
        y.relationships.push(rel("r1", "b", "a", Basis::Extracted, &[]));
        y.relationships.push(rel("r2", "a", "b", Basis::Derived, &["a", "b"]));
        assert_eq!(x.validate(), Ok(()));
        assert_eq!(x.canonical_json(), y.canonical_json());
        let reparsed: FairDocument = serde_json::from_str(&x.canonical_json()).expect("parses");
        assert_eq!(reparsed.canonical_json(), x.canonical_json());
    }

    #[test]
    fn fair2_fixture_round_trips() {
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/example.fair2.json"))
            .expect("fixture present");
        let parsed: FairDocument = serde_json::from_str(&raw).expect("fixture parses");
        assert_eq!(parsed.validate(), Ok(()));
        assert_eq!(parsed.canonical_json(), raw);
    }
}
