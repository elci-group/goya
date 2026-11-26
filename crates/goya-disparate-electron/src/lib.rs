//! Electron + React semantic extraction into GY-FAIR/1 (Disparates Phase 1).
//!
//! Deterministic, offline, static extraction: the foreign application is never
//! executed and no LLM participates. Extractor 0.2.0 (ADR 0001) parses
//! TS/JS with oxc and emits elements in AST source order; the 0.1.0
//! line-oriented lexical scanner remains as the declared fallback for CSS
//! files, unparseable files, and files over 1 MiB. Recognition limits are
//! part of the declared identity in [`EXTRACTOR_ID_RULE`]: resolutions are
//! bounded (one same-file indirection for `require` bindings, one file-local
//! const-object lookup for computed channels), effect classification is
//! syntactic only, and anything not statically resolvable is recorded as
//! `undetermined` rather than dropped.
//!
//! Contract: `GY-FAIR/1`. See `docs/disparates/contracts/fair-1.md`.

use goya_fair::{
    ElementKind, ElementProvenance, ExtractorIdentity, FairDocument, FairElement,
    FrameworkDetection, ProjectProvenance, Representability, RepresentabilityStatus,
    SemanticsValue, SourceFile, SourceManifest, FAIR_CONTRACT,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub mod ast;

/// Extractor identity (recorded in every produced document).
pub const EXTRACTOR_NAME: &str = "goya-disparate-electron";
/// Extractor version.
pub const EXTRACTOR_VERSION: &str = "0.2.0";
/// Element-ID assignment rule (recorded in every produced document).
pub const EXTRACTOR_ID_RULE: &str = "one element per recognised construct; IDs assigned \
     sequentially per kind in (sorted relative file path, AST source-order DFS or \
     scanner line order for fallback files) as <prefix>-<NNNN>; the producing \
     mechanism (AST or scanner fallback) is per file, with CSS and >1MiB files \
     always on the scanner path";
/// Scanner file-size guard: files above this use the scanner path regardless.
const MAX_AST_FILE_BYTES: u64 = 1024 * 1024;

/// Dependencies that statically indicate a native addon and therefore an
/// unrepresentable migration element. The list grows by contract revision.
const KNOWN_NATIVE_MODULES: &[&str] = &[
    "better-sqlite3", "keytar", "node-gyp", "ffi-napi", "ref-napi",
    "node-pty", "sqlite3", "bcrypt", "canvas", "native-abort-controller",
];

/// Directories never scanned.
const SKIPPED_DIRS: &[&str] = &[
    "node_modules", "dist", "build", "out", "coverage", ".git", ".github",
    ".storybook", "target",
];

/// Scanned source extensions.
const SCANNED_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "css"];

/// Extraction failure modes.
#[derive(Debug)]
pub enum ExtractionError {
    /// The root has no `package.json`; not an application root.
    MissingManifest(PathBuf),
    /// Filesystem or parse failure, with context.
    Io(String),
}

