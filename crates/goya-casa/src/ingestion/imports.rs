//! `@import` resolution with cycle detection (directive §9).
//!
//! Imported sheets are inlined into the IR in resolution order: the entry
//! sheet first, then each imported sheet depth-first. Cycles are cut at
//! the repeated sheet with a diagnostic; a diamond (the same sheet
//! reached twice) loads once, first reference wins. Per the CSS
//! specification, an `@import` after a style rule is ignored.

use crate::diagnostics::codes;
use crate::diagnostics::reporter::{Location, Reporter};
use crate::ir::casa_ir::{CasaIr, CasaRule, Lowering, Sheet, SheetScope, IR_SCHEMA_VERSION};
use goya_css_syntax::{parse_stylesheet, Rule};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

/// Loads the text of a stylesheet path.
pub trait Loader {
    fn load(&self, path: &Path) -> std::io::Result<String>;
}

/// Loads from the real filesystem.
pub struct FileSystemLoader;

impl Loader for FileSystemLoader {
    fn load(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// In-memory loader for tests and embedded stylesheets.
#[derive(Debug, Default)]
pub struct MapLoader {
    pub files: BTreeMap<String, String>,
}

impl MapLoader {
    pub fn insert(mut self, path: impl Into<String>, contents: impl Into<String>) -> Self {
        self.files.insert(path.into(), contents.into());
        self
    }
}

impl Loader for MapLoader {
    fn load(&self, path: &Path) -> std::io::Result<String> {
        self.files
            .get(&path.display().to_string())
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not in map"))
    }
}

/// A stylesheet with its `@import` graph resolved.
pub struct Resolved {
    pub ir: CasaIr,
    pub reporter: Reporter,
}

/// Resolve an entry stylesheet and its transitive imports.
pub fn resolve_entry(entry: &str, loader: &dyn Loader) -> Resolved {
    let mut importer = Importer {
        loader,
        stack: Vec::new(),
        seen: BTreeSet::new(),
    };
    let mut sheets = Vec::new();
    let mut resources = Vec::new();
    let mut reporter = Reporter::new();
    importer.load_sheet(entry, &mut sheets, &mut resources, &mut reporter);
    let resources = crate::ingestion::resources::dedup(resources, &mut reporter);
    Resolved {
        ir: CasaIr {
            schema_version: IR_SCHEMA_VERSION,
            origin: entry.to_string(),
            sheets,
            resources,
        },
        reporter,
    }
}

struct Importer<'a> {
    loader: &'a dyn Loader,
    /// Path chain of the sheets currently being inlined.
    stack: Vec<String>,
    seen: BTreeSet<String>,
}

impl Importer<'_> {
    fn load_sheet(
        &mut self,
        path: &str,
        sheets: &mut Vec<Sheet>,
        resources: &mut Vec<crate::ir::casa_ir::ResourceRef>,
        reporter: &mut Reporter,
    ) {
        let key = normalise(path);
        if self.stack.contains(&key) {
            let mut chain: VecDeque<&String> = self.stack.iter().collect();
            chain.push_back(&key);
            let chain: Vec<&str> = chain.iter().map(|p| p.as_str()).collect();
            reporter.emit(
                codes::IMPORT_CYCLE,
                format!("@import cycle: {}", chain.join(" -> ")),
                None,
            );
            return;
        }
        if !self.seen.insert(key.clone()) {
            return; // diamond: already in the IR, first load wins
        }
        let source = match self.loader.load(Path::new(path)) {
            Ok(source) => source,
            Err(error) => {
                reporter.emit(codes::IMPORT_UNRESOLVED, format!("{path}: {error}"), None);
                return;
            }
        };
        self.stack.push(key);
        // Pre-order: the entry sheet lands first, imports after it.
        let index = sheets.len();
        sheets.push(Sheet {
            name: path.to_string(),
            scope: SheetScope::Light,
            rules: Vec::new(),
        });
        let sheet = self.lower_sheet(path, &source, sheets, resources, reporter);
        self.stack.pop();
        sheets[index] = sheet;
    }

