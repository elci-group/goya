//! Construction of a bounded FIFO queue capability.
//!
//! The only emitted factory binds to this crate's process-local Rust runtime.
//! It does not promise cross-process sharing, host availability, or that the
//! generated crate path is linked in a consumer crate.

use std::fmt;

pub const CONTRACT: &str = "goya.compute.work_queue.bounded_process_local.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_CAPACITY: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueDeclaration {
    /// Maximum accepted queued items, from one through `MAX_CAPACITY`.
    pub capacity: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueIr {
    capacity: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// Single Rust process using `crate::compute_runtime::BoundedQueue`.
    ProcessLocal,
    CrossProcess,
    Browser,
    Node,
}

/// Explicitly reported host facilities needed by this particular lowering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub process_local_memory: bool,
    pub standard_mutex: bool,
    /// Pointer width of the target Rust process, explicitly reported by host.
    pub pointer_width_bits: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    CapacityZero,
    CapacityTooLarge,
    UnsupportedTarget,
    MissingProcessLocalMemory,
    MissingStandardMutex,
    UnsupportedPointerWidth,
    CapacityDoesNotFitTarget,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CapacityZero => "queue capacity must be positive",
            Self::CapacityTooLarge => "queue capacity exceeds the supported bound",
            Self::UnsupportedTarget => {
                "bounded queue factory supports only a process-local Rust target"
            }
            Self::MissingProcessLocalMemory => "target runtime must provide process-local memory",
            Self::MissingStandardMutex => {
                "target runtime must provide the standard mutex primitive"
            }
            Self::UnsupportedPointerWidth => "target pointer width must be 16, 32, or 64 bits",
            Self::CapacityDoesNotFitTarget => {
                "queue capacity does not fit the target pointer width"
            }
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that generated code was compiled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

impl QueueDeclaration {
    pub fn normalize(self) -> Result<QueueIr, Diagnostic> {
        if self.capacity == 0 {
            return Err(Diagnostic::CapacityZero);
        }
        if self.capacity > MAX_CAPACITY {
            return Err(Diagnostic::CapacityTooLarge);
        }
        Ok(QueueIr {
            capacity: self.capacity,
        })
    }
}

