//! Admission and serialization for Phase 1 analysis evidence.

use crate::{Diagnostic, Span, PROFILE};
use goya_fair::{ElementKind, FairDocument, RepresentabilityStatus, SemanticsValue, FAIR_CONTRACT};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const BUNDLE_CONTRACT: &str = "GY-CORTADOS-BUNDLE/0.1";
pub const CSIR_CONTRACT: &str = "GY-CSIR-JAVA/0.1";
pub const DIAGNOSTICS_CONTRACT: &str = "GY-CORTADOS-DIAGNOSTICS/0.1";
pub const DEPENDENCIES_CONTRACT: &str = "GY-CORTADOS-DEPENDENCIES/0.1";
pub const CAPABILITIES_CONTRACT: &str = "GY-CORTADOS-CAPABILITIES/0.1";
pub(crate) const FAIR_PATH: &str = "application.fair.json";
pub(crate) const CSIR_PATH: &str = "application.csir.json";
const MANIFEST_PATH: &str = "bundle.json";
const DIAGNOSTICS_PATH: &str = "diagnostics.json";
const DEPENDENCIES_PATH: &str = "dependencies.json";
const CAPABILITIES_PATH: &str = "capabilities.json";
const PRODUCER: &str = "goya-cortados";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const ARTIFACTS: [(&str, &str, &str); 5] = [
    (CSIR_PATH, "semantic_sidecar", CSIR_CONTRACT),
    (FAIR_PATH, "foreign_application_ir", FAIR_CONTRACT),
    (CAPABILITIES_PATH, "capabilities", CAPABILITIES_CONTRACT),
    (DEPENDENCIES_PATH, "dependencies", DEPENDENCIES_CONTRACT),
    (DIAGNOSTICS_PATH, "diagnostics", DIAGNOSTICS_CONTRACT),
];

/// The six versioned reports, bound to their exact original source bytes.
#[derive(Debug)]
pub struct AnalysisBundle {
    artifacts: BTreeMap<String, Vec<u8>>,
    sources: BTreeMap<String, Vec<u8>>,
    retained_csir_sha256: String,
}

