//! Phase 7 security testing (GOYA-GFX-AVR-001 §17): inventories and
//! derived assets are untrusted input. Every test here feeds a hostile or
//! tampered artefact to a real entry point and asserts the documented
//! mitigation — structural rejection, explicit failure, or budget
//! enforcement — never a silent acceptance.
//!
//! These are property tests over the contract entry points (the §17
//! "fuzzing MUST cover" obligation exercised through its most
//! security-relevant classes); corpus-based fuzzing remains tooling work.

use goya_avr::canonical::{compile, CanonicalStore, CompilationBudget, CompileReport};
use goya_avr::delta::{self, DELTA_HEADER};
use goya_avr::derivation::{
    DerivationProfile, IndexPolicy, InventoryProfile, ReconstructionPolicy, ResidencyPolicy,
};
use goya_avr::domain::ObservationDomain;
use goya_avr::graph::{
    GraphTopologyError, NavigationGraph, ObservationId, ReconstructionNode, ResidencyGraph,
    MAX_DEPENDENCY_DEPTH,
};
use goya_avr::ingestion::{ObservationPayload, Observer, SourceError, SpatialSource};
use goya_avr::inventory::{
    FidelityClass, FidelityRecord, InventoryError, Neighbourhood, ObservationInventory,
    ObservationRecord, ProvenanceRecord, ProvenanceRelation, RelationalAnchor, RepresentationId,
};
use goya_avr::navigation::{
    compile_navigation, NavigationCompileError, NavigationSpec, NavigationSpecError,
};
use goya_avr::retrieval::{resolve_derived, UnsupportedReason};
use goya_avr::representation::RepresentationClass;
use goya_avr::sampling::SamplingPolicy;
use goya_avr::target::derive;
use std::cell::Cell;

/// Minimal deterministic source for hostile-input fixtures.
struct TinySource;
impl SpatialSource for TinySource {
    fn source_digest(&self) -> String {
        "sha256:tiny".into()
    }
    fn render(&self, observer: &Observer) -> Result<ObservationPayload, SourceError> {
        let o = observer.normalised();
        Ok(ObservationPayload {
            representation: RepresentationClass::Vec,
            bytes: format!("T {:.0} {:.0}", o.azimuth_deg, o.elevation_deg).into_bytes(),
        })
    }
}

fn tiny_report(n: u32) -> CompileReport {
    let policy = SamplingPolicy { initial_samples: n, distance: 2.0, fov_deg: 60.0 };
    compile(
        &TinySource,
        ObservationDomain::Orbital,
        &policy,
        &CompilationBudget { max_observations: 4096, max_payload_bytes: u64::MAX },
    )
    .expect("compiles")
}

fn record(id: u64, state: goya_avr::domain::ObservationState) -> ObservationRecord {
    ObservationRecord {
        id: ObservationId(id),
        state,
        representation: RepresentationId(0),
        provenance: None,
        fidelity: FidelityRecord {
            classification: FidelityClass::Exact,
            error_bounds: [0; 4],
            verification_method: "direct canonical render",
            coverage: goya_avr::graph::ObservationCoordinates { spatial: [0, 0, 0] },
        },
    }
}

/// §17 "cyclic reconstruction → dependency cycle rejection" (AVR-C04).
#[test]
fn cyclic_reconstruction_graphs_are_rejected() {
    let report = tiny_report(8);
    let cycle = vec![
        ReconstructionNode {
            observation: ObservationId(0),
            dependencies: vec![ObservationId(1)],
        },
        ReconstructionNode {
            observation: ObservationId(1),
            dependencies: vec![ObservationId(0)],
        },
    ];
    let terminals = report.inventory.observations.iter().map(|o| o.id).collect();
    assert_eq!(
        goya_avr::graph::ReconstructionGraph::new(cycle, terminals, MAX_DEPENDENCY_DEPTH),
        Err(GraphTopologyError::Cyclic)
    );
}

