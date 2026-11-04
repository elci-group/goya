//! GY-RPI/1 — Relativistic Probability Indexing.
//!
//! Level-1 deterministic conditional probability learning over
//! (context, action) pairs, prediction calibration, economic ranking,
//! and strict preparation/execution separation.

use std::collections::HashMap;

pub const RPI_CONTRACT: &str = "GY-RPI/1";

/// Identifies an action within the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(pub u64);

/// Identifies a context frame; probabilities are relative to it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContextTag(pub String);

/// Conditional probability field P(a|C), learned by counting.
#[derive(Debug, Clone, Default)]
pub struct ProbabilityField {
    /// context -> action -> observation count.
    pub context_weights: HashMap<ContextTag, HashMap<ActionId, f64>>,
}

impl ProbabilityField {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one occurrence of `action` in `context` (count-based, deterministic).
    pub fn observe(&mut self, context: &ContextTag, action: ActionId) {
        let actions = self.context_weights.entry(context.clone()).or_default();
        *actions.entry(action).or_insert(0.0) += 1.0;
    }

    /// Normalised conditional share P(action | context). 0.0 when unknown.
    pub fn probability(&self, context: &ContextTag, action: ActionId) -> f64 {
        let actions = match self.context_weights.get(context) {
            Some(a) => a,
            None => return 0.0,
        };
        let total: f64 = actions.values().copied().sum();
        if total <= 0.0 {
            return 0.0;
        }
        actions.get(&action).copied().unwrap_or(0.0) / total
    }
}

/// Calibration of predicted probabilities against observed outcomes.
#[derive(Debug, Clone, Default)]
pub struct Calibration {
    pub predicted_sum: f64,
    pub hits: u64,
    pub n: u64,
    /// Private accumulator: sum of (p_i - y_i)^2 for exact Brier MSE.
    squared_error_sum: f64,
}

impl Calibration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a prediction and whether the predicted event occurred.
    pub fn record(&mut self, predicted: f64, occurred: bool) {
        let p = predicted.clamp(0.0, 1.0);
        let y = if occurred { 1.0 } else { 0.0 };
        self.predicted_sum += p;
        if occurred {
            self.hits += 1;
        }
        self.squared_error_sum += (p - y) * (p - y);
        self.n += 1;
    }

    /// Mean squared error over recorded predictions (Brier score).
    /// 0.0 = perfect; higher is worse. 0.0 for empty history.
    pub fn brier_score(&self) -> f64 {
        if self.n == 0 {
            return 0.0;
        }
        self.squared_error_sum / self.n as f64
    }
}

/// Economic value of a speculated action.
/// `net_value = p*benefit - cost - lambda*wasted`.
pub fn net_value(p: f64, benefit: f64, cost: f64, wasted: f64, lambda: f64) -> f64 {
    p * benefit - cost - lambda * wasted
}

/// Rank candidates by net value, descending. Returns (action, net value).
pub fn rank_candidates(
    candidates: Vec<(ActionId, f64, f64, f64, f64)>,
    lambda: f64,
) -> Vec<(ActionId, f64)> {
    let mut ranked: Vec<(ActionId, f64)> = candidates
        .into_iter()
        .map(|(action, p, benefit, cost, wasted)| {
            (action, net_value(p, benefit, cost, wasted, lambda))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked
}

/// A prepared (not yet executed) outcome, tied to state and geometry revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preparation {
    pub action: ActionId,
    pub state_revision: u64,
    pub geometry_revision: u64,
    pub payload_hash: u64,
}

/// A preparation is valid only if neither revision has moved.
pub fn valid_for(prep: &Preparation, state_rev: u64, geom_rev: u64) -> bool {
    prep.state_revision == state_rev && prep.geometry_revision == geom_rev
}

/// Opt-in gate for Level-2 learned behaviour. Requires BOTH flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LearnedMode {
    pub enabled: bool,
    pub consented: bool,
}

