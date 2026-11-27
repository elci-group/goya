//! JavaScript and TypeScript pass (Oxc). Static only: the code is parsed and
//! never run.
//!
//! Names are resolved by binding only: `const { ipcMain } = require('electron')`
//! makes `ipcMain.handle` read as `electron.ipcMain.handle`. There is no scope
//! analysis, so a shadowing local is read as the outer binding.

use crate::emit::{b, list, s, Emitter, Origin, Semantics, Status};
use goya_fair::{ElementKind, SemanticsValue};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{walk, Visit};
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::scope::ScopeFlags;
use std::collections::{BTreeMap, BTreeSet};

/// A `window.<key>.<member>(…)` call outside any DOM handler. Whether it is a
/// bridge call depends on keys exposed in other files, so the caller decides.
pub struct BridgeCandidate {
    pub span: (u32, u32),
    pub key: String,
    pub member: String,
}

pub struct JsOutput {
    pub bridge_candidates: Vec<BridgeCandidate>,
    /// Keys this file passes to `contextBridge.exposeInMainWorld`.
    pub exposed_keys: BTreeSet<String>,
}

fn source_type(path: &str) -> SourceType {
    if path.ends_with(".cjs") {
        SourceType::cjs()
    } else if path.ends_with(".mjs") {
        SourceType::mjs()
    } else if path.ends_with(".js") {
        SourceType::unambiguous()
    } else {
        SourceType::from_path(path).unwrap_or_else(|_| SourceType::unambiguous())
    }
}

pub fn extract(text: &str, em: &mut Emitter) -> JsOutput {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, source_type(em.file())).parse();
    if parsed.diagnostics.has_errors() || parsed.panicked {
        let message = parsed
            .diagnostics
            .errors()
            .next()
            .map(|d| d.message.to_string())
            .unwrap_or_else(|| "parser stopped".into());
        let mut sem = Semantics::new();
        sem.insert("panicked".into(), b(parsed.panicked));
        em.unknown(
            "javascript_parse_error",
            Origin::Line {
                line: 1,
                fallback: true,
            },
            "PARSE_FAILED",
            message,
            sem,
        );
    }
    let mut walker = Walker {
        text,
        em,
        bindings: BTreeMap::new(),
        dom: BTreeMap::new(),
        windows: BTreeSet::new(),
        lets: BTreeMap::new(),
        depth: 0,
        handler_depth: 0,
        out: JsOutput {
            bridge_candidates: Vec::new(),
            exposed_keys: BTreeSet::new(),
        },
    };
    walker.collect_module_lets(&parsed.program);
    walker.visit_program(&parsed.program);
    walker.emit_state_stores();
    walker.out
}

struct LetInfo {
    span: (u32, u32),
    kind: &'static str,
    init: Option<String>,
    assignment_lines: BTreeSet<u64>,
}

struct Walker<'t, 'e> {
    text: &'t str,
    em: &'e mut Emitter,
    /// Local name → canonical name (`electron.ipcMain`, `fs`, `path`).
    bindings: BTreeMap<String, String>,
    /// Local name → DOM element ID, from `getElementById` or `querySelector('#…')`.
    dom: BTreeMap<String, String>,
    /// Locals bound to `new BrowserWindow(…)`.
    windows: BTreeSet<String>,
    /// Module-scope `let` and `var` bindings.
    lets: BTreeMap<String, LetInfo>,
    depth: usize,
    handler_depth: usize,
    out: JsOutput,
}

/// Node built-ins are named without the `node:` prefix.
fn canonical_module(name: &str) -> String {
    name.strip_prefix("node:").unwrap_or(name).to_string()
}