impl QueueIr {
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=process_local_rust\nqueue=fifo\ncapacity={}\noverflow=reject\nsharing=single_process\n",
            self.capacity
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
    ) -> Result<QueueArtifact, Diagnostic> {
        if target != Target::ProcessLocal {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !matches!(capabilities.pointer_width_bits, 16 | 32 | 64) {
            return Err(Diagnostic::UnsupportedPointerWidth);
        }
        let target_max = (1u128 << capabilities.pointer_width_bits) - 1;
        if self.capacity as u128 > target_max {
            return Err(Diagnostic::CapacityDoesNotFitTarget);
        }
        if !capabilities.process_local_memory {
            return Err(Diagnostic::MissingProcessLocalMemory);
        }
        if !capabilities.standard_mutex {
            return Err(Diagnostic::MissingStandardMutex);
        }
        Ok(QueueArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for_target(capabilities.pointer_width_bits),
            target_pointer_width_bits: capabilities.pointer_width_bits,
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; process-local FIFO, full admission is rejected.\npub fn make_bounded_queue() -> Result<crate::compute_runtime::BoundedQueue<u8>, crate::compute_runtime::QueueError> {{\n    crate::compute_runtime::BoundedQueue::<u8>::new({})\n}}\n",
                self.capacity
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROCESS_LOCAL: RuntimeCapabilities = RuntimeCapabilities {
        process_local_memory: true,
        standard_mutex: true,
        pointer_width_bits: 64,
    };

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = QueueDeclaration { capacity: 32 }.normalize().unwrap();
        assert_eq!(ir.capacity(), 32);
        let first = ir
            .lower_checked(Target::ProcessLocal, PROCESS_LOCAL)
            .unwrap();
        let second = ir
            .lower_checked(Target::ProcessLocal, PROCESS_LOCAL)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("capacity=32\n"));
        assert!(first
            .canonical_ir
            .contains("target_pointer_width_bits=64\n"));
        assert!(first.canonical_ir.contains("overflow=reject\n"));
        assert!(first
            .source
            .contains("crate::compute_runtime::BoundedQueue::<u8>::new(32)"));
        assert!(first.source.contains(
            "Result<crate::compute_runtime::BoundedQueue<u8>, crate::compute_runtime::QueueError>"
        ));
    }

    #[test]
    fn rejects_zero_and_over_bound_capacity() {
        assert_eq!(
            QueueDeclaration { capacity: 0 }.normalize(),
            Err(Diagnostic::CapacityZero)
        );
        assert_eq!(
            QueueDeclaration {
                capacity: MAX_CAPACITY + 1,
            }
            .normalize(),
            Err(Diagnostic::CapacityTooLarge)
        );
    }

    #[test]
    fn negotiation_rejects_other_target_classes_and_missing_facilities() {
        let ir = QueueDeclaration { capacity: 1 }.normalize().unwrap();
        for target in [Target::CrossProcess, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, PROCESS_LOCAL),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    pointer_width_bits: 64,
                    standard_mutex: true,
                    process_local_memory: false,
                }
            ),
            Err(Diagnostic::MissingProcessLocalMemory)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    process_local_memory: true,
                    standard_mutex: false,
                    pointer_width_bits: 64,
                }
            ),
            Err(Diagnostic::MissingStandardMutex)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    pointer_width_bits: 8,
                    ..PROCESS_LOCAL
                }
            ),
            Err(Diagnostic::UnsupportedPointerWidth)
        );
        assert_eq!(
            QueueDeclaration { capacity: 65_536 }
                .normalize()
                .unwrap()
                .lower_checked(
                    Target::ProcessLocal,
                    RuntimeCapabilities {
                        pointer_width_bits: 16,
                        ..PROCESS_LOCAL
                    }
                ),
            Err(Diagnostic::CapacityDoesNotFitTarget)
        );
    }

    #[test]
    fn generated_factory_matches_runtime_constructor_contract() {
        let artifact = QueueDeclaration { capacity: 4 }
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocal, PROCESS_LOCAL)
            .unwrap();
        let queue = crate::compute_runtime::BoundedQueue::<u8>::new(4).unwrap();
        assert_eq!(queue.capacity(), 4);
        assert!(artifact.source.contains("BoundedQueue::<u8>::new(4)"));
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_compute_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = QueueDeclaration { capacity: 3 }
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocal, PROCESS_LOCAL)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/compute_runtime.rs");
        assert!(
            runtime_path.is_file(),
            "compute_runtime.rs should be present next to this crate"
        );
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod compute_runtime;\n{}\nfn main() {{\n    let queue = make_bounded_queue().expect(\"factory should construct queue\");\n    assert_eq!(queue.capacity(), 3);\n    queue.try_push(41).unwrap();\n    queue.try_push(42).unwrap();\n    queue.try_push(43).unwrap();\n    assert_eq!(queue.try_push(44), Err(compute_runtime::QueueError::Full));\n    assert_eq!(queue.try_pop().unwrap(), Some(41));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-queue-factory-{}-{nonce}",
            std::process::id()
        ));
        let _cleanup = BinaryCleanup(output_path.clone());
        let mut rustc = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&output_path)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        rustc
            .stdin
            .take()
            .expect("rustc stdin")
            .write_all(harness.as_bytes())
            .expect("write generated factory harness");
        let compile = rustc.wait_with_output().expect("wait for rustc");
        assert!(
            compile.status.success(),
            "generated factory failed to compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let execution = Command::new(&output_path)
            .output()
            .expect("run generated queue factory");
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }

    /// Run with `rustc --test ... --ignored --nocapture` for a construction
    /// throughput sample; timings are local smoke measurements only.
    #[test]
    #[ignore]
    fn benchmark_queue_factory_construction() {
        let ir = QueueDeclaration { capacity: 256 }.normalize().unwrap();
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(
                ir.lower_checked(Target::ProcessLocal, PROCESS_LOCAL)
                    .unwrap(),
            );
        }
        println!(
            "100,000 queue factory constructions in {:?}",
            start.elapsed()
        );
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
