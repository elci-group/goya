//! Phase 6 (GOYA-GFX-AVR-001 §22: environmental expansion), end to end.
//! A `.gscene` viewport declares a navigation-graph environment; the scene
//! is parsed with strict GSSH/4 admission, the viewport (and a *nested*
//! viewport, §5.1 environment→environment) is validated through the
//! composition contract, the environment is compiled once per node over
//! real Rugid geometry, certified (AVR-C02/C03/C06), derived relationally,
//! and requests resolve bit-identically — including a transition through
//! the door edge between rooms.
//!
//! Run with: `cargo test -p goya-avr-rugid --features rugid`.

#![cfg(feature = "rugid")]

use goya_avr::canonical::CompilationBudget;
use goya_avr::composition::{CompositionClass, CompositionTree, SpatialElement, SpatialContentKind};
use goya_avr::domain::ObservationDomain;
use goya_avr::ingestion::Observer;
use goya_avr::navigation::{
    adjacent_nodes, certify_navigation, compile_navigation, observations_per_node,
    resolve_navigation, NavigationRequest, NavigationSpec,
};
use goya_avr::retrieval::{DerivedResolution, UnsupportedReason};
use goya_avr::navigation::EnvironmentalSource as _;
use goya_avr::sampling::SamplingPolicy;
use goya_avr_rugid::{RugidEnvironmentRoom, RugidNavigationSource};
use goya_lang::{parse_strict_gssh4, SpatialContent};
use rugid::shapes3d::ShapeType;

