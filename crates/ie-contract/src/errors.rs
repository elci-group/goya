//! Typed error model (IE-A01 "Failure semantics"). Wire code is stable.

use core::fmt;

/// Typed failures. Wire codes are frozen at v0.1; new variants require a
/// contract revision (ADR), never silent reuse of an existing code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedError {
    UnsupportedOp { op: String },
    InvalidTensor { reason: String },
    PolicyDenied { reason: String },
    UntrustedPeer,
    ModelMissing { digest: String },
    ResourceExhausted { reason: String },
    DeadlineExceeded,
    Cancelled,
    PeerLost,
    VersionMismatch { local: u32, remote: u32 },
    Internal { reason: String },
}

impl TypedError {
    /// Stable wire code for canonical encoding.
    pub fn code(&self) -> u16 {
        match self {
            TypedError::UnsupportedOp { .. } => 1,
            TypedError::InvalidTensor { .. } => 2,
            TypedError::PolicyDenied { .. } => 3,
            TypedError::UntrustedPeer => 4,
            TypedError::ModelMissing { .. } => 5,
            TypedError::ResourceExhausted { .. } => 6,
            TypedError::DeadlineExceeded => 7,
            TypedError::Cancelled => 8,
            TypedError::PeerLost => 9,
            TypedError::VersionMismatch { .. } => 10,
            TypedError::Internal { .. } => 11,
        }
    }

    /// Stable ASCII name, used in logs and conformance reports.
    pub fn name(&self) -> &'static str {
        match self {
            TypedError::UnsupportedOp { .. } => "UNSUPPORTED_OP",
            TypedError::InvalidTensor { .. } => "INVALID_TENSOR",
            TypedError::PolicyDenied { .. } => "POLICY_DENIED",
            TypedError::UntrustedPeer => "UNTRUSTED_PEER",
            TypedError::ModelMissing { .. } => "MODEL_MISSING",
            TypedError::ResourceExhausted { .. } => "RESOURCE_EXHAUSTED",
            TypedError::DeadlineExceeded => "DEADLINE_EXCEEDED",
            TypedError::Cancelled => "CANCELLED",
            TypedError::PeerLost => "PEER_LOST",
            TypedError::VersionMismatch { .. } => "VERSION_MISMATCH",
            TypedError::Internal { .. } => "INTERNAL",
        }
    }

    pub fn from_code(code: u16, detail: &str) -> Option<TypedError> {
        let mk = |s: &str| if s.is_empty() { None } else { Some(s.to_string()) };
        match code {
            1 => mk(detail).map(|op| TypedError::UnsupportedOp { op }),
            2 => mk(detail).map(|reason| TypedError::InvalidTensor { reason }),
            3 => mk(detail).map(|reason| TypedError::PolicyDenied { reason }),
            4 => Some(TypedError::UntrustedPeer),
            5 => mk(detail).map(|digest| TypedError::ModelMissing { digest }),
            6 => mk(detail).map(|reason| TypedError::ResourceExhausted { reason }),
            7 => Some(TypedError::DeadlineExceeded),
            8 => Some(TypedError::Cancelled),
            9 => Some(TypedError::PeerLost),
            10 => {
                let Some((l, rr)) = detail.split_once(':') else { return None };
                let local = l.parse::<u32>().ok()?;
                let remote = rr.parse::<u32>().ok()?;
                Some(TypedError::VersionMismatch { local, remote })
            }
            11 => mk(detail).map(|reason| TypedError::Internal { reason }),
            _ => None,
        }
    }
}

impl fmt::Display for TypedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_for_code_only_variants() {
        for (e, code) in [
            (TypedError::UntrustedPeer, 4u16),
            (TypedError::DeadlineExceeded, 7),
            (TypedError::Cancelled, 8),
            (TypedError::PeerLost, 9),
        ] {
            assert_eq!(TypedError::from_code(code, ""), Some(e));
        }
    }

    #[test]
    fn unknown_code_is_none() {
        assert_eq!(TypedError::from_code(0, ""), None);
        assert_eq!(TypedError::from_code(99, "x"), None);
    }

    #[test]
    fn names_match_spec() {
        assert_eq!(TypedError::UnsupportedOp { op: "x".into() }.name(), "UNSUPPORTED_OP");
        assert_eq!(TypedError::Internal { reason: "".into() }.name(), "INTERNAL");
    }
}
