//! goya-economics — deterministic economic policy layer (GY-ECONOMICS/1).
//!
//! std-only. Relativistic valuation, allowance authorisation, economic
//! inheritance (narrowing-only), and immutable intent records.

/// Contract identifier for this crate.
pub const ECONOMICS_CONTRACT: &str = "GY-ECONOMICS/1";

use std::sync::Arc;

/// Exact-integer money amount (mirror of goya-x402's Amount; std-only crates
/// are independent, so the shape is duplicated here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amount {
    pub units: u128,
    pub asset: String,
    pub network: String,
}

impl Amount {
    pub fn parse(units: u128, asset: &str, network: &str) -> Self {
        Amount {
            units,
            asset: asset.to_string(),
            network: network.to_string(),
        }
    }
}

/// Relativistic valuation:
/// V(a) = P(success|C) * B - C_pay - C_compute - C_risk
///
/// This is a decision-scoring quantity, not a settled monetary balance,
/// so floating point is acceptable here; monetary amounts themselves are
/// always exact integers (`Amount`).
pub fn value(p_success: f64, benefit: f64, pay: f64, compute: f64, risk: f64) -> f64 {
    p_success * benefit - pay - compute - risk
}

/// Outcome of an allowance authorisation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorisation {
    /// Within pre-approved policy; proceed without human sign-off.
    Auto,
    /// Within allowance but policy demands per-payment approval.
    RequiresApproval,
    /// Over allowance; denied outright.
    Denied,
}

/// Spending policy knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpendingPolicy {
    pub require_approval_per_payment: bool,
}

/// An allowance with a hard integer limit and running spend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowance {
    pub limit_units: u128,
    pub spent_units: u128,
}

impl Allowance {
    pub fn new(limit_units: u128) -> Self {
        Allowance {
            limit_units,
            spent_units: 0,
        }
    }

    /// Remaining spendable units (saturating; spent can never exceed limit
    /// through this API).
    pub fn remaining(&self) -> u128 {
        self.limit_units.saturating_sub(self.spent_units)
    }

    /// Attempt to authorise `cost`. On `Auto`/`RequiresApproval` the spend is
    /// reserved (recorded); `Denied` leaves the allowance untouched.
    pub fn try_authorise(&mut self, cost: Amount, policy: &SpendingPolicy) -> Authorisation {
        let remaining = self.remaining();
        if cost.units > remaining {
            return Authorisation::Denied;
        }
        let decision = if policy.require_approval_per_payment {
            Authorisation::RequiresApproval
        } else {
            Authorisation::Auto
        };
        // Reserve spend for approved paths so consecutive authorisations see
        // the reduced balance. RequiresApproval reserves against over-draw too:
        // approval is per-payment, not per-limit.
        self.spent_units = self.spent_units.saturating_add(cost.units);
        decision
    }
}

/// Economic inheritance (spec §8): a child's effective allowance is the MIN of
/// its ancestors' remaining allowances. Permissions may narrow through
/// inheritance, never expand.
///
/// An empty ancestor list inherits nothing (0) — no ancestors means no
/// delegated economic authority.
pub fn inherited_allowance(ancestors: &[Allowance]) -> u128 {
    ancestors.iter().map(|a| a.remaining()).min().unwrap_or(0)
}

/// Settlement state of a recorded intent (spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementState {
    Pending,
    Settled,
    /// Settlement outcome unknown; reconciliation required.
    Unreconciled,
}

/// Immutable intent record (spec §6): a deterministic snapshot of the full
/// authorisation basis at the moment a payment intent was formed. Every field
/// is captured at construction; nothing is inferred later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentRecord {
    pub capability: String,
    pub resource: String,
    pub amount: Amount,
    pub recipient: String,
    pub principal: String,
    pub policy_hash: u64,
    pub payload_ref: String,
    pub settlement: SettlementState,
}

