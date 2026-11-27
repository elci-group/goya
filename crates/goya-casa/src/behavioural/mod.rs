//! Behavioural state (directive §11–12): the interaction dimension of a
//! stylesheet.
//!
//! Purpose: extract which interaction states elements style and carry,
//! measure the combinatorial state space they imply, cap it at a budget
//! with deterministic collapse, and parse transitions into the typed
//! values the host's motion system consumes. State vocabulary is shared
//! with the adaptive family; timing vocabulary is shared with the
//! animation family's [`Easing`](crate::animation::Easing).
//!
//! Progressive scope, by submodule:
//! - [`state_extraction`] walks IR selectors and the scene tree for
//!   interaction states per element.
//! - [`state_space`] assembles the per-element state sets and counts the
//!   combinations they imply (§11.1).
//! - [`explosion_prevention`] enforces the §11.2 budget, collapsing the
//!   space deterministically and reporting every collapse.
//! - [`transitions`] parses `transition` shorthand and longhands into
//!   typed [`Transition`] values with CSS defaults.

pub mod explosion_prevention;
pub mod state_extraction;
pub mod state_space;
pub mod transitions;
