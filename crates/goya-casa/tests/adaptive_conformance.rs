//! Phase 4 exit criteria: adaptive presentation and behavioural state,
//! end to end — responsive CSS evaluates into typed records, container
//! rules defer to the host with diagnostics, breakpoints collapse into
//! one ladder, interaction states extract per element, transitions parse
//! into typed values and keyframe animations classify for precomputation.

use goya_casa::adaptive::breakpoint;
use goya_casa::adaptive::container_queries;
use goya_casa::adaptive::interaction_adaptation::{self};
use goya_casa::adaptive::media_queries;
use goya_casa::animation::{classify, parse_keyframes, Classification, Easing};
use goya_casa::behavioural::explosion_prevention::{self};
use goya_casa::behavioural::state_extraction;
use goya_casa::behavioural::state_space;
use goya_casa::behavioural::transitions::{self};
use goya_casa::cascade::conditional::{ColorScheme, Environment, MediaType};
use goya_casa::context::computed_style::{compute_styles, ComputedStyles};
use goya_casa::context::structural::SceneTree;
use goya_casa::diagnostics::codes;
use goya_casa::diagnostics::reporter::Reporter;
use goya_casa::geometry::Context;
use goya_casa::ingestion::parser::ingest_to_ir;
use goya_casa::ir::casa_ir::CasaRule;

fn environment() -> Environment {
    Environment {
        viewport_width: Some(800.0),
        viewport_height: Some(600.0),
        media_type: MediaType::Screen,
        color_scheme: Some(ColorScheme::Dark),
    }
}

fn rules_of(css: &str) -> Vec<CasaRule> {
    let ir = ingest_to_ir("app.css", css);
    ir.sheets
        .first()
        .map(|sheet| sheet.rules.clone())
        .unwrap_or_default()
}

fn computed(css: &str, html: &str) -> (SceneTree, ComputedStyles, Reporter) {
    let ir = ingest_to_ir("app.css", css);
    let (tree, _) = SceneTree::from_html(html);
    let mut reporter = Reporter::new();
    let styles = compute_styles(&ir, &tree, &environment(), &mut reporter);
    (tree, styles, reporter)
}

fn node_by_name<'a>(
    tree: &SceneTree,
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
fn nested_media_cascades_inner_rule() {
    // Phase 2 regression guard: a `@media` inside a `@media` applies its
    // inner rules when both conditions hold in the environment.
    let (tree, styles, reporter) = computed(
        "p { color: red } \
         @media (min-width: 400px) { @media (min-height: 300px) { p { color: blue } } }",
        r#"<html><body><p>hi</p></body></html>"#,
    );
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
    assert_eq!(node_by_name(&tree, &styles, "p").get("color"), Some("blue"));
}

#[test]
fn container_rules_defer_with_diagnostic() {
    let mut reporter = Reporter::new();
    let rules = rules_of("@container card (min-width: 400px) { p { color: teal } }");
    let constraints = container_queries::collect_constraints(&rules, &mut reporter);
    assert_eq!(constraints.len(), 1);
    assert_eq!(constraints[0].name.as_deref(), Some("card"));
    assert_eq!(constraints[0].condition, "(min-width: 400px)");
    assert_eq!(constraints[0].gated_selectors, vec!["p".to_string()]);
    assert_eq!(
        reporter.diagnostics()[0].code.as_str(),
        codes::ADAPTIVE_CONTAINER_DEFERRED.code
    );

    // The gated rule is excluded from cascade-time matching: the element
    // keeps only the non-container styling.
    let (tree, styles, _) = computed(
        "p { color: olive } @container card (min-width: 400px) { p { color: teal } }",
        r#"<html><body><p>hi</p></body></html>"#,
    );
    assert_eq!(
        node_by_name(&tree, &styles, "p").get("color"),
        Some("olive")
    );
}

#[test]
fn media_query_records_match_under_environment() {
    let mut reporter = Reporter::new();
    let record = media_queries::evaluate("(min-width: 400px)", &environment(), &mut reporter);
    assert!(record.matched);
    assert!(record.evaluated);
    let record = media_queries::evaluate("(hover: hover)", &environment(), &mut reporter);
    assert!(!record.matched);
    assert!(!record.evaluated);
    assert_eq!(
        record.deferred_features.iter().cloned().collect::<Vec<_>>(),
        vec!["hover".to_string()]
    );
}

