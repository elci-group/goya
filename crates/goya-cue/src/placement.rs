//! Hardware placement strategies for a colour cue.
//!
//! Unified memory here is process-visible shared memory, not a Metal allocation.
//! A discrete GPU's device-local palette is refused by residency.

use crate::residency::PlaceError;

/// Machine the cue is being placed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardwareTarget {
    DiscreteGpu,
    IntegratedAmdIntel,
    AppleSilicon,
    AndroidTile,
    EmbeddedIndexed,
    CpuOnly,
    NativeWidgets,
}

/// How colours are stored for a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    DirectCpu,
    SoftwarePalette,
    SharedMemoryInventory,
    /// Unified memory here is process-visible shared memory, not a Metal allocation.
    UnifiedMemory,
    TileDamage,
    PlatformTheme,
    DeviceLocalPalette,
}

// Embedded indexed tables are admitted as SoftwarePalette because there is no GPU.
pub fn strategy_for(target: HardwareTarget) -> Strategy {
    match target {
        HardwareTarget::DiscreteGpu => Strategy::DeviceLocalPalette,
        HardwareTarget::IntegratedAmdIntel => Strategy::SharedMemoryInventory,
        HardwareTarget::AppleSilicon => Strategy::UnifiedMemory,
        HardwareTarget::AndroidTile => Strategy::TileDamage,
        HardwareTarget::EmbeddedIndexed => Strategy::SoftwarePalette,
        HardwareTarget::CpuOnly => Strategy::SoftwarePalette,
        HardwareTarget::NativeWidgets => Strategy::PlatformTheme,
    }
}

/// Admit a strategy only when it is legal on this machine.
/// DeviceLocalPalette is always PlaceError::VulkanBlocked, even when indexed_cheaper is true.
/// TileDamage and PlatformTheme are admitted as themselves, ignoring indexed_cheaper.
/// Every other candidate is admitted as itself when indexed_cheaper is true, otherwise DirectCpu.
pub fn admit(target: HardwareTarget, indexed_cheaper: bool) -> Result<Strategy, PlaceError> {
    let candidate = strategy_for(target);
    if candidate == Strategy::DeviceLocalPalette {
        return crate::residency::place(crate::residency::MemoryTier::DeviceLocal)
            .map(|_| candidate);
    }
    if matches!(candidate, Strategy::TileDamage | Strategy::PlatformTheme) {
        return Ok(candidate);
    }
    if indexed_cheaper {
        Ok(candidate)
    } else {
        Ok(Strategy::DirectCpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discrete_gpu_is_blocked_either_way() {
        assert!(admit(HardwareTarget::DiscreteGpu, true).is_err());
        assert_eq!(
            admit(HardwareTarget::DiscreteGpu, true).unwrap_err(),
            PlaceError::VulkanBlocked
        );
        assert_eq!(
            admit(HardwareTarget::DiscreteGpu, false).unwrap_err(),
            PlaceError::VulkanBlocked
        );
    }

    #[test]
    fn tile_and_theme_ignore_indexed_cheaper() {
        assert_eq!(
            admit(HardwareTarget::AndroidTile, false),
            Ok(Strategy::TileDamage)
        );
        assert_eq!(
            admit(HardwareTarget::NativeWidgets, false),
            Ok(Strategy::PlatformTheme)
        );
    }

    #[test]
    fn integrated_follows_indexed_cheaper() {
        assert_eq!(
            admit(HardwareTarget::IntegratedAmdIntel, true),
            Ok(Strategy::SharedMemoryInventory)
        );
        assert_eq!(
            admit(HardwareTarget::IntegratedAmdIntel, false),
            Ok(Strategy::DirectCpu)
        );
    }

    #[test]
    fn apple_silicon_unified_memory_when_indexed_is_cheaper() {
        assert_eq!(
            admit(HardwareTarget::AppleSilicon, true),
            Ok(Strategy::UnifiedMemory)
        );
    }

    #[test]
    fn cpu_only_falls_back_when_index_is_not_cheaper() {
        assert_eq!(
            admit(HardwareTarget::CpuOnly, false),
            Ok(Strategy::DirectCpu)
        );
    }

    #[test]
    fn embedded_indexed_is_software_palette() {
        assert_eq!(
            admit(HardwareTarget::EmbeddedIndexed, true),
            Ok(Strategy::SoftwarePalette)
        );
        assert_eq!(
            strategy_for(HardwareTarget::EmbeddedIndexed),
            Strategy::SoftwarePalette
        );
    }
}