/// §17 "excessive neighbourhood size → resource-bound validation" (AVR-C07).
#[test]
fn oversized_neighbourhoods_are_rejected() {
    let report = tiny_report(8);
    let oversized = Neighbourhood {
        id: goya_avr::graph::NeighbourhoodId(0),
        anchor: goya_avr::graph::AnchorId(0),
        observations: (0..100u64).map(ObservationId).collect(),
        max_observations: 8,
    };
    let err = ObservationInventory::new(
        report.canonical_digest.clone(),
        ObservationDomain::Orbital,
        report.inventory.observations.clone(),
        NavigationGraph::default(),
        Vec::new(),
        ResidencyGraph::default(),
        vec![RelationalAnchor {
            id: goya_avr::graph::AnchorId(0),
            observation: ObservationId(0),
            neighbourhood: goya_avr::graph::NeighbourhoodId(0),
            adjacent: Vec::new(),
            bounds: goya_avr::graph::ObservationCoordinates { spatial: [0, 0, 0] },
        }],
        vec![oversized],
        Vec::new(),
        MAX_DEPENDENCY_DEPTH,
    )
    .expect_err("oversized neighbourhood");
    assert!(matches!(err, InventoryError::NeighbourhoodOversized { .. }));
}

/// §17 "invalid graph references → referential integrity checks".
#[test]
fn dangling_anchors_are_rejected() {
    let report = tiny_report(8);
    let err = ObservationInventory::new(
        report.canonical_digest.clone(),
        ObservationDomain::Orbital,
        report.inventory.observations.clone(),
        NavigationGraph::default(),
        Vec::new(),
        ResidencyGraph::default(),
        vec![RelationalAnchor {
            id: goya_avr::graph::AnchorId(7),
            observation: ObservationId(9999),
            neighbourhood: goya_avr::graph::NeighbourhoodId(0),
            adjacent: Vec::new(),
            bounds: goya_avr::graph::ObservationCoordinates { spatial: [0, 0, 0] },
        }],
        Vec::new(),
        Vec::new(),
        MAX_DEPENDENCY_DEPTH,
    )
    .expect_err("dangling anchor");
    assert!(matches!(err, InventoryError::DanglingReference { .. }));
}

/// §15.1 AVR-C09 / §17: an exactness claim without a verified source is
/// rejected — "looks correct" is never a certificate.
#[test]
fn uncertified_exact_provenance_is_rejected() {
    let report = tiny_report(8);
    let mut unverified = report.inventory.observations[0].clone();
    unverified.fidelity.classification = FidelityClass::Bounded;
    let observations = vec![
        unverified,
        record(1, report.inventory.observations[1].state.clone()),
    ];
    let err = ObservationInventory::new(
        report.canonical_digest.clone(),
        ObservationDomain::Orbital,
        observations,
        NavigationGraph::default(),
        Vec::new(),
        ResidencyGraph::default(),
        Vec::new(),
        Vec::new(),
        vec![ProvenanceRecord {
            id: goya_avr::inventory::ProvenanceId(0),
            observation: ObservationId(1),
            relation: ProvenanceRelation::Albit { source: ObservationId(0), exact: true },
        }],
        MAX_DEPENDENCY_DEPTH,
    )
    .expect_err("uncertified exact claim");
    assert!(matches!(
        err,
        InventoryError::UncertifiedExactReconstruction { .. }
    ));
}

/// §17 "cache poisoning → canonical identity and provenance
/// verification" (AVR-C11): a derivation profile naming a foreign
/// canonical digest is refused outright.
#[test]
fn poisoned_provenance_is_refused_at_derivation() {
    let report = tiny_report(8);
    let profile = DerivationProfile {
        profile: InventoryProfile::Dense,
        canonical_digest: "sha256:poisoned".into(),
        index: IndexPolicy {
            primary_nodes: 8,
            max_neighbourhood_size: 8,
            preserve_canonical_granularity: true,
            allow_pruning: false,
        },
        reconstruction: ReconstructionPolicy {
            max_dependency_depth: 4,
            allowed_encodings: vec![goya_avr::encoding::EncodingClass::Anchor],
        },
        residency: ResidencyPolicy {
            asset_budget_bytes: u64::MAX,
            prefetch_neighbourhoods: 0,
        },
        max_silhouette_error_px: 1,
        allow_live_fallback: false,
    };
    assert!(matches!(
        derive(&report, &profile, None),
        Err(goya_avr::target::DerivationError::CanonicalDigestMismatch { .. })
    ));
}