#[test]
fn breakpoints_collapse_to_one_ladder() {
    let rules = rules_of(
        "@media (min-width: 400px) { .a { color: red } } \
         @media (min-width: 800px) { .b { color: blue } } \
         @media (max-width: 400px) { .c { color: green } } \
         @media (orientation: landscape) { .d { color: gold } }",
    );
    let set = breakpoint::collect(&rules);
    assert_eq!(set.widths, vec![400.0, 800.0]);
    assert_eq!(
        set.non_width_conditions,
        vec!["(orientation: landscape)".to_string()]
    );
    assert_eq!(set.widest(), Some(800.0));
}

#[test]
fn interaction_states_extract_per_element() {
    let rules = rules_of(
        "button:hover { color: red } \
         button:disabled { color: gray } \
         @media (hover: hover) and (pointer: fine) { a:focus { outline: 1px } }",
    );
    let extracted = state_extraction::extract_element_states(&rules);
    assert_eq!(extracted.len(), 3);
    assert_eq!(extracted[0].selector, "button:hover");

    let requirements = interaction_adaptation::collect_capability_requirements(&rules);
    assert!(requirements.hover_consulted);
    assert!(requirements.pointer_consulted);

    let affordances = interaction_adaptation::collect_affordances(&rules);
    assert_eq!(affordances.len(), 3);
    assert_eq!(
        affordances[2].selector, "a:focus",
        "affordances keep source order across group rules"
    );
}

#[test]
fn node_states_surface_from_the_scene_tree() {
    let (tree, _styles, _reporter) = computed(
        "button:hover { color: red }",
        r#"<html><body><button>go</button></body></html>"#,
    );
    let mut tree = tree;
    let button = tree
        .walk()
        .into_iter()
        .find(|node| tree.get(*node).name == "button")
        .expect("button in fixture");
    tree.set_state(button, "hover");
    let by_state = state_extraction::extract_node_states(&tree);
    assert_eq!(
        by_state.get("hover").map(Vec::as_slice),
        Some(&["button".to_string()][..])
    );
}

#[test]
fn state_space_budget_collapses_explosions() {
    let rules = rules_of(
        "a:hover:focus:active:checked:disabled { color: red } \
         b:hover { color: blue }",
    );
    let space = state_space::build(&rules);
    assert_eq!(space.combinations(), 64);

    let mut reporter = Reporter::new();
    let enforced = explosion_prevention::enforce(&space, 8, &mut reporter);
    assert!(enforced.combinations() <= 8);
    assert_eq!(
        reporter.diagnostics()[0].code.as_str(),
        codes::BEHAVIOURAL_STATE_OVERFLOW.code
    );
}

#[test]
fn transitions_parse_from_ingested_css() {
    let rules = rules_of("a { transition: color 300ms ease-in 100ms, transform 250ms }");
    let mut declarations = Vec::new();
    for rule in &rules {
        if let CasaRule::Style(style) = rule {
            declarations.extend(style.declarations.clone());
        }
    }
    let mut reporter = Reporter::new();
    let parsed = transitions::parse(&declarations, &mut reporter);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].property, "color");
    assert!((parsed[0].duration - 0.3).abs() < 1e-9);
    assert_eq!(parsed[0].easing, Easing::EaseIn);
    assert!((parsed[0].delay - 0.1).abs() < 1e-9);
    assert_eq!(parsed[1].property, "transform");
    assert!((parsed[1].duration - 0.25).abs() < 1e-9);
    assert_eq!(parsed[1].easing, Easing::Linear);
}

#[test]
fn keyframe_animation_classifies_for_precomputation() {
    let rules = rules_of(
        "@keyframes fade { from { opacity: 0 } to { opacity: 1 } } \
         a { animation: fade 2s ease-in-out }",
    );
    let mut keyframes = None;
    for rule in &rules {
        if let CasaRule::Keyframes(kf) = rule {
            keyframes = Some(kf.clone());
        }
    }
    let keyframes = keyframes.expect("keyframes lower into CasaIR");
    let mut reporter = Reporter::new();
    let animation = parse_keyframes(&keyframes, &mut reporter);
    assert_eq!(animation.name, "fade");
    assert_eq!(animation.steps.len(), 2);

    let context = Context::default();
    let classification = classify(&animation, &context, &mut reporter);
    let trajectory = match classification {
        Classification::Precomputed(trajectory) => trajectory,
        Classification::Runtime { reason } => {
            panic!("simple opacity animation deferred: {reason:?}")
        }
    };
    assert!(!trajectory.samples.is_empty());
    assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
}
