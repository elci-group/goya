//! Cortados Phase 1: static Java evidence extraction, without native conversion.
pub mod analysis;
pub mod bundle;
pub mod ingest;
pub mod java;
mod model;
pub use model::{Diagnostic, Limits, Span, PROFILE};
