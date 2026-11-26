//! Memory residency for a colour cue.
//!
//! Device-local placement is refused. No Vulkan buffer is allocated.

use std::fmt;

/// Where a cue's colours live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryTier {
    CpuExpanded,
    CpuIndexed,
    SharedMemory,
    DeviceLocal,
}

/// A placement the machine is allowed to use.
///
/// There is no device-local variant: Vulkan stays blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Cpu { indexed: bool },
    SharedMemory { indexed: bool },
}

/// Why a tier cannot be placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// GY-EMP-RENDER/1: Vulkan is blocked. No device-local buffer is allocated.
    VulkanBlocked,
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaceError::VulkanBlocked => f.write_str(
                "GY-EMP-RENDER/1: Vulkan is blocked. No device-local buffer is allocated.",
            ),
        }
    }
}

impl std::error::Error for PlaceError {}

/// Place `tier` without allocating device memory.
///
/// `MemoryTier::DeviceLocal` is [`PlaceError::VulkanBlocked`].
pub fn place(tier: MemoryTier) -> Result<Placement, PlaceError> {
    match tier {
        MemoryTier::CpuExpanded => Ok(Placement::Cpu { indexed: false }),
        MemoryTier::CpuIndexed => Ok(Placement::Cpu { indexed: true }),
        MemoryTier::SharedMemory => Ok(Placement::SharedMemory { indexed: true }),
        MemoryTier::DeviceLocal => Err(PlaceError::VulkanBlocked),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_expanded_is_unindexed_cpu() {
        assert_eq!(
            place(MemoryTier::CpuExpanded),
            Ok(Placement::Cpu { indexed: false })
        );
    }

    #[test]
    fn cpu_indexed_is_indexed_cpu() {
        assert_eq!(
            place(MemoryTier::CpuIndexed),
            Ok(Placement::Cpu { indexed: true })
        );
    }

    #[test]
    fn shared_memory_is_indexed() {
        assert_eq!(
            place(MemoryTier::SharedMemory),
            Ok(Placement::SharedMemory { indexed: true })
        );
    }

    #[test]
    fn device_local_is_vulkan_blocked() {
        let err = place(MemoryTier::DeviceLocal).unwrap_err();
        assert_eq!(err, PlaceError::VulkanBlocked);
        assert_eq!(
            err.to_string(),
            "GY-EMP-RENDER/1: Vulkan is blocked. No device-local buffer is allocated."
        );
    }
}
