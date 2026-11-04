//! goya-settlement — settlement half of directive GY-X402-001.
//!
//! Immutable payment intents, an append-only ledger, and reconciliation
//! rules that honour the directive's safety rule: *a timeout must not mean
//! a failed payment*. A lost receipt means funds may have moved; the intent
//! must be reconciled before any duplicate charge may be authorised.
//!
//! std + goya-x402. No network, no crypto, no floats.

/// Wire/contract identifier for this directive.
pub const SETTLEMENT_CONTRACT: &str = "GY-SETTLEMENT/1";

/// Settlement status recorded against an intent. Set at creation and only
/// ever superseded by appending a newer outcome to the ledger — records
/// themselves are never mutated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementState {
    /// Recorded, awaiting a receipt.
    Pending,
    /// Receipt verified; obligation discharged.
    Settled,
    /// Receipt lost/timeout: funds may have moved. Requires investigation
    /// before any duplicate authorisation.
    NeedsInvestigation,
    /// A duplicate charge was attempted and blocked.
    DuplicateBlocked,
}

/// An immutable record of one payment intent. Never mutated after creation;
/// corrections are appended as new ledger outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentRecord {
    pub intent_ref: String,
    pub capability: String,
    pub amount_units: u128,
    /// Secret-classified: excluded from diagnostics output.
    pub signature_ref: String,
    /// Secret/idempotency-classified: excluded from diagnostics output.
    pub payload_ref: String,
    pub created_at_ns: u64,
    pub settlement: SettlementState,
}

impl IntentRecord {
    /// Create a new immutable intent record in the `Pending` state.
    pub fn new(
        intent_ref: &str,
        capability: &str,
        amount_units: u128,
        signature_ref: &str,
        payload_ref: &str,
        created_at_ns: u64,
    ) -> Self {
        IntentRecord {
            intent_ref: intent_ref.to_string(),
            capability: capability.to_string(),
            amount_units,
            signature_ref: signature_ref.to_string(),
            payload_ref: payload_ref.to_string(),
            created_at_ns,
            settlement: SettlementState::Pending,
        }
    }
}

/// Observed receipt status reported by the (fallible) outside world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptStatus {
    /// Receipt confirmed.
    Verified,
    /// Receipt lost / timeout. NOT a failed payment: funds may have moved.
    Lost,
    /// A second charge for the same obligation is suspected.
    DuplicateSuspected,
}

/// Outcome of reconciling an intent against an observed receipt status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciliation {
    /// Receipt verified; obligation discharged.
    Settled,
    /// Funds may have moved; investigate before authorising any duplicate.
    NeedsInvestigation,
    /// Duplicate charge blocked.
    DuplicateBlocked,
}

/// Reconcile an intent against an observed receipt status.
///
/// Directive rule: "timeout must not mean failed payment". `Lost` maps to
/// `NeedsInvestigation` — never to a settled failure that would license an
/// automatic duplicate charge. `DuplicateSuspected` maps to
/// `DuplicateBlocked` — the second charge is refused.
pub fn reconcile(record: &IntentRecord, observed: ReceiptStatus) -> Reconciliation {
    let _ = record; // records are immutable inputs; status alone decides
    match observed {
        ReceiptStatus::Verified => Reconciliation::Settled,
        ReceiptStatus::Lost => Reconciliation::NeedsInvestigation,
        ReceiptStatus::DuplicateSuspected => Reconciliation::DuplicateBlocked,
    }
}

/// Append-only settlement ledger. Intents are added via [`Ledger::record_intent`]
/// and outcomes via [`Ledger::record_outcome`]; neither ever removes or rewrites
/// a prior entry. Outcomes are keyed by intent_ref with later entries winning,
/// but the full history is retained for audit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Append-only list of immutable intent records.
    pub intents: Vec<IntentRecord>,
    /// Append-only list of (intent_ref, outcome) reconciliation events.
    pub outcomes: Vec<(String, Reconciliation)>,
}

impl Ledger {
    /// Append an intent record. Appends only; there is no removal API.
    pub fn record_intent(&mut self, record: IntentRecord) {
        self.intents.push(record);
    }

    /// Append a reconciliation outcome for an intent. Appends only.
    pub fn record_outcome(&mut self, intent_ref: &str, outcome: Reconciliation) {
        self.outcomes.push((intent_ref.to_string(), outcome));
    }

    /// Latest appended outcome for an intent, if any.
    pub fn latest_outcome(&self, intent_ref: &str) -> Option<Reconciliation> {
        self.outcomes
            .iter()
            .rev()
            .find(|(r, _)| r == intent_ref)
            .map(|(_, o)| *o)
    }
}

/// Idempotency gate: a fresh charge may only be authorised if no recorded
/// intent already carries the same `payload_ref` (nonce reuse is refused).
pub fn can_authorise(ledger: &Ledger, payload_ref: &str) -> bool {
    !ledger.intents.iter().any(|r| r.payload_ref == payload_ref)
}

/// A settlement receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub intent_ref: String,
    pub payload_ref: String,
    pub settled_at_ns: u64,
}

