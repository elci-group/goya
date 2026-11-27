//! `goya-disparate-electron` — the Disparates programme's Electron extractor
//! (directive DEL-TD-001, "Delectable").
//!
//! It reads an Electron application from a directory or an `app.asar`
//! archive and emits a GY-FAIR/2 document: elements, typed relationships
//! between them, and a project record of entry points and frameworks. Extraction is static, offline and
//! deterministic: no application code is run, no network is used, and no
//! Goya source is generated (GY-ENT-001 withholds reconstruction).
//!
//! Every element is `extracted`, or `undetermined` with a reason code. This
//! crate does not judge representability; that is a separate stage.

mod css;
mod emit;
mod html;
mod js;
pub mod material;
mod relate;

use emit::{s, Emitter, Origin, Semantics};
use goya_fair::{
    ElementKind, EntryPoint, ExtractorIdentity, FairDocument, FairProject, SemanticsValue,
    SourceFile, SourceManifest, FAIR_CONTRACT,
};
use material::{Budgets, Inventory};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const EXTRACTOR_NAME: &str = "goya-disparate-electron";
pub const EXTRACTOR_VERSION: &str = "0.2.0";
pub const ID_RULE: &str = "<file>#<kind>.<nnnn>: nnnn (zero-padded to at least four digits) counts elements of that FAIR kind in that file, in \
emission order (source order within each pass), from 1. Files are processed in byte order of their \
relative paths; bridge calls are emitted after all JavaScript, and state stores after their file's walk.";

/// The FAIR/2 reason-code register. Every code this extractor emits is in it,
/// and FAIR/2 validation refuses any other.
pub use goya_fair::REASON_CODES;

#[derive(Debug)]
pub enum ExtractError {
    Io(String),
    /// Malformed or unsafe input, including archive structure.
    Invalid(String),
    /// A resource budget was exceeded.
    Budget(String),
    /// The produced document failed FAIR validation (an extractor bug).
    Validation(Vec<goya_fair::ValidationError>),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "i/o error: {m}"),
            Self::Invalid(m) => write!(f, "invalid input: {m}"),
            Self::Budget(m) => write!(f, "budget exceeded: {m}"),
            Self::Validation(errors) => {
                write!(f, "extractor produced an invalid FAIR document:")?;
                for e in errors {
                    write!(f, " [{}] {}", e.code, e.message)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ExtractError {}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Extracts from a directory, or from an archive whose name ends in `.asar`.
/// `root_label` is recorded as the manifest root.
pub fn extract_path(
    path: &Path,
    root_label: &str,
    budgets: &Budgets,
) -> Result<FairDocument, ExtractError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| ExtractError::Io(format!("{}: {e}", path.display())))?;
    let inventory = if meta.is_dir() {
        material::from_directory(path, budgets)?
    } else if path.extension().is_some_and(|e| e == "asar") {
        if meta.len()
            > budgets
                .max_total_bytes
                .saturating_add(budgets.max_asar_header_bytes)
        {
            return Err(ExtractError::Budget(format!(
                "archive is {} bytes",
                meta.len()
            )));
        }
        let bytes = std::fs::read(path)
            .map_err(|e| ExtractError::Io(format!("{}: {e}", path.display())))?;
        material::from_asar(&bytes, budgets)?
    } else {
        return Err(ExtractError::Invalid(format!(
            "{} is neither a directory nor an .asar archive",
            path.display()
        )));
    };
    extract_inventory(inventory, root_label)
}

enum Pass {
    Js,
    Html,
    Css,
    Package,
    None,
}

fn pass_for(path: &str) -> Pass {
    let name = path.rsplit('/').next().unwrap_or(path);
    if path == "package.json" {
        Pass::Package
    } else if [".js", ".cjs", ".mjs", ".jsx", ".ts", ".cts", ".mts", ".tsx"]
        .iter()
        .any(|ext| name.ends_with(ext) && !name.ends_with(".d.ts"))
    {
        Pass::Js
    } else if name.ends_with(".html") || name.ends_with(".htm") {
        Pass::Html
    } else if name.ends_with(".css") {
        Pass::Css
    } else {
        Pass::None
    }
}

