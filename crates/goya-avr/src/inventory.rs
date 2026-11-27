//! Canonical observation inventory — logical data model (directive §13).
//!
//! This is the contract, not a binary layout. Serialisation into `OBSV`,
//! `ODOM`, … container sections is Phase 2 work gated on `.gvi` version
//! negotiation (directive §13.2).

use crate::derivation::DerivationProfileDigest;
use crate::domain::{ObservationDomain, ObservationState};
use crate::graph::{
    AnchorId, NavigationGraph, NeighbourhoodId, ObservationCoordinates, ObservationId,
    ReconstructionGraph, ResidencyGraph,
};

/// Identifier of a stored representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepresentationId(pub u32);

/// Identifier of a provenance record (generative or differential relation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProvenanceId(pub u32);

/// Fidelity classification of an observation or reconstruction (§15.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FidelityClass {
    /// Exhaustively verified or mathematically established equivalence.
    Exact,
    /// Reconstruction error established within a specified bound.
    Bounded,
    /// Fidelity assessed through sampled verification. MUST NOT be
    /// represented as mathematical proof of continuous-domain completeness.
    Empirical,
    /// Observation cannot be certified.
    Unsupported,
}

/// Fidelity and coverage record for one observation (§13 `FidelityRecord`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FidelityRecord {
    pub classification: FidelityClass,
    /// Error bounds; interpretation is policy-owned.
    pub error_bounds: [u32; 4],
    /// How the bounds were established.
    pub verification_method: &'static str,
    /// Spatial coverage the record applies to.
    pub coverage: ObservationCoordinates,
}

/// Generative or differential relationship between observations (§7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub id: ProvenanceId,
    /// The observation this provenance explains.
    pub observation: ObservationId,
    pub relation: ProvenanceRelation,
}

/// Kind of generative relation. Exact reconstruction MUST be distinguished
/// from approximate reconstruction (directive §12.3); `Albit` carries an
/// explicit exactness bit that Inquisition verifies (AVR-C09) — an
/// operation is never certified exact because its output looks similar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceRelation {
    /// Derived from `source` by exact transformation.
    ExactTransform { source: ObservationId },
    /// Derived from `source` approximately, within the record's bounds.
    ApproximateTransform { source: ObservationId },
    /// Reconstructed generatively (Albit); `exact` must be certified.
    Albit { source: ObservationId, exact: bool },
    /// Combined from several stored components.
    Composite { sources: Vec<ObservationId> },
}

/// One canonical observation (§13 `ObservationRecord`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationRecord {
    pub id: ObservationId,
    pub state: ObservationState,
    pub representation: RepresentationId,
    pub provenance: Option<ProvenanceId>,
    pub fidelity: FidelityRecord,
}

/// A primary anchor with its bounded neighbourhood (§13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationalAnchor {
    pub id: AnchorId,
    pub observation: ObservationId,
    pub neighbourhood: NeighbourhoodId,
    pub adjacent: Vec<AnchorId>,
    pub bounds: ObservationCoordinates,
}

/// Bounded local neighbourhood around one anchor (§13 `Neighbourhood`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbourhood {
    pub id: NeighbourhoodId,
    pub anchor: AnchorId,
    pub observations: Vec<ObservationId>,
    /// Resource bounds the runtime enforces (AVR-C07); an oversized
    /// neighbourhood is a validation failure, not a soft warning.
    pub max_observations: u32,
}

/// Canonical observation inventory (§13 `ObservationInventory`).
///
/// Invariants enforced at construction:
/// - the observation domain is valid (AVR-C01);
/// - every neighbourhood stays within its declared bound (AVR-C07);
/// - the reconstruction graph is acyclic and depth-bounded (AVR-C04/C05);
/// - provenance references resolve (referential integrity, §17).
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationInventory {
    pub canonical_digest: String,
    pub domain: ObservationDomain,
    pub observations: Vec<ObservationRecord>,
    pub navigation: NavigationGraph,
    pub reconstruction: ReconstructionGraph,
    pub residency: ResidencyGraph,
    pub anchors: Vec<RelationalAnchor>,
    pub neighbourhoods: Vec<Neighbourhood>,
    pub provenance: Vec<ProvenanceRecord>,
    /// Digest of the derivation profiles this canonical inventory is
    /// intended to serve; target derivation validates provenance (AVR-C11).
    pub derivation_contract: Option<DerivationProfileDigest>,
}

/// Why an inventory was rejected at construction or load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    /// The declared observation domain is invalid (AVR-C01).
    InvalidDomain,
    /// A neighbourhood exceeds its declared resource bound (AVR-C07).
    NeighbourhoodOversized { neighbourhood: NeighbourhoodId },
    /// A record references an unknown observation (§17 referential integrity).
    DanglingReference { from: String },
    /// The reconstruction graph failed topology verification.
    Topology(crate::graph::GraphTopologyError),
    /// A provenance record claims exactness without a verifying method
    /// (AVR-C09): "looks correct" is never a certificate.
    UncertifiedExactReconstruction { observation: ObservationId },
}

