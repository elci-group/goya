//! Resource ledger and cancellation (WP-004, contract v0.1 semantics from
//! IE-A01/IE-X01).
//!
//! - Bounded queue: admission budget is reserved and released atomically on
//!   cancellation or terminal completion; saturation is `RESOURCE_EXHAUSTED`.
//! - Request IDs deduplicate within bounded retention; duplicate submission
//!   of a live request returns its current status rather than double-queueing.
//! - Deadlines are remaining durations enforced by the backend (never peer
//!   wall-clock); expiry is `DEADLINE_EXCEEDED`.
//! - Cancellation is asynchronous and terminal: `CANCELLED`, and the queue
//!   slot is released immediately.
//! - Ambiguous completion (result consumed/retention lapsed) reports an
//!   unknown outcome rather than claiming exactly-once.
//!
//! Single-threaded core; callers own any concurrency wrapper, matching the
//! reference-backend scope of WP-003/004.

use crate::execute;
use ie_contract::errors::TypedError;
use ie_contract::messages::{Request, Status};
use ie_contract::tensors::TensorData;

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Queued(Request),
    Finished(Status, Vec<TensorData>),
}

/// Bounded request ledger for the CPU reference backend.
pub struct Ledger {
    queue_limit: usize,
    /// Requests currently admitted (queued or finished-but-retained).
    entries: Vec<(u64, Entry)>,
    /// Bounded retention ring for terminal results already consumed.
    consumed: Vec<u64>,
    retention_capacity: usize,
    /// Registered model digests; anything else is MODEL_MISSING (PR-08).
    models: Vec<[u8; 32]>,
}

impl Ledger {
    /// Create a ledger with a hard queue limit and bounded result retention.
    pub fn new(queue_limit: usize, retention_capacity: usize) -> Ledger {
        Ledger {
            queue_limit,
            entries: Vec::new(),
            consumed: Vec::new(),
            retention_capacity,
            models: Vec::new(),
        }
    }

    /// Register a model digest the backend can execute (version pinning).
    pub fn register_model(&mut self, digest: [u8; 32]) {
        if !self.models.contains(&digest) {
            self.models.push(digest);
        }
    }

    /// Admit a request. Reserves a queue slot atomically on success.
    /// Typed rejections: RESOURCE_EXHAUSTED (queue full), MODEL_MISSING
    /// (unregistered digest), INVALID_TENSOR (payload/descriptor mismatch,
    /// checked before admission), DEADLINE_EXCEEDED (zero deadline).
    pub fn submit(&mut self, req: Request) -> Result<Status, TypedError> {
        if req.deadline_ms == 0 {
            return Err(TypedError::DeadlineExceeded);
        }
        if let Some((_, e)) = self.entries.iter().find(|(id, _)| *id == req.request_id) {
            // Dedup within retention: never double-queue a live request.
            return Ok(match e {
                Entry::Queued(_) => Status::Pending,
                Entry::Finished(s, _) => s.clone(),
            });
        }
        for t in &req.inputs {
            t.desc.validate(t.data.len())?;
        }
        if !self.models.contains(&req.model_sha256) {
            return Err(TypedError::ModelMissing { digest: hex(&req.model_sha256) });
        }
        if self.queue_depth() >= self.queue_limit {
            return Err(TypedError::ResourceExhausted { reason: "queue full".into() });
        }
        self.entries.push((req.request_id, Entry::Queued(req)));
        Ok(Status::Pending)
    }

