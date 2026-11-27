//! Target-specific `.gvi` derivation (directive §9, WS-07; Phase 3
//! reconstruction integration per §12.2/§12.3).
//!
//! Derivation consumes **only** the canonical compilation result: the
//! function signature takes no spatial source, so an eligible derivation
//! cannot repeat the expensive canonical rendering (§4.1, AVR-G03).
//!
//! Encoding pipeline per observation (§8.5): candidates are generated and
//! the cheapest adequate encoding is selected by complete encoded size —
//! header, instructions, parameters and overhead included (§12.5):
//!
//! 1. `ANCHOR` — stored independently (always available).
//! 2. `COPY` — byte-identical payload already stored raw.
//! 3. `DELTA` — verified differential encoding against the
//!    neighbourhood's anchor payload (GVI-D, exact by verification).
//! 4. `ALBIT` — generative reconstruction candidates from the optional
//!    [`ReconstructionEngine`](crate::engine::ReconstructionEngine)
//!    (GVI-A): byte-verified → exact; within the declared error bound and
//!    the profile's fidelity policy → bounded (AVR-C09/C10).
//!
//! Storage reduction alone never justifies an encoding: decode is a
//! bounded linear pass, chains are depth-checked through the
//! reconstruction graph, and `COPY` references only raw payloads so
//! reconstruction chains stay within `max_dependency_depth` (§12.2).
//!
//! Residency tiers (§11.1): anchors (or everything, for Dense/Hybrid) are
//! hot; the rest of the retained canonical set is cold. When
//! `allow_pruning` is false, every canonical observation is retained and
//! observational granularity is preserved regardless of indexing or
//! residency (§8.1).

