//! Construction of bounded process-local parallel execution for finite work.
//!
//! Independent inputs are processed by a bounded set of Rust threads. Each
//! callback error is retained at its input position; callback panics are
//! caught and reported per item. This is cooperative work execution and makes
//! no scheduling or timing guarantee.

use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub const CONTRACT: &str = "goya.compute.parallelism.process_local_rust.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORK_ITEMS: u64 = 100_000;
pub const MAX_CONCURRENCY: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelismDeclaration {
    /// Maximum number of finite input items accepted by one execution.
    pub max_items: usize,
    /// Maximum number of simultaneously executing callback threads.
    pub max_concurrency: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelismIr {
    max_items: usize,
    max_concurrency: usize,
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
    pub standard_threads: bool,
    /// Panic catching requires a Rust target built with unwind semantics.
    pub panic_unwind: bool,
    /// Explicitly reported pointer width of the Rust target.
    pub target_pointer_width_bits: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroItemLimit,
    TooManyItems,
    ZeroConcurrency,
    ConcurrencyTooLarge,
    UnsupportedTarget,
    MissingProcessLocalMemory,
    MissingStandardThreads,
    MissingPanicUnwind,
    UnsupportedPointerWidth,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroItemLimit => "maximum item count must be positive",
            Self::TooManyItems => "maximum item count exceeds the supported bound",
            Self::ZeroConcurrency => "maximum concurrency must be positive",
            Self::ConcurrencyTooLarge => "maximum concurrency exceeds the supported bound",
            Self::UnsupportedTarget => "parallelism supports only process-local Rust",
            Self::MissingProcessLocalMemory => "target runtime must provide process-local memory",
            Self::MissingStandardThreads => "target runtime must provide standard threads",
            Self::MissingPanicUnwind => "target must support panic unwinding for per-item capture",
            Self::UnsupportedPointerWidth => "target pointer width must be 32 or 64 bits",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelismError {
    ZeroItemLimit,
    TooManyItems,
    ZeroConcurrency,
    ConcurrencyTooLarge,
    InputExceedsLimit,
    AllocationFailed,
    ThreadSpawnFailed,
    WorkerPanicked,
    UnsupportedPointerWidth,
    TargetPointerWidthMismatch,
    PanicUnwindUnavailable,
}

impl fmt::Display for ParallelismError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroItemLimit => "maximum item count must be positive",
            Self::TooManyItems => "maximum item count exceeds the supported bound",
            Self::ZeroConcurrency => "maximum concurrency must be positive",
            Self::ConcurrencyTooLarge => "maximum concurrency exceeds the supported bound",
            Self::InputExceedsLimit => "input item count exceeds the declared maximum",
            Self::AllocationFailed => "unable to allocate bounded parallel execution state",
            Self::ThreadSpawnFailed => "unable to start a worker thread",
            Self::WorkerPanicked => "parallel worker failed outside the item callback",
            Self::UnsupportedPointerWidth => "runtime pointer width must be 32 or 64 bits",
            Self::TargetPointerWidthMismatch => {
                "runtime pointer width differs from the negotiated target"
            }
            Self::PanicUnwindUnavailable => {
                "runtime was compiled with panic=abort; per-item panic capture is unavailable"
            }
        })
    }
}

impl std::error::Error for ParallelismError {}

#[derive(Debug, Eq, PartialEq)]
pub enum ItemFailure<E> {
    Callback(E),
    Panicked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParallelismArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelExecutor {
    max_items: usize,
    max_concurrency: usize,
}

impl ParallelExecutor {
    pub fn new(max_items: usize, max_concurrency: usize) -> Result<Self, ParallelismError> {
        Self::new_for_target(max_items, max_concurrency, usize::BITS as u8)
    }

    pub fn new_for_target(
        max_items: usize,
        max_concurrency: usize,
        target_pointer_width_bits: u8,
    ) -> Result<Self, ParallelismError> {
        if !cfg!(panic = "unwind") {
            return Err(ParallelismError::PanicUnwindUnavailable);
        }
        let actual_pointer_width_bits = usize::BITS as u8;
        if !matches!(target_pointer_width_bits, 32 | 64)
            || !matches!(actual_pointer_width_bits, 32 | 64)
        {
            return Err(ParallelismError::UnsupportedPointerWidth);
        }
        if target_pointer_width_bits != actual_pointer_width_bits {
            return Err(ParallelismError::TargetPointerWidthMismatch);
        }
        if max_items == 0 {
            return Err(ParallelismError::ZeroItemLimit);
        }
        if (max_items as u128) > (MAX_WORK_ITEMS as u128) {
            return Err(ParallelismError::TooManyItems);
        }
        if max_concurrency == 0 {
            return Err(ParallelismError::ZeroConcurrency);
        }
        if max_concurrency > MAX_CONCURRENCY {
            return Err(ParallelismError::ConcurrencyTooLarge);
        }
        Ok(Self {
            max_items,
            max_concurrency,
        })
    }

