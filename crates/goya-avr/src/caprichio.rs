//! §14 — Caprichios: hardware-adaptive target selection (WS-11).
//!
//! Selection is driven by measured *capabilities*, never by device names
//! (§14.2: "a powerful Android device may favour dense residency, while an
//! older desktop may require relational execution"). The thresholds below are
//! provisional engineering judgements pending the §18 benchmark programme;
//! they are deliberately explicit rather than hidden behind a `Default` impl
//! (§9.3: profiles MUST NOT acquire universal defaults without benchmarking).
//!
//! §14.3: selecting and deriving from a canonical inventory never requires
//! the original expensive rendering pass — only *requesting new observations*
//! does. `Capabilities::can_compile_observations_locally` makes that check
//! explicit for callers that would trigger re-rendering.

use crate::derivation::{
    DerivationProfile, IndexPolicy, InventoryProfile, ReconstructionPolicy, ResidencyPolicy,
};
use crate::encoding::EncodingClass;

/// §14.1 capability dimensions relevant to Phase 5 strategy selection.
///
/// Everything except the mandatory RAM budget is optional: an unknown
/// dimension conservatively degrades the selected profile rather than
/// over-promising hardware.
#[derive(Debug, Clone, PartialEq)]
pub struct Capabilities {
    /// Available RAM for expanded observation payloads (§14.1).
    pub ram_bytes: u64,
    /// GPU memory, when known.
    pub gpu_bytes: Option<u64>,
    /// Sustained storage throughput in bytes/second, when known. Low
    /// throughput raises the cost of neighbourhood transitions (§11.4).
    pub storage_throughput_bytes_per_s: Option<u64>,
    /// Display resolution, when known.
    pub display: Option<DisplayCapability>,
    /// Required interaction latency budget, when known (§14.1).
    pub interaction_latency: Option<LatencyRequirement>,
}

/// Display capability summary (§14.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayCapability {
    /// Long display edge in pixels.
    pub width_px: u32,
    /// Short display edge in pixels.
    pub height_px: u32,
}

/// §14.1 interaction-latency class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LatencyRequirement {
    /// Touch or remote-control interaction; generous frame budget.
    Relaxed,
    /// Responsive local interaction.
    Interactive,
}

impl Capabilities {
    /// Weak-device reference point (§19: constrained mobile, set-top,
    /// embedded television class). Small RAM, no assumed GPU headroom,
    /// relaxed interaction.
    pub fn constrained() -> Self {
        Self {
            ram_bytes: 256 * 1024 * 1024,
            gpu_bytes: Some(64 * 1024 * 1024),
            storage_throughput_bytes_per_s: Some(8 * 1024 * 1024),
            display: Some(DisplayCapability { width_px: 1280, height_px: 720 }),
            interaction_latency: Some(LatencyRequirement::Relaxed),
        }
    }

    /// Mid-range reference point (§19: midrange phones, laptops).
    pub fn midrange() -> Self {
        Self {
            ram_bytes: 4 * 1024 * 1024 * 1024,
            gpu_bytes: Some(1024 * 1024 * 1024),
            storage_throughput_bytes_per_s: Some(200 * 1024 * 1024),
            display: Some(DisplayCapability { width_px: 1920, height_px: 1080 }),
            interaction_latency: Some(LatencyRequirement::Interactive),
        }
    }

    /// Strong-device reference point (§19: canonical-rendering desktop).
    pub fn workstation() -> Self {
        Self {
            ram_bytes: 32 * 1024 * 1024 * 1024,
            gpu_bytes: Some(8 * 1024 * 1024 * 1024),
            storage_throughput_bytes_per_s: Some(2 * 1024 * 1024 * 1024),
            display: Some(DisplayCapability { width_px: 3840, height_px: 2160 }),
            interaction_latency: Some(LatencyRequirement::Interactive),
        }
    }

