//! CSS ingestion: parse with goya-css-syntax and lower into CasaIR.

use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::{CasaIr, Lowering, Sheet, SheetScope, IR_SCHEMA_VERSION};
use goya_css_syntax::{parse_stylesheet, Rule};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("CSS syntax error: {0}")]
    Syntax(String),
    #[error("Import resolution failed: {0}")]
    ImportResolution(String),
}

/// A stylesheet parsed into rules, keeping the source for span slicing.
pub struct ParsedStylesheet {
    pub rules: Vec<Rule>,
    pub source_map: SourceMap,
}

/// Byte-span access into the original input.
pub struct SourceMap {
    pub source: String,
}

impl SourceMap {
    pub fn slice(&self, start: usize, end: usize) -> &str {
        let end = end.min(self.source.len());
        let start = start.min(end);
        &self.source[start..end]
    }
}

impl std::str::FromStr for ParsedStylesheet {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(ParsedStylesheet {
            rules: parse_stylesheet(input),
            source_map: SourceMap {
                source: input.to_string(),
            },
        })
    }
}

/// One sheet parsed and lowered into CasaIR form.
pub struct IngestedSheet {
    pub sheet: Sheet,
    pub resources: Vec<crate::ir::casa_ir::ResourceRef>,
    pub reporter: Reporter,
}

/// Parse and lower a single stylesheet's text (no import resolution).
pub fn ingest_text(name: &str, source: &str) -> IngestedSheet {
    let rules = parse_stylesheet(source);
    let mut reporter = Reporter::new();
    let mut resources = Vec::new();
    let mut lowering = Lowering::new(name, source, &mut reporter, &mut resources);
    let lowered = lowering.rules(rules);
    IngestedSheet {
        sheet: Sheet {
            name: name.to_string(),
            scope: SheetScope::Light,
            rules: lowered,
        },
        resources,
        reporter,
    }
}

/// Convenience: a complete [`CasaIr`] for one sheet without imports.
pub fn ingest_to_ir(name: &str, source: &str) -> CasaIr {
    let ingested = ingest_text(name, source);
    CasaIr {
        schema_version: IR_SCHEMA_VERSION,
        origin: name.to_string(),
        sheets: vec![ingested.sheet],
        resources: ingested.resources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_text_builds_ir_with_sheet_and_resources() {
        let ingested = ingest_text("app.css", "body { background: url(bg.png); color: red }");
        assert_eq!(ingested.sheet.name, "app.css");
        assert_eq!(ingested.sheet.rules.len(), 1);
        assert_eq!(ingested.resources.len(), 1);
        assert!(ingested.reporter.is_empty());
    }

    #[test]
    fn ingest_to_ir_sets_schema_version_and_origin() {
        let ir = ingest_to_ir("app.css", "body { color: red }");
        assert_eq!(ir.schema_version, IR_SCHEMA_VERSION);
        assert_eq!(ir.origin, "app.css");
        assert_eq!(ir.entry_sheet().unwrap().name, "app.css");
    }

    #[test]
    fn source_map_slices_spans() {
        let parsed: ParsedStylesheet = "body { color: red }".parse().unwrap();
        assert_eq!(parsed.source_map.slice(0, 4), "body");
    }
}
