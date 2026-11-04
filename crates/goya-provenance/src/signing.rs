//! GY-AIM-001 L4 — Signed manifests (C2PA-style claims).
//!
//! A signed manifest binds a set of provenance claims about an artifact to
//! an Ed25519 identity. Signing and verification delegate entirely to
//! `ed25519-dalek`; no signature arithmetic is implemented here.
//!
//! ## Semantics of verification
//!
//! `verify` asserts **signer authenticity, not claim truthfulness**. A
//! `SignatureValid` outcome means: the signature bytes match the canonical
//! claim bytes *and* the signer is in the caller-supplied trust set. It
//! makes no statement about whether the claims describe reality — that is
//! the issuer's responsibility and the auditor's to investigate.
//!
//! ## Key separation
//!
//! Release manifests must be signed with keys held in the release role
//! (`KeyRole::Release`), never keys used by development agents. Policy-aware
//! verification (`verify_with_policy`) requires the verifier's trusted-key
//! registry to bind a key to the requested role. A role claim inside the
//! manifest cannot grant a development key release authority.

use ed25519_dalek::{Signature, SignatureError, Signer, SigningKey, Verifier, VerifyingKey};

/// Role a signing key plays. Release keys are provisioned separately from
/// development agent keys and never share material with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRole {
    /// Keys held by the release pipeline only.
    Release,
    /// Keys held by development agents; never valid for release policy.
    Development,
}

/// A public key together with its role from the verifier's trusted key
/// registry. The role comes from verifier configuration, not from a signed
/// claim supplied by the manifest author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedSigner {
    pub verifying_key: [u8; 32],
    pub role: KeyRole,
}

/// Minimal C2PA-inspired claim set describing an artifact's provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestClaims {
    /// Hash of the artifact this manifest covers.
    pub artifact_hash: u64,
    /// Hash/signature algorithm applied to the artifact.
    pub algorithm: String,
    /// Free-form C2PA-style assertions (e.g. "trained-on:corpus-v3").
    pub assertions: Vec<String>,
    /// Identity of the issuing party.
    pub issuer: String,
    /// Claim creation time, nanoseconds since the Unix epoch.
    pub timestamp_ns: u64,
    /// Role of the key this manifest is intended to be signed with.
    pub key_role: KeyRole,
}

impl ManifestClaims {
    /// Deterministic serialization of the claims. Field order is fixed and
    /// length-prefixed so equal claims always produce identical bytes and
    /// distinct claims never collide by concatenation ambiguity.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        fn push_len_prefixed(out: &mut Vec<u8>, field: &str) {
            out.extend_from_slice(&(field.len() as u64).to_le_bytes());
            out.extend_from_slice(field.as_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"GY-AIM-001\x00");
        out.extend_from_slice(&self.artifact_hash.to_le_bytes());
        push_len_prefixed(&mut out, &self.algorithm);
        out.extend_from_slice(&(self.assertions.len() as u64).to_le_bytes());
        for a in &self.assertions {
            push_len_prefixed(&mut out, a);
        }
        push_len_prefixed(&mut out, &self.issuer);
        out.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        out.push(match self.key_role {
            KeyRole::Release => 0,
            KeyRole::Development => 1,
        });
        out
    }
}

/// An Ed25519 signing keypair plus the verifying key bytes for the public half.
pub struct SigningKeyPair {
    /// Private signing half. Keep secret; release and dev keys must not share.
    pub signing_key: SigningKey,
    /// Public verifying half, as 32 compressed bytes.
    pub verifying: [u8; 32],
}

impl SigningKeyPair {
    /// Generate a fresh random keypair from the OS entropy source.
    pub fn generate() -> SigningKeyPair {
        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        let verifying = signing_key.verifying_key().to_bytes();
        SigningKeyPair {
            signing_key,
            verifying,
        }
    }
}

/// A manifest whose claims are signed over their canonical bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedManifest2 {
    pub claims: ManifestClaims,
    /// 64-byte Ed25519 signature over `claims.canonical_bytes()`.
    pub signature: [u8; 64],
    /// 32-byte verifying key of the signer.
    pub signer: [u8; 32],
}

/// Sign claims with a signing key. The signature covers exactly the
/// canonical bytes of the claims.
pub fn sign(claims: ManifestClaims, signing_key: &SigningKey) -> SignedManifest2 {
    let signature = signing_key.sign(&claims.canonical_bytes()).to_bytes();
    SignedManifest2 {
        signer: signing_key.verifying_key().to_bytes(),
        claims,
        signature,
    }
}

/// Result of verifying a signed manifest.
///
/// These are three *separate* outcomes: cryptographic validity and trust are
/// independent properties, and neither says anything about whether the claims
/// are true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Signature matches the claims and the signer is in the trust set.
    SignatureValid,
    /// Signature does not match the claims (or is malformed).
    SignatureInvalid,
    /// Signature is cryptographically valid but the signer is not trusted.
    SignerUntrusted,
}

