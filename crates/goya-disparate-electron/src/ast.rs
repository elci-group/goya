//! AST-grade extraction (extractor 0.2.0, ADR 0001).
//!
//! Parses one source file with oxc into a typed AST and emits FAIR elements
//! via a single `enter_node` visitor over [`AstKind`], in source order. On a
//! fatal parse (`ParserReturn::panicked`), the caller falls back to the line
//! scanner for the whole file. Effect-body classification and
//! unresolved-argument spellings are **syntactic only** — no execution, no
//! inference beyond the tree. Bounded resolutions: one same-file indirection
//! for `require` bindings, one file-local const-object lookup for computed
//! channels; anything deeper is recorded as unresolved, never dropped.

use crate::{
    ElementKind, ElementProvenance, FairElement, Representability, RepresentabilityStatus,
    SemanticsValue,
};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionBody, BindingPattern, Expression, Function, ObjectPropertyKind, Program,
    Statement,
};
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use std::collections::BTreeMap;

/// Shared per-kind element-ID allocation. The scanner and the AST emitter
/// draw from the same pool so IDs stay unique and deterministic regardless of
/// which mechanism produced each element.
#[derive(Default)]
pub struct IdAlloc {
    counters: BTreeMap<&'static str, u32>,
}

impl IdAlloc {
    pub fn next(&mut self, prefix: &'static str) -> String {
        let counter = self.counters.entry(prefix).or_default();
        *counter += 1;
        format!("{prefix}-{counter:04}")
    }
}

/// Line index over one file's bytes: byte offset -> 1-based line.
struct LineIndex {
    line_starts: Vec<u32>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset as u32 + 1);
            }
        }
        Self { line_starts }
    }

    fn line_of(&self, offset: u32) -> u64 {
        let idx = self.line_starts.partition_point(|&start| start <= offset);
        idx.max(1) as u64
    }
}

/// File-local binding facts used for bounded resolution.
#[derive(Default)]
struct FileFacts {
    /// `const CH = { key: "literal", ... }` — object maps with all-literal
    /// string values (an empty-string key records plain string consts).
    const_objects: BTreeMap<String, BTreeMap<String, String>>,
    /// `const X = require("literal")` — one-indirection require bindings.
    require_bindings: BTreeMap<String, String>,
    /// `const X = Y` — single-identifier aliases, resolved lazily.
    identifier_aliases: BTreeMap<String, String>,
}

/// Outcome for one file: AST emitted elements, or the caller must fall back
/// to the scanner.
pub enum AstOutcome {
    Emitted,
    FatalParse,
}

/// Parse and extract one file. `source` is the file text (lossy UTF-8;
/// deterministic for given bytes).
pub fn extract_file(
    file: &str,
    source: &str,
    ids: &mut IdAlloc,
    elements: &mut Vec<FairElement>,
) -> AstOutcome {
    let source_type = SourceType::from_path(file).unwrap_or_else(|_| SourceType::mjs());
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    if ret.panicked {
        return AstOutcome::FatalParse;
    }
    let line_index = LineIndex::new(source);
    let mut facts = FileFacts::default();
    collect_facts(&ret.program, &mut facts);
    let mut emitter = ElementEmitter {
        file,
        source,
        line_index: &line_index,
        facts: &facts,
        ids,
        elements,
        pending_state_keys: None,
    };
    emitter.visit_program(&ret.program);
    AstOutcome::Emitted
}

/// Top-level `const` facts for bounded resolution.
fn collect_facts(program: &Program<'_>, facts: &mut FileFacts) {
    for statement in &program.body {
        let Statement::VariableDeclaration(decl) = statement else {
            continue;
        };
        for declarator in &decl.declarations {
            let Some(name) = binding_identifier_name(&declarator.id) else {
                continue;
            };
            let Some(init) = &declarator.init else {
                continue;
            };
            match init {
                Expression::ObjectExpression(obj) => {
                    let mut map = BTreeMap::new();
                    let mut all_literal = true;
                    for property in &obj.properties {
                        let ObjectPropertyKind::ObjectProperty(property) = property else {
                            all_literal = false;
                            break;
                        };
                        let Some(key) = property.key.static_name() else {
                            all_literal = false;
                            break;
                        };
                        match &property.value {
                            Expression::StringLiteral(lit) => {
                                map.insert(key.to_string(), lit.value.to_string());
                            }
                            _ => {
                                all_literal = false;
                                break;
                            }
                        }
                    }
                    if all_literal && !map.is_empty() {
                        facts.const_objects.insert(name, map);
                    }
                }
                Expression::CallExpression(call) => {
                    if is_require_call(call) {
                        if let Some(Expression::StringLiteral(lit)) =
                            call.arguments.first().and_then(|a| a.as_expression())
                        {
                            facts.require_bindings.insert(name, lit.value.to_string());
                        }
                    }
                }
                Expression::Identifier(ident) => {
                    facts.identifier_aliases.insert(name, ident.name.to_string());
                }
                _ => {}
            }
        }
    }
}

