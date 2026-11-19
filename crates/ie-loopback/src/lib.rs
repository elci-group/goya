//! ie-loopback — in-process loopback transport (WP-005 seed, S3 España).
//!
//! A mock-transport provider implementing the contract v0.1 message flow
//! (Describe → AcquireLease → Submit → Poll/Cancel) over canonical framed
//! messages, with no network and no TLS. TLS transport is a later packet;
//! this crate exists so client code can be written against the frozen
//! codec before any socket exists.
//!
//! Trust rules honoured (IE-A01): discovery metadata is untrusted —
//! capability descriptors are only valid after an explicit authenticated
//! step, which loopback models as `authenticate()`. No lease, no service:
//! Submit with an unknown/stale lease is POLICY_DENIED. Epochs are
//! monotonic; results carry the provider epoch so stale completions can
//! be discarded by the caller.

pub mod pairing;
pub use pairing::SharedProvider;

use ie_contract::codec;
use ie_contract::errors::TypedError;
use ie_contract::framing::{decode_frame, encode_frame};
use ie_contract::messages::{CapabilityDescriptor, Request, Status};
use ie_cpu::ledger::Ledger;
use ie_contract::PROTOCOL_VERSION;

/// An issued lease. Stable identity is the opaque lease id; the epoch pins
/// it to the provider generation, so a lease from a restarted provider is
/// stale by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lease {
    pub lease_id: u64,
    pub epoch: u64,
    pub expiry_ms: u64,
}

pub struct Provider {
    endpoint_id: String,
    epoch: u64,
    next_lease: u64,
    active_leases: Vec<Lease>,
    ledger: Ledger,
    /// Monotonic provider clock, advanced explicitly by the test/runtime.
    now_ms: u64,
}

impl Provider {
    pub fn new(endpoint_id: &str, queue_limit: usize, retention: usize) -> Provider {
        Provider {
            endpoint_id: endpoint_id.to_string(),
            epoch: 1,
            next_lease: 1,
            active_leases: Vec::new(),
            ledger: Ledger::new(queue_limit, retention),
            now_ms: 0,
        }
    }

    /// Register a model digest (version pinning; PR-08).
    pub fn register_model(&mut self, digest: [u8; 32]) {
        self.ledger.register_model(digest);
    }

    /// Capability descriptor. In loopback, calling this *is* the
    /// authenticated describe; a real transport signs these bytes.
    pub fn describe(&self) -> CapabilityDescriptor {
        CapabilityDescriptor {
            protocol_version: PROTOCOL_VERSION,
            endpoint_id: self.endpoint_id.clone(),
            epoch: self.epoch,
            supported_ops: ie_cpu::SUPPORTED_OPS.iter().map(|s| s.to_string()).collect(),
            max_tensor_bytes: ie_contract::tensors::MAX_TENSOR_BYTES,
            queue_limit: self.ledger.queue_depth() as u32, // depth, not limit
            expiry_ms: self
                .active_leases
                .iter()
                .map(|l| l.expiry_ms.saturating_sub(self.now_ms))
                .max()
                .unwrap_or(0),
        }
    }

    /// Advance the provider's monotonic clock (runtime/test driver only).
    pub fn advance_clock(&mut self, ms: u64) {
        self.now_ms += ms;
    }

    /// Issue a lease with the given TTL. Leases are per-client (the
    /// pairing layer binds them to identities); all remain valid until
    /// expiry or revocation.
    pub fn acquire_lease(&mut self, ttl_ms: u64) -> Result<Lease, TypedError> {
        if ttl_ms == 0 {
            return Err(TypedError::PolicyDenied { reason: "zero TTL".into() });
        }
        let lease = Lease {
            lease_id: self.next_lease,
            epoch: self.epoch,
            expiry_ms: self.now_ms + ttl_ms,
        };
        self.next_lease += 1;
        self.active_leases.push(lease);
        Ok(lease)
    }

    /// Revoke the provider identity entirely (credential compromise,
    /// IE-M01): epoch bump invalidates every outstanding lease.
    pub fn revoke(&mut self) {
        self.epoch += 1;
        self.active_leases.clear();
    }

    /// Drop one lease (e.g. released by its owner or revoked by pairing).
    pub fn release_lease(&mut self, lease_id: u64) {
        self.active_leases.retain(|l| l.lease_id != lease_id);
    }

    fn lease_valid(&self, lease: &Lease) -> Result<(), TypedError> {
        // A lease pinned to an older provider generation is stale by
        // construction — always UNTRUSTED_PEER, regardless of current state.
        if lease.epoch != self.epoch {
            return Err(TypedError::UntrustedPeer);
        }
        match self.active_leases.iter().find(|l| l.lease_id == lease.lease_id) {
            Some(l) if self.now_ms < l.expiry_ms => Ok(()),
            _ => Err(TypedError::PolicyDenied { reason: "no valid lease".into() }),
        }
    }

    /// Submit a request against a lease. Validates the lease (existence,
    /// epoch, expiry) before any admission into the bounded queue.
    pub fn submit(&mut self, lease: &Lease, req: Request) -> Result<Status, TypedError> {
        self.lease_valid(lease)?;
        self.ledger.submit(req)
    }

    /// Execute queued work up to the provider's monotonic clock.
    pub fn run_pending(&mut self) -> Option<u64> {
        self.ledger.run_one(self.now_ms)
    }