impl std::fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingManifest(p) => {
                write!(f, "no package.json at {}", p.display())
            }
            Self::Io(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ExtractionError {}

/// Discovery facts about the foreign application.
#[derive(Debug, Clone)]
pub struct Discovery {
    /// Project name from its manifest.
    pub name: String,
    /// Declared main entry point, if any.
    pub main_entry: Option<String>,
    /// Framework detections with evidence.
    pub frameworks: Vec<FrameworkDetection>,
}

/// Extract a FAIR document for the Electron (or Electron + React) application
/// rooted at `root`. Deterministic: identical bytes plus identical extractor
/// version yield byte-identical canonical output.
pub fn extract_app(root: &Path) -> Result<FairDocument, ExtractionError> {
    let manifest_path = root.join("package.json");
    if !manifest_path.is_file() {
        return Err(ExtractionError::MissingManifest(manifest_path));
    }
    let manifest_bytes = std::fs::read(&manifest_path)
        .map_err(|e| ExtractionError::Io(format!("reading package.json: {e}")))?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| ExtractionError::Io(format!("parsing package.json: {e}")))?;

    let discovery = discover(root, &manifest);
    let mut manifest_files = Vec::new();
    manifest_files.push(source_file(root, Path::new("package.json"))?);

    let mut ids = ast::IdAlloc::default();
    let mut elements = Vec::new();
    for rel in sorted_source_files(root)? {
        manifest_files.push(source_file(root, &rel)?);
        let bytes = std::fs::read(root.join(&rel))
            .map_err(|e| ExtractionError::Io(format!("reading {:?}: {e}", rel.display())))?;
        let text = String::from_utf8_lossy(&bytes);
        let file = rel.display().to_string();
        if is_css(&rel) {
            for (idx, line) in text.lines().enumerate() {
                scan_line(&mut elements, &mut ids, &file, idx as u64 + 1, line, true);
            }
            continue;
        }
        // AST path with declared fallbacks: unparseable or oversized files go
        // to the scanner whole-file, never line-mixed with the AST.
        let via_ast = bytes.len() as u64 <= MAX_AST_FILE_BYTES
            && matches!(
                ast::extract_file(&file, &text, &mut ids, &mut elements),
                ast::AstOutcome::Emitted
            );
        if !via_ast {
            for (idx, line) in text.lines().enumerate() {
                scan_line(&mut elements, &mut ids, &file, idx as u64 + 1, line, false);
            }
        }
    }

    if let Some(deps) = manifest.get("dependencies").and_then(|d| d.as_object()) {
        let mut names: Vec<&String> = deps.keys().collect();
        names.sort();
        for name in names {
            if KNOWN_NATIVE_MODULES.contains(&name.as_str()) {
                let line = manifest_line(&manifest_bytes, name);
                push_native_module(&mut elements, &mut ids, "package.json", line, name);
            }
        }
    }

    let document = FairDocument {
        contract: FAIR_CONTRACT.to_string(),
        project: ProjectProvenance {
            name: discovery.name.clone(),
            frameworks: discovery.frameworks.clone(),
            entry_points: discovery.main_entry.iter().cloned().collect(),
        },
        source: SourceManifest {
            root: root.display().to_string(),
            files: manifest_files,
        },
        extractor: ExtractorIdentity {
            name: EXTRACTOR_NAME.to_string(),
            version: EXTRACTOR_VERSION.to_string(),
            id_rule: EXTRACTOR_ID_RULE.to_string(),
        },
        elements,
    };
    document
        .validate()
        .map_err(|errors| ExtractionError::Io(format!("extractor produced invalid FAIR: {errors:?}")))?;
    Ok(document)
}

/// Read discovery facts from the manifest. Detection evidence is explicit:
/// a detection is recorded only with the dependency that produced it.
fn discover(root: &Path, manifest: &serde_json::Value) -> Discovery {
    let _ = root;
    let mut frameworks = Vec::new();
    let mut check = |key: &str, framework: &str| {
        for field in ["dependencies", "devDependencies"] {
            if let Some(version) = manifest
                .get(field)
                .and_then(|d| d.get(key))
                .and_then(|v| v.as_str())
            {
                frameworks.push(FrameworkDetection {
                    framework: framework.to_string(),
                    evidence: format!("{field} {key} {version}"),
                });
                break;
            }
        }
    };
    check("electron", "electron");
    check("react", "react");
    check("react-dom", "react");
    Discovery {
        name: manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        main_entry: manifest
            .get("main")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        frameworks,
    }
}

fn is_css(rel: &Path) -> bool {
    rel.extension().and_then(|e| e.to_str()) == Some("css")
}

fn source_file(root: &Path, rel: &Path) -> Result<SourceFile, ExtractionError> {
    let bytes = std::fs::read(root.join(rel))
        .map_err(|e| ExtractionError::Io(format!("reading {:?}: {e}", rel.display())))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Ok(SourceFile {
        path: rel.display().to_string(),
        sha256: hex(&digest),
    })
}

