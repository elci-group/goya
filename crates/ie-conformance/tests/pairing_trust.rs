//! WP-009 seed — pairing trust injection (assurance, S6).
//!
//! Adversarial tests against ie_loopback::SharedProvider: lease replay
//! after revocation, expired-lease reuse, cross-client lease theft,
//! pairing-code enumeration and quota boundaries.

use ie_contract::errors::TypedError;
use ie_contract::fixture::GoldenFixture;
use ie_contract::messages::{Request, Status};
use ie_loopback::pairing::{Quota, SharedProvider};
use ie_loopback::Lease;

const MODEL: [u8; 32] = [7; 32];

fn request(id: u64, deadline: u64) -> Request {
    Request {
        request_id: id,
        model_sha256: MODEL,
        inputs: vec![
            GoldenFixture::tensor(&[1.0, 2.0], &[2]),
            GoldenFixture::tensor(&[3.0, 4.0], &[2]),
        ],
        deadline_ms: deadline,
        lease_id: 0,
        policy_digest: [0; 32],
        requested_outputs: vec![],
    }
}

fn provider() -> SharedProvider {
    let mut p = SharedProvider::new("ie://peer/esp0/0", 16, 16);
    p.inner_mut().register_model(MODEL);
    p
}

/// Lease replay after provider revoke: the old-epoch lease must be
/// UNTRUSTED_PEER even if the client is still paired.
#[test]
fn lease_replay_after_provider_revoke_is_untrusted() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(4, 5_000));
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 60_000).unwrap();
    p.inner_mut().revoke();
    let mut req = request(1, 1_000);
    req.lease_id = lease.lease_id;
    let err = p.submit("phone", &lease, req).unwrap_err();
    assert_eq!(err.name(), "UNTRUSTED_PEER");
    // And revoke_client afterwards keeps new work denied.
    let fresh = p.acquire_lease("phone", 60_000).unwrap();
    let mut req2 = request(2, 1_000);
    req2.lease_id = fresh.lease_id;
    p.revoke_client("phone");
    assert_eq!(p.submit("phone", &fresh, req2).unwrap_err().name(), "POLICY_DENIED");
}

/// Lease replay after client-level revocation: the lease was released, so
/// submission must be denied (POLICY_DENIED family), never executed.
#[test]
fn lease_after_client_revoke_is_denied() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(4, 5_000));
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 60_000).unwrap();
    p.revoke_client("phone");
    // Even if the client somehow re-pairs, the released lease must be dead.
    p.pair("phone", 424_242).unwrap();
    let mut req = request(1, 1_000);
    req.lease_id = lease.lease_id;
    let err = p.submit("phone", &lease, req).unwrap_err();
    assert!(
        matches!(err, TypedError::PolicyDenied { .. } | TypedError::UntrustedPeer),
        "released lease must be denied, got {err}"
    );
}

/// Expired-lease reuse: a lease past its TTL must be POLICY_DENIED; the
/// boundary (now == expiry) is already expired — zero remaining budget.
#[test]
fn expired_lease_reuse_denied_at_and_after_expiry() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(4, 5_000));
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 100).unwrap();
    p.inner_mut().advance_clock(100); // exactly at expiry boundary
    let mut req = request(1, 1_000);
    req.lease_id = lease.lease_id;
    let err = p.submit("phone", &lease, req).unwrap_err();
    assert!(matches!(err, TypedError::PolicyDenied { .. }), "got {err}");
    // Still denied even further past expiry.
    p.inner_mut().advance_clock(1_000);
    let mut req2 = request(2, 1_000);
    req2.lease_id = lease.lease_id;
    assert!(matches!(
        p.submit("phone", &lease, req2),
        Err(TypedError::PolicyDenied { .. })
    ));
}

/// Within TTL the lease works (sanity guard for the boundary test above).
#[test]
fn lease_valid_one_tick_before_expiry() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(4, 5_000));
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 100).unwrap();
    p.inner_mut().advance_clock(99);
    let mut req = request(1, 1_000);
    req.lease_id = lease.lease_id;
    assert_eq!(p.submit("phone", &lease, req), Ok(Status::Pending));
}

/// Cross-client lease theft: laptop presenting phone's lease must be
/// rejected; and vice versa.
#[test]
fn cross_client_lease_theft_is_untrusted() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(4, 5_000));
    p.offer_pairing("laptop", 111_111, Quota::new(4, 5_000));
    p.pair("phone", 424_242).unwrap();
    p.pair("laptop", 111_111).unwrap();
    let phone = p.acquire_lease("phone", 60_000).unwrap();
    let laptop = p.acquire_lease("laptop", 60_000).unwrap();

    let mut r1 = request(1, 1_000);
    r1.lease_id = phone.lease_id;
    assert_eq!(p.submit("laptop", &phone, r1).unwrap_err().name(), "UNTRUSTED_PEER");

    let mut r2 = request(2, 1_000);
    r2.lease_id = laptop.lease_id;
    assert_eq!(p.submit("phone", &laptop, r2).unwrap_err().name(), "UNTRUSTED_PEER");

    // Stolen lease must not have been consumed: the legitimate owner still
    // submits successfully with it.
    assert_eq!(p.submit("phone", &phone, r1), Ok(Status::Pending));
}

