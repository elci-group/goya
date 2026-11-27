//! Phase 1 exit criteria: CSS → CasaIR round-trip with deterministic
//! serialization, across the full ingestion surface (imports, media,
//! font-face, keyframes, resources, diagnostics).

use goya_casa::diagnostics::codes;
use goya_casa::ingestion::imports::{resolve_entry, MapLoader};
use goya_casa::ir::casa_ir::{CasaIr, CasaRule, ResourceKind};
use goya_casa::ir::serialization::{SerializationError, IR_SCHEMA_VERSION};
use goya_casa::ir::value_categories::ValueCategory;

const ENTRY_CSS: &str = "@import \"base.css\";\n\
     :root { --accent: rebeccapurple; --gap: var(--base-gap, 8px) }\n\
     body { font-family: sans-serif; color: var(--accent); background: url(bg.png) }\n\
     @media (min-width: 400px) { .card { padding: var(--gap) } }\n\
     @font-face { font-family: Body; src: url(body.woff2) format(woff2) }\n\
     @keyframes pulse { 0%, 100% { opacity: 1 } 50% { opacity: 0.5 } }\n";

const BROKEN_DECLARATION: &str = ".broken { colour red }\n";

const BASE_CSS: &str = ":root { --base-gap: 12px }\n.a { margin: 0 auto }\n";

fn fixture_loader() -> MapLoader {
    MapLoader::default()
        .insert("/site/app.css", ENTRY_CSS)
        .insert("/site/base.css", BASE_CSS)
}

fn expected_ir() -> CasaIr {
    let resolved = resolve_entry("/site/app.css", &fixture_loader());
    assert!(
        resolved.reporter.is_empty(),
        "{:?}",
        resolved.reporter.diagnostics()
    );
    resolved.ir
}

#[test]
fn css_to_ir_to_json_round_trips() {
    let ir = expected_ir();
    // Entry first, then the import, in resolution order.
    let names: Vec<_> = ir.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["/site/app.css", "/site/base.css"]);

    let entry = ir.entry_sheet().unwrap();
    // Import inlined and recorded.
    assert!(matches!(&entry.rules[0], CasaRule::Import(import)
        if import.resolved.as_deref() == Some("/site/base.css")));
    // Custom properties, var() references, url() resources all captured.
    assert!(entry.rules.iter().any(|rule| matches!(rule,
        CasaRule::Style(style) if style.selectors.iter().any(|s| s.text == ":root"))));
    let fonts: Vec<_> = ir
        .resources
        .iter()
        .filter(|r| r.kind == ResourceKind::Font)
        .collect();
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0].url, "body.woff2");
    assert!(ir.resources.iter().any(|r| r.url == "bg.png"));

    // A malformed declaration is reported as a warning, not lost.
    let reparsed = goya_casa::ingestion::parser::ingest_text(
        "/site/app.css",
        &format!("{ENTRY_CSS}{BROKEN_DECLARATION}"),
    );
    assert!(reparsed
        .reporter
        .diagnostics()
        .iter()
        .any(|d| d.code == codes::SYNTAX_INVALID.code));

    // Round-trip: JSON → IR → JSON is byte-identical.
    let json = ir.to_json();
    let back = CasaIr::from_json(&json).unwrap();
    assert_eq!(back, ir);
    assert_eq!(back.to_json(), json);
    assert_eq!(back.schema_version, IR_SCHEMA_VERSION);

    // Every lowered declaration starts at Declared.
    let entry = back.entry_sheet().unwrap();
    let declared = entry
        .rules
        .iter()
        .filter_map(|rule| match rule {
            CasaRule::Style(style) => Some(&style.declarations),
            _ => None,
        })
        .flatten()
        .all(|d| d.category == ValueCategory::Declared);
    assert!(declared);
}

#[test]
fn serialization_is_deterministic_across_ingestions() {
    let first = expected_ir().to_json();
    let second = expected_ir().to_json();
    assert_eq!(first, second);
}

#[test]
fn schema_mismatch_is_rejected() {
    let ir = expected_ir();
    let json = ir.to_json().replace(
        &format!("\"schema_version\":{IR_SCHEMA_VERSION}"),
        "\"schema_version\":99",
    );
    assert!(matches!(
        CasaIr::from_json(&json),
        Err(SerializationError::Schema { found: 99, .. })
    ));
}

#[test]
fn import_cycles_cut_with_diagnostics_and_terminate() {
    let loader = MapLoader::default()
        .insert("/site/x.css", "@import \"y.css\"; .x { color: red }")
        .insert("/site/y.css", "@import \"x.css\"; .y { color: blue }");
    let resolved = resolve_entry("/site/x.css", &loader);
    assert_eq!(resolved.reporter.diagnostics()[0].code, "CASA-IMP-001");
    // Both sheets still present exactly once.
    assert_eq!(resolved.ir.sheets.len(), 2);
}
