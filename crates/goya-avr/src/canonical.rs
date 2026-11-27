//! Canonical observation compilation (directive §6.4, §7).
//!
//! The canonical compiler renders a finite observation mesh through a
//! [`SpatialSource`](crate::ingestion::SpatialSource) and emits the
//! canonical observation inventory: the reusable source of graphical
//! outcomes for target derivation (§7.1). Phase 1 emits `VEC` payloads
//! with exact per-observation certification; every observation is
//! independently reconstructable (anchor semantics), so the
//! reconstruction graph is empty and relational structure is Phase 2.
//!
//! Budget exhaustion produces an explicit incomplete-coverage result
//! rather than an unsupported certification claim (§6.4 step 8).

use crate::domain::ObservationDomain;
use crate::graph::ObservationId;
use crate::ingestion::{observer_in_domain, SpatialSource, SourceError};
use crate::inventory::{
    FidelityClass, FidelityRecord, ObservationInventory, ObservationRecord, RepresentationId,
};
use crate::sampling::{sample_domain, SamplingError, SamplingPolicy};
use std::collections::BTreeMap;

/// Compilation budget (§6.4). There is no `Default`: budgets are explicit
/// compilation decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilationBudget {
    /// Maximum observations the compilation may generate.
    pub max_observations: u32,
    /// Maximum total encoded payload bytes.
    pub max_payload_bytes: u64,
}

/// Why a canonical compilation failed outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// The sampling policy or domain was rejected (AVR-C01).
    Sampling(SamplingError),
    /// The domain rejected an observer produced by its own mesh — an
    /// internal inconsistency and always a bug in the sampler.
    SampleOutsideDomain { index: u32 },
    /// The source failed to render an observation.
    Source(SourceError),
    /// The budget's observation limit is below the mesh size (§6.4 step 8:
    /// stop, report, never silently truncate).
    BudgetExhausted { requested: u32, allowed: u32 },
}

/// Stored payloads of a canonical inventory, keyed by observation id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalStore {
    payloads: BTreeMap<u64, crate::ingestion::ObservationPayload>,
}

impl CanonicalStore {
    pub fn get(&self, id: ObservationId) -> Option<&crate::ingestion::ObservationPayload> {
        self.payloads.get(&id.0)
    }

    /// Insert a rendered payload (used by the navigation compiler, which
    /// builds stores incrementally per node).
    pub fn insert(&mut self, id: u64, payload: crate::ingestion::ObservationPayload) {
        self.payloads.insert(id, payload);
    }

    pub fn len(&self) -> usize {
        self.payloads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.payloads.is_empty()
    }

    /// Total encoded payload bytes held by the store.
    pub fn payload_bytes(&self) -> u64 {
        self.payloads.values().map(|p| p.bytes.len() as u64).sum()
    }
}

/// Result of a canonical compilation (§6.4 steps 9–10).
#[derive(Debug, Clone, PartialEq)]
pub struct CompileReport {
    pub inventory: ObservationInventory,
    pub store: CanonicalStore,
    /// True when every requested observation was generated and certified.
    /// False means budget exhaustion: coverage is explicitly incomplete
    /// and MUST NOT be presented as a certification claim.
    pub complete: bool,
    pub requested: u32,
    pub generated: u32,
    /// Canonical identity digest (§7.3): source digest + contract identity
    /// + domain + sampling policy + sorted payload digests.
    pub canonical_digest: String,
}