type Sources = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceDigest {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceStatus {
    pub path: String,
    pub syntax_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Exclusion {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Origin {
    pub path: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub raw_sha256: String,
}

impl Origin {
    pub(crate) fn new(path: &str, bytes: &[u8], span: Span) -> Self {
        Self {
            path: path.into(),
            start_byte: span.start,
            end_byte: span.end,
            raw_sha256: sha256(&bytes[span.start..span.end]),
        }
    }

    fn span(&self) -> Span {
        Span {
            start: self.start_byte,
            end: self.end_byte,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Domain {
    Type,
    Identity,
    Control,
    Memory,
    Effects,
    Concurrency,
    Lifecycle,
    Interface,
    Graphics,
    Dependency,
    Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Unit,
    Type,
    Object,
    Procedure,
    Operation,
    Allocation,
    Effect,
    Concurrency,
    Lifecycle,
    Interface,
    Graphics,
    Dependency,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Resolution {
    Unknown,
    SuppliedDeclaration {
        target_node_id: String,
        basis: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Payload {
    pub syntax_kind: String,
    pub name: Option<String>,
    pub fields: BTreeMap<String, String>,
    pub line: u64,
    pub observations: Vec<String>,
    pub resolution: Option<Resolution>,
    pub encoding: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Node {
    pub id: String,
    pub domain: Domain,
    pub kind: Kind,
    pub level: String,
    pub origin: Origin,
    pub fair_element_id: Option<String>,
    pub payload: Payload,
    pub unresolved: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Edge {
    pub id: String,
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub evidence: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attachment {
    path: String,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frontend {
    construction: String,
    name: String,
    version: String,
}

// Phase 1 admits no transformation records. This field remains an empty JSON array.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Csir {
    contract: String,
    profile_id: String,
    frontend: Frontend,
    source_manifest_sha256: String,
    sources: Vec<SourceDigest>,
    fair: Attachment,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    transformations: Vec<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Finding {
    pub code: String,
    pub severity: String,
    pub path: String,
    pub span: Option<Span>,
    pub requirement: String,
    pub message: String,
    pub remediation: String,
    pub attempted_pass: String,
    pub subject_ids: Vec<String>,
}

impl Finding {
    pub(crate) fn from_diagnostic(
        diagnostic: Diagnostic,
        pass: &str,
        subject: Option<&str>,
    ) -> Self {
        Self {
            code: diagnostic.code,
            severity: diagnostic.severity,
            path: diagnostic.path,
            span: diagnostic.span,
            requirement: diagnostic.requirement,
            message: diagnostic.message,
            remediation: diagnostic.remediation,
            attempted_pass: pass.into(),
            subject_ids: subject.into_iter().map(str::to_owned).collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Diagnostics {
    contract: String,
    diagnostics: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dependency {
    pub id: String,
    pub identity: String,
    pub observed_as: String,
    pub version: Option<String>,
    pub license: Option<String>,
    pub attribution: Vec<String>,
    pub source_sha256: String,
    pub artifact_sha256: Option<String>,
    pub origin: Option<Origin>,
    pub subject_ids: Vec<String>,
    pub reachable_members: Vec<String>,
    pub reachability_assumptions: Vec<String>,
    pub unknown_reachability: bool,
    pub attempted_strategies: Vec<String>,
    pub selected_strategy: Option<String>,
    pub verification_evidence: Vec<String>,
    pub residual_requirements: Vec<String>,
    pub authority: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Dependencies {
    contract: String,
    dependencies: Vec<Dependency>,
    original_notices: Vec<SourceDigest>,
    jdk_symbols_supplied: bool,
    external_symbols_supplied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capability {
    pub id: String,
    pub operation: String,
    pub scope: String,
    pub status: String,
    pub origin: Origin,
    pub subject_ids: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Capabilities {
    contract: String,
    requirements: Vec<Capability>,
    grants: Vec<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: String,
    kind: String,
    contract: String,
    sha256: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Producer {
    name: String,
    version: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completeness {
    syntax_complete: bool,
    semantic_complete: bool,
    complete: bool,
    native_conversion_available: bool,
    certification: Option<()>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    contract: String,
    profile_id: String,
    producer: Producer,
    passes: BTreeMap<String, String>,
    artifacts: Vec<Artifact>,
    source_manifest_sha256: String,
    sources: Vec<SourceDigest>,
    excluded: Vec<Exclusion>,
    source_syntax: Vec<SourceStatus>,
    completeness: Completeness,
}

#[derive(Default)]
pub(crate) struct Reports {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub diagnostics: Vec<Finding>,
    pub dependencies: Vec<Dependency>,
    pub capabilities: Vec<Capability>,
    pub original_notices: Vec<SourceDigest>,
    pub excluded: Vec<Exclusion>,
    pub source_syntax: Vec<SourceStatus>,
}

impl AnalysisBundle {
    pub(crate) fn new(
        sources: Sources,
        fair: FairDocument,
        mut reports: Reports,
    ) -> Result<Self, Vec<Diagnostic>> {
        let sources_inventory = inventory(&sources);
        let source_manifest_sha256 = sha256(&canonical(&sources_inventory));
        let fair_bytes = fair.canonical_json().into_bytes();
        reports.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        reports.edges.sort_by(|a, b| a.id.cmp(&b.id));
        reports.dependencies.sort_by(|a, b| a.id.cmp(&b.id));
        reports.capabilities.sort_by(|a, b| a.id.cmp(&b.id));
        sort_findings(&mut reports.diagnostics);
        let csir = Csir {
            contract: CSIR_CONTRACT.into(),
            profile_id: PROFILE.into(),
            frontend: Frontend {
                construction: "static_analysis".into(),
                name: PRODUCER.into(),
                version: VERSION.into(),
            },
            source_manifest_sha256: source_manifest_sha256.clone(),
            sources: sources_inventory.clone(),
            fair: Attachment {
                path: FAIR_PATH.into(),
                sha256: sha256(&fair_bytes),
            },
            nodes: reports.nodes,
            edges: reports.edges,
            transformations: Vec::new(),
        };
        let csir_bytes = canonical(&csir);
        let retained_csir_sha256 = sha256(&csir_bytes);
        let mut artifacts = BTreeMap::from([
            (FAIR_PATH.into(), fair_bytes),
            (CSIR_PATH.into(), csir_bytes),
            (
                DIAGNOSTICS_PATH.into(),
                canonical(&Diagnostics {
                    contract: DIAGNOSTICS_CONTRACT.into(),
                    diagnostics: reports.diagnostics,
                }),
            ),
            (
                DEPENDENCIES_PATH.into(),
                canonical(&Dependencies {
                    contract: DEPENDENCIES_CONTRACT.into(),
                    dependencies: reports.dependencies,
                    original_notices: reports.original_notices,
                    jdk_symbols_supplied: false,
                    external_symbols_supplied: false,
                }),
            ),
            (
                CAPABILITIES_PATH.into(),
                canonical(&Capabilities {
                    contract: CAPABILITIES_CONTRACT.into(),
                    requirements: reports.capabilities,
                    grants: Vec::new(),
                }),
            ),
        ]);
        let manifest = Manifest {
            contract: BUNDLE_CONTRACT.into(),
            profile_id: PROFILE.into(),
            producer: producer(),
            passes: passes(),
            artifacts: artifact_inventory(&artifacts)?,
            source_manifest_sha256,
            sources: sources_inventory,
            excluded: reports.excluded,
            completeness: Completeness {
                syntax_complete: reports
                    .source_syntax
                    .iter()
                    .all(|record| record.syntax_complete),
                semantic_complete: false,
                complete: false,
                native_conversion_available: false,
                certification: None,
            },
            source_syntax: reports.source_syntax,
        };
        artifacts.insert(MANIFEST_PATH.into(), canonical(&manifest));
        let bundle = Self {
            artifacts,
            sources,
            retained_csir_sha256,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    /// Returns canonical artifact bytes keyed by their fixed relative paths.
    pub fn artifacts(&self) -> BTreeMap<String, Vec<u8>> {
        self.artifacts.clone()
    }

    /// Checks the reports against their retained source and extraction evidence.
    pub fn validate(&self) -> Result<(), Vec<Diagnostic>> {
        validate_artifacts(&self.artifacts, &self.sources)?;
        require(
            sha256(&self.artifacts[CSIR_PATH]) == self.retained_csir_sha256,
            CSIR_PATH,
            "CSIR differs from the retained parser observations",
        )
    }
}

/// Checks detached evidence using the supplied exact original bytes. This validates
/// structural witnesses and provenance. It does not re-run Java attribution.
pub fn validate_artifacts(
    artifacts: &BTreeMap<String, Vec<u8>>,
    sources: &BTreeMap<String, Vec<u8>>,
) -> Result<(), Vec<Diagnostic>> {
    let manifest: Manifest = read(artifacts, MANIFEST_PATH)?;
    require(
        artifacts.len() == ARTIFACTS.len() + 1
            && manifest.contract == BUNDLE_CONTRACT
            && manifest.profile_id == PROFILE
            && manifest.producer == producer()
            && manifest.passes == passes()
            && manifest.artifacts == artifact_inventory(artifacts)?,
        MANIFEST_PATH,
        "artifact inventory, contract, producer or pass versions do not match",
    )?;
    require(
        sources.keys().all(|path| safe_path(path))
            && inventory(sources) == manifest.sources
            && sha256(&canonical(&manifest.sources)) == manifest.source_manifest_sha256,
        MANIFEST_PATH,
        "source inventory or digest does not match the original bytes",
    )?;
    require(
        strictly_sorted(manifest.excluded.iter().map(|item| &item.path))
            && manifest.excluded.iter().all(|item| {
                safe_path(&item.path)
                    && !item.reason.is_empty()
                    && !sources.contains_key(&item.path)
            }),
        MANIFEST_PATH,
        "exclusion inventory is invalid",
    )?;
    let java_paths: Vec<&String> = sources
        .keys()
        .filter(|path| path.ends_with(".java"))
        .collect();
    require(
        java_paths.iter().any(|path| !sources[*path].is_empty())
            && manifest
                .source_syntax
                .iter()
                .map(|item| &item.path)
                .eq(java_paths)
            && manifest.completeness.syntax_complete
                == manifest
                    .source_syntax
                    .iter()
                    .all(|item| item.syntax_complete)
            && !manifest.completeness.semantic_complete
            && !manifest.completeness.complete
            && !manifest.completeness.native_conversion_available
            && manifest.completeness.certification.is_none(),
        MANIFEST_PATH,
        "source syntax inventory or completeness claims are invalid",
    )?;

    let fair: FairDocument = decode(artifacts, FAIR_PATH)?;
    if let Err(errors) = fair.validate() {
        return Err(errors
            .into_iter()
            .map(|error| {
                Diagnostic::new(
                    "COR-E700",
                    FAIR_PATH,
                    None,
                    format!("{}: {}", error.code, error.message),
                )
            })
            .collect());
    }
    require(
        fair.is_fair2()
            && fair.canonical_json().as_bytes() == artifacts[FAIR_PATH]
            && fair.source.root == "application"
            && fair
                .source
                .files
                .iter()
                .map(|file| (&file.path, &file.sha256))
                .eq(manifest
                    .sources
                    .iter()
                    .map(|file| (&file.path, &file.sha256)))
            && fair.extractor.name == PRODUCER
            && fair.extractor.version == VERSION
            && fair.project.is_empty(),
        FAIR_PATH,
        "FAIR canonical bytes, source binding or extractor identity do not match",
    )?;

    let csir: Csir = read(artifacts, CSIR_PATH)?;
    require(
        csir.contract == CSIR_CONTRACT
            && csir.profile_id == PROFILE
            && csir.frontend.construction == "static_analysis"
            && csir.frontend.name == PRODUCER
            && csir.frontend.version == VERSION
            && csir.fair.path == FAIR_PATH
            && csir.fair.sha256 == sha256(&artifacts[FAIR_PATH])
            && csir.sources == manifest.sources
            && csir.source_manifest_sha256 == manifest.source_manifest_sha256
            && csir.transformations.is_empty(),
        CSIR_PATH,
        "CSIR contract, attachment or source binding is invalid",
    )?;

    let observed_paths: BTreeSet<_> = csir
        .nodes
        .iter()
        .map(|node| node.origin.path.as_str())
        .collect();
    let validated_sources = sources
        .iter()
        .filter(|(path, _)| path.ends_with(".java") || observed_paths.contains(path.as_str()))
        .map(|(path, bytes)| (path.as_str(), SourceView::new(bytes)))
        .collect();
    let nodes = validate_graph(&csir, &fair, &manifest, &validated_sources)?;
    let diagnostics: Diagnostics = read(artifacts, DIAGNOSTICS_PATH)?;
    validate_diagnostics(diagnostics, &manifest, sources, &nodes)?;

    let dependencies: Dependencies = read(artifacts, DEPENDENCIES_PATH)?;
    validate_dependencies(dependencies, &manifest, &nodes)?;
    let capabilities: Capabilities = read(artifacts, CAPABILITIES_PATH)?;
    let operations: BTreeSet<_> = capabilities
        .requirements
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    require(
        capabilities.contract == CAPABILITIES_CONTRACT
            && capabilities.grants.is_empty()
            && strictly_sorted(capabilities.requirements.iter().map(|item| &item.id))
            && capabilities.requirements.iter().all(|item| {
                valid_id(&item.id)
                    && !item.operation.is_empty()
                    && item.scope == "source_observation"
                    && item.status == "unresolved"
                    && item.subject_ids.len() == 1
                    && nodes.get(item.subject_ids[0].as_str()).is_some_and(|node| {
                        node.origin == item.origin
                            && item.id == format!("cap.{}.{}", node.id, item.operation)
                    })
            })
            && nodes.values().all(|node| {
                let includes = |operation| {
                    operations.contains(format!("cap.{}.{operation}", node.id).as_str())
                };
                let invocation = is_invocation(&node.payload.syntax_kind);
                (!invocation || includes("invocation_effects"))
                    && [
                        ("COR-E210: ", "dynamic_resolution"),
                        ("COR-E220: ", "native_interop"),
                    ]
                    .iter()
                    .all(|&(code, operation)| {
                        !node
                            .unresolved
                            .iter()
                            .any(|reason| reason.starts_with(code))
                            || includes(operation)
                    })
            }),
        CAPABILITIES_PATH,
        "capability requirements must retain source observations without authority grants",
    )
}

fn validate_graph<'a>(
    csir: &'a Csir,
    fair: &FairDocument,
    manifest: &Manifest,
    sources: &BTreeMap<&str, SourceView<'_>>,
) -> Result<BTreeMap<&'a str, &'a Node>, Vec<Diagnostic>> {
    let nodes: BTreeMap<_, _> = csir
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let elements: BTreeMap<_, _> = fair
        .elements
        .iter()
        .map(|element| (element.id.as_str(), element))
        .collect();
    let parents = validate_edges(&csir.edges, &nodes, fair)?;
    require(
        strictly_sorted(csir.nodes.iter().map(|node| &node.id))
            && strictly_sorted(csir.edges.iter().map(|edge| &edge.id)),
        CSIR_PATH,
        "CSIR node and edge identifiers must be unique and sorted",
    )?;
    for node in &csir.nodes {
        let origin = &node.origin;
        let Some(source) = sources.get(origin.path.as_str()) else {
            return Err(admission_error(
                CSIR_PATH,
                "CSIR origin is outside the source inventory",
            ));
        };
        require(
            valid_id(&node.id)
                && node.level == "implementation"
                && !node.payload.syntax_kind.is_empty()
                && strictly_sorted(node.payload.observations.iter())
                && strictly_sorted(node.unresolved.iter())
                && node.unresolved.iter().all(|reason| !reason.is_empty())
                && (node.kind != Kind::Unresolved || !node.unresolved.is_empty())
                && source.valid_origin(origin, &node.payload),
            CSIR_PATH,
            format!("invalid CSIR origin or opaque node {}", node.id),
        )?;
        let type_parameter_name = node.payload.syntax_kind == "type_identifier"
            && parents
                .get(node.id.as_str())
                .is_some_and(|parent| parent.payload.syntax_kind == "type_parameter");
        let reference =
            reference_syntax(&node.payload.syntax_kind).filter(|_| !type_parameter_name);
        require(
            evidence_ids(&node.id, &node.evidence, &nodes)
                && reference.is_some() == node.payload.resolution.is_some()
                && reference
                    .is_none_or(|role| node.payload.observations.iter().any(|value| value == role))
                && match &node.payload.resolution {
                    Some(Resolution::Unknown) => {
                        node.evidence.is_empty()
                            && node
                                .unresolved
                                .iter()
                                .any(|reason| reason.starts_with("COR-E110: "))
                    }
                    Some(Resolution::SuppliedDeclaration {
                        target_node_id,
                        basis,
                    }) => {
                        manifest.source_syntax.len() == 1
                            && manifest.completeness.syntax_complete
                            && basis == "unique_top_level_same_unit_header"
                            && node.payload.syntax_kind == "type_identifier"
                            && node.evidence.as_slice() == [target_node_id.as_str()]
                            && nodes.get(target_node_id.as_str()).is_some_and(|target| {
                                target.origin.path == origin.path
                                    && target.kind == Kind::Type
                                    && node.payload.name.is_some()
                                    && node.payload.name == target.payload.name
                                    && parents.get(target.id.as_str()).is_some_and(|parent| {
                                        parent.payload.syntax_kind == "program"
                                    })
                                    && valid_header_target(node, target, &parents)
                            })
                    }
                    None => node.evidence.is_empty(),
                },
            CSIR_PATH,
            format!("invalid resolution evidence on {}", node.id),
        )?;
        let Some(element) = elements.get(node.id.as_str()) else {
            return Err(admission_error(
                CSIR_PATH,
                format!("missing FAIR element for {}", node.id),
            ));
        };
        let provenance = &element.provenance;
        let expected_kind = fair_kind(&node.payload.syntax_kind);
        let classification = &element.representability;
        let disposition_matches = if expected_kind == ElementKind::Unknown {
            classification.status == RepresentabilityStatus::Undetermined
                && classification.reason.as_deref() == Some("NO_FAIR_KIND")
        } else {
            classification.status == RepresentabilityStatus::Extracted
                && classification.reason.is_none()
        };
        require(
            node.fair_element_id.as_deref() == Some(node.id.as_str())
                && provenance.file == origin.path
                && provenance.line == node.payload.line
                && provenance.span_start == Some(origin.start_byte as u64)
                && provenance.span_end == Some(origin.end_byte as u64)
                && !provenance.fallback_origin
                && element.kind == expected_kind
                && disposition_matches
                && element.semantics.get("observed")
                    == Some(&SemanticsValue::Str(node.payload.syntax_kind.clone()))
                && element.semantics.get("raw_sha256")
                    == Some(&SemanticsValue::Str(origin.raw_sha256.clone()))
                && element.semantics.get("unresolved")
                    == Some(&SemanticsValue::List(
                        node.unresolved
                            .iter()
                            .cloned()
                            .map(SemanticsValue::Str)
                            .collect(),
                    )),
            CSIR_PATH,
            format!(
                "FAIR provenance, authority or unresolved facts differ for {}",
                node.id
            ),
        )?;
    }
    // Every nonempty Java source has an original-byte unit even on parse failure.
    let units: BTreeMap<_, _> = csir
        .nodes
        .iter()
        .filter(|node| node.payload.syntax_kind == "source_file")
        .map(|node| (node.origin.path.as_str(), node))
        .collect();
    require(
        manifest.source_syntax.iter().all(|item| {
            let source = &sources[item.path.as_str()];
            source.bytes.is_empty()
                || units.get(item.path.as_str()).is_some_and(|unit| {
                    unit.origin.start_byte == 0
                        && unit.origin.end_byte == source.bytes.len()
                        && unit.kind
                            == if item.syntax_complete {
                                Kind::Unit
                            } else {
                                Kind::Unresolved
                            }
                        && (source.utf8 || !item.syntax_complete)
                })
        }) && nodes.len() == elements.len()
            && acyclic(&nodes),
        CSIR_PATH,
        "source unit coverage, FAIR attachments or proof dependencies are invalid",
    )?;
    Ok(nodes)
}

fn valid_header_target(node: &Node, target: &Node, parents: &BTreeMap<&str, &Node>) -> bool {
    let Some(clause) = parents.get(node.id.as_str()) else {
        return false;
    };
    let clause = if clause.payload.syntax_kind == "type_list" {
        let Some(parent) = parents.get(clause.id.as_str()) else {
            return false;
        };
        parent
    } else {
        clause
    };
    let kind = match clause.payload.syntax_kind.as_str() {
        "superclass" => "class_declaration",
        "super_interfaces" | "extends_interfaces" => "interface_declaration",
        _ => return false,
    };
    target.payload.syntax_kind == kind
        && parents.get(clause.id.as_str()).is_some_and(|owner| {
            owner.id != target.id
                && parents
                    .get(owner.id.as_str())
                    .is_some_and(|program| program.payload.syntax_kind == "program")
        })
}

fn validate_edges<'a>(
    edges: &'a [Edge],
    nodes: &BTreeMap<&str, &'a Node>,
    fair: &FairDocument,
) -> Result<BTreeMap<&'a str, &'a Node>, Vec<Diagnostic>> {
    let relationships: BTreeMap<_, _> = fair
        .relationships
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    require(
        edges.len() == relationships.len(),
        CSIR_PATH,
        "FAIR containment inventory differs",
    )?;
    let mut parents = BTreeMap::new();
    for edge in edges {
        let (Some(subject), Some(object), Some(relationship)) = (
            nodes.get(edge.subject.as_str()),
            nodes.get(edge.object.as_str()),
            relationships.get(edge.id.as_str()),
        ) else {
            return Err(admission_error(
                CSIR_PATH,
                format!("dangling endpoint on {}", edge.id),
            ));
        };
        let provenance = &relationship.provenance;
        require(
            valid_id(&edge.id)
                && !nodes.contains_key(edge.id.as_str())
                && edge.relation == "contains"
                && edge.subject < edge.object
                && parents.insert(edge.object.as_str(), *subject).is_none()
                && edge.evidence.as_slice() == [edge.object.as_str()]
                && subject.origin.path == object.origin.path
                && subject.origin.start_byte <= object.origin.start_byte
                && object.origin.end_byte <= subject.origin.end_byte
                && relationship.relation == goya_fair::Relation::Contains
                && relationship.basis == goya_fair::Basis::Extracted
                && subject.fair_element_id.as_deref() == Some(relationship.subject.as_str())
                && object.fair_element_id.as_deref() == Some(relationship.object.as_str())
                && relationship.evidence.as_slice() == [relationship.object.as_str()]
                && provenance.file == object.origin.path
                && provenance.line == object.payload.line
                && provenance.span_start == Some(object.origin.start_byte as u64)
                && provenance.span_end == Some(object.origin.end_byte as u64)
                && !provenance.fallback_origin,
            CSIR_PATH,
            format!("invalid containment witness {}", edge.id),
        )?;
    }
    require(
        nodes.values().all(|node| {
            matches!(
                node.payload.syntax_kind.as_str(),
                "source_file" | "build_metadata"
            ) || parents.contains_key(node.id.as_str())
        }),
        CSIR_PATH,
        "a Java syntax observation is detached from its compilation unit",
    )?;
    Ok(parents)
}

fn validate_diagnostics(
    reports: Diagnostics,
    manifest: &Manifest,
    sources: &Sources,
    nodes: &BTreeMap<&str, &Node>,
) -> Result<(), Vec<Diagnostic>> {
    let mut sorted = reports.diagnostics.clone();
    sort_findings(&mut sorted);
    let mut referenced = BTreeSet::new();
    require(
        reports.contract == DIAGNOSTICS_CONTRACT
            && reports.diagnostics == sorted
            && reports.diagnostics.iter().all(|item| {
                const CODES: &[&str] = &[
                    "COR-E100", "COR-E110", "COR-E120", "COR-E200", "COR-E210", "COR-E220",
                    "COR-E300", "COR-E310", "COR-E320", "COR-E400", "COR-E410", "COR-E500",
                    "COR-E600", "COR-E700",
                ];
                CODES.contains(&item.code.as_str())
                    && matches!(item.severity.as_str(), "error" | "warning" | "note")
                    && (item.path == "." || safe_path(&item.path))
                    && item.span.is_none_or(|span| {
                        sources
                            .get(&item.path)
                            .is_some_and(|bytes| span.start <= span.end && span.end <= bytes.len())
                    })
                    && manifest.passes.contains_key(&item.attempted_pass)
                    && !item.requirement.is_empty()
                    && !item.message.is_empty()
                    && !item.remediation.is_empty()
                    && strictly_sorted(item.subject_ids.iter())
                    && item.subject_ids.iter().all(|id| {
                        let found = nodes.get(id.as_str()).is_some_and(|node| {
                            item.path == node.origin.path && item.span == Some(node.origin.span())
                        });
                        if found {
                            referenced.insert((
                                id.as_str(),
                                item.code.as_str(),
                                item.message.as_str(),
                            ));
                        }
                        found
                    })
            })
            && nodes.values().all(|node| {
                node.unresolved.iter().all(|reason| {
                    reason.split_once(": ").is_some_and(|(code, message)| {
                        referenced.contains(&(node.id.as_str(), code, message))
                    })
                })
            }),
        DIAGNOSTICS_PATH,
        "diagnostics are malformed or do not retain all unresolved evidence",
    )
}

fn validate_dependencies(
    reports: Dependencies,
    manifest: &Manifest,
    nodes: &BTreeMap<&str, &Node>,
) -> Result<(), Vec<Diagnostic>> {
    require(
        reports.contract == DEPENDENCIES_CONTRACT
            && !reports.jdk_symbols_supplied
            && !reports.external_symbols_supplied
            && strictly_sorted(reports.original_notices.iter().map(|notice| &notice.path))
            && reports
                .original_notices
                .iter()
                .all(|notice| manifest.sources.contains(notice))
            && strictly_sorted(reports.dependencies.iter().map(|item| &item.id))
            && reports.dependencies.iter().all(|item| {
                valid_id(&item.id)
                    && !item.identity.is_empty()
                    && !item.observed_as.is_empty()
                    && !item.residual_requirements.is_empty()
                    && item.version.is_none()
                    && item.license.is_none()
                    && item.attribution.is_empty()
                    && item.artifact_sha256.is_none()
                    && item.reachable_members.is_empty()
                    && item.unknown_reachability
                    && item.attempted_strategies.is_empty()
                    && item.selected_strategy.is_none()
                    && item.verification_evidence.is_empty()
                    && item.authority == "source_observation"
                    && strictly_sorted(item.subject_ids.iter())
                    && match &item.origin {
                        Some(origin) => {
                            manifest.sources.iter().any(|file| {
                                file.path == origin.path && file.sha256 == item.source_sha256
                            }) && !item.subject_ids.is_empty()
                                && item.subject_ids.iter().all(|id| {
                                    nodes
                                        .get(id.as_str())
                                        .is_some_and(|node| node.origin == *origin)
                                })
                        }
                        None => {
                            item.subject_ids.is_empty()
                                && manifest.sources.iter().any(|file| {
                                    file.path == item.identity && file.sha256 == item.source_sha256
                                })
                        }
                    }
            }),
        DEPENDENCIES_PATH,
        "dependency records must preserve source bindings and unknown provenance",
    )
}

fn evidence_ids(id: &str, evidence: &[String], nodes: &BTreeMap<&str, &Node>) -> bool {
    strictly_sorted(evidence.iter())
        && evidence
            .iter()
            .all(|item| item != id && nodes.contains_key(item.as_str()))
}

fn acyclic(nodes: &BTreeMap<&str, &Node>) -> bool {
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut counts: BTreeMap<_, _> = nodes
        .iter()
        .map(|(&id, node)| (id, node.evidence.len()))
        .collect();
    let mut ready: Vec<_> = counts
        .iter()
        .filter_map(|(&id, &count)| (count == 0).then_some(id))
        .collect();
    let mut visited = 0;
    for (&id, node) in nodes {
        for parent in &node.evidence {
            dependents.entry(parent).or_default().push(id);
        }
    }
    while let Some(id) = ready.pop() {
        visited += 1;
        for dependent in dependents.remove(id).into_iter().flatten() {
            if let Some(count) = counts.get_mut(dependent) {
                *count -= 1;
                if *count == 0 {
                    ready.push(dependent);
                }
            }
        }
    }
    visited == nodes.len()
}

pub(crate) struct SourceView<'a> {
    pub bytes: &'a [u8],
    pub utf8: bool,
    lines_at_checkpoint: Vec<u64>,
}

const LINE_CHECKPOINT_BYTES: usize = 256;

impl<'a> SourceView<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        let mut lines_at_checkpoint = Vec::with_capacity(bytes.len() / LINE_CHECKPOINT_BYTES + 1);
        let mut line = 1;
        for offset in (0..=bytes.len()).step_by(LINE_CHECKPOINT_BYTES) {
            lines_at_checkpoint.push(line);
            let end = (offset + LINE_CHECKPOINT_BYTES).min(bytes.len());
            line += count_line_breaks(bytes, offset, end);
        }
        Self {
            bytes,
            utf8: std::str::from_utf8(bytes).is_ok(),
            lines_at_checkpoint,
        }
    }

    pub(crate) fn line(&self, byte: usize) -> u64 {
        let checkpoint = byte / LINE_CHECKPOINT_BYTES;
        self.lines_at_checkpoint[checkpoint]
            + count_line_breaks(self.bytes, checkpoint * LINE_CHECKPOINT_BYTES, byte)
    }

    fn valid_origin(&self, origin: &Origin, payload: &Payload) -> bool {
        let start = origin.start_byte;
        let end = origin.end_byte;
        start < end
            && end <= self.bytes.len()
            && origin.raw_sha256 == sha256(&self.bytes[start..end])
            && payload.line == self.line(start)
            && if self.utf8 {
                payload.encoding == "utf8" && self.byte_boundary(start) && self.byte_boundary(end)
            } else {
                payload.encoding == "invalid_utf8"
                    && matches!(
                        payload.syntax_kind.as_str(),
                        "source_file" | "build_metadata"
                    )
                    && start == 0
                    && end == self.bytes.len()
            }
    }

    pub(crate) fn byte_boundary(&self, offset: usize) -> bool {
        self.bytes
            .get(offset)
            .is_none_or(|byte| byte & 0xc0 != 0x80)
    }
}

fn count_line_breaks(bytes: &[u8], start: usize, end: usize) -> u64 {
    bytes[start..end]
        .iter()
        .enumerate()
        .filter(|&(index, &byte)| {
            byte == b'\n' || (byte == b'\r' && bytes.get(start + index + 1) != Some(&b'\n'))
        })
        .count() as u64
}

pub(crate) fn is_invocation(kind: &str) -> bool {
    matches!(
        kind,
        "method_invocation"
            | "object_creation_expression"
            | "explicit_constructor_invocation"
            | "method_reference"
    )
}

pub(crate) fn reference_syntax(kind: &str) -> Option<&'static str> {
    match kind {
        "type_identifier" | "scoped_type_identifier" | "annotation" | "marker_annotation" => {
            Some("type_reference")
        }
        "method_invocation" => Some("method_invocation"),
        "object_creation_expression" | "explicit_constructor_invocation" => {
            Some("constructor_invocation")
        }
        "method_reference" => Some("method_reference"),
        "field_access" => Some("member_reference"),
        "array_access" => Some("array_element_reference"),
        "assignment_expression" | "update_expression" => Some("mutation_target_reference"),
        "throw_statement" => Some("thrown_value_reference"),
        "import_declaration" => Some("import_reference"),
        "requires_module_directive" | "uses_module_directive" | "provides_module_directive" => {
            Some("module_reference")
        }
        _ => None,
    }
}

pub(crate) fn fair_kind(syntax_kind: &str) -> ElementKind {
    match syntax_kind {
        "import_declaration" => ElementKind::ModuleImport,
        "method_declaration" | "constructor_declaration" | "compact_constructor_declaration" => {
            ElementKind::LogicUnit
        }
        _ => ElementKind::Unknown,
    }
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn inventory(sources: &Sources) -> Vec<SourceDigest> {
    sources
        .iter()
        .map(|(path, bytes)| SourceDigest {
            path: path.clone(),
            sha256: sha256(bytes),
        })
        .collect()
}

fn canonical<T: Serialize>(data: &T) -> Vec<u8> {
    let mut value = serde_json::to_value(data).expect("Cortados report serializes");
    value.sort_all_objects();
    let mut bytes = serde_json::to_vec_pretty(&value).expect("Cortados JSON serializes");
    bytes.push(b'\n');
    bytes
}

fn decode<T: DeserializeOwned>(
    artifacts: &BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<T, Vec<Diagnostic>> {
    let bytes = artifacts
        .get(path)
        .ok_or_else(|| admission_error(path, "artifact is missing"))?;
    serde_json::from_slice(bytes)
        .map_err(|error| admission_error(path, format!("invalid report schema: {error}")))
}

fn read<T: DeserializeOwned + Serialize>(
    artifacts: &BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<T, Vec<Diagnostic>> {
    let value = decode(artifacts, path)?;
    require(
        canonical(&value) == artifacts[path],
        path,
        "report is not canonical JSON",
    )?;
    Ok(value)
}

fn artifact_inventory(
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<Artifact>, Vec<Diagnostic>> {
    ARTIFACTS
        .iter()
        .map(|&(path, kind, contract)| {
            let bytes = artifacts
                .get(path)
                .ok_or_else(|| admission_error(path, "artifact is missing"))?;
            Ok(Artifact {
                path: path.into(),
                kind: kind.into(),
                contract: contract.into(),
                sha256: sha256(bytes),
            })
        })
        .collect()
}

fn producer() -> Producer {
    Producer {
        name: PRODUCER.into(),
        version: VERSION.into(),
    }
}

fn passes() -> BTreeMap<String, String> {
    [
        ("ingestion", VERSION),
        ("java_parse", VERSION),
        ("tree_sitter", "0.25.10"),
        ("tree_sitter_java", "0.23.5"),
        ("semantic_observation", VERSION),
        ("dependency_inventory", VERSION),
        ("bundle_admission", VERSION),
    ]
    .into_iter()
    .map(|(name, version)| (name.into(), version.into()))
    .collect()
}

fn sort_findings(findings: &mut Vec<Finding>) {
    findings.sort_by(|a, b| {
        let key = |span: Option<Span>| span.map(|span| (span.start, span.end));
        (
            &a.path,
            key(a.span),
            &a.code,
            &a.subject_ids,
            &a.message,
            &a.attempted_pass,
            &a.requirement,
            &a.severity,
            &a.remediation,
        )
            .cmp(&(
                &b.path,
                key(b.span),
                &b.code,
                &b.subject_ids,
                &b.message,
                &b.attempted_pass,
                &b.requirement,
                &b.severity,
                &b.remediation,
            ))
    });
    findings.dedup();
}

fn safe_path(path: &str) -> bool {
    !path.contains(['\\', '\0', ':'])
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
}

fn strictly_sorted<T: Ord>(items: impl Iterator<Item = T>) -> bool {
    let mut last = None;
    for item in items {
        if last.as_ref().is_some_and(|prior| prior >= &item) {
            return false;
        }
        last = Some(item);
    }
    true
}

fn require(test: bool, path: &str, message: impl Into<String>) -> Result<(), Vec<Diagnostic>> {
    if test {
        Ok(())
    } else {
        Err(admission_error(path, message))
    }
}

fn admission_error(path: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::new("COR-E700", path, None, message)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis::analyse_material, ingest, Limits};
    use serde_json::{json, Value};

    const SOURCE: &[u8] = b"class Base {} class A extends Base { void call() { target(); } }";

    fn fixture() -> (BTreeMap<String, Vec<u8>>, Sources) {
        let source = ingest::SourceFile {
            path: "A.java".into(),
            bytes: SOURCE.to_vec(),
        };
        let sources = BTreeMap::from([(source.path.clone(), source.bytes.clone())]);
        let material = ingest::Material {
            files: vec![source],
            excluded: Vec::new(),
            diagnostics: Vec::new(),
        };
        let bundle = analyse_material(material, &Limits::default()).expect("test fixture");
        (bundle.artifacts(), sources)
    }

    fn change_report(
        artifacts: &mut BTreeMap<String, Vec<u8>>,
        path: &str,
        edit: impl FnOnce(&mut Value),
    ) {
        let mut value: Value = serde_json::from_slice(&artifacts[path]).unwrap();
        edit(&mut value);
        artifacts.insert(path.into(), canonical(&value));
        let mut manifest: Manifest = decode(artifacts, MANIFEST_PATH).unwrap();
        manifest.artifacts = artifact_inventory(artifacts).unwrap();
        artifacts.insert(MANIFEST_PATH.into(), canonical(&manifest));
    }

    #[test]
    fn rehashed_corrupt_provenance_and_dangling_graph_are_rejected() {
        let (original, sources) = fixture();
        validate_artifacts(&original, &sources).unwrap();
        let mut bad_span = original.clone();
        change_report(&mut bad_span, CSIR_PATH, |csir| {
            csir["nodes"][0]["origin"]["raw_sha256"] = json!("0".repeat(64));
        });
        assert!(validate_artifacts(&bad_span, &sources).is_err());

        let mut bad_graph = original;
        change_report(&mut bad_graph, CSIR_PATH, |csir| {
            csir["edges"][0]["object"] = json!("absent.node");
        });
        assert!(validate_artifacts(&bad_graph, &sources).is_err());
    }

    #[test]
    fn rehashed_omission_of_unresolved_diagnostics_or_capabilities_is_rejected() {
        let (original, sources) = fixture();
        let mut without_diagnostics = original.clone();
        change_report(&mut without_diagnostics, DIAGNOSTICS_PATH, |report| {
            report["diagnostics"] = json!([]);
        });
        assert!(validate_artifacts(&without_diagnostics, &sources).is_err());

        let mut without_capabilities = original;
        change_report(&mut without_capabilities, CAPABILITIES_PATH, |report| {
            report["requirements"] = json!([]);
        });
        assert!(validate_artifacts(&without_capabilities, &sources).is_err());
    }

    #[test]
    fn proof_cycles_are_detected_iteratively() {
        let first = Node {
            id: "node.a".into(),
            domain: Domain::Type,
            kind: Kind::Type,
            level: "implementation".into(),
            origin: Origin::new("A.java", b"a", Span { start: 0, end: 1 }),
            fair_element_id: None,
            payload: Payload {
                syntax_kind: "type_identifier".into(),
                name: Some("A".into()),
                fields: BTreeMap::new(),
                line: 1,
                observations: Vec::new(),
                resolution: None,
                encoding: "utf8".into(),
            },
            unresolved: Vec::new(),
            evidence: vec!["node.b".into()],
        };
        let mut second = first.clone();
        second.id = "node.b".into();
        second.evidence.clear();
        assert!(acyclic(&BTreeMap::from([
            ("node.a", &first),
            ("node.b", &second)
        ])));
        second.evidence.push("node.a".into());
        assert!(!acyclic(&BTreeMap::from([
            ("node.a", &first),
            ("node.b", &second)
        ])));
    }

    #[test]
    fn original_line_counts_handle_crlf_crossing_a_checkpoint() {
        let mut bytes = vec![b' '; LINE_CHECKPOINT_BYTES - 1];
        bytes.extend_from_slice(b"\r\na\rb\nc");
        let view = SourceView::new(&bytes);
        let mark = LINE_CHECKPOINT_BYTES;
        assert_eq!(view.line(mark), 1);
        assert_eq!(view.line(mark + 1), 2);
        assert_eq!(view.line(mark + 3), 3);
        assert_eq!(view.line(mark + 5), 4);
    }
}
