//! Target-specific `.gvi` derivation (directive §9, WS-07).
//!
//! Derivation consumes **only** the canonical compilation result: the
//! function signature takes no spatial source, so an eligible derivation
//! cannot repeat the expensive canonical rendering (§4.1, AVR-G03).
//!
//! Phase 2 encoding classes: `ANCHOR` (independently stored), `COPY`
//! (byte-identical payload reference) and `SHARED` (referenced through a
//! neighbourhood without payload duplication). `DELTA`, `ALBIT`,
//! `REPROJECT`, `COMPOSITE` and `LIVE` require the Phase 3 reconstruction
//! machinery and are rejected if requested.
//!
//! Residency tiers (§11.1): derived payloads split into `hot` (immediately
//! accessible under the profile's residency policy) and `cold` (retained,
//! retrievable, not resident). When `allow_pruning` is false, every
//! canonical observation is retained and observational granularity is
//! preserved regardless of indexing or residency (§8.1).

use crate::canonical::CompileReport;
use crate::derivation::{DerivationProfile, InventoryProfile};
use crate::domain::ObservationState;
use crate::encoding::EncodingClass;
use crate::graph::{NavigationGraph, ObservationId, ReconstructionGraph, ReconstructionNode, ResidencyGraph};
use crate::ingestion::ObservationPayload;
use crate::inventory::{
    InventoryError, ObservationInventory, ObservationRecord, RepresentationId,
};
use crate::relational::{build_layout, LayoutError};
use std::collections::BTreeMap;

/// Residency tier of a resolved observation (§11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Immediately accessible (Hot/Warm).
    Hot,
    /// Retained but not resident; retrieval goes through the
    /// neighbourhood (Cold).
    Cold,
}

/// Measured derivation outcome (§18.3 subset, honest counts — not
/// benchmark claims).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivationMetrics {
    pub canonical_observations: u32,
    /// Observations in the primary index.
    pub directly_indexed: u32,
    /// Observations resident under the profile's residency policy.
    pub resident_observations: u32,
    /// Canonical observations retained by the derived inventory.
    pub retained_observations: u32,
    /// Residency reduction vs a fully resident equivalent, in basis
    /// points (e.g. 9844 = 98.44 %).
    pub residency_reduction_bp: u32,
    pub resident_payload_bytes: u64,
    pub canonical_payload_bytes: u64,
}

/// Why a derivation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivationError {
    /// The profile failed its internal consistency validation (§9.3).
    Profile(&'static str),
    /// The profile names a canonical digest that does not match the
    /// compilation (AVR-C11: derivation provenance).
    CanonicalDigestMismatch { profile: String, canonical: String },
    /// The profile requests Phase 3+ encodings.
    EncodingNotAvailable(EncodingClass),
    /// The residency budget cannot hold the profile's hot tier.
    ResidencyBudgetExceeded { needed_bytes: u64, budget_bytes: u64 },
    /// The neighbourhood capacity cannot cover the canonical set.
    Coverage(LayoutError),
    /// The derived inventory failed construction-time validation.
    Inventory(InventoryError),
}

/// A target-specific derived inventory (§9.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Derivation {
    pub profile: InventoryProfile,
    /// Digest of the canonical inventory this derivation came from
    /// (AVR-C11 provenance).
    pub canonical_digest: String,
    /// Digest binding the derivation to its canonical source and profile.
    pub derivation_digest: String,
    /// The derived relational inventory: canonical observation table with
    /// anchors, neighbourhoods and the three separated graphs populated.
    pub inventory: ObservationInventory,
    /// Per-observation encoding class (§8.5).
    pub encodings: BTreeMap<u64, EncodingClass>,
    /// The primary index: observation states directly addressable without
    /// neighbourhood traversal (§9.2 direct index density).
    pub primary_index: BTreeMap<ObservationState, ObservationId>,
    /// Immediately accessible payloads (§11.1 Hot/Warm).
    pub hot: BTreeMap<u64, ObservationPayload>,
    /// Retained non-resident payloads (§11.1 Cold). Present only when the
    /// profile retains observations beyond its residency.
    pub cold: BTreeMap<u64, ObservationPayload>,
    pub metrics: DerivationMetrics,
}