    fn lower_sheet(
        &mut self,
        name: &str,
        source: &str,
        sheets: &mut Vec<Sheet>,
        resources: &mut Vec<crate::ir::casa_ir::ResourceRef>,
        reporter: &mut Reporter,
    ) -> Sheet {
        let parsed = parse_stylesheet(source);
        let mut sheet_rules = Vec::new();
        let mut seen_style = false;
        for rule in &parsed {
            if let Rule::At(at) = rule {
                if at.name.eq_ignore_ascii_case("import") && seen_style {
                    reporter.emit(
                        codes::IMPORT_MISPLACED,
                        "@import after a style rule is ignored",
                        Some(Location {
                            source: name.to_string(),
                            start: at.span.start,
                            end: at.span.end,
                        }),
                    );
                    continue;
                }
            }
            // A fresh Lowering per rule: the borrows of `reporter` and
            // `resources` end with the block, before an import recurses.
            let lowered = {
                let mut lowering = Lowering::new(name, source, reporter, resources);
                lowering.rule(rule)
            };
            match lowered {
                Some(CasaRule::Import(mut import)) => {
                    let child = resolve_relative(name, &import.url);
                    self.load_sheet(&child, sheets, resources, reporter);
                    import.resolved = Some(child);
                    // All imports must precede any style rule, so once one
                    // import is inlined, later ones are misplaced.
                    seen_style = true;
                    sheet_rules.push(CasaRule::Import(import));
                }
                Some(rule) => {
                    if matches!(rule, CasaRule::Style(_)) {
                        seen_style = true;
                    }
                    sheet_rules.push(rule);
                }
                None => {}
            }
        }
        Sheet {
            name: name.to_string(),
            scope: SheetScope::Light,
            rules: sheet_rules,
        }
    }
}

/// Resolve an import URL relative to the importing sheet. Absolute paths
/// and URLs are kept as-is; unsupported schemes fail later at load with
/// `CASA-IMP-002`.
fn resolve_relative(from: &str, url: &str) -> String {
    if url.starts_with('/') || url.contains("://") {
        return url.to_string();
    }
    let base = match Path::new(from).parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    base.join(url).display().to_string()
}

/// Prefer the filesystem's canonical form so aliasing paths share one
/// identity; fall back to the literal path when the file does not exist.
fn normalise(path: &str) -> String {
    match std::fs::canonicalize(path) {
        Ok(canonical) => canonical.display().to_string(),
        Err(_) => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader() -> MapLoader {
        MapLoader::default()
            .insert("/site/entry.css", "@import \"a.css\";\nbody { color: red }")
            .insert("/site/a.css", "@import \"b.css\";\n.a { margin: 0 }")
            .insert("/site/b.css", ".b { padding: 0 }")
    }

    #[test]
    fn imports_inline_depth_first_in_resolution_order() {
        let resolved = resolve_entry("/site/entry.css", &loader());
        assert!(
            resolved.reporter.is_empty(),
            "{:?}",
            resolved.reporter.diagnostics()
        );
        let names: Vec<_> = resolved.ir.sheets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["/site/entry.css", "/site/a.css", "/site/b.css"]);
        match &resolved.ir.entry_sheet().unwrap().rules[0] {
            CasaRule::Import(import) => {
                assert_eq!(import.url, "a.css");
                assert_eq!(import.resolved.as_deref(), Some("/site/a.css"));
            }
            other => panic!("expected import, got {other:?}"),
        }
    }

    #[test]
    fn cycles_are_cut_with_a_diagnostic() {
        let loader = MapLoader::default()
            .insert("/site/a.css", "@import \"b.css\"")
            .insert("/site/b.css", "@import \"a.css\"");
        let resolved = resolve_entry("/site/a.css", &loader);
        let diagnostics = resolved.reporter.diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "CASA-IMP-001");
        assert!(diagnostics[0]
            .message
            .contains("/site/a.css -> /site/b.css -> /site/a.css"));
    }

    #[test]
    fn missing_imports_report_and_continue() {
        let loader = MapLoader::default().insert("/site/a.css", "@import \"gone.css\"");
        let resolved = resolve_entry("/site/a.css", &loader);
        assert_eq!(resolved.reporter.diagnostics()[0].code, "CASA-IMP-002");
        // The importing sheet itself still lands in the IR.
        assert_eq!(resolved.ir.sheets.len(), 1);
    }

    #[test]
    fn misplaced_imports_are_ignored_per_spec() {
        let loader = MapLoader::default()
            .insert("/site/a.css", "body { color: red }\n@import \"b.css\"")
            .insert("/site/b.css", ".b { padding: 0 }");
        let resolved = resolve_entry("/site/a.css", &loader);
        assert_eq!(resolved.reporter.diagnostics()[0].code, "CASA-IMP-003");
        assert_eq!(resolved.ir.sheets.len(), 1); // b.css never loaded
        match &resolved.ir.entry_sheet().unwrap().rules[0] {
            CasaRule::Style(style) => assert_eq!(style.declarations[0].name, "color"),
            other => panic!("expected style rule, got {other:?}"),
        }
    }

    #[test]
    fn nested_media_rules_lower_inside_imported_sheets() {
        let loader =
            MapLoader::default().insert("/site/a.css", "@media screen { .a { color: blue } }");
        let resolved = resolve_entry("/site/a.css", &loader);
        assert!(resolved.reporter.is_empty());
        match &resolved.ir.entry_sheet().unwrap().rules[0] {
            CasaRule::Media(media) => {
                assert_eq!(media.condition, "screen");
                assert_eq!(media.rules.len(), 1);
            }
            other => panic!("expected media rule, got {other:?}"),
        }
    }
}