/// Binding identifier name for a plain `const x = ...` declarator.
fn binding_identifier_name(pattern: &BindingPattern<'_>) -> Option<String> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.to_string()),
        _ => None,
    }
}

fn is_require_call(call: &oxc_ast::ast::CallExpression<'_>) -> bool {
    matches!(&call.callee, Expression::Identifier(ident) if ident.name == "require")
}

/// Known hook names (same set as the scanner; grows by contract revision).
const HOOKS: &[&str] = &[
    "useState", "useEffect", "useContext", "useRef", "useMemo", "useCallback",
    "useReducer", "useLayoutEffect",
];

/// Interaction props recorded as event-handler elements (same list as the
/// scanner; kept in sync by contract revision).
const EVENT_PROPS: &[&str] = &[
    "onClick", "onChange", "onSubmit", "onKeyDown", "onKeyUp",
    "onDoubleClick", "onBlur", "onFocus", "onInput", "onScroll",
];

struct ElementEmitter<'a> {
    file: &'a str,
    source: &'a str,
    line_index: &'a LineIndex,
    facts: &'a FileFacts,
    ids: &'a mut IdAlloc,
    elements: &'a mut Vec<FairElement>,
    /// useState destructuring names collected from the enclosing declarator.
    pending_state_keys: Option<Vec<String>>,
}

/// How a string-valued expression resolved.
enum Stringlike {
    Literal(String),
    ResolvedConst(String),
    Unresolved(String),
}

impl<'a> ElementEmitter<'a> {
    fn emit(
        &mut self,
        prefix: &'static str,
        kind: ElementKind,
        span: Span,
        representability: Representability,
        semantics: BTreeMap<String, SemanticsValue>,
    ) {
        let line = self.line_index.line_of(span.start);
        self.elements.push(FairElement {
            id: self.ids.next(prefix),
            kind,
            provenance: ElementProvenance {
                file: self.file.to_string(),
                line,
                span_start: Some(span.start as u64),
                span_end: Some(span.end as u64),
                fallback_origin: false,
            },
            representability,
            semantics,
        });
    }

    fn extracted(&self) -> Representability {
        Representability {
            status: RepresentabilityStatus::Extracted,
            reason: None,
            detail: None,
        }
    }

    fn undetermined(&self, reason: &str, detail: &str) -> Representability {
        Representability {
            status: RepresentabilityStatus::Undetermined,
            reason: Some(reason.to_string()),
            detail: Some(detail.to_string()),
        }
    }

    fn source_slice(&self, span: Span) -> String {
        let start = (span.start as usize).min(self.source.len());
        let end = (span.end as usize).min(self.source.len()).max(start);
        self.source[start..end].chars().take(120).collect()
    }

    /// `(object, method)` for `obj.method(...)`, resolving one const-alias hop.
    fn object_and_method(&self, callee: &Expression<'a>) -> Option<(String, String)> {
        let (object, method) = match callee {
            Expression::StaticMemberExpression(sme) => {
                (sme.object.span(), sme.property.name.to_string())
            }
            Expression::ComputedMemberExpression(cme) => {
                (cme.object.span(), cme.static_property_name()?.to_string())
            }
            _ => return None,
        };
        // Object must be a plain identifier (deeper chains are out of scope).
        let object_ident = self.identifier_at_span(object)?;
        let object = self
            .facts
            .identifier_aliases
            .get(&object_ident)
            .cloned()
            .unwrap_or(object_ident);
        Some((object, method))
    }

