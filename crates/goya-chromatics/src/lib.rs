//! GY-CHROMATICS/0.1 (draft) — adaptive semantic input allocation.
//!
//! Chromatics assigns scarce physical inputs (on webOS, the four colour
//! keys) to the semantic actions most worth one press in the active scope.
//! This crate is the platform-independent core of directive
//! GOYA-INT-CHR-001 (Phase 1 and the logic of Phase 2): it has no target
//! code, no language syntax, no clock, no I/O and no dependencies.
//!
//! * [`model`]: slots, capability, candidates, strategies, policy.
//! * [`Engine::allocate`]: eligibility → ranking → Chromatic Inertia →
//!   placement → verification. Pure and deterministic: equal inputs give
//!   equal generations, whatever the candidate order.
//! * [`Registry`]: staged/committed generations and dispatch. A press keeps
//!   its original meaning; it executes that or is refused, never redirected.
//! * [`Scopes`]: nested scopes; only the innermost one can execute. A scope
//!   may declare that it inherits its parent's candidate pool
//!   ([`compose`], §6).
//! * [`fixture`]: the shared conformance script format.
//!
//! What this crate does not do is in `docs/chromatics/CHROMATICS-0.md`;
//! the `.gscene` authoring surface is `CHROMATICS-1.md`
//! (`goya-lang::chromatics`).

pub mod allocate;
pub mod eligibility;
pub mod engine;
pub mod fixture;
pub mod generation;
pub mod model;
pub mod registry;
pub mod scope;

pub use allocate::{Allocation, Note, Request};
pub use eligibility::Rejection;
pub use engine::{CapabilityError, Engine, Refused};
pub use generation::{verify, Binding, Generation, Placement, Selection, Violation};
pub use model::{
    Candidate, Capability, Policy, Risk, Slot, SlotId, SlotStatus, Stability, Strategy, Trigger,
    MAX_SLOTS,
};
pub use registry::{Authority, Dispatch, Press, Refusal, Registry};
pub use scope::{compose, Inheritance, ScopeError, ScopeKind, Scopes};

/// Contract implemented by this crate. Draft: not ratified.
pub const CONTRACT: &str = "GY-CHROMATICS/0.1";
