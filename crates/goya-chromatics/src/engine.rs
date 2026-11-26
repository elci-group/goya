//! The engine: capability, registered intents, per-context history and
//! verified publication.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::allocate::{allocate, Allocation, Request};
use crate::generation::{verify, Generation, Violation};
use crate::model::{Capability, MAX_SLOTS};

/// A malformed capability table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    TooManySlots(usize),
    DuplicateSlot(String),
    EmptySlotName,
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CapabilityError {}

/// Verification refused a generation; the previous one stands (§25).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused(pub Vec<Violation>);

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "generation refused: {:?}", self.0)
    }
}

impl std::error::Error for Refused {}

fn check(capability: &Capability) -> Result<(), CapabilityError> {
    if capability.slots.len() > MAX_SLOTS {
        return Err(CapabilityError::TooManySlots(capability.slots.len()));
    }
    let mut names = BTreeSet::new();
    for s in &capability.slots {
        if s.name.is_empty() {
            return Err(CapabilityError::EmptySlotName);
        }
        if !names.insert(s.name.as_str()) {
            return Err(CapabilityError::DuplicateSlot(s.name.clone()));
        }
    }
    Ok(())
}

/// Allocation state shared by every scope of one application instance.
#[derive(Debug, Clone)]
pub struct Engine {
    capability: Capability,
    intents: BTreeSet<String>,
    next_id: u64,
    history: BTreeMap<String, Generation>,
}

impl Engine {
    /// `intents` are the registered semantic intent handlers. A candidate
    /// whose intent is not registered is never bound (§20).
    pub fn new<I, S>(capability: Capability, intents: I) -> Result<Self, CapabilityError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        check(&capability)?;
        Ok(Engine {
            capability,
            intents: intents.into_iter().map(Into::into).collect(),
            next_id: 1,
            history: BTreeMap::new(),
        })
    }

    pub fn capability(&self) -> &Capability {
        &self.capability
    }

    pub fn intents(&self) -> &BTreeSet<String> {
        &self.intents
    }

    /// Replace the capability. Callers then re-resolve with
    /// [`crate::Trigger::Capability`].
    pub fn set_capability(&mut self, capability: Capability) -> Result<(), CapabilityError> {
        check(&capability)?;
        self.capability = capability;
        Ok(())
    }

    /// The last published generation of a context.
    pub fn previous(&self, context: &str) -> Option<&Generation> {
        self.history.get(context)
    }

    /// Drop history for every context under `prefix` (profile reset).
    pub fn forget(&mut self, prefix: &str) {
        self.history.retain(|k, _| !k.starts_with(prefix));
    }

    /// Allocate, verify, and record the generation for its context. On a
    /// violation nothing is recorded and the previous generation stands.
    pub fn allocate(&mut self, req: &Request<'_>) -> Result<Allocation, Refused> {
        let allocation = allocate(
            &self.capability,
            &self.intents,
            req,
            self.history.get(req.context),
            self.next_id,
        );
        verify(
            &allocation.generation,
            req.context,
            &self.capability,
            req.policy,
            req.candidates,
            &self.intents,
        )
        .map_err(Refused)?;
        self.next_id += 1;
        self.history
            .insert(req.context.to_string(), allocation.generation.clone());
        Ok(allocation)
    }
}
