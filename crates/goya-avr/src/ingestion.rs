//! AVR→Rugid ingestion contract (ADR-AVR-001 F1, directive WS-01/WS-03).
//!
//! Phase 1 decision: a canonical AVR compiler consumes a Rugid model
//! through the [`SpatialSource`] trait — the audited state subset is the
//! static model definition (shape/geometry, materials, lighting) plus one
//! parametrised observer per observation. Animation, simulation, FSM and
//! OCRS weight-space state are out of Phase 1 scope.
//!
//! Camera-model decision: Phase 1 uses the **AVR-side parametrised
//! observer** ([`Observer`]). For object-centric models an observer is
//! equivalent to an inverse model rotation (Rugid's render path has no
//! camera node; its fixed camera at −Z looks at a rotated model), so the
//! bridge maps observer angles onto `Rotation3D`. Environment cameras
//! (`rugid.viewport` observation, directive §5.3) stay deferred to Phase 6.
//!
//! Quantisation rule: Rugid's spatial pipeline is `f32` throughout, so
//! observation coordinates entering the deterministic inventory are
//! quantised here. Same-host byte-identical output holds; cross-host
//! reproducibility is tolerance-based (directive §7.3).

use crate::domain::ObservationDomain;
use crate::representation::RepresentationClass;

/// Fixed-point scales for the quantisation rule (ADR-AVR-001 F1).
/// Angles: centi-degrees. Distances: 1/256 spatial units.
pub const ANGLE_QUANTISATION: f32 = 100.0;
pub const DISTANCE_QUANTISATION: f32 = 256.0;

/// Round half to even (banker's rounding), matching the GVI arithmetic
/// convention (`docs/graphics/gvi.md` §1: exact products rounded
/// half-to-even).
pub fn round_half_even(x: f32) -> f32 {
    let floor = x.floor();
    let diff = x - floor;
    if diff > 0.5 {
        floor + 1.0
    } else if diff < 0.5 {
        floor
    } else if (floor as i64) % 2 == 0 {
        // Exactly .5: round towards the even neighbour.
        floor
    } else {
        floor + 1.0
    }
}

pub fn quantise_angle(degrees: f32) -> i64 {
    round_half_even(degrees * ANGLE_QUANTISATION) as i64
}

pub fn quantise_distance(units: f32) -> i64 {
    round_half_even(units * DISTANCE_QUANTISATION) as i64
}

/// AVR-side parametrised observer: the Phase 1 camera model.
///
/// `azimuth_deg` is normalised into `[0, 360)`; `elevation_deg` lies in
/// `[-90, 90]` (Spherical), `[0, 90]` (Hemispherical) or is exactly `0`
/// (Orbital), per the observation domain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observer {
    pub azimuth_deg: f32,
    pub elevation_deg: f32,
    /// Observer distance in spatial units; zoom is distance-driven.
    pub distance: f32,
    /// Field of view in degrees. Recorded in the observation state but not
    /// varied by Phase 1 domains.
    pub fov_deg: f32,
}

impl Observer {
    /// Normalise the observer's azimuth into `[0, 360)`.
    pub fn normalised(self) -> Self {
        let mut azimuth = self.azimuth_deg % 360.0;
        if azimuth < 0.0 {
            azimuth += 360.0;
        }
        Self { azimuth_deg: azimuth, ..self }
    }

    /// Quantised observation state coordinates (directive §6.1).
    ///
    /// Mapping: orientation holds quantised azimuth/elevation; camera holds
    /// the quantised distance on the x axis. FOV is folded into the
    /// distance axis slot's neighbour: recorded in `zoom` as centi-degrees
    /// when non-default is ever needed — Phase 1 keeps it constant and
    /// out of the state to keep identity minimal.
    pub fn to_state(&self) -> crate::domain::ObservationState {
        let o = self.normalised();
        crate::domain::ObservationState {
            camera: [quantise_distance(o.distance), 0, 0],
            orientation: [quantise_angle(o.azimuth_deg), quantise_angle(o.elevation_deg), 0],
            zoom: None,
            node: None,
            material_revision: 0,
            environment_revision: 0,
        }
    }
}

/// Snap an observer onto the quantisation grid: quantise, then
/// dequantise. Canonical payloads MUST be a function of the quantised
/// observation state (§7.3), so sources render `snap(observer)`, never the
/// raw observer. `snap` is idempotent and `snap(o).to_state() ==
/// o.to_state()`.
pub fn snap(observer: &Observer) -> Observer {
    let state = observer.normalised().to_state();
    Observer {
        azimuth_deg: state.orientation[0] as f32 / ANGLE_QUANTISATION,
        elevation_deg: state.orientation[1] as f32 / ANGLE_QUANTISATION,
        distance: state.camera[0] as f32 / DISTANCE_QUANTISATION,
        fov_deg: observer.fov_deg,
    }
}

/// Why a source could not render an observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The renderer failed for this observation.
    RenderFailed(String),
    /// The observer is outside the source's supported region.
    UnsupportedObservation,
}

