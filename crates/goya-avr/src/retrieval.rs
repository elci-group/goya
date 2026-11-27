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
use crate::ingestion::{observer_in_domain, ObservationPayload, Observer};

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
    /// In-domain but not present in the canonical inventory. Locality,
    /// reconstruction and live fallback are later phases; Phase 1 fails
    /// explicitly.
    NotCompiled,
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
}