use crate::canonical::CompileReport;
use crate::delta;
use crate::derivation::{DerivationProfile, InventoryProfile};
use crate::domain::ObservationState;
use crate::encoding::EncodingClass;
use crate::engine::ReconstructionEngine;
use crate::graph::{NavigationGraph, ObservationId, ReconstructionGraph, ReconstructionNode, ResidencyGraph};
use crate::ingestion::ObservationPayload;
use crate::inventory::{
    InventoryError, ObservationInventory, ObservationRecord, ProvenanceId, ProvenanceRecord,
    ProvenanceRelation, RepresentationId,
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

/// Fixed per-entry overhead counted in the §12.5 encoding comparison:
/// a stable observation id, encoding tag and length prefix.
pub const ENTRY_OVERHEAD: u64 = 8;

/// How one observation is stored in a derived inventory.
#[derive(Debug, Clone, PartialEq)]
pub enum EncodedObservation {
    /// Stored independently (`ANCHOR`/`SHARED`): the payload itself.
    Raw { payload: ObservationPayload, tier: Tier },
    /// Reference to a byte-identical raw-stored observation (`COPY`).
    Copy { of: u64, tier: Tier },
    /// Verified differential reconstruction against a raw base (`DELTA`).
    Delta { base: u64, delta: Vec<u8>, tier: Tier },
    /// Generative reconstruction (`ALBIT`) certified exact or bounded.
    Albit { base: u64, op: String, params: Vec<u8>, exact: bool, tier: Tier },
}

impl EncodedObservation {
    pub fn tier(&self) -> Tier {
        match self {
            EncodedObservation::Raw { tier, .. }
            | EncodedObservation::Copy { tier, .. }
            | EncodedObservation::Delta { tier, .. }
            | EncodedObservation::Albit { tier, .. } => *tier,
        }
    }

    /// Stored bytes this entry contributes (§12.5 accounting).
    pub fn stored_bytes(&self) -> u64 {
        match self {
            EncodedObservation::Raw { payload, .. } => payload.bytes.len() as u64,
            EncodedObservation::Copy { .. } => 0,
            EncodedObservation::Delta { delta, .. } => delta.len() as u64,
            EncodedObservation::Albit { params, .. } => params.len() as u64,
        }
    }
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
    /// points (e.g. 9844 = 98.44 %), by observation count.
    pub residency_reduction_bp: u32,
    /// Stored bytes in the hot tier (encoded form).
    pub resident_payload_bytes: u64,
    /// Raw bytes of the equivalent fully resident canonical inventory.
    pub canonical_payload_bytes: u64,
    /// Total stored bytes (hot + cold, encoded form).
    pub stored_payload_bytes: u64,
    /// Persistent-storage reduction vs raw canonical, in basis points
    /// (0 when nothing compresses).
    pub storage_reduction_bp: u32,
}

/// Why a derivation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivationError {
    /// The profile failed its internal consistency validation (§9.3).
    Profile(&'static str),
    /// The profile names a canonical digest that does not match the
    /// compilation (AVR-C11: derivation provenance).
    CanonicalDigestMismatch { profile: String, canonical: String },
    /// The profile requests Phase 3+ encodings its policy does not allow.
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
    /// How each observation is stored, keyed by observation id.
    pub entries: BTreeMap<u64, EncodedObservation>,
    /// The primary index: observation states directly addressable without
    /// neighbourhood traversal (§9.2 direct index density).
    pub primary_index: BTreeMap<ObservationState, ObservationId>,
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

/// A candidate encoding produced by the pipeline, before selection.
struct Candidate {
    encoding: EncodingClass,
    entry: EncodedObservation,
    stored_bytes: u64,
    /// Deterministic tie-break rank; lower wins.
    rank: u8,
}

/// Derive a target-specific inventory from a canonical compilation (§9.3).
///
/// `engine` provides GVI-A generative candidates; `None` restricts the
/// pipeline to `ANCHOR`/`COPY`/`SHARED`/`DELTA`.
pub fn derive(
    report: &CompileReport,
    profile: &DerivationProfile,
    engine: Option<&dyn ReconstructionEngine>,
) -> Result<Derivation, DerivationError> {
    profile.validate().map_err(DerivationError::Profile)?;
    if profile.canonical_digest != report.canonical_digest {
        return Err(DerivationError::CanonicalDigestMismatch {
            profile: profile.canonical_digest.clone(),
            canonical: report.canonical_digest.clone(),
        });
    }
    // Phase 3: DELTA and ALBIT are now available (§12.2/§12.3). A profile
    // that allows ALBIT without an engine simply derives no albit entries —
    // generative reconstruction is computed, never stored, so its absence
    // degrades to other encodings rather than failing the derivation.
    for encoding in &profile.reconstruction.allowed_encodings {
        if !matches!(
            encoding,
            EncodingClass::Anchor
                | EncodingClass::Copy
                | EncodingClass::Shared
                | EncodingClass::Delta
                | EncodingClass::Albit
        ) {
            return Err(DerivationError::EncodingNotAvailable(*encoding));
        }
    }
    if profile.index.allow_pruning {
        return Err(DerivationError::Profile(
            "pruning requires the §6.3 weighted-selection machinery (later phase); Phase 2+ retains every canonical observation",
        ));
    }

    let canonical: BTreeMap<u64, &ObservationRecord> =
        report.inventory.observations.iter().map(|o| (o.id.0, o)).collect();

    let (anchor_count,) = match profile.profile {
        InventoryProfile::Dense => (report.inventory.observations.len() as u32,),
        InventoryProfile::Hybrid | InventoryProfile::Relational => {
            (profile.index.primary_nodes,)
        }
    };

    // Relational layout. Dense skips it: everything is primary.
    let (anchors, neighbourhoods, navigation, residency, membership) =
        if profile.profile == InventoryProfile::Dense {
            (
                Vec::new(),
                Vec::new(),
                NavigationGraph::default(),
                ResidencyGraph::default(),
                vec![Vec::new(); canonical.len()],
            )
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

    // Candidate reconstruction bases: the anchors of every neighbourhood
    // the observation belongs to, in deterministic rank order, capped to
    // bound encoding work (§8.2: neighbourhoods may overlap; §6.3: base
    // choice is an encoding decision, not a fixed owner).
    const MAX_BASES_PER_OBSERVATION: usize = 8;
    let mut bases_of: std::collections::HashMap<u64, Vec<u64>> = std::collections::HashMap::new();
    for (obs, refs) in membership.iter().enumerate() {
        let mut bases: Vec<u64> = Vec::new();
        for rank in refs.iter().take(MAX_BASES_PER_OBSERVATION) {
            let base = anchors[*rank as usize].observation.0;
            if !bases.contains(&base) {
                bases.push(base);
            }
        }
        bases_of.insert(obs as u64, bases);
    }

    // Encoding pipeline (§8.5). Iteration is in canonical id order, so
    // COPY references and candidate selection are deterministic.
    let mut encodings: BTreeMap<u64, EncodingClass> = BTreeMap::new();
    let mut entries: BTreeMap<u64, EncodedObservation> = BTreeMap::new();
    let mut raw_by_digest: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    let mut reconstruction_nodes: Vec<ReconstructionNode> = Vec::new();
    let mut provenance: Vec<ProvenanceRecord> = Vec::new();
    let mut payload_digests: Vec<(u64, String)> = Vec::with_capacity(canonical.len());
    let mut next_provenance_id: u32 = 0;

    for (&id, _record) in canonical.iter() {
        let payload = report
            .store
            .get(ObservationId(id))
            .expect("canonical store holds every canonical record");
        let digest = payload_digest(payload);
        payload_digests.push((id, digest.clone()));

        let is_anchor = profile.profile == InventoryProfile::Dense
            || anchors.iter().any(|a| a.observation.0 == id);

        let mut candidates: Vec<Candidate> = Vec::new();
        if is_anchor {
            candidates.push(Candidate {
                encoding: EncodingClass::Anchor,
                entry: EncodedObservation::Raw {
                    payload: payload.clone(),
                    tier: Tier::Hot,
                },
                stored_bytes: payload.bytes.len() as u64,
                rank: 0,
            });
        } else {
            // COPY: a byte-identical payload already stored raw (chains
            // stay at depth 1 under the raw-only reference rule).
            if let Some(&first) = raw_by_digest.get(&digest) {
                candidates.push(Candidate {
                    encoding: EncodingClass::Copy,
                    entry: EncodedObservation::Copy {
                        of: first,
                        tier: Tier::Hot,
                    },
                    stored_bytes: ENTRY_OVERHEAD,
                    rank: 1,
                });
            }

            // DELTA against a neighbourhood anchor's raw payload (GVI-D):
            // every candidate base is evaluated, the cheapest wins.
            if let Some(bases) = bases_of.get(&id) {
                for &base in bases {
                    let base_payload = report
                        .store
                        .get(ObservationId(base))
                        .expect("anchor is a canonical record");
                    if let Some(delta_bytes) = delta::encode(&base_payload.bytes, &payload.bytes) {
                        let total = delta_bytes.len() as u64 + ENTRY_OVERHEAD;
                        if total < payload.bytes.len() as u64 {
                            candidates.push(Candidate {
                                encoding: EncodingClass::Delta,
                                entry: EncodedObservation::Delta {
                                    base,
                                    delta: delta_bytes,
                                    tier: Tier::Hot,
                                },
                                stored_bytes: total,
                                rank: 2,
                            });
                        }
                    }
                }
            }

            // ALBIT candidates from the engine (GVI-A), verified here:
            // byte-identical output certifies exact; otherwise the op's
            // declared error must fit the profile's fidelity policy
            // (§12.3, AVR-C09/C10).
            if let Some(engine) = engine {
                if let (Some(bases), Some(target_record)) = (
                    bases_of.get(&id),
                    canonical.get(&id),
                ) {
                    for &base in bases {
                    let base_state = canonical
                        .get(&base)
                        .expect("anchor is a canonical record")
                        .state
                        .clone();
                    for (op, params) in engine.candidates(&target_record.state, &base_state) {
                        let base_payload = report
                            .store
                            .get(ObservationId(base))
                            .expect("anchor is a canonical record");
                        let outcome = match engine.apply(&op, &params, base_payload) {
                            Ok(outcome) => outcome,
                            Err(_) => continue,
                        };
                        let total = params.len() as u64 + ENTRY_OVERHEAD;
                        if total >= payload.bytes.len() as u64 {
                            continue; // §12.5: must pay for itself
                        }
                        if outcome.payload.bytes == payload.bytes {
                            candidates.push(Candidate {
                                encoding: EncodingClass::Albit,
                                entry: EncodedObservation::Albit {
                                    base,
                                    op,
                                    params,
                                    exact: true,
                                    tier: Tier::Hot,
                                },
                                stored_bytes: total,
                                rank: 3,
                            });
                        } else if let Some(declared) = outcome.declared_error {
                            if (declared as u64) <= profile.max_silhouette_error_px as u64 {
                                candidates.push(Candidate {
                                    encoding: EncodingClass::Albit,
                                    entry: EncodedObservation::Albit {
                                        base,
                                        op,
                                        params,
                                        exact: false,
                                        tier: Tier::Hot,
                                    },
                                    stored_bytes: total,
                                    rank: 4,
                                });
                            }
                        }
                        // Not byte-identical and no acceptable declared
                        // bound: uncertified — never encoded (AVR-C09).
                    }
                    }
                }
            }

            // Fallback: stored raw (Shared semantics).
            candidates.push(Candidate {
                encoding: EncodingClass::Shared,
                entry: EncodedObservation::Raw {
                    payload: payload.clone(),
                    tier: Tier::Hot,
                },
                stored_bytes: payload.bytes.len() as u64,
                rank: 5,
            });
        }

        // Select the cheapest adequate candidate; ties by rank (§8.5).
        candidates.sort_by_key(|c| (c.stored_bytes, c.rank));
        let chosen = candidates.into_iter().next().expect("raw fallback always exists");

        if chosen.encoding == EncodingClass::Anchor || chosen.encoding == EncodingClass::Shared {
            // Raw-stored payloads join the COPY reference pool.
            raw_by_digest.entry(digest.clone()).or_insert(id);
        }
        if let EncodedObservation::Delta { base, .. } = &chosen.entry {
            reconstruction_nodes.push(ReconstructionNode {
                observation: ObservationId(id),
                dependencies: vec![ObservationId(*base)],
            });
            provenance.push(ProvenanceRecord {
                id: ProvenanceId(next_provenance_id),
                observation: ObservationId(id),
                relation: ProvenanceRelation::ExactTransform { source: ObservationId(*base) },
            });
            next_provenance_id += 1;
        }
        if let EncodedObservation::Albit { base, exact, .. } = &chosen.entry {
            reconstruction_nodes.push(ReconstructionNode {
                observation: ObservationId(id),
                dependencies: vec![ObservationId(*base)],
            });
            provenance.push(ProvenanceRecord {
                id: ProvenanceId(next_provenance_id),
                observation: ObservationId(id),
                relation: ProvenanceRelation::Albit {
                    source: ObservationId(*base),
                    exact: *exact,
                },
            });
            next_provenance_id += 1;
        }

        encodings.insert(id, chosen.encoding);
        entries.insert(id, chosen.entry);
    }

    // Residency demotion (§11.1): in a relational profile only anchors
    // stay hot; everything else is retained cold. Encoding tier follows.
    let anchor_ids: std::collections::HashSet<u64> =
        anchors.iter().map(|a| a.observation.0).collect();
    if profile.profile == InventoryProfile::Relational {
        for (id, entry) in entries.iter_mut() {
            if !anchor_ids.contains(id) {
                match entry {
                    EncodedObservation::Raw { tier, .. }
                    | EncodedObservation::Copy { tier, .. }
                    | EncodedObservation::Delta { tier, .. }
                    | EncodedObservation::Albit { tier, .. } => *tier = Tier::Cold,
                }
            }
        }
    }

    let canonical_payload_bytes: u64 = report.store.payload_bytes();
    let resident_payload_bytes: u64 = entries
        .values()
        .filter(|e| e.tier() == Tier::Hot)
        .map(EncodedObservation::stored_bytes)
        .sum();
    if resident_payload_bytes > profile.residency.asset_budget_bytes {
        return Err(DerivationError::ResidencyBudgetExceeded {
            needed_bytes: resident_payload_bytes,
            budget_bytes: profile.residency.asset_budget_bytes,
        });
    }
    let stored_payload_bytes: u64 = entries.values().map(EncodedObservation::stored_bytes).sum();

    // Reconstruction graph: dependencies of DELTA/ALBIT/COPY entries.
    // All bases are raw-stored (terminals), so chains are acyclic and
    // depth-bounded by construction (AVR-C04/C05, §12.2).
    let terminals: Vec<ObservationId> = canonical.keys().map(|id| ObservationId(*id)).collect();
    let reconstruction = ReconstructionGraph::new(
        reconstruction_nodes,
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
    let mut inventory = ObservationInventory::new(
        report.canonical_digest.clone(),
        report.inventory.domain.clone(),
        records,
        navigation,
        Vec::new(),
        residency,
        anchors,
        neighbourhoods,
        provenance,
        profile.reconstruction.max_dependency_depth,
    )
    .map_err(DerivationError::Inventory)?;
    inventory.reconstruction = reconstruction;

    let directly_indexed = if profile.profile == InventoryProfile::Dense {
        canonical.len()
    } else {
        inventory.anchors.len()
    };
    let canonical_count = canonical.len() as u32;
    let resident_count = entries.values().filter(|e| e.tier() == Tier::Hot).count() as u32;
    let metrics = DerivationMetrics {
        canonical_observations: canonical_count,
        directly_indexed: directly_indexed as u32,
        resident_observations: resident_count,
        retained_observations: entries.len() as u32,
        residency_reduction_bp: if canonical_count > 0 {
            ((1.0 - resident_count as f64 / canonical_count as f64) * 10_000.0) as u32
        } else {
            0
        },
        resident_payload_bytes,
        canonical_payload_bytes,
        stored_payload_bytes,
        storage_reduction_bp: if canonical_payload_bytes > 0 {
            ((1.0 - stored_payload_bytes as f64 / canonical_payload_bytes as f64) * 10_000.0)
                .max(0.0) as u32
        } else {
            0
        },
    };

    let primary_index: BTreeMap<ObservationState, ObservationId> = inventory
        .observations
        .iter()
        .filter(|o| {
            profile.profile == InventoryProfile::Dense
                || inventory.anchors.iter().any(|a| a.observation == o.id)
        })
        .map(|o| (o.state.clone(), o.id))
        .collect();

    Ok(Derivation {
        profile: profile.profile,
        canonical_digest: report.canonical_digest.clone(),
        derivation_digest: derivation_digest(&report.canonical_digest, profile, &payload_digests),
        inventory,
        encodings,
        entries,
        primary_index,
        metrics,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::canonical::{compile, CompilationBudget};
    use crate::derivation::{IndexPolicy, ReconstructionPolicy, ResidencyPolicy};
    use crate::domain::ObservationDomain;
    use crate::engine::tests::TestEngine;
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

    /// Source whose payloads carry redundant content (`OBS <az> <el> dup
    /// <az> <el>`): long enough for truncation to declare a non-trivial
    /// error and redundant enough for parametric reconstruction to beat
    /// raw storage (§12.5).
    pub(crate) struct UpperSource;
    impl SpatialSource for UpperSource {
        fn source_digest(&self) -> String {
            "upper:1".into()
        }
        fn render(&self, o: &Observer) -> Result<ObservationPayload, SourceError> {
            let (az, el) = (
                crate::ingestion::quantise_angle(o.azimuth_deg),
                crate::ingestion::quantise_angle(o.elevation_deg),
            );
            Ok(ObservationPayload {
                representation: RepresentationClass::Vec,
                bytes: format!("OBS {az} {el} dup {az} {el}").into_bytes(),
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
            let derivation =
                derive(&report, &profile(kind, &report.canonical_digest, 32, 96), None)
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
            let a = derive(&report, &profile(kind, &report.canonical_digest, 16, 64), None)
                .expect("a");
            let b = derive(&report, &profile(kind, &report.canonical_digest, 16, 64), None)
                .expect("b");
            assert_eq!(a.derivation_digest, b.derivation_digest, "AVR-G04");
            assert_eq!(a.inventory.anchors, b.inventory.anchors);
            assert_eq!(a.encodings, b.encodings);
            assert_eq!(a.entries, b.entries);
        }
    }

    #[test]
    fn profile_digest_mismatch_is_rejected() {
        let calls = Cell::new(0);
        let report = orbital_report(16, &calls);
        let mut p = profile(InventoryProfile::Relational, &report.canonical_digest, 4, 16);
        p.canonical_digest = "sha256:stale".into();
        assert_eq!(
            derive(&report, &p, None).unwrap_err(),
            DerivationError::CanonicalDigestMismatch {
                profile: "sha256:stale".into(),
                canonical: report.canonical_digest.clone(),
            }
        );
    }

    #[test]
    fn encodings_outside_the_phase_3_set_are_rejected() {
        let calls = Cell::new(0);
        let report = orbital_report(16, &calls);
        let mut p = profile(InventoryProfile::Relational, &report.canonical_digest, 4, 16);
        p.reconstruction.allowed_encodings.push(EncodingClass::Live);
        assert_eq!(
            derive(&report, &p, None).unwrap_err(),
            DerivationError::EncodingNotAvailable(EncodingClass::Live)
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
            derive(&report, &dense, None).unwrap_err(),
            DerivationError::ResidencyBudgetExceeded { .. }
        ));
        let mut relational =
            profile(InventoryProfile::Relational, &report.canonical_digest, 8, 64);
        relational.residency.asset_budget_bytes = 1;
        assert!(matches!(
            derive(&report, &relational, None).unwrap_err(),
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
            None,
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
            None,
        )
        .expect("derivation");
        // §8.2: every observation is stored exactly once.
        assert_eq!(derivation.entries.len(), 64);
        assert_eq!(
            derivation.metrics.retained_observations, 64,
            "nothing dropped, nothing duplicated"
        );
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
        let dense =
            derive(&report, &profile(InventoryProfile::Dense, &report.canonical_digest, 64, 96), None)
                .expect("dense");
        assert_eq!(dense.metrics.directly_indexed, 64);
        assert_eq!(dense.metrics.resident_observations, 64);
        assert!(dense
            .entries
            .values()
            .all(|e| e.tier() == Tier::Hot));

        let hybrid =
            derive(&report, &profile(InventoryProfile::Hybrid, &report.canonical_digest, 8, 48), None)
                .expect("hybrid");
        assert_eq!(hybrid.metrics.directly_indexed, 8);
        assert_eq!(hybrid.metrics.resident_observations, 64, "hybrid keeps neighbourhoods resident");

        let relational = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 8, 48),
            None,
        )
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
            None,
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
    fn reconstruction_graph_is_independent_acyclic_and_populated_by_deltas() {
        let calls = Cell::new(0);
        let report = orbital_report(32, &calls);
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 8, 16),
            None,
        )
        .expect("derivation");
        // §8.3: three separated graphs; §12.2: delta nodes with bounded
        // depth, acyclic (every base is a raw-stored anchor).
        derivation.inventory.reconstruction.verify().expect("acyclic");
        let delta_count = derivation
            .encodings
            .values()
            .filter(|e| **e == EncodingClass::Delta)
            .count();
        assert_eq!(
            derivation.inventory.reconstruction.nodes().len(),
            delta_count,
            "every delta contributes exactly one dependency node"
        );
        for node in derivation.inventory.reconstruction.nodes() {
            assert_eq!(node.dependencies.len(), 1, "Phase 3 chains are depth 1");
        }
        assert!(!derivation.inventory.navigation.edges.is_empty());
        assert!(!derivation.inventory.residency.adjacency.is_empty());
    }

    #[test]
    fn similar_observations_delta_compress_dissimilar_do_not() {
        // The CountingSource payloads share their prefix per elevation;
        // verify the §12.5 selection actually stores less overall.
        let calls = Cell::new(0);
        let report = orbital_report(64, &calls);
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 8, 48),
            None,
        )
        .expect("derivation");
        assert!(
            derivation.metrics.stored_payload_bytes <= derivation.metrics.canonical_payload_bytes,
            "encoding selection must never exceed the raw size (§12.5)"
        );

        // Dissimilar payloads: the delta is bigger and must not be chosen.
        struct NoisySource;
        impl SpatialSource for NoisySource {
            fn source_digest(&self) -> String {
                "noisy:1".into()
            }
            fn render(&self, o: &Observer) -> Result<ObservationPayload, SourceError> {
                Ok(ObservationPayload {
                    representation: RepresentationClass::Vec,
                    bytes: format!(
                        "unique-{}-{}-payload with no shared structure",
                        o.azimuth_deg, o.elevation_deg
                    )
                    .into_bytes(),
                })
            }
        }
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: 16,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let report = compile(
            &NoisySource,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 64, max_payload_bytes: u64::MAX },
        )
        .expect("compilation");
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 4, 16),
            None,
        )
        .expect("derivation");
        assert!(
            derivation.encodings.values().all(|e| *e != EncodingClass::Delta),
            "deltas that do not pay for themselves must not be selected"
        );
        // COPY also must not fire: payloads differ.
        assert!(derivation.encodings.values().all(|e| *e != EncodingClass::Copy));
    }

    #[test]
    fn exact_albit_requires_byte_verification_not_similarity() {
        // The test engine's "uppercase" op is byte-exact for payloads that
        // are already uppercase; the counting source's are lowercase, so
        // the op does not verify exact — and must NOT be encoded as exact.
        let calls = Cell::new(0);
        let report = orbital_report(32, &calls);
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 4, 32),
            Some(&TestEngine),
        )
        .expect("derivation");
        for encoding in derivation.encodings.values() {
            if *encoding == EncodingClass::Albit {
                panic!("an unverified op must never be certified exact (AVR-C09)");
            }
        }

        // Payloads are uppercase and redundant: "reconstruct" is
        // byte-exact and gets certified; "truncate" is approximate and
        // must respect the profile's error policy.
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: 32,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let report = compile(
            &UpperSource,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 64, max_payload_bytes: u64::MAX },
        )
        .expect("compilation");
        let derivation = derive(
            &report,
            &profile(InventoryProfile::Relational, &report.canonical_digest, 4, 32),
            Some(&TestEngine),
        )
        .expect("derivation");
        let albits: Vec<&EncodedObservation> = derivation
            .entries
            .values()
            .filter(|e| matches!(e, EncodedObservation::Albit { .. }))
            .collect();
        assert!(!albits.is_empty(), "verified ops should be selected when smaller");
        for entry in albits {
            match entry {
                EncodedObservation::Albit { exact, .. } => {
                    assert!(exact, "uppercase is byte-exact here; nothing else qualifies");
                }
                _ => unreachable!(),
            }
        }
        // Provenance records the exact certification (§12.3).
        assert!(derivation.inventory.provenance.iter().any(|p| matches!(
            p.relation,
            ProvenanceRelation::Albit { exact: true, .. }
        )));
    }

    #[test]
    fn approximate_albit_respects_the_fidelity_policy() {
        // "truncate" is approximate with a declared error; the profile's
        // max_silhouette_error_px gates acceptance (AVR-C10). The long
        // UpperSource payloads give truncation something to remove.
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: 32,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let report = compile(
            &UpperSource,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 64, max_payload_bytes: u64::MAX },
        )
        .expect("compilation");

        let mut strict = profile(InventoryProfile::Relational, &report.canonical_digest, 4, 32);
        strict.max_silhouette_error_px = 0;
        strict
            .reconstruction
            .allowed_encodings
            .extend([EncodingClass::Delta, EncodingClass::Albit]);
        let derivation = derive(&report, &strict, Some(&TestEngine)).expect("strict derivation");
        assert!(
            derivation
                .entries
                .values()
                .all(|e| !matches!(e, EncodedObservation::Albit { exact: false, .. })),
            "zero-error policy rejects approximate reconstructions"
        );

        let mut lenient = profile(InventoryProfile::Relational, &report.canonical_digest, 4, 32);
        lenient.max_silhouette_error_px = 10_000;
        lenient
            .reconstruction
            .allowed_encodings
            .extend([EncodingClass::Delta, EncodingClass::Albit]);
        let derivation = derive(&report, &lenient, Some(&TestEngine)).expect("lenient derivation");
        assert!(
            derivation
                .entries
                .values()
                .any(|e| matches!(e, EncodedObservation::Albit { exact: false, .. })),
            "a declared bound within policy is accepted as Bounded"
        );
        assert!(derivation.inventory.provenance.iter().any(|p| matches!(
            p.relation,
            ProvenanceRelation::Albit { exact: false, .. }
        )));
    }
}
