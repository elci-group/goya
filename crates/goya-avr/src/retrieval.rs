//! Runtime observation resolution — Phase 1: exact retrieval
//! (directive §10.1, resolution hierarchy priorities 1–2 and 8).
//!
//! Phase 1 implements:
//! - domain validation of the requested observation (AVR-C01, AVR-C14);
//! - exact resident observation lookup (priority 1) on the quantised
//!   observation state;
//! - explicit resolution failure (priority 8) for any other request.
//!
//! No fallback exists: a request that is in-domain but not compiled is an
//! explicit `Resolution::Unsupported`, never a silent substitution
//! (§4.3, §10.1). Generative, differential and approximate reconstruction
//! (priorities 3–6) and live rendering (priority 7) arrive with Phases 2–3.

use crate::canonical::CompileReport;
use crate::delta;
use crate::engine::ReconstructionEngine;
use crate::ingestion::{observer_in_domain, ObservationPayload, Observer};
use crate::inventory::FidelityClass;
use crate::target::{Derivation, EncodedObservation, Tier};

/// Result of resolving a requested observation.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution<'a> {
    /// Priority 1: exactly this observation state is resident.
    Exact(&'a ObservationPayload),
    /// Priority 8: explicit resolution failure. Phase 1 never substitutes.
    Unsupported(UnsupportedReason),
}

/// Why resolution failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// The request lies outside the inventory's observation domain
    /// (AVR-C14: unsupported observation rejection).
    OutsideDomain,
    /// In-domain but not present in the canonical inventory. Locality
    /// accelerates resolution but never restricts valid interaction
    /// (§10.1); a retained-but-unresolvable request fails explicitly.
    NotCompiled,
    /// A reconstruction chain could not be materialised: the base entry
    /// is missing, the depth bound is violated, no engine is available
    /// for an `ALBIT` step, or an application failed (§10.3 priority 8).
    ReconstructionUnavailable,
}

/// Resolve a requested observation against a canonical compilation
/// (§10.1 sequence).
pub fn resolve<'a>(report: &'a CompileReport, request: &Observer) -> Resolution<'a> {
    // Step 1–2: validate the observation domain.
    if !observer_in_domain(&report.inventory.domain, request) {
        return Resolution::Unsupported(UnsupportedReason::OutsideDomain);
    }
    // Step 3–4: exact lookup on the quantised state. Requests outside the
    // compiled mesh but inside the domain are valid interaction; Phase 1
    // rejects them explicitly (§10.1: locality accelerates, never restricts
    // — the *restriction* here is inventory coverage, reported honestly).
    let state = request.normalised().to_state();
    match report
        .inventory
        .observations
        .iter()
        .find(|record| record.state == state)
        .and_then(|record| report.store.get(record.id))
    {
        Some(payload) => Resolution::Exact(payload),
        None => Resolution::Unsupported(UnsupportedReason::NotCompiled),
    }
}

/// Result of resolving against a derived (target-specific) inventory.
#[derive(Debug, Clone, PartialEq)]
pub enum DerivedResolution {
    /// Priorities 1–5: the observation resolved to a concrete payload,
    /// either stored directly or materialised through reconstruction
    /// (copy, differential or generative). `fidelity` distinguishes an
    /// exact payload from a certified bounded reconstruction (§10.3).
    Exact {
        payload: ObservationPayload,
        tier: Tier,
        fidelity: FidelityClass,
    },
    /// Priority 8: explicit resolution failure — no substitution.
    Unsupported(UnsupportedReason),
}

/// Resolve a request against a derived inventory (§10.1).
///
/// The primary index is consulted first; a relational miss falls through
/// to the neighbourhood search over the retained observation table.
/// Locality accelerates resolution but never restricts valid interaction:
/// any in-domain request that the derivation retains resolves, regardless
/// of the neighbourhood the runtime currently occupies.
///
/// Encoded entries (`COPY`, `DELTA`, `ALBIT`) are materialised through
/// their reconstruction chains, bounded by the inventory's maximum
/// dependency depth (§12.2). A missing engine or a failed application is
/// an explicit `Unsupported`, never a silent substitution (§4.3).
pub fn resolve_derived(
    derivation: &Derivation,
    request: &Observer,
    engine: Option<&dyn ReconstructionEngine>,
) -> DerivedResolution {
    if !observer_in_domain(&derivation.inventory.domain, request) {
        return DerivedResolution::Unsupported(UnsupportedReason::OutsideDomain);
    }
    let state = request.normalised().to_state();

    // Priority 1: primary index (directly addressable).
    if let Some(id) = derivation.primary_index.get(&state) {
        if let Some(entry) = derivation.entries.get(&id.0) {
            return materialise(derivation, entry, engine);
        }
    }
    // Priorities 2–5: exact observation retained in the derived inventory
    // (cold tier / neighbourhood storage), possibly via reconstruction.
    if let Some(record) = derivation
        .inventory
        .observations
        .iter()
        .find(|record| record.state == state)
    {
        if let Some(entry) = derivation.entries.get(&record.id.0) {
            return materialise(derivation, entry, engine);
        }
    }
    DerivedResolution::Unsupported(UnsupportedReason::NotCompiled)
}

