//! Binding generations (§11) and the Verification Layer (§25).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::eligibility::{validity, Rejection};
use crate::model::{Candidate, Capability, Policy, Risk, SlotId, SlotStatus};

/// Why a candidate was selected (§27 provenance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// Explicit manual assignment.
    Pinned,
    /// Supplied by the primary strategy at this place (0 first).
    Primary(u32),
    /// Supplied by the fallback strategy at this place in the whole order.
    Fallback(u32),
    /// Kept by Chromatic Inertia although a better challenger exists.
    Retained,
}

/// Why a binding sits on its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Pinned,
    /// Same slot as in the previous generation of this context.
    Preserved,
    /// Next free slot in slot order.
    Free,
}

/// One executable assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub slot: SlotId,
    pub slot_name: String,
    pub candidate: String,
    pub entity: Option<String>,
    pub intent: String,
    pub label: String,
    /// Non-colour identification carried from the candidate (§21).
    pub symbol: Option<String>,
    /// Accessible description carried from the candidate (§21).
    pub description: Option<String>,
    pub risk: Risk,
    pub confirmation: bool,
    pub selected: Selection,
    pub placed: Placement,
    /// Tick from which this candidate has held this slot.
    pub since: u64,
    /// Tick from which a better challenger has been held back, if any.
    pub contested_since: Option<u64>,
}

/// An immutable, published allocation. Presentation and dispatch consume
/// the same generation; nothing mutates it after publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    pub id: u64,
    pub context: String,
    /// Bindings in slot order; each slot appears at most once.
    pub bindings: Vec<Binding>,
    pub now: u64,
    /// Tick from which reallocation has been deferred by interaction.
    pub deferred_since: Option<u64>,
}

impl Generation {
    pub fn binding(&self, slot: SlotId) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.slot == slot)
    }

    pub fn by_candidate(&self, id: &str) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.candidate == id)
    }

    /// `slot name -> candidate id` for display and fixtures.
    pub fn assignment(&self) -> Vec<(String, String)> {
        self.bindings
            .iter()
            .map(|b| (b.slot_name.clone(), b.candidate.clone()))
            .collect()
    }
}

/// An invariant a generation breaks (§25). Any violation blocks publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    SlotOutOfRange(SlotId),
    SlotNotAvailable(String),
    SlotNameMismatch(SlotId),
    SlotReused(String),
    OverCapacity { bound: usize, capacity: usize },
    UnknownCandidate(String),
    DuplicateCandidate(String),
    DuplicateEntity(String),
    BindingMismatch(String),
    Invalid(String, Rejection),
    NativeEquivalent(String),
    /// A binding with no text label (§21: colour never identifies a
    /// binding, so the accessible floor is mandatory at publication).
    Unlabelled(String),
    ContextMismatch { expected: String, found: String },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Independent re-check of a generation against its inputs. It does not
/// trust the allocator.
pub fn verify(
    generation: &Generation,
    context: &str,
    capability: &Capability,
    policy: &Policy,
    candidates: &[Candidate],
    intents: &BTreeSet<String>,
) -> Result<(), Vec<Violation>> {
    let mut v = Vec::new();
    if generation.context != context {
        v.push(Violation::ContextMismatch {
            expected: context.to_string(),
            found: generation.context.clone(),
        });
    }
    let mut by_id: BTreeMap<&str, Vec<&Candidate>> = BTreeMap::new();
    for c in candidates {
        by_id.entry(c.id.as_str()).or_default().push(c);
    }
    let capacity = capability.capacity();
    if generation.bindings.len() > capacity {
        v.push(Violation::OverCapacity {
            bound: generation.bindings.len(),
            capacity,
        });
    }
    let mut slots = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut entities = BTreeSet::new();
    for b in &generation.bindings {
        match capability.slots.get(usize::from(b.slot)) {
            None => v.push(Violation::SlotOutOfRange(b.slot)),
            Some(s) => {
                if s.name != b.slot_name {
                    v.push(Violation::SlotNameMismatch(b.slot));
                }
                if s.status != SlotStatus::Available {
                    v.push(Violation::SlotNotAvailable(s.name.clone()));
                }
            }
        }
        if !slots.insert(b.slot) {
            v.push(Violation::SlotReused(b.slot_name.clone()));
        }
        if !ids.insert(b.candidate.as_str()) {
            v.push(Violation::DuplicateCandidate(b.candidate.clone()));
        }
        if policy.exclude_duplicates {
            if let Some(e) = &b.entity {
                if !entities.insert(e.as_str()) {
                    v.push(Violation::DuplicateEntity(e.clone()));
                }
            }
        }
        let c = match by_id.get(b.candidate.as_str()).map(Vec::as_slice) {
            None | Some([]) => {
                v.push(Violation::UnknownCandidate(b.candidate.clone()));
                continue;
            }
            Some([c]) => *c,
            Some(_) => {
                v.push(Violation::Invalid(
                    b.candidate.clone(),
                    Rejection::DuplicateId,
                ));
                continue;
            }
        };
        if c.intent != b.intent
            || c.entity != b.entity
            || c.risk != b.risk
            || c.label != b.label
            || c.symbol != b.symbol
            || c.description != b.description
        {
            v.push(Violation::BindingMismatch(b.candidate.clone()));
        }
        if b.label.trim().is_empty() {
            v.push(Violation::Unlabelled(b.candidate.clone()));
        }
        if let Some(r) = validity(c, intents, generation.now) {
            v.push(Violation::Invalid(b.candidate.clone(), r));
        }
        if let Some(k) = &c.native {
            if policy.exclude_native && capability.native.contains(k) {
                v.push(Violation::NativeEquivalent(b.candidate.clone()));
            }
        }
    }
    if v.is_empty() {
        Ok(())
    } else {
        Err(v)
    }
}