impl IntentRecord {
    /// Snapshot everything into an immutable record, settlement Pending.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        capability: &str,
        resource: &str,
        amount: Amount,
        recipient: &str,
        principal: &str,
        policy_hash: u64,
        payload_ref: &str,
    ) -> Self {
        IntentRecord {
            capability: capability.to_string(),
            resource: resource.to_string(),
            amount,
            recipient: recipient.to_string(),
            principal: principal.to_string(),
            policy_hash,
            payload_ref: payload_ref.to_string(),
            settlement: SettlementState::Pending,
        }
    }

    /// Shared immutable handle (the record is never mutated in place).
    pub fn freeze(self) -> Arc<IntentRecord> {
        Arc::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usdc(units: u128) -> Amount {
        Amount::parse(units, "usdc", "base")
    }

    #[test]
    fn value_formula_matches_spec() {
        // V = 0.9 * 100 - 10 - 5 - 2 = 73
        let v = value(0.9, 100.0, 10.0, 5.0, 2.0);
        assert!((v - 73.0).abs() < 1e-9);
    }

    #[test]
    fn allowance_auto_within_limit() {
        let mut a = Allowance::new(1_000);
        let pol = SpendingPolicy::default();
        assert_eq!(a.try_authorise(usdc(400), &pol), Authorisation::Auto);
        assert_eq!(a.spent_units, 400);
    }

    #[test]
    fn allowance_requires_approval_when_policy_demands() {
        let mut a = Allowance::new(1_000);
        let pol = SpendingPolicy {
            require_approval_per_payment: true,
        };
        assert_eq!(
            a.try_authorise(usdc(1), &pol),
            Authorisation::RequiresApproval
        );
    }

    #[test]
    fn allowance_denied_over_limit_and_unspent() {
        let mut a = Allowance::new(100);
        let pol = SpendingPolicy::default();
        assert_eq!(a.try_authorise(usdc(101), &pol), Authorisation::Denied);
        // Denied must not record spend.
        assert_eq!(a.spent_units, 0);
        // Exactly-at-limit is allowed.
        assert_eq!(a.try_authorise(usdc(100), &pol), Authorisation::Auto);
        // Follow-up spend now exceeds remaining.
        assert_eq!(a.try_authorise(usdc(1), &pol), Authorisation::Denied);
    }

    #[test]
    fn inheritance_takes_minimum_narrowing_only() {
        let ancestors = vec![Allowance::new(1_000), Allowance::new(300)];
        assert_eq!(inherited_allowance(&ancestors), 300);
        // A child can never exceed the tightest ancestor.
        let ancestors2 = vec![Allowance::new(5_000), Allowance::new(9_000)];
        assert_eq!(inherited_allowance(&ancestors2), 5_000);
        // Spend on an ancestor narrows the inherited amount.
        let mut spent = Allowance::new(1_000);
        spent.try_authorise(usdc(700), &SpendingPolicy::default());
        let ancestors3 = vec![Allowance::new(1_000), spent];
        assert_eq!(inherited_allowance(&ancestors3), 300);
    }

    #[test]
    fn inheritance_with_no_ancestors_is_zero() {
        assert_eq!(inherited_allowance(&[]), 0);
    }

    #[test]
    fn intent_record_snapshots_all_fields() {
        let rec = IntentRecord::record(
            "http.fetch",
            "https://api.example.test/data",
            usdc(1_000),
            "0xaaa",
            "agent:goya-1",
            0xdead_beef,
            "x402:v2:exact:sig",
        );
        assert_eq!(rec.capability, "http.fetch");
        assert_eq!(rec.resource, "https://api.example.test/data");
        assert_eq!(rec.amount, usdc(1_000));
        assert_eq!(rec.recipient, "0xaaa");
        assert_eq!(rec.principal, "agent:goya-1");
        assert_eq!(rec.policy_hash, 0xdead_beef);
        assert_eq!(rec.payload_ref, "x402:v2:exact:sig");
        assert_eq!(rec.settlement, SettlementState::Pending);
    }

    #[test]
    fn money_amounts_are_exact_integers() {
        // u128 units, never floats.
        let big = u128::MAX;
        let a = usdc(big);
        assert_eq!(a.units, u128::MAX);
        let mut al = Allowance::new(u128::MAX);
        assert_eq!(
            al.try_authorise(a, &SpendingPolicy::default()),
            Authorisation::Auto
        );
        assert_eq!(al.remaining(), 0);
    }
}