impl ObservationInventory {
    /// Construct and validate an inventory. Structural validation happens
    /// here and again at load: inventories are untrusted input (§17).
    // The inventory has ten logical sections (§13); threading them through
    // a parameter struct would only rename the list.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canonical_digest: String,
        domain: ObservationDomain,
        observations: Vec<ObservationRecord>,
        navigation: NavigationGraph,
        reconstruction_nodes: Vec<crate::graph::ReconstructionNode>,
        residency: ResidencyGraph,
        anchors: Vec<RelationalAnchor>,
        neighbourhoods: Vec<Neighbourhood>,
        provenance: Vec<ProvenanceRecord>,
        max_depth: u8,
    ) -> Result<Self, InventoryError> {
        if !domain.is_valid() {
            return Err(InventoryError::InvalidDomain);
        }

        let known: std::collections::HashSet<u64> =
            observations.iter().map(|o| o.id.0).collect();

        // Every stored observation may serve as an independently
        // reconstructable leaf dependency.
        let terminals: Vec<ObservationId> = observations.iter().map(|o| o.id).collect();
        let reconstruction = ReconstructionGraph::new(reconstruction_nodes, terminals, max_depth)
            .map_err(InventoryError::Topology)?;

        for neighbourhood in &neighbourhoods {
            if neighbourhood.observations.len() as u32 > neighbourhood.max_observations {
                return Err(InventoryError::NeighbourhoodOversized {
                    neighbourhood: neighbourhood.id,
                });
            }
        }

        for record in &provenance {
            if let ProvenanceRelation::Albit { source, exact: true } = record.relation {
                let source_record = observations.iter().find(|o| o.id == source);
                let certified = source_record.is_some_and(|o| {
                    o.fidelity.classification == FidelityClass::Exact
                });
                if !certified {
                    return Err(InventoryError::UncertifiedExactReconstruction {
                        observation: source,
                    });
                }
            }
        }

        for anchor in &anchors {
            if !known.contains(&anchor.observation.0) {
                return Err(InventoryError::DanglingReference {
                    from: format!("anchor {} observation", anchor.id.0),
                });
            }
        }

        Ok(Self {
            canonical_digest,
            domain,
            observations,
            navigation,
            reconstruction,
            residency,
            anchors,
            neighbourhoods,
            provenance,
            derivation_contract: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{ObservationId, ReconstructionNode};

    fn record(id: u64, class: FidelityClass) -> ObservationRecord {
        ObservationRecord {
            id: ObservationId(id),
            state: ObservationState {
                camera: [0, 0, 0],
                orientation: [0, 0, 0],
                zoom: None,
                node: None,
                material_revision: 0,
                environment_revision: 0,
            },
            representation: RepresentationId(0),
            provenance: None,
            fidelity: FidelityRecord {
                classification: class,
                error_bounds: [0; 4],
                verification_method: "test",
                coverage: ObservationCoordinates { spatial: [0, 0, 0] },
            },
        }
    }

    fn domain() -> ObservationDomain {
        ObservationDomain::NavigationGraph { nodes: 4 }
    }

    #[test]
    fn valid_inventory_constructs() {
        let inventory = ObservationInventory::new(
            "sha256:abc".into(),
            domain(),
            vec![record(1, FidelityClass::Exact), record(2, FidelityClass::Bounded)],
            NavigationGraph::default(),
            vec![ReconstructionNode {
                observation: ObservationId(2),
                dependencies: vec![ObservationId(1)],
            }],
            ResidencyGraph::default(),
            vec![],
            vec![Neighbourhood {
                id: NeighbourhoodId(0),
                anchor: AnchorId(0),
                observations: vec![ObservationId(1), ObservationId(2)],
                max_observations: 4,
            }],
            vec![],
            crate::graph::MAX_DEPENDENCY_DEPTH,
        )
        .expect("well-formed inventory");
        assert_eq!(inventory.observations.len(), 2);
    }

    #[test]
    fn oversized_neighbourhood_is_rejected() {
        let err = ObservationInventory::new(
            "sha256:abc".into(),
            domain(),
            vec![record(1, FidelityClass::Exact)],
            NavigationGraph::default(),
            vec![],
            ResidencyGraph::default(),
            vec![],
            vec![Neighbourhood {
                id: NeighbourhoodId(0),
                anchor: AnchorId(0),
                observations: vec![ObservationId(1), ObservationId(2), ObservationId(3)],
                max_observations: 2,
            }],
            vec![],
            crate::graph::MAX_DEPENDENCY_DEPTH,
        )
        .expect_err("bound must be enforced");
        assert_eq!(
            err,
            InventoryError::NeighbourhoodOversized { neighbourhood: NeighbourhoodId(0) }
        );
    }

    #[test]
    fn uncertified_exact_albit_is_rejected() {
        let err = ObservationInventory::new(
            "sha256:abc".into(),
            domain(),
            vec![record(1, FidelityClass::Empirical), record(2, FidelityClass::Bounded)],
            NavigationGraph::default(),
            vec![],
            ResidencyGraph::default(),
            vec![],
            vec![],
            vec![ProvenanceRecord {
                id: ProvenanceId(0),
                observation: ObservationId(2),
                relation: ProvenanceRelation::Albit { source: ObservationId(1), exact: true },
            }],
            crate::graph::MAX_DEPENDENCY_DEPTH,
        )
        .expect_err("exactness must be certified, not asserted");
        assert_eq!(
            err,
            InventoryError::UncertifiedExactReconstruction { observation: ObservationId(1) }
        );
    }

    #[test]
    fn invalid_domain_is_rejected() {
        let err = ObservationInventory::new(
            "sha256:abc".into(),
            ObservationDomain::Custom { caprichio: String::new(), max_observations: 0 },
            vec![],
            NavigationGraph::default(),
            vec![],
            ResidencyGraph::default(),
            vec![],
            vec![],
            vec![],
            crate::graph::MAX_DEPENDENCY_DEPTH,
        )
        .expect_err("unbounded custom domain must be rejected");
        assert_eq!(err, InventoryError::InvalidDomain);
    }
}
