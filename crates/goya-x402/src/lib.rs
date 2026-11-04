//! goya-x402 — deterministic policy/validation layer for native x402 (GY-X402/1).
//!
//! std-only. No network, no crypto: this crate validates challenges against
//! allowlists, produces deterministic payment payloads, and defines the
//! payment lifecycle state machine with reconciliation-safe failure rules.

/// Wire/contract identifier for this directive.
pub const X402_CONTRACT: &str = "GY-X402/1";

/// Exact-integer money amount. Never floats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amount {
    pub units: u128,
    pub asset: String,
    pub network: String,
}

impl Amount {
    /// Construct an exact amount from integer units plus asset/network ids.
    pub fn parse(units: u128, asset: &str, network: &str) -> Self {
        Amount {
            units,
            asset: asset.to_string(),
            network: network.to_string(),
        }
    }
}

/// A payment challenge as presented by a resource server (x402 v2 headers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRequired {
    pub endpoint: String,
    pub amount: Amount,
    pub recipient: String,
    pub scheme: String,
}

/// Reasons a challenge may be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeError {
    /// The challenged asset is not on the allowlist.
    DisallowedAsset(String),
    /// The challenged recipient is not on the allowlist (an agent must never
    /// accept a substituted recipient).
    DisallowedRecipient(String),
}

impl core::fmt::Display for ChallengeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ChallengeError::DisallowedAsset(a) => write!(f, "asset not allowlisted: {a}"),
            ChallengeError::DisallowedRecipient(r) => {
                write!(f, "recipient not allowlisted (substitution refused): {r}")
            }
        }
    }
}

impl std::error::Error for ChallengeError {}

/// Validate a challenge against allowlisted assets and recipients.
///
/// The recipient check is the anti-substitution guard: a malicious endpoint
/// may rewrite `payTo`; an agent must refuse to pay anyone off-list.
pub fn validate_challenge(
    ch: &PaymentRequired,
    allowlisted_assets: &[&str],
    allowlisted_recipients: &[&str],
) -> Result<(), ChallengeError> {
    if !allowlisted_assets.iter().any(|a| *a == ch.amount.asset) {
        return Err(ChallengeError::DisallowedAsset(ch.amount.asset.clone()));
    }
    if !allowlisted_recipients.iter().any(|r| *r == ch.recipient) {
        return Err(ChallengeError::DisallowedRecipient(ch.recipient.clone()));
    }
    Ok(())
}

/// Deterministic payload produced when retrying an endpoint with payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentPayload {
    pub challenge_hash: u64,
    pub signature_ref: String,
}

/// FNV-1a (64-bit) over the canonical challenge field serialization.
/// Implemented locally; no crypto dependency — this is a bookkeeping digest,
/// not an authenticity primitive.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(PRIME);
    }
    h
}

fn push_field(buf: &mut Vec<u8>, field: &str) {
    // Length-prefixed, NUL-terminated framing so field boundaries are
    // unambiguous regardless of content.
    buf.extend_from_slice(&(field.len() as u64).to_le_bytes());
    buf.extend_from_slice(field.as_bytes());
    buf.push(0);
}

/// Hash the challenge fields deterministically (FNV-1a, local impl).
pub fn challenge_digest(ch: &PaymentRequired) -> u64 {
    let mut buf = Vec::new();
    push_field(&mut buf, &ch.endpoint);
    push_field(&mut buf, &ch.amount.asset);
    push_field(&mut buf, &ch.amount.network);
    buf.extend_from_slice(&ch.amount.units.to_le_bytes());
    push_field(&mut buf, &ch.recipient);
    push_field(&mut buf, &ch.scheme);
    fnv1a64(&buf)
}

/// Build the deterministic retry payload for a validated challenge.
pub fn retry_with_payment(ch: &PaymentRequired) -> PaymentPayload {
    PaymentPayload {
        challenge_hash: challenge_digest(ch),
        signature_ref: format!("x402:v2:{}:sig", ch.scheme),
    }
}

/// Payment lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentState {
    Ready,
    PaymentRequired,
    Authorised,
    Processing,
    Complete,
    Failed,
    /// Outcome unknown (timeout / lost receipt). Reconciliation is required
    /// before any duplicate charge may be issued.
    Ambiguous,
}

/// Lifecycle events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentEvent {
    ChallengeReceived,
    Approved,
    Signed,
    ReceiptVerified,
    Timeout,
    ReceiptLost,
}