impl LearnedMode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Learning is permitted only when explicitly enabled AND consented.
    pub fn can_learn(&self) -> bool {
        self.enabled && self.consented
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(name: &str) -> ContextTag {
        ContextTag(name.to_string())
    }

    #[test]
    fn conditional_normalisation_sums_to_one() {
        let mut field = ProbabilityField::new();
        let c = ctx("waking");
        for _ in 0..3 {
            field.observe(&c, ActionId(1));
        }
        for _ in 0..7 {
            field.observe(&c, ActionId(2));
        }
        let total = field.probability(&c, ActionId(1)) + field.probability(&c, ActionId(2));
        assert!((total - 1.0).abs() < 1e-9);
        assert!((field.probability(&c, ActionId(1)) - 0.3).abs() < 1e-9);
        assert!((field.probability(&c, ActionId(2)) - 0.7).abs() < 1e-9);
    }

    #[test]
    fn unknown_context_or_action_is_zero() {
        let mut field = ProbabilityField::new();
        let c = ctx("empty");
        assert_eq!(field.probability(&c, ActionId(9)), 0.0);
        field.observe(&ctx("other"), ActionId(1));
        assert_eq!(field.probability(&c, ActionId(1)), 0.0);
        assert_eq!(field.probability(&ctx("other"), ActionId(42)), 0.0);
    }

    #[test]
    fn context_relativity_same_action_different_probabilities() {
        let mut field = ProbabilityField::new();
        let a = ActionId(1);
        let commuting = ctx("commuting");
        let relaxing = ctx("relaxing");
        for _ in 0..9 {
            field.observe(&commuting, a);
        }
        field.observe(&commuting, ActionId(2));
        for _ in 0..2 {
            field.observe(&relaxing, a);
        }
        for _ in 0..8 {
            field.observe(&relaxing, ActionId(2));
        }
        let p_commute = field.probability(&commuting, a);
        let p_relax = field.probability(&relaxing, a);
        assert!((p_commute - 0.9).abs() < 1e-9);
        assert!((p_relax - 0.2).abs() < 1e-9);
        assert!(p_commute != p_relax);
    }

    #[test]
    fn brier_score_decreases_for_better_predictions() {
        let mut poor = Calibration::new();
        let mut good = Calibration::new();
        let outcomes = [true, false, true, false, true];
        for &y in &outcomes {
            poor.record(0.9, y); // overconfident, frequently wrong
            good.record(if y { 0.9 } else { 0.1 }, y); // well calibrated
        }
        assert!(good.brier_score() < poor.brier_score());
        let mut perfect = Calibration::new();
        for &y in &outcomes {
            perfect.record(if y { 1.0 } else { 0.0 }, y);
        }
        assert!((perfect.brier_score()).abs() < 1e-9);
    }

    #[test]
    fn net_value_formula_and_ranking() {
        let nv = net_value(0.5, 10.0, 2.0, 1.0, 3.0);
        assert!((nv - (0.5 * 10.0 - 2.0 - 3.0 * 1.0)).abs() < 1e-9);
        assert!((nv - 0.0).abs() < 1e-9);

        let ranked = rank_candidates(
            vec![
                (ActionId(3), 0.2, 10.0, 1.0, 0.0), // 1.0
                (ActionId(1), 0.9, 10.0, 1.0, 0.0), // 8.0
                (ActionId(2), 0.5, 10.0, 1.0, 0.0), // 4.0
            ],
            1.0,
        );
        assert_eq!(ranked[0].0, ActionId(1));
        assert_eq!(ranked[1].0, ActionId(2));
        assert_eq!(ranked[2].0, ActionId(3));
        assert!(ranked[0].1 >= ranked[1].1 && ranked[1].1 >= ranked[2].1);
    }

    #[test]
    fn negative_net_value_ranks_last() {
        let ranked = rank_candidates(
            vec![
                (ActionId(7), 0.1, 5.0, 9.0, 0.0),  // -8.5
                (ActionId(8), 1.0, 20.0, 2.0, 0.0), // 18.0
                (ActionId(9), 0.0, 5.0, 1.0, 1.0),  // -2.0
            ],
            1.0,
        );
        assert_eq!(ranked.last().map(|(a, _)| *a), Some(ActionId(7)));
        assert_eq!(ranked.first().map(|(a, _)| *a), Some(ActionId(8)));
    }

    #[test]
    fn revision_invalidation_both_axes() {
        let prep = Preparation {
            action: ActionId(1),
            state_revision: 4,
            geometry_revision: 2,
            payload_hash: 0xDEAD_BEEF,
        };
        assert!(valid_for(&prep, 4, 2));
        assert!(!valid_for(&prep, 5, 2), "state revision change invalidates");
        assert!(
            !valid_for(&prep, 4, 3),
            "geometry revision change invalidates"
        );
        assert!(!valid_for(&prep, 5, 3));
    }

    #[test]
    fn learned_mode_off_by_default_and_requires_consent() {
        assert!(!LearnedMode::default().can_learn());
        assert!(!LearnedMode {
            enabled: true,
            consented: false
        }
        .can_learn());
        assert!(!LearnedMode {
            enabled: false,
            consented: true
        }
        .can_learn());
        assert!(LearnedMode {
            enabled: true,
            consented: true
        }
        .can_learn());
    }

    #[test]
    fn contract_constant_matches_spec() {
        assert_eq!(RPI_CONTRACT, "GY-RPI/1");
    }
}