    /// Execute the oldest queued request whose remaining deadline is still
    /// positive. Returns the request id worked on, if any.
    /// Cancellation (via `cancel`) removes it before this gets to it.
    pub fn run_one(&mut self, now_elapsed_ms: u64) -> Option<u64> {
        let idx = self.entries.iter().position(|(_, e)| matches!(e, Entry::Queued(_)))?;
        let (id, entry) = self.entries.remove(idx);
        let Entry::Queued(req) = entry else { unreachable!() };
        if req.deadline_ms <= now_elapsed_ms {
            self.finish(id, Status::Failed(TypedError::DeadlineExceeded), Vec::new());
            return Some(id);
        }
        // Dispatch by rank: rank-1 -> add, rank>=2 -> matmul (first op of
        // the golden subset that fits the input shape).
        let rank = req.inputs.first().map(|t| t.desc.dims.len()).unwrap_or(0);
        let op = if rank <= 1 { "add" } else { "matmul" };
        match execute(op, &req.inputs) {
            Ok(out) => {
                self.finish(id, Status::Completed, vec![out]);
            }
            Err(e) => self.finish(id, Status::Failed(e), Vec::new()),
        }
        Some(id)
    }

    fn finish(&mut self, id: u64, status: Status, outputs: Vec<TensorData>) {
        // Queue slot released at terminal completion; finished entries are
        // retained for polling only, not counted against admission budget.
        self.entries.push((id, Entry::Finished(status, outputs)));
        self.trim_retention();
    }

    fn trim_retention(&mut self) {
        let finished: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, (_, e))| matches!(e, Entry::Finished(_, _)))
            .map(|(i, _)| i)
            .collect();
        let excess = finished.len().saturating_sub(self.retention_capacity);
        for &i in finished.iter().take(excess) {
            let (id, _) = self.entries.remove(i);
            self.consumed.push(id);
            if self.consumed.len() > self.retention_capacity * 2 {
                self.consumed.remove(0);
            }
        }
    }

    /// Poll a request. Terminal states are returned once; after consumption
    /// (or retention lapse) the outcome is reported as unknown
    /// (`PEER_LOST`), never replayed as exactly-once.
    pub fn poll(&mut self, request_id: u64) -> Result<(Status, Vec<TensorData>), TypedError> {
        if let Some(pos) = self.entries.iter().position(|(id, _)| *id == request_id) {
            let (_, e) = self.entries.remove(pos);
            match e {
                Entry::Queued(_) => {
                    self.entries.push((request_id, e));
                    Ok((Status::Pending, Vec::new()))
                }
                Entry::Finished(s, out) => {
                    self.consumed.push(request_id);
                    Ok((s, out))
                }
            }
        } else if self.consumed.contains(&request_id) {
            Err(TypedError::PeerLost)
        } else {
            Err(TypedError::InvalidTensor {
                reason: format!("unknown request id {request_id}"),
            })
        }
    }

    /// Asynchronous cancellation (PR-03). Queued requests are removed and
    /// their slot released atomically; already-finished results survive
    /// until polled (cancellation does not destroy evidence).
    pub fn cancel(&mut self, request_id: u64) -> Result<Status, TypedError> {
        if let Some(pos) = self.entries.iter().position(|(id, _)| *id == request_id) {
            let (_, e) = self.entries.remove(pos);
            match e {
                Entry::Queued(_) => Ok(Status::Failed(TypedError::Cancelled)),
                Entry::Finished(s, out) => {
                    self.entries.push((request_id, Entry::Finished(s, out)));
                    Ok(Status::Pending)
                }
            }
        } else {
            Err(TypedError::InvalidTensor { reason: format!("unknown request id {request_id}") })
        }
    }

    /// Number of queue slots currently reserved (admission budget view).
    pub fn queue_depth(&self) -> usize {
        self.entries.iter().filter(|(_, e)| matches!(e, Entry::Queued(_))).count()
    }
}

