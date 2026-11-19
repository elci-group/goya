//! Process-local building blocks for NVIDIA CUDA launch plans.
//!
//! [`CudaLaunchPlan`] performs the checked arithmetic for a bounded CUDA
//! kernel launch. This type plans the launch; it never compiles kernels,
//! contacts a driver, or executes work on a GPU.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CudaError {
    ZeroThreadsPerBlock,
    ThreadsPerBlockNotPowerOfTwo,
    ThreadsPerBlockTooLarge,
    ZeroBlocks,
    BlocksTooLarge,
    ZeroSharedMemBytes,
    SharedMemTooLarge,
    BlockThreadOverflow,
}

impl fmt::Display for CudaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroThreadsPerBlock => "threads per block must be positive",
            Self::ThreadsPerBlockNotPowerOfTwo => {
                "threads per block must be a power of two"
            }
            Self::ThreadsPerBlockTooLarge => {
                "threads per block exceeds the CUDA limit of 1024"
            }
            Self::ZeroBlocks => "block count must be positive",
            Self::BlocksTooLarge => "block count exceeds the supported bound",
            Self::ZeroSharedMemBytes => "shared memory bound must be positive",
            Self::SharedMemTooLarge => "shared memory bound exceeds the supported bound",
            Self::BlockThreadOverflow => "total thread count does not fit the supported bound",
        })
    }
}

impl std::error::Error for CudaError {}

/// CUDA limit: at most 1024 threads per block on every architecture in the
/// catalog.
pub const MAX_THREADS_PER_BLOCK: u32 = 1024;
/// Deliberately bounded below the hardware grid limit; the plan type stays
/// within a conservative dispatch envelope.
pub const MAX_BLOCKS: u32 = 1_000_000;
/// Ada Lovelace per-block dynamic shared-memory bound (99 KiB). The plan
/// bound is the conservative minimum across the catalog, not the largest
/// any architecture supports.
pub const MAX_SHARED_MEM_BYTES: u64 = 101_376;

/// Checked launch arithmetic for one bounded CUDA kernel launch. This type
/// plans the launch; it never executes work on a GPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CudaLaunchPlan {
    threads_per_block: u32,
    blocks: u32,
    max_shared_mem_bytes: u64,
}

impl CudaLaunchPlan {
    pub fn new(
        threads_per_block: u32,
        blocks: u32,
        max_shared_mem_bytes: u64,
    ) -> Result<Self, CudaError> {
        if threads_per_block == 0 {
            return Err(CudaError::ZeroThreadsPerBlock);
        }
        if !threads_per_block.is_power_of_two() {
            return Err(CudaError::ThreadsPerBlockNotPowerOfTwo);
        }
        if threads_per_block > MAX_THREADS_PER_BLOCK {
            return Err(CudaError::ThreadsPerBlockTooLarge);
        }
        if blocks == 0 {
            return Err(CudaError::ZeroBlocks);
        }
        if blocks > MAX_BLOCKS {
            return Err(CudaError::BlocksTooLarge);
        }
        if max_shared_mem_bytes == 0 {
            return Err(CudaError::ZeroSharedMemBytes);
        }
        if max_shared_mem_bytes > MAX_SHARED_MEM_BYTES {
            return Err(CudaError::SharedMemTooLarge);
        }
        if u64::from(blocks) * u64::from(threads_per_block) > u64::from(u32::MAX) {
            return Err(CudaError::BlockThreadOverflow);
        }
        Ok(Self {
            threads_per_block,
            blocks,
            max_shared_mem_bytes,
        })
    }

    pub fn threads_per_block(&self) -> u32 {
        self.threads_per_block
    }

    pub fn blocks(&self) -> u32 {
        self.blocks
    }

    pub fn max_shared_mem_bytes(&self) -> u64 {
        self.max_shared_mem_bytes
    }

    /// Total threads across the planned grid.
    pub fn total_threads(&self) -> u64 {
        u64::from(self.blocks) * u64::from(self.threads_per_block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_accepts_full_bounds_and_rejects_violations() {
        let plan = CudaLaunchPlan::new(
            MAX_THREADS_PER_BLOCK,
            MAX_BLOCKS,
            MAX_SHARED_MEM_BYTES,
        )
        .unwrap();
        assert_eq!(plan.threads_per_block(), MAX_THREADS_PER_BLOCK);
        assert_eq!(
            plan.total_threads(),
            u64::from(MAX_BLOCKS) * u64::from(MAX_THREADS_PER_BLOCK)
        );
        assert_eq!(
            CudaLaunchPlan::new(0, 1, 1024),
            Err(CudaError::ZeroThreadsPerBlock)
        );
        assert_eq!(
            CudaLaunchPlan::new(63, 1, 1024),
            Err(CudaError::ThreadsPerBlockNotPowerOfTwo)
        );
        assert_eq!(
            CudaLaunchPlan::new(MAX_THREADS_PER_BLOCK * 2, 1, 1024),
            Err(CudaError::ThreadsPerBlockTooLarge)
        );
        assert_eq!(
            CudaLaunchPlan::new(1, 0, 1024),
            Err(CudaError::ZeroBlocks)
        );
        assert_eq!(
            CudaLaunchPlan::new(1, MAX_BLOCKS + 1, 1024),
            Err(CudaError::BlocksTooLarge)
        );
        assert_eq!(
            CudaLaunchPlan::new(1, 1, 0),
            Err(CudaError::ZeroSharedMemBytes)
        );
        assert_eq!(
            CudaLaunchPlan::new(1, 1, MAX_SHARED_MEM_BYTES + 1),
            Err(CudaError::SharedMemTooLarge)
        );
    }
}
