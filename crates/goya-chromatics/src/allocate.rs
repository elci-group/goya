//! Allocation Engine and Stability Controller (§4, §8–§10).
//!
//! The procedure is fixed and deterministic:
//! 1. eligibility; 2. pins; 3. one total order (primary, then fallback,
//!    ties by candidate id); 4. selection, either fresh (immediate triggers)
//!    or stabilised (Chromatic Inertia); 5. placement (pins, preserved
//!    slots, then free slots in slot order).
//!
//! It is a stable top-k over at most a few slots, O(n log n) in the number
//! of candidates. Nothing heavier is warranted (§9).

use std::collections::{BTreeMap, BTreeSet};

use crate::eligibility::{evaluate, Rejection};
use crate::generation::{Binding, Generation, Placement, Selection};
use crate::model::{Candidate, Capability, Policy, SlotId, Strategy, Trigger};

/// One allocation request for the active scope.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// Context key. It must include the profile, so profiles never share
    /// history (§19).
    pub context: &'a str,
    pub policy: &'a Policy,
    pub candidates: &'a [Candidate],
    pub trigger: Trigger,
    /// Caller-supplied monotonic tick. The engine never reads a clock.
    pub now: u64,
    /// The user is navigating; stabilised triggers are deferred (§10.2).
    pub interacting: bool,
}

/// A stability or pin decision worth explaining (§27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// The pin could not be honoured; the candidate competed unpinned.
    PinIgnored {
        candidate: String,
        reason: Rejection,
    },
    /// An incumbent kept its slot against a better challenger.
    Retained {
        incumbent: String,
        challenger: String,
    },
    /// A challenger displaced an incumbent.
    Displaced {
        incumbent: String,
        challenger: String,
    },
    /// Reallocation was deferred because the user is interacting.
    Deferred,
}

/// Result of allocation: the generation plus diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    pub generation: Generation,
    /// Candidates left unbound, with the reason, sorted by id.
    pub rejected: Vec<(String, Rejection)>,
    /// Ordering score of every ranked candidate, best first.
    pub scores: Vec<(String, i64)>,
    pub notes: Vec<Note>,
}

impl Allocation {
    pub fn rejection(&self, id: &str) -> Option<&Rejection> {
        self.rejected.iter().find(|(c, _)| c == id).map(|(_, r)| r)
    }
}

struct Ranked<'a> {
    c: &'a Candidate,
    selection: Selection,
    score: i64,
}