/// Deterministic canonical digest (§7.3).
pub(crate) fn canonical_digest(
    source_digest: &str,
    domain: &ObservationDomain,
    policy: &SamplingPolicy,
    payload_digests: &[String],
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(crate::DIRECTIVE_ID.as_bytes());
    hasher.update(crate::CONTRACT_VERSION.as_bytes());
    hasher.update(source_digest.as_bytes());
    hasher.update(format!("{domain:?}").as_bytes());
    hasher.update(format!("{policy:?}").as_bytes());
    for digest in payload_digests {
        hasher.update(digest.as_bytes());
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// §7.3 identity for navigation environments: the environment digest,
/// navigation spec and policy join the payload digests.
pub(crate) fn navigation_digest(
    source_digest: &str,
    spec: &crate::navigation::NavigationSpec,
    policy: &SamplingPolicy,
    payload_digests: &[String],
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(crate::DIRECTIVE_ID.as_bytes());
    hasher.update(crate::CONTRACT_VERSION.as_bytes());
    hasher.update(source_digest.as_bytes());
    hasher.update(format!("{spec:?}").as_bytes());
    hasher.update(format!("{policy:?}").as_bytes());
    for digest in payload_digests {
        hasher.update(digest.as_bytes());
    }
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// Compile the canonical observation inventory for a static model.
pub fn compile(
    source: &dyn SpatialSource,
    domain: ObservationDomain,
    policy: &SamplingPolicy,
    budget: &CompilationBudget,
) -> Result<CompileReport, CompileError> {
    if !domain.is_valid() {
        return Err(CompileError::Sampling(SamplingError::UnsupportedDomain));
    }
    let samples = sample_domain(&domain, policy).map_err(CompileError::Sampling)?;
    if samples.len() as u32 > budget.max_observations {
        return Err(CompileError::BudgetExhausted {
            requested: samples.len() as u32,
            allowed: budget.max_observations,
        });
    }

    let mut records = Vec::with_capacity(samples.len());
    let mut store = CanonicalStore::default();
    let mut payload_digests = Vec::with_capacity(samples.len());
    let mut total_bytes: u64 = 0;

    for (index, observer) in samples.iter().enumerate() {
        // A sampler that emits observers outside its own domain is broken;
        // fail loudly instead of certifying an out-of-domain observation
        // (AVR-C01/AVR-C14).
        if !observer_in_domain(&domain, observer) {
            return Err(CompileError::SampleOutsideDomain { index: index as u32 });
        }
        let payload = source.render(observer).map_err(CompileError::Source)?;
        total_bytes += payload.bytes.len() as u64;
        if total_bytes > budget.max_payload_bytes {
            return Err(CompileError::BudgetExhausted {
                requested: samples.len() as u32,
                allowed: budget.max_observations,
            });
        }
        payload_digests.push(payload.digest());
        let id = ObservationId(index as u64);
        store.payloads.insert(id.0, payload);
        records.push(ObservationRecord {
            id,
            state: observer.to_state(),
            representation: RepresentationId(0),
            provenance: None,
            fidelity: FidelityRecord {
                classification: FidelityClass::Exact,
                error_bounds: [0; 4],
                verification_method: "direct canonical render",
                coverage: crate::graph::ObservationCoordinates {
                    spatial: [
                        observer.azimuth_deg.to_bits() as i64,
                        observer.elevation_deg.to_bits() as i64,
                        observer.distance.to_bits() as i64,
                    ],
                },
            },
        });
    }

    let digest = canonical_digest(&source.source_digest(), &domain, policy, &payload_digests);
    let report = CompileReport {
        inventory: ObservationInventory::new(
            digest.clone(),
            domain,
            records,
            crate::graph::NavigationGraph::default(),
            Vec::new(),
            crate::graph::ResidencyGraph::default(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            crate::graph::MAX_DEPENDENCY_DEPTH,
        )
        .expect("canonical compilation constructs a well-formed inventory"),
        store,
        complete: true,
        requested: samples.len() as u32,
        generated: samples.len() as u32,
        canonical_digest: digest,
    };
    Ok(report)
}

/// Determinism check helper: the quantised states of two compilations of
/// the same model/policy are byte-identical (§7.3, same-host).
pub fn states_are_reproducible(a: &CompileReport, b: &CompileReport) -> bool {
    a.inventory.observations.iter().map(|o| &o.state).eq(b.inventory.observations.iter().map(|o| &o.state))
        && a.canonical_digest == b.canonical_digest
        && a.store.payloads == b.store.payloads
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::ObservationPayload;
    use crate::representation::RepresentationClass;

    /// Synthetic source standing in for the Rugid bridge in unit tests.
    struct FakeSource {
        render_calls: std::cell::Cell<u32>,
        fail_from: Option<u32>,
    }

    impl FakeSource {
        fn new() -> Self {
            Self { render_calls: std::cell::Cell::new(0), fail_from: None }
        }
    }

    impl SpatialSource for FakeSource {
        fn source_digest(&self) -> String {
            "fake:cube/1".into()
        }

        fn render(&self, observer: &crate::ingestion::Observer) -> Result<ObservationPayload, SourceError> {
            let call = self.render_calls.get();
            self.render_calls.set(call + 1);
            if self.fail_from.is_some_and(|from| call >= from) {
                return Err(SourceError::RenderFailed("injected".into()));
            }
            let bytes = format!(
                "obs az={} el={} d={}",
                crate::ingestion::quantise_angle(observer.azimuth_deg),
                crate::ingestion::quantise_angle(observer.elevation_deg),
                crate::ingestion::quantise_distance(observer.distance),
            )
            .into_bytes();
            Ok(ObservationPayload {
                representation: RepresentationClass::Vec,
                bytes,
            })
        }
    }

    fn budget(n: u32) -> CompilationBudget {
        CompilationBudget { max_observations: n, max_payload_bytes: u64::MAX }
    }

    #[test]
    fn compilation_is_complete_deterministic_and_exact() {
        let domain = ObservationDomain::Orbital;
        let policy = SamplingPolicy { initial_samples: 16, distance: 2.0, fov_deg: 60.0 };
        let a = compile(&FakeSource::new(), domain.clone(), &policy, &budget(32))
            .expect("compilation succeeds");
        let b = compile(&FakeSource::new(), domain, &policy, &budget(32))
            .expect("compilation succeeds");

        assert!(a.complete);
        assert_eq!(a.generated, 16);
        assert!(states_are_reproducible(&a, &b));
        assert_eq!(a.canonical_digest, b.canonical_digest);
        assert!(a.canonical_digest.starts_with("sha256:"));

        for record in &a.inventory.observations {
            assert_eq!(record.fidelity.classification, FidelityClass::Exact);
        }
        assert_eq!(a.store.len(), 16);
    }

    #[test]
    fn budget_below_mesh_size_is_an_explicit_error() {
        let policy = SamplingPolicy { initial_samples: 16, distance: 2.0, fov_deg: 60.0 };
        let err = compile(&FakeSource::new(), ObservationDomain::Orbital, &policy, &budget(8))
            .unwrap_err();
        assert_eq!(err, CompileError::BudgetExhausted { requested: 16, allowed: 8 });
    }

    #[test]
    fn payload_budget_is_an_explicit_error() {
        let policy = SamplingPolicy { initial_samples: 16, distance: 2.0, fov_deg: 60.0 };
        let tiny = CompilationBudget { max_observations: 32, max_payload_bytes: 10 };
        let err = compile(&FakeSource::new(), ObservationDomain::Orbital, &policy, &tiny)
            .unwrap_err();
        assert!(matches!(err, CompileError::BudgetExhausted { .. }));
    }

    #[test]
    fn source_failure_is_reported_not_swallowed() {
        let mut source = FakeSource::new();
        source.fail_from = Some(3);
        let policy = SamplingPolicy { initial_samples: 16, distance: 2.0, fov_deg: 60.0 };
        let err = compile(&source, ObservationDomain::Orbital, &policy, &budget(32))
            .unwrap_err();
        assert_eq!(err, CompileError::Source(SourceError::RenderFailed("injected".into())));
    }

    #[test]
    fn quantise_angle_helper_is_exposed_consistently() {
        assert_eq!(crate::ingestion::quantise_angle(0.0), 0);
        assert_eq!(crate::ingestion::quantise_angle(90.0), 9000);
    }
}
