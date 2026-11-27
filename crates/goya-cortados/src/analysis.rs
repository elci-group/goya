//! Offline source evidence extraction for the Java 17 profile.

use crate::{
    bundle::{
        self, is_invocation, AnalysisBundle, Capability, Dependency, Domain, Edge, Exclusion,
        Finding, Kind, Node, Origin, Payload, Reports, Resolution, SourceDigest, SourceStatus,
        SourceView,
    },
    ingest,
    java::{self, ParsedFile, SyntaxNode},
    Diagnostic, Limits, Span,
};
use goya_fair::{
    Basis, ElementKind, ElementProvenance, ExtractorIdentity, FairDocument, FairElement,
    FairRelationship, Relation, Representability, RepresentabilityStatus, SemanticsValue,
    SourceFile, SourceManifest, FAIR_CONTRACT,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Analyse original files without executing Java or build logic or fetching symbols.
pub fn analyse(input: &Path, limits: &Limits) -> Result<AnalysisBundle, Vec<Diagnostic>> {
    analyse_material(ingest::ingest(input, limits)?, limits)
}

pub(crate) fn analyse_material(
    material: ingest::Material,
    limits: &Limits,
) -> Result<AnalysisBundle, Vec<Diagnostic>> {
    let files: BTreeMap<_, _> = material
        .files
        .into_iter()
        .map(|file| (file.path, file.bytes))
        .collect();
    let mut reports = Reports {
        excluded: material
            .excluded
            .into_iter()
            .map(|item| Exclusion {
                path: item.path,
                reason: item.reason,
            })
            .collect(),
        diagnostics: material
            .diagnostics
            .into_iter()
            .map(|diagnostic| Finding::from_diagnostic(diagnostic, "ingestion", None))
            .collect(),
        ..Reports::default()
    };
    let java_count = files.keys().filter(|path| path.ends_with(".java")).count();
    if !files
        .iter()
        .any(|(path, bytes)| path.ends_with(".java") && !bytes.is_empty())
    {
        return Err(vec![Diagnostic::new(
            "COR-E100",
            ".",
            None,
            "The supplied material contains no nonempty Java source files",
        )]);
    }
    let inventory = bundle::inventory(&files);
    let mut fair = FairDocument {
        contract: FAIR_CONTRACT.into(),
        source: SourceManifest {
            root: "application".into(),
            files: inventory.iter().map(|file| SourceFile {
                path: file.path.clone(),
                sha256: file.sha256.clone(),
            }).collect(),
        },
        extractor: ExtractorIdentity {
            name: "goya-cortados".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            id_rule: "n.<sorted file ordinal>.<preorder syntax ordinal>; each ordinal is eight lowercase hexadecimal digits".into(),
        },
        project: Default::default(),
        elements: Vec::new(),
        relationships: Vec::new(),
    };
    for (index, (file, bytes)) in inventory.iter().zip(files.values()).enumerate() {
        if is_notice(&file.path) {
            reports.original_notices.push(file.clone());
        }
        let java_source = file.path.ends_with(".java");
        if !(java_source || is_build_metadata(&file.path)) {
            continue;
        }
        let mut observation = Observation {
            file,
            file_index: index,
            source: SourceView::new(bytes),
            reports: &mut reports,
            fair: &mut fair,
        };
        if java_source {
            observation.java(java::parse(&file.path, bytes, limits), java_count == 1);
        } else {
            observation.build_metadata();
        }
    }
    AnalysisBundle::new(files, fair, reports)
}

struct Observation<'a> {
    file: &'a SourceDigest,
    file_index: usize,
    source: SourceView<'a>,
    reports: &'a mut Reports,
    fair: &'a mut FairDocument,
}

