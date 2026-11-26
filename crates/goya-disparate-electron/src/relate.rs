//! FAIR/2 structure over the extracted elements: process topology (entry
//! points), typed relationships, and framework detection.
//!
//! A relationship is `extracted` when one source site shows it (markup
//! nesting, an IPC call inside a bridge member). It is `derived` when it joins
//! two sites by a literal value (a channel name, a bridge key, a DOM ID, a
//! binding name), and it then names both elements as evidence. Matches that
//! are not unique produce no relationship.

use crate::html::HtmlOutput;
use crate::js;
use goya_fair::{
    Basis, ElementKind, ElementProvenance, FairElement, FairRelationship, FrameworkDetection,
    Relation, SemanticsValue,
};
use std::collections::BTreeMap;

fn str_of<'e>(e: &'e FairElement, key: &str) -> Option<&'e str> {
    match e.semantics.get(key) {
        Some(SemanticsValue::Str(v)) => Some(v),
        _ => None,
    }
}

fn strs_of<'e>(e: &'e FairElement, key: &str) -> Vec<&'e str> {
    match e.semantics.get(key) {
        Some(SemanticsValue::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                SemanticsValue::Str(s) => Some(s.as_str()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn map_of<'e>(e: &'e FairElement, key: &str) -> Option<&'e BTreeMap<String, SemanticsValue>> {
    match e.semantics.get(key) {
        Some(SemanticsValue::Map(m)) => Some(m),
        _ => None,
    }
}

fn html_id(e: &FairElement) -> Option<&str> {
    match map_of(e, "attributes")?.get("id") {
        Some(SemanticsValue::Str(v)) => Some(v),
        _ => None,
    }
}

/// One entry point: (path, role, evidence element ID).
pub type Entry = (String, &'static str, String);

/// Entry points. `main` comes from `package.json`; `preload` and the renderer
/// page from that file's windows; renderer scripts and styles from that page.
pub fn topology(
    main: Option<(String, String)>,
    elements: &[FairElement],
    html_out: &BTreeMap<String, HtmlOutput>,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    let Some((main, manifest_id)) = main else {
        return entries;
    };
    entries.push((main.clone(), "main", manifest_id));
    let mut pages = Vec::new();
    for e in elements.iter().filter(|e| e.provenance.file == main) {
        if let Some(p) = str_of(e, "preload_path") {
            entries.push((p.to_string(), "preload", e.id.clone()));
        }
        if let Some(p) = str_of(e, "resolved_path") {
            entries.push((p.to_string(), "renderer", e.id.clone()));
            pages.push(p.to_string());
        }
    }
    for page in pages {
        let Some(out) = html_out.get(&page) else {
            continue;
        };
        let dir = js::parent_dir(&page);
        for (reference, id) in out.scripts.iter().chain(&out.stylesheets) {
            if reference.contains(':') || reference.starts_with('/') {
                continue;
            }
            let joined = if dir.is_empty() {
                reference.clone()
            } else {
                format!("{dir}/{reference}")
            };
            if let Some(p) = js::normalise(&joined) {
                entries.push((p, "renderer", id.clone()));
            }
        }
    }
    entries
}

struct Builder {
    out: BTreeMap<String, FairRelationship>,
}

impl Builder {
    fn add(
        &mut self,
        subject: &FairElement,
        relation: Relation,
        object: &FairElement,
        basis: Basis,
        provenance: &ElementProvenance,
    ) {
        let tag = serde_json::to_value(relation)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        let id = format!("{tag}:{}->{}", subject.id, object.id);
        let evidence = match basis {
            Basis::Derived => vec![subject.id.clone(), object.id.clone()],
            _ => Vec::new(),
        };
        self.out.entry(id.clone()).or_insert(FairRelationship {
            id,
            subject: subject.id.clone(),
            relation,
            object: object.id.clone(),
            basis,
            evidence,
            provenance: provenance.clone(),
        });
    }
}

/// The one element matching `pred`, or none when zero or several match.
fn unique(elements: &[FairElement], pred: impl Fn(&FairElement) -> bool) -> Option<&FairElement> {
    let mut found = elements.iter().filter(|e| pred(e));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

fn within(inner: &ElementProvenance, outer: &ElementProvenance) -> bool {
    matches!(
        (inner.span_start, inner.span_end, outer.span_start, outer.span_end),
        (Some(a), Some(b), Some(c), Some(d)) if inner.file == outer.file && c <= a && b <= d
    )
}

const RENDERER_SENDS: &[&str] = &["send", "sendSync", "postMessage"];
const MAIN_LISTENS: &[&str] = &["on", "once"];

pub fn relationships(elements: &[FairElement]) -> Vec<FairRelationship> {
    let mut b = Builder {
        out: BTreeMap::new(),
    };
    let by_id: BTreeMap<&str, &FairElement> = elements.iter().map(|e| (e.id.as_str(), e)).collect();
    let of_kind = |kind: ElementKind| elements.iter().filter(move |e| e.kind == kind);

    // contains: markup nesting.
    for child in of_kind(ElementKind::Component) {
        if let Some(parent) = str_of(child, "parent").and_then(|p| by_id.get(p)) {
            b.add(
                parent,
                Relation::Contains,
                child,
                Basis::Extracted,
                &child.provenance,
            );
        }
    }

    // mutates / reads: handlers and IPC handlers on module-scope state, by name.
    for e in elements
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::IpcChannel | ElementKind::EventHandler))
    {
        for (key, relation) in [("mutates", Relation::Mutates), ("reads", Relation::Reads)] {
            for name in strs_of(e, key) {
                if let Some(state) = unique(elements, |s| {
                    s.kind == ElementKind::StateStore
                        && s.provenance.file == e.provenance.file
                        && str_of(s, "name") == Some(name)
                }) {
                    b.add(e, relation, state, Basis::Derived, &e.provenance);
                }
            }
        }
    }

    // exposes: a bridge member that calls ipcRenderer inside the bridge object.
    for bridge in of_kind(ElementKind::PreloadBridge) {
        let Some(members) = map_of(bridge, "members") else {
            continue;
        };
        let mut calls: Vec<String> = Vec::new();
        for member in members.values() {
            if let SemanticsValue::Map(m) = member {
                if let Some(SemanticsValue::List(ipc)) = m.get("ipc") {
                    calls.extend(ipc.iter().filter_map(|v| match v {
                        SemanticsValue::Str(s) => Some(s.clone()),
                        _ => None,
                    }));
                }
            }
        }
        for ipc in of_kind(ElementKind::IpcChannel) {
            let (Some(method), Some(channel)) = (str_of(ipc, "method"), str_of(ipc, "channel"))
            else {
                continue;
            };
            if str_of(ipc, "side") == Some("renderer")
                && within(&ipc.provenance, &bridge.provenance)
                && calls.contains(&format!("{method} {channel}"))
            {
                b.add(
                    bridge,
                    Relation::Exposes,
                    ipc,
                    Basis::Extracted,
                    &ipc.provenance,
                );
            }
        }
    }

    // invokes: IPC sender to receiver, by literal channel.
    let ipc: Vec<&FairElement> = of_kind(ElementKind::IpcChannel).collect();
    for from in &ipc {
        let (Some(side), Some(method), Some(channel)) = (
            str_of(from, "side"),
            str_of(from, "method"),
            str_of(from, "channel"),
        ) else {
            continue;
        };
        let (to_side, to_methods): (&str, &[&str]) = match (side, method) {
            ("renderer", "invoke") => ("main", &["handle", "handleOnce"]),
            ("renderer", m) if RENDERER_SENDS.contains(&m) => ("main", MAIN_LISTENS),
            ("main", "webContents.send") => ("renderer", &["on", "once"]),
            _ => continue,
        };
        for to in &ipc {
            if str_of(to, "side") == Some(to_side)
                && str_of(to, "channel") == Some(channel)
                && str_of(to, "method").is_some_and(|m| to_methods.contains(&m))
            {
                b.add(
                    from,
                    Relation::Invokes,
                    to,
                    Basis::Derived,
                    &from.provenance,
                );
            }
        }
    }

    // invokes: renderer code calling through a bridge, by key and member.
    let bridge_for = |key: &str, member: &str| {
        unique(elements, |e| {
            e.kind == ElementKind::PreloadBridge
                && str_of(e, "key") == Some(key)
                && map_of(e, "members").is_some_and(|m| m.contains_key(member))
        })
    };
    for e in elements {
        let calls: Vec<String> = match e.kind {
            ElementKind::EventHandler => strs_of(e, "window_calls")
                .into_iter()
                .filter_map(|c| c.strip_prefix("window.").map(str::to_string))
                .collect(),
            ElementKind::BridgeCall => str_of(e, "bridge")
                .map(str::to_string)
                .into_iter()
                .collect(),
            _ => continue,
        };
        for call in calls {
            if let Some((key, member)) = call.split_once('.') {
                if let Some(bridge) = bridge_for(key, member) {
                    b.add(e, Relation::Invokes, bridge, Basis::Derived, &e.provenance);
                }
            }
        }
    }

    // handles / styles: by DOM ID.
    let component_with_id = |id: &str| {
        unique(elements, |c| {
            c.kind == ElementKind::Component && html_id(c) == Some(id)
        })
    };
    for h in of_kind(ElementKind::EventHandler) {
        let target = map_of(h, "target").and_then(|t| match t.get("dom_id") {
            Some(SemanticsValue::Str(v)) => Some(v.as_str()),
            _ => None,
        });
        if let Some(c) = target.and_then(component_with_id) {
            b.add(h, Relation::Handles, c, Basis::Derived, &h.provenance);
        }
    }
    for rule in of_kind(ElementKind::StyleRule) {
        let selector = str_of(rule, "selector").unwrap_or("");
        let simple = selector.strip_prefix('#').filter(|id| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        });
        if let Some(c) = simple.and_then(component_with_id) {
            b.add(rule, Relation::Styles, c, Basis::Derived, &rule.provenance);
        }
    }

    // loads: a window's loadFile to the page's first root element.
    for w in of_kind(ElementKind::WindowLifecycle) {
        let Some(page) = str_of(w, "resolved_path") else {
            continue;
        };
        let root = elements
            .iter()
            .filter(|c| {
                c.kind == ElementKind::Component
                    && c.provenance.file == page
                    && !c.semantics.contains_key("parent")
            })
            .min_by(|a, b| a.id.cmp(&b.id));
        if let Some(root) = root {
            b.add(w, Relation::Loads, root, Basis::Derived, &w.provenance);
        }
    }

    b.out.into_values().collect()
}

/// Electron, from the package manifest's dependencies and `electron` imports.
pub fn frameworks(elements: &[FairElement]) -> Vec<FrameworkDetection> {
    let mut evidence = Vec::new();
    let mut version = None;
    for e in elements {
        match e.kind {
            ElementKind::PackageManifest => {
                for key in ["dependencies", "devDependencies", "optionalDependencies"] {
                    if let Some(SemanticsValue::Str(v)) =
                        map_of(e, key).and_then(|m| m.get("electron"))
                    {
                        version.get_or_insert_with(|| v.clone());
                        if !evidence.contains(&e.id) {
                            evidence.push(e.id.clone());
                        }
                    }
                }
            }
            ElementKind::ModuleImport if str_of(e, "module") == Some("electron") => {
                evidence.push(e.id.clone())
            }
            _ => {}
        }
    }
    if evidence.is_empty() {
        return Vec::new();
    }
    vec![FrameworkDetection {
        framework: "electron".into(),
        version,
        evidence,
    }]
}
