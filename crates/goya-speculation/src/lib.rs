//! GY-SPECULATION/1 — Speculative preparation, commit gating, and greedy
//! value-per-cost scheduling on top of GY-RPI/1.
//!
//! Speculation never overruns its [`Budget`], never commits an outcome whose
//! revisions have moved (strict preparation/execution separation from
//! `goya_rpi`), and only the outcome of the actually authorised action is
//! committed; all other speculative preparations are dropped at commit time.

use std::collections::HashMap;

use goya_ir::ActionId as RuntimeActionId;
use goya_rpi::{net_value, valid_for, ActionId, Preparation};
use goya_rpi2::{ConsentRecord, SequenceModel};

pub const SPECULATION_CONTRACT: &str = "GY-SPECULATION/1";

/// One learned successor prediction, bound to the exact runtime snapshot that
/// produced it. The caller must validate it again immediately before using
/// it; a state or geometry revision change makes it stale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level2Prediction {
    pub action: RuntimeActionId,
    pub probability: f64,
    pub state_revision: u64,
    pub geometry_revision: u64,
}

/// Consent-gated sequence learner used by the runtime's speculative path.
/// It is disabled by default, and does not learn or predict until the caller
/// explicitly opts in with a valid consent record.
#[derive(Debug, Clone)]
pub struct Level2Predictor {
    enabled: bool,
    model: SequenceModel,
    previous: Option<RuntimeActionId>,
}

impl Default for Level2Predictor {
    fn default() -> Self {
        Self::new()
    }
}

impl Level2Predictor {
    /// Create a disabled learner with no retained sequence data.
    pub fn new() -> Self {
        Self {
            enabled: false,
            model: SequenceModel::without_consent(),
            previous: None,
        }
    }

    /// Explicitly opt in. Consent remains checked at each observe/predict
    /// call against the caller-supplied timestamp.
    pub fn opt_in(&mut self, consent: ConsentRecord, max_transitions: usize) {
        self.model = SequenceModel::with_consent_and_cap(consent, max_transitions);
        self.previous = None;
        self.enabled = true;
    }

    /// Withdraw consent and erase all learned transitions immediately.
    pub fn opt_out(&mut self) {
        self.enabled = false;
        self.model.forget_all();
        self.model = SequenceModel::without_consent();
        self.previous = None;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Record one authorised interaction. The first event seeds the sequence;
    /// later events train a single adjacent transition. No event is retained
    /// when consent is absent, withdrawn, or expired.
    pub fn observe_action(&mut self, action: RuntimeActionId, now_ns: u64) -> bool {
        if !self.enabled || !self.model.can_learn_at(now_ns) {
            self.previous = None;
            return false;
        }
        let current = goya_rpi2::ActionId(action.0);
        if let Some(previous) = self.previous {
            self.model
                .observe_sequence(&[goya_rpi2::ActionId(previous.0), current], now_ns);
        }
        self.previous = Some(action);
        true
    }

    /// Predict the most likely next action from the latest observed action.
    /// Predictions are tagged with both revisions and withheld if the model
    /// is disabled or the consent record has expired.
    pub fn predict_next_at(
        &self,
        now_ns: u64,
        state_revision: u64,
        geometry_revision: u64,
    ) -> Option<Level2Prediction> {
        if !self.enabled || !self.model.can_learn_at(now_ns) {
            return None;
        }
        let last = self.previous?;
        let (action, probability) = self.model.predict_next(goya_rpi2::ActionId(last.0))?;
        Some(Level2Prediction {
            action: RuntimeActionId(action.0),
            probability,
            state_revision,
            geometry_revision,
        })
    }

    /// Re-check consent and exact revisions before speculative work is used.
    pub fn is_current_at(
        &self,
        prediction: &Level2Prediction,
        now_ns: u64,
        state_revision: u64,
        geometry_revision: u64,
    ) -> bool {
        self.enabled
            && self.model.can_learn_at(now_ns)
            && prediction.state_revision == state_revision
            && prediction.geometry_revision == geometry_revision
            && prediction.probability.is_finite()
            && (0.0..=1.0).contains(&prediction.probability)
    }
}

/// Finite speculation budget (Mesut-governed): speculation never overruns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Budget {
    pub remaining: i64,
}

impl Budget {
    pub fn new(remaining: i64) -> Self {
        Self { remaining }
    }

    /// Spend `cost` if and only if the budget covers it; never goes negative.
    /// Returns `true` when the spend succeeded.
    pub fn try_spend(&mut self, cost: i64) -> bool {
        if cost < 0 || cost > self.remaining {
            return false;
        }
        self.remaining -= cost;
        true
    }
}

