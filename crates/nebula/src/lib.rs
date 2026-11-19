//! Nebula: the COSMIC desktop profile and qualification lane for Emperor.
//!
//! Nebula specialises Emperor. It does not fork lowering, codegen or the
//! scene language. Only COSMIC-specific behaviour lives here:
//!
//! - [`session`] decides whether the process runs inside a COSMIC Wayland
//!   session and identifies the `cosmic-comp` build.
//! - [`protocols`] lists Wayland globals and classifies them against the
//!   COSMIC application profile. Classification is a pure function.
//! - [`theme`] reads the COSMIC theme RON files (format `v1` only) and maps
//!   them onto Goya Chroma, with COSMIC-only geometry kept apart.
//! - [`caprichios`] selects the few draft Caprichio catalog entries whose
//!   relevance to a desktop host is clear.
//! - [`probe`] assembles the `nebula.probe/0.1` record.
//!
//! Protocol advertisement is not behaviour (GY-EMP-001 §4.3). Nothing here
//! opens a window, and nothing here is a release qualification.

pub mod caprichios;
pub mod probe;
pub mod protocols;
pub mod session;
pub mod theme;
pub mod time;

/// Schema identifier of the probe record.
pub const PROBE_SCHEMA: &str = "nebula.probe/0.1";

/// Schema identifier of the `nebula theme --json` record.
pub const THEME_SCHEMA: &str = "nebula.theme/0.1";

/// Crate version, recorded in every report.
pub const NEBULA_VERSION: &str = env!("CARGO_PKG_VERSION");
