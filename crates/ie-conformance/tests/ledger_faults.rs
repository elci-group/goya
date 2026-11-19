//! WP-009 seed — ledger fault injection (assurance, S6).
//!
//! Cancellation races modeled sequentially against ie_cpu::ledger::Ledger:
//! typed-error contracts, queue-slot accounting, deadline boundary, and the
//! no-false-exactly-once rule (consumed results report unknown outcome).

use ie_contract::errors::TypedError;
use ie_contract::fixture::GoldenFixture;
use ie_contract::messages::{Request, Status};
use ie_cpu::ledger::Ledger;

fn digest(b: u8) -> [u8; 32] {
    [b; 32]
}

fn request(id: u64, deadline: u64) -> Request {
    Request {
        request_id: id,
        model_sha256: digest(9),
        inputs: vec![
            GoldenFixture::tensor(&[1.0, 2.0], &[2]),
            GoldenFixture::tensor(&[3.0, 4.0], &[2]),
        ],
        deadline_ms: deadline,
        lease_id: 1,
        policy_digest: [0; 32],
        requested_outputs: vec![],
    }
}

fn ledger() -> Ledger {
    let mut l = Ledger::new(4, 8);
    l.register_model(digest(9));
    l
}

/// Cancel-after-completion: cancellation must NOT destroy a retained
/// terminal result and must NOT report Cancelled for finished work.
#[test]
fn cancel_after_completion_preserves_result() {
    let mut l = ledger();
    l.submit(request(1, 100)).unwrap();
    l.run_one(0);
    // Race modeled sequentially: completion won.
    let st = l.cancel(1).unwrap();
    assert_eq!(st, Status::Pending, "finished entry must not report Cancelled");
    let (status, out) = l.poll(1).unwrap();
    assert_eq!(status, Status::Completed);
    assert_eq!(out.len(), 1);
}

/// Cancel-unknown-id: typed rejection, not a panic, not silent success.
#[test]
fn cancel_unknown_id_is_typed_error() {
    let mut l = ledger();
    assert!(matches!(l.cancel(999), Err(TypedError::InvalidTensor { .. })));
}

/// Poll-twice: first poll consumes the terminal result; the second must
/// report an unknown outcome (PEER_LOST), never replay the result — the
/// no-false-exactly-once rule.
#[test]
fn poll_twice_second_reports_unknown_outcome() {
    let mut l = ledger();
    l.submit(request(1, 100)).unwrap();
    l.run_one(0);
    let (s1, o1) = l.poll(1).unwrap();
    assert_eq!(s1, Status::Completed);
    assert_eq!(o1.len(), 1);
    let e2 = l.poll(1).unwrap_err();
    assert_eq!(e2.name(), "PEER_LOST");
    // And a third poll keeps reporting unknown, never the old result.
    assert_eq!(l.poll(1).unwrap_err().name(), "PEER_LOST");
}

/// Poll-unknown-id: typed rejection distinct from PEER_LOST-consumed.
#[test]
fn poll_unknown_id_is_typed_error() {
    let mut l = ledger();
    assert!(matches!(l.poll(31337), Err(TypedError::InvalidTensor { .. })));
}

/// Submit-after-saturation then recovery: quota is respected; cancelling
/// one queued request releases the slot for the previously rejected id;
/// and the rejected id is NOT silently admitted later (no zombie state).
#[test]
fn saturate_then_recover_via_cancel() {
    let mut l = Ledger::new(2, 8);
    l.register_model(digest(9));
    l.submit(request(1, 100)).unwrap();
    l.submit(request(2, 100)).unwrap();
    assert_eq!(
        l.submit(request(3, 100)).unwrap_err().name(),
        "RESOURCE_EXHAUSTED"
    );
    assert_eq!(l.queue_depth(), 2);
    l.cancel(2).unwrap();
    assert_eq!(l.queue_depth(), 1);
    assert_eq!(l.submit(request(3, 100)), Ok(Status::Pending));
    assert_eq!(l.queue_depth(), 2);
    // Duplicate of the cancelled id may be resubmitted fresh (id 2 no
    // longer live), but a duplicate of a live id never double-queues.
    assert_eq!(l.submit(request(1, 100)), Ok(Status::Pending));
    assert_eq!(l.queue_depth(), 2);
}

/// Recovery via completion: after run_one the slot frees and saturation
/// clears; the completed result stays pollable exactly once.
#[test]
fn saturate_then_recover_via_completion() {
    let mut l = Ledger::new(1, 8);
    l.register_model(digest(9));
    l.submit(request(1, 100)).unwrap();
    assert_eq!(
        l.submit(request(2, 100)).unwrap_err().name(),
        "RESOURCE_EXHAUSTED"
    );
    l.run_one(0);
    assert_eq!(l.submit(request(2, 100)), Ok(Status::Pending));
    l.run_one(0);
    let (s, _) = l.poll(2).unwrap();
    assert_eq!(s, Status::Completed);
    assert_eq!(l.poll(2).unwrap_err().name(), "PEER_LOST");
}

/// Deadline boundary: elapsed exactly == deadline is already expired
/// (remaining-duration semantics: zero remaining budget).
#[test]
fn deadline_boundary_exactly_elapsed_is_expired() {
    let mut l = ledger();
    l.submit(request(1, 50)).unwrap();
    l.run_one(50); // elapsed == deadline
    let (status, _) = l.poll(1).unwrap();
    assert_eq!(status, Status::Failed(TypedError::DeadlineExceeded));
}

/// One tick of headroom under the deadline still executes successfully.
#[test]
fn deadline_one_under_boundary_executes() {
    let mut l = ledger();
    l.submit(request(1, 50)).unwrap();
    l.run_one(49);
    let (status, _) = l.poll(1).unwrap();
    assert_eq!(status, Status::Completed);
}

/// Cancellation of an unknown id after the queue drained (race where the
/// request already completed and was consumed) is a typed error — the
/// caller learns nothing about the vanished result either way.
#[test]
fn cancel_after_result_consumed_is_typed_unknown() {
    let mut l = ledger();
    l.submit(request(1, 100)).unwrap();
    l.run_one(0);
    let _ = l.poll(1).unwrap();
    assert!(matches!(l.cancel(1), Err(TypedError::InvalidTensor { .. })));
    assert!(matches!(l.poll(1), Err(TypedError::PeerLost)));
}

/// Retention bound: finished entries beyond retention_capacity are evicted
/// and their ids then report PEER_LOST (unknown outcome), never a replay.
#[test]
fn retention_eviction_reports_unknown_not_replay() {
    let mut l = Ledger::new(4, 1);
    l.register_model(digest(9));
    l.submit(request(1, 100)).unwrap();
    l.run_one(0);
    l.submit(request(2, 100)).unwrap();
    l.run_one(0);
    // Retention 1: the first finished entry was evicted when the second
    // finished. Its id must report unknown, not Completed.
    let e = l.poll(1).unwrap_err();
    assert_eq!(e.name(), "PEER_LOST");
    let (s, _) = l.poll(2).unwrap();
    assert_eq!(s, Status::Completed);
}

/// Zero deadline is rejected at admission (remaining duration <= 0).
#[test]
fn zero_deadline_rejected_at_admission() {
    let mut l = ledger();
    assert!(matches!(l.submit(request(1, 0)), Err(TypedError::DeadlineExceeded)));
    assert_eq!(l.queue_depth(), 0);
}
