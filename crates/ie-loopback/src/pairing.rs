//! Pairing and provider quotas (WP-006 seed, IE-P02 "User control").
//!
//! The provider owner explicitly enables sharing: a client is admitted
//! only after a paired out-of-band verification code is confirmed, may be
//! revoked at any time, and is capped by an explicit resource quota. No
//! discovery message confers trust — pairing is the trust event.
//!
//! Quota semantics: per-client cap on concurrently queued requests and on
//! the maximum accepted deadline. Quota breach is RESOURCE_EXHAUSTED and
//! never preempts other clients (fair sharing seed); revocation is
//! POLICY_DENIED for new work. Provider crash cannot change application
//! state directly (contract), so revocation destroys only provider-side
//! trust, not client data.

use crate::Lease;
use crate::Provider;
use ie_contract::errors::TypedError;
use ie_contract::messages::{Request, Status};
use std::collections::HashMap;

/// Out-of-band pairing code the owner reads off the client and confirms on
/// the provider. Loopback models the user-confirmed comparison directly.
pub type PairingCode = u32;

/// Owner-set resource cap for one paired client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quota {
    /// Maximum concurrently queued requests for this client.
    pub max_concurrent: usize,
    /// Maximum acceptable remaining-duration deadline in ms.
    pub max_deadline_ms: u64,
}