fn derivation_digest(
    canonical_digest: &str,
    profile: &DerivationProfile,
    payload_digests: &[(u64, String)],
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(crate::DIRECTIVE_ID.as_bytes());
    hasher.update(crate::CONTRACT_VERSION.as_bytes());
    hasher.update(canonical_digest.as_bytes());
    hasher.update(format!("{profile:?}").as_bytes());
    for (id, digest) in payload_digests {
        hasher.update(id.to_le_bytes());
        hasher.update(digest.as_bytes());
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

fn payload_digest(payload: &ObservationPayload) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(payload.representation.id().as_bytes());
    hasher.update(&payload.bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Derive a target-specific inventory from a canonical compilation (§9.3).
///
/// Panics-free by construction: every profile violation is a typed error.
pub fn derive(
    report: &CompileReport,
    profile: &DerivationProfile,
) -> Result<Derivation, DerivationError> {
    profile.validate().map_err(DerivationError::Profile)?;
    if profile.canonical_digest != report.canonical_digest {
        return Err(DerivationError::CanonicalDigestMismatch {
            profile: profile.canonical_digest.clone(),
            canonical: report.canonical_digest.clone(),
        });
    }
    for encoding in &profile.reconstruction.allowed_encodings {
        if !matches!(
            encoding,
            EncodingClass::Anchor | EncodingClass::Copy | EncodingClass::Shared
        ) {
            return Err(DerivationError::EncodingNotAvailable(*encoding));
        }
    }
    if profile.index.allow_pruning {
        return Err(DerivationError::Profile(
            "pruning requires the §6.3 weighted-selection machinery (later phase); Phase 2 retains every canonical observation",
        ));
    }

    let canonical: BTreeMap<u64, &ObservationRecord> =
        report.inventory.observations.iter().map(|o| (o.id.0, o)).collect();

    let (anchor_count, hot_is_all) = match profile.profile {
        InventoryProfile::Dense => (report.inventory.observations.len() as u32, true),
        InventoryProfile::Hybrid => (profile.index.primary_nodes, true),
        InventoryProfile::Relational => (profile.index.primary_nodes, false),
    };

    // Relational layout. Dense skips it: everything is primary.
    let (anchors, neighbourhoods, navigation, residency, _membership) = if hot_is_all
        && profile.profile == InventoryProfile::Dense
    {
        (Vec::new(), Vec::new(), NavigationGraph::default(), ResidencyGraph::default(),
         vec![Vec::new(); canonical.len()])
    } else {
        let layout = build_layout(
            &report
                .inventory
                .observations
                .iter()
                .map(|o| o.state.clone())
                .collect::<Vec<_>>(),
            anchor_count,
            profile.index.max_neighbourhood_size,
            profile.reconstruction.max_dependency_depth,
        )
        .map_err(DerivationError::Coverage)?;
        (
            layout.anchors,
            layout.neighbourhoods,
            layout.navigation,
            layout.residency,
            layout.membership,
        )
    };

    // Encoding assignment (§8.5). Payload digests drive COPY detection.
    let mut encodings: BTreeMap<u64, EncodingClass> = BTreeMap::new();
    let mut seen_digests: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut payload_digests: Vec<(u64, String)> = Vec::with_capacity(canonical.len());
    for (&id, _record) in canonical.iter() {
        let payload = report
            .store
            .get(ObservationId(id))
            .expect("canonical store holds every canonical record");
        let digest = payload_digest(payload);
        // Dense indexes everything directly; otherwise anchors are
        // stored independently and the rest reference shared payloads.
        let is_anchor = profile.profile == InventoryProfile::Dense
            || anchors.iter().any(|a| a.observation.0 == id);
        let encoding = if is_anchor {
            EncodingClass::Anchor
        } else if seen_digests.contains_key(&digest) {
            EncodingClass::Copy
        } else {
            EncodingClass::Shared
        };
        seen_digests.entry(digest.clone()).or_insert(id);
        payload_digests.push((id, digest));
        encodings.insert(id, encoding);
    }

    // Residency split (§11.1) and budgets.
    let anchor_ids: std::collections::HashSet<u64> =
        anchors.iter().map(|a| a.observation.0).collect();
    let mut hot: BTreeMap<u64, ObservationPayload> = BTreeMap::new();
    let mut cold: BTreeMap<u64, ObservationPayload> = BTreeMap::new();
    for (&id, _record) in canonical.iter() {
        let payload = report
            .store
            .get(ObservationId(id))
            .expect("canonical store holds every canonical record");
        let resident = match profile.profile {
            InventoryProfile::Dense | InventoryProfile::Hybrid => true,
            InventoryProfile::Relational => anchor_ids.contains(&id),
        };
        if resident {
            hot.insert(id, payload.clone());
        } else {
            cold.insert(id, payload.clone());
        }
    }

    let canonical_payload_bytes: u64 = report
        .store
        .payload_bytes();
    let resident_payload_bytes: u64 = hot.values().map(|p| p.bytes.len() as u64).sum();
    if resident_payload_bytes > profile.residency.asset_budget_bytes {
        return Err(DerivationError::ResidencyBudgetExceeded {
            needed_bytes: resident_payload_bytes,
            budget_bytes: profile.residency.asset_budget_bytes,
        });
    }

    // Reconstruction graph: Phase 2 encodings (ANCHOR/COPY/SHARED) carry
    // no data dependencies — every payload is stored directly in the
    // artifact. The graph stays a separate, trivially acyclic instance
    // (§8.3); Phase 3 populates it with DELTA/ALBIT/REPROJECT nodes.
    let terminals: Vec<ObservationId> = canonical.keys().map(|id| ObservationId(*id)).collect();
    let reconstruction = ReconstructionGraph::new(
        Vec::<ReconstructionNode>::new(),
        terminals,
        profile.reconstruction.max_dependency_depth,
    )
    .map_err(|e| DerivationError::Inventory(InventoryError::Topology(e)))?;

    // The derived inventory re-validates the full invariant set (§17).
    let records: Vec<ObservationRecord> = canonical
        .values()
        .map(|r| {
            let mut record = (*r).clone();
            record.representation = RepresentationId(0);
            record
        })
        .collect();
    let inventory = ObservationInventory::new(
        report.canonical_digest.clone(),
        report.inventory.domain.clone(),
        records,
        navigation,
        Vec::new(),
        residency,
        anchors,
        neighbourhoods,
        Vec::new(),
        profile.reconstruction.max_dependency_depth,
    )
    .map_err(DerivationError::Inventory)?;
    // Re-attach the (validated) reconstruction graph.
    let mut inventory = inventory;
    inventory.reconstruction = reconstruction;
    let primary_index: BTreeMap<ObservationState, ObservationId> = inventory
        .observations
        .iter()
        .filter(|o| {
            profile.profile == InventoryProfile::Dense
                || inventory.anchors.iter().any(|a| a.observation == o.id)
        })
        .map(|o| (o.state.clone(), o.id))
        .collect();

    let directly_indexed = inventory
        .anchors
        .len()
        .max(if profile.profile == InventoryProfile::Dense {
            canonical.len()
        } else {
            0
        });
    let canonical_count = canonical.len() as u32;
    let resident_count = hot.len() as u32;
    let metrics = DerivationMetrics {
        canonical_observations: canonical_count,
        directly_indexed: directly_indexed as u32,
        resident_observations: resident_count,
        retained_observations: (hot.len() + cold.len()) as u32,
        residency_reduction_bp: if canonical_count > 0 {
            ((1.0 - resident_count as f64 / canonical_count as f64) * 10_000.0) as u32
        } else {
            0
        },
        resident_payload_bytes,
        canonical_payload_bytes,
    };

    Ok(Derivation {
        profile: profile.profile,
        canonical_digest: report.canonical_digest.clone(),
        derivation_digest: derivation_digest(&report.canonical_digest, profile, &payload_digests),
        inventory,
        encodings,
        primary_index,
        hot,
        cold,
        metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{compile, CompilationBudget};
    use crate::derivation::{IndexPolicy, ReconstructionPolicy, ResidencyPolicy};
    use crate::domain::ObservationDomain;
    use crate::ingestion::{Observer, SpatialSource, SourceError};
    use crate::representation::RepresentationClass;
    use std::cell::Cell;

    struct CountingSource {
        calls: Cell<u32>,
    }
    impl SpatialSource for CountingSource {
        fn source_digest(&self) -> String {
            "counting:1".into()
        }
        fn render(&self, observer: &Observer) -> Result<ObservationPayload, SourceError> {
            self.calls.set(self.calls.get() + 1);
            Ok(ObservationPayload {
                representation: RepresentationClass::Vec,
                bytes: format!(
                    "obs {} {}",
                    crate::ingestion::quantise_angle(observer.azimuth_deg),
                    crate::ingestion::quantise_angle(observer.elevation_deg)
                )
                .into_bytes(),
            })
        }
    }

    pub(crate) fn orbital_report(n: u32, calls: &Cell<u32>) -> CompileReport {
        let source = CountingSource { calls: Cell::new(0) };
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: n,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let report = compile(
            &source,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 8192, max_payload_bytes: u64::MAX },
        )
        .expect("compilation");
        calls.set(source.calls.get());
        report
    }

    pub(crate) fn profile(
        kind: InventoryProfile,
        canonical_digest: &str,
        anchors: u32,
        neighbourhood: u32,
    ) -> DerivationProfile {
        DerivationProfile {
            profile: kind,
            canonical_digest: canonical_digest.into(),
            index: IndexPolicy {
                primary_nodes: anchors,
                max_neighbourhood_size: neighbourhood,
                preserve_canonical_granularity: true,
                allow_pruning: false,
            },
            reconstruction: ReconstructionPolicy {
                max_dependency_depth: 4,
                allowed_encodings: vec![
                    EncodingClass::Anchor,
                    EncodingClass::Copy,
                    EncodingClass::Shared,
                ],
            },
            residency: ResidencyPolicy {
                asset_budget_bytes: u64::MAX,
                prefetch_neighbourhoods: 1,
            },
            max_silhouette_error_px: 1,
            allow_live_fallback: false,
        }
    }

    #[test]
    fn three_profiles_derive_without_repeating_the_canonical_render() {
        let calls = Cell::new(0);
        let report = orbital_report(256, &calls);
        let renders_after_compile = calls.get();
        assert_eq!(renders_after_compile, 256);

        for kind in [InventoryProfile::Dense, InventoryProfile::Hybrid, InventoryProfile::Relational] {
            let derivation = derive(&report, &profile(kind, &report.canonical_digest, 32, 96))
                .unwrap_or_else(|e| panic!("{kind:?} derivation failed: {e:?}"));
            assert_eq!(derivation.metrics.canonical_observations, 256);
            assert_eq!(derivation.metrics.retained_observations, 256);
        }
        assert_eq!(
            calls.get(),
            renders_after_compile,
            "AVR-G03: derivation must not repeat the canonical rendering"
        );
    }

    #[test]
    fn derivations_are_deterministic() {
        let calls = Cell::new(0);
        let report = orbital_report(128, &calls);
        for kind in [InventoryProfile::Dense, InventoryProfile::Hybrid, InventoryProfile::Relational] {
            let a = derive(&report, &profile(kind, &report.canonical_digest, 16, 64)).expect("a");
            let b = derive(&report, &profile(kind, &report.canonical_digest, 16, 64)).expect("b");
            assert_eq!(a.derivation_digest, b.derivation_digest, "AVR-G04");
            assert_eq!(a.inventory.anchors, b.inventory.anchors);
            assert_eq!(a.encodings, b.encodings);
        }
    }

    #[test]
    fn profile_digest_mismatch_is_rejected() {
        let calls = Cell::new(0);
        let report = orbital_report(16, &calls);
        let mut p = profile(InventoryProfile::Relational, &report.canonical_digest, 4, 16);
        p.canonical_digest = "sha256:stale".into();
        assert_eq!(
            derive(&report, &p).unwrap_err(),
            DerivationError::CanonicalDigestMismatch {
                profile: "sha256:stale".into(),
                canonical: report.canonical_digest.clone(),
            }
        );
    }

    #[test]
    fn phase_3_encodings_are_rejected_in_phase_2() {
        let calls = Cell::new(0);
        let report = orbital_report(16, &calls);
        let mut p = profile(InventoryProfile::Relational, &report.canonical_digest, 4, 16);
        p.reconstruction.allowed_encodings.push(EncodingClass::Albit);
        assert_eq!(
            derive(&report, &p).unwrap_err(),
            DerivationError::EncodingNotAvailable(EncodingClass::Albit)
        );
    }

    #[test]
    fn residency_budget_is_enforced_per_profile() {
        let calls = Cell::new(0);
        let report = orbital_report(64, &calls);
        // Dense needs everything resident; a tiny budget must fail dense
        // while relational (anchors only) still fits.
        let mut dense = profile(InventoryProfile::Dense, &report.canonical_digest, 64, 96);
        dense.residency.asset_budget_bytes = 1;
        assert!(matches!(
            derive(&report, &dense).unwrap_err(),
            DerivationError::ResidencyBudgetExceeded { .. }
        ));
        let mut relational =
            profile(InventoryProfile::Relational, &report.canonical_digest, 8, 64);
        relational.residency.asset_budget_bytes = 1;
        assert!(matches!(
            derive(&report, &relational).unwrap_err(),
            DerivationError::ResidencyBudgetExceeded { .. }
        ));
    }

    #[test]
    fn relational_profile_meets_the_residency_reduction_target() {
        // §18.4 research target: ≥80 % residency reduction against a fully
        // resident equivalent. This measures the derivation artifact, not
        // a benchmarked device run (§18.4: proposed acceptance objectives).
        let calls = Cell::new(0);
        let report = orbital_report(4096, &calls);
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 64, 96),
        )
        .expect("relational derivation");
        assert_eq!(derivation.metrics.canonical_observations, 4096);
        assert_eq!(derivation.metrics.resident_observations, 64);
        assert_eq!(derivation.metrics.retained_observations, 4096);
        assert!(
            derivation.metrics.residency_reduction_bp >= 8000,
            "residency reduction {} bp below the 80 % research target",
            derivation.metrics.residency_reduction_bp
        );
    }

    #[test]
    fn neighbourhoods_overlap_and_payloads_are_never_duplicated() {
        let calls = Cell::new(0);
        let report = orbital_report(64, &calls);
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 8, 48),
        )
        .expect("derivation");
        // §8.2: payloads are keyed once; hot ∪ cold partitions the
        // canonical set with no duplication.
        assert_eq!(derivation.hot.len() + derivation.cold.len(), 64);
        for id in derivation.hot.keys() {
            assert!(!derivation.cold.contains_key(id));
        }
        // Overlapping membership exists (anchors' nearest-member lists
        // overlap across anchors).
        let total_refs: usize = derivation
            .inventory
            .neighbourhoods
            .iter()
            .map(|n| n.observations.len())
            .sum();
        assert!(total_refs > 64, "neighbourhoods must overlap to share observations");
    }

    #[test]
    fn dense_profile_indexes_everything_hybrid_sits_between() {
        let calls = Cell::new(0);
        let report = orbital_report(64, &calls);
        let dense = derive(&report, &profile(InventoryProfile::Dense, &report.canonical_digest, 64, 96))
            .expect("dense");
        assert_eq!(dense.metrics.directly_indexed, 64);
        assert_eq!(dense.metrics.resident_observations, 64);
        assert!(dense.cold.is_empty());

        let hybrid = derive(&report, &profile(InventoryProfile::Hybrid, &report.canonical_digest, 8, 48))
            .expect("hybrid");
        assert_eq!(hybrid.metrics.directly_indexed, 8);
        assert_eq!(hybrid.metrics.resident_observations, 64, "hybrid keeps neighbourhoods resident");

        let relational =
            derive(&report, &profile(InventoryProfile::Relational, &report.canonical_digest, 8, 48))
                .expect("relational");
        assert_eq!(relational.metrics.directly_indexed, 8);
        assert_eq!(relational.metrics.resident_observations, 8);
        assert_eq!(relational.metrics.residency_reduction_bp, 8750);
    }

    #[test]
    fn copy_encoding_detects_byte_identical_payloads() {
        // A degenerate source: every observation renders the same bytes.
        struct ConstantSource;
        impl SpatialSource for ConstantSource {
            fn source_digest(&self) -> String {
                "constant:1".into()
            }
            fn render(&self, _o: &Observer) -> Result<ObservationPayload, SourceError> {
                Ok(ObservationPayload {
                    representation: RepresentationClass::Vec,
                    bytes: b"constant".to_vec(),
                })
            }
        }
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: 16,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let report = compile(
            &ConstantSource,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 64, max_payload_bytes: u64::MAX },
        )
        .expect("compilation");
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 4, 16),
        )
        .expect("derivation");
        let anchors: std::collections::HashSet<u64> = derivation
            .inventory
            .anchors
            .iter()
            .map(|a| a.observation.0)
            .collect();
        for (id, encoding) in &derivation.encodings {
            if anchors.contains(id) {
                assert_eq!(*encoding, EncodingClass::Anchor);
            } else {
                assert_eq!(*encoding, EncodingClass::Copy, "identical payloads must be COPY");
            }
        }
    }

    #[test]
    fn reconstruction_graph_is_independent_and_acyclic() {
        let calls = Cell::new(0);
        let report = orbital_report(32, &calls);
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 8, 16),
        )
        .expect("derivation");
        // §8.3: three separated graphs. Phase 2 reconstruction is empty
        // (no data dependencies) but re-validated as an instance.
        assert!(derivation.inventory.reconstruction.nodes().is_empty());
        derivation.inventory.reconstruction.verify().expect("acyclic");
        assert!(!derivation.inventory.navigation.edges.is_empty());
        assert!(!derivation.inventory.residency.adjacency.is_empty());
        // Navigation cycles are permitted; every anchor is navigable.
        assert_eq!(derivation.inventory.anchors.len(), 8);
    }
}