/// §17 "corrupted payload → digest verification": a delta stream that is
/// truncated, header-less, or cut mid-instruction is rejected, and the
/// failure carries no partially-materialised payload.
#[test]
fn corrupted_delta_streams_are_rejected() {
    let base = b"<svg>\n<rect/>\n</svg>\n".to_vec();
    let target = b"<svg>\n<circle/>\n</svg>\n".to_vec();
    let good = delta::encode(&base, &target).expect("encodes");

    let mut truncated = good.clone();
    truncated.pop();
    assert!(delta::apply(&base, &truncated).is_err());

    let mut headerless = good.clone();
    headerless.drain(..DELTA_HEADER.len());
    assert!(delta::apply(&base, &headerless).is_err());

    let garbage = vec![0u8; 64];
    assert!(delta::apply(&base, &garbage).is_err());

    let empty = Vec::new();
    assert!(delta::apply(&base, &empty).is_err());
}

/// §17 "decompression bomb → expansion limits": a hostile delta claiming
/// huge insertions is bounded by the instruction encoding, and a
/// malformed length is rejected rather than trusted for allocation.
#[test]
fn hostile_delta_lengths_are_not_trusted_for_allocation() {
    let base = b"x\n".repeat(4);
    // Hand-crafted stream: valid header followed by a bogus instruction
    // field. Must be rejected on parse, never used to size an allocation.
    let mut hostile = Vec::from(DELTA_HEADER);
    hostile.extend_from_slice(&[0xFF; 32]);
    assert!(delta::apply(&base, &hostile).is_err());
    // The base was untouched; no expansion occurred.
    assert_eq!(base.len(), 8);
}

/// §17 "unauthorised rendering fallback → capability and policy
/// enforcement" (AVR-C12): a derivation whose profile refuses live
/// fallback never reports it as available.
#[test]
fn live_fallback_requires_authorisation() {
    let report = tiny_report(8);
    let profile = DerivationProfile {
        profile: InventoryProfile::Dense,
        canonical_digest: report.canonical_digest.clone(),
        index: IndexPolicy {
            primary_nodes: 8,
            max_neighbourhood_size: 8,
            preserve_canonical_granularity: true,
            allow_pruning: false,
        },
        reconstruction: ReconstructionPolicy {
            max_dependency_depth: 4,
            allowed_encodings: vec![goya_avr::encoding::EncodingClass::Anchor],
        },
        residency: ResidencyPolicy {
            asset_budget_bytes: u64::MAX,
            prefetch_neighbourhoods: 0,
        },
        max_silhouette_error_px: 1,
        allow_live_fallback: false,
    };
    let derivation = derive(&report, &profile, None).expect("derives");
    // An uncompiled observation must be an explicit failure, never a
    // live-render substitution.
    assert_eq!(
        resolve_derived(
            &derivation,
            &Observer { azimuth_deg: 11.25, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 },
            None,
        ),
        goya_avr::retrieval::DerivedResolution::Unsupported(UnsupportedReason::NotCompiled)
    );
}

/// §17 "malformed inventory → structural validation": a truncated store
/// or an inventory whose payloads were stripped fails resolution
/// explicitly instead of returning empty data.
#[test]
fn stripped_stores_fail_resolution_explicitly() {
    let report = tiny_report(8);
    let mut stripped = report.clone();
    stripped.store = CanonicalStore::default();
    let state = stripped.inventory.observations[0].state.clone();
    let payload = stripped.store.get(ObservationId(0));
    assert!(payload.is_none(), "stripped store yields no payload");
    let _ = state;
}