impl SignedManifest2 {
    /// Verify against a set of expected (trusted) signer keys.
    pub fn verify(&self, expected_signers: &[[u8; 32]]) -> VerifyOutcome {
        if !expected_signers.contains(&self.signer) {
            return VerifyOutcome::SignerUntrusted;
        }
        self.verify_signature()
    }

    /// Verify, additionally demanding that the recorded key role satisfy
    /// `require_role`. The verifier's trusted-key registry supplies each
    /// signer's role; the signed manifest claim alone cannot establish it.
    pub fn verify_with_policy(
        &self,
        trusted_signers: &[TrustedSigner],
        require_role: KeyRole,
    ) -> VerifyOutcome {
        let Some(trusted) = trusted_signers
            .iter()
            .find(|signer| signer.verifying_key == self.signer)
        else {
            return VerifyOutcome::SignerUntrusted;
        };
        // Both values must agree with verifier policy. The manifest claim is
        // covered by the signature, but its role is still issuer-controlled;
        // only the trusted registry can establish the signer's role.
        if trusted.role != require_role || self.claims.key_role != trusted.role {
            return VerifyOutcome::SignerUntrusted;
        }
        self.verify_signature()
    }

    fn verify_signature(&self) -> VerifyOutcome {
        let ok = VerifyingKey::from_bytes(&self.signer)
            .ok()
            .and_then(|vk| {
                Signature::from_slice(&self.signature)
                    .ok()
                    .map(|s| vk.verify(&self.claims.canonical_bytes(), &s))
            })
            .map(|r: Result<(), SignatureError>| r.is_ok())
            .unwrap_or(false);
        if ok {
            VerifyOutcome::SignatureValid
        } else {
            VerifyOutcome::SignatureInvalid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> ManifestClaims {
        ManifestClaims {
            artifact_hash: 0xdead_beef_cafe_f00d,
            algorithm: "sha256".into(),
            assertions: vec!["trained-on:corpus-v3".into(), "license:audited".into()],
            issuer: "goya-release".into(),
            timestamp_ns: 1_800_000_000_000_000_000,
            key_role: KeyRole::Release,
        }
    }

    #[test]
    fn sign_verify_roundtrip_is_valid() {
        let pair = SigningKeyPair::generate();
        let manifest = sign(claims(), &pair.signing_key);
        assert_eq!(
            manifest.verify(&[pair.verifying]),
            VerifyOutcome::SignatureValid
        );
    }

    #[test]
    fn tampered_claim_bytes_are_invalid() {
        let pair = SigningKeyPair::generate();
        let mut manifest = sign(claims(), &pair.signing_key);
        manifest.claims.artifact_hash ^= 1;
        assert_eq!(
            manifest.verify(&[pair.verifying]),
            VerifyOutcome::SignatureInvalid
        );
    }

    #[test]
    fn untrusted_signer_is_reported_separately() {
        let pair = SigningKeyPair::generate();
        let other = SigningKeyPair::generate();
        let manifest = sign(claims(), &pair.signing_key);
        assert_eq!(
            manifest.verify(&[other.verifying]),
            VerifyOutcome::SignerUntrusted
        );
    }

    #[test]
    fn different_claims_do_not_verify() {
        let pair = SigningKeyPair::generate();
        let manifest = sign(claims(), &pair.signing_key);
        let mut swapped = manifest.clone();
        swapped.claims.assertions.pop();
        swapped.claims.issuer = "someone-else".into();
        swapped.signature = manifest.signature;
        assert_eq!(
            swapped.verify(&[pair.verifying]),
            VerifyOutcome::SignatureInvalid
        );
    }

    #[test]
    fn dev_key_rejected_under_release_policy() {
        let pair = SigningKeyPair::generate();
        let mut c = claims();
        c.key_role = KeyRole::Development;
        let manifest = sign(c, &pair.signing_key);
        assert_eq!(
            manifest.verify_with_policy(
                &[TrustedSigner {
                    verifying_key: pair.verifying,
                    role: KeyRole::Release,
                }],
                KeyRole::Release,
            ),
            VerifyOutcome::SignerUntrusted
        );
        // Without a stricter policy than the manifest's own role, it verifies.
        assert_eq!(
            manifest.verify_with_policy(
                &[TrustedSigner {
                    verifying_key: pair.verifying,
                    role: KeyRole::Development,
                }],
                KeyRole::Development,
            ),
            VerifyOutcome::SignatureValid,
        );
    }

    #[test]
    fn canonical_bytes_are_deterministic() {
        let a = claims().canonical_bytes();
        let b = claims().canonical_bytes();
        assert_eq!(a, b);
        let mut c = claims();
        c.assertions.reverse();
        assert_ne!(c.canonical_bytes(), a);
    }
}