    pub fn max_items(&self) -> usize {
        self.max_items
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    /// Executes each input once, returning outcomes in input order.
    /// Callback errors and panics are item outcomes; worker infrastructure
    /// failures abort the whole execution with an error. If infrastructure
    /// fails, already-running callbacks may have completed side effects and
    /// no all-or-nothing guarantee applies.
    pub fn execute<T, R, E, F>(
        &self,
        items: &[T],
        callback: F,
    ) -> Result<Vec<Result<R, ItemFailure<E>>>, ParallelismError>
    where
        T: Sync,
        R: Send,
        E: Send,
        F: Fn(&T) -> Result<R, E> + Sync,
    {
        if items.len() > self.max_items {
            return Err(ParallelismError::InputExceedsLimit);
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(items.len())
            .map_err(|_| ParallelismError::AllocationFailed)?;
        slots.resize_with(items.len(), || Mutex::new(None));
        let next = AtomicUsize::new(0);
        let worker_count = self.max_concurrency.min(items.len());

        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            handles
                .try_reserve_exact(worker_count)
                .map_err(|_| ParallelismError::AllocationFailed)?;
            for _ in 0..worker_count {
                let spawned = std::thread::Builder::new().spawn_scoped(scope, || loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= items.len() {
                        break;
                    }
                    let outcome = match catch_unwind(AssertUnwindSafe(|| callback(&items[index]))) {
                        Ok(Ok(value)) => Ok(value),
                        Ok(Err(error)) => Err(ItemFailure::Callback(error)),
                        Err(_) => Err(ItemFailure::Panicked),
                    };
                    let mut slots = slots[index]
                        .lock()
                        .unwrap_or_else(|_| panic!("result slot mutex poisoned"));
                    *slots = Some(outcome);
                });
                match spawned {
                    Ok(handle) => handles.push(handle),
                    Err(_) => {
                        let mut worker_panicked = false;
                        for handle in handles {
                            worker_panicked |= handle.join().is_err();
                        }
                        if worker_panicked {
                            return Err(ParallelismError::WorkerPanicked);
                        }
                        return Err(ParallelismError::ThreadSpawnFailed);
                    }
                }
            }
            let mut worker_panicked = false;
            for handle in handles {
                worker_panicked |= handle.join().is_err();
            }
            if worker_panicked {
                return Err(ParallelismError::WorkerPanicked);
            }
            Ok(())
        })?;

        let mut outcomes = Vec::new();
        outcomes
            .try_reserve_exact(items.len())
            .map_err(|_| ParallelismError::AllocationFailed)?;
        for slot in slots {
            let value = slot
                .into_inner()
                .map_err(|_| ParallelismError::WorkerPanicked)?
                .ok_or(ParallelismError::WorkerPanicked)?;
            outcomes.push(value);
        }
        Ok(outcomes)
    }
}

impl ParallelismDeclaration {
    pub fn normalize(self) -> Result<ParallelismIr, Diagnostic> {
        if self.max_items == 0 {
            return Err(Diagnostic::ZeroItemLimit);
        }
        if (self.max_items as u128) > (MAX_WORK_ITEMS as u128) {
            return Err(Diagnostic::TooManyItems);
        }
        if self.max_concurrency == 0 {
            return Err(Diagnostic::ZeroConcurrency);
        }
        if self.max_concurrency > MAX_CONCURRENCY {
            return Err(Diagnostic::ConcurrencyTooLarge);
        }
        Ok(ParallelismIr {
            max_items: self.max_items,
            max_concurrency: self.max_concurrency,
        })
    }
}