fn string_arg<'a>(args: &'a [Argument<'a>], index: usize) -> Option<&'a str> {
    match args.get(index) {
        Some(Argument::StringLiteral(lit)) => Some(lit.value.as_str()),
        Some(Argument::TemplateLiteral(t)) if t.expressions.is_empty() && t.quasis.len() == 1 => {
            t.quasis[0].value.cooked.as_ref().map(|c| c.as_str())
        }
        _ => None,
    }
}

/// `require('<literal>')`, possibly followed by `.member`.
fn require_target(expr: &Expression<'_>) -> Option<(String, Option<String>)> {
    match expr.without_parentheses() {
        Expression::CallExpression(call) if is_require(&call.callee) => {
            string_arg(&call.arguments, 0).map(|m| (canonical_module(m), None))
        }
        Expression::StaticMemberExpression(m) => require_target(&m.object)
            .filter(|(_, member)| member.is_none())
            .map(|(module, _)| (module, Some(m.property.name.as_str().to_string()))),
        _ => None,
    }
}

fn is_require(callee: &Expression<'_>) -> bool {
    matches!(callee.without_parentheses(), Expression::Identifier(id) if id.name.as_str() == "require")
}

fn chain_with(
    bindings: &BTreeMap<String, String>,
    windows: &BTreeSet<String>,
    expr: &Expression<'_>,
) -> Option<String> {
    match expr.without_parentheses() {
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            Some(if let Some(bound) = bindings.get(name) {
                bound.clone()
            } else if windows.contains(name) {
                "BrowserWindow(instance)".to_string()
            } else {
                name.to_string()
            })
        }
        Expression::StaticMemberExpression(m) => Some(format!(
            "{}.{}",
            chain_with(bindings, windows, &m.object)?,
            m.property.name.as_str()
        )),
        _ => None,
    }
}

