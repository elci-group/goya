//! Phase 2 exit criteria: cascade conformance end to end — CSS → CasaIR →
//! scene tree → computed styles — covering origin/importance, layers,
//! specificity, source order, inheritance, defaulting, custom properties,
//! conditional rules and structural (FAIR) binding.

use goya_casa::cascade::conditional::{ColorScheme, Environment, MediaType};
use goya_casa::context::computed_style::{compute_styles, ComputedStyles};
use goya_casa::context::structural::{bind_fair, SceneTree};
use goya_casa::diagnostics::codes;
use goya_casa::diagnostics::reporter::Reporter;
use goya_casa::ingestion::parser::ingest_to_ir;

fn environment() -> Environment {
    Environment {
        viewport_width: Some(800.0),
        viewport_height: Some(600.0),
        media_type: MediaType::Screen,
        color_scheme: Some(ColorScheme::Dark),
    }
}

fn computed(css: &str, html: &str) -> (SceneTree, ComputedStyles, Reporter) {
    let ir = ingest_to_ir("app.css", css);
    let (tree, _) = SceneTree::from_html(html);
    let mut reporter = Reporter::new();
    let styles = compute_styles(&ir, &tree, &environment(), &mut reporter);
    (tree, styles, reporter)
}

fn node_by_name<'a>(
    tree: &'a SceneTree,
    styles: &'a ComputedStyles,
    name: &str,
) -> &'a goya_casa::context::computed_style::ComputedStyle {
    let node = tree
        .walk()
        .into_iter()
        .find(|node| tree.get(*node).name == name)
        .unwrap_or_else(|| panic!("no <{name}> in fixture"));
    styles.get(node).unwrap()
}