    pub fn poll(&mut self, request_id: u64) -> Result<(Status, Vec<ie_contract::tensors::TensorData>), TypedError> {
        self.ledger.poll(request_id)
    }

    pub fn cancel(&mut self, request_id: u64) -> Result<Status, TypedError> {
        self.ledger.cancel(request_id)
    }

    /// Handle one framed canonical request: decode, submit against the
    /// lease encoded in the flow, execute, and return a framed canonical
    /// response. This is the shape a real transport adapter will call.
    pub fn handle_frame(&mut self, lease: &Lease, frame: &[u8]) -> Result<Vec<u8>, TypedError> {
        let (payload, _) = decode_frame(frame).map_err(|_| {
            TypedError::InvalidTensor { reason: "bad frame".into() }
        })?;
        let (req, _) = codec::decode_request(payload).map_err(|_| {
            TypedError::InvalidTensor { reason: "bad request encoding".into() }
        })?;
        let status = self.submit(lease, req.clone())?;
        if status == Status::Pending {
            while self.run_pending().is_some() {}
        }
        let (mut status, outputs) = self.poll(req.request_id)?;
        if status == Status::Pending {
            status = Status::Failed(TypedError::PeerLost);
        }
        let resp = ie_contract::messages::Response {
            request_id: req.request_id,
            status,
            backend: "cpu0".into(),
            provider_id: self.endpoint_id.clone(),
            epoch: self.epoch,
            outputs,
            timing_us: [0, 0, 0],
        };
        let mut buf = Vec::new();
        codec::encode_response(&mut buf, &resp)
            .map_err(|_| TypedError::Internal { reason: "encode".into() })?;
        encode_frame(&buf).map_err(|_| TypedError::Internal { reason: "frame".into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_contract::fixture::GoldenFixture;

    const MODEL: [u8; 32] = [9; 32];

    fn matmul_request(id: u64) -> Request {
        Request {
            request_id: id,
            model_sha256: MODEL,
            inputs: vec![
                GoldenFixture::tensor(&[1.0, 2.0, 3.0, 4.0], &[2, 2]),
                GoldenFixture::tensor(&[5.0, 6.0, 7.0, 8.0], &[2, 2]),
            ],
            deadline_ms: 1000,
            lease_id: 0, // filled after acquire
            policy_digest: [0; 32],
            requested_outputs: vec!["out".into()],
        }
    }

    fn framed(req: &Request) -> Vec<u8> {
        let mut buf = Vec::new();
        codec::encode_request(&mut buf, req).unwrap();
        encode_frame(&buf).unwrap()
    }

    #[test]
    fn t08_loopback_parity_with_direct_execution() {
        let mut p = Provider::new("ie://local/cpu0", 4, 8);
        p.register_model(MODEL);
        let lease = p.acquire_lease(10_000).unwrap();
        let mut req = matmul_request(1);
        req.lease_id = lease.lease_id;

        let resp_frame = p.handle_frame(&lease, &framed(&req)).unwrap();
        let (payload, _) = decode_frame(&resp_frame).unwrap();
        let (resp, _) = codec::decode_response(payload).unwrap();
        assert_eq!(resp.status, Status::Completed);
        assert_eq!(resp.provider_id, "ie://local/cpu0");
        assert_eq!(resp.epoch, 1);
        // Parity: transport path yields the same golden result as direct.
        let got: Vec<f32> = resp.outputs[0]
            .data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(got, vec![19.0, 22.0, 43.0, 50.0]);
    }

    #[test]
    fn submit_without_lease_is_policy_denied() {
        let mut p = Provider::new("ie://local/cpu0", 4, 8);
        p.register_model(MODEL);
        let fake = Lease { lease_id: 99, epoch: 1, expiry_ms: 1000 };
        assert!(matches!(
            p.submit(&fake, matmul_request(1)),
            Err(TypedError::PolicyDenied { .. })
        ));
    }

    #[test]
    fn expired_lease_is_policy_denied() {
        let mut p = Provider::new("ie://local/cpu0", 4, 8);
        p.register_model(MODEL);
        let lease = p.acquire_lease(100).unwrap();
        p.advance_clock(200);
        assert!(matches!(
            p.submit(&lease, matmul_request(1)),
            Err(TypedError::PolicyDenied { .. })
        ));
    }

    /// T12 seed — revocation and no silent failover: after revoke(), the
    /// old epoch lease is UNTRUSTED_PEER and the descriptor advertises the
    /// new epoch.
    #[test]
    fn revocation_invalidates_old_epoch_leases() {
        let mut p = Provider::new("ie://peer/esp0/0", 4, 8);
        p.register_model(MODEL);
        let old = p.acquire_lease(10_000).unwrap();
        p.revoke();
        let mut req = matmul_request(1);
        req.lease_id = old.lease_id;
        assert_eq!(p.submit(&old, req).unwrap_err().name(), "UNTRUSTED_PEER");
        // New epoch works.
        let fresh = p.acquire_lease(10_000).unwrap();
        assert_eq!(fresh.epoch, 2);
        let mut req2 = matmul_request(2);
        req2.lease_id = fresh.lease_id;
        assert_eq!(p.submit(&fresh, req2), Ok(Status::Pending));
    }
}
