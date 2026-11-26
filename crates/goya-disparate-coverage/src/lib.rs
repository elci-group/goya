//! GY-COVERAGE/1 — Representability assessment and capability-deficit report.
//!
//! Assesses a GY-FAIR/1 document's `extracted` elements against a versioned,
//! serialisable map of Goya capabilities. The map is data, not code: every
//! rule names the Goya contract (crate) that provides the equivalent
//! semantics, or the reason code for its absence. Assessment upgrades
//! `extracted` elements only — existing `undetermined` and `unrepresentable`
//! classifications are preserved, never downgraded or silently re-decided.
//!
//! The derived capability-deficit report aggregates blocked elements by
//! reason code: this is the Disparates programme's engineering-intelligence
//! output — observed migration blockers, not speculative demand.
//!
//! At programme scale, [`DeficitReport::aggregate`] combines the deficit
//! reports of many corpora into one prioritisation input, and
//! [`category_of`] classifies every blocking reason into the two blocking
//! categories of GY-COVERAGE/1 — goya-side capability demand versus
//! programme-side debt — per the reason-code register
//! (`docs/disparates/reason-codes.md`, the authoritative source this mapping
//! must stay in sync with).
//!
//! Contract: `GY-COVERAGE/1`. See `docs/disparates/contracts/coverage-1.md`.

use goya_fair::{
    ElementKind, FairDocument, FairElement, Representability, RepresentabilityStatus,
    SemanticsValue,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Coverage contract identifier.
pub const COVERAGE_CONTRACT: &str = "GY-COVERAGE/1";
/// Map format version.
pub const MAP_VERSION: &str = "1";

/// One assessment rule: an element selector plus the Goya disposition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Element kind this rule applies to. `null` matches every kind.
    pub kind: Option<ElementKind>,
    /// Required semantics facets (subset match); empty matches any.
    pub semantics: BTreeMap<String, String>,
    /// Assessment outcome. Must not be `extracted`.
    pub status: RepresentabilityStatus,
    /// Register reason code. Required for every status other than
    /// `representable` (which may still carry one).
    pub reason: Option<String>,
    /// Named Goya contract(s) or capability providing the semantics.
    pub goya_evidence: String,
    /// Human explanation of the mapping or the gap.
    pub detail: String,
}

/// Versioned map of Goya capabilities used for assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMap {
    /// Format version; must equal [`MAP_VERSION`].
    pub version: String,
    /// Assessment rules; first match wins.
    pub rules: Vec<Rule>,
}

impl CapabilityMap {
    /// Parse a map from JSON, validating it.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let map: CapabilityMap =
            serde_json::from_str(json).map_err(|e| format!("invalid map: {e}"))?;
        map.validate()?;
        Ok(map)
    }

    /// Validate map invariants: version, no `extracted` outcomes, reasons
    /// where required, evidence always present.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != MAP_VERSION {
            return Err(format!("map version must be {MAP_VERSION}, found {}", self.version));
        }
        for (idx, rule) in self.rules.iter().enumerate() {
            if rule.status == RepresentabilityStatus::Extracted {
                return Err(format!("rule {idx}: assessment outcome cannot be `extracted`"));
            }
            if rule.reason.is_none() {
                return Err(format!(
                    "rule {idx}: every assessment outcome requires a reason code (representable mappings cite their mapping code)"
                ));
            }
            if let Some(code) = &rule.reason {
                if !code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
                    return Err(format!("rule {idx}: reason code {code:?} is not register form"));
                }
            }
            if rule.goya_evidence.trim().is_empty() {
                return Err(format!("rule {idx}: goya_evidence is required even for absent capabilities (cite the checked contract space)"));
            }
        }
        Ok(())
    }

    /// First matching rule for an element, if any. First match wins.
    pub fn match_rule(&self, element: &FairElement) -> Option<&Rule> {
        self.rules.iter().find(|rule| {
            rule.kind.is_none_or(|k| k == element.kind)
                && rule.semantics.iter().all(|(key, value)| {
                    element.semantics.get(key) == Some(&SemanticsValue::Str(value.clone()))
                })
        })
    }
}