    /// §14.3: local derivation from a canonical inventory never requires the
    /// original rendering pass. Requesting *additional observations* does;
    /// this reports whether the device plausibly holds that capability.
    /// Conservative: unknown GPU means "no local canonical compilation".
    pub fn can_compile_observations_locally(&self) -> bool {
        self.gpu_bytes.is_some() && self.ram_bytes >= CANONICAL_COMPILATION_MIN_RAM_BYTES
    }
}

/// Provisional floor for devices that would run canonical (heavy) AVR
/// compilation locally (§14.3). Pending §18 measurements.
pub const CANONICAL_COMPILATION_MIN_RAM_BYTES: u64 = 4 * 1024 * 1024 * 1024;

// Provisional §14.2 selection thresholds, pending §18 benchmark evidence.
/// Below this RAM, only relational profiles are eligible.
const RELATIONAL_MAX_RAM_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Below this RAM, dense residency is not attempted (hybrid ceiling).
const DENSE_MIN_RAM_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Relational profile asset budget for constrained devices.
const RELATIONAL_ASSET_BUDGET_BYTES: u64 = 8 * 1024 * 1024;
/// Hybrid profile asset budget.
const HYBRID_ASSET_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// Dense profile asset budget.
const DENSE_ASSET_BUDGET_BYTES: u64 = 512 * 1024 * 1024;

/// Strategy decision with its provenance, suitable for `.escene` emission
/// (§16 machine-readable metadata: responsible Caprichios, decisions).
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyDecision {
    pub profile: DerivationProfile,
    /// Human-readable rationale recorded in `.escene` diagnostics.
    pub rationale: String,
    /// True when the device may also run canonical compilation locally
    /// (§14.3); false means inventories must arrive pre-compiled.
    pub may_compile_canonical_locally: bool,
}