/// Cache of speculative `Preparation`s keyed by `ActionId`, each tied to the
/// state and geometry revisions it was prepared against.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    entries: HashMap<ActionId, Preparation>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) the speculative preparation for `action`.
    pub fn insert(&mut self, prep: Preparation) {
        self.entries.insert(prep.action, prep);
    }

    /// Commit gate: return the prepared outcome ONLY when `goya_rpi::valid_for`
    /// passes on both the state and geometry revisions.
    pub fn lookup_valid(
        &self,
        action: ActionId,
        state_rev: u64,
        geom_rev: u64,
    ) -> Option<&Preparation> {
        self.entries
            .get(&action)
            .filter(|prep| valid_for(prep, state_rev, geom_rev))
    }

    /// Drop all entries prepared against a stale geometry revision.
    pub fn invalidate_revision(&mut self, geom_rev: u64) {
        self.entries
            .retain(|_, prep| prep.geometry_revision == geom_rev);
    }

    /// Only the outcome of the ACTUAL authorised action commits; all other
    /// speculative preparations are dropped.
    pub fn drop_uncommitted(&mut self, keep: ActionId) {
        self.entries.retain(|action, _| *action == keep);
    }

    /// Number of cached preparations.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Result of attempting to commit a speculated action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitDecision {
    /// A valid preparation existed and is committed.
    Commit(Preparation),
    /// Nothing was prepared for this action; recompute from scratch.
    Recompute,
    /// A preparation existed but its revisions have moved; reject it.
    Reject,
}

/// Commit gate for the authorised action: `Commit` only for valid
/// preparations, missing -> `Recompute`, stale -> `Reject`.
pub fn commit(cache: &Cache, action: ActionId, state_rev: u64, geom_rev: u64) -> CommitDecision {
    match cache.lookup_valid(action, state_rev, geom_rev) {
        Some(prep) => CommitDecision::Commit(prep.clone()),
        None => {
            if cache.entries.contains_key(&action) {
                CommitDecision::Reject
            } else {
                CommitDecision::Recompute
            }
        }
    }
}

/// A speculative scheduling candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub action: ActionId,
    pub p: f64,
    pub benefit: f64,
    pub cost: f64,
    pub wasted: f64,
}

impl Candidate {
    fn net(&self, lambda: f64) -> f64 {
        net_value(self.p, self.benefit, self.cost, self.wasted, lambda)
    }
}