/// Normalises a `/`-separated relative path. `None` if it escapes the root.
pub fn normalise(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

pub fn parent_dir(file: &str) -> &str {
    file.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

const OS_API_MODULES: &[&str] = &[
    "autoUpdater",
    "clipboard",
    "crashReporter",
    "desktopCapturer",
    "dialog",
    "globalShortcut",
    "Menu",
    "MenuItem",
    "nativeImage",
    "nativeTheme",
    "net",
    "Notification",
    "powerMonitor",
    "powerSaveBlocker",
    "protocol",
    "safeStorage",
    "screen",
    "session",
    "shell",
    "systemPreferences",
    "Tray",
];

impl<'t, 'e> Walker<'t, 'e> {
    fn chain(&self, expr: &Expression<'_>) -> Option<String> {
        chain_with(&self.bindings, &self.windows, expr)
    }

    fn slice(&self, span: oxc_span::Span) -> &'t str {
        &self.text[span.start as usize..span.end as usize]
    }

    fn origin(span: oxc_span::Span) -> Origin {
        Origin::Span(span.start, span.end)
    }

    fn collect_module_lets(&mut self, program: &Program<'_>) {
        for stmt in &program.body {
            if let Statement::VariableDeclaration(decl) = stmt {
                let kind = match decl.kind {
                    VariableDeclarationKind::Let => "let",
                    VariableDeclarationKind::Var => "var",
                    _ => continue,
                };
                for d in &decl.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &d.id {
                        let init = d.init.as_ref().map(|e| {
                            let span = oxc_span::GetSpan::span(e);
                            self.slice(span).to_string()
                        });
                        self.lets.insert(
                            id.name.as_str().to_string(),
                            LetInfo {
                                span: (d.span.start, d.span.end),
                                kind,
                                init,
                                assignment_lines: BTreeSet::new(),
                            },
                        );
                    }
                }
            }
        }
    }

    /// Module-scope mutable bindings assigned inside a function are state.
    fn emit_state_stores(&mut self) {
        let mut found: Vec<_> = self
            .lets
            .iter()
            .filter(|(_, info)| !info.assignment_lines.is_empty())
            .collect();
        found.sort_by_key(|(_, info)| info.span.0);
        let mut pending = Vec::new();
        for (name, info) in found {
            let mut sem = Semantics::new();
            sem.insert("name".into(), s(name.clone()));
            sem.insert("declaration".into(), s(info.kind));
            if let Some(init) = &info.init {
                sem.insert("initial".into(), s(init.clone()));
            }
            sem.insert(
                "assigned_at_lines".into(),
                list(
                    info.assignment_lines
                        .iter()
                        .map(|l| SemanticsValue::Int(*l as i64)),
                ),
            );
            sem.insert(
                "basis".into(),
                s("module-scope binding assigned inside a function"),
            );
            pending.push((info.span, sem));
        }
        for (span, sem) in pending {
            self.em.emit(
                ElementKind::StateStore,
                Origin::Span(span.0, span.1),
                Status::Extracted,
                sem,
            );
        }
    }

    fn note_assignment(&mut self, name: &str, offset: u32) {
        if self.depth > 0 {
            let line = self.em.line_of(offset as usize);
            if let Some(info) = self.lets.get_mut(name) {
                info.assignment_lines.insert(line);
            }
        }
    }

    fn literal(&self, expr: &Expression<'_>) -> SemanticsValue {
        match expr.without_parentheses() {
            Expression::StringLiteral(l) => s(l.value.as_str()),
            Expression::BooleanLiteral(l) => b(l.value),
            Expression::NumericLiteral(n) if n.value.fract() == 0.0 && n.value.abs() < 9.0e15 => {
                SemanticsValue::Int(n.value as i64)
            }
            Expression::ObjectExpression(obj) => {
                let mut map = BTreeMap::new();
                for prop in &obj.properties {
                    match prop {
                        ObjectPropertyKind::ObjectProperty(p) => match p.key.static_name() {
                            Some(key) => {
                                map.insert(key.to_string(), self.literal(&p.value));
                            }
                            None => {
                                map.insert(
                                    format!("[computed@{}]", p.span.start),
                                    self.expression_text(p.span),
                                );
                            }
                        },
                        ObjectPropertyKind::SpreadProperty(sp) => {
                            map.insert(
                                format!("[spread@{}]", sp.span.start),
                                self.expression_text(sp.span),
                            );
                        }
                    }
                }
                SemanticsValue::Map(map)
            }
            other => self.expression_text(oxc_span::GetSpan::span(other)),
        }
    }

    fn expression_text(&self, span: oxc_span::Span) -> SemanticsValue {
        let mut map = BTreeMap::new();
        map.insert("expression".into(), s(self.slice(span)));
        SemanticsValue::Map(map)
    }

    /// Resolves `path.join(__dirname, 'a', …)` (or `path.resolve`) in this file.
    fn dirname_path(&self, expr: &Expression<'_>) -> Option<String> {
        let Expression::CallExpression(call) = expr.without_parentheses() else {
            return None;
        };
        let callee = self.chain(&call.callee)?;
        if callee != "path.join" && callee != "path.resolve" {
            return None;
        }
        let mut args = call.arguments.iter();
        match args.next()? {
            Argument::Identifier(id) if id.name.as_str() == "__dirname" => {}
            _ => return None,
        }
        let mut joined = parent_dir(self.em.file()).to_string();
        for arg in args {
            let Argument::StringLiteral(lit) = arg else {
                return None;
            };
            joined.push('/');
            joined.push_str(lit.value.as_str());
        }
        normalise(&joined)
    }

    fn scan(&self, expr: &Expression<'_>) -> Scan<'_> {
        let mut scan = Scan {
            bindings: &self.bindings,
            windows: &self.windows,
            lets: &self.lets,
            prevents_default: false,
            window_calls: BTreeSet::new(),
            mutates: BTreeSet::new(),
            reads: BTreeSet::new(),
            ipc: BTreeSet::new(),
        };
        scan.visit_expression(expr);
        scan
    }

    fn ipc(&mut self, call: &CallExpression<'_>, side: &str, method: &str) {
        let mut sem = Semantics::new();
        sem.insert("side".into(), s(side));
        sem.insert("method".into(), s(method));
        let status = match string_arg(&call.arguments, 0) {
            Some(channel) => {
                sem.insert("channel".into(), s(channel));
                Status::Extracted
            }
            None => Status::Undetermined {
                reason: "IPC_CHANNEL_COMPUTED",
                detail: "the channel name is not a string literal".into(),
            },
        };
        if let Some(handler) = call.arguments.get(1).and_then(|a| a.as_expression()) {
            let scan = self.scan(handler);
            if !scan.mutates.is_empty() {
                sem.insert(
                    "mutates".into(),
                    list(scan.mutates.iter().map(|m| s(m.clone()))),
                );
            }
            if !scan.reads.is_empty() {
                sem.insert(
                    "reads".into(),
                    list(scan.reads.iter().map(|m| s(m.clone()))),
                );
            }
        }
        self.em.emit(
            ElementKind::IpcChannel,
            Self::origin(call.span),
            status,
            sem,
        );
    }

    fn event_handler(
        &mut self,
        span: oxc_span::Span,
        event: &str,
        target: SemanticsValue,
        via: &str,
        handler: Option<&Expression<'_>>,
    ) {
        let mut sem = Semantics::new();
        sem.insert("event".into(), s(event));
        sem.insert("target".into(), target);
        sem.insert("via".into(), s(via));
        if let Some(handler) = handler {
            let scan = self.scan(handler);
            sem.insert("prevents_default".into(), b(scan.prevents_default));
            if !scan.window_calls.is_empty() {
                sem.insert(
                    "window_calls".into(),
                    list(scan.window_calls.iter().map(|c| s(c.clone()))),
                );
            }
            if !scan.mutates.is_empty() {
                sem.insert(
                    "mutates".into(),
                    list(scan.mutates.iter().map(|m| s(m.clone()))),
                );
            }
            if !scan.reads.is_empty() {
                sem.insert(
                    "reads".into(),
                    list(scan.reads.iter().map(|m| s(m.clone()))),
                );
            }
        }
        self.em.emit(
            ElementKind::EventHandler,
            Self::origin(span),
            Status::Extracted,
            sem,
        );
    }

    fn dom_target(&self, expr: &Expression<'_>) -> SemanticsValue {
        if let Expression::Identifier(id) = expr.without_parentheses() {
            if let Some(dom_id) = self.dom.get(id.name.as_str()) {
                let mut map = BTreeMap::new();
                map.insert("dom_id".into(), s(dom_id.clone()));
                return SemanticsValue::Map(map);
            }
        }
        match self.chain(expr) {
            Some(chain) => {
                let mut map = BTreeMap::new();
                map.insert("chain".into(), s(chain));
                SemanticsValue::Map(map)
            }
            None => self.expression_text(oxc_span::GetSpan::span(expr)),
        }
    }

    fn module_import(&mut self, span: oxc_span::Span, module: &str, form: &str) {
        let mut sem = Semantics::new();
        sem.insert("module".into(), s(module));
        sem.insert("form".into(), s(form));
        if module.ends_with(".node") {
            self.em.emit(
                ElementKind::NativeModule,
                Self::origin(span),
                Status::Extracted,
                sem,
            );
        } else {
            self.em.emit(
                ElementKind::ModuleImport,
                Self::origin(span),
                Status::Extracted,
                sem,
            );
        }
    }

    fn api_element(&mut self, kind: ElementKind, call: &CallExpression<'_>, api: &str) {
        let mut sem = Semantics::new();
        sem.insert("api".into(), s(api));
        if let Some(arg) = string_arg(&call.arguments, 0) {
            sem.insert("first_argument".into(), s(arg));
        }
        self.em
            .emit(kind, Self::origin(call.span), Status::Extracted, sem);
    }

    fn classify_call(&mut self, call: &CallExpression<'_>) {
        if is_require(&call.callee) {
            match string_arg(&call.arguments, 0) {
                Some(module) => self.module_import(call.span, &canonical_module(module), "require"),
                None => self.dynamic(call.span, "require"),
            }
            return;
        }
        let Some(callee) = self.chain(&call.callee) else {
            return;
        };
        let (head, method) = callee.rsplit_once('.').unwrap_or(("", callee.as_str()));
        match head {
            "electron.ipcMain" => return self.ipc(call, "main", method),
            "electron.ipcRenderer" => return self.ipc(call, "renderer", method),
            "BrowserWindow(instance).webContents" if method == "send" => {
                return self.ipc(call, "main", "webContents.send")
            }
            _ => {}
        }
        if callee == "electron.contextBridge.exposeInMainWorld" {
            return self.preload_bridge(call);
        }
        if head == "electron.app" || head.starts_with("BrowserWindow(instance)") {
            let api = callee.replacen("electron.", "", 1);
            let mut sem = Semantics::new();
            sem.insert("api".into(), s(api));
            if let Some(event) = string_arg(&call.arguments, 0) {
                sem.insert("first_argument".into(), s(event));
            }
            if matches!(method, "loadFile" | "loadURL") {
                if let Some(arg) = call.arguments.first().and_then(|a| a.as_expression()) {
                    let resolved = match (
                        method,
                        self.dirname_path(arg),
                        string_arg(&call.arguments, 0),
                    ) {
                        ("loadFile", Some(p), _) => Some(p),
                        // loadFile resolves a relative path against the app root.
                        ("loadFile", None, Some(lit)) => normalise(lit),
                        _ => None,
                    };
                    match resolved {
                        Some(p) => {
                            sem.insert("resolved_path".into(), s(p));
                        }
                        None => {
                            sem.insert("target".into(), self.literal(arg));
                        }
                    }
                }
            }
            self.em.emit(
                ElementKind::WindowLifecycle,
                Self::origin(call.span),
                Status::Extracted,
                sem,
            );
            return;
        }
        if let Some(rest) = head
            .strip_prefix("electron.")
            .or((head == "electron").then_some(""))
        {
            let module = rest.split('.').next().unwrap_or("");
            let module = if module.is_empty() { method } else { module };
            if OS_API_MODULES.contains(&module) {
                return self.api_element(
                    ElementKind::OsApiCall,
                    call,
                    &callee["electron.".len()..],
                );
            }
            let mut sem = Semantics::new();
            sem.insert("api".into(), s(callee.clone()));
            self.em.unknown(
                &format!("electron_api:{callee}"),
                Self::origin(call.span),
                "ELECTRON_API_UNCLASSIFIED",
                "an Electron API this extractor version does not classify",
                sem,
            );
            return;
        }
        let root = callee.split('.').next().unwrap_or("");
        match root {
            "fs" | "fs/promises" => {
                return self.api_element(ElementKind::FilesystemOperation, call, &callee)
            }
            "child_process" => return self.api_element(ElementKind::ChildProcess, call, &callee),
            _ => {}
        }
        if matches!(
            head,
            "localStorage" | "sessionStorage" | "window.localStorage" | "window.sessionStorage"
        ) || callee == "indexedDB.open"
            || callee == "window.indexedDB.open"
        {
            return self.api_element(ElementKind::StateStore, call, &callee);
        }
        if method == "addEventListener" {
            if let (Some(event), Expression::StaticMemberExpression(m)) = (
                string_arg(&call.arguments, 0),
                call.callee.without_parentheses(),
            ) {
                let target = self.dom_target(&m.object);
                let handler = call.arguments.get(1).and_then(|a| a.as_expression());
                self.event_handler(call.span, event, target, "addEventListener", handler);
            }
            return;
        }
        if self.handler_depth == 0 {
            let parts: Vec<&str> = callee.split('.').collect();
            if parts.len() == 3 && parts[0] == "window" {
                self.out.bridge_candidates.push(BridgeCandidate {
                    span: (call.span.start, call.span.end),
                    key: parts[1].to_string(),
                    member: parts[2].to_string(),
                });
            }
        }
    }

    fn dynamic(&mut self, span: oxc_span::Span, form: &str) {
        let mut sem = Semantics::new();
        sem.insert("form".into(), s(form));
        sem.insert("resolution".into(), s("unresolved"));
        sem.insert("expression".into(), s(self.slice(span)));
        self.em.emit(
            ElementKind::DynamicModuleResolution,
            Self::origin(span),
            Status::Undetermined {
                reason: "DYNAMIC_REQUIRE_UNRESOLVED",
                detail: "the module specifier is not a string literal".into(),
            },
            sem,
        );
    }

    fn preload_bridge(&mut self, call: &CallExpression<'_>) {
        let mut sem = Semantics::new();
        let key = string_arg(&call.arguments, 0);
        let status = match key {
            Some(key) => {
                sem.insert("key".into(), s(key));
                self.out.exposed_keys.insert(key.to_string());
                Status::Extracted
            }
            None => Status::Undetermined {
                reason: "BRIDGE_KEY_COMPUTED",
                detail: "the exposed key is not a string literal".into(),
            },
        };
        if let Some(Expression::ObjectExpression(obj)) = call
            .arguments
            .get(1)
            .and_then(|a| a.as_expression())
            .map(|e| e.without_parentheses())
        {
            let mut members = BTreeMap::new();
            for prop in &obj.properties {
                if let ObjectPropertyKind::ObjectProperty(p) = prop {
                    let name = p
                        .key
                        .static_name()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| format!("[computed@{}]", p.span.start));
                    let scan = self.scan(&p.value);
                    let mut member = BTreeMap::new();
                    member.insert("ipc".into(), list(scan.ipc.iter().map(|c| s(c.clone()))));
                    members.insert(name, SemanticsValue::Map(member));
                }
            }
            sem.insert("members".into(), SemanticsValue::Map(members));
        }
        self.em.emit(
            ElementKind::PreloadBridge,
            Self::origin(call.span),
            status,
            sem,
        );
    }
}

