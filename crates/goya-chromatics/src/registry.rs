//! Binding Registry and Interaction Dispatcher (§4, §11).
//!
//! Dispatch never ranks. It reads the committed generation only. A new
//! generation is *staged* until the presentation reports it on screen; then
//! it is *committed*. While a generation is staged, a slot whose binding
//! would change is suppressed. A press keeps the identity it had when the
//! key went down: on release it executes that original intent if it is
//! still valid, or it is rejected. It is never redirected.

use std::collections::{BTreeMap, BTreeSet};

use crate::generation::Generation;
use crate::model::SlotId;

/// Execution-time permission check (§20). Allocation confers nothing.
pub trait Authority {
    fn permits(&self, intent: &str, entity: Option<&str>) -> bool;
}

impl<F: Fn(&str, Option<&str>) -> bool> Authority for F {
    fn permits(&self, intent: &str, entity: Option<&str>) -> bool {
        self(intent, entity)
    }
}

/// A key that went down, bound to what it meant at that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Press {
    pub seq: u64,
    pub slot: SlotId,
    /// Committed generation at press time, if any.
    pub generation: Option<u64>,
    pub context: Option<String>,
    /// `(candidate, intent, entity, confirmation)` bound at press time.
    pub target: Option<(String, String, Option<String>, bool)>,
    /// The slot was changing between committed and staged generations.
    pub ambiguous: bool,
    /// The committed generation's scope was not active at press time.
    pub inactive: bool,
}

/// Outcome of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    /// Run this intent. `confirmation` means it opens a confirmation
    /// workflow, which the handler must not bypass.
    Execute {
        candidate: String,
        intent: String,
        entity: Option<String>,
        confirmation: bool,
    },
    /// Nothing was bound: the target may leave the key to the platform.
    Unassigned,
    Rejected(Refusal),
}

/// Why a release did not execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The slot was being reassigned when the key went down.
    Transition,
    /// The binding was revoked after the press.
    Revoked,
    /// The press belongs to a scope that is no longer active.
    ScopeInactive,
    /// The execution-time permission check failed.
    NotAuthorised,
    /// No press is held on this slot.
    NotHeld,
}

impl Refusal {
    pub fn code(self) -> &'static str {
        match self {
            Refusal::Transition => "transition",
            Refusal::Revoked => "revoked",
            Refusal::ScopeInactive => "scope-inactive",
            Refusal::NotAuthorised => "not-authorised",
            Refusal::NotHeld => "not-held",
        }
    }
}

/// Authoritative published binding state for one application instance.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    committed: Option<Generation>,
    staged: Option<Generation>,
    active: Option<String>,
    revoked: BTreeSet<String>,
    held: BTreeMap<SlotId, Press>,
    next_seq: u64,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn committed(&self) -> Option<&Generation> {
        self.committed.as_ref()
    }

    pub fn staged(&self) -> Option<&Generation> {
        self.staged.as_ref()
    }

    /// The generation the presentation should draw now.
    pub fn to_present(&self) -> Option<&Generation> {
        self.staged.as_ref().or(self.committed.as_ref())
    }

    /// Make `context` the only scope whose bindings may execute. Presses
    /// from any other scope are refused on release.
    pub fn activate(&mut self, context: &str) {
        self.active = Some(context.to_string());
    }

    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// Stage a verified generation. It is not dispatchable until
    /// [`Registry::presented`] reports it on screen.
    pub fn stage(&mut self, generation: Generation) {
        self.staged = Some(generation);
    }

    /// The presentation drew generation `id`. Returns whether it committed.
    pub fn presented(&mut self, id: u64) -> bool {
        if self.staged.as_ref().is_some_and(|g| g.id == id) {
            self.committed = self.staged.take();
            true
        } else {
            false
        }
    }

    /// Stage and commit at once, for targets whose presentation and input
    /// publication are synchronous.
    pub fn publish(&mut self, generation: Generation) {
        self.staged = None;
        self.committed = Some(generation);
    }

    /// Immediate invalidation: a revoked candidate cannot execute, whatever
    /// any generation says, until reinstated (§10.5).
    pub fn revoke(&mut self, candidate: &str) {
        self.revoked.insert(candidate.to_string());
    }

    pub fn reinstate(&mut self, candidate: &str) {
        self.revoked.remove(candidate);
    }

    /// Suspension or focus loss: forget held keys; their releases are
    /// refused as `NotHeld`.
    pub fn cancel_presses(&mut self) {
        self.held.clear();
    }

    /// A key went down. Returns `None` for an auto-repeat of a held key.
    pub fn press(&mut self, slot: SlotId) -> Option<Press> {
        if self.held.contains_key(&slot) {
            return None;
        }
        self.next_seq += 1;
        let current = self.committed.as_ref().and_then(|g| g.binding(slot));
        let ambiguous = match &self.staged {
            Some(next) => {
                let a = current.map(|b| (&b.candidate, &b.intent));
                let b = next.binding(slot).map(|b| (&b.candidate, &b.intent));
                a != b
            }
            None => false,
        };
        let press = Press {
            seq: self.next_seq,
            slot,
            generation: self.committed.as_ref().map(|g| g.id),
            context: self.committed.as_ref().map(|g| g.context.clone()),
            target: current.map(|b| {
                (
                    b.candidate.clone(),
                    b.intent.clone(),
                    b.entity.clone(),
                    b.confirmation,
                )
            }),
            ambiguous,
            inactive: self.committed.as_ref().map(|g| g.context.as_str()) != self.active.as_deref(),
        };
        self.held.insert(slot, press.clone());
        Some(press)
    }

    /// The key came up: execute at most once.
    pub fn release(&mut self, slot: SlotId, authority: &dyn Authority) -> Dispatch {
        let Some(press) = self.held.remove(&slot) else {
            return Dispatch::Rejected(Refusal::NotHeld);
        };
        if press.ambiguous {
            return Dispatch::Rejected(Refusal::Transition);
        }
        let Some((candidate, intent, entity, confirmation)) = press.target else {
            return Dispatch::Unassigned;
        };
        if press.inactive || press.context.as_deref() != self.active.as_deref() {
            return Dispatch::Rejected(Refusal::ScopeInactive);
        }
        if self.revoked.contains(&candidate) {
            return Dispatch::Rejected(Refusal::Revoked);
        }
        if !authority.permits(&intent, entity.as_deref()) {
            return Dispatch::Rejected(Refusal::NotAuthorised);
        }
        Dispatch::Execute {
            candidate,
            intent,
            entity,
            confirmation,
        }
    }
}