fn hex(digest: &[u8]) -> String {
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Sorted relative paths of scanned source files, forward-slash separated.
fn sorted_source_files(root: &Path) -> Result<Vec<PathBuf>, ExtractionError> {
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ExtractionError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| ExtractionError::Io(format!("reading {}: {e}", dir.display())))?;
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if SKIPPED_DIRS.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            walk(root, &entry.path(), out)?;
        } else if SCANNED_EXTENSIONS.contains(&name.rsplit('.').next().unwrap_or("")) {
            out.push(PathBuf::from(
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap_or(entry.path().as_path())
                    .display()
                    .to_string()
                    .replace('\\', "/"),
            ));
        }
    }
    Ok(())
}

/// 1-based line of the first occurrence of a manifest key, for provenance.
fn manifest_line(bytes: &[u8], key: &str) -> u64 {
    let needle = format!("\"{key}\"");
    String::from_utf8_lossy(bytes)
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(&needle))
        .map(|(i, _)| i as u64 + 1)
        .unwrap_or(1)
}

fn extracted() -> Representability {
    Representability {
        status: RepresentabilityStatus::Extracted,
        reason: None,
        detail: None,
    }
}

fn push(
    elements: &mut Vec<FairElement>,
    ids: &mut ast::IdAlloc,
    prefix: &'static str,
    kind: ElementKind,
    file: &str,
    line: u64,
    representability: Representability,
    semantics: BTreeMap<String, SemanticsValue>,
) {
    let id = ids.next(prefix);
    elements.push(FairElement {
        id,
        kind,
        provenance: ElementProvenance {
            file: file.to_string(),
            line,
            span_start: None,
            span_end: None,
            fallback_origin: false,
        },
        representability,
        semantics,
    });
}

fn push_native_module(
    elements: &mut Vec<FairElement>,
    ids: &mut ast::IdAlloc,
    file: &str,
    line: u64,
    name: &str,
) {
    let mut semantics = BTreeMap::new();
    semantics.insert("module".into(), SemanticsValue::Str(name.into()));
    semantics.insert(
        "resolution".into(),
        SemanticsValue::Str("native_addon".into()),
    );
    push(
        elements,
        ids,
        "nat",
        ElementKind::NativeModule,
        file,
        line,
        Representability {
            status: RepresentabilityStatus::Unrepresentable,
            reason: Some("NATIVE_MODULE_OPAQUE".into()),
            detail: Some("native addon dependency cannot be transliterated; it must be replaced by a Goya capability or recorded as a migration wall".into()),
        },
        semantics,
    );
}

