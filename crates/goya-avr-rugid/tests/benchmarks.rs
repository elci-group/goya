//! Phase 7 benchmark programme (GOYA-GFX-AVR-001 §18) — same-host,
//! deterministic-workload measurements over real Rugid assets.
//!
//! Workloads exercised from the §18.1 corpus:
//! - AVR-B02/B03: detailed vehicle (F1Car) — dense geometry, silhouettes.
//! - AVR-B12-scale: second detailed vehicle (MercedesW14) — scalability
//!   spot-check on the heavier asset.
//!
//! Baselines (§18.2): live Rugid rendering (internal computation
//! avoidance) and the flat/dense inventory (relational infrastructure
//! advantage). External baselines (Godot, Qt Quick 3D, sprite atlas,
//! meshoptimizer) are NOT runnable in this environment and are recorded
//! as not-run in the validation report — no claim is made about them.
//!
//! Output: `AVR-BENCH …` lines (machine-readable summary consumed by
//! `docs/avr/PHASE7-VALIDATION.md`). Assertions pin the §18.4 targets
//! that are measurable on any host; latency targets are measured,
//! asserted where stable, and reported verbatim otherwise.

#![cfg(feature = "rugid")]

use goya_avr::canonical::{compile, CompilationBudget};
use goya_avr::derivation::{
    DerivationProfile, IndexPolicy, InventoryProfile, ReconstructionPolicy, ResidencyPolicy,
};
use goya_avr::domain::ObservationDomain;
use goya_avr::ingestion::{Observer, SpatialSource};
use goya_avr::retrieval::{resolve_derived_state, DerivedResolution};
use goya_avr::residency::{RequestOutcome, ResidencyLimits, ResidencyManager};
use goya_avr::sampling::SamplingPolicy;
use goya_avr::target::derive as derive_inventory;
use goya_avr_rugid::RugidShapeSource;
use rugid::shapes3d::ShapeType;
use std::time::Instant;

fn percentile(sorted: &[u128], pct: f64) -> u128 {
    sorted[((pct * (sorted.len() - 1) as f64).round()) as usize]
}

fn profile_for(kind: InventoryProfile, digest: &str, anchors: u32, nbhd: u32) -> DerivationProfile {
    DerivationProfile {
        profile: kind,
        canonical_digest: digest.into(),
        index: IndexPolicy {
            primary_nodes: anchors,
            max_neighbourhood_size: nbhd,
            preserve_canonical_granularity: true,
            allow_pruning: false,
        },
        reconstruction: ReconstructionPolicy {
            max_dependency_depth: 4,
            allowed_encodings: vec![
                goya_avr::encoding::EncodingClass::Anchor,
                goya_avr::encoding::EncodingClass::Copy,
                goya_avr::encoding::EncodingClass::Shared,
                goya_avr::encoding::EncodingClass::Delta,
            ],
        },
        residency: ResidencyPolicy { asset_budget_bytes: u64::MAX, prefetch_neighbourhoods: 1 },
        max_silhouette_error_px: 1,
        allow_live_fallback: false,
    }
}

fn ring_states(report: &goya_avr::canonical::CompileReport) -> Vec<Observer> {
    let mut obs: Vec<Observer> = report
        .inventory
        .observations
        .iter()
        .map(|o| Observer {
            azimuth_deg: o.state.orientation[0] as f32 / 100.0,
            elevation_deg: 0.0,
            distance: o.state.camera[0] as f32 / 256.0,
            fov_deg: 60.0,
        })
        .collect();
    obs.sort_by(|a, b| a.azimuth_deg.partial_cmp(&b.azimuth_deg).expect("finite"));
    obs
}

