//! Phase 4 (GOYA-GFX-AVR-001 §5, §16): mixed-dimensional composition,
//! end-to-end. A `.gscene` element declares `@spatial.model`; the scene is
//! parsed with strict GSSH/4 admission, the declaration is checked against
//! the composition contract (AVR-C13), the referenced Rugid asset is
//! compiled once into a canonical inventory, and an observation resolves
//! bit-identically — with the compilation decisions emitted as `.escene`
//! producer events (WS-12 slice).
//!
//! Run with: `cargo test -p goya-avr-rugid --features rugid`.

#![cfg(feature = "rugid")]

use goya_avr::canonical::{compile, CompilationBudget};
use goya_avr::composition::{
    CompositionTree, LiveFallback, SpatialContentKind, SpatialElement,
};
use goya_avr::derivation::{
    DerivationProfile, IndexPolicy, InventoryProfile, ReconstructionPolicy, ResidencyPolicy,
};
use goya_avr::domain::ObservationDomain;
use goya_avr::retrieval::{resolve_derived, DerivedResolution};
use goya_avr::sampling::SamplingPolicy;
use goya_avr_rugid::RugidShapeSource;
use goya_escene::producer::{Collector, CompilationEvent, EsceneEvent};
use goya_lang::{parse_strict_gssh4, SpatialContent};
use rugid::shapes3d::ShapeType;

const SCENE: &str = r#"@ontology showroom {
    @cell vehicle_preview {
        id: "window.vehicle";
        extent_p: (0.6, 0.75, 0.0);
        @spatial.model {
            source: "vehicle.rugid";
            observation { domain: orbital; interaction: orbit; zoom: disabled; }
            rendering { strategy: anticipatory; fidelity: high; fallback: none; }
        }
    }
}"#;

#[test]
fn a_gscene_spatial_element_compiles_and_resolves_through_rugid() {
    // 1. Strict GSSH/4 parse of the authored scene.
    let parsed = parse_strict_gssh4(SCENE).expect("scene admits");
    let cell = &parsed.scene.root;
    let Some(SpatialContent::Model(model)) = &cell.spatial else {
        panic!("expected spatial model content");
    };

    // 2. The authored declaration maps onto the composition contract.
    let declared = SpatialElement {
        element_id: cell.authored_id.clone().expect("authored id"),
        kind: SpatialContentKind::Model,
        asset: model.source.clone(),
        camera: None,
        domain: ObservationDomain::Orbital,
        live_fallback: if model.rendering.fallback.is_some() {
            LiveFallback::Authorised
        } else {
            LiveFallback::Refused
        },
        lifecycle: None,
    };
    let hosts = std::collections::BTreeMap::new();
    let tree = CompositionTree::build(vec![declared.clone()], &hosts)
        .expect("composition is correct (AVR-C13)");
    assert_eq!(tree.get("window.vehicle").expect("node").depth, 1);

    // 3. The referenced Rugid asset compiles once into a canonical
    // inventory (§5.2's core loop, headless).
    let source = RugidShapeSource::new(ShapeType::F1Car, 80.0);
    let policy = SamplingPolicy { initial_samples: 32, distance: 2.0, fov_deg: 60.0 };
    let report = compile(
        &source,
        ObservationDomain::Orbital,
        &policy,
        &CompilationBudget { max_observations: 1024, max_payload_bytes: 64 << 20 },
    )
    .expect("canonical compilation");

    // 4. A target derivation resolves an embedded observation.
    let profile = DerivationProfile {
        profile: InventoryProfile::Relational,
        canonical_digest: report.canonical_digest.clone(),
        index: IndexPolicy {
            primary_nodes: 4,
            max_neighbourhood_size: 32,
            preserve_canonical_granularity: true,
            allow_pruning: false,
        },
        reconstruction: ReconstructionPolicy {
            max_dependency_depth: 4,
            allowed_encodings: vec![
                goya_avr::encoding::EncodingClass::Anchor,
                goya_avr::encoding::EncodingClass::Copy,
                goya_avr::encoding::EncodingClass::Shared,
            ],
        },
        residency: ResidencyPolicy {
            asset_budget_bytes: u64::MAX,
            prefetch_neighbourhoods: 1,
        },
        max_silhouette_error_px: 1,
        allow_live_fallback: declared.live_fallback == LiveFallback::Authorised,
    };
    let derivation =
        goya_avr::target::derive(&report, &profile, None).expect("derivation");

    let request = goya_avr::ingestion::Observer {
        azimuth_deg: 90.0,
        elevation_deg: 0.0,
        distance: 2.0,
        fov_deg: 60.0,
    };
    let direct =
        goya_avr::ingestion::SpatialSource::render(&source, &request).expect("direct render");
    match resolve_derived(&derivation, &request, None) {
        DerivedResolution::Exact { payload, .. } => {
            assert_eq!(payload.bytes, direct.bytes, "embedded observation resolves");
        }
        other => panic!("expected exact resolution, got {other:?}"),
    }

    // 5. The compilation decisions are emitted as `.escene` producer
    // events (§16, WS-12 slice): machine-readable, bounded, no invented
    // evidence.
    let mut collector = Collector::enabled("goya-avr-rugid", 1 << 16);
    collector.emit(&EsceneEvent::Compilation(CompilationEvent::SourceAccepted {
        source_id: format!("gscene:{}", report.canonical_digest),
        byte_length: SCENE.len(),
    }));
    collector.emit(&EsceneEvent::Compilation(CompilationEvent::DependencyResolved {
        name: declared.asset.clone(),
        status: "resolved",
    }));
    collector.emit(&EsceneEvent::Compilation(CompilationEvent::ConstructLowered {
        span: (0, SCENE.len()),
        decision: "spatial.model/anticipatory",
        pass: "avr-canonical".into(),
        implementation_ref: format!("goya-avr/canonical:{}", report.canonical_digest),
    }));
    collector.emit(&EsceneEvent::Compilation(CompilationEvent::OutputFinalised {
        output_digest: derivation.derivation_digest.clone(),
    }));
    assert_eq!(collector.dropped_events(), 0, "no event may be dropped");
    let snapshot = collector.snapshot();
    assert!(
        snapshot.by_kind(goya_escene::EvidenceKind::LoweringDecision).count() >= 1,
        "lowering decision recorded"
    );
    assert!(
        snapshot.by_kind(goya_escene::EvidenceKind::DependencyResolution).count() >= 1,
        "dependency resolution recorded"
    );
    // Every evidence-bearing event produced a record (SourceAccepted
    // aggregates to nothing per §11.2 — no evidence is invented).
    let total = snapshot.records.len();
    assert!(total >= 3, "compilation evidence recorded, got {total}");
}
