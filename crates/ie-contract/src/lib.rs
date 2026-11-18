//! ie-contract — Iberia/España shared protocol and conformance contract v0.1
//! (frozen per PAMPER IE-X01, ADR-IE-001).
//!
//! This crate owns no implementation dependencies. It defines the typed
//! error model, capability descriptor, tensor descriptors, request/response
//! messages, the canonical bounded length-prefixed wire framing, and the
//! FP32 golden-tolerance oracle used by the conformance harness.

pub mod codec;
pub mod errors;
pub mod fixture;
pub mod framing;
pub mod messages;
pub mod tensors;

pub use errors::TypedError;
pub use fixture::{GoldenFixture, TOLERANCE};
pub use framing::{decode_frame, encode_frame, FrameError, MAX_MESSAGE_BYTES};
pub use messages::{CapabilityDescriptor, Request, Response, Status};
pub use tensors::{DType, TensorDesc, TensorData};

/// Protocol version frozen at G1. Receivers must reject on mismatch
/// (VERSION_MISMATCH) rather than best-effort interpret.
pub const PROTOCOL_VERSION: u32 = 1;

/// Endpoint states (IE-A01 lifecycle). Only `Ready` admits jobs; `Degraded`
/// requires policy approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointState {
    Discovered,
    Authenticated,
    Ready,
    Degraded,
    Draining,
    Revoked,
    Offline,
}

impl EndpointState {
    /// Whether this state admits new work without policy approval.
    pub fn admits_jobs(self) -> bool {
        matches!(self, EndpointState::Ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ready_admits_jobs() {
        assert!(EndpointState::Ready.admits_jobs());
        assert!(!EndpointState::Degraded.admits_jobs());
        for s in [
            EndpointState::Discovered,
            EndpointState::Authenticated,
            EndpointState::Draining,
            EndpointState::Revoked,
            EndpointState::Offline,
        ] {
            assert!(!s.admits_jobs());
        }
    }
}
