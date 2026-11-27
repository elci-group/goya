//! Deterministic observation sampling (directive §6.2).
//!
//! Phase 1 scope: static object models over the `Spherical`,
//! `Hemispherical` and `Orbital` domains, sampled on a deterministic mesh.
//! Adaptive refinement (§6.4 steps 4–7) and weighted acceptance (§6.3) are
//! Phase 2; the Phase 1 mesh is uniform and its size is the compilation's
//! observational granularity.
//!
//! The mesh uses the Fibonacci sphere (golden-angle increment): every
//! sample is a pure function of its index and the sample count, so the
//! sample set is reproducible without stored randomness (§7.3).

use crate::domain::ObservationDomain;
use crate::ingestion::Observer;

/// Sampling configuration. There is no `Default`: sample count is a
/// compilation decision that shows up in canonical identity (§7.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplingPolicy {
    /// Number of observations to request from the domain mesh.
    pub initial_samples: u32,
    /// Observer distance for the initial mesh (zoom bands are Phase 2).
    pub distance: f32,
    /// Field of view in degrees.
    pub fov_deg: f32,
}

/// Why a domain could not be sampled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SamplingError {
    /// The domain is invalid or out of Phase 1 scope (AVR-C01).
    UnsupportedDomain,
    /// The requested sample count exceeds the domain's declared bound
    /// (AVR-C02) or the budget must reject it upstream.
    SampleCountExceedsDomainBound { requested: u32, bound: u64 },
    /// The sampling policy itself is not usable.
    InvalidPolicy,
}

/// Golden angle in degrees: `360°/φ²`.
const GOLDEN_ANGLE_DEG: f32 = 137.507_76;