/// Default map: grounded in the workspace's actual contract register. Rules
/// claim `representable` only where a named Goya contract provides equivalent
/// semantics; everything without a provider is declared honestly absent.
pub fn default_map() -> CapabilityMap {
    let rule = |kind, status, reason: Option<&str>, evidence: &str, detail: &str| Rule {
        kind,
        semantics: BTreeMap::new(),
        status,
        reason: reason.map(str::to_string),
        goya_evidence: evidence.to_string(),
        detail: detail.to_string(),
    };
    CapabilityMap {
        version: MAP_VERSION.to_string(),
        rules: vec![
            // UI structure and interaction: GY-LANG/1 scenes declare structure,
            // GY-IR/1 actions and GY-RUNTIME/1 dispatch declare behaviour.
            rule(
                Some(ElementKind::Component),
                RepresentabilityStatus::Representable,
                Some("MAPPED_TO_SCENE_DECLARATION"),
                "GY-LANG/1 scene syntax; GY-IR/1 nodes",
                "component structure maps to scene node declarations",
            ),
            rule(
                Some(ElementKind::EventHandler),
                RepresentabilityStatus::Representable,
                Some("MAPPED_TO_ACTION_DECLARATION"),
                "GY-IR/1 actions; GY-RUNTIME/1 event dispatch",
                "interaction handlers map to declared actions",
            ),
            rule(
                Some(ElementKind::IpcChannel),
                RepresentabilityStatus::Representable,
                Some("MAPPED_TO_ACTION_DISPATCH"),
                "GY-IR/1 actions; GY-RUNTIME/1 dispatch",
                "renderer/main process messaging dissolves in Goya's single-process model; the channel's requests and responses become actions; this is architectural simplification, not 1:1 transport mapping",
            ),
            rule(
                Some(ElementKind::Hook),
                RepresentabilityStatus::Undetermined,
                Some("HOOK_DATAFLOW_NOT_EXTRACTED"),
                "GY-STATE/1 covers value state; side-effect scheduling has no assessed equivalent",
                "the extractor records hook occurrence sites without data-flow; useState maps to state transactions, effects do not map mechanically",
            ),
            rule(
                Some(ElementKind::StyleRule),
                RepresentabilityStatus::Representable,
                Some("MAPPED_TO_SCENE_STYLING"),
                "GY-CHROMA/1 theming; GY-LAYOUT/1 deterministic layout",
                "static visual declarations map to scene styling and layout constraints",
            ),
            rule(
                Some(ElementKind::WindowLifecycle),
                RepresentabilityStatus::Representable,
                Some("MAPPED_TO_NATIVE_WINDOW_HOST"),
                "GY-WAYLAND/1; goya-window-management; Sash (Windows) and Emperor parity programme",
                "application window lifecycle maps to the native windowing host",
            ),
            rule(
                Some(ElementKind::FilesystemOperation),
                RepresentabilityStatus::Undetermined,
                Some("PERSISTENCE_CAPABILITY_UNRESOLVED"),
                "no filesystem-access contract exists in the register (checked 2026-11-19)",
                "Goya has no declared user-filesystem capability; mapping requires a scoping and authority decision first",
            ),
            rule(
                Some(ElementKind::ChildProcess),
                RepresentabilityStatus::Unrepresentable,
                Some("NO_GOYA_CAPABILITY_CHILD_PROCESS"),
                "no process-spawn capability exists in the register (checked 2026-11-19)",
                "Goya provides no child-process model; foreign spawn/exec logic is blocked",
            ),
            rule(
                Some(ElementKind::StateStore),
                RepresentabilityStatus::Undetermined,
                Some("PERSISTENCE_CAPABILITY_UNRESOLVED"),
                "GY-STATE/1 (in-memory transactions) and GY-HOTSWAP/1 (persisted-state migration) do not declare a disk-persistence contract",
                "electron-store style key-value persistence has no declared equivalent",
            ),
            // Map revision 2 (2026-11-19): clipboard writes are already
            // provided by a real, ratified-in-register capability. Inserted
            // before the coarse desktop-OS rule; first match wins.
            // Pending rules recorded in docs/disparates/revisions/map-v2-proposal.md:
            // shell/dialog splits await GY-DESKINT/1 ratification and must not
            // be emitted until then.
            Rule {
                kind: Some(ElementKind::OsApiCall),
                semantics: {
                    let mut m = BTreeMap::new();
                    m.insert("api".into(), "clipboard".into());
                    m
                },
                status: RepresentabilityStatus::Representable,
                reason: Some("MAPPED_TO_CLIPBOARD_CAPABILITY".into()),
                goya_evidence: "GY-CLIPBOARD/1 (crates/goya-clipboard): capability-gated clipboard write, Verbatim and Deep Copy modes behind ClipboardCapability/CapabilityPolicy".into(),
                detail: "clipboard write calls map to GY-CLIPBOARD/1 capability-gated clipboard write; removes api=clipboard elements from the desktop-OS-integration deficit".into(),
            },
            rule(
                Some(ElementKind::OsApiCall),
                RepresentabilityStatus::Unrepresentable,
                Some("NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION"),
                "no desktop OS-integration contract (notifications, dialogs, shell, tray) exists in the register (checked 2026-11-19)",
                "desktop OS integration is absent; each specific API is a candidate capability deficit",
            ),
            // Everything else (native modules, dynamic resolution, unknown
            // kinds) keeps the extractor's own classification.
        ],
    }
}

/// Assessment outcome for one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    /// Document with upgraded representability statuses.
    pub document: FairDocument,
    /// Number of elements upgraded from `extracted`.
    pub upgraded: usize,
    /// Number of elements left untouched (already classified, or no rule).
    pub untouched: usize,
}

