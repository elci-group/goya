//! Process-local building blocks for Intel desktop CPU worker plans.
//!
//! [`WorkerPlan`] performs the checked arithmetic for a bounded CPU-compute
//! worker plan. This type plans the work; it never spawns threads, executes
//! SIMD, or proves that a CPU executed work.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerError {
    ZeroWorkerThreads,
    WorkerThreadsTooLarge,
    ZeroBatch,
    BatchTooLarge,
    ZeroWorkspaceBytes,
    WorkspaceBytesTooLarge,
    WorkerOverflow,
}

impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroWorkerThreads => "worker thread count must be positive",
            Self::WorkerThreadsTooLarge => "worker thread count exceeds the supported bound",
            Self::ZeroBatch => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::ZeroWorkspaceBytes => "workspace byte bound must be positive",
            Self::WorkspaceBytesTooLarge => "workspace byte bound exceeds the supported bound",
            Self::WorkerOverflow => "total worker executions do not fit the supported bound",
        })
    }
}

impl std::error::Error for WorkerError {}

/// Conservative bound across the catalog's desktop parts: a Raptor Lake
/// flagship exposes 24 cores with SMT, or 32 hardware threads. The HEDT
/// Core-X rows document up to 18 cores / 36 hardware threads; the v1 bound
/// caps planning at 32 worker threads, so those parts plan below their
/// hardware ceiling and fail closed at this bound first.
pub const MAX_WORKER_THREADS: u32 = 32;
/// Deliberately bounded batch size; the plan type stays within a
/// conservative worker envelope.
pub const MAX_BATCH: u32 = 65_536;
/// 1 GiB workspace bound. The plan bound is a conservative envelope, not
/// the largest any desktop part supports.
pub const MAX_WORKSPACE_BYTES: u64 = 1 << 30;

/// Checked arithmetic for one bounded Intel desktop CPU worker plan. This
/// type plans the work; it never spawns threads or executes work on a
/// CPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerPlan {
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
}

impl WorkerPlan {
    pub fn new(worker_threads: u32, batch: u32, workspace_bytes: u64) -> Result<Self, WorkerError> {
        if worker_threads == 0 {
            return Err(WorkerError::ZeroWorkerThreads);
        }
        if worker_threads > MAX_WORKER_THREADS {
            return Err(WorkerError::WorkerThreadsTooLarge);
        }
        if batch == 0 {
            return Err(WorkerError::ZeroBatch);
        }
        if batch > MAX_BATCH {
            return Err(WorkerError::BatchTooLarge);
        }
        if workspace_bytes == 0 {
            return Err(WorkerError::ZeroWorkspaceBytes);
        }
        if workspace_bytes > MAX_WORKSPACE_BYTES {
            return Err(WorkerError::WorkspaceBytesTooLarge);
        }
        if u64::from(worker_threads) * u64::from(batch) > u64::from(u32::MAX) {
            return Err(WorkerError::WorkerOverflow);
        }
        Ok(Self {
            worker_threads,
            batch,
            workspace_bytes,
        })
    }

    pub fn worker_threads(&self) -> u32 {
        self.worker_threads
    }

    pub fn batch(&self) -> u32 {
        self.batch
    }

    pub fn workspace_bytes(&self) -> u64 {
        self.workspace_bytes
    }

    /// Total worker executions across the batch.
    pub fn total_worker_executions(&self) -> u64 {
        u64::from(self.worker_threads) * u64::from(self.batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_accepts_full_bounds_and_rejects_violations() {
        let plan = WorkerPlan::new(MAX_WORKER_THREADS, MAX_BATCH, MAX_WORKSPACE_BYTES).unwrap();
        assert_eq!(plan.worker_threads(), MAX_WORKER_THREADS);
        assert_eq!(
            plan.total_worker_executions(),
            u64::from(MAX_WORKER_THREADS) * u64::from(MAX_BATCH)
        );
        assert_eq!(
            WorkerPlan::new(0, 1, 1024),
            Err(WorkerError::ZeroWorkerThreads)
        );
        assert_eq!(
            WorkerPlan::new(MAX_WORKER_THREADS + 1, 1, 1024),
            Err(WorkerError::WorkerThreadsTooLarge)
        );
        assert_eq!(WorkerPlan::new(1, 0, 1024), Err(WorkerError::ZeroBatch));
        assert_eq!(
            WorkerPlan::new(1, MAX_BATCH + 1, 1024),
            Err(WorkerError::BatchTooLarge)
        );
        assert_eq!(
            WorkerPlan::new(1, 1, 0),
            Err(WorkerError::ZeroWorkspaceBytes)
        );
        assert_eq!(
            WorkerPlan::new(1, 1, MAX_WORKSPACE_BYTES + 1),
            Err(WorkerError::WorkspaceBytesTooLarge)
        );
    }
}