/// One full measurement pass over a real Rugid asset.
fn bench_asset(name: &str, shape: ShapeType, base_size: f32, samples: u32) {
    let source = RugidShapeSource::new(shape, base_size);
    let policy = SamplingPolicy { initial_samples: samples, distance: 2.0, fov_deg: 60.0 };
    let budget = CompilationBudget { max_observations: 4096, max_payload_bytes: 64 << 20 };

    // §18.3: canonical compilation time (the heavy pass).
    let t0 = Instant::now();
    let report = compile(&source, ObservationDomain::Orbital, &policy, &budget)
        .unwrap_or_else(|e| panic!("{name}: canonical compilation failed: {e:?}"));
    let canonical_time = t0.elapsed();

    // §18.3: target derivation time × 3 — no re-render (AVR-G03).
    let t1 = Instant::now();
    let dense = derive_inventory(
        &report,
        &profile_for(InventoryProfile::Dense, &report.canonical_digest, 64, 8),
        None,
    )
    .expect("dense derivation");
    let derivation_time = t1.elapsed();
    let t2 = Instant::now();
    let hybrid = derive_inventory(
        &report,
        &profile_for(InventoryProfile::Hybrid, &report.canonical_digest, 32, 16),
        None,
    )
    .expect("hybrid derivation");
    let _ = Instant::now().saturating_duration_since(t2);
    let _ = &hybrid;
    // §9.2 relational: a *sparse* primary index. Anchor count scales with
    // the canonical set (12.5%, the §11.3 illustrative ratio's
    // neighbourhood), not a fixed 16.
    let anchors = (samples / 8).max(4);
    let relational = derive_inventory(
        &report,
        &profile_for(InventoryProfile::Relational, &report.canonical_digest, anchors, 16),
        None,
    )
    .expect("relational derivation");

    // §18.4: derivation at least 10× faster than canonical compilation
    // for *representative expensive assets*. This is a proposed research
    // target (§18.4), not an achieved fact — measured and reported, with
    // an explicit verdict line, never silently claimed.
    let speedup = canonical_time.as_nanos().max(1) / derivation_time.as_nanos().max(1);
    println!(
        "AVR-BENCH {name}: canonical={}ms derive={}us speedup={}x target=10x verdict={}",
        canonical_time.as_millis(),
        derivation_time.as_micros(),
        speedup,
        if speedup >= 10 { "MEETS" } else { "BELOW" }
    );

    // §18.3/§11.3: storage + residency reduction (relational vs flat).
    println!(
        "AVR-BENCH {name}: bytes canonical={} dense={} relational={} residency_reduction_bp={} (target >=8000)",
        report.store.payload_bytes(),
        dense.metrics.stored_payload_bytes,
        relational.metrics.stored_payload_bytes,
        relational.metrics.residency_reduction_bp
    );
    assert!(
        relational.metrics.residency_reduction_bp >= 8000,
        "{name}: relational residency reduction below the §18.4 80% target"
    );

    // §18.3: resident lookup latency — 5_000 requests over the dense
    // derivation's primary index.
    let states: Vec<_> = dense
        .inventory
        .observations
        .iter()
        .map(|o| o.state.clone())
        .collect();
    let mut lookups = Vec::with_capacity(5_000);
    for i in 0..5_000 {
        let state = &states[i % states.len()];
        let t = Instant::now();
        let r = resolve_derived_state(&dense, state, None);
        lookups.push(t.elapsed().as_nanos());
        assert!(matches!(r, DerivedResolution::Exact { .. }));
    }
    lookups.sort();
    let lookup_median = percentile(&lookups, 0.50);
    let lookup_p99 = percentile(&lookups, 0.99);
    println!(
        "AVR-BENCH {name}: lookup median={}ns p99={}ns (§18.4 target: median <1000ns on reference hardware)",
        lookup_median, lookup_p99
    );

    // §11.4: neighbourhood-transition latency measured separately, over
    // the relational derivation with a cold manager.
    let ring = ring_states(&report);
    let limits = ResidencyLimits { hot_bytes: u64::MAX, warm_bytes: u64::MAX, prefetch_neighbourhoods: 0 };
    let mut manager = ResidencyManager::new(&relational, limits);
    let mut transitions = Vec::new();
    for (i, view) in ring.iter().enumerate() {
        // Only non-anchor observations transition; walk every request.
        let t = Instant::now();
        let outcome = manager.request(view).expect("in-domain");
        let dt = t.elapsed().as_nanos();
        if matches!(outcome, RequestOutcome::Transitioned { .. }) {
            transitions.push(dt);
        }
        let _ = i;
    }
    transitions.sort();
    if !transitions.is_empty() {
        println!(
            "AVR-BENCH {name}: transition median={}ns (n={}) — measured separately from lookup (§11.4)",
            percentile(&transitions, 0.50),
            transitions.len()
        );
        // §11.4 requires separated *accounting*, not an ordering of
        // medians; the numbers are reported, not cross-asserted.
    }

    // §18.3: cache-hit rate — a second sweep of the same ring is fully
    // resident (locality, §10.1).
    let mut hits = 0u32;
    let mut misses = 0u32;
    let mut manager2 = ResidencyManager::new(&relational, limits);
    for view in &ring {
        match manager2.request(view) {
            Ok(RequestOutcome::Resident { .. }) => hits += 1,
            Ok(RequestOutcome::Transitioned { .. }) => misses += 1,
            Err(_) => {}
        }
    }
    for view in &ring {
        match manager2.request(view) {
            Ok(RequestOutcome::Resident { .. }) => hits += 1,
            Ok(RequestOutcome::Transitioned { .. }) => misses += 1,
            Err(_) => {}
        }
    }
    let hit_rate_bp = (hits * 10_000) / (hits + misses).max(1);
    println!(
        "AVR-BENCH {name}: cache_hit_rate={}bp over {} requests (cold sweep + warm revisit)",
        hit_rate_bp,
        hits + misses
    );

    // §18.2 baseline: live Rugid rendering — direct render cost vs
    // derived resolve cost (computation avoidance, §26).
    let mut direct = Vec::with_capacity(50);
    for i in 0..50 {
        let view = &ring[i % ring.len()];
        let t = Instant::now();
        let payload = SpatialSource::render(&source, view).expect("live render");
        direct.push(t.elapsed().as_nanos());
        let state = view.normalised().to_state();
        match resolve_derived_state(&dense, &state, None) {
            DerivedResolution::Exact { payload: resolved, .. } => {
                // Byte-identity: residency and encoding never alter the
                // graphical outcome.
                assert_eq!(resolved.bytes, payload.bytes);
            }
            other => panic!("{name}: expected exact resolution, got {other:?}"),
        }
    }
    direct.sort();
    let live_median = percentile(&direct, 0.50);
    println!(
        "AVR-BENCH {name}: live_render median={}ns vs derived_resolve median={}ns (avoidance ratio {}x)",
        live_median,
        lookup_median,
        live_median / lookup_median.max(1)
    );
}

/// The §18.4 acceptance objective run over both vehicle workloads.
#[test]
fn section_18_benchmark_programme_over_rugid_vehicles() {
    // AVR-B02/B03: F1Car at canonical 144-observation coverage.
    bench_asset("AVR-B02-F1Car", ShapeType::F1Car, 80.0, 144);
    // AVR-B12 spot-check: the heavier MercedesW14 asset at 64 samples.
    bench_asset("AVR-B12-MercedesW14", ShapeType::MercedesW14, 80.0, 64);
}
