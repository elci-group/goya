//! Phase 3 exit criteria: basic CSS → layout → scene conversion, end to
//! end. Each test drives the full pipeline (CSS → CasaIR → scene tree →
//! computed styles → layout) and checks the resolved boxes for one
//! layout system, plus the emission stage outputs.

use goya_casa::cascade::conditional::{Environment, MediaType};
use goya_casa::context::computed_style::{compute_styles, ComputedStyles};
use goya_casa::context::structural::{NodeId, SceneTree};
use goya_casa::diagnostics::reporter::Reporter;
use goya_casa::emission::gscene;
use goya_casa::emission::manifest::TransformationManifest;
use goya_casa::emission::provenance;
use goya_casa::geometry::layout_systems::{layout_document, LayoutTree};
use goya_casa::geometry::Context;
use goya_casa::ingestion::parser::ingest_to_ir;

fn environment() -> Environment {
    Environment {
        viewport_width: Some(800.0),
        viewport_height: Some(600.0),
        media_type: MediaType::Screen,
        color_scheme: None,
    }
}

fn context() -> Context {
    Context {
        font_size: 16.0,
        root_font_size: 16.0,
        viewport_width: 800.0,
        viewport_height: 600.0,
    }
}

/// Full CSS → layout pipeline. Returns the tree, styles and resolved
/// border boxes.
fn pipeline(css: &str, html: &str) -> (SceneTree, ComputedStyles, LayoutTree, Reporter) {
    let ir = ingest_to_ir("app.css", css);
    let (tree, _) = SceneTree::from_html(html);
    let mut reporter = Reporter::new();
    let styles = compute_styles(&ir, &tree, &environment(), &mut reporter);
    let layout = layout_document(&tree, &styles, &context(), &mut reporter);
    (tree, styles, layout, reporter)
}

/// The node id of the first element matching a class, in tree order.
fn by_class(tree: &SceneTree, class: &str) -> NodeId {
    tree.walk()
        .into_iter()
        .find(|node| tree.get(*node).classes.iter().any(|c| c == class))
        .unwrap_or_else(|| panic!("no .{class} in fixture"))
}

const PAGE: &str = "<html><body><div class=\"a\"></div><div class=\"b\"></div></body></html>";

