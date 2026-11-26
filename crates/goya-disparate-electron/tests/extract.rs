use goya_disparate_electron::material::{self, Budgets, Inventory, Material};
use goya_disparate_electron::{extract_inventory, extract_path, ExtractError};
use goya_fair::{is_registered, Basis, Relation, FAIR_CONTRACT};
use goya_fair::{ElementKind, FairDocument, FairElement, RepresentabilityStatus, SemanticsValue};
use std::path::{Path, PathBuf};

const LABEL: &str = "fixtures/disparates/electron-counter";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/disparates/electron-counter")
}

fn counter() -> FairDocument {
    extract_path(&fixture(), LABEL, &Budgets::default()).expect("fixture extracts")
}

fn str_of<'d>(e: &'d FairElement, key: &str) -> Option<&'d str> {
    match e.semantics.get(key) {
        Some(SemanticsValue::Str(v)) => Some(v),
        _ => None,
    }
}

fn find<'d>(doc: &'d FairDocument, kind: ElementKind, pairs: &[(&str, &str)]) -> &'d FairElement {
    doc.elements
        .iter()
        .find(|e| e.kind == kind && pairs.iter().all(|(k, v)| str_of(e, k) == Some(*v)))
        .unwrap_or_else(|| panic!("no {kind:?} with {pairs:?}"))
}

fn inline(files: &[(&str, &str)]) -> FairDocument {
    let inventory = Inventory {
        files: files
            .iter()
            .map(|(path, text)| Material {
                path: path.to_string(),
                bytes: text.as_bytes().to_vec(),
            })
            .collect(),
        skipped: Vec::new(),
    };
    extract_inventory(inventory, "inline").expect("inline extracts")
}

fn reasons(doc: &FairDocument) -> Vec<&str> {
    doc.elements
        .iter()
        .filter_map(|e| e.representability.reason.as_deref())
        .collect()
}

// ---- the DEL-B01 fixture --------------------------------------------------

#[test]
fn counter_matches_golden() {
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/electron-counter.fair.json");
    let actual = counter().canonical_json();
    if std::env::var_os("GOLDEN_UPDATE").is_some() {
        std::fs::write(&golden, &actual).expect("write golden");
    }
    let expected =
        std::fs::read_to_string(&golden).expect("golden present (run with GOLDEN_UPDATE=1)");
    assert_eq!(
        actual, expected,
        "canonical FAIR drifted from the golden file"
    );
}

#[test]
fn counter_is_deterministic() {
    assert_eq!(counter().canonical_json(), counter().canonical_json());
}