impl Observation<'_> {
    fn java(&mut self, parsed: ParsedFile, single_unit: bool) {
        self.reports.source_syntax.push(SourceStatus {
            path: self.file.path.clone(),
            syntax_complete: parsed.syntax_complete,
        });
        let failure_code = parsed
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.code.clone());
        self.reports.diagnostics.extend(
            parsed
                .diagnostics
                .into_iter()
                .map(|diagnostic| Finding::from_diagnostic(diagnostic, "java_parse", None)),
        );
        if self.source.bytes.is_empty() {
            return;
        }

        let file_span = self.whole_file();
        let root_id = self.id(0);
        let mut unit = self.node(
            root_id.clone(),
            Domain::Provenance,
            Kind::Unit,
            "source_file",
            file_span,
        );
        if let Some(code) = failure_code {
            unit.kind = Kind::Unresolved;
            self.finding(
                &mut unit, &code, "java_parse",
                "Java syntax is incomplete. Original bytes in this compilation unit remain source evidence.",
            );
        }
        self.emit(unit, ElementKind::Unknown, None);

        let syntax = parsed.nodes;
        let scope = HeaderScope::new(&syntax, single_unit && parsed.syntax_complete);
        let mut emitted: Vec<(String, Span)> = Vec::with_capacity(syntax.len());
        for (index, syntax_node) in syntax.iter().enumerate() {
            let (parent, parent_span) = syntax_node
                .parent
                .and_then(|parent| emitted.get(parent))
                .map_or((root_id.as_str(), file_span), |(id, span)| {
                    (id.as_str(), *span)
                });
            let span = self.schema_span(syntax_node.span, parent_span);
            let (domain, kind, labels) = classification(syntax_node, &syntax);
            let mut node = self.node(self.id(index + 1), domain, kind, &syntax_node.kind, span);
            node.payload.name.clone_from(&syntax_node.name);
            node.payload.fields.clone_from(&syntax_node.fields);
            node.payload
                .observations
                .extend(labels.iter().map(|label| (*label).into()));
            if span != syntax_node.span {
                node.payload.fields.insert(
                    "parser_start_byte".into(),
                    syntax_node.span.start.to_string(),
                );
                node.payload
                    .fields
                    .insert("parser_end_byte".into(), syntax_node.span.end.to_string());
                node.payload
                    .observations
                    .push("anchor_for_zero_width_syntax".into());
            }
            if node.kind == Kind::Unresolved {
                self.finding(
                    &mut node,
                    "COR-E100",
                    "java_parse",
                    "Parser recovery preserved this syntax as opaque evidence.",
                );
            }
            if let Some(reference) = reference_kind(syntax_node, &syntax) {
                node.payload.observations.push(reference.into());
                if let Some(target) = scope.bind(syntax_node, &syntax) {
                    let target_node_id = self.id(target + 1);
                    node.evidence.push(target_node_id.clone());
                    node.payload.resolution = Some(Resolution::SuppliedDeclaration {
                        target_node_id,
                        basis: "unique_top_level_same_unit_header".into(),
                    });
                } else {
                    node.payload.resolution = Some(Resolution::Unknown);
                    self.finding(
                        &mut node, "COR-E110", "semantic_observation",
                        "Source syntax is present but Java name, overload or dispatch attribution is unresolved.",
                    );
                }
            }
            let probes = probes(syntax_node);
            for probe in probes {
                node.payload.observations.push(probe.label.into());
                self.finding(&mut node, probe.code, "semantic_observation", probe.message);
                self.capability(&node, probe.label);
                if matches!(probe.code, "COR-E210" | "COR-E220") {
                    self.dependency(
                        Some(&node),
                        syntax_node.name.as_deref().unwrap_or(probe.label),
                        probe.label,
                    );
                }
            }
            if is_invocation(&syntax_node.kind) {
                self.capability(&node, "invocation_effects");
            }
            if matches!(
                syntax_node.kind.as_str(),
                "import_declaration"
                    | "requires_module_directive"
                    | "uses_module_directive"
                    | "provides_module_directive"
            ) {
                let raw_name = syntax_node
                    .name
                    .as_deref()
                    .or_else(|| syntax_node.fields.get("module").map(String::as_str))
                    .or_else(|| syntax_node.fields.get("provided").map(String::as_str))
                    .or_else(|| syntax_node.fields.get("type").map(String::as_str))
                    .unwrap_or("unresolved declaration");
                let kind = if syntax_node.kind == "import_declaration" {
                    if jdk_namespace(raw_name) {
                        "jdk_namespace_import"
                    } else {
                        "import"
                    }
                } else {
                    "module_dependency"
                };
                let identity = if matches!(
                    syntax_node.fields.get("wildcard").map(String::as_str),
                    Some("true")
                ) {
                    format!("{raw_name}.*")
                } else {
                    raw_name.to_owned()
                };
                self.finding(
                    &mut node,
                    "COR-E120",
                    "dependency_inventory",
                    "Dependency origin, version, license and reachability are unresolved.",
                );
                self.dependency(Some(&node), &identity, kind);
            } else if is_type_reference(&syntax_node.kind) {
                if let Some(name) = syntax_node
                    .name
                    .as_deref()
                    .filter(|name| jdk_namespace(name))
                {
                    self.dependency(Some(&node), name, "jdk_namespace_type_reference");
                }
            }
            self.emit(node, bundle::fair_kind(&syntax_node.kind), Some(parent));
            emitted.push((self.id(index + 1), span));
        }
    }

    fn build_metadata(&mut self) {
        let span = self.whole_file();
        let message = "Build metadata is inventoried as supplied. Computed dependencies, generated sources and build configuration remain unresolved.";
        if span.start == span.end {
            self.reports.diagnostics.push(Finding::from_diagnostic(
                Diagnostic::new("COR-E120", &self.file.path, None, message),
                "dependency_inventory",
                None,
            ));
            self.dependency(None, &self.file.path, "build_metadata");
            return;
        }
        let mut node = self.node(
            self.id(0),
            Domain::Dependency,
            Kind::Dependency,
            "build_metadata",
            span,
        );
        node.payload.name = Some(self.file.path.clone());
        self.finding(&mut node, "COR-E120", "dependency_inventory", message);
        self.dependency(Some(&node), &self.file.path, "build_metadata");
        self.emit(node, ElementKind::Unknown, None);
    }

    fn whole_file(&self) -> Span {
        Span {
            start: 0,
            end: self.source.bytes.len(),
        }
    }

    fn id(&self, ordinal: usize) -> String {
        format!("n.{:08x}.{ordinal:08x}", self.file_index)
    }

    fn schema_span(&self, span: Span, parent: Span) -> Span {
        if span.start < span.end {
            return span;
        }
        // CSIR origins are nonempty. Record a containing original code point as
        // evidence for any insertion point, and retain the exact parser offsets.
        if span.start >= parent.end {
            let mut start = parent.end - 1;
            while !self.source.byte_boundary(start) {
                start -= 1;
            }
            Span {
                start,
                end: parent.end,
            }
        } else {
            let start = span.start.max(parent.start);
            let mut end = start + 1;
            while !self.source.byte_boundary(end) {
                end += 1;
            }
            Span { start, end }
        }
    }

    fn node(&self, id: String, domain: Domain, kind: Kind, syntax_kind: &str, span: Span) -> Node {
        Node {
            id,
            domain,
            kind,
            level: "implementation".into(),
            origin: Origin::new(&self.file.path, self.source.bytes, span),
            fair_element_id: None,
            payload: Payload {
                syntax_kind: syntax_kind.into(),
                name: None,
                fields: BTreeMap::new(),
                line: self.source.line(span.start),
                observations: Vec::new(),
                resolution: None,
                encoding: if self.source.utf8 {
                    "utf8"
                } else {
                    "invalid_utf8"
                }
                .into(),
            },
            unresolved: Vec::new(),
            evidence: Vec::new(),
        }
    }

    fn finding(&mut self, node: &mut Node, code: &str, pass: &str, message: &str) {
        node.unresolved.push(format!("{code}: {message}"));
        let span = Span {
            start: node.origin.start_byte,
            end: node.origin.end_byte,
        };
        self.reports.diagnostics.push(Finding::from_diagnostic(
            Diagnostic::new(code, &self.file.path, Some(span), message),
            pass,
            Some(&node.id),
        ));
    }

    fn capability(&mut self, node: &Node, operation: &str) {
        self.reports.capabilities.push(Capability {
            id: format!("cap.{}.{operation}", node.id),
            operation: operation.into(),
            scope: "source_observation".into(),
            status: "unresolved".into(),
            origin: node.origin.clone(),
            subject_ids: vec![node.id.clone()],
        });
    }

    fn dependency(&mut self, node: Option<&Node>, identity: &str, observed_as: &str) {
        self.reports.dependencies.push(Dependency {
            id: format!("dep.{:08x}", self.reports.dependencies.len()),
            identity: identity.into(),
            observed_as: observed_as.into(),
            version: None,
            license: None,
            attribution: Vec::new(),
            source_sha256: self.file.sha256.clone(),
            artifact_sha256: None,
            origin: node.map(|node| node.origin.clone()),
            subject_ids: node.into_iter().map(|node| node.id.clone()).collect(),
            reachable_members: Vec::new(),
            reachability_assumptions: vec!["Static source observation only. Dependencies have not been attributed or fetched.".into()],
            unknown_reachability: true,
            attempted_strategies: Vec::new(),
            selected_strategy: None,
            verification_evidence: Vec::new(),
            residual_requirements: vec!["Resolve the source or artifact, version, license, attribution and reachable members.".into()],
            authority: "source_observation".into(),
        });
    }

    fn emit(&mut self, mut node: Node, fair_kind: ElementKind, parent_id: Option<&str>) {
        node.payload.observations.sort_unstable();
        node.payload.observations.dedup();
        node.unresolved.sort_unstable();
        node.unresolved.dedup();
        let provenance = ElementProvenance {
            file: node.origin.path.clone(),
            line: node.payload.line,
            span_start: Some(node.origin.start_byte as u64),
            span_end: Some(node.origin.end_byte as u64),
            fallback_origin: false,
        };
        let mut semantics = BTreeMap::from([
            (
                "observed".into(),
                SemanticsValue::Str(node.payload.syntax_kind.clone()),
            ),
            (
                "raw_sha256".into(),
                SemanticsValue::Str(node.origin.raw_sha256.clone()),
            ),
            (
                "unresolved".into(),
                SemanticsValue::List(
                    node.unresolved
                        .iter()
                        .cloned()
                        .map(SemanticsValue::Str)
                        .collect(),
                ),
            ),
        ]);
        if let Some(name) = &node.payload.name {
            semantics.insert("name".into(), SemanticsValue::Str(name.clone()));
        }
        let known_kind = fair_kind != ElementKind::Unknown;
        self.fair.elements.push(FairElement {
            id: node.id.clone(),
            kind: fair_kind,
            provenance: provenance.clone(),
            representability: if known_kind {
                Representability {
                    status: RepresentabilityStatus::Extracted,
                    reason: None,
                    detail: None,
                }
            } else {
                Representability {
                    status: RepresentabilityStatus::Undetermined,
                    reason: Some("NO_FAIR_KIND".into()),
                    detail: Some("Java syntax preserved in the attached CSIR.".into()),
                }
            },
            semantics,
        });
        node.fair_element_id = Some(node.id.clone());
        if let Some(parent) = parent_id {
            let id = format!("e.{}", node.id);
            self.fair.relationships.push(FairRelationship {
                id: id.clone(),
                subject: parent.into(),
                relation: Relation::Contains,
                object: node.id.clone(),
                basis: Basis::Extracted,
                evidence: vec![node.id.clone()],
                provenance,
            });
            self.reports.edges.push(Edge {
                id,
                subject: parent.into(),
                relation: "contains".into(),
                object: node.id.clone(),
                evidence: vec![node.id.clone()],
            });
        }
        self.reports.nodes.push(node);
    }
}