/// Extracts from an inventory that is already in memory.
pub fn extract_inventory(
    mut inventory: Inventory,
    root_label: &str,
) -> Result<FairDocument, ExtractError> {
    inventory.files.sort_by(|a, b| a.path.cmp(&b.path));
    inventory.skipped.sort_by(|a, b| a.path.cmp(&b.path));

    let manifest = SourceManifest {
        root: root_label.to_string(),
        files: inventory
            .files
            .iter()
            .map(|m| SourceFile {
                path: m.path.clone(),
                sha256: sha256_hex(&m.bytes),
            })
            .collect(),
    };

    let mut emitters: BTreeMap<String, Emitter> = BTreeMap::new();
    let mut js_out: BTreeMap<String, js::JsOutput> = BTreeMap::new();
    let mut html_out: BTreeMap<String, html::HtmlOutput> = BTreeMap::new();
    let mut package_main: Option<(String, String)> = None;

    for skipped in &inventory.skipped {
        let em = emitters
            .entry(skipped.path.clone())
            .or_insert_with(|| Emitter::new(&skipped.path, ""));
        em.unknown(
            skipped.observed,
            Origin::Line {
                line: 1,
                fallback: true,
            },
            skipped.reason,
            skipped.detail.clone(),
            Semantics::new(),
        );
    }

    for file in &inventory.files {
        let pass = pass_for(&file.path);
        if matches!(pass, Pass::None) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            let em = emitters
                .entry(file.path.clone())
                .or_insert_with(|| Emitter::new(&file.path, ""));
            em.unknown(
                "non_utf8_source",
                Origin::Line {
                    line: 1,
                    fallback: true,
                },
                "NON_UTF8_SOURCE",
                "source text is not UTF-8",
                Semantics::new(),
            );
            continue;
        };
        let mut em = Emitter::new(&file.path, text);
        match pass {
            Pass::Js => {
                js_out.insert(file.path.clone(), js::extract(text, &mut em));
            }
            Pass::Html => {
                html_out.insert(file.path.clone(), html::extract(text, &mut em));
            }
            Pass::Css => css::extract(text, &mut em),
            Pass::Package => {
                package_main = package(text, &mut em)
                    .map(|main| (main, em.elements[em.elements.len() - 1].id.clone()))
            }
            Pass::None => {}
        }
        emitters.insert(file.path.clone(), em);
    }

    // Bridge calls: `window.<key>.<member>(…)` where `<key>` is exposed by a
    // preload bridge somewhere in the application.
    let exposed: BTreeSet<String> = js_out
        .values()
        .flat_map(|o| o.exposed_keys.iter().cloned())
        .collect();
    for (path, out) in &js_out {
        let em = emitters.get_mut(path).expect("emitter per parsed file");
        for c in out
            .bridge_candidates
            .iter()
            .filter(|c| exposed.contains(&c.key))
        {
            let mut sem = Semantics::new();
            sem.insert("bridge".into(), s(format!("{}.{}", c.key, c.member)));
            em.emit(
                ElementKind::BridgeCall,
                Origin::Span(c.span.0, c.span.1),
                emit::Status::Extracted,
                sem,
            );
        }
    }

    let mut elements: Vec<_> = emitters.into_values().flat_map(|em| em.elements).collect();
    let manifest_paths: BTreeSet<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    let entries = relate::topology(package_main, &elements, &html_out);
    let mut roles: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    let mut entry_points: BTreeMap<(String, &'static str), Vec<String>> = BTreeMap::new();
    for (path, role, evidence) in entries {
        roles.entry(path.clone()).or_default().insert(role);
        if manifest_paths.contains(path.as_str()) {
            let list = entry_points.entry((path, role)).or_default();
            if !list.contains(&evidence) {
                list.push(evidence);
            }
        }
    }
    for element in &mut elements {
        let value = match roles.get(&element.provenance.file) {
            None => s("unresolved"),
            Some(set) if set.len() == 1 => s(set.iter().next().expect("one").to_string()),
            Some(set) => SemanticsValue::List(set.iter().map(|r| s(r.to_string())).collect()),
        };
        element.semantics.insert("process".into(), value);
    }
    let relationships = relate::relationships(&elements);
    let project = FairProject {
        entry_points: entry_points
            .into_iter()
            .map(|((path, role), evidence)| EntryPoint {
                path,
                role: role.to_string(),
                evidence,
            })
            .collect(),
        framework_detections: relate::frameworks(&elements),
    };

    let document = FairDocument {
        contract: FAIR_CONTRACT.to_string(),
        source: manifest,
        extractor: ExtractorIdentity {
            name: EXTRACTOR_NAME.to_string(),
            version: EXTRACTOR_VERSION.to_string(),
            id_rule: ID_RULE.to_string(),
        },
        project,
        elements,
        relationships,
    };
    document.validate().map_err(ExtractError::Validation)?;
    Ok(document)
}

/// Records `package.json` and returns its `main` entry, normalised.
fn package(text: &str, em: &mut Emitter) -> Option<String> {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            em.unknown(
                "package_manifest",
                Origin::Line {
                    line: e.line().max(1) as u64,
                    fallback: true,
                },
                "PARSE_FAILED",
                e.to_string(),
                Semantics::new(),
            );
            return None;
        }
    };
    let mut sem = Semantics::new();
    for key in ["name", "version", "main", "license", "type"] {
        if let Some(v) = value.get(key).and_then(|v| v.as_str()) {
            sem.insert(key.into(), s(v));
        }
    }
    for key in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(deps) = value.get(key).and_then(|v| v.as_object()) {
            let mut map = BTreeMap::new();
            for (name, version) in deps {
                map.insert(name.clone(), s(version.as_str().unwrap_or("").to_string()));
            }
            sem.insert(key.into(), SemanticsValue::Map(map));
        }
    }
    let main = value
        .get("main")
        .and_then(|v| v.as_str())
        .and_then(js::normalise);
    em.emit(
        ElementKind::PackageManifest,
        Origin::Line {
            line: 1,
            fallback: true,
        },
        emit::Status::Extracted,
        sem,
    );
    main
}
