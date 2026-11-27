//! Adaptive presentation (directive §13): environment-dependent styling.
//!
//! Purpose: turn responsive CSS (`@media`, `@container`, interaction
//! capability queries) into typed records the host evaluates once the
//! environment or layout is known, plus the canonical breakpoint set a
//! stylesheet consults. Nothing here guesses at a runtime value: what the
//! compiler cannot judge is deferred, reported, and recorded for the host.
//!
//! Progressive scope, by submodule:
//! - [`env_registry`] classifies media features as evaluable now or
//!   deferred to the host, and evaluates the evaluable ones.
//! - [`media_queries`] evaluates `@media` conditions into typed records
//!   under a known [`Environment`], deferring host-only features.
//! - [`container_queries`] turns `@container` rules into deferred
//!   constraints handed to the host once container sizes are known.
//! - [`breakpoint`] collects the distinct widths a stylesheet consults
//!   into one ordered breakpoint ladder (breakpoint economy, §13.2).
//! - [`interaction_adaptation`] maps interaction pseudo-classes and
//!   hover/pointer capability conditions to typed affordance records.

pub mod breakpoint;
pub mod container_queries;
pub mod env_registry;
pub mod interaction_adaptation;
pub mod media_queries;