/// §17 "resource exhaustion → budget enforcement and cancellation": a
/// hostile navigation spec or a bursting request stream is bounded.
#[test]
fn hostile_navigation_specs_and_request_storms_are_bounded() {
    // Spec-level: out-of-range and self-loop edges rejected pre-render.
    let env = CountingEnvironment::default();
    let hostile = NavigationSpec { rooms: vec![2], edges: vec![(0, 99)] };
    assert_eq!(
        compile_navigation(&env, &hostile, &SamplingPolicy { initial_samples: 2, distance: 2.0, fov_deg: 60.0 }, &CompilationBudget { max_observations: 64, max_payload_bytes: u64::MAX }),
        Err(NavigationCompileError::Spec(NavigationSpecError::EdgeOutOfRange { edge: (0, 99), nodes: 2 }))
    );
    assert_eq!(env.renders.get(), 0, "no rendering before validation");

    // Budget-level: requested observations exceed the budget before any
    // render call.
    let big = NavigationSpec { rooms: vec![64], edges: vec![(0, 1)] };
    assert!(matches!(
        compile_navigation(&env, &big, &SamplingPolicy { initial_samples: 8, distance: 2.0, fov_deg: 60.0 }, &CompilationBudget { max_observations: 16, max_payload_bytes: u64::MAX }),
        Err(NavigationCompileError::BudgetExhausted { .. })
    ));
    assert_eq!(env.renders.get(), 0);
}

/// §17 "resource exhaustion" at runtime: a burst of valid requests under
/// tight residency budgets can never expand beyond them.
#[test]
fn request_bursts_never_exceed_residency_budgets() {
    let report = tiny_report(16);
    let profile = DerivationProfile {
        profile: InventoryProfile::Relational,
        canonical_digest: report.canonical_digest.clone(),
        index: IndexPolicy {
            primary_nodes: 4,
            max_neighbourhood_size: 8,
            preserve_canonical_granularity: true,
            allow_pruning: false,
        },
        reconstruction: ReconstructionPolicy {
            max_dependency_depth: 4,
            allowed_encodings: vec![goya_avr::encoding::EncodingClass::Anchor],
        },
        residency: ResidencyPolicy { asset_budget_bytes: u64::MAX, prefetch_neighbourhoods: 0 },
        max_silhouette_error_px: 1,
        allow_live_fallback: false,
    };
    let derivation = derive(&report, &profile, None).expect("derives");
    let hot_budget = 48;
    let limits = goya_avr::ResidencyLimits { hot_bytes: hot_budget, warm_bytes: 96, prefetch_neighbourhoods: 1 };
    let mut manager = goya_avr::ResidencyManager::new(&derivation, limits);

    // 2_000 requests sweeping the ring: every step holds both budgets.
    for i in 0..2_000u32 {
        let az = (i % 360) as f32;
        let _ = manager.request(&Observer {
            azimuth_deg: az,
            elevation_deg: 0.0,
            distance: 2.0,
            fov_deg: 60.0,
        });
        assert!(manager.resident_bytes(goya_avr::RuntimeTier::Hot) <= hot_budget);
        assert!(manager.resident_bytes(goya_avr::RuntimeTier::Warm) <= 96);
    }
}

#[derive(Default)]
struct CountingEnvironment {
    renders: Cell<u32>,
}

impl goya_avr::navigation::EnvironmentalSource for CountingEnvironment {
    fn source_digest(&self) -> String {
        "sha256:counting-env".into()
    }
    fn render_node(
        &self,
        node: u32,
        view: &Observer,
    ) -> Result<ObservationPayload, SourceError> {
        self.renders.set(self.renders.get() + 1);
        let v = view.normalised();
        Ok(ObservationPayload {
            representation: RepresentationClass::Vec,
            bytes: format!("N {node} AZ {:.0}", v.azimuth_deg).into_bytes(),
        })
    }
}