// Only top-level types in this compilation unit are candidates. Imports,
// type parameters, and nested declarations make this deliberately small
// header-only proof unavailable. Bodies need full Java attribution.
struct HeaderScope<'a> {
    declarations: BTreeMap<&'a str, Option<(usize, &'a str)>>,
    blocked_headers: BTreeSet<usize>,
}

impl<'a> HeaderScope<'a> {
    fn new(syntax: &'a [SyntaxNode], complete: bool) -> Self {
        let mut scope = Self {
            declarations: BTreeMap::new(),
            blocked_headers: BTreeSet::new(),
        };
        if !complete || syntax.iter().any(|node| node.kind == "import_declaration") {
            return scope;
        }
        for (index, node) in syntax.iter().enumerate() {
            if is_type_declaration(&node.kind) && has_parent(node, syntax, "program") {
                if let Some(name) = node.name.as_deref() {
                    scope
                        .declarations
                        .entry(name)
                        .and_modify(|entry| *entry = None)
                        .or_insert(Some((index, node.kind.as_str())));
                }
            }
            if node.kind == "type_parameters" {
                if let Some(parent) = node.parent {
                    scope.blocked_headers.insert(parent);
                }
            }
            if is_type_declaration(&node.kind) && !has_parent(node, syntax, "program") {
                let mut ancestor = node.parent;
                while let Some(parent) = ancestor {
                    let owner = &syntax[parent];
                    if is_type_declaration(&owner.kind) && has_parent(owner, syntax, "program") {
                        scope.blocked_headers.insert(parent);
                        break;
                    }
                    ancestor = owner.parent;
                }
            }
        }
        scope
    }

