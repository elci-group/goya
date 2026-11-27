//! Eligibility Evaluator (§4): availability, permission, safety, native
//! redundancy and malformed input. Every refusal carries a reason (§27).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::model::{Candidate, Capability, Policy};

/// Why a candidate is not bound.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rejection {
    Unavailable,
    Unauthorised,
    Expired,
    /// A dedicated native input covers this function.
    NativeEquivalent(String),
    /// Sensitive, destructive or privileged without a confirmation workflow.
    UnsafeWithoutConfirmation,
    /// The intent has no registered handler.
    UnregisteredIntent,
    /// No text label (§21).
    EmptyLabel,
    /// Two or more candidates share this id; all are refused.
    DuplicateId,
    /// Another binding already exposes this entity.
    DuplicateEntity,
    /// The pinned slot does not exist or is not available.
    PinnedSlotUnavailable,
    /// Another candidate (named) holds the pinned slot.
    PinConflict(String),
    /// Neither the primary nor the fallback strategy supplies it.
    NotSupplied,
    /// Eligible, but every slot went to a better candidate.
    OverCapacity,
}

impl Rejection {
    pub fn code(&self) -> &'static str {
        match self {
            Rejection::Unavailable => "unavailable",
            Rejection::Unauthorised => "unauthorised",
            Rejection::Expired => "expired",
            Rejection::NativeEquivalent(_) => "native-equivalent",
            Rejection::UnsafeWithoutConfirmation => "unsafe-without-confirmation",
            Rejection::UnregisteredIntent => "unregistered-intent",
            Rejection::EmptyLabel => "empty-label",
            Rejection::DuplicateId => "duplicate-id",
            Rejection::DuplicateEntity => "duplicate-entity",
            Rejection::PinnedSlotUnavailable => "pinned-slot-unavailable",
            Rejection::PinConflict(_) => "pin-conflict",
            Rejection::NotSupplied => "not-supplied",
            Rejection::OverCapacity => "over-capacity",
        }
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rejection::NativeEquivalent(k) => write!(f, "native-equivalent({k})"),
            Rejection::PinConflict(by) => write!(f, "pin-conflict({by})"),
            other => f.write_str(other.code()),
        }
    }
}

/// Reasons that make a candidate unfit to execute at all, independent of
/// ranking. Shared by allocation and verification.
pub fn validity(c: &Candidate, intents: &BTreeSet<String>, now: u64) -> Option<Rejection> {
    if !c.available {
        return Some(Rejection::Unavailable);
    }
    if !c.authorised {
        return Some(Rejection::Unauthorised);
    }
    if c.expires.is_some_and(|t| t <= now) {
        return Some(Rejection::Expired);
    }
    if c.risk.needs_confirmation() && !c.confirmation {
        return Some(Rejection::UnsafeWithoutConfirmation);
    }
    if !intents.contains(&c.intent) {
        return Some(Rejection::UnregisteredIntent);
    }
    if c.label.trim().is_empty() {
        return Some(Rejection::EmptyLabel);
    }
    None
}

/// Split candidates into eligible ones (sorted by id) and refusals.
pub(crate) fn evaluate<'a>(
    candidates: &'a [Candidate],
    capability: &Capability,
    policy: &Policy,
    intents: &BTreeSet<String>,
    now: u64,
) -> (Vec<&'a Candidate>, Vec<(String, Rejection)>) {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for c in candidates {
        *count.entry(c.id.as_str()).or_default() += 1;
    }
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();
    for c in candidates {
        let reason = if count[c.id.as_str()] > 1 {
            Some(Rejection::DuplicateId)
        } else if let Some(r) = validity(c, intents, now) {
            Some(r)
        } else {
            match &c.native {
                Some(k) if policy.exclude_native && capability.native.contains(k) => {
                    Some(Rejection::NativeEquivalent(k.clone()))
                }
                _ => None,
            }
        };
        match reason {
            Some(r) => rejected.push((c.id.clone(), r)),
            None => eligible.push(c),
        }
    }
    eligible.sort_by(|a, b| a.id.cmp(&b.id));
    rejected.sort();
    rejected.dedup();
    (eligible, rejected)
}
