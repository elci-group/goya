//! Bounded, process-local byte buffers with explicit owner-scoped handles.
//!
//! This module implements ordinary Rust memory only. It does not expose hardware,
//! shared-memory, persistence, or platform capabilities. Buffers are zero-initialized,
//! reads return copies, writes are range checked, and releasing an owner drops all of
//! its buffers. Limits constrain accepted allocations; they are not reservations of
//! system memory.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Weak};

pub const CONTRACT: &str = "goya.memory.buffers.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryLimits {
    pub max_total_bytes: usize,
    pub max_buffer_bytes: usize,
    pub max_buffers: usize,
    pub max_owners: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryError {
    InvalidLimits,
    OwnerLimitExceeded,
    BufferLimitExceeded,
    BufferTooLarge,
    TotalLimitExceeded,
    AllocationFailed,
    IdentifierExhausted,
    InvalidOwner,
    InvalidBuffer,
    OutOfBounds,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "memory limits must be positive",
            Self::OwnerLimitExceeded => "maximum owner count exceeded",
            Self::BufferLimitExceeded => "maximum buffer count exceeded",
            Self::BufferTooLarge => "buffer exceeds per-buffer size limit",
            Self::TotalLimitExceeded => "buffer allocation exceeds total byte limit",
            Self::AllocationFailed => "memory allocation failed",
            Self::IdentifierExhausted => "memory identifier space exhausted",
            Self::InvalidOwner => "owner is invalid or belongs to another arena",
            Self::InvalidBuffer => "buffer is invalid, released, or belongs to another owner",
            Self::OutOfBounds => "buffer range is out of bounds",
        })
    }
}

impl std::error::Error for MemoryError {}

#[derive(Clone, Debug)]
pub struct MemoryOwner {
    arena: Weak<()>,
    id: u64,
}

#[derive(Clone, Debug)]
pub struct BufferHandle {
    arena: Weak<()>,
    owner_id: u64,
    id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryStats {
    pub owners: usize,
    pub buffers: usize,
    pub allocated_bytes: usize,
}

struct Buffer {
    owner_id: u64,
    bytes: Vec<u8>,
}

/// A bounded in-memory arena. Handles are scoped to this arena and to their owner.
pub struct MemoryArena {
    identity: Arc<()>,
    limits: MemoryLimits,
    owners: BTreeSet<u64>,
    buffers: BTreeMap<u64, Buffer>,
    allocated_bytes: usize,
    next_owner_id: u64,
    next_buffer_id: u64,
}

impl MemoryArena {
    pub fn new(limits: MemoryLimits) -> Result<Self, MemoryError> {
        if limits.max_total_bytes == 0
            || limits.max_buffer_bytes == 0
            || limits.max_buffers == 0
            || limits.max_owners == 0
        {
            return Err(MemoryError::InvalidLimits);
        }
        Ok(Self {
            identity: Arc::new(()),
            limits,
            owners: BTreeSet::new(),
            buffers: BTreeMap::new(),
            allocated_bytes: 0,
            next_owner_id: 1,
            next_buffer_id: 1,
        })
    }

    pub fn create_owner(&mut self) -> Result<MemoryOwner, MemoryError> {
        if self.owners.len() >= self.limits.max_owners {
            return Err(MemoryError::OwnerLimitExceeded);
        }
        let next = self
            .next_owner_id
            .checked_add(1)
            .ok_or(MemoryError::IdentifierExhausted)?;
        let id = self.next_owner_id;
        self.owners.insert(id);
        self.next_owner_id = next;
        Ok(MemoryOwner {
            arena: Arc::downgrade(&self.identity),
            id,
        })
    }

    /// Allocate a zero-filled buffer. Zero-sized buffers are rejected.
    pub fn allocate(
        &mut self,
        owner: &MemoryOwner,
        size: usize,
    ) -> Result<BufferHandle, MemoryError> {
        self.validate_owner(owner)?;
        if size == 0 || size > self.limits.max_buffer_bytes {
            return Err(MemoryError::BufferTooLarge);
        }
        if self.buffers.len() >= self.limits.max_buffers {
            return Err(MemoryError::BufferLimitExceeded);
        }
        let new_total = self
            .allocated_bytes
            .checked_add(size)
            .filter(|total| *total <= self.limits.max_total_bytes)
            .ok_or(MemoryError::TotalLimitExceeded)?;
        let next = self
            .next_buffer_id
            .checked_add(1)
            .ok_or(MemoryError::IdentifierExhausted)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| MemoryError::AllocationFailed)?;
        bytes.resize(size, 0);