    fn bind(&self, reference: &SyntaxNode, syntax: &[SyntaxNode]) -> Option<usize> {
        if reference.kind != "type_identifier" {
            return None;
        }
        let mut header = reference.parent?;
        if syntax[header].kind == "type_list" {
            header = syntax[header].parent?;
        }
        let expected = match syntax[header].kind.as_str() {
            "superclass" => "class_declaration",
            "super_interfaces" | "extends_interfaces" => "interface_declaration",
            _ => return None,
        };
        let owner = syntax[header].parent?;
        let (target, kind) = self
            .declarations
            .get(reference.name.as_deref()?)?
            .as_ref()?;
        (*kind == expected
            && owner != *target
            && has_parent(&syntax[owner], syntax, "program")
            && !self.blocked_headers.contains(&owner))
        .then_some(*target)
    }
}

fn has_parent(node: &SyntaxNode, syntax: &[SyntaxNode], kind: &str) -> bool {
    node.parent.is_some_and(|index| syntax[index].kind == kind)
}

fn is_type_declaration(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
    )
}

fn is_type_reference(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier" | "scoped_type_identifier" | "annotation" | "marker_annotation"
    )
}

fn reference_kind(node: &SyntaxNode, syntax: &[SyntaxNode]) -> Option<&'static str> {
    if node.kind == "type_identifier" && has_parent(node, syntax, "type_parameter") {
        return None;
    }
    bundle::reference_syntax(&node.kind)
}

