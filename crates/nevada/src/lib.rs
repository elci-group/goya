//! Nevada — Goya's Android TV compiler target.
//!
//! Nevada translates Goya application intent into television-native,
//! remote-first interaction models. This crate owns the dependency-free
//! capability model, interaction translation, spatial focus and blocking
//! static gate. The SDK admits GSSH/3 scenes and consumes the gate receipt
//! to emit the Android TV project.

pub mod caprichio;
pub mod conformance;
pub mod dcr;
pub mod diagnostics;
pub mod focus;
pub mod gate;
pub mod orchestrate;
pub mod profiles;
pub mod runtime;
pub mod target;
pub mod translate;

/// Nevada crate contract identifier.
pub const CONTRACT: &str = "nevada.androidtv/0.0.2";
