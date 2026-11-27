//! Phase 4 (GOYA-GFX-AVR-001 §5): `@spatial.model` / `@spatial.viewport`
//! grammar and strict-admission corpus.
//!
//! Note: `cargo test -p goya-lang` currently fails on the unrelated
//! in-flight `chromatics_corpus.rs` target; run this corpus with
//! `cargo test -p goya-lang --test spatial_corpus`.

use goya_lang::{
    parse_strict, parse_strict_gssh3, parse_strict_gssh3_live, parse_strict_gssh4,
    parse_strict_gssh4_live, parse_with_source_map, SpatialContent, SpatialDomain,
};

const MODEL_SCENE: &str = r#"@ontology demo {
    @cell vehicle_preview {
        id: "window.vehicle";
        extent_p: (0.6, 0.75, 0.0);
        @spatial.model {
            source: "vehicle.rugid";
            observation { domain: spherical; interaction: orbit; zoom: enabled; }
            rendering { strategy: anticipatory; fidelity: high; fallback: live; }
        }
    }
}"#;

const VIEWPORT_SCENE: &str = r#"@ontology demo {
    @cell property_explorer {
        id: "window.property";
        extent_p: (0.8, 1.0, 0.0);
        @spatial.viewport {
            environment: "property.rugid";
            camera: "visitor";
            observation { domain: navigation_graph; movement: constrained; }
            rendering { strategy: hybrid; anticipation: enabled; }
            lifecycle { simulation: independent; rendering: visibility_bound; }
        }
    }
}"#;

#[test]
fn embedded_model_parses_with_declared_intent() {
    let parsed = parse_with_source_map(MODEL_SCENE).expect("model scene parses");
    let cell = &parsed.scene.root;
    let Some(SpatialContent::Model(model)) = &cell.spatial else {
        panic!("expected spatial model content");
    };
    assert_eq!(model.source, "vehicle.rugid");
    assert_eq!(model.observation.domain, SpatialDomain::Spherical);
    assert!(model.observation.zoom);
    assert_eq!(model.rendering.strategy, goya_lang::SpatialStrategy::Anticipatory);
    assert_eq!(model.rendering.fidelity, Some(goya_lang::SpatialFidelity::High));
    assert_eq!(model.rendering.fallback, Some(goya_lang::SpatialFallback::Live));
}

#[test]
fn environmental_viewport_parses_with_camera_and_lifecycle() {
    let parsed = parse_with_source_map(VIEWPORT_SCENE).expect("viewport scene parses");
    let cell = &parsed.scene.root;
    let Some(SpatialContent::Viewport(viewport)) = &cell.spatial else {
        panic!("expected spatial viewport content");
    };
    assert_eq!(viewport.environment, "property.rugid");
    assert_eq!(viewport.camera, "visitor");
    assert_eq!(viewport.observation.domain, SpatialDomain::NavigationGraph);
    assert!(viewport.rendering.anticipation);
    assert_eq!(viewport.lifecycle.simulation, goya_lang::SpatialSimulation::Independent);
    assert_eq!(
        viewport.lifecycle.rendering,
        goya_lang::SpatialRenderingPolicy::VisibilityBound
    );
}

#[test]
fn spatial_content_requires_gssh4_admission() {
    // Earlier profiles refuse the declaration rather than silently
    // flattening a spatial element into an ordinary one (§4.3).
    for parse in [
        parse_strict as fn(&str) -> Result<goya_lang::ParsedScene, goya_lang::StrictParseError>,
        parse_strict_gssh3,
    ] {
        assert_eq!(parse(MODEL_SCENE).unwrap_err().code, "GY-E1019");
        assert_eq!(parse(VIEWPORT_SCENE).unwrap_err().code, "GY-E1019");
    }
    assert_eq!(
        parse_strict_gssh3_live(MODEL_SCENE).unwrap_err().code,
        "GY-E1019"
    );
    // GSSH/4 profiles admit it.
    parse_strict_gssh4(MODEL_SCENE).expect("gssh4 admits model");
    parse_strict_gssh4(VIEWPORT_SCENE).expect("gssh4 admits viewport");
    parse_strict_gssh4_live(VIEWPORT_SCENE).expect("gssh4-live admits viewport");
}

#[test]
fn unknown_spatial_keys_are_refused() {
    let bad = MODEL_SCENE.replace("zoom: enabled;", "zoom: sideways;");
    assert!(parse_with_source_map(&bad).is_err());
    let bad_key = MODEL_SCENE.replace("source: \"vehicle.rugid\";", "src: \"vehicle.rugid\";");
    assert!(parse_with_source_map(&bad_key).is_err());
    // `movement` is a viewport key, not a model key.
    let wrong_kind = MODEL_SCENE.replace("interaction: orbit;", "movement: constrained;");
    assert!(parse_with_source_map(&wrong_kind).is_err());
    // `interaction` is a model key, not a viewport key.
    let wrong_kind_vp = VIEWPORT_SCENE.replace("movement: constrained;", "interaction: orbit;");
    assert!(parse_with_source_map(&wrong_kind_vp).is_err());
}

#[test]
fn missing_required_spatial_declarations_are_refused() {
    // A model without a source names no asset.
    let no_source = MODEL_SCENE.replace("            source: \"vehicle.rugid\";\n", "");
    assert!(parse_with_source_map(&no_source).is_err());
    // A viewport without a camera cannot bind independent camera state.
    let no_camera = VIEWPORT_SCENE.replace("            camera: \"visitor\";\n", "");
    assert!(parse_with_source_map(&no_camera).is_err());
    // A viewport without a lifecycle has not decided the §5.3 split.
    let no_lifecycle = VIEWPORT_SCENE.replace(
        "            lifecycle { simulation: independent; rendering: visibility_bound; }\n",
        "",
    );
    assert!(parse_with_source_map(&no_lifecycle).is_err());
}

#[test]
fn duplicate_spatial_blocks_are_refused() {
    let dup = MODEL_SCENE.replace(
        "        }\n    }\n}",
        "        }\n        @spatial.model { source: \"b.rugid\"; observation { domain: orbital; } rendering { strategy: hybrid; } }\n    }\n}",
    );
    assert!(parse_with_source_map(&dup).is_err());
}

#[test]
fn the_compatibility_parse_carries_spatial_intent() {
    // `parse` returns the Scene including spatial content; admission
    // policy is enforced by the strict profiles, not by the AST.
    let scene = goya_lang::parse(MODEL_SCENE).expect("compatibility parse");
    assert!(scene.root.spatial.is_some());
}