/// Pairing-code enumeration: wrong codes and codes belonging to other
/// clients never admit; repeated guessing leaves state unchanged.
#[test]
fn pairing_code_enumeration_never_admits() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(1, 1_000));
    // Off-by-one and far guesses on the real client id.
    for guess in [424_241, 424_243, 0, u32::MAX, 111_111] {
        assert_eq!(
            p.pair("phone", guess).unwrap_err().name(),
            "UNTRUSTED_PEER",
            "guess {guess} must not pair"
        );
    }
    // The real code aimed at an unoffered client must not pair either.
    assert_eq!(p.pair("stranger", 424_242).unwrap_err().name(), "UNTRUSTED_PEER");
    // Enumeration attempts must not have granted a lease path.
    assert!(p.acquire_lease("phone", 1_000).is_err());
    assert!(p.acquire_lease("stranger", 1_000).is_err());
    // Correct code still works after failed guesses.
    p.pair("phone", 424_242).unwrap();
    assert!(p.acquire_lease("phone", 1_000).is_ok());
}

/// Quota boundary: exactly at cap is admitted; one over breaches with
/// RESOURCE_EXHAUSTED and does not disturb the admitted work.
#[test]
fn quota_boundary_exactly_at_cap_ok_one_over_breaches() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(2, 1_000));
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 60_000).unwrap();
    let mut r1 = request(1, 1_000);
    r1.lease_id = lease.lease_id;
    let mut r2 = request(2, 1_000);
    r2.lease_id = lease.lease_id;
    assert_eq!(p.submit("phone", &lease, r1), Ok(Status::Pending));
    assert_eq!(p.submit("phone", &lease, r2), Ok(Status::Pending));
    let mut r3 = request(3, 1_000);
    r3.lease_id = lease.lease_id;
    assert_eq!(
        p.submit("phone", &lease, r3).unwrap_err().name(),
        "RESOURCE_EXHAUSTED"
    );
    // Cancel one queued request frees the slot.
    p.cancel("phone", 1).unwrap();
    let mut r4 = request(4, 1_000);
    r4.lease_id = lease.lease_id;
    assert_eq!(p.submit("phone", &lease, r4), Ok(Status::Pending));
}

/// Deadline-quota boundary: a deadline exactly at the cap is accepted,
/// one millisecond over is POLICY_DENIED.
#[test]
fn deadline_quota_boundary() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(4, 1_000));
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 60_000).unwrap();
    let mut at_cap = request(1, 1_000);
    at_cap.lease_id = lease.lease_id;
    assert_eq!(p.submit("phone", &lease, at_cap), Ok(Status::Pending));
    let mut over = request(2, 1_001);
    over.lease_id = lease.lease_id;
    assert_eq!(p.submit("phone", &lease, over).unwrap_err().name(), "POLICY_DENIED");
}

/// A revoked client cannot re-pair itself silently through submission
/// paths; pairing state transitions only via pair()/revoke_client().
#[test]
fn revoked_client_new_lease_denied_then_recoverable_by_owner() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(2, 5_000));
    p.pair("phone", 424_242).unwrap();
    p.revoke_client("phone");
    assert!(p.acquire_lease("phone", 1_000).is_err());
    // Owner re-consent (fresh pair) restores service on a fresh lease.
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 60_000).unwrap();
    let mut req = request(9, 1_000);
    req.lease_id = lease.lease_id;
    assert_eq!(p.submit("phone", &lease, req), Ok(Status::Pending));
}

/// Revocation must not strand queued work so it executes after consent
/// ends: revoke while a request is queued, then drive the provider —
/// that client's request must not complete.
#[test]
fn queued_work_never_executes_after_revoke() {
    let mut p = provider();
    p.offer_pairing("phone", 424_242, Quota::new(4, 5_000));
    p.pair("phone", 424_242).unwrap();
    let lease = p.acquire_lease("phone", 60_000).unwrap();
    let mut req = request(1, 1_000);
    req.lease_id = lease.lease_id;
    assert_eq!(p.submit("phone", &lease, req), Ok(Status::Pending));
    p.revoke_client("phone");
    while p.run_pending().is_some() {}
    // The result must not be retrievable as a Completed output: either the
    // id is unknown or reports unknown outcome — never a replayed result.
    match p.poll(1) {
        Err(e) => assert!(
            matches!(e, TypedError::InvalidTensor { .. } | TypedError::PeerLost),
            "unexpected {e}"
        ),
        Ok((st, _)) => panic!("revoked client's queued work completed: {st:?}"),
    }
}