/// Line-oriented construct scanner (0.1.0 mechanism, retained as the declared
/// fallback for CSS, unparseable and oversized files).
fn scan_line(
    elements: &mut Vec<FairElement>,
    ids: &mut ast::IdAlloc,
    file: &str,
    line_no: u64,
    line: &str,
    css: bool,
) {
    if css {
        if line.contains('{') && !line.trim_start().starts_with("/*") {
            let mut semantics = BTreeMap::new();
            semantics.insert(
                "selector".into(),
                SemanticsValue::Str(line.trim().trim_end_matches('{').trim().into()),
            );
            push(elements, ids, "sty", ElementKind::StyleRule, file, line_no, extracted(), semantics);
        }
        return;
    }

    for (marker, side, verb) in [
        ("ipcMain.handle(", "main", "handle"),
        ("ipcMain.on(", "main", "on"),
        ("ipcRenderer.invoke(", "renderer", "invoke"),
        ("ipcRenderer.send(", "renderer", "send"),
        ("ipcRenderer.on(", "renderer", "on"),
    ] {
        if let Some(rest) = after(line, marker) {
            if let Some(channel) = quoted(rest) {
                let mut semantics = BTreeMap::new();
                semantics.insert("channel".into(), SemanticsValue::Str(channel));
                semantics.insert("side".into(), SemanticsValue::Str(side.into()));
                semantics.insert("verb".into(), SemanticsValue::Str(verb.into()));
                push(elements, ids, "ipc", ElementKind::IpcChannel, file, line_no, extracted(), semantics);
            }
        }
    }

    if let Some(rest) = after(line, "contextBridge.exposeInMainWorld(") {
        if let Some(exposed) = quoted(rest) {
            let mut semantics = BTreeMap::new();
            semantics.insert("exposed".into(), SemanticsValue::Str(exposed));
            push(elements, ids, "brg", ElementKind::PreloadBridge, file, line_no, extracted(), semantics);
        }
    }

    if line.contains("new BrowserWindow(") {
        push(elements, ids, "win", ElementKind::WindowLifecycle, file, line_no, extracted(), BTreeMap::new());
    }

    for (marker, api) in [
        ("new Notification(", "notification"),
        ("dialog.", "dialog"),
        ("shell.", "shell"),
        ("clipboard.", "clipboard"),
        ("new Tray(", "tray"),
    ] {
        if line.contains(marker) {
            let mut semantics = BTreeMap::new();
            semantics.insert("api".into(), SemanticsValue::Str(api.into()));
            push(elements, ids, "api", ElementKind::OsApiCall, file, line_no, extracted(), semantics);
        }
    }

    for marker in ["require(\"child_process\")", "from \"child_process\"", "spawn(", "exec("] {
        if line.contains(marker) {
            push(elements, ids, "proc", ElementKind::ChildProcess, file, line_no, extracted(), BTreeMap::new());
            break;
        }
    }

    if line.contains("fs.") {
        push(elements, ids, "fs", ElementKind::FilesystemOperation, file, line_no, extracted(), BTreeMap::new());
    }

    for marker in ["electron-store", "localStorage."] {
        if line.contains(marker) {
            push(elements, ids, "store", ElementKind::StateStore, file, line_no, extracted(), BTreeMap::new());
            break;
        }
    }

    for marker in ["export default function ", "export function "] {
        if let Some(rest) = after(line, marker) {
            if let Some(name) = identifier(rest) {
                let mut semantics = BTreeMap::new();
                semantics.insert("component".into(), SemanticsValue::Str(name));
                push(elements, ids, "ui", ElementKind::Component, file, line_no, extracted(), semantics);
                break;
            }
        }
    }
    for marker in ["extends React.Component", "extends Component", "extends PureComponent"] {
        if line.contains(marker) {
            let name = line
                .split("class ")
                .nth(1)
                .and_then(identifier)
                .unwrap_or_default();
            let mut semantics = BTreeMap::new();
            semantics.insert("component".into(), SemanticsValue::Str(name));
            push(elements, ids, "ui", ElementKind::Component, file, line_no, extracted(), semantics);
            break;
        }
    }

    for hook in [
        "useState(", "useEffect(", "useContext(", "useRef(",
        "useMemo(", "useCallback(", "useReducer(", "useLayoutEffect(",
    ] {
        if line.contains(hook) {
            let mut semantics = BTreeMap::new();
            semantics.insert("hook".into(), SemanticsValue::Str(hook.trim_end_matches('(').into()));
            push(elements, ids, "hook", ElementKind::Hook, file, line_no, extracted(), semantics);
        }
    }

    for prop in [
        "onClick", "onChange", "onSubmit", "onKeyDown", "onKeyUp",
        "onDoubleClick", "onBlur", "onFocus", "onInput", "onScroll",
    ] {
        if line.contains(&format!("{prop}=")) {
            let mut semantics = BTreeMap::new();
            semantics.insert("prop".into(), SemanticsValue::Str(prop.into()));
            push(elements, ids, "evt", ElementKind::EventHandler, file, line_no, extracted(), semantics);
        }
    }

    if line.contains("styled.") || line.contains("styled(") {
        let mut semantics = BTreeMap::new();
        semantics.insert(
            "mechanism".into(),
            SemanticsValue::Str("styled-components".into()),
        );
        push(elements, ids, "sty", ElementKind::StyleRule, file, line_no, extracted(), semantics);
    }

    let dynamic = after(line, "require(")
        .map(|rest| !rest.starts_with('"') && !rest.starts_with('\''))
        .unwrap_or(false)
        || line.contains("import(");
    if dynamic {
        let mut semantics = BTreeMap::new();
        semantics.insert("resolution".into(), SemanticsValue::Str("unresolved".into()));
        push(
            elements,
            ids,
            "node",
            ElementKind::DynamicModuleResolution,
            file,
            line_no,
            Representability {
                status: RepresentabilityStatus::Undetermined,
                reason: Some("DYNAMIC_REQUIRE_UNRESOLVED".into()),
                detail: Some("require/import destination is computed; not statically resolvable by this extractor".into()),
            },
            semantics,
        );
    }

    for spelling in ["eval(", "new Function("] {
        if line.contains(spelling) {
            let mut semantics = BTreeMap::new();
            semantics.insert(
                "observed".into(),
                SemanticsValue::Str(spelling.trim_end_matches('(').into()),
            );
            push(
                elements,
                ids,
                "unk",
                ElementKind::Unknown,
                file,
                line_no,
                Representability {
                    status: RepresentabilityStatus::Undetermined,
                    reason: Some("DYNAMIC_CODE_EVALUATION".into()),
                    detail: Some("dynamic code evaluation has no statically extractable semantics".into()),
                },
                semantics,
            );
        }
    }

    if let Some(rest) = after(line, "require(\"") {
        if let Some(module) = quoted(rest) {
            if module.ends_with(".node") {
                push_native_module(elements, ids, file, line_no, &module);
            }
        }
    }
}