fn classification(
    node: &SyntaxNode,
    syntax: &[SyntaxNode],
) -> (Domain, Kind, &'static [&'static str]) {
    use Domain as D;
    use Kind as K;
    if node.kind == "ERROR" || flag(node, "missing") {
        return (D::Provenance, K::Unresolved, &["parser_recovery"]);
    }
    match node.kind.as_str() {
        "class_declaration" | "enum_declaration" | "record_declaration" => {
            (D::Type, K::Type, &["type_declaration"])
        }
        "interface_declaration" | "annotation_type_declaration" => {
            (D::Interface, K::Type, &["type_declaration"])
        }
        "superclass" => (D::Type, K::Type, &["extends_clause"]),
        "super_interfaces" => (D::Interface, K::Type, &["implements_clause"]),
        "extends_interfaces" => (D::Interface, K::Type, &["extends_interface_clause"]),
        "permits" => (D::Type, K::Type, &["permits_clause"]),
        "generic_type" | "type_parameters" | "type_parameter" | "type_arguments" | "wildcard"
        | "type_bound" => (D::Type, K::Type, &["generic_type_syntax"]),
        "type_identifier"
        | "scoped_type_identifier"
        | "integral_type"
        | "floating_point_type"
        | "boolean_type"
        | "void_type"
        | "array_type"
        | "intersection_type"
        | "union_type"
        | "annotation"
        | "marker_annotation"
        | "annotated_type" => (D::Type, K::Type, &[]),
        "constructor_declaration" | "compact_constructor_declaration" => {
            (D::Lifecycle, K::Procedure, &["constructor_declaration"])
        }
        "static_initializer" => (
            D::Lifecycle,
            K::Lifecycle,
            &["static_initialization_syntax"],
        ),
        "method_declaration" => (D::Control, K::Procedure, &["method_declaration"]),
        "lambda_expression" => (D::Control, K::Procedure, &["lambda_syntax"]),
        "explicit_constructor_invocation" => (D::Lifecycle, K::Operation, &[]),
        "object_creation_expression" | "array_creation_expression" => {
            (D::Identity, K::Allocation, &["allocation_syntax"])
        }
        "assignment_expression" | "update_expression" => {
            (D::Memory, K::Operation, &["mutation_syntax"])
        }
        "field_access" | "array_access" => (D::Memory, K::Operation, &["storage_access_syntax"]),
        "field_declaration"
        | "local_variable_declaration"
        | "variable_declarator"
        | "constant_declaration"
        | "formal_parameter"
        | "spread_parameter" => (D::Memory, K::Object, &["storage_declaration"]),
        "synchronized_statement" => (D::Concurrency, K::Concurrency, &["monitor_syntax"]),
        "modifiers" if flag(node, "native") => (D::Effects, K::Effect, &["native_modifier"]),
        "modifiers" if flag(node, "volatile") || flag(node, "synchronized") => {
            (D::Concurrency, K::Concurrency, &[])
        }
        "throw_statement" | "throws" | "catch_clause" | "try_statement" => {
            (D::Control, K::Operation, &["exception_syntax"])
        }
        "try_with_resources_statement" | "finally_clause" | "resource" => {
            (D::Lifecycle, K::Lifecycle, &["cleanup_syntax"])
        }
        "method_invocation"
        | "method_reference"
        | "return_statement"
        | "if_statement"
        | "for_statement"
        | "enhanced_for_statement"
        | "while_statement"
        | "do_statement"
        | "switch_expression"
        | "switch_statement"
        | "break_statement"
        | "continue_statement"
        | "yield_statement"
        | "conditional_expression"
        | "binary_expression"
        | "unary_expression"
        | "ternary_expression" => (D::Control, K::Operation, &[]),
        "import_declaration"
        | "module_declaration"
        | "requires_module_directive"
        | "exports_module_directive"
        | "opens_module_directive"
        | "uses_module_directive"
        | "provides_module_directive" => (D::Dependency, K::Dependency, &[]),
        "block"
            if has_parent(node, syntax, "class_body")
                || has_parent(node, syntax, "enum_body_declarations") =>
        {
            (D::Lifecycle, K::Lifecycle, &["instance_initializer_syntax"])
        }
        "block" => (D::Control, K::Operation, &[]),
        _ => (D::Provenance, K::Unit, &[]),
    }
}

