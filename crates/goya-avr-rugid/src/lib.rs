//! goya-avr-rugid — AVR bridge to the Rugid spatial engine.
//!
//! Implements the [`goya_avr::SpatialSource`] ingestion contract
//! (ADR-AVR-001 F1) over Rugid's headless vector render path. The bridge
//! is feature-gated on `rugid` (an external path dependency at
//! `~/RUGID`) so the rest of the Goya workspace never compiles it.
//!
//! Camera-model decision (Phase 1): the AVR parametrised observer maps
//! onto Rugid's fixed camera through inverse model rotation — observing
//! from azimuth/elevation is rendered as the model rotated by those
//! angles. Zoom is distance-driven: Rugid's render path has a fixed focal
//! length, so the bridge converts observer distance to an equivalent
//! model size. Environment cameras and viewports (§5.3) are Phase 6.

#![forbid(unsafe_code)]

#[cfg(feature = "rugid")]
pub mod adapter;

#[cfg(feature = "rugid")]
pub use adapter::{default_scene, RugidEnvironmentRoom, RugidNavigationSource, RugidShapeSource};

/// Bridge identity recorded in canonical provenance.
pub const BRIDGE_ID: &str = "goya-avr-rugid 0.0.1";