/// Text after the first occurrence of `marker`, if present.
fn after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.split_once(marker).map(|(_, rest)| rest)
}

/// First quoted string in `s`.
fn quoted(s: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(start) = s.find(quote) {
            if let Some(len) = s[start + 1..].find(quote) {
                return Some(s[start + 1..start + 1 + len].to_string());
            }
        }
    }
    None
}

/// Leading ASCII identifier in `s`, if any.
fn identifier(s: &str) -> Option<String> {
    let name: String = s
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    if name.is_empty() || name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_fair::ElementKind;
    use RepresentabilityStatus::*;

    fn fixture() -> FairDocument {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample-electron-app");
        extract_app(&root).expect("fixture extracts")
    }

    fn count_by_kind(doc: &FairDocument, kind: ElementKind) -> usize {
        doc.elements.iter().filter(|e| e.kind == kind).count()
    }

    fn count_by_status(doc: &FairDocument, status: RepresentabilityStatus) -> usize {
        doc.elements
            .iter()
            .filter(|e| e.representability.status == status)
            .count()
    }

    fn has_reason(doc: &FairDocument, prefix: &str, reason: &str) -> bool {
        doc.elements.iter().any(|e| {
            e.id.starts_with(prefix)
                && e.representability.reason.as_deref() == Some(reason)
        })
    }

    #[test]
    fn fixture_extracts_and_validates() {
        let doc = fixture();
        assert_eq!(doc.validate(), Ok(()));
        assert_eq!(doc.project.name, "sample-electron-app");
        assert!(doc
            .project
            .frameworks
            .iter()
            .any(|f| f.framework == "electron"));
        assert!(doc.project.frameworks.iter().any(|f| f.framework == "react"));
        assert_eq!(doc.extractor.version, "0.2.0");
    }

    #[test]
    fn ipc_channels_recorded_both_sides() {
        let doc = fixture();
        let ipc = count_by_kind(&doc, ElementKind::IpcChannel);
        assert!(ipc >= 5, "expected main, renderer and computed IPC elements, got {ipc}");
        assert!(doc.elements.iter().any(|e| e.kind == ElementKind::IpcChannel
            && e.semantics.get("side") == Some(&SemanticsValue::Str("main".into()))));
        assert!(doc.elements.iter().any(|e| e.kind == ElementKind::IpcChannel
            && e.semantics.get("side") == Some(&SemanticsValue::Str("renderer".into()))));
    }

    #[test]
    fn known_constructs_are_all_present() {
        let doc = fixture();
        assert!(count_by_kind(&doc, ElementKind::WindowLifecycle) >= 1);
        assert!(count_by_kind(&doc, ElementKind::PreloadBridge) >= 2);
        assert!(count_by_kind(&doc, ElementKind::OsApiCall) >= 1);
        assert!(count_by_kind(&doc, ElementKind::Component) >= 1);
        assert!(count_by_kind(&doc, ElementKind::Hook) >= 2);
        assert!(count_by_kind(&doc, ElementKind::EventHandler) >= 1);
        assert!(count_by_kind(&doc, ElementKind::StyleRule) >= 2);
        assert!(count_by_kind(&doc, ElementKind::FilesystemOperation) >= 1);
        assert!(count_by_kind(&doc, ElementKind::ChildProcess) >= 1);
    }

    #[test]
    fn dynamic_require_cases_are_all_represented() {
        let doc = fixture();
        // Line 19: member on file-local const object -> resolved.
        let resolved = doc.elements.iter().find(|e| e.kind == ElementKind::DynamicModuleResolution
            && e.provenance.line == 19
            && e.provenance.file == "src/main/index.js")
            .expect("line 19 require must be recorded");
        assert_eq!(resolved.representability.status, Extracted);
        assert_eq!(
            resolved.semantics.get("resolution"),
            Some(&SemanticsValue::Str("resolved".into()))
        );
        // Line 22: binds to a call result -> REQUIRE_SCOPE_UNRESOLVED.
        let unresolved = doc.elements.iter().find(|e| e.kind == ElementKind::DynamicModuleResolution
            && e.provenance.line == 22
            && e.provenance.file == "src/main/index.js")
            .expect("line 22 require must be recorded");
        assert_eq!(unresolved.representability.status, Undetermined);
        assert_eq!(
            unresolved.representability.reason.as_deref(),
            Some("REQUIRE_SCOPE_UNRESOLVED")
        );
        // eval stays unknown + observed.
        assert!(has_reason(&doc, "unk-", "DYNAMIC_CODE_EVALUATION"));
        // Undetermined population: scope-unresolved require (line 22) + eval
        // + the two unresolved computed channels (template, call-form).
        assert_eq!(count_by_status(&doc, Undetermined), 4);
    }

    #[test]
    fn computed_channels_resolve_and_preserve() {
        let doc = fixture();
        // channels.read / channels.write resolve via the file-local const map.
        let resolved: Vec<_> = doc
            .elements
            .iter()
            .filter(|e| e.kind == ElementKind::IpcChannel
                && e.semantics.get("channel-source")
                    == Some(&SemanticsValue::Str("file-local-const".into())))
            .collect();
        assert_eq!(resolved.len(), 2, "channels.read and channels.write must resolve");
        assert!(resolved.iter().all(|e| e.representability.status == Extracted));
        // Template and call-form channels are preserved as unresolved.
        let unresolved: Vec<_> = doc
            .elements
            .iter()
            .filter(|e| e.kind == ElementKind::IpcChannel
                && e.representability.reason.as_deref() == Some("COMPUTED_CHANNEL_UNRESOLVED"))
            .collect();
        assert_eq!(unresolved.len(), 2, "template and call-form channels must be preserved");
        assert!(unresolved
            .iter()
            .all(|e| e.semantics.contains_key("channel-observed")));
    }

    #[test]
    fn hook_dataflow_facets_are_recorded() {
        let doc = fixture();
        let state = doc
            .elements
            .iter()
            .find(|e| e.kind == ElementKind::Hook
                && e.semantics.get("hook") == Some(&SemanticsValue::Str("useState".into())))
            .expect("useState must be recorded");
        assert_eq!(state.semantics.get("state-key"), Some(&SemanticsValue::Str("count,setCount".into())));
        assert_eq!(state.semantics.get("state-initial"), Some(&SemanticsValue::Str("0".into())));
        let effect = doc
            .elements
            .iter()
            .find(|e| e.kind == ElementKind::Hook
                && e.semantics.get("hook") == Some(&SemanticsValue::Str("useEffect".into())))
            .expect("useEffect must be recorded");
        assert_eq!(effect.semantics.get("deps"), Some(&SemanticsValue::List(vec![])));
        assert_eq!(
            effect.semantics.get("effect-class"),
            Some(&SemanticsValue::Str("unclassifiable".into()))
        );
        assert_eq!(effect.semantics.get("cleanup"), Some(&SemanticsValue::Bool(false)));
    }

    #[test]
    fn unparseable_file_falls_back_to_scanner() {
        let doc = fixture();
        let fallback: Vec<_> = doc
            .elements
            .iter()
            .filter(|e| e.provenance.file == "src/main/broken-preload.js")
            .collect();
        assert!(
            fallback.iter().any(|e| e.kind == ElementKind::PreloadBridge),
            "broken file's contextBridge must still be recorded"
        );
        assert!(fallback
            .iter()
            .any(|e| e.kind == ElementKind::IpcChannel), "broken file's IPC calls must still be recorded");
    }

    #[test]
    fn dynamic_code_evaluation_records_observed_spelling() {
        let doc = fixture();
        let unk = doc
            .elements
            .iter()
            .find(|e| e.kind == ElementKind::Unknown)
            .expect("eval must be recorded");
        assert_eq!(
            unk.semantics.get("observed"),
            Some(&SemanticsValue::Str("eval".into()))
        );
    }

    #[test]
    fn native_dependency_becomes_unrepresentable() {
        let doc = fixture();
        assert!(has_reason(&doc, "nat-", "NATIVE_MODULE_OPAQUE"));
        assert_eq!(count_by_status(&doc, Unrepresentable), 1);
    }

    #[test]
    fn extraction_is_deterministic() {
        let a = fixture();
        let b = fixture();
        assert_eq!(a.canonical_json(), b.canonical_json());
    }

    #[test]
    fn ast_output_is_superset_of_the_0_1_0_snapshot() {
        let snapshot_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/expected-0.1.0-elements.json");
        let snapshot: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&snapshot_path).expect("snapshot present"))
                .expect("snapshot parses");
        assert_eq!(snapshot["extractor_version"], "0.1.0");
        let doc = fixture();
        // Loose per-kind keys: dynamic-resolution and unknown constructs are
        // allowed to enrich semantics/status at the same (kind, file, line).
        let strict_kinds = [
            ElementKind::IpcChannel,
            ElementKind::PreloadBridge,
            ElementKind::OsApiCall,
            ElementKind::WindowLifecycle,
            ElementKind::FilesystemOperation,
            ElementKind::ChildProcess,
            ElementKind::StateStore,
            ElementKind::Component,
            ElementKind::Hook,
            ElementKind::EventHandler,
            ElementKind::StyleRule,
        ];
        for expected in snapshot["elements"].as_array().expect("array") {
            let kind = serde_json::from_str::<ElementKind>(
                &format!("{:?}", "").replace("", ""),
            )
            .or_else(|_| {
                serde_json::from_value::<ElementKind>(expected["kind"].clone())
            })
            .expect("kind parses");
            let file = expected["provenance"]["file"].as_str().unwrap();
            let line = expected["provenance"]["line"].as_u64().unwrap();
            let loose = !strict_kinds.contains(&kind);
            let matched = doc.elements.iter().any(|e| {
                e.kind == kind
                    && e.provenance.file == file
                    && e.provenance.line == line
                    && (loose || {
                        // Strict kinds must also agree on identifying facets.
                        expected["semantics"].as_object().expect("semantics").iter().all(|(key, value)| {
                            e.semantics.get(key)
                                == Some(&serde_json::from_value::<SemanticsValue>(value.clone()).expect("value"))
                        })
                    })
            });
            assert!(
                matched,
                "0.2.0 output must cover snapshot element {kind:?} at {file}:{line}"
            );
        }
    }

    #[test]
    fn missing_manifest_is_rejected() {
        let tmp = std::env::temp_dir().join(format!("goya-disparate-empty-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let err = extract_app(&tmp).expect_err("empty dir must fail");
        std::fs::remove_dir(&tmp).ok();
        assert!(matches!(err, ExtractionError::MissingManifest(_)));
    }

    #[test]
    fn all_non_extracted_elements_carry_reason_codes() {
        let doc = fixture();
        for element in &doc.elements {
            if element.representability.status != Extracted {
                assert!(
                    element.representability.reason.is_some(),
                    "element {} lacks a reason code",
                    element.id
                );
            }
        }
    }
}
