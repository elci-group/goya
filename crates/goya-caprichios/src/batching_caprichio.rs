//! A narrow process-local bounded batch builder.
//!
//! Items are copied into memory and retained until an explicit successful
//! flush. Admission is nonblocking and all-or-nothing. An empty flush is
//! distinct from a nonempty flush, and allocation failure leaves the pending
//! batch intact. This is not a distributed batcher or a catalog-wide claim.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Mutex, MutexGuard};

pub const CONTRACT: &str = "goya.compute.batching.process_local.v1";
pub const RECIPE_VERSION: &str = "0.1.0";
pub const MAX_BATCH_ITEMS: u32 = 4096;
pub const MAX_BATCH_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_ITEM_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchDeclaration {
    pub max_items: u32,
    pub max_batch_bytes: u64,
    pub max_item_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchIr {
    max_items: u32,
    max_batch_bytes: u64,
    max_item_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    CrossProcess,
    Browser,
    Node,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub process_local_memory: bool,
    pub standard_mutex: bool,
    pub pointer_width_bits: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroItemLimit,
    ZeroBatchByteLimit,
    ZeroItemByteLimit,
    ItemLimitTooLarge,
    BatchByteLimitTooLarge,
    ItemByteLimitTooLarge,
    ItemByteLimitExceedsBatchLimit,
    UnsupportedTarget,
    UnsupportedPointerWidth,
    CapacityDoesNotFitTarget,
    MissingProcessLocalMemory,
    MissingStandardMutex,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroItemLimit => "maximum batch item count must be positive",
            Self::ZeroBatchByteLimit => "maximum batch byte count must be positive",
            Self::ZeroItemByteLimit => "maximum per-item byte count must be positive",
            Self::ItemLimitTooLarge => "maximum batch item count exceeds the supported bound",
            Self::BatchByteLimitTooLarge => "maximum batch bytes exceed the supported bound",
            Self::ItemByteLimitTooLarge => "maximum item bytes exceed the supported bound",
            Self::ItemByteLimitExceedsBatchLimit => {
                "per-item byte limit exceeds total batch byte limit"
            }
            Self::UnsupportedTarget => "batch factory supports only process-local Rust",
            Self::UnsupportedPointerWidth => "target pointer width must be 32 or 64 bits",
            Self::CapacityDoesNotFitTarget => "batch limits do not fit target allocation bounds",
            Self::MissingProcessLocalMemory => "target runtime must provide process-local memory",
            Self::MissingStandardMutex => "target runtime must provide a standard mutex",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchError {
    ZeroItemLimit,
    ZeroBatchByteLimit,
    ZeroItemByteLimit,
    LimitsTooLarge,
    ItemTooLarge,
    BatchFullByItems,
    BatchFullByBytes,
    AllocationFailed,
    Poisoned,
}

impl fmt::Display for BatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroItemLimit => "batch item limit must be positive",
            Self::ZeroBatchByteLimit => "batch byte limit must be positive",
            Self::ZeroItemByteLimit => "per-item byte limit must be positive",
            Self::LimitsTooLarge => "batch limits exceed supported runtime bounds",
            Self::ItemTooLarge => "item exceeds the configured per-item byte limit",
            Self::BatchFullByItems => "batch has reached its item limit",
            Self::BatchFullByBytes => "batch has insufficient remaining byte capacity",
            Self::AllocationFailed => "batch could not reserve memory without losing pending items",
            Self::Poisoned => "batch lock was poisoned",
        })
    }
}

impl std::error::Error for BatchError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlushResult {
    Empty,
    Items(Vec<Vec<u8>>),
}

struct BatchState {
    items: VecDeque<Vec<u8>>,
    buffered_bytes: usize,
}

/// Mutex-protected FIFO batch with fixed count, total-byte and per-item bounds.
pub struct Batcher {
    max_items: usize,
    max_batch_bytes: usize,
    max_item_bytes: usize,
    state: Mutex<BatchState>,
}