fn order<'a>(
    pool: &[&'a Candidate],
    policy: &Policy,
    used_entities: &BTreeSet<String>,
    rejected: &mut Vec<(String, Rejection)>,
) -> Vec<Ranked<'a>> {
    let mut seen_entities = used_entities.clone();
    let mut out: Vec<Ranked<'a>> = Vec::new();
    let mut taken: BTreeSet<&str> = BTreeSet::new();
    let stages = [Some(policy.primary), policy.fallback];
    for (stage, strategy) in stages.iter().enumerate() {
        let Some(strategy) = *strategy else { continue };
        let mut keyed: Vec<(i128, &'a Candidate)> = pool
            .iter()
            .filter(|c| !taken.contains(c.id.as_str()))
            .filter_map(|c| strategy.key(c).map(|k| (k, *c)))
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        for (_, c) in keyed {
            taken.insert(c.id.as_str());
            if policy.exclude_duplicates {
                if let Some(e) = &c.entity {
                    if !seen_entities.insert(e.clone()) {
                        rejected.push((c.id.clone(), Rejection::DuplicateEntity));
                        continue;
                    }
                }
            }
            let place = out.len() as u32;
            let selection = if stage == 0 {
                Selection::Primary(place)
            } else {
                Selection::Fallback(place)
            };
            let score = if strategy == Strategy::Utility {
                c.utility()
            } else {
                -i64::from(place)
            };
            out.push(Ranked {
                c,
                selection,
                score,
            });
        }
    }
    for c in pool {
        if !taken.contains(c.id.as_str()) {
            rejected.push((c.id.clone(), Rejection::NotSupplied));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn binding(
    capability: &Capability,
    slot: SlotId,
    c: &Candidate,
    selected: Selection,
    placed: Placement,
    previous: Option<&Generation>,
    now: u64,
    contested_since: Option<u64>,
) -> Binding {
    let since = previous
        .and_then(|g| g.binding(slot))
        .filter(|b| b.candidate == c.id)
        .map_or(now, |b| b.since);
    Binding {
        slot,
        slot_name: capability.slots[usize::from(slot)].name.clone(),
        candidate: c.id.clone(),
        entity: c.entity.clone(),
        intent: c.intent.clone(),
        label: c.label.clone(),
        symbol: c.symbol.clone(),
        description: c.description.clone(),
        risk: c.risk,
        confirmation: c.confirmation,
        selected,
        placed,
        since,
        contested_since,
    }
}

/// Allocate without publishing. `previous` is this context's last
/// generation, if any; `id` is the generation id to stamp.
pub(crate) fn allocate(
    capability: &Capability,
    intents: &BTreeSet<String>,
    req: &Request<'_>,
    previous: Option<&Generation>,
    id: u64,
) -> Allocation {
    let policy = req.policy;
    let now = req.now;
    let previous = previous.filter(|g| g.context == req.context);
    let (eligible, mut rejected) = evaluate(req.candidates, capability, policy, intents, now);
    let available: BTreeSet<SlotId> = capability.available_slots().into_iter().collect();
    let mut free = available.clone();
    let mut bindings: Vec<Binding> = Vec::new();
    let mut used_entities: BTreeSet<String> = BTreeSet::new();
    let mut notes = Vec::new();

    // Pins, in candidate-id order.
    let mut holder: BTreeMap<SlotId, String> = BTreeMap::new();
    let mut pinned_ids: BTreeSet<&str> = BTreeSet::new();
    for c in &eligible {
        let Some(name) = &c.pinned else { continue };
        let slot = capability.slot(name).filter(|s| available.contains(s));
        let s = match slot {
            Some(s) if free.contains(&s) => s,
            other => {
                let reason = match other.and_then(|s| holder.get(&s)) {
                    Some(by) => Rejection::PinConflict(by.clone()),
                    None => Rejection::PinnedSlotUnavailable,
                };
                notes.push(Note::PinIgnored {
                    candidate: c.id.clone(),
                    reason,
                });
                continue;
            }
        };
        if policy.exclude_duplicates {
            if let Some(e) = &c.entity {
                if !used_entities.insert(e.clone()) {
                    rejected.push((c.id.clone(), Rejection::DuplicateEntity));
                    pinned_ids.insert(c.id.as_str());
                    continue;
                }
            }
        }
        free.remove(&s);
        holder.insert(s, c.id.clone());
        pinned_ids.insert(c.id.as_str());
        bindings.push(binding(
            capability,
            s,
            c,
            Selection::Pinned,
            Placement::Pinned,
            previous,
            now,
            None,
        ));
    }

    let pool: Vec<&Candidate> = eligible
        .iter()
        .copied()
        .filter(|c| !pinned_ids.contains(c.id.as_str()))
        .collect();
    let ranked = order(&pool, policy, &used_entities, &mut rejected);
    let scores: Vec<(String, i64)> = ranked.iter().map(|r| (r.c.id.clone(), r.score)).collect();
    let index: BTreeMap<&str, usize> = ranked
        .iter()
        .enumerate()
        .map(|(i, r)| (r.c.id.as_str(), i))
        .collect();
    let k = free.len();

    // Selection: list of (ranked index, kept slot, selection override, contested_since).
    struct Pick {
        idx: usize,
        keep: Option<SlotId>,
        retained: bool,
        contested_since: Option<u64>,
    }
    let mut picks: Vec<Pick> = Vec::new();
    let stabilised = matches!(req.trigger, Trigger::Background | Trigger::Invalidation);
    let incumbents: Vec<(usize, &Binding)> = match previous {
        Some(g) if stabilised => g
            .bindings
            .iter()
            .filter(|b| free.contains(&b.slot))
            .filter_map(|b| index.get(b.candidate.as_str()).map(|i| (*i, b)))
            .collect(),
        _ => Vec::new(),
    };
    let deferred_since = previous.and_then(|g| g.deferred_since).unwrap_or(now);
    let defer = stabilised
        && req.interacting
        && previous.is_some()
        && now.saturating_sub(deferred_since) < policy.stability.max_deferral;

    if !stabilised {
        picks.extend((0..ranked.len().min(k)).map(|idx| Pick {
            idx,
            keep: None,
            retained: false,
            contested_since: None,
        }));
    } else if defer || req.trigger == Trigger::Invalidation {
        // Keep every still-valid incumbent where it is.
        for (idx, b) in &incumbents {
            picks.push(Pick {
                idx: *idx,
                keep: Some(b.slot),
                retained: false,
                contested_since: b.contested_since,
            });
        }
        if defer {
            notes.push(Note::Deferred);
        } else if !req.interacting {
            // Fill freed slots from the order.
            let held: BTreeSet<usize> = incumbents.iter().map(|(i, _)| *i).collect();
            let room = k.saturating_sub(picks.len());
            picks.extend(
                (0..ranked.len())
                    .filter(|i| !held.contains(i))
                    .take(room)
                    .map(|idx| Pick {
                        idx,
                        keep: None,
                        retained: false,
                        contested_since: None,
                    }),
            );
        }
    } else {
        // Chromatic Inertia (§10.3).
        let ideal: BTreeSet<usize> = (0..ranked.len().min(k)).collect();
        let held: BTreeSet<usize> = incumbents.iter().map(|(i, _)| *i).collect();
        for (idx, b) in &incumbents {
            if ideal.contains(idx) {
                picks.push(Pick {
                    idx: *idx,
                    keep: Some(b.slot),
                    retained: false,
                    contested_since: None,
                });
            }
        }
        // Contested incumbents, worst first.
        let mut contested: Vec<(usize, &Binding)> = incumbents
            .iter()
            .filter(|(i, _)| !ideal.contains(i))
            .copied()
            .collect();
        contested.sort_by_key(|a| std::cmp::Reverse(a.0));
        let mut empty = k - picks.len() - contested.len();
        let challengers: Vec<usize> = ideal
            .iter()
            .copied()
            .filter(|i| !held.contains(i))
            .collect();
        let st = policy.stability;
        for ch in challengers {
            if empty > 0 {
                empty -= 1;
                picks.push(Pick {
                    idx: ch,
                    keep: None,
                    retained: false,
                    contested_since: None,
                });
                continue;
            }
            let pos = contested.iter().position(|(d, b)| {
                let gain = ranked[ch].score.saturating_sub(ranked[*d].score);
                let resident = now.saturating_sub(b.since) >= st.min_residency;
                let overdue = b
                    .contested_since
                    .is_some_and(|t| now.saturating_sub(t) >= st.max_deferral);
                overdue || (gain >= st.min_improvement && resident)
            });
            match pos {
                Some(p) => {
                    let (d, _) = contested.remove(p);
                    notes.push(Note::Displaced {
                        incumbent: ranked[d].c.id.clone(),
                        challenger: ranked[ch].c.id.clone(),
                    });
                    picks.push(Pick {
                        idx: ch,
                        keep: None,
                        retained: false,
                        contested_since: None,
                    });
                }
                None => {
                    if let Some((d, _)) = contested.first() {
                        notes.push(Note::Retained {
                            incumbent: ranked[*d].c.id.clone(),
                            challenger: ranked[ch].c.id.clone(),
                        });
                    }
                }
            }
        }
        for (idx, b) in contested {
            picks.push(Pick {
                idx,
                keep: Some(b.slot),
                retained: true,
                contested_since: Some(b.contested_since.unwrap_or(now)),
            });
        }
    }

    // Placement.
    picks.sort_by_key(|p| p.idx);
    let mut placed: Vec<(usize, SlotId, Placement, bool, Option<u64>)> = Vec::new();
    let mut rest = Vec::new();
    for p in &picks {
        let keep = if policy.ordered_slots {
            None
        } else if p.keep.is_some() {
            p.keep
        } else if policy.preserve_slots && !stabilised {
            previous
                .and_then(|g| g.by_candidate(&ranked[p.idx].c.id))
                .map(|b| b.slot)
                .filter(|s| free.contains(s))
        } else {
            None
        };
        match keep {
            Some(s) if free.remove(&s) => placed.push((
                p.idx,
                s,
                Placement::Preserved,
                p.retained,
                p.contested_since,
            )),
            _ => rest.push(p),
        }
    }
    for p in rest {
        let Some(s) = free.iter().next().copied() else {
            break;
        };
        free.remove(&s);
        placed.push((p.idx, s, Placement::Free, p.retained, p.contested_since));
    }
    for (idx, slot, placement, retained, contested_since) in placed {
        let r = &ranked[idx];
        let selection = if retained {
            Selection::Retained
        } else {
            r.selection
        };
        bindings.push(binding(
            capability,
            slot,
            r.c,
            selection,
            placement,
            previous,
            now,
            contested_since,
        ));
    }
    bindings.sort_by_key(|b| b.slot);

    let bound: BTreeSet<&str> = bindings.iter().map(|b| b.candidate.as_str()).collect();
    let already: BTreeSet<String> = rejected.iter().map(|(c, _)| c.clone()).collect();
    for r in &ranked {
        if !bound.contains(r.c.id.as_str()) && !already.contains(&r.c.id) {
            rejected.push((r.c.id.clone(), Rejection::OverCapacity));
        }
    }
    rejected.sort();
    rejected.dedup();

    Allocation {
        generation: Generation {
            id,
            context: req.context.to_string(),
            bindings,
            now,
            deferred_since: if defer { Some(deferred_since) } else { None },
        },
        rejected,
        scores,
        notes,
    }
}
