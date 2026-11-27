//! Phase 5 (GOYA-GFX-AVR-001 §11, §14): hardware-adaptive execution over a
//! real Rugid asset, end-to-end. Caprichio strategy selection runs on device
//! capabilities (WS-11), the chosen profile derives from a canonical
//! inventory compiled **once**, and a runtime `ResidencyManager` services
//! observation requests with anchors hot, Cold transitions accounted
//! separately (§11.4) and byte budgets held against real SVG payloads
//! (WS-09).
//!
//! Run with: `cargo test -p goya-avr-rugid --features rugid`.

#![cfg(feature = "rugid")]

use goya_avr::canonical::{compile, CompilationBudget};
use goya_avr::caprichio::{select_strategy, Capabilities};
use goya_avr::domain::ObservationDomain;
use goya_avr::ingestion::{Observer, SpatialSource};
use goya_avr::residency::{RequestOutcome, ResidencyLimits, ResidencyManager, RuntimeTier};
use goya_avr::sampling::SamplingPolicy;
use goya_avr::target::Tier;
use goya_avr_rugid::RugidShapeSource;
use rugid::shapes3d::ShapeType;

#[test]
fn caprichio_selection_derives_and_serves_a_rugid_asset_within_budgets() {
    // 1. Canonical compilation runs exactly once (AVR-G03).
    let source = RugidShapeSource::new(ShapeType::F1Car, 80.0);
    let policy = SamplingPolicy { initial_samples: 144, distance: 2.0, fov_deg: 60.0 };
    let report = compile(
        &source,
        ObservationDomain::Orbital,
        &policy,
        &CompilationBudget { max_observations: 1024, max_payload_bytes: 64 << 20 },
    )
    .expect("canonical compilation");

    // 2. Caprichio selection from capabilities only (§14.2): a constrained
    // device (§19 set-top / television class) selects relational.
    let caps = Capabilities::constrained();
    let decision = select_strategy(&report.canonical_digest, &caps).expect("decision");
    assert_eq!(decision.profile.profile, goya_avr::derivation::InventoryProfile::Relational);
    assert!(!decision.may_compile_canonical_locally, "§14.3: weak device cannot re-render");

    // 3. Derivation from the canonical inventory — no rendering pass.
    let derivation =
        goya_avr::target::derive(&report, &decision.profile, None).expect("derivation");
    assert!(derivation
        .entries
        .values()
        .any(|e| e.tier() == Tier::Cold), "relational inventory ships non-resident members");

    // 4. Runtime residency: anchors start Hot; Cold requests transition and
    // then serve as resident lookups (§10.1, §11.4). Requests walk the
    // canonical orbital ring, reconstructed from the inventory's own
    // quantised states.
    let limits = ResidencyLimits { hot_bytes: 96 << 10, warm_bytes: 256 << 10, prefetch_neighbourhoods: 1 };
    let mut manager = ResidencyManager::new(&derivation, limits);

    let request = |az: f32| Observer {
        azimuth_deg: az,
        elevation_deg: 0.0,
        distance: 2.0,
        fov_deg: 60.0,
    };

    let mut ring: Vec<f32> = derivation
        .inventory
        .observations
        .iter()
        .map(|o| o.state.orientation[0] as f32 / 100.0)
        .collect();
    ring.sort_by(|a, b| a.partial_cmp(b).expect("finite azimuths"));

    let mut transitions = 0u64;
    let mut resident = 0u64;
    for az in ring.iter().step_by(4) {
        let az = *az;
        match manager.request(&request(az)).expect("in-domain request") {
            RequestOutcome::Transitioned { .. } => transitions += 1,
            RequestOutcome::Resident { .. } => resident += 1,
        }
        // §11.1: budgets are hard at every step, over real SVG payloads.
        assert!(manager.resident_bytes(RuntimeTier::Hot) <= limits.hot_bytes);
        assert!(manager.resident_bytes(RuntimeTier::Warm) <= limits.warm_bytes);
    }
    assert!(transitions > 0, "cold members were exercised");
    // Locality: a revisit within the retained hot set is an ordinary
    // resident lookup, not another neighbourhood transition (§10.1).
    let revisit = ring[(ring.len() - 1) / 4 * 4]; // last actually-visited index
    match manager.request(&request(revisit)).expect("revisit") {
        RequestOutcome::Resident { .. } => {}
        other => panic!("expected resident hit on revisit, got {other:?}"),
    }
    assert_eq!(
        transitions + resident,
        (ring.len() as u64).div_ceil(4),
        "§11.4: transition and lookup accounting partition all requests"
    );

    // 5. A served request resolves to the same bytes a direct render
    // produces: residency never alters the graphical outcome.
    let az = ring[ring.len() / 2];
    let manager_request = request(az);
    let outcome = manager.request(&manager_request).expect("request");
    let served_id = match outcome {
        RequestOutcome::Resident { id, .. } | RequestOutcome::Transitioned { id, .. } => id,
    };
    let entry = derivation.entries.get(&served_id.0).expect("retained");
    let direct = SpatialSource::render(&source, &manager_request).expect("direct render");
    match entry {
        goya_avr::target::EncodedObservation::Raw { payload, .. } => {
            assert_eq!(payload.bytes, direct.bytes);
        }
        // COPY/DELTA/ALBIT entries resolve through retrieval; the canonical
        // identity check in Phase 3 covers their byte-equality contract.
        goya_avr::target::EncodedObservation::Copy { .. } => {}
        goya_avr::target::EncodedObservation::Delta { .. }
        | goya_avr::target::EncodedObservation::Albit { .. } => {}
    }
}