        let id = self.next_buffer_id;
        self.buffers.insert(
            id,
            Buffer {
                owner_id: owner.id,
                bytes,
            },
        );
        self.next_buffer_id = next;
        self.allocated_bytes = new_total;
        Ok(BufferHandle {
            arena: Arc::downgrade(&self.identity),
            owner_id: owner.id,
            id,
        })
    }

    /// Return a snapshot of the complete buffer.
    pub fn read(&self, owner: &MemoryOwner, handle: &BufferHandle) -> Result<Vec<u8>, MemoryError> {
        let buffer = self.buffer_for(owner, handle)?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(buffer.bytes.len())
            .map_err(|_| MemoryError::AllocationFailed)?;
        copy.extend_from_slice(&buffer.bytes);
        Ok(copy)
    }

    /// Copy bytes into a buffer at `offset`; the whole write succeeds or none does.
    pub fn write(
        &mut self,
        owner: &MemoryOwner,
        handle: &BufferHandle,
        offset: usize,
        input: &[u8],
    ) -> Result<(), MemoryError> {
        self.validate_owner(owner)?;
        self.validate_handle_identity(owner, handle)?;
        let buffer = self
            .buffers
            .get_mut(&handle.id)
            .filter(|buffer| buffer.owner_id == owner.id)
            .ok_or(MemoryError::InvalidBuffer)?;
        let end = offset
            .checked_add(input.len())
            .filter(|end| *end <= buffer.bytes.len())
            .ok_or(MemoryError::OutOfBounds)?;
        buffer.bytes[offset..end].copy_from_slice(input);
        Ok(())
    }

    pub fn release_buffer(
        &mut self,
        owner: &MemoryOwner,
        handle: BufferHandle,
    ) -> Result<(), MemoryError> {
        self.validate_owner(owner)?;
        self.validate_handle_identity(owner, &handle)?;
        let buffer = self
            .buffers
            .get(&handle.id)
            .filter(|buffer| buffer.owner_id == owner.id)
            .ok_or(MemoryError::InvalidBuffer)?;
        let size = buffer.bytes.len();
        self.buffers.remove(&handle.id);
        self.allocated_bytes -= size;
        Ok(())
    }

