//! Contract v0.1 messages (IE-X01 "API contract v0.1 proposal", frozen by
//! ADR-IE-001). Deadlines are remaining durations, never peer wall-clock.

use crate::errors::TypedError;
use crate::tensors::{TensorData, TensorDesc};

/// Authenticated capability descriptor returned by Describe/discovery.
/// Discovery metadata is untrusted until authenticated; this struct is what
/// a provider signs after authentication.
#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityDescriptor {
    pub protocol_version: u32,
    /// Opaque, stable endpoint identity (never a transport address).
    pub endpoint_id: String,
    /// Monotonic epoch counter; results from stale epochs are discarded.
    pub epoch: u64,
    pub supported_ops: Vec<String>,
    pub max_tensor_bytes: u64,
    pub queue_limit: u32,
    /// Lease/entry expiry in provider-monotonic milliseconds from now.
    pub expiry_ms: u64,
}

/// Execution status carried in every response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Accepted; result pending (Poll returns this until terminal).
    Pending,
    Completed,
    Failed(TypedError),
}

/// Inference request (PR-02). Model is pinned by SHA-256 digest; the signed
/// manifest is out of scope for the v0.1 wire profile.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// Client-generated; deduplicated by the provider within bounded
    /// retention. Ambiguous remote completion reports an unknown outcome
    /// rather than claiming exactly-once.
    pub request_id: u64,
    pub model_sha256: [u8; 32],
    pub inputs: Vec<TensorData>,
    /// Remaining-duration deadline in milliseconds (receiver-enforced).
    pub deadline_ms: u64,
    /// Lease this request consumes; must be current (non-stale epoch).
    pub lease_id: u64,
    /// Digest of the client policy document governing this request.
    pub policy_digest: [u8; 32],
    /// Names of requested outputs.
    pub requested_outputs: Vec<String>,
}

/// Terminal/pending response. `backend` reports the *actual* backend used
/// (PR-06): estimates are never labelled as measurements.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub request_id: u64,
    pub status: Status,
    pub backend: String,
    pub provider_id: String,
    pub epoch: u64,
    pub outputs: Vec<TensorData>,
    /// Measured timings in microseconds: queue, execute, transfer.
    pub timing_us: [u64; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_input(vals: &[f32]) -> TensorData {
        let desc = TensorDesc {
            dtype: crate::tensors::DType::F32,
            dims: vec![vals.len() as u64],
            quant_scale: None,
            quant_zero_point: None,
        };
        let mut data = Vec::new();
        for v in vals {
            data.extend_from_slice(&v.to_le_bytes());
        }
        TensorData::new(desc, data).unwrap()
    }

    #[test]
    fn request_response_roundtrip_shape() {
        let req = Request {
            request_id: 42,
            model_sha256: [7u8; 32],
            inputs: vec![f32_input(&[1.0, 2.0])],
            deadline_ms: 500,
            lease_id: 9,
            policy_digest: [0u8; 32],
            requested_outputs: vec!["out".into()],
        };
        assert_eq!(req.inputs[0].desc.payload_len().unwrap(), 8);

        let resp = Response {
            request_id: 42,
            status: Status::Failed(TypedError::UnsupportedOp { op: "atan2".into() }),
            backend: "cpu0".into(),
            provider_id: "local".into(),
            epoch: 3,
            outputs: vec![],
            timing_us: [10, 20, 30],
        };
        assert_eq!(resp.status, Status::Failed(TypedError::UnsupportedOp { op: "atan2".into() }));
    }
}