struct Probe {
    label: &'static str,
    code: &'static str,
    message: &'static str,
}

fn probes(node: &SyntaxNode) -> Vec<Probe> {
    let mut probes = Vec::new();
    let name = node
        .name
        .as_deref()
        .or_else(|| node.fields.get("type").map(String::as_str))
        .unwrap_or_default();
    let receiver = node.fields.get("object").map_or("", String::as_str);
    let receiver_tail = receiver.rsplit('.').next().unwrap_or_default();
    let invocation = is_invocation(&node.kind);
    if flag(node, "native")
        || (invocation && matches!(name, "loadLibrary" | "registerNatives"))
        || (invocation
            && name == "load"
            && (matches!(receiver_tail, "System" | "Runtime")
                || receiver.starts_with("Runtime.getRuntime(")
                || receiver.starts_with("java.lang.Runtime.getRuntime(")))
    {
        probes.push(Probe {
            label: "native_interop",
            code: "COR-E220",
            message: "Native interoperation syntax or an API candidate is present. The linkage, ABI and effects are unresolved.",
        });
    }
    let dynamic_owner = matches!(
        receiver_tail,
        "ClassLoader" | "MethodHandle" | "MethodHandles" | "Lookup" | "ServiceLoader" | "Proxy"
    ) || receiver.starts_with("java.lang.reflect.")
        || receiver.starts_with("java.lang.invoke.");
    let dynamic_type = (is_type_reference(&node.kind) || node.kind == "import_declaration")
        && (matches!(
            name.rsplit('.').next().unwrap_or_default(),
            "ClassLoader" | "MethodHandle" | "MethodHandles" | "ServiceLoader" | "Proxy"
        ) || name.starts_with("java.lang.reflect.")
            || name.starts_with("java.lang.invoke.")
            || matches!(name, "java.lang.reflect" | "java.lang.invoke"));
    let dynamic_member = matches!(
        name,
        "forName"
            | "loadClass"
            | "defineClass"
            | "findClass"
            | "defineHiddenClass"
            | "newProxyInstance"
            | "getProxyClass"
            | "getMethod"
            | "getMethods"
            | "getDeclaredMethod"
            | "getDeclaredMethods"
            | "getField"
            | "getFields"
            | "getDeclaredField"
            | "getDeclaredFields"
            | "getConstructor"
            | "getConstructors"
            | "getDeclaredConstructor"
            | "getDeclaredConstructors"
            | "newInstance"
            | "invoke"
            | "invokeExact"
            | "invokeWithArguments"
            | "setAccessible"
            | "trySetAccessible"
            | "findVirtual"
            | "findStatic"
            | "findSpecial"
            | "findConstructor"
            | "unreflect"
            | "unreflectSpecial"
            | "unreflectConstructor"
            | "bind"
    );
    if dynamic_type || (invocation && (dynamic_owner || dynamic_member)) {
        probes.push(Probe {
            label: "dynamic_resolution",
            code: "COR-E210",
            message: "The spelling is a candidate for dynamic resolution. Receiver, runtime target and reachability are unknown.",
        });
    }
    if flag(node, "volatile")
        || flag(node, "synchronized")
        || node.kind == "synchronized_statement"
        || (invocation
            && (matches!(name, "wait" | "notify" | "notifyAll" | "interrupt" | "join")
                || matches!(receiver_tail, "Thread" | "Executors")))
        || ((is_type_reference(&node.kind) || invocation)
            && (matches!(
                name,
                "Thread"
                    | "ThreadLocal"
                    | "InheritableThreadLocal"
                    | "Executor"
                    | "ExecutorService"
                    | "Future"
                    | "CompletableFuture"
                    | "AtomicInteger"
                    | "AtomicReference"
            ) || name.starts_with("java.util.concurrent.")
                || receiver.starts_with("java.util.concurrent.")))
    {
        probes.push(Probe {
            label: "concurrency",
            code: "COR-E410",
            message: "Concurrency syntax or an API candidate is present. Scheduling, visibility and happens-before obligations are unresolved.",
        });
    }
    for (prefix, label, code, message) in [
        (
            "java.io.",
            "file_or_stream_effects",
            "COR-E200",
            "File or stream API spelling is present. Effect scope and authority are unresolved.",
        ),
        (
            "java.nio.file.",
            "filesystem_effects",
            "COR-E200",
            "Filesystem API spelling is present. Effect scope and authority are unresolved.",
        ),
        (
            "java.net.",
            "network_effects",
            "COR-E200",
            "Network API spelling is present. Effect scope and authority are unresolved.",
        ),
        (
            "java.sql.",
            "transaction_effects",
            "COR-E200",
            "Transaction API spelling is present. Effect scope and authority are unresolved.",
        ),
        (
            "javax.swing.",
            "desktop_interface",
            "COR-E300",
            "Swing API spelling is present. UI behavior and event ownership are unresolved.",
        ),
        (
            "java.awt.",
            "desktop_graphics",
            "COR-E320",
            "AWT API spelling is present. Graphics behavior and platform effects are unresolved.",
        ),
        (
            "javafx.",
            "desktop_interface",
            "COR-E300",
            "JavaFX API spelling is present. UI behavior and event ownership are unresolved.",
        ),
    ] {
        if (is_type_reference(&node.kind) || node.kind == "import_declaration" || invocation)
            && (name.starts_with(prefix)
                || receiver.starts_with(prefix)
                || (flag(node, "wildcard") && Some(name) == prefix.strip_suffix('.')))
        {
            probes.push(Probe {
                label,
                code,
                message,
            });
        }
    }
    probes
}