/// Greedy value-per-cost scheduler: rank candidates by `goya_rpi::net_value`
/// descending; take while the budget allows and net value > 0. Expensive
/// items that do not fit are skipped (not blocking) so a later cheaper
/// positive-value item can still be scheduled.
pub fn schedule(candidates: Vec<Candidate>, budget: i64, lambda: f64) -> Vec<ActionId> {
    let mut ranked: Vec<&Candidate> = candidates.iter().collect();
    ranked.sort_by(|a, b| {
        b.net(lambda)
            .partial_cmp(&a.net(lambda))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut remaining = budget;
    let mut chosen = Vec::new();
    for candidate in ranked {
        let net = candidate.net(lambda);
        if net <= 0.0 {
            // Sorted descending: everything after this is also non-positive.
            break;
        }
        let cost = candidate.cost as i64;
        if cost <= remaining {
            remaining -= cost;
            chosen.push(candidate.action);
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prep(action: u64, state_rev: u64, geom_rev: u64) -> Preparation {
        Preparation {
            action: ActionId(action),
            state_revision: state_rev,
            geometry_revision: geom_rev,
            payload_hash: action * 1000 + state_rev,
        }
    }

    #[test]
    fn budget_never_goes_negative() {
        let mut budget = Budget::new(10);
        assert!(budget.try_spend(7));
        assert_eq!(budget.remaining, 3);
        // Overdraft refused: budget stays at 3, never negative.
        assert!(!budget.try_spend(4));
        assert_eq!(budget.remaining, 3);
        assert!(!budget.try_spend(i64::MAX));
        assert_eq!(budget.remaining, 3);
        assert!(budget.try_spend(3));
        assert_eq!(budget.remaining, 0);
        assert!(!budget.try_spend(1));
        assert_eq!(budget.remaining, 0);
    }

    #[test]
    fn negative_cost_is_refused() {
        let mut budget = Budget::new(5);
        assert!(!budget.try_spend(-1));
        assert_eq!(budget.remaining, 5);
    }

    #[test]
    fn valid_lookup_succeeds() {
        let mut cache = Cache::new();
        cache.insert(prep(1, 5, 9));
        assert_eq!(cache.lookup_valid(ActionId(1), 5, 9), Some(&prep(1, 5, 9)));
        // Same action, moved revisions -> not returned.
        assert_eq!(cache.lookup_valid(ActionId(1), 6, 9), None);
        assert_eq!(cache.lookup_valid(ActionId(1), 5, 10), None);
    }

    #[test]
    fn stale_state_revision_rejects() {
        let mut cache = Cache::new();
        cache.insert(prep(2, 4, 1));
        assert_eq!(commit(&cache, ActionId(2), 5, 1), CommitDecision::Reject);
    }

    #[test]
    fn stale_geometry_revision_rejects() {
        let mut cache = Cache::new();
        cache.insert(prep(3, 1, 7));
        assert_eq!(commit(&cache, ActionId(3), 1, 8), CommitDecision::Reject);
    }

    #[test]
    fn missing_preparation_recomputes() {
        let cache = Cache::new();
        assert_eq!(commit(&cache, ActionId(9), 1, 1), CommitDecision::Recompute);
    }

    #[test]
    fn valid_preparation_commits() {
        let mut cache = Cache::new();
        cache.insert(prep(4, 2, 2));
        assert_eq!(
            commit(&cache, ActionId(4), 2, 2),
            CommitDecision::Commit(prep(4, 2, 2))
        );
    }

    #[test]
    fn invalidate_revision_drops_stale_geometry() {
        let mut cache = Cache::new();
        cache.insert(prep(1, 1, 3));
        cache.insert(prep(2, 1, 3));
        cache.insert(prep(3, 1, 4));
        cache.invalidate_revision(4);
        assert!(cache.lookup_valid(ActionId(1), 1, 3).is_none());
        assert!(cache.lookup_valid(ActionId(2), 1, 3).is_none());
        assert_eq!(
            cache.lookup_valid(ActionId(3), 1, 4).unwrap().action,
            ActionId(3)
        );
    }

    #[test]
    fn drop_uncommitted_removes_others() {
        let mut cache = Cache::new();
        cache.insert(prep(1, 1, 1));
        cache.insert(prep(2, 1, 1));
        cache.insert(prep(3, 1, 1));
        cache.drop_uncommitted(ActionId(2));
        assert_eq!(cache.len(), 1);
        assert!(cache.lookup_valid(ActionId(2), 1, 1).is_some());
        assert!(cache.lookup_valid(ActionId(1), 1, 1).is_none());
        assert!(cache.lookup_valid(ActionId(3), 1, 1).is_none());
    }

    #[test]
    fn scheduler_greedy_within_budget_and_skips_unaffordable() {
        let lambda = 0.5;
        let candidates = vec![
            // net = 0.9*10 - 8 - 0.5*0 = 1.0
            Candidate {
                action: ActionId(1),
                p: 0.9,
                benefit: 10.0,
                cost: 8.0,
                wasted: 0.0,
            },
            // net = 0.8*5 - 2 - 0.5*1 = 1.5 (highest)
            Candidate {
                action: ActionId(2),
                p: 0.8,
                benefit: 5.0,
                cost: 2.0,
                wasted: 1.0,
            },
            // net = 0.5*4 - 3 - 0.5*0 = -1.0 (negative, never picked)
            Candidate {
                action: ActionId(3),
                p: 0.5,
                benefit: 4.0,
                cost: 3.0,
                wasted: 0.0,
            },
            // net = 0.9*4 - 1 - 0.5*1 = 2.1
            Candidate {
                action: ActionId(4),
                p: 0.9,
                benefit: 4.0,
                cost: 1.0,
                wasted: 1.0,
            },
        ];
        // Budget 4: picks 4 (2.1), 2 (1.5) -> remaining 1; skips 1 (cost 8,
        // does not fit) without blocking; 3 is negative.
        let chosen = schedule(candidates, 4, lambda);
        assert_eq!(chosen, vec![ActionId(4), ActionId(2)]);
    }

    #[test]
    fn scheduler_budget_bounds_total_cost() {
        let lambda = 0.0;
        let candidates = vec![
            Candidate {
                action: ActionId(1),
                p: 1.0,
                benefit: 10.0,
                cost: 6.0,
                wasted: 0.0,
            },
            Candidate {
                action: ActionId(2),
                p: 1.0,
                benefit: 10.0,
                cost: 6.0,
                wasted: 0.0,
            },
            Candidate {
                action: ActionId(3),
                p: 1.0,
                benefit: 10.0,
                cost: 6.0,
                wasted: 0.0,
            },
        ];
        // Only one 6-cost item fits in a budget of 10.
        let chosen = schedule(candidates, 10, lambda);
        assert_eq!(chosen, vec![ActionId(1)]);
    }

    #[test]
    fn scheduler_empty_and_zero_budget() {
        assert!(schedule(Vec::new(), 100, 1.0).is_empty());
        assert!(schedule(
            vec![Candidate {
                action: ActionId(1),
                p: 1.0,
                benefit: 1.0,
                cost: 1.0,
                wasted: 0.0
            }],
            0,
            1.0
        )
        .is_empty());
    }
}
