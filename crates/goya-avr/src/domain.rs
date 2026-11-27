//! Observation domains (directive §6.2) and observation state coordinates.
//!
//! A continuous spatial domain MUST never be enumerated without bound
//! (directive §4.4): every variant either carries an explicit finite
//! discretisation or a `SubdivisionPolicy` that bounds it.

/// Permitted observation space for a Rugid model or environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationDomain {
    /// Full spherical object exploration (`domain: spherical`).
    Spherical,
    /// Surface-bound exploration above a ground plane.
    Hemispherical,
    /// Horizontal rotation only.
    Orbital,
    /// Rotation combined with bounded zoom bands.
    SphericalShells {
        /// Inclusive zoom band bounds, in arbitrary spatial units.
        shells: Vec<(u32, u32)>,
    },
    /// Movement constrained to a navigation graph (environmental viewports).
    NavigationGraph {
        /// Number of nodes in the navigation graph. Must be finite.
        nodes: u32,
    },
    /// Complex spatial domains refined adaptively under a bound.
    AdaptiveSubdivision(SubdivisionPolicy),
    /// Custom domains expressed through Caprichios, subject to validation
    /// and boundedness requirements (directive §6.2).
    Custom {
        /// Registered Caprichio declaring the domain.
        caprichio: String,
        /// Explicit upper bound on the number of observations the domain
        /// may generate. A custom domain without a bound is invalid.
        max_observations: u32,
    },
}

impl ObservationDomain {
    /// The declared upper bound on observations this domain may generate.
    ///
    /// Used by bounded-computation checks (AVR-C01, AVR-C02): a compilation
    /// that would exceed this bound must stop with an explicit
    /// incomplete-coverage result rather than silently truncate.
    pub fn observation_bound(&self) -> Option<u64> {
        match self {
            ObservationDomain::Spherical | ObservationDomain::Hemispherical
            | ObservationDomain::Orbital => None, // bound set by the sampling mesh, not the domain
            ObservationDomain::SphericalShells { shells } => {
                Some(shells.iter().map(|(_, hi)| *hi as u64).sum())
            }
            ObservationDomain::NavigationGraph { nodes } => Some(*nodes as u64),
            ObservationDomain::AdaptiveSubdivision(policy) => Some(policy.max_observations as u64),
            ObservationDomain::Custom { max_observations, .. } => Some(*max_observations as u64),
        }
    }

    /// A custom domain without a registered Caprichio or without a bound is
    /// invalid (directive §6.2).
    pub fn is_valid(&self) -> bool {
        match self {
            ObservationDomain::Custom { caprichio, max_observations } => {
                !caprichio.is_empty() && *max_observations > 0
            }
            ObservationDomain::SphericalShells { shells } => {
                !shells.is_empty() && shells.iter().all(|(lo, hi)| lo <= hi)
            }
            ObservationDomain::NavigationGraph { nodes } => *nodes > 0,
            ObservationDomain::AdaptiveSubdivision(policy) => policy.is_valid(),
            ObservationDomain::Spherical | ObservationDomain::Hemispherical
            | ObservationDomain::Orbital => true,
        }
    }
}

/// Bound policy for adaptive subdivision domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubdivisionPolicy {
    /// Maximum subdivision depth.
    pub max_depth: u8,
    /// Maximum observations the subdivision may generate in total.
    pub max_observations: u32,
    /// Declared spatial tolerance within which intermediate observations
    /// are approximated rather than sampled (directive §4.4).
    pub tolerance: u32,
}

impl SubdivisionPolicy {
    pub fn is_valid(&self) -> bool {
        self.max_observations > 0 && self.tolerance > 0
    }
}

/// Spatial and environmental state coordinates of one observation
/// `O = F(G, C, L, M, S, P)` (directive §6.1).
///
/// Coordinates are abstract at this contract level: the Rugid spatial
/// contract (ADR-AVR-001 F1) defines their concrete representation.
/// Ordered so quantised states can key a `BTreeMap` primary index (§9.2).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObservationState {
    /// Camera position, as fixed-point spatial coordinates.
    pub camera: [i64; 3],
    /// Camera orientation, as fixed-point Euler components.
    pub orientation: [i64; 3],
    /// Zoom band index, where applicable.
    pub zoom: Option<u32>,
    /// Navigation-graph node, for environmental observation.
    pub node: Option<u32>,
    /// Material/appearance revision the observation was rendered against.
    pub material_revision: u64,
    /// Environmental state revision (lighting, simulation state).
    pub environment_revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_domain_requires_caprichio_and_bound() {
        let unbounded = ObservationDomain::Custom { caprichio: String::new(), max_observations: 0 };
        assert!(!unbounded.is_valid());
        assert_eq!(unbounded.observation_bound(), Some(0));

        let ok = ObservationDomain::Custom {
            caprichio: "spatial.exposure_sweep".into(),
            max_observations: 512,
        };
        assert!(ok.is_valid());
        assert_eq!(ok.observation_bound(), Some(512));
    }

    #[test]
    fn shell_bounds_are_validated_and_summed() {
        let shells = ObservationDomain::SphericalShells { shells: vec![(0, 1024), (1024, 3072)] };
        assert!(shells.is_valid());
        assert_eq!(shells.observation_bound(), Some(4096));

        let inverted = ObservationDomain::SphericalShells { shells: vec![(10, 5)] };
        assert!(!inverted.is_valid());
    }

    #[test]
    fn adaptive_subdivision_bound_enforced() {
        let policy = SubdivisionPolicy { max_depth: 6, max_observations: 4096, tolerance: 1 };
        let domain = ObservationDomain::AdaptiveSubdivision(policy);
        assert!(domain.is_valid());
        assert_eq!(domain.observation_bound(), Some(4096));
    }
}