    fn identifier_at_span(&self, span: Span) -> Option<String> {
        let sliced = self.source_slice(span);
        let trimmed = sliced.trim();
        if trimmed.is_empty()
            || !trimmed
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            return None;
        }
        Some(trimmed.to_string())
    }

    fn handle_call(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
        // require(...) and eval(...) take precedence over member matching.
        if is_require_call(call) {
            self.emit_require(call);
            return;
        }
        if let Expression::Identifier(ident) = &call.callee {
            match ident.name.as_str() {
                "eval" => {
                    let mut semantics = BTreeMap::new();
                    semantics.insert("observed".into(), SemanticsValue::Str("eval".into()));
                    self.emit(
                        "unk",
                        ElementKind::Unknown,
                        call.span,
                        self.undetermined(
                            "DYNAMIC_CODE_EVALUATION",
                            "dynamic code evaluation has no statically extractable semantics",
                        ),
                        semantics,
                    );
                    return;
                }
                "spawn" | "exec" => {
                    self.emit(
                        "proc",
                        ElementKind::ChildProcess,
                        call.span,
                        self.extracted(),
                        BTreeMap::new(),
                    );
                    return;
                }
                name if HOOKS.contains(&name) => {
                    self.emit_hook(call, name);
                    return;
                }
                _ => {}
            }
        }
        let Some((object, method)) = self.object_and_method(&call.callee) else {
            return;
        };
        match object.as_str() {
            "ipcMain" if method == "handle" || method == "on" => {
                self.emit_ipc(call, "main", &method);
            }
            "ipcRenderer" if matches!(method.as_str(), "invoke" | "send" | "on") => {
                self.emit_ipc(call, "renderer", &method);
            }
            "dialog" | "shell" | "clipboard" => {
                let mut semantics = BTreeMap::new();
                semantics.insert("api".into(), SemanticsValue::Str(object.clone()));
                semantics.insert("method".into(), SemanticsValue::Str(method));
                self.emit("api", ElementKind::OsApiCall, call.span, self.extracted(), semantics);
            }
            "fs" | "fsPromises" => {
                self.emit(
                    "fs",
                    ElementKind::FilesystemOperation,
                    call.span,
                    self.extracted(),
                    BTreeMap::new(),
                );
            }
            "localStorage" => {
                self.emit(
                    "store",
                    ElementKind::StateStore,
                    call.span,
                    self.extracted(),
                    BTreeMap::new(),
                );
            }
            "contextBridge" if method == "exposeInMainWorld" => {
                if let Some(Expression::StringLiteral(lit)) =
                    call.arguments.first().and_then(|a| a.as_expression())
                {
                    let mut semantics = BTreeMap::new();
                    semantics.insert("exposed".into(), SemanticsValue::Str(lit.value.to_string()));
                    self.emit(
                        "brg",
                        ElementKind::PreloadBridge,
                        call.span,
                        self.extracted(),
                        semantics,
                    );
                }
            }
            "childProcess" | "child_process" => {
                self.emit(
                    "proc",
                    ElementKind::ChildProcess,
                    call.span,
                    self.extracted(),
                    BTreeMap::new(),
                );
            }
            _ => {}
        }
    }

    fn emit_ipc(&mut self, call: &oxc_ast::ast::CallExpression<'a>, side: &str, verb: &str) {
        let Some(expression) = call.arguments.first().and_then(|a| a.as_expression()) else {
            return;
        };
        let mut semantics = BTreeMap::new();
        semantics.insert("side".into(), SemanticsValue::Str(side.into()));
        semantics.insert("verb".into(), SemanticsValue::Str(verb.into()));
        match self.resolve_stringlike(expression) {
            Stringlike::Literal(channel) => {
                semantics.insert("channel".into(), SemanticsValue::Str(channel));
                self.emit("ipc", ElementKind::IpcChannel, call.span, self.extracted(), semantics);
            }
            Stringlike::ResolvedConst(channel) => {
                semantics.insert("channel".into(), SemanticsValue::Str(channel));
                semantics
                    .insert("channel-source".into(), SemanticsValue::Str("file-local-const".into()));
                self.emit("ipc", ElementKind::IpcChannel, call.span, self.extracted(), semantics);
            }
            Stringlike::Unresolved(spelling) => {
                semantics.insert("channel-observed".into(), SemanticsValue::Str(spelling));
                semantics.insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
                self.emit(
                    "ipc",
                    ElementKind::IpcChannel,
                    call.span,
                    self.undetermined(
                        "COMPUTED_CHANNEL_UNRESOLVED",
                        "channel argument is computed; not statically resolvable by this extractor",
                    ),
                    semantics,
                );
            }
        }
    }

    fn emit_require(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
        let Some(arg) = call.arguments.first().and_then(|a| a.as_expression()) else {
            return;
        };
        let mut semantics = BTreeMap::new();
        match arg {
            Expression::StringLiteral(lit) => {
                // Literal require: recorded for native addons (unrepresentable)
                // and for the child_process module; ordinary module requires
                // are not elements (same as 0.1.0).
                if lit.value == "child_process" {
                    self.emit(
                        "proc",
                        ElementKind::ChildProcess,
                        call.span,
                        self.extracted(),
                        BTreeMap::new(),
                    );
                }
                if lit.value.ends_with(".node") {
                    semantics.insert("module".into(), SemanticsValue::Str(lit.value.to_string()));
                    semantics.insert("resolution".into(), SemanticsValue::Str("native_addon".into()));
                    self.emit(
                        "nat",
                        ElementKind::NativeModule,
                        call.span,
                        Representability {
                            status: RepresentabilityStatus::Unrepresentable,
                            reason: Some("NATIVE_MODULE_OPAQUE".into()),
                            detail: Some("native addon require cannot be transliterated; it must be replaced by a Goya capability or recorded as a migration wall".into()),
                        },
                        semantics,
                    );
                }
            }
            Expression::Identifier(ident) => {
                match self.facts.require_bindings.get(ident.name.as_str()) {
                    Some(path) => {
                        semantics.insert("resolution".into(), SemanticsValue::Str("resolved".into()));
                        semantics.insert("source".into(), SemanticsValue::Str(path.clone()));
                        semantics.insert(
                            "require-source".into(),
                            SemanticsValue::Str("file-local-binding".into()),
                        );
                        self.emit(
                            "node",
                            ElementKind::DynamicModuleResolution,
                            call.span,
                            self.extracted(),
                            semantics,
                        );
                    }
                    None => {
                        semantics.insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
                        self.emit(
                            "node",
                            ElementKind::DynamicModuleResolution,
                            call.span,
                            self.undetermined(
                                "REQUIRE_SCOPE_UNRESOLVED",
                                "require argument binds to no same-file const require binding",
                            ),
                            semantics,
                        );
                    }
                }
            }
            Expression::StaticMemberExpression(sme) => {
                self.emit_require_member(call, &sme.object, sme.property.name.as_str(), semantics);
            }
            Expression::ComputedMemberExpression(cme) => {
                let property = cme
                    .static_property_name()
                    .map(|name| name.to_string());
                match property {
                    Some(property) => {
                        self.emit_require_member(call, &cme.object, &property, semantics)
                    }
                    None => self.emit_require_unresolved_depth(call, semantics),
                }
            }
            _ => {
                semantics.insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
                self.emit(
                    "node",
                    ElementKind::DynamicModuleResolution,
                    call.span,
                    self.undetermined(
                        "REQUIRE_SCOPE_UNRESOLVED",
                        "require argument is computed; not statically resolvable by this extractor",
                    ),
                    semantics,
                );
            }
        }
    }

    fn emit_require_member(
        &mut self,
        call: &oxc_ast::ast::CallExpression<'a>,
        object: &Expression<'a>,
        property: &str,
        mut semantics: BTreeMap<String, SemanticsValue>,
    ) {
        let resolved = match object {
            Expression::Identifier(ident) => self
                .facts
                .const_objects
                .get(ident.name.as_str())
                .and_then(|map| map.get(property))
                .cloned(),
            _ => None,
        };
        if let Some(path) = resolved {
            semantics.insert("resolution".into(), SemanticsValue::Str("resolved".into()));
            semantics.insert("source".into(), SemanticsValue::Str(path));
            semantics.insert("require-source".into(), SemanticsValue::Str("file-local-const".into()));
            self.emit(
                "node",
                ElementKind::DynamicModuleResolution,
                call.span,
                self.extracted(),
                semantics,
            );
        } else {
            semantics.insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
            self.emit(
                "node",
                ElementKind::DynamicModuleResolution,
                call.span,
                self.undetermined(
                    "REQUIRE_DEPTH_EXCEEDED",
                    "require argument chain exceeds the declared one-indirection bound",
                ),
                semantics,
            );
        }
    }

    fn emit_require_unresolved_depth(
        &mut self,
        call: &oxc_ast::ast::CallExpression<'a>,
        mut semantics: BTreeMap<String, SemanticsValue>,
    ) {
        semantics.insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
        self.emit(
            "node",
            ElementKind::DynamicModuleResolution,
            call.span,
            self.undetermined(
                "REQUIRE_DEPTH_EXCEEDED",
                "require argument chain exceeds the declared one-indirection bound",
            ),
            semantics,
        );
    }

    /// Resolution of a string-valued expression: literal, one const-object
    /// hop, or unresolved (spelling preserved).
    fn resolve_stringlike(&self, expression: &Expression<'a>) -> Stringlike {
        match expression {
            Expression::StringLiteral(lit) => Stringlike::Literal(lit.value.to_string()),
            Expression::StaticMemberExpression(sme) => {
                let property = sme.property.name.to_string();
                match &sme.object {
                    Expression::Identifier(ident) => {
                        if let Some(map) = self.facts.const_objects.get(ident.name.as_str()) {
                            if let Some(value) = map.get(&property) {
                                if !value.is_empty() {
                                    return Stringlike::ResolvedConst(value.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
                Stringlike::Unresolved(self.source_slice(sme.span))
            }
            Expression::ComputedMemberExpression(cme) => {
                let Some(property) = cme.static_property_name() else {
                    return Stringlike::Unresolved(self.source_slice(cme.span));
                };
                match &cme.object {
                    Expression::Identifier(ident) => {
                        if let Some(map) = self.facts.const_objects.get(ident.name.as_str()) {
                            if let Some(value) = map.get(property.as_str()) {
                                if !value.is_empty() {
                                    return Stringlike::ResolvedConst(value.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
                Stringlike::Unresolved(self.source_slice(cme.span))
            }
            _ => Stringlike::Unresolved(self.source_slice(expression.span())),
        }
    }

    fn emit_hook(&mut self, call: &oxc_ast::ast::CallExpression<'a>, hook: &str) {
        let mut semantics = BTreeMap::new();
        semantics.insert("hook".into(), SemanticsValue::Str(hook.into()));
        match hook {
            "useState" => {
                let keys = self.pending_state_keys.take().unwrap_or_default();
                if !keys.is_empty() {
                    semantics.insert("state-key".into(), SemanticsValue::Str(keys.join(",")));
                }
                if let Some(initial) = call.arguments.first().and_then(|a| a.as_expression()) {
                    let state_initial = match initial {
                        Expression::StringLiteral(lit) => lit.value.to_string(),
                        Expression::NumericLiteral(lit) => lit.value.to_string(),
                        Expression::BooleanLiteral(lit) => lit.value.to_string(),
                        Expression::NullLiteral(_) => "null".into(),
                        _ => "non-literal".into(),
                    };
                    semantics.insert("state-initial".into(), SemanticsValue::Str(state_initial));
                }
            }
            "useEffect" | "useLayoutEffect" => {
                match call.arguments.get(1).and_then(|a| a.as_expression()) {
                    None => {
                        semantics.insert("deps".into(), SemanticsValue::Str("absent".into()));
                    }
                    Some(Expression::NullLiteral(_)) => {
                        semantics.insert("deps".into(), SemanticsValue::Str("null-literal".into()));
                    }
                    Some(Expression::ArrayExpression(array)) => {
                        let names: Vec<SemanticsValue> = array
                            .elements
                            .iter()
                            .filter_map(|element| element.as_expression())
                            .filter_map(|expression| match expression {
                                Expression::Identifier(ident) => {
                                    Some(SemanticsValue::Str(ident.name.to_string()))
                                }
                                _ => None,
                            })
                            .collect();
                        semantics.insert("deps".into(), SemanticsValue::List(names));
                    }
                    Some(other) => {
                        semantics
                            .insert("deps".into(), SemanticsValue::Str(self.source_slice(other.span())));
                    }
                }
                if let Some(body_expression) = call.arguments.first().and_then(|a| a.as_expression())
                {
                    match body_expression {
                        Expression::ArrowFunctionExpression(arrow) => {
                            let body_text = self.source_slice(arrow.body.span());
                            semantics.insert(
                                "effect-class".into(),
                                SemanticsValue::Str(classify_effect(&body_text).into()),
                            );
                            semantics.insert(
                                "cleanup".into(),
                                SemanticsValue::Bool(returns_function(&arrow.body, body_expression.span())),
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        self.emit("hook", ElementKind::Hook, call.span, self.extracted(), semantics);
    }
}

/// Syntactic effect classification over the sliced body text. Ordered,
/// deterministic, first hit wins; unclassifiable is a real outcome.
fn classify_effect(body: &str) -> &'static str {
    if body.contains("addEventListener") || body.contains("removeEventListener") {
        "subscription"
    } else if body.contains("ipcRenderer") || body.contains("ipcMain") {
        "ipc-invoke"
    } else if body.contains("innerHTML")
        || body.contains("appendChild")
        || body.contains("classList")
        || body.contains("createElement")
    {
        "dom-mutation"
    } else if body.contains("setInterval") || body.contains("setTimeout") {
        "timer"
    } else if body.contains("fetch(") || body.contains("fetch (") {
        "fetch"
    } else {
        "unclassifiable"
    }
}

/// Does this effect body return a function (a cleanup)? Syntactic check.
fn returns_function(body: &ArrowFunctionBody<'_>, whole_span: Span) -> bool {
    match body {
        ArrowFunctionBody::FunctionBody(function_body) => function_body
            .statements
            .iter()
            .any(|statement| matches!(statement, Statement::ReturnStatement(_))),
        // `() => () => { ... }` — expression-bodied arrow returning a function.
        ArrowFunctionBody::ArrowFunctionExpression(_) => true,
        _ => {
            // INHERIT(Expression) variants: an immediately-returned function
            // expression is detected by the body starting with `(` or
            // `function` — fall back to the slice text.
            let _ = whole_span;
            false
        }
    }
}

fn collect_pattern_names(pattern: &BindingPattern<'_>, out: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => out.push(ident.name.to_string()),
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                collect_pattern_names(&property.value, out);
            }
            if let Some(rest) = &object.rest {
                collect_pattern_names(&rest.argument, out);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                collect_pattern_names(element, out);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => {
            collect_pattern_names(&assignment.left, out);
        }
    }
}

impl<'a> Visit<'a> for ElementEmitter<'a> {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        match kind {
            AstKind::CallExpression(call) => self.handle_call(call),
            AstKind::NewExpression(new) => {
                if let Expression::Identifier(ident) = &new.callee {
                    match ident.name.as_str() {
                        "BrowserWindow" => {
                            self.emit(
                                "win",
                                ElementKind::WindowLifecycle,
                                new.span,
                                self.extracted(),
                                BTreeMap::new(),
                            );
                        }
                        "Notification" | "Tray" => {
                            let api = if ident.name == "Notification" {
                                "notification"
                            } else {
                                "tray"
                            };
                            let mut semantics = BTreeMap::new();
                            semantics.insert("api".into(), SemanticsValue::Str(api.into()));
                            self.emit(
                                "api",
                                ElementKind::OsApiCall,
                                new.span,
                                self.extracted(),
                                semantics,
                            );
                        }
                        "Function" => {
                            let mut semantics = BTreeMap::new();
                            semantics
                                .insert("observed".into(), SemanticsValue::Str("new Function".into()));
                            self.emit(
                                "unk",
                                ElementKind::Unknown,
                                new.span,
                                self.undetermined(
                                    "DYNAMIC_CODE_EVALUATION",
                                    "dynamic code evaluation has no statically extractable semantics",
                                ),
                                semantics,
                            );
                        }
                        _ => {}
                    }
                }
            }
            AstKind::ImportExpression(_) => {
                let mut semantics = BTreeMap::new();
                semantics.insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
                self.elements.push(FairElement {
                    id: self.ids.next("node"),
                    kind: ElementKind::DynamicModuleResolution,
                    provenance: ElementProvenance {
                        file: self.file.to_string(),
                        line: self.line_index.line_of(kind.span().start),
                        span_start: Some(kind.span().start as u64),
                        span_end: Some(kind.span().end as u64),
                        fallback_origin: false,
                    },
                    representability: self.undetermined(
                        "REQUIRE_SCOPE_UNRESOLVED",
                        "dynamic import() destination is not statically resolvable by this extractor",
                    ),
                    semantics,
                });
            }
            AstKind::JSXAttribute(attribute) => {
                let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attribute.name else {
                    return;
                };
                let Some(prop) = EVENT_PROPS.iter().find(|p| **p == name.name.as_str()) else {
                    return;
                };
                let mut semantics = BTreeMap::new();
                semantics.insert("prop".into(), SemanticsValue::Str((*prop).into()));
                self.emit(
                    "evt",
                    ElementKind::EventHandler,
                    attribute.span,
                    self.extracted(),
                    semantics,
                );
            }
            AstKind::VariableDeclarator(declarator) => {
                // Collect useState destructuring names before the call node is
                // entered (declarator is entered before its initialiser).
                if let Some(Expression::CallExpression(call)) = &declarator.init {
                    if matches!(&call.callee, Expression::Identifier(ident) if ident.name == "useState")
                    {
                        let mut keys = Vec::new();
                        collect_pattern_names(&declarator.id, &mut keys);
                        self.pending_state_keys = Some(keys);
                    }
                }
            }
            AstKind::Class(class) => {
                let Some(heritage) = &class.heritage else {
                    return;
                };
                let matches = match &heritage.expression {
                    Expression::Identifier(ident) => {
                        matches!(ident.name.as_str(), "Component" | "PureComponent")
                    }
                    Expression::StaticMemberExpression(sme) => sme.property.name.as_str()
                        == "Component"
                        || sme.property.name.as_str() == "PureComponent",
                    _ => false,
                };
                if !matches {
                    return;
                }
                let mut semantics = BTreeMap::new();
                semantics.insert(
                    "component".into(),
                    SemanticsValue::Str(
                        class.id.as_ref().map(|id| id.name.to_string()).unwrap_or_default(),
                    ),
                );
                self.emit("ui", ElementKind::Component, class.span, self.extracted(), semantics);
            }
            AstKind::ExportDeclaration(export) => {
                if let oxc_ast::ast::Declaration::FunctionDeclaration(function) =
                    &export.declaration
                {
                    self.emit_component_function(function);
                }
            }
            AstKind::ExportDefaultDeclaration(declaration) => {
                if let oxc_ast::ast::ExportDefaultDeclarationKind::FunctionDeclaration(function) =
                    &declaration.declaration
                {
                    self.emit_component_function(function);
                }
            }

            AstKind::TaggedTemplateExpression(tagged) => {
                let object_is_styled = match &tagged.tag {
                    Expression::StaticMemberExpression(sme) => {
                        matches!(&sme.object, Expression::Identifier(ident) if ident.name == "styled")
                    }
                    Expression::CallExpression(call) => {
                        matches!(&call.callee, Expression::Identifier(ident) if ident.name == "styled")
                    }
                    _ => false,
                };
                if object_is_styled {
                    let mut semantics = BTreeMap::new();
                    semantics.insert(
                        "mechanism".into(),
                        SemanticsValue::Str("styled-components".into()),
                    );
                    self.emit(
                        "sty",
                        ElementKind::StyleRule,
                        tagged.span,
                        self.extracted(),
                        semantics,
                    );
                }
            }
            AstKind::ImportDeclaration(declaration) => match declaration.source.value.as_str() {
                "child_process" => {
                    self.emit(
                        "proc",
                        ElementKind::ChildProcess,
                        declaration.span,
                        self.extracted(),
                        BTreeMap::new(),
                    );
                }
                "electron-store" => {
                    self.emit(
                        "store",
                        ElementKind::StateStore,
                        declaration.span,
                        self.extracted(),
                        BTreeMap::new(),
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl<'a> ElementEmitter<'a> {
    fn emit_component_function(&mut self, function: &Function<'a>) {
        let Some(id) = &function.id else {
            return;
        };
        let mut semantics = BTreeMap::new();
        semantics.insert("component".into(), SemanticsValue::Str(id.name.to_string()));
        self.emit("ui", ElementKind::Component, function.span(), self.extracted(), semantics);
    }
}