/// Deterministically selects a target derivation profile from device
/// capabilities (§14.1–14.2). Same capabilities always yield the same
/// decision; ties resolve toward the more conservative profile.
pub fn select_strategy(
    canonical_digest: &str,
    capabilities: &Capabilities,
) -> Result<StrategyDecision, &'static str> {
    if canonical_digest.is_empty() {
        return Err("canonical digest is required (AVR-C11)");
    }

    let (kind, rationale, budget) = if capabilities.ram_bytes < RELATIONAL_MAX_RAM_BYTES {
        (
            InventoryProfile::Relational,
            format!(
                "ram {} bytes below relational ceiling {RELATIONAL_MAX_RAM_BYTES}: sparse primary index with bounded neighbourhood residency (§9.2)",
                capabilities.ram_bytes
            ),
            RELATIONAL_ASSET_BUDGET_BYTES,
        )
    } else if capabilities.ram_bytes < DENSE_MIN_RAM_BYTES {
        (
            InventoryProfile::Hybrid,
            format!(
                "ram {} bytes between relational ceiling and dense floor {DENSE_MIN_RAM_BYTES}: mixed indexing with adaptive residency (§9.2)",
                capabilities.ram_bytes
            ),
            HYBRID_ASSET_BUDGET_BYTES,
        )
    } else {
        (
            InventoryProfile::Dense,
            format!(
                "ram {} bytes at or above dense floor {DENSE_MIN_RAM_BYTES}: expanded direct indexing (§9.2)",
                capabilities.ram_bytes
            ),
            DENSE_ASSET_BUDGET_BYTES,
        )
    };

    // §11.2: prediction is optional. Low storage throughput raises transition
    // cost, so prefetch exactly one neighbourhood; otherwise rely on demand.
    let prefetch_neighbourhoods = match capabilities.storage_throughput_bytes_per_s {
        Some(t) if t < 32 * 1024 * 1024 => 1,
        _ => 0,
    };

    // Reconstruction policy: constrained devices economise on dependency
    // depth (§8.4 hard latency/memory constraints outrank weighted gains).
    let (max_dependency_depth, encodings) = if kind == InventoryProfile::Relational {
        (
            2,
            vec![EncodingClass::Anchor, EncodingClass::Copy, EncodingClass::Shared],
        )
    } else {
        (
            4,
            vec![
                EncodingClass::Anchor,
                EncodingClass::Copy,
                EncodingClass::Shared,
                EncodingClass::Delta,
            ],
        )
    };

    let profile = DerivationProfile {
        profile: kind,
        canonical_digest: canonical_digest.into(),
        index: IndexPolicy {
            primary_nodes: 64,
            max_neighbourhood_size: 96,
            preserve_canonical_granularity: true,
            allow_pruning: false,
        },
        reconstruction: ReconstructionPolicy { max_dependency_depth, allowed_encodings: encodings },
        residency: ResidencyPolicy {
            asset_budget_bytes: budget,
            prefetch_neighbourhoods,
        },
        max_silhouette_error_px: 1,
        allow_live_fallback: false,
    };

    profile.validate()?;

    Ok(StrategyDecision {
        may_compile_canonical_locally: capabilities.can_compile_observations_locally(),
        profile,
        rationale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest() -> String {
        "sha256:caprichio-test".into()
    }

    /// §14.2: a weak device selects a relational profile with tight budgets.
    #[test]
    fn weak_device_selects_relational() {
        let d = select_strategy(&digest(), &Capabilities::constrained()).expect("decision");
        assert_eq!(d.profile.profile, InventoryProfile::Relational);
        assert_eq!(d.profile.residency.asset_budget_bytes, RELATIONAL_ASSET_BUDGET_BYTES);
        assert!(d.profile.index.preserve_canonical_granularity);
        assert!(!d.profile.allow_live_fallback);
        // §14.3: a constrained device cannot run canonical compilation.
        assert!(!d.may_compile_canonical_locally);
    }

    /// §14.2: a strong device selects dense; mid-range selects hybrid.
    #[test]
    fn strong_and_midrange_devices_select_appropriate_profiles() {
        let strong = select_strategy(&digest(), &Capabilities::workstation()).expect("decision");
        assert_eq!(strong.profile.profile, InventoryProfile::Dense);
        assert!(strong.may_compile_canonical_locally);

        let mid = select_strategy(&digest(), &Capabilities::midrange()).expect("decision");
        assert_eq!(mid.profile.profile, InventoryProfile::Hybrid);
        assert!(mid.may_compile_canonical_locally);
    }

    /// §14.2: the same capabilities always produce the same decision.
    #[test]
    fn selection_is_deterministic() {
        let caps = Capabilities::midrange();
        let a = select_strategy(&digest(), &caps).expect("a");
        let b = select_strategy(&digest(), &caps).expect("b");
        assert_eq!(a, b);
    }

    /// Every selected profile passes §9.3 validation before it is returned.
    #[test]
    fn selected_profiles_validate() {
        for caps in
            [Capabilities::constrained(), Capabilities::midrange(), Capabilities::workstation()]
        {
            let d = select_strategy(&digest(), &caps).expect("decision");
            assert!(d.profile.validate().is_ok());
            assert!(!d.rationale.is_empty());
        }
    }

    /// AVR-C11: an empty canonical digest is rejected outright.
    #[test]
    fn empty_digest_is_rejected() {
        assert!(select_strategy("", &Capabilities::workstation()).is_err());
    }

    /// §14.3: local derivation of targets never needs the heavy pass, but
    /// new-observation compilation is gated behind known GPU + RAM.
    #[test]
    fn local_canonical_compilation_is_gated() {
        let mut weak = Capabilities::constrained();
        assert!(!weak.can_compile_observations_locally());
        weak.ram_bytes = CANONICAL_COMPILATION_MIN_RAM_BYTES;
        weak.gpu_bytes = None;
        assert!(!weak.can_compile_observations_locally());
        weak.gpu_bytes = Some(1024 * 1024 * 1024);
        assert!(weak.can_compile_observations_locally());
    }
}
