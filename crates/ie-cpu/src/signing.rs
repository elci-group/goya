//! Detached signature verification for pinned local model packages.

use ed25519_dalek::{Signature, VerifyingKey};
use ie_contract::errors::TypedError;

const SIGNATURE_DOMAIN: &[u8] = b"iberia/model-package/signature/v1\0";

/// Supported package encodings, bound into the signed message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFormat {
    Json,
    Onnx,
}

impl ModelFormat {
    const fn code(self) -> u8 {
        match self {
            Self::Json => 1,
            Self::Onnx => 2,
        }
    }
}

/// Construct the exact bytes that a trusted signer signs for a package.
pub fn signing_payload(format: ModelFormat, digest: [u8; 32]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(SIGNATURE_DOMAIN.len() + 1 + digest.len());
    payload.extend_from_slice(SIGNATURE_DOMAIN);
    payload.push(format.code());
    payload.extend_from_slice(&digest);
    payload
}

/// Verify a detached Ed25519 signature over a format-bound package digest.
///
/// Trusted public keys and signatures are inputs only; this module does not
/// create, persist, or manage secret key material.
pub(crate) fn verify(
    format: ModelFormat,
    digest: [u8; 32],
    trusted_key: [u8; 32],
    signature: [u8; 64],
) -> Result<(), TypedError> {
    let key = VerifyingKey::from_bytes(&trusted_key).map_err(|_| TypedError::PolicyDenied {
        reason: "invalid Ed25519 trusted key".into(),
    })?;
    if key.is_weak() {
        return Err(TypedError::PolicyDenied {
            reason: "weak Ed25519 trusted key".into(),
        });
    }
    let signature = Signature::from_bytes(&signature);
    key.verify_strict(&signing_payload(format, digest), &signature)
        .map_err(|_| TypedError::PolicyDenied {
            reason: "model package signature verification failed".into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_matches_domain_separated_vector() {
        let mut expected = b"iberia/model-package/signature/v1\0".to_vec();
        expected.push(1);
        expected.extend_from_slice(&[0x5a; 32]);
        assert_eq!(signing_payload(ModelFormat::Json, [0x5a; 32]), expected);
        assert_eq!(
            signing_payload(ModelFormat::Onnx, [0x5a; 32])[expected.len() - 33],
            2
        );
    }

    #[test]
    fn rejects_invalid_or_weak_trusted_key() {
        let err = verify(ModelFormat::Json, [0; 32], [0; 32], [0; 64]).unwrap_err();
        assert!(matches!(err, TypedError::PolicyDenied { .. }));
    }
}