/// Materialise an entry through its reconstruction chain (§10.3
/// priorities 1–5). `COPY` walks to its raw referent; `DELTA` and `ALBIT`
/// materialise their base and apply the transformation. The chain depth
/// is bounded by the reconstruction graph's maximum depth (§12.2), and
/// the resolved payload is `Bounded` if any approximate step participated.
fn materialise(
    derivation: &Derivation,
    entry: &EncodedObservation,
    engine: Option<&dyn ReconstructionEngine>,
) -> DerivedResolution {
    let tier = entry.tier();
    let mut exact = true;
    match materialise_payload(derivation, entry, engine, 0, &mut exact) {
        Ok(payload) => DerivedResolution::Exact {
            payload,
            tier,
            fidelity: if exact {
                FidelityClass::Exact
            } else {
                FidelityClass::Bounded
            },
        },
        Err(reason) => DerivedResolution::Unsupported(reason),
    }
}

/// Walk one entry to a concrete payload. `exact` is cleared by any
/// approximate `ALBIT` step in the chain (§12.3: exact operations are
/// certified at encoding time, not re-derived here).
fn materialise_payload(
    derivation: &Derivation,
    entry: &EncodedObservation,
    engine: Option<&dyn ReconstructionEngine>,
    depth: u8,
    exact: &mut bool,
) -> Result<ObservationPayload, UnsupportedReason> {
    if depth > derivation.inventory.reconstruction.max_depth() {
        // Bounded chains cannot exceed the graph's certified depth
        // (AVR-C05); treat a violation as resolution failure.
        return Err(UnsupportedReason::ReconstructionUnavailable);
    }
    match entry {
        EncodedObservation::Raw { payload, .. } => Ok(payload.clone()),
        EncodedObservation::Copy { of, .. } => {
            let base = derivation
                .entries
                .get(of)
                .ok_or(UnsupportedReason::ReconstructionUnavailable)?;
            materialise_payload(derivation, base, engine, depth + 1, exact)
        }
        EncodedObservation::Delta { base, delta, .. } => {
            let base_entry = derivation
                .entries
                .get(base)
                .ok_or(UnsupportedReason::ReconstructionUnavailable)?;
            let base_payload =
                materialise_payload(derivation, base_entry, engine, depth + 1, exact)?;
            // Differential reconstruction: the delta self-verified at
            // encoding time (AVR-C09); a failed application here means
            // the inventory or engine contract is broken — fail explicitly.
            let representation = base_payload.representation;
            delta::apply(&base_payload.bytes, delta)
                .map(|bytes| ObservationPayload { representation, bytes })
                .map_err(|_| UnsupportedReason::ReconstructionUnavailable)
        }
        EncodedObservation::Albit {
            base,
            op,
            params,
            exact: op_exact,
            ..
        } => {
            let base_entry = derivation
                .entries
                .get(base)
                .ok_or(UnsupportedReason::ReconstructionUnavailable)?;
            let base_payload =
                materialise_payload(derivation, base_entry, engine, depth + 1, exact)?;
            let engine = engine.ok_or(UnsupportedReason::ReconstructionUnavailable)?;
            let outcome = engine
                .apply(op, params, &base_payload)
                .map_err(|_| UnsupportedReason::ReconstructionUnavailable)?;
            if !op_exact {
                *exact = false;
            }
            Ok(outcome.payload)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{compile, CompilationBudget};
    use crate::domain::ObservationDomain;
    use crate::ingestion::{ObservationPayload, SpatialSource, SourceError};
    use crate::representation::RepresentationClass;
    use std::cell::Cell;

    struct FixedSource;
    impl SpatialSource for FixedSource {
        fn source_digest(&self) -> String {
            "fixed:1".into()
        }
        fn render(
            &self,
            observer: &Observer,
        ) -> Result<ObservationPayload, SourceError> {
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

    struct CountingSource {
        calls: Cell<u32>,
    }
    impl SpatialSource for CountingSource {
        fn source_digest(&self) -> String {
            "counting:1".into()
        }
        fn render(
            &self,
            observer: &Observer,
        ) -> Result<ObservationPayload, SourceError> {
            self.calls.set(self.calls.get() + 1);
            FixedSource.render(observer)
        }
    }

    fn orbital_report(n: u32) -> CompileReport {
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: n,
            distance: 2.0,
            fov_deg: 60.0,
        };
        compile(
            &FixedSource,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 1024, max_payload_bytes: u64::MAX },
        )
        .expect("orbital compilation")
    }

    #[test]
    fn exact_request_resolves_from_the_inventory_without_rerendering() {
        let report = orbital_report(8);
        // Request the exact sampled state: azimuth 90°, elevation 0.
        let request =
            Observer { azimuth_deg: 90.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 };
        let resolution = resolve(&report, &request);
        match &resolution {
            Resolution::Exact(payload) => {
                assert_eq!(payload.bytes, b"obs 9000 0".to_vec());
            }
            other => panic!("expected exact resolution, got {other:?}"),
        }
    }

    #[test]
    fn equivalent_request_with_unnormalised_azimuth_resolves_identically() {
        let report = orbital_report(8);
        let request =
            Observer { azimuth_deg: 450.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 };
        assert!(matches!(resolve(&report, &request), Resolution::Exact(_)));
    }

    #[test]
    fn out_of_domain_request_is_rejected_without_fallback() {
        let report = orbital_report(8);
        let above = Observer {
            azimuth_deg: 0.0,
            elevation_deg: 30.0, // Orbital requires elevation 0
            distance: 2.0,
            fov_deg: 60.0,
        };
        assert_eq!(
            resolve(&report, &above),
            Resolution::Unsupported(UnsupportedReason::OutsideDomain)
        );
    }

    #[test]
    fn in_domain_uncompiled_request_fails_explicitly() {
        let report = orbital_report(8); // azimuths 0, 45, ..., 315
        let between = Observer {
            azimuth_deg: 10.0, // compiled mesh has no 10° observation
            elevation_deg: 0.0,
            distance: 2.0,
            fov_deg: 60.0,
        };
        assert_eq!(
            resolve(&report, &between),
            Resolution::Unsupported(UnsupportedReason::NotCompiled)
        );
    }

    #[test]
    fn resolution_never_invokes_the_source() {
        // The compiler rendered once per sample; resolution must add zero
        // renders (§1: compute once).
        let counting = CountingSource { calls: Cell::new(0) };
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: 4,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let report = compile(
            &counting,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 1024, max_payload_bytes: u64::MAX },
        )
        .expect("compilation");
        assert_eq!(counting.calls.get(), 4);

        let hits = [
            Observer { azimuth_deg: 0.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 },
            Observer { azimuth_deg: 90.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 },
            Observer { azimuth_deg: 180.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 },
            Observer { azimuth_deg: 270.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 },
        ];
        for request in &hits {
            assert!(matches!(resolve(&report, request), Resolution::Exact(_)));
        }
        assert_eq!(counting.calls.get(), 4, "resolution must not re-render");
    }

    // ---- Phase 3: reconstruction-aware derived resolution (§10.3) ----

    use crate::engine::tests::TestEngine;
    use crate::encoding::EncodingClass;
    use crate::derivation::InventoryProfile;
    use crate::target::derive;

    /// Rebuild the grid Observer for a quantised observation state (the
    /// inverse of `Observer::to_state` via `snap`).
    fn observer_of(state: &crate::domain::ObservationState) -> Observer {
        Observer {
            azimuth_deg: state.orientation[0] as f32 / 100.0,
            elevation_deg: state.orientation[1] as f32 / 100.0,
            distance: state.camera[0] as f32 / 256.0,
            fov_deg: 60.0,
        }
    }

    fn upper_report(n: u32) -> CompileReport {
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: n,
            distance: 2.0,
            fov_deg: 60.0,
        };
        compile(
            &crate::target::tests::UpperSource,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 1024, max_payload_bytes: u64::MAX },
        )
        .expect("upper compilation")
    }

    fn encoded_profile(
        report: &CompileReport,
        anchors: u32,
    ) -> crate::derivation::DerivationProfile {
        let mut profile = crate::target::tests::profile(
            InventoryProfile::Relational,
            &report.canonical_digest,
            anchors,
            32,
        );
        profile
            .reconstruction
            .allowed_encodings
            .extend([EncodingClass::Delta, EncodingClass::Albit]);
        profile
    }

    #[test]
    fn exact_albit_resolution_reconstructs_the_canonical_payload() {
        // UpperSource + TestEngine: non-anchor entries are byte-verified
        // exact ALBITs (`reconstruct` op).
        let report = upper_report(32);
        let derivation = derive(
            &report,
            &encoded_profile(&report, 4),
            Some(&TestEngine),
        )
        .expect("derivation");
        let albit_id = derivation
            .entries
            .iter()
            .find(|(_, e)| matches!(e, crate::target::EncodedObservation::Albit { exact: true, .. }))
            .map(|(id, _)| *id)
            .expect("an exact albit entry exists");

        let record = derivation
            .inventory
            .observations
            .iter()
            .find(|r| r.id.0 == albit_id)
            .expect("record");
        let request = observer_of(&record.state);
        let expected = crate::target::tests::UpperSource
            .render(&request)
            .expect("direct render");

        match resolve_derived(&derivation, &request, Some(&TestEngine)) {
            DerivedResolution::Exact { payload, fidelity, .. } => {
                assert_eq!(payload, expected, "reconstruction must be bit-identical");
                assert_eq!(fidelity, FidelityClass::Exact);
            }
            other => panic!("expected exact reconstruction, got {other:?}"),
        }
    }

    #[test]
    fn albit_resolution_without_an_engine_fails_explicitly() {
        // Generative reconstruction is computed, never stored: resolution
        // without the engine is an explicit failure (§12.3, priority 8).
        let report = upper_report(32);
        let derivation = derive(&report, &encoded_profile(&report, 4), Some(&TestEngine))
            .expect("derivation");
        eprintln!("DEBUG {:?}", derivation.encodings.values().collect::<Vec<_>>());
        let albit_id = derivation
            .entries
            .iter()
            .find(|(_, e)| matches!(e, crate::target::EncodedObservation::Albit { .. }))
            .map(|(id, _)| *id)
            .expect("an albit entry exists");
        let record = derivation
            .inventory
            .observations
            .iter()
            .find(|r| r.id.0 == albit_id)
            .expect("record");
        let request = observer_of(&record.state);

        assert_eq!(
            resolve_derived(&derivation, &request, None),
            DerivedResolution::Unsupported(UnsupportedReason::ReconstructionUnavailable)
        );
    }

    #[test]
    fn delta_resolution_is_bit_identical_to_the_direct_render() {
        // Multi-line payloads share their header/footer lines, so the
        // line-based delta against the neighbourhood anchor wins (§12.2).
        struct LayeredSource;
        impl SpatialSource for LayeredSource {
            fn source_digest(&self) -> String {
                "layered:1".into()
            }
            fn render(&self, observer: &Observer) -> Result<ObservationPayload, SourceError> {
                let (az, el) = (
                    crate::ingestion::quantise_angle(observer.azimuth_deg),
                    crate::ingestion::quantise_angle(observer.elevation_deg),
                );
                Ok(ObservationPayload {
                    representation: RepresentationClass::Vec,
                    // Large shared lines make the line-based LCS delta much
                    // smaller than either raw payload (§12.5).
                    bytes: format!(
                        "layer\n{az}\n{el}\nshared-material-block\nshared-material-block\nshared-material-block\nend\n"
                    )
                    .into_bytes(),
                })
            }
        }
        let policy = crate::sampling::SamplingPolicy {
            initial_samples: 32,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let report = compile(
            &LayeredSource,
            ObservationDomain::Orbital,
            &policy,
            &CompilationBudget { max_observations: 1024, max_payload_bytes: u64::MAX },
        )
        .expect("compilation");
        let derivation = derive(&report, &encoded_profile(&report, 4), None)
            .expect("derivation");
        assert!(
            derivation
                .encodings
                .values()
                .any(|e| *e == EncodingClass::Delta),
            "layered payloads must delta-compress against their anchor"
        );

        let delta_id = derivation
            .encodings
            .iter()
            .find(|(_, e)| **e == EncodingClass::Delta)
            .map(|(id, _)| *id)
            .expect("a delta entry exists");
        let record = derivation
            .inventory
            .observations
            .iter()
            .find(|r| r.id.0 == delta_id)
            .expect("record");
        let request = observer_of(&record.state);
        let expected = LayeredSource.render(&request).expect("direct render");

        match resolve_derived(&derivation, &request, None) {
            DerivedResolution::Exact { payload, fidelity, .. } => {
                assert_eq!(payload, expected, "delta reconstruction must be bit-identical");
                assert_eq!(fidelity, FidelityClass::Exact);
            }
            other => panic!("expected exact delta resolution, got {other:?}"),
        }
    }
}