impl Batcher {
    pub fn new(
        max_items: usize,
        max_batch_bytes: usize,
        max_item_bytes: usize,
    ) -> Result<Self, BatchError> {
        if max_items == 0 {
            return Err(BatchError::ZeroItemLimit);
        }
        if max_batch_bytes == 0 {
            return Err(BatchError::ZeroBatchByteLimit);
        }
        if max_item_bytes == 0 {
            return Err(BatchError::ZeroItemByteLimit);
        }
        if max_items > MAX_BATCH_ITEMS as usize
            || max_batch_bytes as u64 > MAX_BATCH_BYTES
            || max_item_bytes as u64 > MAX_ITEM_BYTES
            || max_item_bytes > max_batch_bytes
        {
            return Err(BatchError::LimitsTooLarge);
        }
        Ok(Self {
            max_items,
            max_batch_bytes,
            max_item_bytes,
            state: Mutex::new(BatchState {
                items: VecDeque::new(),
                buffered_bytes: 0,
            }),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, BatchState>, BatchError> {
        self.state.lock().map_err(|_| BatchError::Poisoned)
    }

    /// Atomically retain one copied item, or reject it without changing the batch.
    /// Empty byte strings are valid items and count against `max_items`.
    pub fn add(&self, item: &[u8]) -> Result<(), BatchError> {
        if item.len() > self.max_item_bytes {
            return Err(BatchError::ItemTooLarge);
        }
        let mut state = self.lock()?;
        if state.items.len() >= self.max_items {
            return Err(BatchError::BatchFullByItems);
        }
        let new_buffered_bytes = state
            .buffered_bytes
            .checked_add(item.len())
            .ok_or(BatchError::BatchFullByBytes)?;
        if new_buffered_bytes > self.max_batch_bytes {
            return Err(BatchError::BatchFullByBytes);
        }

        let mut owned_item = Vec::new();
        owned_item
            .try_reserve_exact(item.len())
            .map_err(|_| BatchError::AllocationFailed)?;
        owned_item.extend_from_slice(item);
        state
            .items
            .try_reserve(1)
            .map_err(|_| BatchError::AllocationFailed)?;
        state.items.push_back(owned_item);
        state.buffered_bytes = new_buffered_bytes;
        Ok(())
    }

    /// Return all pending items in FIFO order. Allocation failure is reported
    /// before any item is removed, so callers can retry the flush without loss.
    pub fn flush(&self) -> Result<FlushResult, BatchError> {
        let mut state = self.lock()?;
        if state.items.is_empty() {
            return Ok(FlushResult::Empty);
        }
        let mut items = Vec::new();
        items
            .try_reserve_exact(state.items.len())
            .map_err(|_| BatchError::AllocationFailed)?;
        items.extend(state.items.drain(..));
        state.buffered_bytes = 0;
        Ok(FlushResult::Items(items))
    }

    pub fn item_count(&self) -> Result<usize, BatchError> {
        Ok(self.lock()?.items.len())
    }

    pub fn buffered_bytes(&self) -> Result<usize, BatchError> {
        Ok(self.lock()?.buffered_bytes)
    }

    pub fn max_items(&self) -> usize {
        self.max_items
    }

    pub fn max_batch_bytes(&self) -> usize {
        self.max_batch_bytes
    }

    pub fn max_item_bytes(&self) -> usize {
        self.max_item_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

impl BatchDeclaration {
    pub fn normalize(self) -> Result<BatchIr, Diagnostic> {
        if self.max_items == 0 {
            return Err(Diagnostic::ZeroItemLimit);
        }
        if self.max_batch_bytes == 0 {
            return Err(Diagnostic::ZeroBatchByteLimit);
        }
        if self.max_item_bytes == 0 {
            return Err(Diagnostic::ZeroItemByteLimit);
        }
        if self.max_items > MAX_BATCH_ITEMS {
            return Err(Diagnostic::ItemLimitTooLarge);
        }
        if self.max_batch_bytes > MAX_BATCH_BYTES {
            return Err(Diagnostic::BatchByteLimitTooLarge);
        }
        if self.max_item_bytes > MAX_ITEM_BYTES {
            return Err(Diagnostic::ItemByteLimitTooLarge);
        }
        if self.max_item_bytes > self.max_batch_bytes {
            return Err(Diagnostic::ItemByteLimitExceedsBatchLimit);
        }
        Ok(BatchIr {
            max_items: self.max_items,
            max_batch_bytes: self.max_batch_bytes,
            max_item_bytes: self.max_item_bytes,
        })
    }
}

impl BatchIr {
    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=process_local_rust\nbatch=fifo_items\nmax_items={}\nmax_batch_bytes={}\nmax_item_bytes={}\nadmission=atomic_reject_on_limit\nflush=fifo_all_or_preserve_on_allocation_error\nempty_flush=explicit_empty\n",
            self.max_items, self.max_batch_bytes, self.max_item_bytes
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BatchArtifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !matches!(capabilities.pointer_width_bits, 32 | 64) {
            return Err(Diagnostic::UnsupportedPointerWidth);
        }
        let target_isize_max = match capabilities.pointer_width_bits {
            32 => i32::MAX as u64,
            64 => i64::MAX as u64,
            _ => unreachable!("pointer width validated above"),
        };
        if u64::from(self.max_items) > target_isize_max
            || self.max_batch_bytes > target_isize_max
            || self.max_item_bytes > target_isize_max
        {
            return Err(Diagnostic::CapacityDoesNotFitTarget);
        }
        if !capabilities.process_local_memory {
            return Err(Diagnostic::MissingProcessLocalMemory);
        }
        if !capabilities.standard_mutex {
            return Err(Diagnostic::MissingStandardMutex);
        }
        Ok(BatchArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            target_pointer_width_bits: capabilities.pointer_width_bits,
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; process-local bounded FIFO batch.\npub fn make_batcher() -> Result<crate::batching_caprichio::Batcher, crate::batching_caprichio::BatchError> {{\n    crate::batching_caprichio::Batcher::new({}usize, {}usize, {}usize)\n}}\n",
                self.max_items, self.max_batch_bytes, self.max_item_bytes
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: RuntimeCapabilities = RuntimeCapabilities {
        process_local_memory: true,
        standard_mutex: true,
        pointer_width_bits: 64,
    };

    fn declaration() -> BatchDeclaration {
        BatchDeclaration {
            max_items: 3,
            max_batch_bytes: 10,
            max_item_bytes: 6,
        }
    }

    #[test]
    fn add_rejects_overflow_without_losing_pending_items() {
        let batch = Batcher::new(2, 5, 4).unwrap();
        batch.add(b"ab").unwrap();
        batch.add(b"cd").unwrap();
        assert_eq!(batch.add(b"e"), Err(BatchError::BatchFullByItems));
        assert_eq!(batch.item_count().unwrap(), 2);
        assert_eq!(batch.buffered_bytes().unwrap(), 4);
        assert_eq!(
            batch.flush().unwrap(),
            FlushResult::Items(vec![b"ab".to_vec(), b"cd".to_vec()])
        );

        batch.add(b"1234").unwrap();
        assert_eq!(batch.add(b"xy"), Err(BatchError::BatchFullByBytes));
        assert_eq!(batch.item_count().unwrap(), 1);
        assert_eq!(batch.buffered_bytes().unwrap(), 4);
        assert_eq!(
            batch.flush().unwrap(),
            FlushResult::Items(vec![b"1234".to_vec()])
        );
        assert_eq!(batch.add(b"12345"), Err(BatchError::ItemTooLarge));
    }

    #[test]
    fn empty_items_count_and_flush_is_explicit_and_fifo() {
        let batch = Batcher::new(2, 4, 4).unwrap();
        assert_eq!(batch.flush().unwrap(), FlushResult::Empty);
        batch.add(b"").unwrap();
        batch.add(b"x").unwrap();
        assert_eq!(batch.item_count().unwrap(), 2);
        assert_eq!(batch.buffered_bytes().unwrap(), 1);
        assert_eq!(batch.add(b"y"), Err(BatchError::BatchFullByItems));
        assert_eq!(
            batch.flush().unwrap(),
            FlushResult::Items(vec![Vec::new(), b"x".to_vec()])
        );
        assert_eq!(batch.item_count().unwrap(), 0);
        assert_eq!(batch.buffered_bytes().unwrap(), 0);
        assert_eq!(batch.flush().unwrap(), FlushResult::Empty);
    }

    #[test]
    fn direct_runtime_constructor_enforces_hard_bounds() {
        assert_eq!(Batcher::new(0, 1, 1).err(), Some(BatchError::ZeroItemLimit));
        assert_eq!(
            Batcher::new(1, 0, 1).err(),
            Some(BatchError::ZeroBatchByteLimit)
        );
        assert_eq!(
            Batcher::new(1, 1, 0).err(),
            Some(BatchError::ZeroItemByteLimit)
        );
        assert_eq!(
            Batcher::new(MAX_BATCH_ITEMS as usize + 1, 1, 1).err(),
            Some(BatchError::LimitsTooLarge)
        );
        assert_eq!(
            Batcher::new(1, MAX_BATCH_BYTES as usize + 1, 1).err(),
            Some(BatchError::LimitsTooLarge)
        );
        assert_eq!(
            Batcher::new(1, 1, 2).err(),
            Some(BatchError::LimitsTooLarge)
        );
    }

    #[test]
    fn declaration_and_target_negotiation_fail_closed() {
        assert_eq!(
            BatchDeclaration {
                max_items: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroItemLimit)
        );
        assert_eq!(
            BatchDeclaration {
                max_batch_bytes: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroBatchByteLimit)
        );
        assert_eq!(
            BatchDeclaration {
                max_item_bytes: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroItemByteLimit)
        );
        assert_eq!(
            BatchDeclaration {
                max_item_bytes: 11,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ItemByteLimitExceedsBatchLimit)
        );
        assert_eq!(
            BatchDeclaration {
                max_items: MAX_BATCH_ITEMS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ItemLimitTooLarge)
        );
        assert_eq!(
            BatchDeclaration {
                max_batch_bytes: MAX_BATCH_BYTES + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BatchByteLimitTooLarge)
        );
        assert_eq!(
            BatchDeclaration {
                max_item_bytes: MAX_ITEM_BYTES + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ItemByteLimitTooLarge)
        );

        let ir = declaration().normalize().unwrap();
        assert!(BatchDeclaration {
            max_items: 100,
            max_batch_bytes: 1,
            max_item_bytes: 1,
        }
        .normalize()
        .is_ok());
        for target in [Target::CrossProcess, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, LOCAL),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    pointer_width_bits: 16,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::UnsupportedPointerWidth)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    process_local_memory: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingProcessLocalMemory)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    standard_mutex: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingStandardMutex)
        );
    }

    #[test]
    fn canonical_ir_and_generated_source_are_deterministic() {
        let ir = declaration().normalize().unwrap();
        let first = ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap();
        let second = ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert_eq!(first.target_pointer_width_bits, 64);
        assert!(first.canonical_ir.contains("max_items=3\n"));
        assert!(first.canonical_ir.contains("max_batch_bytes=10\n"));
        assert!(first.canonical_ir.contains("max_item_bytes=6\n"));
        assert!(first
            .source
            .contains("Batcher::new(3usize, 10usize, 6usize)"));
    }

    #[test]
    fn generated_binding_compiles_and_runs_without_batch_loss() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, LOCAL)
            .unwrap();
        let module =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/batching_caprichio.rs");
        let harness = format!(
            "#[path = {module:?}] pub mod batching_caprichio;\n{}\nfn main() {{\n    let batch = make_batcher().unwrap();\n    assert_eq!(batch.max_items(), 3);\n    batch.add(b\"one\").unwrap();\n    batch.add(b\"two\").unwrap();\n    batch.add(b\"tri\").unwrap();\n    assert_eq!(batch.add(b\"four\"), Err(batching_caprichio::BatchError::BatchFullByItems));\n    assert_eq!(batch.item_count().unwrap(), 3);\n    assert_eq!(batch.flush().unwrap(), batching_caprichio::FlushResult::Items(vec![b\"one\".to_vec(), b\"two\".to_vec(), b\"tri\".to_vec()]));\n    assert_eq!(batch.flush().unwrap(), batching_caprichio::FlushResult::Empty);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let binary = std::env::temp_dir().join(format!(
            "goya-generated-batch-factory-{}-{nonce}",
            std::process::id()
        ));
        let _cleanup = BinaryCleanup(binary.clone());
        let mut compiler = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&binary)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let compiled = compiler.wait_with_output().unwrap();
        assert!(
            compiled.status.success(),
            "generated batch factory failed to compile:\n{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let executed = Command::new(&binary).output().unwrap();
        assert!(
            executed.status.success(),
            "generated batch factory failed at runtime:\n{}",
            String::from_utf8_lossy(&executed.stderr)
        );
    }

    #[test]
    #[ignore = "construction microbenchmark"]
    fn benchmark_batch_factory_construction() {
        let ir = declaration().normalize().unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap());
        }
        println!(
            "100,000 batch factory constructions in {:?}",
            start.elapsed()
        );
    }

    #[test]
    #[ignore = "runtime microbenchmark"]
    fn benchmark_batch_runtime_add_flush() {
        let batch = Batcher::new(128, 4096, 32).unwrap();
        let item = [0x5a; 32];
        let start = std::time::Instant::now();
        for _ in 0..10_000 {
            for _ in 0..128 {
                batch.add(&item).unwrap();
            }
            std::hint::black_box(batch.flush().unwrap());
        }
        println!("10,000 128-item batches in {:?}", start.elapsed());
    }
}

#[cfg(test)]
struct BinaryCleanup(std::path::PathBuf);

#[cfg(test)]
impl Drop for BinaryCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