fn hex(d: &[u8; 32]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_contract::fixture::GoldenFixture;

    fn digest(b: u8) -> [u8; 32] {
        [b; 32]
    }

    fn request(id: u64, model: u8, deadline: u64) -> Request {
        Request {
            request_id: id,
            model_sha256: digest(model),
            inputs: vec![
                GoldenFixture::tensor(&[1.0, 2.0], &[2]),
                GoldenFixture::tensor(&[3.0, 4.0], &[2]),
            ],
            deadline_ms: deadline,
            lease_id: 1,
            policy_digest: [0u8; 32],
            requested_outputs: vec!["out".into()],
        }
    }

    /// T04 seed — quota saturation and recovery (PR-03/06).
    #[test]
    fn queue_saturation_and_recovery() {
        let mut l = Ledger::new(2, 8);
        l.register_model(digest(9));
        assert_eq!(l.submit(request(1, 9, 100)), Ok(Status::Pending));
        assert_eq!(l.submit(request(2, 9, 100)), Ok(Status::Pending));
        assert_eq!(
            l.submit(request(3, 9, 100)),
            Err(TypedError::ResourceExhausted { reason: "queue full".into() })
        );
        assert_eq!(l.queue_depth(), 2);
        // Recovery: completion releases the slot.
        l.run_one(0);
        assert_eq!(l.queue_depth(), 1);
        assert_eq!(l.submit(request(3, 9, 100)), Ok(Status::Pending));
    }

    /// T05 seed — cancellation and deadline (PR-03).
    #[test]
    fn cancellation_releases_slot_and_is_terminal() {
        let mut l = Ledger::new(1, 8);
        l.register_model(digest(9));
        l.submit(request(1, 9, 100)).unwrap();
        assert_eq!(l.cancel(1), Ok(Status::Failed(TypedError::Cancelled)));
        assert_eq!(l.queue_depth(), 0);
        // Slot released: a new request is admitted.
        assert_eq!(l.submit(request(2, 9, 100)), Ok(Status::Pending));
    }

    #[test]
    fn expired_deadline_fails_typed() {
        let mut l = Ledger::new(1, 8);
        l.register_model(digest(9));
        l.submit(request(1, 9, 50)).unwrap();
        l.run_one(100); // elapsed past deadline
        let (status, _) = l.poll(1).unwrap();
        assert_eq!(status, Status::Failed(TypedError::DeadlineExceeded));
    }

    #[test]
    fn unregistered_model_is_model_missing() {
        let mut l = Ledger::new(1, 8);
        l.register_model(digest(1));
        assert!(matches!(
            l.submit(request(1, 2, 100)),
            Err(TypedError::ModelMissing { .. })
        ));
    }

    #[test]
    fn dedup_does_not_double_queue() {
        let mut l = Ledger::new(2, 8);
        l.register_model(digest(9));
        l.submit(request(1, 9, 100)).unwrap();
        assert_eq!(l.submit(request(1, 9, 100)), Ok(Status::Pending));
        assert_eq!(l.queue_depth(), 1);
    }

    /// Ambiguous completion: consumed results are not replayed.
    #[test]
    fn consumed_result_reports_unknown_outcome() {
        let mut l = Ledger::new(1, 8);
        l.register_model(digest(9));
        l.submit(request(1, 9, 100)).unwrap();
        l.run_one(0);
        let (status, out) = l.poll(1).unwrap();
        assert_eq!(status, Status::Completed);
        assert_eq!(out.len(), 1);
        assert!(matches!(l.poll(1), Err(TypedError::PeerLost)));
    }

    #[test]
    fn add_then_matmul_dispatch() {
        let mut l = Ledger::new(2, 8);
        l.register_model(digest(9));
        // add: inputs same shape
        l.submit(request(1, 9, 100)).unwrap();
        // matmul: rank-2 inputs
        let mm = Request {
            request_id: 2,
            model_sha256: digest(9),
            inputs: vec![
                GoldenFixture::tensor(&[1.0, 2.0, 3.0, 4.0], &[2, 2]),
                GoldenFixture::tensor(&[5.0, 6.0, 7.0, 8.0], &[2, 2]),
            ],
            deadline_ms: 100,
            lease_id: 1,
            policy_digest: [0u8; 32],
            requested_outputs: vec![],
        };
        l.submit(mm).unwrap();
        l.run_one(0);
        l.run_one(0);
        let (_, out) = l.poll(2).unwrap();
        let got: Vec<f32> =
            out[0].data.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(got, vec![19.0, 22.0, 43.0, 50.0]);
    }
}