/// Assess every `extracted` element against the map. Non-`extracted`
/// statuses are preserved unchanged.
pub fn assess(document: &FairDocument, map: &CapabilityMap) -> Result<Assessment, String> {
    map.validate()?;
    let mut upgraded = 0;
    let mut untouched = 0;
    let mut elements = document.elements.clone();
    for element in &mut elements {
        if element.representability.status != RepresentabilityStatus::Extracted {
            untouched += 1;
            continue;
        }
        match map.match_rule(element) {
            Some(rule) => {
                element.representability = Representability {
                    status: rule.status,
                    reason: rule.reason.clone(),
                    detail: Some(rule.detail.clone()),
                };
                element.semantics.insert(
                    "goya_evidence".into(),
                    SemanticsValue::Str(rule.goya_evidence.clone()),
                );
                upgraded += 1;
            }
            None => {
                element.representability = Representability {
                    status: RepresentabilityStatus::Undetermined,
                    reason: Some("NO_COVERAGE_RULE".into()),
                    detail: Some("no rule in capability map v1 covers this construct; the map must grow by contract revision".into()),
                };
                upgraded += 1;
            }
        }
    }
    let mut assessed = document.clone();
    assessed.elements = elements;
    assessed
        .validate()
        .map_err(|errors| format!("assessed document failed validation: {errors:?}"))?;
    Ok(Assessment { document: assessed, upgraded, untouched })
}

/// One aggregate entry in the capability-deficit report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeficitEntry {
    /// Reason code blocking the elements.
    pub reason: String,
    /// Goya evidence recorded by the assessment.
    pub goya_evidence: String,
    /// Element kinds blocked under this reason, with counts.
    pub kinds: BTreeMap<String, usize>,
    /// Distinct source files affected.
    pub files: Vec<String>,
}

impl DeficitEntry {
    /// Merge `other` into `self` (same reason code by construction): kind
    /// counts are summed, file lists are unioned (deduplicated, re-sorted),
    /// and the first non-empty `goya_evidence` is kept. The evidence should
    /// agree across corpora — both derive from the same capability map — but
    /// if it conflicts, the first is kept and remains visible in the merged
    /// report.
    pub fn merge(&mut self, other: &DeficitEntry) {
        for (kind, count) in &other.kinds {
            *self.kinds.entry(kind.clone()).or_default() += count;
        }
        self.files.extend(other.files.iter().cloned());
        self.files.sort();
        self.files.dedup();
        if self.goya_evidence.is_empty() {
            self.goya_evidence = other.goya_evidence.clone();
        }
    }
}

/// Derived capability-deficit report: what prevents this application from
/// becoming a Goya application, aggregated by reason code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeficitReport {
    /// Contract identifier; equals [`COVERAGE_CONTRACT`].
    pub contract: String,
    /// Source FAIR document root.
    pub source_root: String,
    /// Total elements assessed.
    pub elements: usize,
    /// Totals per representability status after assessment.
    pub totals: BTreeMap<String, usize>,
    /// Blocked elements aggregated by reason code, sorted by blocking count
    /// descending (insertion order preserved for equal counts).
    pub deficits: Vec<DeficitEntry>,
}