#[test]
fn counter_meets_the_readme_oracle() {
    let doc = counter();
    // Topology.
    let window = find(
        &doc,
        ElementKind::WindowLifecycle,
        &[("api", "new BrowserWindow")],
    );
    assert_eq!(str_of(window, "preload_path"), Some("src/preload.js"));
    assert_eq!(str_of(window, "process"), Some("main"));
    let SemanticsValue::Map(options) = &window.semantics["options"] else {
        panic!("options")
    };
    assert_eq!(options["width"], SemanticsValue::Int(320));
    assert_eq!(options["height"], SemanticsValue::Int(160));
    assert_eq!(options["title"], SemanticsValue::Str("Count".into()));
    let SemanticsValue::Map(prefs) = &options["webPreferences"] else {
        panic!("webPreferences")
    };
    assert_eq!(prefs["contextIsolation"], SemanticsValue::Bool(true));
    assert_eq!(prefs["nodeIntegration"], SemanticsValue::Bool(false));
    assert_eq!(prefs["sandbox"], SemanticsValue::Bool(true));
    find(
        &doc,
        ElementKind::WindowLifecycle,
        &[
            ("api", "BrowserWindow(instance).loadFile"),
            ("resolved_path", "src/index.html"),
        ],
    );
    find(
        &doc,
        ElementKind::WindowLifecycle,
        &[("api", "app.whenReady")],
    );
    find(
        &doc,
        ElementKind::WindowLifecycle,
        &[("api", "app.on"), ("first_argument", "window-all-closed")],
    );
    find(&doc, ElementKind::WindowLifecycle, &[("api", "app.quit")]);

    // Bridge and IPC, both sides of both channels.
    let bridge = find(
        &doc,
        ElementKind::PreloadBridge,
        &[("key", "counter"), ("process", "preload")],
    );
    let SemanticsValue::Map(members) = &bridge.semantics["members"] else {
        panic!("members")
    };
    assert_eq!(
        members.keys().map(String::as_str).collect::<Vec<_>>(),
        ["add", "get"]
    );
    for channel in ["counter:add", "counter:get"] {
        find(
            &doc,
            ElementKind::IpcChannel,
            &[("channel", channel), ("side", "main"), ("process", "main")],
        );
        find(
            &doc,
            ElementKind::IpcChannel,
            &[
                ("channel", channel),
                ("side", "renderer"),
                ("process", "preload"),
            ],
        );
    }
    let add = find(
        &doc,
        ElementKind::IpcChannel,
        &[("channel", "counter:add"), ("side", "main")],
    );
    assert_eq!(
        add.semantics["mutates"],
        SemanticsValue::List(vec![SemanticsValue::Str("count".into())])
    );

    // State.
    let count = find(
        &doc,
        ElementKind::StateStore,
        &[("name", "count"), ("process", "main")],
    );
    assert_eq!(str_of(count, "initial"), Some("0n"));
    find(
        &doc,
        ElementKind::StateStore,
        &[("name", "pending"), ("process", "renderer")],
    );

    // Validation logic is kept as a logic unit.
    find(
        &doc,
        ElementKind::LogicUnit,
        &[("name", "parseCanonicalI64"), ("process", "main")],
    );

    // DOM handlers.
    let paste = find(&doc, ElementKind::EventHandler, &[("event", "paste")]);
    assert_eq!(
        paste.semantics["prevents_default"],
        SemanticsValue::Bool(true)
    );
    let click = find(&doc, ElementKind::EventHandler, &[("event", "click")]);
    assert_eq!(
        click.semantics["window_calls"],
        SemanticsValue::List(vec![SemanticsValue::Str("window.counter.add".into())])
    );
    find(
        &doc,
        ElementKind::BridgeCall,
        &[("bridge", "counter.get"), ("process", "renderer")],
    );

    // Components and accessibility attributes.
    let input = find(&doc, ElementKind::Component, &[("tag", "input")]);
    let SemanticsValue::Map(attrs) = &input.semantics["attributes"] else {
        panic!("attributes")
    };
    assert_eq!(attrs["id"], SemanticsValue::Str("window-amount".into()));
    assert_eq!(attrs["aria-label"], SemanticsValue::Str("Amount".into()));
    assert_eq!(attrs["value"], SemanticsValue::Str("1".into()));
    // HTML provenance is the exact start tag, which spans two lines.
    let html = std::fs::read_to_string(fixture().join("src/index.html")).unwrap();
    let p = &input.provenance;
    let tag = &html[p.span_start.unwrap() as usize..p.span_end.unwrap() as usize];
    assert!(
        tag.starts_with("<input id=\"window-amount\"") && tag.ends_with("count\">"),
        "{tag}"
    );
    assert_eq!((p.line, p.fallback_origin), (12, false));
    let output = find(
        &doc,
        ElementKind::Component,
        &[("tag", "output"), ("text", "0")],
    );
    let SemanticsValue::Map(attrs) = &output.semantics["attributes"] else {
        panic!("attributes")
    };
    assert_eq!(attrs["aria-live"], SemanticsValue::Str("polite".into()));
    find(
        &doc,
        ElementKind::Component,
        &[("tag", "button"), ("text", "Add")],
    );
    find(
        &doc,
        ElementKind::Component,
        &[("tag", "span"), ("text", "Count")],
    );

    // Layout.
    let amount_rule = find(
        &doc,
        ElementKind::StyleRule,
        &[("selector", "#window-amount"), ("process", "renderer")],
    );
    let SemanticsValue::List(decls) = &amount_rule.semantics["declarations"] else {
        panic!("declarations")
    };
    assert!(decls.iter().any(|d| matches!(d, SemanticsValue::Map(m)
        if m["name"] == SemanticsValue::Str("left".into()) && m["value"] == SemanticsValue::Str("6.25%".into()))));

    // Every element has a process role, and no file went unresolved.
    assert!(doc
        .elements
        .iter()
        .all(|e| e.semantics.contains_key("process")));
    let unresolved: Vec<_> = doc
        .elements
        .iter()
        .filter(|e| {
            str_of(e, "process") == Some("unresolved") && e.provenance.file != "package.json"
        })
        .map(|e| e.id.as_str())
        .collect();
    assert!(unresolved.is_empty(), "unresolved roles: {unresolved:?}");
    // Nothing was judged representable: that is a later stage.
    assert!(doc.elements.iter().all(|e| matches!(
        e.representability.status,
        RepresentabilityStatus::Extracted | RepresentabilityStatus::Undetermined
    )));
    for reason in reasons(&doc) {
        assert!(is_registered(reason), "{reason} missing from the register");
    }
}