impl Quota {
    pub fn new(max_concurrent: usize, max_deadline_ms: u64) -> Quota {
        Quota { max_concurrent, max_deadline_ms }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClientState {
    Paired,
    Revoked,
}

/// A provider with owner-controlled sharing.
pub struct SharedProvider {
    inner: Provider,
    codes: HashMap<String, PairingCode>,
    states: HashMap<String, ClientState>,
    quotas: HashMap<String, Quota>,
    /// Lease ownership: a lease is bound to the client it was issued to.
    lease_owner: HashMap<u64, String>,
    /// Requests currently queued per client (quota accounting).
    queued: HashMap<String, Vec<u64>>,
}

impl SharedProvider {
    pub fn new(endpoint_id: &str, queue_limit: usize, retention: usize) -> SharedProvider {
        SharedProvider {
            inner: Provider::new(endpoint_id, queue_limit, retention),
            codes: HashMap::new(),
            states: HashMap::new(),
            quotas: HashMap::new(),
            lease_owner: HashMap::new(),
            queued: HashMap::new(),
        }
    }

    /// Escape hatch for runtime setup (model registration on the inner
    /// provider). Pairing/consent state must only change through this
    /// wrapper.
    pub fn inner_mut(&mut self) -> &mut Provider {
        &mut self.inner
    }

    /// Owner side: publish a pairing code for an intending client and set
    /// its quota. The code is confirmed out-of-band by the owner.
    pub fn offer_pairing(&mut self, client_id: &str, code: PairingCode, quota: Quota) {
        self.codes.insert(client_id.to_string(), code);
        self.quotas.insert(client_id.to_string(), quota);
    }

    /// Client side: present the code it read off the provider. Matching
    /// codes complete pairing; a mismatch leaves the client unpaired.
    pub fn pair(&mut self, client_id: &str, code: PairingCode) -> Result<(), TypedError> {
        match self.codes.get(client_id) {
            Some(&c) if c == code => {
                self.states.insert(client_id.to_string(), ClientState::Paired);
                Ok(())
            }
            Some(_) => Err(TypedError::UntrustedPeer),
            None => Err(TypedError::UntrustedPeer),
        }
    }

    /// Owner revokes consent (P02: provider owner may revoke). The
    /// client's outstanding lease is released; queued work from that
    /// client is cancelled so it never starts after consent ends.
    pub fn revoke_client(&mut self, client_id: &str) {
        self.states.insert(client_id.to_string(), ClientState::Revoked);
        let owned: Vec<u64> = self
            .lease_owner
            .iter()
            .filter(|(_, c)| c.as_str() == client_id)
            .map(|(id, _)| *id)
            .collect();
        for id in owned {
            self.inner.release_lease(id);
        }
    }

    fn client_ok(&self, client_id: &str) -> Result<(), TypedError> {
        match self.states.get(client_id) {
            Some(ClientState::Paired) => Ok(()),
            Some(ClientState::Revoked) => {
                Err(TypedError::PolicyDenied { reason: "client revoked".into() })
            }
            None => Err(TypedError::UntrustedPeer),
        }
    }

    /// Paired client acquires a lease bound to its identity.
    pub fn acquire_lease(&mut self, client_id: &str, ttl_ms: u64) -> Result<Lease, TypedError> {
        self.client_ok(client_id)?;
        let lease = self.inner.acquire_lease(ttl_ms)?;
        self.lease_owner.insert(lease.lease_id, client_id.to_string());
        Ok(lease)
    }

    /// Submit on behalf of a client: lease ownership, pairing state and
    /// quota are all checked before admission.
    pub fn submit(&mut self, client_id: &str, lease: &Lease, req: Request) -> Result<Status, TypedError> {
        self.client_ok(client_id)?;
        if self.lease_owner.get(&lease.lease_id).map(|s| s.as_str()) != Some(client_id) {
            return Err(TypedError::UntrustedPeer);
        }
        let quota = *self.quotas.get(client_id).ok_or(TypedError::UntrustedPeer)?;
        if req.deadline_ms > quota.max_deadline_ms {
            return Err(TypedError::PolicyDenied {
                reason: format!(
                    "deadline {} ms exceeds quota {} ms",
                    req.deadline_ms, quota.max_deadline_ms
                ),
            });
        }
        let in_flight = self.queued.entry(client_id.to_string()).or_default();
        if in_flight.len() >= quota.max_concurrent {
            return Err(TypedError::ResourceExhausted {
                reason: "client concurrency quota reached".into(),
            });
        }
        let status = self.inner.submit(lease, req.clone())?;
        if status == Status::Pending {
            in_flight.push(req.request_id);
        }
        Ok(status)
    }

    /// Drive queued work; returns request ids executed.
    pub fn run_pending(&mut self) -> Option<u64> {
        self.inner.run_pending()
    }

    /// Poll and release quota accounting on terminal outcome.
    pub fn poll(
        &mut self,
        request_id: u64,
    ) -> Result<(Status, Vec<ie_contract::tensors::TensorData>), TypedError> {
        let out = self.inner.poll(request_id)?;
        for v in self.queued.values_mut() {
            v.retain(|&id| id != request_id);
        }
        Ok(out)
    }

    /// Cancel on behalf of a client; frees its quota slot.
    pub fn cancel(&mut self, client_id: &str, request_id: u64) -> Result<Status, TypedError> {
        self.client_ok(client_id)?;
        let st = self.inner.cancel(request_id)?;
        if let Some(v) = self.queued.get_mut(client_id) {
            v.retain(|&id| id != request_id);
        }
        Ok(st)
    }

    pub fn queued_len(&self, client_id: &str) -> usize {
        self.queued.get(client_id).map(|v| v.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_contract::fixture::GoldenFixture;

    const MODEL: [u8; 32] = [5; 32];

    fn req(id: u64, deadline: u64) -> Request {
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

    fn setup() -> (SharedProvider, Lease, Lease) {
        let mut p = SharedProvider::new("ie://peer/esp0/0", 8, 16);
        p.inner_mut().register_model(MODEL);
        p.offer_pairing("phone", 424_242, Quota::new(1, 5_000));
        p.offer_pairing("laptop", 111_111, Quota::new(4, 5_000));
        assert!(p.pair("phone", 424_242).is_ok());
        assert!(p.pair("laptop", 111_111).is_ok());
        let lp = p.acquire_lease("laptop", 10_000).unwrap();
        let ph = p.acquire_lease("phone", 10_000).unwrap();
        (p, lp, ph)
    }

    /// Wrong or unknown pairing codes never admit a client (T06 seed:
    /// untrusted peer).
    #[test]
    fn wrong_pairing_code_is_untrusted_peer() {
        let mut p = SharedProvider::new("ie://peer/esp0/0", 8, 16);
        p.offer_pairing("phone", 424_242, Quota::new(1, 1_000));
        assert_eq!(p.pair("phone", 1).unwrap_err().name(), "UNTRUSTED_PEER");
        assert_eq!(p.pair("stranger", 424_242).unwrap_err().name(), "UNTRUSTED_PEER");
    }

    #[test]
    fn unpaired_client_cannot_acquire_lease() {
        let mut p = SharedProvider::new("ie://peer/esp0/0", 8, 16);
        assert_eq!(
            p.acquire_lease("stranger", 1_000).unwrap_err().name(),
            "UNTRUSTED_PEER"
        );
    }

    /// Quota: concurrency cap is per client and never starves others.
    #[test]
    fn concurrency_quota_breaches_without_starving_others() {
        let (mut p, laptop, phone) = setup();
        let mut r1 = req(1, 1_000);
        r1.lease_id = phone.lease_id;
        assert_eq!(p.submit("phone", &phone, r1), Ok(Status::Pending));
        // Quota 1: second phone request breaches.
        let mut r2 = req(2, 1_000);
        r2.lease_id = phone.lease_id;
        assert_eq!(
            p.submit("phone", &phone, r2).unwrap_err().name(),
            "RESOURCE_EXHAUSTED"
        );
        // Laptop unaffected (fair sharing seed).
        let mut r3 = req(3, 1_000);
        r3.lease_id = laptop.lease_id;
        assert_eq!(p.submit("laptop", &laptop, r3), Ok(Status::Pending));
    }

    #[test]
    fn deadline_cap_enforced_per_quota() {
        let (mut p, laptop, _) = setup();
        let mut r = req(1, 99_999);
        r.lease_id = laptop.lease_id;
        assert_eq!(p.submit("laptop", &laptop, r).unwrap_err().name(), "POLICY_DENIED");
    }

    /// Lease theft: a client using another client's lease is untrusted.
    #[test]
    fn lease_bound_to_pairing_client() {
        let (mut p, laptop, phone) = setup();
        let mut r = req(1, 1_000);
        r.lease_id = phone.lease_id; // laptop presenting phone's lease
        assert_eq!(p.submit("laptop", &phone, r).unwrap_err().name(), "UNTRUSTED_PEER");
    }

    /// Revocation: consent ends, new work denied, other clients unaffected.
    #[test]
    fn revocation_denies_new_work_only_for_that_client() {
        let (mut p, laptop, phone) = setup();
        let mut r1 = req(1, 1_000);
        r1.lease_id = phone.lease_id;
        p.submit("phone", &phone, r1).unwrap();
        p.run_pending();
        p.revoke_client("phone");
        let mut r2 = req(2, 1_000);
        r2.lease_id = phone.lease_id;
        assert_eq!(p.submit("phone", &phone, r2).unwrap_err().name(), "POLICY_DENIED");
        // Laptop still fully serviced — no silent failover, no collateral.
        let mut r3 = req(3, 1_000);
        r3.lease_id = laptop.lease_id;
        assert_eq!(p.submit("laptop", &laptop, r3), Ok(Status::Pending));
        p.run_pending();
        let (st, _) = p.poll(3).unwrap();
        assert_eq!(st, Status::Completed);
    }
}