/// One rendered observation: an abstract graphical outcome (§4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationPayload {
    pub representation: RepresentationClass,
    pub bytes: Vec<u8>,
}

impl ObservationPayload {
    pub fn digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.representation.id().as_bytes());
        hasher.update(&self.bytes);
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// The static Rugid model subset a canonical AVR compiler consumes.
///
/// Implementations must be deterministic: the same observer must always
/// produce the same payload on the same host build (§7.3).
pub trait SpatialSource {
    /// Digest of the model definition (geometry, materials, lighting) —
    /// the "source asset digest" of canonical identity (§7.3).
    fn source_digest(&self) -> String;

    /// Render one observation of the model as seen by `observer`.
    fn render(&self, observer: &Observer) -> Result<ObservationPayload, SourceError>;
}

/// Whether `observer` lies inside `domain`'s permitted observation space
/// (AVR-C01). Azimuth is compared after normalisation.
pub fn observer_in_domain(domain: &ObservationDomain, observer: &Observer) -> bool {
    let o = observer.normalised();
    let elevation_ok = match domain {
        ObservationDomain::Spherical => (-90.0..=90.0).contains(&o.elevation_deg),
        ObservationDomain::Hemispherical => (0.0..=90.0).contains(&o.elevation_deg),
        ObservationDomain::Orbital => o.elevation_deg == 0.0,
        _ => return false, // environment domains are Phase 6 (ADR-AVR-001 F1)
    };
    elevation_ok && o.distance > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_half_even_matches_gvi_convention() {
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(3.5), 4.0);
        assert_eq!(round_half_even(0.4), 0.0);
        assert_eq!(round_half_even(0.6), 1.0);
        assert_eq!(round_half_even(-2.5), -2.0);
    }

    #[test]
    fn observer_azimuth_normalises_into_half_open_range() {
        let o = Observer { azimuth_deg: 370.5, elevation_deg: 10.0, distance: 1.0, fov_deg: 60.0 };
        assert!((o.normalised().azimuth_deg - 10.5).abs() < 1e-4);
        let neg = Observer { azimuth_deg: -10.0, elevation_deg: 0.0, distance: 1.0, fov_deg: 60.0 };
        assert!((neg.normalised().azimuth_deg - 350.0).abs() < 1e-4);
    }

    #[test]
    fn state_coordinates_are_quantised_deterministically() {
        let o = Observer { azimuth_deg: 45.0, elevation_deg: -30.0, distance: 2.0, fov_deg: 60.0 };
        let state = o.to_state();
        assert_eq!(state.orientation[0], 4500);
        assert_eq!(state.orientation[1], -3000);
        assert_eq!(state.camera[0], 512);
        // Same observer, same state.
        assert_eq!(o.normalised().to_state(), state);
    }

    #[test]
    fn domain_acceptance_matches_directive_table() {
        let at_horizon = Observer { azimuth_deg: 0.0, elevation_deg: 0.0, distance: 1.0, fov_deg: 60.0 };
        let above = Observer { azimuth_deg: 0.0, elevation_deg: 45.0, distance: 1.0, fov_deg: 60.0 };
        let below = Observer { azimuth_deg: 0.0, elevation_deg: -45.0, distance: 1.0, fov_deg: 60.0 };
        let orbital_only = Observer { azimuth_deg: 0.0, elevation_deg: 0.001, distance: 1.0, fov_deg: 60.0 };

        assert!(observer_in_domain(&ObservationDomain::Spherical, &below));
        assert!(observer_in_domain(&ObservationDomain::Hemispherical, &above));
        assert!(!observer_in_domain(&ObservationDomain::Hemispherical, &below));
        assert!(!observer_in_domain(&ObservationDomain::Orbital, &orbital_only));
        assert!(observer_in_domain(&ObservationDomain::Orbital, &at_horizon));
        // Environment domains are not Phase 1.
        assert!(!observer_in_domain(
            &ObservationDomain::NavigationGraph { nodes: 4 },
            &at_horizon
        ));
    }

    #[test]
    fn snap_is_idempotent_and_state_preserving() {
        let raw = Observer { azimuth_deg: 45.1234, elevation_deg: -30.5678, distance: 2.0347, fov_deg: 60.0 };
        let snapped = snap(&raw);
        assert_eq!(snapped.to_state(), raw.to_state());
        assert_eq!(snap(&snapped), snapped);
        // The snapped observer's coordinates lie exactly on the grid.
        assert_eq!(quantise_angle(snapped.azimuth_deg) as f32, snapped.azimuth_deg * ANGLE_QUANTISATION);
    }

    #[test]
    fn payload_digest_is_representation_sensitive() {
        let a = ObservationPayload {
            representation: RepresentationClass::Vec,
            bytes: b"svg".to_vec(),
        };
        let b = ObservationPayload {
            representation: RepresentationClass::Ras,
            bytes: b"svg".to_vec(),
        };
        assert_ne!(a.digest(), b.digest());
        assert_eq!(a.digest(), a.digest());
    }
}