#[test]
fn counter_manifest_hashes_every_file() {
    let doc = counter();
    let paths: Vec<_> = doc.source.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "README.md",
            "package.json",
            "src/index.html",
            "src/main.js",
            "src/preload.js",
            "src/renderer.js",
            "src/style.css"
        ]
    );
}

// ---- ASAR -----------------------------------------------------------------

enum Node {
    File(Vec<u8>),
    Dir(Vec<(String, Node)>),
    Raw(serde_json::Value),
}

fn asar(entries: Vec<(String, Node)>) -> Vec<u8> {
    fn header(entries: &[(String, Node)], data: &mut Vec<u8>) -> serde_json::Value {
        let mut files = serde_json::Map::new();
        for (name, node) in entries {
            let value = match node {
                Node::File(bytes) => {
                    let offset = data.len();
                    data.extend_from_slice(bytes);
                    serde_json::json!({ "offset": offset.to_string(), "size": bytes.len() })
                }
                Node::Dir(children) => serde_json::json!({ "files": header(children, data) }),
                Node::Raw(v) => v.clone(),
            };
            files.insert(name.clone(), value);
        }
        serde_json::Value::Object(files)
    }
    let mut data = Vec::new();
    let json = serde_json::json!({ "files": header(&entries, &mut data) }).to_string();
    let mut pickle = Vec::new();
    let padded = (json.len() + 3) & !3;
    pickle.extend_from_slice(&((4 + padded) as u32).to_le_bytes());
    pickle.extend_from_slice(&(json.len() as u32).to_le_bytes());
    pickle.extend_from_slice(json.as_bytes());
    pickle.resize(8 + padded, 0);
    let mut out = Vec::new();
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(pickle.len() as u32).to_le_bytes());
    out.extend_from_slice(&pickle);
    out.extend_from_slice(&data);
    out
}

fn tree(dir: &Path) -> Vec<(String, Node)> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    entries
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_str().unwrap().to_string();
            let node = if p.is_dir() {
                Node::Dir(tree(&p))
            } else {
                Node::File(std::fs::read(&p).unwrap())
            };
            (name, node)
        })
        .collect()
}

fn from_asar(bytes: &[u8]) -> Result<FairDocument, ExtractError> {
    let inventory = material::from_asar(bytes, &Budgets::default())?;
    extract_inventory(inventory, LABEL)
}

#[test]
fn asar_extraction_equals_directory_extraction() {
    let archive = asar(tree(&fixture()));
    assert_eq!(
        from_asar(&archive).expect("asar extracts").canonical_json(),
        counter().canonical_json()
    );
}

#[test]
fn asar_rejects_entry_names_that_could_escape() {
    for name in ["..", ".", "a/b", "a\\b", ""] {
        let archive = asar(vec![(name.to_string(), Node::File(b"x".to_vec()))]);
        assert!(
            matches!(from_asar(&archive), Err(ExtractError::Invalid(_))),
            "{name:?} admitted"
        );
    }
}

#[test]
fn asar_rejects_out_of_range_data_and_bad_headers() {
    let past_end = asar(vec![(
        "a.js".into(),
        Node::Raw(serde_json::json!({ "offset": "0", "size": 999 })),
    )]);
    assert!(matches!(
        from_asar(&past_end),
        Err(ExtractError::Invalid(_))
    ));
    let huge_offset = asar(vec![(
        "a.js".into(),
        Node::Raw(serde_json::json!({ "offset": "18446744073709551615", "size": 1 })),
    )]);
    assert!(matches!(
        from_asar(&huge_offset),
        Err(ExtractError::Invalid(_))
    ));
    assert!(matches!(
        from_asar(b"\x04\0\0\0\xff\xff\xff\x7f"),
        Err(ExtractError::Budget(_))
    ));
    assert!(matches!(
        from_asar(b"\x05\0\0\0\0\0\0\0"),
        Err(ExtractError::Invalid(_))
    ));
    assert!(matches!(
        from_asar(b"\x04\0"),
        Err(ExtractError::Invalid(_))
    ));
}

