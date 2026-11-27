//! The deterministic transformation manifest (directive §9).
//!
//! A machine-readable record of one CSS → .gscene transformation: what
//! went in, which stages ran, what each produced, and a diagnostics
//! summary. Every collection is ordered (`BTreeMap`/stable vectors), so
//! the same inputs serialize to byte-identical JSON on every host.

use crate::context::computed_style::ComputedStyles;
use crate::context::structural::SceneTree;
use crate::diagnostics::reporter::Reporter;
use crate::geometry::layout_systems::LayoutTree;
use crate::ir::casa_ir::CasaIr;
use serde::Serialize;
use std::collections::BTreeMap;

/// Bumped on any breaking change to the manifest shape.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// What one pipeline stage produced.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StageSummary {
    pub name: String,
    /// Ordered counters: what the stage consumed and emitted.
    pub counts: BTreeMap<String, usize>,
}

/// Diagnostics aggregated by code, in code order.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct DiagnosticsSummary {
    pub total: usize,
    pub errors: usize,
    pub warnings: usize,
    /// Diagnostic code → count.
    pub by_code: BTreeMap<String, usize>,
}

/// The full transformation record.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransformationManifest {
    pub schema_version: u32,
    /// The entry stylesheet the transformation started from.
    pub origin: String,
    /// Pipeline stages in execution order.
    pub stages: Vec<StageSummary>,
    pub diagnostics: DiagnosticsSummary,
}

impl TransformationManifest {
    /// Build the manifest for a completed transformation.
    pub fn build(
        ir: &CasaIr,
        tree: &SceneTree,
        styles: &ComputedStyles,
        layout: &LayoutTree,
        reporter: &Reporter,
    ) -> Self {
        let rules: usize = ir.sheets.iter().map(|sheet| sheet.rules.len()).sum();
        let styled: usize = styles
            .per_node
            .values()
            .filter(|style| !style.properties.is_empty())
            .count();
        let properties: usize = styles
            .per_node
            .values()
            .map(|style| style.properties.len())
            .sum();
        let stages = vec![
            StageSummary {
                name: "ingestion".to_string(),
                counts: BTreeMap::from([
                    ("sheets".to_string(), ir.sheets.len()),
                    ("rules".to_string(), rules),
                    ("resources".to_string(), ir.resources.len()),
                ]),
            },
            StageSummary {
                name: "structural".to_string(),
                counts: BTreeMap::from([("elements".to_string(), tree.walk().len())]),
            },
            StageSummary {
                name: "cascade".to_string(),
                counts: BTreeMap::from([
                    ("styled_elements".to_string(), styled),
                    ("computed_properties".to_string(), properties),
                ]),
            },
            StageSummary {
                name: "geometry".to_string(),
                counts: BTreeMap::from([("laid_out_boxes".to_string(), layout.boxes.len())]),
            },
        ];
        let mut diagnostics = DiagnosticsSummary::default();
        for diagnostic in reporter.diagnostics() {
            diagnostics.total += 1;
            match diagnostic.severity {
                crate::diagnostics::codes::Severity::Error => diagnostics.errors += 1,
                _ => diagnostics.warnings += 1,
            }
            *diagnostics
                .by_code
                .entry(diagnostic.code.clone())
                .or_default() += 1;
        }
        TransformationManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            origin: ir.origin.clone(),
            stages,
            diagnostics,
        }
    }

    /// Deterministic JSON: field order is struct order and every map is
    /// a `BTreeMap`, so identical inputs yield identical bytes.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("manifest serialization is infallible")
    }
}