const SCENE: &str = r#"@ontology tour {
    @cell property_explorer {
        id: "window.tour";
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
fn a_gscene_navigation_viewport_compiles_certifies_and_resolves_through_rugid() {
    // 1. Strict GSSH/4 parse: the environmental viewport declaration.
    let parsed = parse_strict_gssh4(SCENE).expect("scene admits");
    let cell = &parsed.scene.root;
    let Some(SpatialContent::Viewport(viewport)) = &cell.spatial else {
        panic!("expected spatial viewport content");
    };
    assert_eq!(viewport.camera, "visitor");

    // 2. Composition: the authored viewport maps onto the contract, and a
    // nested viewport (§5.1 environment→environment) is admitted at
    // depth 2 with the nesting budget enforced.
    let outer = SpatialElement {
        element_id: cell.authored_id.clone().expect("authored id"),
        kind: SpatialContentKind::Viewport,
        asset: viewport.environment.clone(),
        camera: Some(viewport.camera.clone()),
        domain: ObservationDomain::NavigationGraph { nodes: 8 },
        live_fallback: if viewport.rendering.fallback.is_some() {
            goya_avr::composition::LiveFallback::Authorised
        } else {
            goya_avr::composition::LiveFallback::Refused
        },
        lifecycle: Some(goya_avr::composition::LifecyclePolicy {
            simulation: goya_avr::composition::SimulationPolicy::Independent,
            rendering: goya_avr::composition::RenderingPolicy::VisibilityBound,
        }),
    };
    let mut inner = outer.clone();
    inner.element_id = "window.tour.interior".into();
    inner.asset = "interior.rugid".into();
    let hosts = std::collections::BTreeMap::from([(
        "window.tour.interior".to_string(),
        "window.tour".to_string(),
    )]);
    let tree = CompositionTree::build(vec![outer, inner], &hosts)
        .expect("nested viewports compose (AVR-C13)");
    let outer_node = tree.get("window.tour").expect("outer node");
    let inner_node = tree.get("window.tour.interior").expect("inner node");
    assert_eq!(outer_node.depth, 1);
    assert_eq!(inner_node.depth, 2);
    assert_eq!(inner_node.class, CompositionClass::ElementToSpatial);

    // 3. The environment: two rooms over real Rugid models — a cube
    // "atrium" (4 nodes) and an F1 car "garage" (4 nodes) — joined by a
    // door edge between nodes 3 and 4. Camera positions shift the model
    // across each room; every node renders genuinely different geometry.
    let environment = RugidNavigationSource::new(vec![
        RugidEnvironmentRoom {
            shape: ShapeType::Cube,
            base_size: 60.0,
            node_positions: vec![(-45.0, 0.0, 0.0), (-15.0, 0.0, 0.0), (15.0, 0.0, 0.0), (45.0, 0.0, 0.0)],
        },
        RugidEnvironmentRoom {
            shape: ShapeType::F1Car,
            base_size: 80.0,
            node_positions: vec![(-45.0, 0.0, 0.0), (-15.0, 0.0, 0.0), (15.0, 0.0, 0.0), (45.0, 0.0, 0.0)],
        },
    ]);
    let spec = NavigationSpec {
        rooms: vec![4, 4],
        edges: vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7)],
    };
    let policy = SamplingPolicy { initial_samples: 4, distance: 2.0, fov_deg: 60.0 };
    let budget = CompilationBudget { max_observations: 64, max_payload_bytes: 64 << 20 };
    let report = compile_navigation(&environment, &spec, &policy, &budget).expect("compiles");
    assert_eq!(report.inventory.observations.len(), 32);

    // 4. Certification: AVR-C02 (every node compiled), AVR-C03 (valid
    // navigation references), AVR-C06 (room + environment connectivity).
    certify_navigation(&report, &spec).expect("environment certifies");
    assert_eq!(observations_per_node(&report), 4);

    // 5. Relational derivation from the single canonical compilation.
    let profile = goya_avr::derivation::DerivationProfile {
        profile: goya_avr::derivation::InventoryProfile::Relational,
        canonical_digest: report.canonical_digest.clone(),
        index: goya_avr::derivation::IndexPolicy {
            primary_nodes: 8,
            max_neighbourhood_size: 8,
            preserve_canonical_granularity: true,
            allow_pruning: false,
        },
        reconstruction: goya_avr::derivation::ReconstructionPolicy {
            max_dependency_depth: 4,
            allowed_encodings: vec![
                goya_avr::encoding::EncodingClass::Anchor,
                goya_avr::encoding::EncodingClass::Copy,
                goya_avr::encoding::EncodingClass::Shared,
            ],
        },
        residency: goya_avr::derivation::ResidencyPolicy {
            asset_budget_bytes: u64::MAX,
            prefetch_neighbourhoods: 1,
        },
        max_silhouette_error_px: 1,
        allow_live_fallback: false,
    };
    let derivation =
        goya_avr::target::derive(&report, &profile, None).expect("derivation");

    // 6. A request at a node resolves bit-identically to a direct render.
    let view = Observer { azimuth_deg: 90.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 };
    let direct = environment.render_node(6, &view).expect("direct render");
    match resolve_navigation(&derivation, &NavigationRequest { node: 6, view }, None) {
        DerivedResolution::Exact { payload, .. } => {
            assert_eq!(payload.bytes, direct.bytes);
        }
        other => panic!("expected exact resolution, got {other:?}"),
    }

    // 7. Navigation through the door edge: node 3's adjacency reaches into
    // room 1 (§10.1 locality accelerates movement but never restricts it).
    let per_node = observations_per_node(&report);
    let neighbours_of_3 = adjacent_nodes(&report.inventory.navigation.edges, 3, per_node);
    assert!(neighbours_of_3.contains(&4), "door edge crosses the room boundary");
    assert!(neighbours_of_3.contains(&2), "intra-room movement");
    // The far node is still resolvable — locality is an acceleration,
    // not a restriction.
    let far = environment.render_node(0, &view).expect("direct render");
    match resolve_navigation(&derivation, &NavigationRequest { node: 0, view }, None) {
        DerivedResolution::Exact { payload, .. } => assert_eq!(payload.bytes, far.bytes),
        other => panic!("expected exact resolution, got {other:?}"),
    }

    // 8. AVR-C14: requests outside the environment are refused explicitly.
    assert_eq!(
        resolve_navigation(&derivation, &NavigationRequest { node: 8, view }, None),
        DerivedResolution::Unsupported(UnsupportedReason::OutsideDomain)
    );
}