#[test]
fn asar_checks_integrity_records() {
    let wrong = serde_json::json!({ "offset": "0", "size": 1, "integrity": { "algorithm": "SHA256", "hash": "0".repeat(64) } });
    let mut archive = asar(vec![("a.js".into(), Node::Raw(wrong))]);
    archive.push(b'x');
    assert!(
        matches!(from_asar(&archive), Err(ExtractError::Invalid(m)) if m.contains("integrity mismatch"))
    );
}

#[test]
fn asar_links_and_unpacked_files_are_recorded_not_followed() {
    let archive = asar(vec![
        ("a.js".into(), Node::File(b"let x = 1;".to_vec())),
        (
            "addon.node".into(),
            Node::Raw(serde_json::json!({ "size": 10, "unpacked": true })),
        ),
        (
            "link".into(),
            Node::Raw(serde_json::json!({ "link": "../../etc/passwd" })),
        ),
    ]);
    let doc = from_asar(&archive).expect("extracts");
    let r = reasons(&doc);
    assert!(r.contains(&"ASAR_UNPACKED_NOT_SUPPLIED"));
    assert!(r.contains(&"SYMLINK_NOT_FOLLOWED"));
    assert_eq!(doc.source.files.len(), 1);
}

#[test]
fn budgets_are_enforced() {
    let tight = Budgets {
        max_files: 2,
        ..Budgets::default()
    };
    assert!(matches!(
        extract_path(&fixture(), LABEL, &tight),
        Err(ExtractError::Budget(_))
    ));
    let small = Budgets {
        max_file_bytes: 64,
        ..Budgets::default()
    };
    assert!(matches!(
        extract_path(&fixture(), LABEL, &small),
        Err(ExtractError::Budget(_))
    ));
    let deep = asar(vec![(
        "a".into(),
        Node::Dir(vec![(
            "b".into(),
            Node::Dir(vec![("c.js".into(), Node::File(b"1".to_vec()))]),
        )]),
    )]);
    let shallow = Budgets {
        max_depth: 1,
        ..Budgets::default()
    };
    assert!(matches!(
        material::from_asar(&deep, &shallow),
        Err(ExtractError::Budget(_))
    ));
}