/// Generate the deterministic observation mesh for `domain`.
///
/// Observations are returned in a canonical order: ascending quantised
/// elevation, then ascending quantised azimuth — the compiler assigns
/// observation ids in this order (§7.3 determinism).
pub fn sample_domain(
    domain: &ObservationDomain,
    policy: &SamplingPolicy,
) -> Result<Vec<Observer>, SamplingError> {
    if !domain.is_valid() {
        return Err(SamplingError::UnsupportedDomain);
    }
    if policy.initial_samples == 0 || policy.distance <= 0.0 || policy.fov_deg <= 0.0 {
        return Err(SamplingError::InvalidPolicy);
    }
    if let Some(bound) = domain.observation_bound() {
        if policy.initial_samples as u64 > bound {
            return Err(SamplingError::SampleCountExceedsDomainBound {
                requested: policy.initial_samples,
                bound,
            });
        }
    }

    let n = policy.initial_samples;
    let raw: Vec<Observer> = match domain {
        ObservationDomain::Spherical => (0..n)
            .map(|i| {
                // Fibonacci sphere: y in (-1, 1), golden-angle azimuth.
                let y = 1.0 - (2.0 * i as f32 + 1.0) / n as f32;
                let elevation = y.asin().to_degrees();
                let azimuth = (i as f32 * GOLDEN_ANGLE_DEG) % 360.0;
                Observer {
                    azimuth_deg: azimuth,
                    elevation_deg: elevation,
                    distance: policy.distance,
                    fov_deg: policy.fov_deg,
                }
            })
            .collect(),
        ObservationDomain::Hemispherical => (0..n)
            .map(|i| {
                // Upper hemisphere only: y in (0, 1).
                let y = (2.0 * i as f32 + 1.0) / (2.0 * n as f32);
                let elevation = y.asin().to_degrees();
                let azimuth = (i as f32 * GOLDEN_ANGLE_DEG) % 360.0;
                Observer {
                    azimuth_deg: azimuth,
                    elevation_deg: elevation,
                    distance: policy.distance,
                    fov_deg: policy.fov_deg,
                }
            })
            .collect(),
        ObservationDomain::Orbital => (0..n)
            .map(|i| Observer {
                azimuth_deg: (i as f32) * 360.0 / n as f32,
                elevation_deg: 0.0,
                distance: policy.distance,
                fov_deg: policy.fov_deg,
            })
            .collect(),
        // Environment domains need navigation-graph context (Phase 6).
        _ => return Err(SamplingError::UnsupportedDomain),
    };

    let mut samples: Vec<Observer> = raw.into_iter().map(|o| o.normalised()).collect();
    samples.sort_by(|a, b| {
        let (ea, eb) = (
            crate::ingestion::quantise_angle(a.elevation_deg),
            crate::ingestion::quantise_angle(b.elevation_deg),
        );
        ea.cmp(&eb)
            .then_with(|| {
                crate::ingestion::quantise_angle(a.azimuth_deg)
                    .cmp(&crate::ingestion::quantise_angle(b.azimuth_deg))
            })
    });
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(n: u32) -> SamplingPolicy {
        SamplingPolicy { initial_samples: n, distance: 2.0, fov_deg: 60.0 }
    }

    #[test]
    fn spherical_mesh_is_deterministic_and_canonical_ordered() {
        let domain = ObservationDomain::Spherical;
        let a = sample_domain(&domain, &policy(64)).expect("spherical samples");
        let b = sample_domain(&domain, &policy(64)).expect("spherical samples");
        assert_eq!(a, b, "sampling must be reproducible");
        assert_eq!(a.len(), 64);

        for pair in a.windows(2) {
            let ka = crate::ingestion::quantise_angle(pair[0].elevation_deg);
            let kb = crate::ingestion::quantise_angle(pair[1].elevation_deg);
            assert!(ka <= kb, "samples must be canonically ordered");
        }
        for o in &a {
            assert!((-90.0..=90.0).contains(&o.elevation_deg));
            assert!((0.0..360.0).contains(&o.azimuth_deg));
        }
    }

    #[test]
    fn hemispherical_mesh_stays_above_the_horizon() {
        let samples =
            sample_domain(&ObservationDomain::Hemispherical, &policy(32)).expect("hemisphere");
        assert_eq!(samples.len(), 32);
        for o in &samples {
            assert!((0.0..=90.0).contains(&o.elevation_deg));
        }
    }

    #[test]
    fn orbital_mesh_is_evenly_spaced_on_the_horizon() {
        let samples = sample_domain(&ObservationDomain::Orbital, &policy(8)).expect("orbital");
        assert_eq!(samples.len(), 8);
        for (i, o) in samples.iter().enumerate() {
            assert_eq!(o.elevation_deg, 0.0);
            let expected = i as f32 * 45.0;
            assert!((o.azimuth_deg - expected).abs() < 1e-4);
        }
    }

    #[test]
    fn environment_domains_are_out_of_phase_1_scope() {
        let err =
            sample_domain(&ObservationDomain::NavigationGraph { nodes: 4 }, &policy(4)).unwrap_err();
        assert_eq!(err, SamplingError::UnsupportedDomain);
    }

    #[test]
    fn sample_count_respects_the_domain_bound() {
        let domain = ObservationDomain::Custom {
            caprichio: "spatial.test".into(),
            max_observations: 4,
        };
        let err = sample_domain(&domain, &policy(5)).unwrap_err();
        assert_eq!(
            err,
            SamplingError::SampleCountExceedsDomainBound { requested: 5, bound: 4 }
        );
    }

    #[test]
    fn invalid_policies_are_rejected() {
        assert_eq!(
            sample_domain(&ObservationDomain::Orbital, &policy(0)).unwrap_err(),
            SamplingError::InvalidPolicy
        );
        let zero_distance =
            SamplingPolicy { initial_samples: 4, distance: 0.0, fov_deg: 60.0 };
        assert_eq!(
            sample_domain(&ObservationDomain::Orbital, &zero_distance).unwrap_err(),
            SamplingError::InvalidPolicy
        );
    }
}
