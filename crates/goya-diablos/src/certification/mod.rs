//! Certification and acceptance gates for Diablos.

pub mod inquisition;
pub mod provenance;
pub mod gates;

pub use inquisition::{InquisitionReport, verify_inquisition};
pub use provenance::{ArtefactProvenance, create_provenance};
pub use gates::{AcceptanceGate, GateStatus, evaluate_gates, dia_gates};