#[test]
fn block_children_stack_and_auto_margins_center() {
    let (tree, _, layout, reporter) = pipeline(
        "div.a { height: 50px; margin: 10px 0 } div.b { height: 30px; width: 100px; margin: 0 auto }",
        PAGE,
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let a = layout.get(by_class(&tree, "a")).unwrap();
    let b = layout.get(by_class(&tree, "b")).unwrap();
    // .a stacks from the top of the body content box; body has no
    // styles, so the body's border box starts at the viewport origin.
    assert_eq!((a.x, a.y, a.width, a.height), (0.0, 10.0, 800.0, 50.0));
    // .b sits below .a (margin 10 + 50 + margin 10) and centers.
    assert_eq!((b.x, b.y, b.width, b.height), (350.0, 70.0, 100.0, 30.0));
}

#[test]
fn flex_row_grows_items_to_fill_the_main_axis() {
    let (tree, _, layout, reporter) = pipeline(
        "section { display: flex; height: 100px } div { flex: 1 }",
        "<html><body><section><div class=\"x\"></div><div class=\"y\"></div></section></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let x = layout.get(by_class(&tree, "x")).unwrap();
    let y = layout.get(by_class(&tree, "y")).unwrap();
    // Equal grow splits 800px into 400 each; stretch fills the 100px cross.
    assert_eq!((x.x, x.width, x.height), (0.0, 400.0, 100.0));
    assert_eq!((y.x, y.width, y.height), (400.0, 400.0, 100.0));
}

#[test]
fn grid_fixed_and_fr_columns() {
    let (tree, _, layout, reporter) = pipeline(
        "main { display: grid; grid-template-columns: 200px 1fr 1fr; height: 90px } div { grid-row: 1 }",
        "<html><body><main><div class=\"c1\"></div><div class=\"c2\"></div><div class=\"c3\"></div></main></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let c1 = layout.get(by_class(&tree, "c1")).unwrap();
    let c2 = layout.get(by_class(&tree, "c2")).unwrap();
    let c3 = layout.get(by_class(&tree, "c3")).unwrap();
    assert_eq!((c1.x, c1.width), (0.0, 200.0));
    // The 600px remainder splits evenly between the two fr columns.
    assert_eq!((c2.x, c2.width), (200.0, 300.0));
    assert_eq!((c3.x, c3.width), (500.0, 300.0));
}

#[test]
fn absolute_child_positions_against_a_positioned_ancestor() {
    let (tree, _, layout, reporter) = pipeline(
        "section { position: relative; height: 200px } .pin { position: absolute; left: 10px; top: 20px; width: 50px; height: 40px }",
        "<html><body><section><div class=\"pin\"></div></section></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let pin = layout.get(by_class(&tree, "pin")).unwrap();
    // The section starts at the body origin; the pin offsets from there.
    assert_eq!(
        (pin.x, pin.y, pin.width, pin.height),
        (10.0, 20.0, 50.0, 40.0)
    );
}

#[test]
fn absolute_offsets_stretch_a_sized_box() {
    let (tree, _, layout, reporter) = pipeline(
        "section { position: relative; height: 200px } .full { position: absolute; left: 10px; right: 30px; top: 5px; bottom: 15px }",
        "<html><body><section><div class=\"full\"></div></section></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let full = layout.get(by_class(&tree, "full")).unwrap();
    // 800 − 10 − 30 and 200 − 5 − 15: both axes stretch between offsets.
    assert_eq!((full.x, full.width), (10.0, 760.0));
    assert_eq!((full.y, full.height), (5.0, 180.0));
}

#[test]
fn fixed_child_uses_the_viewport_as_containing_block() {
    let (tree, _, layout, reporter) = pipeline(
        "section { position: relative } .hud { position: fixed; right: 0; top: 0; width: 100px; height: 20px }",
        "<html><body><section><div class=\"hud\"></div></section></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let hud = layout.get(by_class(&tree, "hud")).unwrap();
    // Pinned to the viewport's top-right corner, not the section's.
    assert_eq!((hud.x, hud.y), (700.0, 0.0));
}

#[test]
fn table_cells_share_columns_equally() {
    let (tree, _, layout, reporter) = pipeline(
        "table { display: table; width: 600px } tr { display: table-row } td { display: table-cell; height: 20px }",
        "<html><body><table><tr><td class=\"p\"></td><td class=\"q\"></td></tr><tr><td></td><td></td></tr></table></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let p = layout.get(by_class(&tree, "p")).unwrap();
    let q = layout.get(by_class(&tree, "q")).unwrap();
    assert_eq!((p.x, p.width, p.height), (0.0, 300.0, 20.0));
    assert_eq!((q.x, q.width), (300.0, 300.0));
}

#[test]
fn floats_stack_side_by_side_and_flow_resumes_below() {
    let (tree, _, layout, reporter) = pipeline(
        ".side { float: left; width: 100px; height: 40px } .rest { height: 10px }",
        "<html><body><div class=\"side\"></div><div class=\"side\"></div><div class=\"rest\"></div></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let sides: Vec<_> = tree
        .walk()
        .into_iter()
        .filter(|node| tree.get(*node).classes.iter().any(|c| c == "side"))
        .collect();
    let first = layout.get(sides[0]).unwrap();
    let second = layout.get(sides[1]).unwrap();
    assert_eq!((first.x, first.y), (0.0, 0.0));
    assert_eq!((second.x, second.y), (100.0, 0.0));
    let rest = layout.get(by_class(&tree, "rest")).unwrap();
    assert_eq!(rest.y, 40.0);
}

#[test]
fn multicol_splits_children_across_columns() {
    let (tree, _, layout, reporter) = pipeline(
        "article { column-count: 2; column-gap: 0 } p { height: 10px }",
        "<html><body><article><p class=\"m1\"></p><p class=\"m2\"></p><p class=\"m3\"></p></article></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let m1 = layout.get(by_class(&tree, "m1")).unwrap();
    let m2 = layout.get(by_class(&tree, "m2")).unwrap();
    let m3 = layout.get(by_class(&tree, "m3")).unwrap();
    // Two children per column: 800/2 = 400 wide each; the third wraps
    // to the second column top.
    assert_eq!((m1.x, m1.y), (0.0, 0.0));
    assert_eq!((m2.x, m2.y), (0.0, 10.0));
    assert_eq!((m3.x, m3.y), (400.0, 0.0));
}

#[test]
fn sticky_clamps_inside_its_parent() {
    let (tree, _, layout, reporter) = pipeline(
        "section { height: 200px } .bar { position: sticky; top: 10px; height: 30px; margin-bottom: 500px }",
        "<html><body><section><div class=\"bar\"></div></section></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let bar = layout.get(by_class(&tree, "bar")).unwrap();
    // Sticky shifts by top: 10px and stays within the section's box.
    assert_eq!(bar.y, 10.0);
}

#[test]
fn scene_emission_converts_fill_border_and_fractions() {
    let (tree, styles, layout, reporter) = pipeline(
        "div.a { height: 300px; background-color: #204080; border-top: 2px solid red }",
        PAGE,
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let viewport = goya_casa::geometry::Rect {
        x: 0.0,
        y: 0.0,
        width: 800.0,
        height: 600.0,
    };
    let scene = gscene::emit_scene(&tree, &styles, &layout, &context(), &viewport, "page");
    // root → html → body → div.a
    let cell = &scene.root.children[0].children[0].children[0];
    assert_eq!(cell.name, "div");
    assert_eq!(cell.fill, Some(0x204080));
    assert_eq!(cell.stroke, Some(0xff0000));
    assert_eq!(cell.stroke_width, Some(2.0));
    let extent = cell.extent_p.unwrap();
    // Fractions are against the parent (body, height 300px), not the
    // viewport.
    assert_eq!((extent.x, extent.y), (1.0, 1.0));
}

#[test]
fn manifest_is_deterministic_and_counts_the_pipeline() {
    let (tree, styles, layout, reporter) = pipeline("div.a { height: 10px; color: red }", PAGE);
    let ir = ingest_to_ir("app.css", "div.a { height: 10px; color: red }");
    let manifest = TransformationManifest::build(&ir, &tree, &styles, &layout, &reporter);
    let json = manifest.to_json();
    assert_eq!(manifest.to_json(), json, "repeated serialization differs");
    assert!(json.contains("\"origin\": \"app.css\""));
    let geometry = manifest
        .stages
        .iter()
        .find(|stage| stage.name == "geometry")
        .unwrap();
    assert_eq!(geometry.counts["laid_out_boxes"], 4); // html + body + two divs
    assert_eq!(manifest.diagnostics.total, 0);
}

#[test]
fn provenance_records_the_winning_selector() {
    let (tree, styles, _, _) = pipeline("div { height: 10px } .a { height: 20px }", PAGE);
    let node = by_class(&tree, "a");
    let env = environment();
    let provenance = provenance::collect(
        &ingest_to_ir("app.css", "div { height: 10px } .a { height: 20px }"),
        &tree,
        &env,
    );
    let height = provenance.get(node, "height").unwrap();
    // .a (0,1,0) beats div (0,0,1).
    assert_eq!(height.selector, ".a");
    assert_eq!(styles.get(node).unwrap().get("height"), Some("20px"));
}

#[test]
fn unresolvable_lengths_report_once_and_default_to_zero_boxes() {
    let (tree, _, layout, reporter) = pipeline(
        "div.a { height: 50px } div.b { padding-left: nonsense }",
        PAGE,
    );
    let b = layout.get(by_class(&tree, "b")).unwrap();
    // The unresolvable padding resolves to zero; the box still lays out.
    assert_eq!(b.x, 0.0);
    assert!(
        reporter
            .diagnostics()
            .iter()
            .any(|d| d.code == "CASA-GEO-001"),
        "expected a CASA-GEO-001 diagnostic: {:?}",
        reporter.diagnostics()
    );
}

#[test]
fn display_none_removes_the_box_and_its_subtree() {
    let (tree, _, layout, reporter) = pipeline(
        "div.hidden { display: none; height: 50px }",
        "<html><body><div class=\"hidden\"><span class=\"inner\"></span></div><div class=\"visible\"></div></body></html>",
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    assert!(layout.get(by_class(&tree, "hidden")).is_none());
    assert!(layout.get(by_class(&tree, "inner")).is_none());
    assert!(layout.get(by_class(&tree, "visible")).is_some());
}

#[test]
fn calc_and_shorthand_lengths_resolve_in_layout() {
    let (tree, _, layout, reporter) = pipeline(
        "div.a { height: calc(20px + 30px); margin: 5px 10px 15px 20px }",
        PAGE,
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    let a = layout.get(by_class(&tree, "a")).unwrap();
    assert_eq!(a.height, 50.0);
    assert_eq!(a.x, 20.0, "left margin applies");
}