#[cfg(unix)]
#[test]
fn directory_symlinks_and_dependencies_are_recorded_not_followed() {
    let root = std::env::temp_dir().join(format!("gde-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
    std::fs::write(root.join("node_modules/x/index.js"), "secret()").unwrap();
    std::fs::write(root.join("a.js"), "let a = 1;").unwrap();
    std::os::unix::fs::symlink("/etc", root.join("escape")).unwrap();
    let doc = extract_path(&root, "tmp", &Budgets::default()).expect("extracts");
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(
        doc.source
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["a.js"]
    );
    let r = reasons(&doc);
    assert!(r.contains(&"SYMLINK_NOT_FOLLOWED"));
    assert!(r.contains(&"DEPENDENCY_TREE_NOT_ANALYSED"));
}

// ---- constructs the fixture does not exercise ------------------------------

#[test]
fn unresolvable_constructs_are_kept_as_undetermined() {
    let doc = inline(&[
        (
            "main.js",
            "const { ipcMain, BrowserView } = require('electron');\n\
         const name = 'x';\n\
         require(name);\n\
         import('./' + name);\n\
         ipcMain.on(name, () => {});\n\
         new BrowserView();\n\
         ipcMain.emit('x');\n\
         const addon = require('./build/addon.node');\n",
        ),
        ("broken.js", "this is not javascript\n"),
    ]);
    let r = reasons(&doc);
    assert_eq!(
        r.iter()
            .filter(|c| **c == "DYNAMIC_REQUIRE_UNRESOLVED")
            .count(),
        2
    );
    assert!(r.contains(&"IPC_CHANNEL_COMPUTED"));
    assert!(r.contains(&"ELECTRON_API_UNCLASSIFIED"));
    // A fatal syntax error yields one element for the file and nothing invented.
    let broken: Vec<_> = doc
        .elements
        .iter()
        .filter(|e| e.provenance.file == "broken.js")
        .collect();
    assert_eq!(broken.len(), 1);
    assert_eq!(
        broken[0].representability.reason.as_deref(),
        Some("PARSE_FAILED")
    );
    find(
        &doc,
        ElementKind::NativeModule,
        &[("module", "./build/addon.node")],
    );
    for reason in r {
        assert!(is_registered(reason), "{reason} missing from the register");
    }
}

#[test]
fn esm_imports_os_apis_files_and_storage_are_classified() {
    let doc = inline(&[(
        "main.mjs",
        "import { dialog, shell } from 'electron';\n\
         import * as fs from 'node:fs';\n\
         import { exec } from 'child_process';\n\
         dialog.showOpenDialog({});\n\
         shell.openExternal('https://example.org');\n\
         fs.writeFileSync('out.txt', 'x');\n\
         exec('ls');\n\
         localStorage.setItem('k', 'v');\n",
    )]);
    find(
        &doc,
        ElementKind::OsApiCall,
        &[("api", "dialog.showOpenDialog")],
    );
    find(
        &doc,
        ElementKind::OsApiCall,
        &[("api", "shell.openExternal")],
    );
    find(
        &doc,
        ElementKind::FilesystemOperation,
        &[("api", "fs.writeFileSync"), ("first_argument", "out.txt")],
    );
    find(
        &doc,
        ElementKind::ChildProcess,
        &[("api", "child_process.exec")],
    );
    find(
        &doc,
        ElementKind::StateStore,
        &[("api", "localStorage.setItem"), ("first_argument", "k")],
    );
    find(&doc, ElementKind::ModuleImport, &[("module", "fs")]);
}

#[test]
fn property_handlers_and_inline_html_are_recorded() {
    let doc = inline(&[
        (
            "r.js",
            "const b = document.querySelector('#go');\nb.onclick = () => window.api.go();\n",
        ),
        (
            "p.html",
            "<button id=go onclick=\"go()\">Go</button><script>inline()</script><style>p{}</style>",
        ),
        (
            "s.css",
            "@media (min-width: 10px) { p { color: red } }\np { color: blue; color: red }\n",
        ),
    ]);
    let handler = find(
        &doc,
        ElementKind::EventHandler,
        &[("event", "click"), ("via", "property")],
    );
    assert_eq!(
        handler.semantics["target"],
        SemanticsValue::Map([("dom_id".to_string(), SemanticsValue::Str("go".into()))].into())
    );
    let r = reasons(&doc);
    for code in [
        "INLINE_HANDLER_NOT_ANALYSED",
        "INLINE_SCRIPT_NOT_ANALYSED",
        "INLINE_STYLE_NOT_ANALYSED",
        "CSS_AT_RULE_NOT_ANALYSED",
    ] {
        assert!(r.contains(&code), "{code} not emitted");
    }
    // Duplicate declarations are kept in order.
    let rule = find(&doc, ElementKind::StyleRule, &[("selector", "p")]);
    let SemanticsValue::List(decls) = &rule.semantics["declarations"] else {
        panic!()
    };
    assert_eq!(decls.len(), 2);
    // Without package.json no role can be assigned.
    assert!(doc
        .elements
        .iter()
        .all(|e| str_of(e, "process") == Some("unresolved")));
}

#[test]
fn counter_is_fair2_with_everything_classified() {
    let doc = counter();
    assert_eq!(doc.contract, FAIR_CONTRACT);
    let unclassified: Vec<_> = doc
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Unknown || e.representability.reason.is_some())
        .map(|e| e.id.as_str())
        .collect();
    assert!(unclassified.is_empty(), "{unclassified:?}");
}

fn related(
    doc: &FairDocument,
    subject: &FairElement,
    relation: Relation,
    object: &FairElement,
) -> Basis {
    doc.relationships
        .iter()
        .find(|r| r.subject == subject.id && r.relation == relation && r.object == object.id)
        .unwrap_or_else(|| panic!("no {relation:?} from {} to {}", subject.id, object.id))
        .basis
}

#[test]
fn counter_relationships_trace_add_from_click_to_state() {
    let doc = counter();
    let click = find(&doc, ElementKind::EventHandler, &[("event", "click")]);
    let paste = find(&doc, ElementKind::EventHandler, &[("event", "paste")]);
    let button = find(&doc, ElementKind::Component, &[("tag", "button")]);
    let input = find(&doc, ElementKind::Component, &[("tag", "input")]);
    let bridge = find(&doc, ElementKind::PreloadBridge, &[("key", "counter")]);
    let add_r = find(
        &doc,
        ElementKind::IpcChannel,
        &[("channel", "counter:add"), ("side", "renderer")],
    );
    let add_m = find(
        &doc,
        ElementKind::IpcChannel,
        &[("channel", "counter:add"), ("side", "main")],
    );
    let get_r = find(
        &doc,
        ElementKind::IpcChannel,
        &[("channel", "counter:get"), ("side", "renderer")],
    );
    let get_m = find(
        &doc,
        ElementKind::IpcChannel,
        &[("channel", "counter:get"), ("side", "main")],
    );
    let count = find(&doc, ElementKind::StateStore, &[("name", "count")]);
    let startup = find(&doc, ElementKind::BridgeCall, &[("bridge", "counter.get")]);

    assert_eq!(
        related(&doc, click, Relation::Handles, button),
        Basis::Derived
    );
    assert_eq!(
        related(&doc, paste, Relation::Handles, input),
        Basis::Derived
    );
    assert_eq!(
        related(&doc, click, Relation::Invokes, bridge),
        Basis::Derived
    );
    assert_eq!(
        related(&doc, bridge, Relation::Exposes, add_r),
        Basis::Extracted
    );
    assert_eq!(
        related(&doc, bridge, Relation::Exposes, get_r),
        Basis::Extracted
    );
    assert_eq!(
        related(&doc, add_r, Relation::Invokes, add_m),
        Basis::Derived
    );
    assert_eq!(
        related(&doc, get_r, Relation::Invokes, get_m),
        Basis::Derived
    );
    assert_eq!(
        related(&doc, add_m, Relation::Mutates, count),
        Basis::Derived
    );
    assert_eq!(related(&doc, add_m, Relation::Reads, count), Basis::Derived);
    assert_eq!(related(&doc, get_m, Relation::Reads, count), Basis::Derived);
    assert_eq!(
        related(&doc, startup, Relation::Invokes, bridge),
        Basis::Derived
    );
    // counter:get only reads.
    assert!(!doc
        .relationships
        .iter()
        .any(|r| r.subject == get_m.id && r.relation == Relation::Mutates));

    let window = find(
        &doc,
        ElementKind::WindowLifecycle,
        &[("api", "BrowserWindow(instance).loadFile")],
    );
    let html = find(&doc, ElementKind::Component, &[("tag", "html")]);
    assert_eq!(related(&doc, window, Relation::Loads, html), Basis::Derived);
    let rule = find(
        &doc,
        ElementKind::StyleRule,
        &[("selector", "#window-amount")],
    );
    assert_eq!(related(&doc, rule, Relation::Styles, input), Basis::Derived);
    let main = find(&doc, ElementKind::Component, &[("tag", "main")]);
    assert_eq!(
        related(&doc, main, Relation::Contains, input),
        Basis::Extracted
    );
    // `html, body` is not a simple ID selector, so it styles nothing.
    let compound = find(&doc, ElementKind::StyleRule, &[("selector", "html, body")]);
    assert!(!doc.relationships.iter().any(|r| r.subject == compound.id));
    // Derived relationships name both ends as evidence.
    for r in doc
        .relationships
        .iter()
        .filter(|r| r.basis == Basis::Derived)
    {
        assert_eq!(r.evidence, vec![r.subject.clone(), r.object.clone()]);
    }
}

#[test]
fn counter_project_record() {
    let doc = counter();
    let entries: Vec<(&str, &str)> = doc
        .project
        .entry_points
        .iter()
        .map(|e| (e.path.as_str(), e.role.as_str()))
        .collect();
    assert_eq!(
        entries,
        [
            ("src/index.html", "renderer"),
            ("src/main.js", "main"),
            ("src/preload.js", "preload"),
            ("src/renderer.js", "renderer"),
            ("src/style.css", "renderer"),
        ]
    );
    let manifest = find(
        &doc,
        ElementKind::PackageManifest,
        &[("main", "src/main.js")],
    );
    assert_eq!(
        doc.project.entry_points[1].evidence,
        vec![manifest.id.clone()]
    );
    let detections = &doc.project.framework_detections;
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].framework, "electron");
    assert_eq!(detections[0].version.as_deref(), Some("33.2.1"));
    assert_eq!(detections[0].evidence.len(), 3);
}

#[test]
fn ambiguous_matches_produce_no_relationship() {
    let doc = inline(&[
        ("a.html", "<p id=x></p><p id=x></p>"),
        (
            "r.js",
            "const p = document.getElementById('x');\np.addEventListener('click', () => {});\n",
        ),
        ("s.css", "#x { color: red }\n"),
    ]);
    find(&doc, ElementKind::EventHandler, &[("event", "click")]);
    assert!(
        !doc.relationships
            .iter()
            .any(|r| matches!(r.relation, Relation::Handles | Relation::Styles)),
        "{:?}",
        doc.relationships
    );
}