impl ParallelismIr {
    pub fn max_items(&self) -> usize {
        self.max_items
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=process_local_rust\nmax_items={}\nmax_concurrency={}\ncallback_errors=preserve_input_order\ncallback_panics=capture_per_item\npanic_profile=unwind_required\nworker_infrastructure_errors=abort\nscheduling_guarantee=none\n",
            self.max_items, self.max_concurrency
        )
    }

    fn canonical_for_target(&self, pointer_width_bits: u8) -> String {
        format!(
            "{}target_pointer_width_bits={}\n",
            self.canonical(),
            pointer_width_bits
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<ParallelismArtifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !matches!(capabilities.target_pointer_width_bits, 32 | 64) {
            return Err(Diagnostic::UnsupportedPointerWidth);
        }
        if !capabilities.process_local_memory {
            return Err(Diagnostic::MissingProcessLocalMemory);
        }
        if !capabilities.standard_threads {
            return Err(Diagnostic::MissingStandardThreads);
        }
        if !capabilities.panic_unwind {
            return Err(Diagnostic::MissingPanicUnwind);
        }
        Ok(ParallelismArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for_target(capabilities.target_pointer_width_bits),
            target_pointer_width_bits: capabilities.target_pointer_width_bits,
            source: format!(
                "// {CONTRACT}; bounded process-local workers; no scheduling guarantee.\npub fn make_parallel_executor() -> Result<crate::parallelism_caprichio::ParallelExecutor, crate::parallelism_caprichio::ParallelismError> {{\n    crate::parallelism_caprichio::ParallelExecutor::new_for_target({}, {}, {}u8)\n}}\n",
                self.max_items, self.max_concurrency, capabilities.target_pointer_width_bits
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const LOCAL: RuntimeCapabilities = RuntimeCapabilities {
        process_local_memory: true,
        standard_threads: true,
        panic_unwind: true,
        target_pointer_width_bits: usize::BITS as u8,
    };

    fn declaration() -> ParallelismDeclaration {
        ParallelismDeclaration {
            max_items: 128,
            max_concurrency: 4,
        }
    }

    #[test]
    fn declaration_normalizes_and_factory_is_deterministic() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.max_items(), 128);
        assert_eq!(ir.max_concurrency(), 4);
        let first = ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap();
        assert_eq!(
            first,
            ir.lower_checked(Target::ProcessLocalRust, LOCAL).unwrap()
        );
        assert_eq!(first.contract, CONTRACT);
        assert!(first
            .canonical_ir
            .contains("callback_panics=capture_per_item\n"));
        assert!(first
            .canonical_ir
            .contains("panic_profile=unwind_required\n"));
        assert!(first.canonical_ir.contains("scheduling_guarantee=none\n"));
        assert!(first
            .canonical_ir
            .contains(&format!("target_pointer_width_bits={}\n", usize::BITS)));
        assert_eq!(first.target_pointer_width_bits, usize::BITS as u8);
        assert!(first.source.contains(&format!(
            "ParallelExecutor::new_for_target(128, 4, {}u8)",
            usize::BITS
        )));
    }

    #[test]
    #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
    fn declaration_and_target_checks_fail_closed() {
        assert_eq!(
            ParallelismDeclaration {
                max_items: 0,
                max_concurrency: 1
            }
            .normalize(),
            Err(Diagnostic::ZeroItemLimit)
        );
        assert_eq!(
            ParallelismDeclaration {
                max_items: usize::try_from(MAX_WORK_ITEMS + 1).unwrap(),
                max_concurrency: 1
            }
            .normalize(),
            Err(Diagnostic::TooManyItems)
        );
        assert_eq!(
            ParallelismDeclaration {
                max_items: 1,
                max_concurrency: 0
            }
            .normalize(),
            Err(Diagnostic::ZeroConcurrency)
        );
        assert_eq!(
            ParallelismDeclaration {
                max_items: 1,
                max_concurrency: MAX_CONCURRENCY + 1
            }
            .normalize(),
            Err(Diagnostic::ConcurrencyTooLarge)
        );
        let ir = declaration().normalize().unwrap();
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
                    standard_threads: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingStandardThreads)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    panic_unwind: false,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::MissingPanicUnwind)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                RuntimeCapabilities {
                    target_pointer_width_bits: 16,
                    ..LOCAL
                }
            ),
            Err(Diagnostic::UnsupportedPointerWidth)
        );
        assert_eq!(
            ParallelExecutor::new_for_target(8, 2, 16),
            Err(ParallelismError::UnsupportedPointerWidth)
        );
        let wrong_target_width = if usize::BITS == 64 { 32 } else { 64 };
        assert_eq!(
            ParallelExecutor::new_for_target(8, 2, wrong_target_width),
            Err(ParallelismError::TargetPointerWidthMismatch)
        );
    }

    #[test]
    fn execution_bounds_concurrency_and_preserves_order_across_errors_and_panics() {
        let executor = ParallelExecutor::new(64, 4).unwrap();
        let items: Vec<usize> = (0..64).collect();
        let active = AtomicUsize::new(0);
        let high_water = AtomicUsize::new(0);
        let results = executor
            .execute(&items, |item| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                high_water.fetch_max(now, Ordering::SeqCst);
                struct Active<'a>(&'a AtomicUsize);
                impl Drop for Active<'_> {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                let _active = Active(&active);
                std::thread::sleep(Duration::from_millis(1));
                match *item {
                    7 => Err("callback-error"),
                    11 => panic!("callback-panic"),
                    value => Ok(value * 2),
                }
            })
            .unwrap();

        assert!(high_water.load(Ordering::SeqCst) <= 4);
        assert!(high_water.load(Ordering::SeqCst) > 1);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(results.len(), items.len());
        for (index, result) in results.iter().enumerate() {
            match index {
                7 => assert_eq!(result, &Err(ItemFailure::Callback("callback-error"))),
                11 => assert_eq!(result, &Err(ItemFailure::Panicked)),
                other => assert_eq!(result, &Ok(other * 2)),
            }
        }
        assert_eq!(
            ParallelExecutor::new(2, 2)
                .unwrap()
                .execute(&[1, 2, 3], |value| Ok::<_, ()>(*value)),
            Err(ParallelismError::InputExceedsLimit)
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs() {
        use std::process::Command;
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, LOCAL)
            .unwrap();
        let manifest_dir = option_env!("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap()
                    .join("crates/goya-caprichios")
            });
        let source_path = manifest_dir.join("src/parallelism_caprichio.rs");
        let harness = format!(
            "#[path = {source_path:?}]\nmod parallelism_caprichio;\n{}\nfn main() {{ let executor = make_parallel_executor().unwrap(); let values = [1, 2, 3, 4]; let results = executor.execute(&values, |x| Ok::<_, ()>(x * 3)).unwrap(); assert_eq!(results, [Ok(3), Ok(6), Ok(9), Ok(12)]); }}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let executable = std::env::temp_dir().join(format!(
            "goya-parallelism-factory-{}-{nonce}",
            std::process::id()
        ));
        let _cleanup = BinaryCleanup(executable.clone());
        let output = Command::new("rustc")
            .arg("--edition=2021")
            .arg("-")
            .arg("-o")
            .arg(&executable)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(harness.as_bytes())?;
                child.wait_with_output()
            })
            .expect("rustc should compile emitted factory");
        assert!(
            output.status.success(),
            "rustc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let run = Command::new(&executable)
            .output()
            .expect("generated factory should execute");
        assert!(
            run.status.success(),
            "generated program failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    #[test]
    fn panic_abort_runtime_constructor_returns_typed_error() {
        use std::process::Command;
        use std::time::{SystemTime, UNIX_EPOCH};

        let manifest_dir = option_env!("CARGO_MANIFEST_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap()
                    .join("crates/goya-caprichios")
            });
        let source_path = manifest_dir.join("src/parallelism_caprichio.rs");
        let fixture = format!(
            "#[path = {source_path:?}]\nmod parallelism_caprichio;\nfn main() {{ assert_eq!(parallelism_caprichio::ParallelExecutor::new_for_target(2, 2, usize::BITS as u8), Err(parallelism_caprichio::ParallelismError::PanicUnwindUnavailable)); }}\n"
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let executable = std::env::temp_dir().join(format!(
            "goya-parallelism-abort-profile-{}-{nonce}",
            std::process::id()
        ));
        let _cleanup = BinaryCleanup(executable.clone());
        let output = Command::new("rustc")
            .arg("--edition=2021")
            .arg("-C")
            .arg("panic=abort")
            .arg("-")
            .arg("-o")
            .arg(&executable)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(fixture.as_bytes())?;
                child.wait_with_output()
            })
            .expect("rustc should compile abort-profile fixture");
        assert!(
            output.status.success(),
            "rustc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let run = Command::new(&executable)
            .output()
            .expect("abort-profile fixture should run");
        assert!(
            run.status.success(),
            "runtime should return a typed error, not abort: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    struct BinaryCleanup(std::path::PathBuf);
    impl Drop for BinaryCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    #[ignore = "manual process-local execution microbenchmark"]
    fn benchmark_bounded_parallel_execution() {
        let executor = ParallelExecutor::new(10_000, 4).unwrap();
        let values: Vec<usize> = (0..10_000).collect();
        let start = std::time::Instant::now();
        let results = executor
            .execute(&values, |value| Ok::<_, ()>(value + 1))
            .unwrap();
        eprintln!("10k parallel callback executions: {:?}", start.elapsed());
        std::hint::black_box(results);
    }
}