/// Transition rule. Key safety property: `Timeout` and `ReceiptLost` map to
/// `Ambiguous`, never `Failed` — a Failed classification would license an
/// automatic retry and risk a duplicate charge; Ambiguous forces
/// reconciliation first.
pub fn transition(state: PaymentState, ev: PaymentEvent) -> PaymentState {
    use PaymentEvent as E;
    use PaymentState as S;
    match (state, ev) {
        (S::Ready, E::ChallengeReceived) => S::PaymentRequired,
        (S::PaymentRequired, E::Approved) => S::Authorised,
        (S::Authorised, E::Signed) => S::Processing,
        (S::Processing, E::ReceiptVerified) => S::Complete,
        // Reconciliation-required outcomes: never collapse to Failed.
        (S::Processing, E::Timeout) | (S::Processing, E::ReceiptLost) => S::Ambiguous,
        (S::Ambiguous, E::ReceiptVerified) => S::Complete,
        // Everything else is not a valid transition; hold state.
        _ => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge() -> PaymentRequired {
        PaymentRequired {
            endpoint: "https://api.example.test/data".to_string(),
            amount: Amount::parse(1_000, "usdc", "base"),
            recipient: "0xaaa".to_string(),
            scheme: "exact".to_string(),
        }
    }

    const ASSETS: &[&str] = &["usdc", "eurc"];
    const RECIPIENTS: &[&str] = &["0xaaa", "0xbbb"];

    #[test]
    fn accepts_valid_challenge() {
        assert_eq!(validate_challenge(&challenge(), ASSETS, RECIPIENTS), Ok(()));
    }

    #[test]
    fn rejects_disallowed_asset() {
        let mut ch = challenge();
        ch.amount = Amount::parse(1_000, "scamcoin", "base");
        assert_eq!(
            validate_challenge(&ch, ASSETS, RECIPIENTS),
            Err(ChallengeError::DisallowedAsset("scamcoin".to_string()))
        );
    }

    #[test]
    fn rejects_substituted_recipient() {
        let mut ch = challenge();
        ch.recipient = "0xevil".to_string();
        assert_eq!(
            validate_challenge(&ch, ASSETS, RECIPIENTS),
            Err(ChallengeError::DisallowedRecipient("0xevil".to_string()))
        );
    }

    #[test]
    fn timeout_maps_to_ambiguous_never_failed() {
        assert_eq!(
            transition(PaymentState::Processing, PaymentEvent::Timeout),
            PaymentState::Ambiguous
        );
        // And Ambiguous is reachable from Failed? No: Failed is terminal hold.
        assert_eq!(
            transition(PaymentState::Failed, PaymentEvent::Timeout),
            PaymentState::Failed
        );
    }

    #[test]
    fn receipt_lost_is_ambiguous() {
        assert_eq!(
            transition(PaymentState::Processing, PaymentEvent::ReceiptLost),
            PaymentState::Ambiguous
        );
    }

    #[test]
    fn ambiguous_can_reconcile_to_complete_via_receipt() {
        let s = transition(PaymentState::Processing, PaymentEvent::Timeout);
        assert_eq!(s, PaymentState::Ambiguous);
        assert_eq!(
            transition(s, PaymentEvent::ReceiptVerified),
            PaymentState::Complete
        );
    }

    #[test]
    fn full_happy_path_lifecycle() {
        let mut s = PaymentState::Ready;
        s = transition(s, PaymentEvent::ChallengeReceived);
        assert_eq!(s, PaymentState::PaymentRequired);
        s = transition(s, PaymentEvent::Approved);
        assert_eq!(s, PaymentState::Authorised);
        s = transition(s, PaymentEvent::Signed);
        assert_eq!(s, PaymentState::Processing);
        s = transition(s, PaymentEvent::ReceiptVerified);
        assert_eq!(s, PaymentState::Complete);
    }

    #[test]
    fn retry_payload_is_deterministic() {
        let p1 = retry_with_payment(&challenge());
        let p2 = retry_with_payment(&challenge());
        assert_eq!(p1, p2);
        assert_ne!(p1.challenge_hash, 0);
        // Distinct recipient must change the hash (substitution detectable).
        let mut ch = challenge();
        ch.recipient = "0xbbb".to_string();
        assert_ne!(retry_with_payment(&ch).challenge_hash, p1.challenge_hash);
    }

    #[test]
    fn invalid_transitions_hold_state() {
        assert_eq!(
            transition(PaymentState::Ready, PaymentEvent::Signed),
            PaymentState::Ready
        );
        assert_eq!(
            transition(PaymentState::Complete, PaymentEvent::ChallengeReceived),
            PaymentState::Complete
        );
    }
}
