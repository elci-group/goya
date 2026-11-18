//! Nevada — Goya's Android TV compiler target.
//!
//! Nevada translates Goya application intent into television-native,
//! remote-first interaction models. This crate is phase 0: the
//! deterministic core subsystems (capability resolution, interaction
//! translation, spatial focus) as pure, std-only library modules with
//! fail-closed semantics. It produces no artefacts yet and admits no
//! scenes; see `docs/nevada/roadmap.md`.

pub mod conformance;
pub mod diagnostics;
pub mod dcr;
pub mod focus;
pub mod profiles;
pub mod orchestrate;
pub mod target;
pub mod translate;

/// Nevada crate contract identifier.
pub const CONTRACT: &str = "nevada.androidtv/0.0.2";