/// Audit trail: every recorded intent whose settlement is not `Settled`
/// (open obligations: pending, needs-investigation, or duplicate-blocked).
pub fn audit(ledger: &Ledger) -> Vec<&IntentRecord> {
    ledger
        .intents
        .iter()
        .filter(|r| match ledger.latest_outcome(&r.intent_ref) {
            Some(Reconciliation::Settled) => false,
            Some(Reconciliation::NeedsInvestigation) => true,
            Some(Reconciliation::DuplicateBlocked) => true,
            None => r.settlement != SettlementState::Settled,
        })
        .collect()
}

/// One-line, secrets-free summary of an intent for diagnostics.
///
/// Includes capability, amount units and settlement state; NEVER includes
/// `signature_ref` or `payload_ref`. Deep-Copy compatible: the returned
/// string shares no secret-classified data with the record.
pub fn redact_for_diagnostics(record: &IntentRecord) -> String {
    format!(
        "intent={} capability={} amount_units={} settlement={:?}",
        record.intent_ref, record.capability, record.amount_units, record.settlement
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(payload: &str) -> IntentRecord {
        IntentRecord::new("intent-1", "fetch+render", 1_000, "sig:abc", payload, 42)
    }

    #[test]
    fn ledger_is_append_only_and_records_stay_intact() {
        let mut ledger = Ledger::default();
        let first = record("payload-a");
        let original = first.clone();
        ledger.record_intent(first);
        ledger.record_intent(record("payload-b"));

        assert_eq!(ledger.intents.len(), 2);
        // First record unchanged after the second insert.
        assert_eq!(ledger.intents[0], original);
        // Appends only: no removal exists, and prior entries keep their refs.
        assert_eq!(ledger.intents[0].payload_ref, "payload-a");
        assert_eq!(ledger.intents[1].payload_ref, "payload-b");
    }

    #[test]
    fn verified_receipt_settles() {
        let r = record("payload-a");
        assert_eq!(
            reconcile(&r, ReceiptStatus::Verified),
            Reconciliation::Settled
        );
    }

    #[test]
    fn lost_receipt_needs_investigation_not_failure() {
        // Directive rule: timeout must not mean failed payment.
        let r = record("payload-a");
        assert_eq!(
            reconcile(&r, ReceiptStatus::Lost),
            Reconciliation::NeedsInvestigation
        );
        // And a lost receipt blocks automatic duplicate authorisation.
        let mut ledger = Ledger::default();
        ledger.record_intent(r);
        ledger.record_outcome("intent-1", Reconciliation::NeedsInvestigation);
        assert_ne!(
            ledger.latest_outcome("intent-1").unwrap(),
            Reconciliation::Settled
        );
        assert_eq!(audit(&ledger).len(), 1); // still an open obligation
    }

    #[test]
    fn duplicate_suspected_blocks_second_charge() {
        let r = record("payload-a");
        assert_eq!(
            reconcile(&r, ReceiptStatus::DuplicateSuspected),
            Reconciliation::DuplicateBlocked
        );
    }

    #[test]
    fn idempotency_refuses_reused_payload_ref() {
        let mut ledger = Ledger::default();
        ledger.record_intent(record("payload-a"));
        assert!(!can_authorise(&ledger, "payload-a"));
    }

    #[test]
    fn fresh_payload_ref_is_authorisable() {
        let mut ledger = Ledger::default();
        ledger.record_intent(record("payload-a"));
        assert!(can_authorise(&ledger, "payload-fresh"));
    }

    #[test]
    fn audit_lists_only_open_obligations() {
        let mut ledger = Ledger::default();
        ledger.record_intent(record("payload-a"));
        ledger.record_intent({
            let mut r = record("payload-b");
            r.intent_ref = "intent-2".to_string();
            r
        });
        ledger.record_intent({
            let mut r = record("payload-c");
            r.intent_ref = "intent-3".to_string();
            r.settlement = SettlementState::DuplicateBlocked;
            r
        });
        ledger.record_outcome("intent-1", Reconciliation::Settled);
        // intent-2 pending, intent-3 blocked by settlement state.
        let open = audit(&ledger);
        assert_eq!(open.len(), 2);
        assert!(open.iter().all(|r| r.intent_ref != "intent-1"));
    }

    #[test]
    fn redaction_excludes_secret_refs_but_includes_amount() {
        let r = record("payload-a");
        let line = redact_for_diagnostics(&r);
        assert!(!line.contains("sig:abc"));
        assert!(!line.contains("payload-a"));
        assert!(line.contains("fetch+render"));
        assert!(line.contains("1000"));
        assert!(line.contains("Pending"));
        assert_eq!(line.lines().count(), 1); // one-line summary
    }

    #[test]
    fn receipt_round_trips_refs() {
        let r = record("payload-a");
        let receipt = Receipt {
            intent_ref: r.intent_ref.clone(),
            payload_ref: r.payload_ref.clone(),
            settled_at_ns: 99,
        };
        assert_eq!(receipt.intent_ref, "intent-1");
        assert_eq!(receipt.payload_ref, "payload-a");
        assert_eq!(receipt.settled_at_ns, 99);
    }

    #[test]
    fn contract_constant() {
        assert_eq!(SETTLEMENT_CONTRACT, "GY-SETTLEMENT/1");
    }
}