    /// Drop all buffers belonging to `owner`, then invalidate that owner token.
    pub fn release_owner(&mut self, owner: MemoryOwner) -> Result<(), MemoryError> {
        self.validate_owner(&owner)?;
        self.buffers.retain(|_, buffer| {
            if buffer.owner_id == owner.id {
                self.allocated_bytes -= buffer.bytes.len();
                false
            } else {
                true
            }
        });
        self.owners.remove(&owner.id);
        Ok(())
    }

    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            owners: self.owners.len(),
            buffers: self.buffers.len(),
            allocated_bytes: self.allocated_bytes,
        }
    }

    fn validate_owner(&self, owner: &MemoryOwner) -> Result<(), MemoryError> {
        let Some(identity) = owner.arena.upgrade() else {
            return Err(MemoryError::InvalidOwner);
        };
        if !Arc::ptr_eq(&identity, &self.identity) || !self.owners.contains(&owner.id) {
            return Err(MemoryError::InvalidOwner);
        }
        Ok(())
    }

    fn validate_handle_identity(
        &self,
        owner: &MemoryOwner,
        handle: &BufferHandle,
    ) -> Result<(), MemoryError> {
        let Some(identity) = handle.arena.upgrade() else {
            return Err(MemoryError::InvalidBuffer);
        };
        if !Arc::ptr_eq(&identity, &self.identity) || handle.owner_id != owner.id {
            return Err(MemoryError::InvalidBuffer);
        }
        Ok(())
    }

    fn buffer_for<'a>(
        &'a self,
        owner: &MemoryOwner,
        handle: &BufferHandle,
    ) -> Result<&'a Buffer, MemoryError> {
        self.validate_owner(owner)?;
        self.validate_handle_identity(owner, handle)?;
        self.buffers
            .get(&handle.id)
            .filter(|buffer| buffer.owner_id == owner.id)
            .ok_or(MemoryError::InvalidBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn arena() -> MemoryArena {
        MemoryArena::new(MemoryLimits {
            max_total_bytes: 16,
            max_buffer_bytes: 8,
            max_buffers: 3,
            max_owners: 2,
        })
        .unwrap()
    }

    #[test]
    fn allocation_is_zeroed_and_writes_are_range_checked() {
        let mut arena = arena();
        let owner = arena.create_owner().unwrap();
        let buffer = arena.allocate(&owner, 4).unwrap();
        assert_eq!(arena.read(&owner, &buffer).unwrap(), [0; 4]);
        arena.write(&owner, &buffer, 1, &[7, 8]).unwrap();
        assert_eq!(arena.read(&owner, &buffer).unwrap(), [0, 7, 8, 0]);
        assert_eq!(
            arena.write(&owner, &buffer, 3, &[1, 2]),
            Err(MemoryError::OutOfBounds)
        );
        assert_eq!(
            arena.write(&owner, &buffer, usize::MAX, &[1]),
            Err(MemoryError::OutOfBounds)
        );
        assert_eq!(arena.read(&owner, &buffer).unwrap(), [0, 7, 8, 0]);
    }

    #[test]
    fn limits_are_enforced_without_partial_allocations() {
        let mut count_limited = MemoryArena::new(MemoryLimits {
            max_total_bytes: 16,
            max_buffer_bytes: 8,
            max_buffers: 2,
            max_owners: 2,
        })
        .unwrap();
        let first = count_limited.create_owner().unwrap();
        let second = count_limited.create_owner().unwrap();
        assert_eq!(
            count_limited.create_owner().unwrap_err(),
            MemoryError::OwnerLimitExceeded
        );
        assert_eq!(
            count_limited.allocate(&first, 9).unwrap_err(),
            MemoryError::BufferTooLarge
        );
        let a = count_limited.allocate(&first, 8).unwrap();
        let _b = count_limited.allocate(&second, 8).unwrap();
        assert_eq!(
            count_limited.allocate(&first, 1).unwrap_err(),
            MemoryError::BufferLimitExceeded
        );
        count_limited.release_buffer(&first, a).unwrap();
        let _c = count_limited.allocate(&first, 1).unwrap();

        let mut byte_limited = arena();
        let owner = byte_limited.create_owner().unwrap();
        let other = byte_limited.create_owner().unwrap();
        let first = byte_limited.allocate(&owner, 8).unwrap();
        let _second = byte_limited.allocate(&other, 8).unwrap();
        assert_eq!(
            byte_limited.allocate(&owner, 1).unwrap_err(),
            MemoryError::TotalLimitExceeded
        );
        byte_limited.release_buffer(&owner, first).unwrap();
        let _replacement = byte_limited.allocate(&owner, 1).unwrap();
        assert_eq!(byte_limited.stats().allocated_bytes, 9);
    }

    #[test]
    fn handles_are_confined_to_their_owner_and_arena() {
        let mut left = arena();
        let mut right = arena();
        let owner = left.create_owner().unwrap();
        let other_owner = left.create_owner().unwrap();
        let foreign_owner = right.create_owner().unwrap();
        let handle = left.allocate(&owner, 2).unwrap();
        assert_eq!(
            left.read(&other_owner, &handle),
            Err(MemoryError::InvalidBuffer)
        );
        assert_eq!(
            right.read(&foreign_owner, &handle),
            Err(MemoryError::InvalidBuffer)
        );
        assert_eq!(left.read(&owner, &handle).unwrap(), [0; 2]);
    }

    #[test]
    fn releasing_owner_drops_buffers_and_invalidates_cloned_tokens() {
        let mut arena = arena();
        let owner = arena.create_owner().unwrap();
        let stale_owner = owner.clone();
        let handle = arena.allocate(&owner, 6).unwrap();
        arena.release_owner(owner).unwrap();
        assert_eq!(
            arena.stats(),
            MemoryStats {
                owners: 0,
                buffers: 0,
                allocated_bytes: 0
            }
        );
        assert_eq!(
            arena.read(&stale_owner, &handle),
            Err(MemoryError::InvalidOwner)
        );
    }

    #[test]
    fn released_handles_cannot_be_reused() {
        let mut arena = arena();
        let owner = arena.create_owner().unwrap();
        let handle = arena.allocate(&owner, 2).unwrap();
        arena.release_buffer(&owner, handle.clone()).unwrap();
        assert_eq!(arena.read(&owner, &handle), Err(MemoryError::InvalidBuffer));
    }

    /// Run explicitly with `cargo test -- --ignored --nocapture` for a local throughput sample.
    #[test]
    #[ignore = "manual microbenchmark"]
    fn benchmark_repeated_buffer_round_trips() {
        let mut arena = MemoryArena::new(MemoryLimits {
            max_total_bytes: 8 * 1024 * 1024,
            max_buffer_bytes: 1024 * 1024,
            max_buffers: 16,
            max_owners: 2,
        })
        .unwrap();
        let owner = arena.create_owner().unwrap();
        let input = vec![0x5a; 64 * 1024];
        let started = Instant::now();
        for _ in 0..10_000 {
            let buffer = arena.allocate(&owner, input.len()).unwrap();
            arena.write(&owner, &buffer, 0, &input).unwrap();
            assert_eq!(arena.read(&owner, &buffer).unwrap(), input);
            arena.release_buffer(&owner, buffer).unwrap();
        }
        eprintln!(
            "10,000 64 KiB allocate/write/read/release cycles: {:?}",
            started.elapsed()
        );
    }
}