impl DeficitReport {
    /// Derive the report from an assessed document.
    pub fn derive(document: &FairDocument) -> DeficitReport {
        let mut totals: BTreeMap<String, usize> = BTreeMap::new();
        let mut blocked: BTreeMap<String, DeficitEntry> = BTreeMap::new();
        for element in &document.elements {
            let status = serde_name(&element.representability.status);
            *totals.entry(status).or_default() += 1;
            let blocked_here = matches!(
                element.representability.status,
                RepresentabilityStatus::Unrepresentable | RepresentabilityStatus::Undetermined
            );
            if !blocked_here {
                continue;
            }
            let reason = element
                .representability
                .reason
                .clone()
                .unwrap_or_else(|| "UNREASONED".into());
            let entry = blocked.entry(reason.clone()).or_insert_with(|| DeficitEntry {
                reason,
                goya_evidence: element
                    .semantics
                    .get("goya_evidence")
                    .and_then(|v| match v {
                        SemanticsValue::Str(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default(),
                kinds: BTreeMap::new(),
                files: Vec::new(),
            });
            let kind = serde_name(&element.kind);
            *entry.kinds.entry(kind).or_default() += 1;
            if !entry.files.iter().any(|f| f == &element.provenance.file) {
                entry.files.push(element.provenance.file.clone());
            }
            entry.files.sort();
        }
        let mut deficits: Vec<DeficitEntry> = blocked.into_values().collect();
        deficits.sort_by_key(|entry| usize::MAX - total(&entry.kinds));
        DeficitReport {
            contract: COVERAGE_CONTRACT.to_string(),
            source_root: document.source.root.clone(),
            elements: document.elements.len(),
            totals,
            deficits,
        }
    }

    /// Canonical JSON (sorted keys via BTreeMap, fixed field order, trailing
    /// newline).
    pub fn canonical_json(&self) -> String {
        let mut out = serde_json::to_string_pretty(self).expect("report serialises");
        out.push('\n');
        out
    }

    /// Aggregate the deficit reports of several corpora into one
    /// programme-level report (contract `GY-COVERAGE/1`): `elements` and
    /// `totals` are summed per status; `deficits` are merged by reason code
    /// ([`DeficitEntry::merge`]) and re-sorted by blocking count descending —
    /// a stable sort over first-seen order across the input sequence, so
    /// equal counts keep the order in which the input reports listed them.
    /// `source_root` is `aggregate(<n> corpora)`. Aggregating exactly one
    /// report is the identity: the report is returned unchanged (its own
    /// `source_root` kept), so the result is byte-identical to its single
    /// input.
    ///
    /// Panics on an empty slice: an aggregate of zero corpora is not defined
    /// under the contract.
    pub fn aggregate(reports: &[DeficitReport]) -> DeficitReport {
        assert!(
            !reports.is_empty(),
            "aggregate of zero corpora is not defined under GY-COVERAGE/1"
        );
        if let [single] = reports {
            return single.clone();
        }
        let mut totals: BTreeMap<String, usize> = BTreeMap::new();
        for report in reports {
            for (status, count) in &report.totals {
                *totals.entry(status.clone()).or_default() += count;
            }
        }
        // First-seen order across the input sequence; entries merged in
        // place per reason code.
        let mut merged: Vec<DeficitEntry> = Vec::new();
        for report in reports {
            for entry in &report.deficits {
                match merged.iter_mut().find(|merged| merged.reason == entry.reason) {
                    Some(existing) => existing.merge(entry),
                    None => merged.push(entry.clone()),
                }
            }
        }
        merged.sort_by_key(|entry| usize::MAX - total(&entry.kinds));
        DeficitReport {
            contract: COVERAGE_CONTRACT.to_string(),
            source_root: format!("aggregate({} corpora)", reports.len()),
            elements: reports.iter().map(|report| report.elements).sum(),
            totals,
            deficits: merged,
        }
    }

    /// Blocked-element subtotals per blocking category, in register order
    /// ([`CATEGORY_ORDER`]); zero categories are included so the output shape
    /// is fixed and deterministic.
    pub fn category_subtotals(&self) -> Vec<(&'static str, usize)> {
        CATEGORY_ORDER
            .iter()
            .map(|category| {
                (
                    *category,
                    self.deficits
                        .iter()
                        .filter(|entry| category_of(&entry.reason) == *category)
                        .map(|entry| total(&entry.kinds))
                        .sum(),
                )
            })
            .collect()
    }

    /// The two blocking categories of GY-COVERAGE/1 as `(goya_side,
    /// programme_side)` blocked-element totals: goya-side capability demand
    /// (`goya-capability-deficit` + `capability-decision-pending`) versus
    /// programme-side debt (`programme-debt` + `extraction-limit`).
    /// Non-blocking (`mapping`) and unclassified reasons count towards
    /// neither.
    pub fn blocking_split(&self) -> (usize, usize) {
        let mut split = (0usize, 0usize);
        for entry in &self.deficits {
            match blocking_side_of(&entry.reason) {
                "goya-side" => split.0 += total(&entry.kinds),
                "programme-side" => split.1 += total(&entry.kinds),
                _ => {}
            }
        }
        split
    }
}

/// Register order of blocking categories, used for deterministic subtotal
/// output ([`DeficitReport::category_subtotals`]).
pub const CATEGORY_ORDER: &[&str] = &[
    "goya-capability-deficit",
    "capability-decision-pending",
    "programme-debt",
    "extraction-limit",
    "mapping",
    "unclassified",
];

/// Classify a reason code into its register category. The authoritative
/// source is the reason-code register, `docs/disparates/reason-codes.md`;
/// this mapping must be kept in sync with that register. Unknown codes are
/// `unclassified`, never guessed.
///
/// | Reason code(s) | Category | Blocking side ([`blocking_side_of`]) |
/// |---|---|---|
/// | `NO_GOYA_CAPABILITY_*` (prefix) | `goya-capability-deficit` | goya-side |
/// | `NATIVE_MODULE_OPAQUE` | `goya-capability-deficit` | goya-side |
/// | `PERSISTENCE_CAPABILITY_UNRESOLVED` | `capability-decision-pending` | goya-side |
/// | `NO_COVERAGE_RULE` | `programme-debt` | programme-side |
/// | `UNREASONED` | `programme-debt` | programme-side |
/// | `HOOK_DATAFLOW_NOT_EXTRACTED` | `extraction-limit` | programme-side |
/// | `DYNAMIC_REQUIRE_UNRESOLVED` | `extraction-limit` | programme-side |
/// | `DYNAMIC_CODE_EVALUATION` | `extraction-limit` | programme-side |
/// | `MAPPED_TO_*` (prefix) | `mapping` | non-blocking |
/// | anything else | `unclassified` | unclassified |
pub fn category_of(reason: &str) -> &'static str {
    // Exact register codes first.
    match reason {
        "PERSISTENCE_CAPABILITY_UNRESOLVED" => return "capability-decision-pending",
        "NATIVE_MODULE_OPAQUE" => return "goya-capability-deficit",
        "NO_COVERAGE_RULE" | "UNREASONED" => return "programme-debt",
        "HOOK_DATAFLOW_NOT_EXTRACTED"
        | "DYNAMIC_REQUIRE_UNRESOLVED"
        | "DYNAMIC_CODE_EVALUATION" => return "extraction-limit",
        _ => {}
    }
    // Prefix families: every absence claim is a Goya-side deficit; every
    // mapping code records a non-blocking `representable` outcome.
    if reason.starts_with("NO_GOYA_CAPABILITY_") {
        return "goya-capability-deficit";
    }
    if reason.starts_with("MAPPED_TO_") {
        return "mapping";
    }
    "unclassified"
}

/// The two blocking categories of GY-COVERAGE/1, which the contract rules
/// must not be conflated: **goya-side** capability demand (categories
/// `goya-capability-deficit` and `capability-decision-pending`) versus
/// **programme-side** debt — extractor or map immaturity (categories
/// `programme-debt` and `extraction-limit`). Mapping codes block nothing;
/// unknown codes stay unclassified.
pub fn blocking_side_of(reason: &str) -> &'static str {
    match category_of(reason) {
        "goya-capability-deficit" | "capability-decision-pending" => "goya-side",
        "programme-debt" | "extraction-limit" => "programme-side",
        "mapping" => "non-blocking",
        _ => "unclassified",
    }
}

fn total(kinds: &BTreeMap<String, usize>) -> usize {
    kinds.values().sum()
}

/// serde snake_case name for any serialisable value (statuses, kinds).
fn serde_name<T: serde::Serialize>(value: &T) -> String {
    let json = serde_json::to_string(value).unwrap_or_default();
    json.trim_matches('"').to_string()
}

/// Convenience: load a map from a JSON file.
pub fn load_map(path: &Path) -> Result<CapabilityMap, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("reading {path:?}: {e}"))?;
    CapabilityMap::from_json(&json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_fair::{
        ElementProvenance, ExtractorIdentity, ProjectProvenance, SourceFile, SourceManifest,
        FAIR_CONTRACT,
    };

    fn element(kind: ElementKind, semantics: BTreeMap<String, SemanticsValue>) -> FairElement {
        FairElement {
            id: "test-0001".into(),
            kind,
            provenance: ElementProvenance {
                file: "src/main.ts".into(),
                line: 1,
                span_start: None,
                span_end: None,
                fallback_origin: false,
            },
            representability: Representability {
                status: RepresentabilityStatus::Extracted,
                reason: None,
                detail: None,
            },
            semantics,
        }
    }

    fn doc(elements: Vec<FairElement>) -> FairDocument {
        FairDocument {
            contract: FAIR_CONTRACT.into(),
            project: ProjectProvenance {
                name: "t".into(),
                frameworks: vec![],
                entry_points: vec![],
            },
            source: SourceManifest {
                root: "fixtures/t".into(),
                files: vec![SourceFile {
                    path: "src/main.ts".into(),
                    sha256: "a".repeat(64),
                }],
            },
            extractor: ExtractorIdentity {
                name: "t".into(),
                version: "0".into(),
                id_rule: "t".into(),
            },
            elements,
        }
    }

    #[test]
    fn default_map_is_valid() {
        assert_eq!(default_map().validate(), Ok(()));
    }

    #[test]
    fn extracted_component_becomes_representable_with_evidence() {
        let map = default_map();
        let assessment = assess(&doc(vec![element(ElementKind::Component, BTreeMap::new())]), &map)
            .expect("assessment succeeds");
        assert_eq!(assessment.upgraded, 1);
        let e = &assessment.document.elements[0];
        assert_eq!(e.representability.status, RepresentabilityStatus::Representable);
        assert!(e.semantics.contains_key("goya_evidence"));
        assert!(matches!(
            e.semantics.get("goya_evidence"),
            Some(SemanticsValue::Str(s)) if s.contains("GY-LANG")
        ));
    }

    #[test]
    fn child_process_is_declared_unrepresentable() {
        let map = default_map();
        let assessment = assess(&doc(vec![element(ElementKind::ChildProcess, BTreeMap::new())]), &map)
            .unwrap();
        assert_eq!(
            assessment.document.elements[0].representability,
            Representability {
                status: RepresentabilityStatus::Unrepresentable,
                reason: Some("NO_GOYA_CAPABILITY_CHILD_PROCESS".into()),
                detail: assessment.document.elements[0]
                    .representability
                    .detail
                    .clone(),
            }
        );
    }

    #[test]
    fn pre_existing_classification_is_preserved() {
        let map = default_map();
        let mut e = element(ElementKind::NativeModule, BTreeMap::new());
        e.representability = Representability {
            status: RepresentabilityStatus::Unrepresentable,
            reason: Some("NATIVE_MODULE_OPAQUE".into()),
            detail: None,
        };
        let assessment = assess(&doc(vec![e]), &map).unwrap();
        assert_eq!(assessment.untouched, 1);
        assert_eq!(assessment.upgraded, 0);
        assert_eq!(
            assessment.document.elements[0].representability.reason.as_deref(),
            Some("NATIVE_MODULE_OPAQUE")
        );
    }

    #[test]
    fn unmatched_kind_falls_to_no_coverage_rule() {
        let map = default_map();
        let assessment = assess(&doc(vec![element(ElementKind::PreloadBridge, BTreeMap::new())]), &map)
            .unwrap();
        assert_eq!(
            assessment.document.elements[0].representability.reason.as_deref(),
            Some("NO_COVERAGE_RULE")
        );
        // Fall-through is an upgrade (extracted -> undetermined), counted once.
        assert_eq!(assessment.upgraded, 1);
        assert_eq!(assessment.untouched, 0);
    }

    #[test]
    fn counters_partition_the_element_set() {
        let map = default_map();
        let mut preserved = element(ElementKind::NativeModule, BTreeMap::new());
        preserved.id = "p".into();
        preserved.representability = Representability {
            status: RepresentabilityStatus::Unrepresentable,
            reason: Some("NATIVE_MODULE_OPAQUE".into()),
            detail: None,
        };
        let mut component = element(ElementKind::Component, BTreeMap::new());
        component.id = "c".into();
        let mut bridge = element(ElementKind::PreloadBridge, BTreeMap::new());
        bridge.id = "b".into();
        let assessment = assess(
            &doc(vec![component, bridge, preserved]),
            &map,
        )
        .unwrap();
        assert_eq!(assessment.upgraded + assessment.untouched, assessment.document.elements.len());
        assert_eq!(assessment.upgraded, 2);
        assert_eq!(assessment.untouched, 1);
    }

    #[test]
    fn semantics_facet_matching_applies_first_match_wins() {
        let mut semantics = BTreeMap::new();
        semantics.insert("mechanism".into(), SemanticsValue::Str("styled-components".into()));
        let mut map = default_map();
        map.rules.insert(
            0,
            Rule {
                kind: Some(ElementKind::StyleRule),
                semantics: {
                    let mut m = BTreeMap::new();
                    m.insert("mechanism".into(), "styled-components".into());
                    m
                },
                status: RepresentabilityStatus::Undetermined,
                reason: Some("STYLED_COMPONENTS_DYNAMIC".into()),
                goya_evidence: "dynamic JS styling has no static scene equivalent".into(),
                detail: "runtime-computed styling is not mechanically expressible".into(),
            },
        );
        assert_eq!(map.validate(), Ok(()));
        let mut plain = element(ElementKind::StyleRule, BTreeMap::new());
        plain.id = "test-0002".into();
        let assessment = assess(
            &doc(vec![element(ElementKind::StyleRule, semantics), plain]),
            &map,
        )
        .unwrap();
        assert_eq!(
            assessment.document.elements[0].representability.reason.as_deref(),
            Some("STYLED_COMPONENTS_DYNAMIC")
        );
        assert_eq!(
            assessment.document.elements[1].representability.status,
            RepresentabilityStatus::Representable
        );
    }

    #[test]
    fn map_rejects_extracted_outcome_and_missing_evidence() {
        let json = r#"{"version":"1","rules":[{"kind":"component","semantics":{},"status":"extracted","reason":null,"goya_evidence":"x","detail":"d"}]}"#;
        assert!(CapabilityMap::from_json(json).is_err());
        let json = r#"{"version":"1","rules":[{"kind":"component","semantics":{},"status":"unrepresentable","reason":"X","goya_evidence":"","detail":"d"}]}"#;
        assert!(CapabilityMap::from_json(json).is_err());
    }

    #[test]
    fn deficit_report_aggregates_and_sorts_by_block_size() {
        let mut a = element(ElementKind::ChildProcess, BTreeMap::new());
        a.id = "a".into();
        let mut b = element(ElementKind::OsApiCall, BTreeMap::new());
        b.id = "b".into();
        let mut c = element(ElementKind::OsApiCall, BTreeMap::new());
        c.id = "c".into();
        let mut d = element(ElementKind::Component, BTreeMap::new());
        d.id = "d".into();
        let assessment =
            assess(&doc(vec![a, b, c, d]), &default_map()).unwrap();
        let report = DeficitReport::derive(&assessment.document);
        assert_eq!(report.deficits.len(), 2);
        assert_eq!(report.deficits[0].reason, "NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION");
        assert_eq!(report.deficits[0].kinds.get("os_api_call"), Some(&2));
        assert_eq!(report.deficits[1].reason, "NO_GOYA_CAPABILITY_CHILD_PROCESS");
        assert_eq!(report.totals.get("representable"), Some(&1));
        assert_eq!(report.totals.get("unrepresentable"), Some(&3));
        assert!(report.canonical_json().ends_with('\n'));
    }

    #[test]
    fn report_is_deterministic() {
        let mut a = element(ElementKind::ChildProcess, BTreeMap::new());
        a.id = "a".into();
        let assessment1 = assess(&doc(vec![a.clone()]), &default_map()).unwrap();
        let assessment2 = assess(&doc(vec![a]), &default_map()).unwrap();
        assert_eq!(
            DeficitReport::derive(&assessment1.document).canonical_json(),
            DeficitReport::derive(&assessment2.document).canonical_json()
        );
    }

    #[test]
    fn default_map_matches_embedded_fixture() {
        assert_eq!(
            serde_json::to_string_pretty(&default_map()).unwrap(),
            include_str!("../fixtures/capability-map-v1.json").trim()
        );
    }

    #[test]
    fn embedded_fixture_validates() {
        CapabilityMap::from_json(include_str!("../fixtures/capability-map-v1.json"))
            .expect("embedded capability-map fixture must parse and validate");
    }

    // --- corpus-level aggregation -------------------------------------

    /// Real evidence input, relative to the worktree root's `evidence/` dir.
    fn evidence_report(relative: &str) -> DeficitReport {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evidence/")
            .join(relative)
            .canonicalize()
            .expect("evidence file exists");
        let json = std::fs::read_to_string(&path).expect("evidence file readable");
        serde_json::from_str(&json).expect("evidence file parses as a deficit report")
    }

    const REACTOTRON_V2: &str = "disparates/reactotron-phase1/reference.map-v2.deficit.json";
    const FIDDLE_V2: &str = "disparates/electron-fiddle-phase2/extraction.map-v2.deficit.json";

    #[test]
    fn merge_sums_kinds_unions_files_keeps_first_evidence() {
        let mut base = DeficitEntry {
            reason: "X".into(),
            goya_evidence: "first evidence".into(),
            kinds: BTreeMap::from([
                ("hook".to_string(), 2),
                ("component".to_string(), 1),
            ]),
            files: vec!["b.ts".into(), "a.ts".into()],
        };
        let other = DeficitEntry {
            reason: "X".into(),
            goya_evidence: "conflicting second evidence — must be ignored".into(),
            kinds: BTreeMap::from([
                ("hook".to_string(), 3),
                ("state_store".to_string(), 4),
            ]),
            files: vec!["a.ts".into(), "c.ts".into()],
        };
        base.merge(&other);
        assert_eq!(base.kinds.get("hook"), Some(&5));
        assert_eq!(base.kinds.get("component"), Some(&1));
        assert_eq!(base.kinds.get("state_store"), Some(&4));
        assert_eq!(base.files, ["a.ts", "b.ts", "c.ts"]);
        assert_eq!(base.goya_evidence, "first evidence");
        // An empty first evidence is backfilled from the merged-in entry.
        let mut empty = DeficitEntry {
            reason: "Y".into(),
            goya_evidence: String::new(),
            kinds: BTreeMap::new(),
            files: Vec::new(),
        };
        let filled = DeficitEntry {
            reason: "Y".into(),
            goya_evidence: "later evidence".into(),
            kinds: BTreeMap::new(),
            files: Vec::new(),
        };
        empty.merge(&filled);
        assert_eq!(empty.goya_evidence, "later evidence");
    }

    #[test]
    fn aggregate_of_real_reactotron_and_fiddle_deficits() {
        let aggregate = DeficitReport::aggregate(&[
            evidence_report(REACTOTRON_V2),
            evidence_report(FIDDLE_V2),
        ]);
        assert_eq!(aggregate.contract, COVERAGE_CONTRACT);
        assert_eq!(aggregate.source_root, "aggregate(2 corpora)");
        assert_eq!(aggregate.elements, 746);
        assert_eq!(aggregate.totals.get("representable"), Some(&435));
        assert_eq!(aggregate.totals.get("undetermined"), Some(&231));
        assert_eq!(aggregate.totals.get("unrepresentable"), Some(&80));
        // Sorted by blocking count descending; all counts distinct here.
        let reasons: Vec<&str> =
            aggregate.deficits.iter().map(|entry| entry.reason.as_str()).collect();
        assert_eq!(
            reasons,
            [
                "PERSISTENCE_CAPABILITY_UNRESOLVED",
                "HOOK_DATAFLOW_NOT_EXTRACTED",
                "NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION",
                "DYNAMIC_REQUIRE_UNRESOLVED",
                "NO_GOYA_CAPABILITY_CHILD_PROCESS",
            ]
        );
        let blocked: Vec<usize> =
            aggregate.deficits.iter().map(|entry| total(&entry.kinds)).collect();
        assert_eq!(blocked, [139, 63, 61, 29, 19]);
        // Merged kind counts.
        assert_eq!(aggregate.deficits[0].kinds.get("filesystem_operation"), Some(&102));
        assert_eq!(aggregate.deficits[0].kinds.get("state_store"), Some(&37));
        assert_eq!(aggregate.deficits[1].kinds.get("hook"), Some(&63));
        assert_eq!(aggregate.deficits[2].kinds.get("os_api_call"), Some(&61));
        assert_eq!(aggregate.deficits[3].kinds.get("dynamic_module_resolution"), Some(&29));
        assert_eq!(aggregate.deficits[4].kinds.get("child_process"), Some(&19));
        // Merged file unions: 7+22, 17, 5+24 (one shared path: src/main/menu.ts),
        // 16, 3+6.
        let file_counts: Vec<usize> =
            aggregate.deficits.iter().map(|entry| entry.files.len()).collect();
        assert_eq!(file_counts, [29, 17, 28, 16, 9]);
        assert!(aggregate.deficits[0]
            .files
            .iter()
            .all(|pair| aggregate.deficits[0].files.iter().filter(|f| *f == pair).count() == 1));
        // Evidence: first non-empty kept; extractor-level deficit has none.
        assert_eq!(
            aggregate.deficits[0].goya_evidence,
            "no filesystem-access contract exists in the register (checked 2026-11-19)"
        );
        assert_eq!(aggregate.deficits[3].goya_evidence, "");
        // Every blocked element is accounted for.
        assert_eq!(blocked.iter().sum::<usize>(), 231 + 80);
    }

    #[test]
    #[should_panic(expected = "aggregate of zero corpora is not defined")]
    fn aggregate_of_zero_reports_is_an_error() {
        let _ = DeficitReport::aggregate(&[]);
    }

    #[test]
    fn aggregate_of_one_report_is_byte_identical() {
        for relative in [REACTOTRON_V2, FIDDLE_V2] {
            let report = evidence_report(relative);
            let identity = DeficitReport::aggregate(std::slice::from_ref(&report));
            assert_eq!(identity.canonical_json(), report.canonical_json());
        }
    }

    #[test]
    fn aggregate_is_deterministic_independent_of_input_order() {
        let reactotron = evidence_report(REACTOTRON_V2);
        let fiddle = evidence_report(FIDDLE_V2);
        let ab = DeficitReport::aggregate(&[reactotron.clone(), fiddle.clone()]);
        // All merged blocking counts are distinct, so input order cannot
        // affect the result.
        let ba = DeficitReport::aggregate(&[fiddle, reactotron]);
        assert_eq!(ab.canonical_json(), ba.canonical_json());
        assert_eq!(ab.canonical_json(), ab.canonical_json());
    }

    #[test]
    fn category_of_follows_the_reason_code_register() {
        let cases: &[(&str, &str, &str)] = &[
            (
                "NO_GOYA_CAPABILITY_CHILD_PROCESS",
                "goya-capability-deficit",
                "goya-side",
            ),
            (
                "NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION",
                "goya-capability-deficit",
                "goya-side",
            ),
            ("NATIVE_MODULE_OPAQUE", "goya-capability-deficit", "goya-side"),
            (
                "PERSISTENCE_CAPABILITY_UNRESOLVED",
                "capability-decision-pending",
                "goya-side",
            ),
            ("NO_COVERAGE_RULE", "programme-debt", "programme-side"),
            ("UNREASONED", "programme-debt", "programme-side"),
            ("HOOK_DATAFLOW_NOT_EXTRACTED", "extraction-limit", "programme-side"),
            ("DYNAMIC_REQUIRE_UNRESOLVED", "extraction-limit", "programme-side"),
            ("DYNAMIC_CODE_EVALUATION", "extraction-limit", "programme-side"),
            ("MAPPED_TO_CLIPBOARD_CAPABILITY", "mapping", "non-blocking"),
            ("MAPPED_TO_SCENE_DECLARATION", "mapping", "non-blocking"),
            ("SOME_FUTURE_UNREGISTERED_CODE", "unclassified", "unclassified"),
            ("", "unclassified", "unclassified"),
        ];
        for (reason, category, side) in cases {
            assert_eq!(category_of(reason), *category, "category of {reason:?}");
            assert_eq!(blocking_side_of(reason), *side, "side of {reason:?}");
        }
    }

    #[test]
    fn aggregate_category_subtotals_of_real_reports() {
        let aggregate = DeficitReport::aggregate(&[
            evidence_report(REACTOTRON_V2),
            evidence_report(FIDDLE_V2),
        ]);
        let subtotal = |category: &str| {
            aggregate
                .category_subtotals()
                .iter()
                .find(|(found, _)| *found == category)
                .map(|(_, count)| *count)
        };
        assert_eq!(subtotal("goya-capability-deficit"), Some(80));
        assert_eq!(subtotal("capability-decision-pending"), Some(139));
        assert_eq!(subtotal("programme-debt"), Some(0));
        assert_eq!(subtotal("extraction-limit"), Some(92));
        assert_eq!(subtotal("mapping"), Some(0));
        assert_eq!(subtotal("unclassified"), Some(0));
        assert_eq!(aggregate.blocking_split(), (219, 92));
    }

    #[test]
    fn unknown_reason_codes_subtotal_as_unclassified() {
        let report = DeficitReport {
            contract: COVERAGE_CONTRACT.into(),
            source_root: "synthetic".into(),
            elements: 1,
            totals: BTreeMap::from([("undetermined".to_string(), 1)]),
            deficits: vec![DeficitEntry {
                reason: "SOME_FUTURE_CODE".into(),
                goya_evidence: String::new(),
                kinds: BTreeMap::from([("hook".to_string(), 1)]),
                files: vec!["x.ts".into()],
            }],
        };
        assert_eq!(category_of("SOME_FUTURE_CODE"), "unclassified");
        let subtotals = report.category_subtotals();
        assert_eq!(
            subtotals.iter().find(|(category, _)| *category == "unclassified"),
            Some(&("unclassified", 1))
        );
        assert_eq!(report.blocking_split(), (0, 0));
    }
}