#[test]
fn specificity_importance_and_source_order() {
    let (tree, styles, reporter) = computed(
        "p { color: red } .lead { color: green } #intro { color: blue } \
         .lead { color: gold } p { color: olive !important }",
        r#"<html><body><p id="intro" class="lead">hi</p></body></html>"#,
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    // Importance beats specificity (0,0,1 important beats 1,0,0 normal).
    assert_eq!(
        node_by_name(&tree, &styles, "p").get("color"),
        Some("olive")
    );
}

#[test]
fn specificity_decides_without_importance() {
    let (tree, styles, reporter) = computed(
        "p { color: red } .lead { color: green } #intro { color: blue } \
         .lead { color: gold }",
        r#"<html><body><p id="intro" class="lead">hi</p></body></html>"#,
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    // ID (1,0,0) beats class (0,1,0) beats type (0,0,1); the two .lead
    // rules tie, so the later one wins.
    assert_eq!(node_by_name(&tree, &styles, "p").get("color"), Some("blue"));
}

#[test]
fn descendant_combinator_and_attribute_matching() {
    let (tree, styles, reporter) = computed(
        "nav a[href$='.org'] { color: teal } nav a { color: olive } a { color: red }",
        r#"<html><body><nav><a href="https://example.org">x</a>\
           <a href="https://example.com">y</a></nav></body></html>"#,
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let links: Vec<_> = tree
        .walk()
        .into_iter()
        .filter(|node| tree.get(*node).name == "a")
        .collect();
    assert_eq!(styles.get(links[0]).unwrap().get("color"), Some("teal"));
    assert_eq!(styles.get(links[1]).unwrap().get("color"), Some("olive"));
}

#[test]
fn inheritance_and_initial_defaulting() {
    let (tree, styles, reporter) = computed(
        "body { color: navy; margin: 4px }",
        "<html><body><p>text</p></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let style = node_by_name(&tree, &styles, "p");
    // color inherits; margin does not and takes its initial value.
    assert_eq!(style.get("color"), Some("navy"));
    assert_eq!(style.get("margin"), Some("0"));
    assert_eq!(style.get("position"), Some("static"));
}

#[test]
fn custom_properties_inherit_and_compose() {
    let (tree, styles, reporter) = computed(
        "body { --accent: teal; --pad: calc(2px + var(--unit)); --unit: 3px } \
         p { color: var(--accent); padding: var(--pad) }",
        "<html><body><p>text</p></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let style = node_by_name(&tree, &styles, "p");
    assert_eq!(style.get("color"), Some("teal"));
    assert_eq!(style.get("padding"), Some("calc(2px + 3px)"));
}

#[test]
fn var_fallback_and_cycle_diagnostics() {
    let (tree, styles, reporter) = computed(
        "body { color: navy } :root { --self: var(--self) } \
         p { margin: var(--nothing, 7px); color: var(--self) }",
        "<html><body><p>text</p></body></html>",
    );
    let messages: Vec<&str> = reporter
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    // Fallback applies; the cycle is cut with CASA-CAS-001 and the
    // affected declaration is invalid at computed-value time
    // (CASA-CAS-002), so color is treated as unset and inherits.
    assert!(messages.contains(&codes::CASCADE_PROPERTY_CYCLE.code));
    assert!(messages.contains(&codes::CASCADE_INVALID_AT_COMPUTED_VALUE_TIME.code));
    let style = node_by_name(&tree, &styles, "p");
    assert_eq!(style.get("margin"), Some("7px"));
    assert_eq!(style.get("color"), Some("navy"));
}

#[test]
fn media_and_supports_gate_rules() {
    let (tree, styles, reporter) = computed(
        "@media (min-width: 900px) { p { color: red } } \
         @media (min-width: 100px) { p { color: green } } \
         @supports (display: grid) { p { margin: 1px } } \
         @supports (not-a-property: 1) { p { padding: 9px } }",
        "<html><body><p>text</p></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let style = node_by_name(&tree, &styles, "p");
    // Only the matching @media and the supported @supports apply.
    assert_eq!(style.get("color"), Some("green"));
    assert_eq!(style.get("margin"), Some("1px"));
    assert_eq!(style.get("padding"), Some("0"));
}

#[test]
fn layer_order_favors_unlayered_and_later_layers() {
    let (tree, styles, reporter) = computed(
        "@layer base { p { color: red } } \
         @layer theme { p { color: gold } } \
         p { color: olive }",
        "<html><body><p>text</p></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    // Unlayered normal declarations beat every layer; between layers the
    // later one wins.
    assert_eq!(
        node_by_name(&tree, &styles, "p").get("color"),
        Some("olive")
    );
}

#[test]
fn layered_rules_beat_only_earlier_layers() {
    let (tree, styles, reporter) = computed(
        "@layer base { p { color: red; margin: 1px } } \
         @layer theme { p { color: gold } }",
        "<html><body><p>text</p></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let style = node_by_name(&tree, &styles, "p");
    assert_eq!(style.get("color"), Some("gold"));
    // No competition for margin: the base layer value stands.
    assert_eq!(style.get("margin"), Some("1px"));
}

#[test]
fn fair_components_bind_to_scene_nodes() {
    let document: goya_fair::FairDocument = serde_json::from_str(
        r#"{
            "contract": "GY-FAIR/2.0",
            "source": { "root": "app", "files": [
                { "path": "app.js", "sha256": "00" }
            ] },
            "extractor": { "name": "test", "version": "1", "id_rule": "line" },
            "elements": [
                { "id": "e1", "kind": "component",
                  "provenance": { "file": "app.js", "line": 1,
                    "span_start": null, "span_end": null,
                    "fallback_origin": false },
                  "representability": { "status": "extracted" },
                  "semantics": {} },
                { "id": "e2", "kind": "component",
                  "provenance": { "file": "app.js", "line": 2,
                    "span_start": null, "span_end": null,
                    "fallback_origin": false },
                  "representability": { "status": "extracted" },
                  "semantics": {} }
            ]
        }"#,
    )
    .unwrap();
    let (tree, _) = SceneTree::from_html(
        r#"<div data-fair-id="e1"><button data-fair-id="e2">go</button></div>"#,
    );
    let bindings = bind_fair(&document, &tree);
    assert_eq!(bindings.bindings.len(), 2);
    assert!(bindings.unbound.is_empty());
}
