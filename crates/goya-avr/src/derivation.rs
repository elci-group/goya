//! Target derivation contract (directive §9).
//!
//! §9.3's numeric values are illustrative and MUST NOT become universal
//! defaults without benchmarking: `DerivationProfile` therefore has no
//! `Default` implementation, and every field must be set explicitly.

use crate::encoding::EncodingClass;
use crate::graph::ObservationId;

/// Target inventory profile (§9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryProfile {
    /// Direct indexing, expanded residency, minimal reconstruction.
    Dense,
    /// Mixed direct indexing, compression and local residency.
    Hybrid,
    /// Sparse primary indexing and bounded local neighbourhoods.
    Relational,
}

/// Primary-index policy (§9.3 `[index]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPolicy {
    pub primary_nodes: u32,
    pub max_neighbourhood_size: u32,
    /// Whether canonical observational granularity is preserved. When
    /// false, `allow_pruning` authorises removal of low-weight
    /// observations that no fidelity requirement protects.
    pub preserve_canonical_granularity: bool,
    pub allow_pruning: bool,
}

/// Reconstruction policy (§9.3 `[reconstruction]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconstructionPolicy {
    pub max_dependency_depth: u8,
    /// Enabled encoding classes. Selection still requires reconstruction
    /// cost review for the classes that demand it (§8.5).
    pub allowed_encodings: Vec<EncodingClass>,
}

/// Residency policy (§9.3 `[residency]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyPolicy {
    pub asset_budget_bytes: u64,
    /// Number of neighbourhoods prefetched ahead of the active one.
    pub prefetch_neighbourhoods: u32,
}

/// Full derivation request: a canonical inventory plus an explicit target
/// capability profile (§9.3 `[derivation]`). There is deliberately no
/// `Default`: an unnamed configuration has no provenance and no benchmark
/// evidence (AVR-G03, §18.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationProfile {
    pub profile: InventoryProfile,
    /// Digest of the canonical inventory this profile derives from.
    pub canonical_digest: String,
    pub index: IndexPolicy,
    pub reconstruction: ReconstructionPolicy,
    pub residency: ResidencyPolicy,
    /// Maximum permitted silhouette error in pixels.
    pub max_silhouette_error_px: u32,
    /// Whether live Rugid rendering fallback is authorised (resolution
    /// priority 7, §10.3). Unauthorised fallback is a policy violation
    /// (AVR-C12, §17).
    pub allow_live_fallback: bool,
}

impl DerivationProfile {
    /// Validate the profile's internal consistency before derivation runs.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.canonical_digest.is_empty() {
            return Err("canonical digest is required (AVR-C11)");
        }
        if self.index.primary_nodes == 0 {
            return Err("at least one primary node is required");
        }
        if self.index.max_neighbourhood_size == 0 {
            return Err("neighbourhood size must be positive (AVR-C07)");
        }
        if !self.reconstruction.allowed_encodings.contains(&EncodingClass::Anchor) {
            return Err("every derivation must permit anchor encodings: a relational inventory requires independently reconstructable primary nodes");
        }
        if !self.index.allow_pruning && !self.index.preserve_canonical_granularity {
            return Err("pruning without granularity preservation would silently reduce observational granularity (§8.1)");
        }
        Ok(())
    }

    /// Whether `observation` may be resolved by live rendering under this
    /// profile (resolution priority 7 in §10.3).
    pub fn live_fallback_authorised(&self, _observation: ObservationId) -> bool {
        self.allow_live_fallback
    }
}

/// Digest binding a derivation to the profiles a canonical inventory was
/// compiled for (§7.3 canonical identity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationProfileDigest(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> DerivationProfile {
        DerivationProfile {
            profile: InventoryProfile::Relational,
            canonical_digest: "sha256:abc".into(),
            index: IndexPolicy {
                primary_nodes: 64,
                max_neighbourhood_size: 96,
                preserve_canonical_granularity: true,
                allow_pruning: false,
            },
            reconstruction: ReconstructionPolicy {
                max_dependency_depth: 4,
                allowed_encodings: vec![
                    EncodingClass::Anchor,
                    EncodingClass::Delta,
                    EncodingClass::Albit,
                    EncodingClass::Reproject,
                ],
            },
            residency: ResidencyPolicy {
                asset_budget_bytes: 8_388_608,
                prefetch_neighbourhoods: 1,
            },
            max_silhouette_error_px: 1,
            allow_live_fallback: false,
        }
    }

    #[test]
    fn illustrative_profile_validates() {
        assert!(profile().validate().is_ok());
    }

    #[test]
    fn relational_derivation_requires_anchor_encodings() {
        let mut p = profile();
        p.reconstruction.allowed_encodings = vec![EncodingClass::Delta];
        assert_eq!(
            p.validate().err(),
            Some("every derivation must permit anchor encodings: a relational inventory requires independently reconstructable primary nodes")
        );
    }

    #[test]
    fn live_fallback_is_off_unless_authorised() {
        let p = profile();
        assert!(!p.live_fallback_authorised(crate::graph::ObservationId(1)));
        let mut p = profile();
        p.allow_live_fallback = true;
        assert!(p.live_fallback_authorised(crate::graph::ObservationId(1)));
    }
}
