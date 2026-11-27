//! goya-avr — Anticipatory Vector Rendering (AVR) and Relational Graphical
//! Variation Infrastructure (GVI-R) contracts, directive GOYA-GFX-AVR-001.
//!
//! Phase 0 scope (ADR-AVR-001): the **logical contract** — observation
//! domains, representation classes, encoding classes, graph separation,
//! the canonical inventory data model, derivation profiles and Inquisition
//! verification classes. No binary encoding lives here yet: §13.1 sections
//! are gated on Phase 2 and the `.gvi` format version negotiation of
//! GOYA-GFX-AVR-001 §13.2.
//!
//! Architectural principle (directive §1): graphical computation is
//! performed at the earliest economically advantageous stage; a canonical
//! observation inventory is compiled once and derived per target. This
//! crate owns the contract that canonical compilers, target derivations,
//! runtimes and Inquisition share; it must stay free of rendering and
//! target dependencies.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod certification;
pub mod derivation;
pub mod domain;
pub mod encoding;
pub mod graph;
pub mod ingestion;
pub mod inventory;
pub mod relational;
pub mod representation;
pub mod retrieval;
pub mod sampling;
pub mod tags;
pub mod target;

/// Directive that defined this contract family.
pub const DIRECTIVE_ID: &str = "GOYA-GFX-AVR-001";

/// Logical contract version. Not a binary format version.
pub const CONTRACT_VERSION: &str = "avr-contract/0.0.1";

pub use certification::{CertificationClass, VerificationClass};
pub use derivation::{DerivationProfile, IndexPolicy, ReconstructionPolicy, ResidencyPolicy};
pub use domain::{ObservationDomain, ObservationState, SubdivisionPolicy};
pub use encoding::EncodingClass;
pub use graph::{
    AnchorId, GraphTopologyError, NeighbourhoodId, NavigationGraph, ObservationId,
    ObservationCoordinates, ReconstructionGraph, ReconstructionNode, ResidencyGraph,
    MAX_DEPENDENCY_DEPTH,
};
pub use ingestion::{Observer, ObservationPayload, SourceError, SpatialSource};
pub use inventory::{
    FidelityClass, FidelityRecord, Neighbourhood, ObservationInventory, ObservationRecord,
    ProvenanceId, ProvenanceRecord, ProvenanceRelation, RelationalAnchor, RepresentationId,
};
pub use representation::RepresentationClass;
pub use relational::{RelationalLayout, LayoutError};
pub use target::{Derivation, DerivationError, DerivationMetrics, Tier};
pub use tags::{ProposedTag, EXISTING_GVI_TAGS};