fn flag(node: &SyntaxNode, name: &str) -> bool {
    node.fields.get(name).is_some_and(|value| value == "true")
}

fn is_build_metadata(path: &str) -> bool {
    matches!(
        basename(path),
        "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "gradle.properties"
            | "libs.versions.toml"
            | "gradle.lockfile"
            | "MANIFEST.MF"
    )
}

fn is_notice(path: &str) -> bool {
    let head = basename(path).split('.').next().unwrap_or_default();
    matches!(
        head.to_ascii_uppercase().as_str(),
        "LICENSE" | "LICENCE" | "NOTICE" | "COPYING" | "COPYRIGHT"
    )
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn jdk_namespace(name: &str) -> bool {
    name.starts_with("java.") || name.starts_with("javax.") || name.starts_with("jdk.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn observe(files: &[(&str, &[u8])]) -> BTreeMap<String, Value> {
        let material = ingest::Material {
            files: files
                .iter()
                .map(|&(path, bytes)| ingest::SourceFile {
                    path: path.into(),
                    bytes: bytes.to_vec(),
                })
                .collect(),
            excluded: Vec::new(),
            diagnostics: Vec::new(),
        };
        analyse_material(material, &Limits::default())
            .expect("analysis bundle")
            .artifacts()
            .into_iter()
            .map(|(path, bytes)| (path, serde_json::from_slice(&bytes).unwrap()))
            .collect()
    }

    fn nodes(reports: &BTreeMap<String, Value>) -> &[Value] {
        reports[bundle::CSIR_PATH]["nodes"].as_array().unwrap()
    }

    #[test]
    fn only_unambiguous_single_unit_headers_bind() {
        let single = observe(&[("Sample.java", b"interface Job {} class Base {} class Sample extends Base implements Job { Base body; }")]);
        let resolutions = |report: &BTreeMap<String, Value>| {
            nodes(report)
                .iter()
                .filter(|node| node["payload"]["resolution"]["status"] == "supplied_declaration")
                .count()
        };
        assert_eq!(resolutions(&single), 2);
        assert!(nodes(&single).iter().any(|node| {
            node["payload"]["name"] == "Base"
                && node["payload"]["resolution"]["status"] == "unknown"
        }));

        for raw in [
            b"import elsewhere.Base; class Base {} class Sample extends Base {}".as_slice(),
            b"class Base {} class Sample<Base> extends Base {}",
            b"class Base {} class Sample extends Base { class Base {} }",
            b"class Base {} class Base {} class Sample extends Base {}",
        ] {
            assert_eq!(resolutions(&observe(&[("Sample.java", raw)])), 0);
        }
        let duplicate = observe(&[
            (
                "Sample.java",
                b"package p; class Base {} class Sample extends Base {}",
            ),
            ("Other.java", b"package p; class Base {}"),
        ]);
        assert_eq!(resolutions(&duplicate), 0);
    }

    #[test]
    fn overloads_dynamic_and_native_candidates_have_exact_origin_without_grants() {
        let raw = br#"
class Sample {
    native void directNative();
    Object method() throws Exception {
        java.util.concurrent.Callable<Class<?>> f = () -> Class.forName("thing.S");
        Object ref = Class::forName;
        System.loadLibrary("unknown");
        return this.method();
    }
}
"#;
        let reports = observe(&[("Sample.java", raw)]);
        let findings = reports["diagnostics.json"]["diagnostics"]
            .as_array()
            .unwrap();
        for (code, origin) in [
            ("COR-E220", "native"),
            ("COR-E220", "System.loadLibrary(\"unknown\")"),
            ("COR-E210", "Class.forName(\"thing.S\")"),
            ("COR-E210", "Class::forName"),
            ("COR-E110", "this.method()"),
        ] {
            assert!(
                findings.iter().any(|finding| {
                    finding["code"] == code
                        && finding["span"]["start"]
                            .as_u64()
                            .zip(finding["span"]["end"].as_u64())
                            .is_some_and(|(s, e)| &raw[s as usize..e as usize] == origin.as_bytes())
                }),
                "missing {code} for {origin}"
            );
        }
        assert_eq!(
            reports["capabilities.json"]["grants"],
            serde_json::json!([])
        );
        assert_eq!(reports["dependencies.json"]["jdk_symbols_supplied"], false);
        assert!(reports[bundle::CSIR_PATH]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|edge| edge["relation"] == "contains"));
    }

    #[test]
    fn source_hashes_cover_binary_resources_unicode_and_invalid_utf8() {
        let bytes = "class Café {\r\n  int \\u0078;\r}\n".as_bytes();
        let bad = b"class Broken {}\xff";
        let reports = observe(&[
            ("Café.java", bytes),
            ("Broken.java", bad),
            ("README.bin", b"\x00\xff"),
            ("build.gradle", b""),
        ]);
        assert_eq!(
            reports["bundle.json"]["sources"].as_array().unwrap().len(),
            4
        );
        assert_eq!(
            reports["bundle.json"]["completeness"]["syntax_complete"],
            false
        );
        assert!(nodes(&reports).iter().any(|node| {
            node["origin"]["path"] == "Café.java"
                && node["payload"]["name"] == "x"
                && node["payload"]["line"] == 2
                && node["origin"]["raw_sha256"] == bundle::sha256(b"\\u0078")
        }));
        assert!(nodes(&reports).iter().any(|node| {
            node["origin"]["path"] == "Broken.java"
                && node["kind"] == "unresolved"
                && node["payload"]["encoding"] == "invalid_utf8"
                && node["origin"]["raw_sha256"] == bundle::sha256(bad)
        }));
    }

    #[test]
    fn parser_recovery_keeps_opaque_nodes_and_insertion_offsets() {
        let reports = observe(&[("Broken.java", b"class Broken { int field")]);
        assert_eq!(
            reports["bundle.json"]["completeness"]["syntax_complete"],
            false
        );
        assert!(nodes(&reports).iter().any(|node| {
            node["kind"] == "unresolved" && node["origin"]["raw_sha256"].as_str().is_some()
        }));
        let recovery = observe(&[("Recovery.java", b"class Recovery { int field }")]);
        assert!(nodes(&recovery).iter().any(|node| {
            node["payload"]["fields"]["missing"] == "true"
                && node["payload"]["fields"]["parser_start_byte"]
                    == node["payload"]["fields"]["parser_end_byte"]
        }));
    }
}