impl<'a> Visit<'a> for Walker<'_, '_> {
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let module = canonical_module(it.source.value.as_str());
        self.module_import(it.span, &module, "import");
        for spec in it.specifiers.iter().flatten() {
            match spec {
                ImportDeclarationSpecifier::ImportSpecifier(sp) => {
                    let imported = match &sp.imported {
                        ModuleExportName::IdentifierName(n) => n.name.as_str().to_string(),
                        ModuleExportName::IdentifierReference(n) => n.name.as_str().to_string(),
                        ModuleExportName::StringLiteral(n) => n.value.as_str().to_string(),
                    };
                    self.bindings.insert(
                        sp.local.name.as_str().to_string(),
                        format!("{module}.{imported}"),
                    );
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(sp) => {
                    self.bindings
                        .insert(sp.local.name.as_str().to_string(), module.clone());
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(sp) => {
                    self.bindings
                        .insert(sp.local.name.as_str().to_string(), module.clone());
                }
            }
        }
        walk::walk_import_declaration(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        match it.source.without_parentheses() {
            Expression::StringLiteral(lit) => {
                let module = canonical_module(lit.value.as_str());
                self.module_import(it.span, &module, "import()");
            }
            _ => self.dynamic(it.span, "import()"),
        }
        walk::walk_import_expression(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        if let Some(init) = &it.init {
            if let Some((module, member)) = require_target(init) {
                let base = match member {
                    Some(m) => format!("{module}.{m}"),
                    None => module,
                };
                match &it.id {
                    BindingPattern::BindingIdentifier(id) => {
                        self.bindings.insert(id.name.as_str().to_string(), base);
                    }
                    BindingPattern::ObjectPattern(pattern) => {
                        for prop in &pattern.properties {
                            if let (Some(key), BindingPattern::BindingIdentifier(local)) =
                                (prop.key.static_name(), &prop.value)
                            {
                                self.bindings.insert(
                                    local.name.as_str().to_string(),
                                    format!("{base}.{key}"),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let BindingPattern::BindingIdentifier(id) = &it.id {
                let local = id.name.as_str().to_string();
                match init.without_parentheses() {
                    Expression::CallExpression(call) => {
                        let callee = self.chain(&call.callee);
                        let arg = string_arg(&call.arguments, 0);
                        match (callee.as_deref(), arg) {
                            (Some("document.getElementById"), Some(dom_id)) => {
                                self.dom.insert(local, dom_id.to_string());
                            }
                            (Some("document.querySelector"), Some(sel))
                                if sel.starts_with('#')
                                    && sel[1..].chars().all(|c| {
                                        c.is_ascii_alphanumeric() || c == '-' || c == '_'
                                    }) =>
                            {
                                self.dom.insert(local, sel[1..].to_string());
                            }
                            _ => {}
                        }
                    }
                    Expression::NewExpression(new)
                        if self.chain(&new.callee).as_deref() == Some("electron.BrowserWindow") =>
                    {
                        self.windows.insert(local);
                    }
                    _ => {}
                }
            }
        }
        walk::walk_variable_declarator(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.classify_call(it);
        let is_listener = matches!(
            it.callee.without_parentheses(),
            Expression::StaticMemberExpression(m) if m.property.name.as_str() == "addEventListener"
        );
        if is_listener {
            self.handler_depth += 1;
        }
        walk::walk_call_expression(self, it);
        if is_listener {
            self.handler_depth -= 1;
        }
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if let Some(callee) = self.chain(&it.callee) {
            if callee == "electron.BrowserWindow" {
                let mut sem = Semantics::new();
                sem.insert("api".into(), s("new BrowserWindow"));
                if let Some(options) = it.arguments.first().and_then(|a| a.as_expression()) {
                    sem.insert("options".into(), self.literal(options));
                    if let Expression::ObjectExpression(obj) = options.without_parentheses() {
                        let preload = obj.properties.iter().find_map(|p| match p {
                            ObjectPropertyKind::ObjectProperty(p)
                                if p.key.static_name().as_deref() == Some("webPreferences") =>
                            {
                                match p.value.without_parentheses() {
                                    Expression::ObjectExpression(prefs) => {
                                        prefs.properties.iter().find_map(|q| match q {
                                            ObjectPropertyKind::ObjectProperty(q)
                                                if q.key.static_name().as_deref()
                                                    == Some("preload") =>
                                            {
                                                Some(&q.value)
                                            }
                                            _ => None,
                                        })
                                    }
                                    _ => None,
                                }
                            }
                            _ => None,
                        });
                        if let Some(path) = preload.and_then(|e| self.dirname_path(e)) {
                            sem.insert("preload_path".into(), s(path));
                        }
                    }
                }
                self.em.emit(
                    ElementKind::WindowLifecycle,
                    Self::origin(it.span),
                    Status::Extracted,
                    sem,
                );
            } else if let Some(rest) = callee.strip_prefix("electron.") {
                let mut sem = Semantics::new();
                sem.insert("api".into(), s(format!("new {rest}")));
                if OS_API_MODULES.contains(&rest) {
                    self.em.emit(
                        ElementKind::OsApiCall,
                        Self::origin(it.span),
                        Status::Extracted,
                        sem,
                    );
                } else {
                    self.em.unknown(
                        &format!("electron_api:new {rest}"),
                        Self::origin(it.span),
                        "ELECTRON_API_UNCLASSIFIED",
                        "an Electron constructor this extractor version does not classify",
                        sem,
                    );
                }
            }
        }
        walk::walk_new_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        match &it.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                let name = id.name.as_str().to_string();
                self.note_assignment(&name, it.span.start);
            }
            AssignmentTarget::StaticMemberExpression(m) => {
                let prop = m.property.name.as_str();
                if prop.len() > 2
                    && prop.starts_with("on")
                    && prop[2..].chars().all(|c| c.is_ascii_lowercase())
                {
                    let target = self.dom_target(&m.object);
                    let event = prop[2..].to_string();
                    self.event_handler(it.span, &event, target, "property", Some(&it.right));
                }
            }
            _ => {}
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        if let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &it.argument {
            let name = id.name.as_str().to_string();
            self.note_assignment(&name, it.span.start);
        }
        walk::walk_update_expression(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if self.depth == 0 && it.r#type == FunctionType::FunctionDeclaration {
            if let Some(id) = &it.id {
                let mut sem = Semantics::new();
                sem.insert("name".into(), s(id.name.as_str()));
                sem.insert(
                    "parameters".into(),
                    SemanticsValue::Int(it.params.items.len() as i64),
                );
                sem.insert("async".into(), b(it.r#async));
                sem.insert("form".into(), s("function_declaration"));
                self.em.emit(
                    ElementKind::LogicUnit,
                    Self::origin(it.span),
                    Status::Extracted,
                    sem,
                );
            }
        }
        self.depth += 1;
        walk::walk_function(self, it, flags);
        self.depth -= 1;
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.depth += 1;
        walk::walk_arrow_function_expression(self, it);
        self.depth -= 1;
    }
}

/// Scans a handler or bridge member without emitting anything.
struct Scan<'w> {
    bindings: &'w BTreeMap<String, String>,
    windows: &'w BTreeSet<String>,
    lets: &'w BTreeMap<String, LetInfo>,
    prevents_default: bool,
    window_calls: BTreeSet<String>,
    mutates: BTreeSet<String>,
    /// Module-scope bindings read (a plain `x = …` is a write only).
    reads: BTreeSet<String>,
    ipc: BTreeSet<String>,
}

impl<'a> Visit<'a> for Scan<'_> {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(callee) = chain_with(self.bindings, self.windows, &it.callee) {
            if callee.ends_with(".preventDefault") {
                self.prevents_default = true;
            }
            if callee.starts_with("window.")
                && callee.matches('.').count() >= 2
                && !callee.ends_with(".addEventListener")
            {
                self.window_calls.insert(callee.clone());
            }
            if let Some(method) = callee.strip_prefix("electron.ipcRenderer.") {
                if let Some(channel) = string_arg(&it.arguments, 0) {
                    self.ipc.insert(format!("{method} {channel}"));
                }
            }
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let AssignmentTarget::AssignmentTargetIdentifier(id) = &it.left {
            if self.lets.contains_key(id.name.as_str()) {
                self.mutates.insert(id.name.as_str().to_string());
                if it.operator == AssignmentOperator::Assign {
                    // A plain assignment writes the binding without reading it.
                    self.visit_expression(&it.right);
                    return;
                }
            }
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if self.lets.contains_key(it.name.as_str()) {
            self.reads.insert(it.name.as_str().to_string());
        }
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        if let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &it.argument {
            if self.lets.contains_key(id.name.as_str()) {
                self.mutates.insert(id.name.as_str().to_string());
            }
        }
        walk::walk_update_expression(self, it);
    }
}
