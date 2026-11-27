//! Manuscrito's initial analysis-only project inventory.
//!
//! This crate deliberately does not parse TypeScript, execute package scripts,
//! resolve/install packages, or emit `.gscene`. Its report describes files and
//! declared package dependencies only.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub mod mir;
pub use mir::{MirNode, MirReport, MirStatus, SemanticKind, MIR_VERSION};

pub const REPORT_SCHEMA: &str = "goya.manuscrito.analysis/1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisReport {
    pub schema: String,
    pub mode: String,
    pub translated: bool,
    pub project_root: String,
    pub source_files: Vec<SourceFile>,
    pub tsconfig_files: Vec<String>,
    pub package_manifests: Vec<String>,
    pub lockfiles: Vec<String>,
    pub package_manifest: Option<String>,
    pub package_name: Option<String>,
    pub dependencies: Vec<Dependency>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceFile {
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dependency {
    pub name: String,
    pub version_spec: String,
    pub declaration: String,
    pub classification: String,
}

#[derive(Debug, Deserialize)]
struct PackageManifest {
    name: Option<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(rename = "devDependencies", default)]
    dev_dependencies: BTreeMap<String, String>,
    #[serde(rename = "peerDependencies", default)]
    peer_dependencies: BTreeMap<String, String>,
    #[serde(rename = "optionalDependencies", default)]
    optional_dependencies: BTreeMap<String, String>,
}

/// Inventory a project deterministically without executing source or scripts.
pub fn analyse(project: impl AsRef<Path>) -> Result<AnalysisReport, String> {
    let root = project
        .as_ref()
        .canonicalize()
        .map_err(|e| format!("cannot resolve project {}: {e}", project.as_ref().display()))?;
    if !root.is_dir() {
        return Err(format!("project is not a directory: {}", root.display()));
    }
    let mut files = Vec::new();
    scan(&root, &root, &mut files)?;
    files.sort();

    let mut tsconfig_files = Vec::new();
    let mut package_manifests = Vec::new();
    let mut lockfiles = Vec::new();
    let mut source_files = Vec::new();
    let mut package_manifest = None;
    for file in &files {
        let name = Path::new(&file.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if name == "tsconfig.json" || (name.starts_with("tsconfig.") && name.ends_with(".json")) {
            tsconfig_files.push(file.path.clone());
        }
        if matches!(
            file.kind.as_str(),
            "typescript" | "typescript-jsx" | "javascript" | "declaration"
        ) {
            source_files.push(file.clone());
        }
        if name == "package.json" {
            package_manifests.push(file.path.clone());
            if file.path == "package.json" {
                package_manifest = Some(file.path.clone());
            }
        }
        if is_lockfile(name) {
            lockfiles.push(file.path.clone());
        }
    }
    tsconfig_files.sort();
    source_files.sort();
    package_manifests.sort();
    lockfiles.sort();

    let mut dependencies = Vec::new();
    let mut package_name = None;
    let mut diagnostics = Vec::new();
    if let Some(manifest) = &package_manifest {
        let bytes =
            fs::read(root.join(manifest)).map_err(|e| format!("cannot read {manifest}: {e}"))?;
        let parsed: PackageManifest =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid package.json: {e}"))?;
        package_name = parsed.name;
        for (group, values) in [
            ("dependency", parsed.dependencies),
            ("development", parsed.dev_dependencies),
            ("peer", parsed.peer_dependencies),
            ("optional", parsed.optional_dependencies),
        ] {
            dependencies.extend(values.into_iter().map(|(name, version_spec)| Dependency {
                name,
                version_spec,
                declaration: group.to_owned(),
                classification: "unclassified".to_owned(),
            }));
        }
        dependencies.sort();
    } else {
        diagnostics
            .push("package.json not found; package dependencies were not inventoried".to_owned());
    }
    if tsconfig_files.is_empty() {
        diagnostics.push(
            "tsconfig.json not found; compiler options and project references were not resolved"
                .to_owned(),
        );
    }
    if lockfiles.is_empty() {
        diagnostics.push("no supported package-manager lockfile found".to_owned());
    }
    if package_manifests.len() > 1 {
        diagnostics.push("multiple package manifests found; workspace membership and package relationships were not resolved".to_owned());
    }
    if source_files.is_empty() {
        diagnostics.push("no TypeScript or JavaScript source files found".to_owned());
    }
    diagnostics.push("analysis-only: no AST/type analysis, dependency resolution, translation, or .gscene emission performed".to_owned());

    Ok(AnalysisReport {
        schema: REPORT_SCHEMA.to_owned(),
        mode: "analysis-only".to_owned(),
        translated: false,
        project_root: root.display().to_string(),
        source_files,
        tsconfig_files,
        package_manifests,
        lockfiles,
        package_manifest,
        package_name,
        dependencies,
        diagnostics,
    })
}

fn scan(root: &Path, dir: &Path, out: &mut Vec<SourceFile>) -> Result<(), String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("cannot enumerate {}: {e}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let ty = entry
            .file_type()
            .map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            let name = entry.file_name();
            if name == "node_modules"
                || name == ".git"
                || name == "target"
                || name == "dist"
                || name == "build"
            {
                continue;
            }
            scan(root, &path, out)?;
        } else if ty.is_file() {
            let kind = source_kind(&path).or_else(|| {
                let name = path.file_name()?.to_string_lossy();
                (name == "package.json"
                    || is_lockfile(&name)
                    || name == "tsconfig.json"
                    || (name.starts_with("tsconfig.") && name.ends_with(".json")))
                .then_some("project-config")
            });
            if let Some(kind) = kind {
                let rel: PathBuf = path
                    .strip_prefix(root)
                    .expect("walked below root")
                    .to_path_buf();
                out.push(SourceFile {
                    path: rel.to_string_lossy().replace('\\', "/"),
                    kind: kind.to_owned(),
                });
            }
        }
    }
    Ok(())
}

fn source_kind(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_string_lossy();
    if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
        return Some("declaration");
    }
    match path.extension()?.to_str()? {
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("typescript-jsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        _ => None,
    }
}

fn is_lockfile(name: &str) -> bool {
    matches!(
        name,
        "package-lock.json"
            | "npm-shrinkwrap.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lock"
            | "bun.lockb"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_kinds_are_explicit_and_stable() {
        assert_eq!(source_kind(Path::new("a.ts")), Some("typescript"));
        assert_eq!(source_kind(Path::new("a.tsx")), Some("typescript-jsx"));
        assert_eq!(source_kind(Path::new("a.d.ts")), Some("declaration"));
        assert_eq!(source_kind(Path::new("a.js")), Some("javascript"));
    }
}
