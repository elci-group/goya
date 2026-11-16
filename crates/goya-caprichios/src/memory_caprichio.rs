//! Deterministic construction of a bounded, process-local memory arena.
//!
//! The emitted factory binds only to this crate's `memory_runtime::MemoryArena` and
//! is negotiated for an explicit pointer width matching the compilation target.
//! It does not claim persistence, shared memory, hardware access, or a system memory
//! reservation. The declared limits bound accepted arena allocations at runtime.

use std::fmt;

pub const CONTRACT: &str = "goya.memory.buffers.process_local.v1";
pub const RECIPE: &str = "rust-process-local-memory";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocal,
    Browser,
    Node,
    Native,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Indicates that the generated factory can reference `memory_runtime`.
    pub memory_runtime: bool,
    /// Pointer width of the target that will compile and run the generated factory.
    pub pointer_width_bits: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryDeclaration {
    pub max_total_bytes: u64,
    pub max_buffer_bytes: u64,
    pub max_buffers: u64,
    pub max_owners: u64,
    /// Requested behavior beyond ordinary local process memory.
    pub shared_across_processes: bool,
    pub persistent: bool,
    pub hardware_backed: bool,
    pub zero_copy: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryIr {
    max_total_bytes: u64,
    max_buffer_bytes: u64,
    max_buffers: u64,
    max_owners: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic(pub &'static str);

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryPlan {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryArtifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl MemoryDeclaration {
    pub fn normalize(self) -> Result<MemoryIr, Diagnostic> {
        if self.max_total_bytes == 0
            || self.max_buffer_bytes == 0
            || self.max_buffers == 0
            || self.max_owners == 0
        {
            return Err(Diagnostic("memory limits must be positive"));
        }
        if self.max_buffer_bytes > self.max_total_bytes {
            return Err(Diagnostic(
                "maximum buffer size cannot exceed maximum total bytes",
            ));
        }
        if self.shared_across_processes {
            return Err(Diagnostic("shared memory is not supported"));
        }
        if self.persistent {
            return Err(Diagnostic("persistent memory is not supported"));
        }
        if self.hardware_backed {
            return Err(Diagnostic("hardware-backed memory is not supported"));
        }
        if self.zero_copy {
            return Err(Diagnostic("zero-copy buffers are not supported"));
        }
        Ok(MemoryIr {
            max_total_bytes: self.max_total_bytes,
            max_buffer_bytes: self.max_buffer_bytes,
            max_buffers: self.max_buffers,
            max_owners: self.max_owners,
        })
    }
}

impl MemoryIr {
    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nmax_total_bytes={}\nmax_buffer_bytes={}\nmax_buffers={}\nmax_owners={}\nshared_across_processes=false\npersistent=false\nhardware_backed=false\nzero_copy=false\n",
            self.max_total_bytes,
            self.max_buffer_bytes,
            self.max_buffers,
            self.max_owners
        )
    }

    pub fn negotiate(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<MemoryPlan, Diagnostic> {
        if target != Target::ProcessLocal {
            return Err(Diagnostic(
                "only the process-local memory target is supported",
            ));
        }
        if !capabilities.memory_runtime {
            return Err(Diagnostic("target runtime requires memory_runtime"));
        }
        let pointer_max = match capabilities.pointer_width_bits {
            16 => u16::MAX as u64,
            32 => u32::MAX as u64,
            64 => u64::MAX,
            _ => {
                return Err(Diagnostic(
                    "target pointer width must be 16, 32, or 64 bits",
                ))
            }
        };
        if [
            self.max_total_bytes,
            self.max_buffer_bytes,
            self.max_buffers,
            self.max_owners,
        ]
        .into_iter()
        .any(|limit| limit > pointer_max)
        {
            return Err(Diagnostic(
                "memory limit cannot be represented by target pointer width",
            ));
        }
        Ok(MemoryPlan {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
        })
    }

    pub fn lower(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<MemoryArtifact, Diagnostic> {
        let plan = self.negotiate(target, capabilities)?;
        let source = format!(
            "// {contract}; recipe={recipe}@{version}\nuse crate::memory_runtime::{{MemoryArena, MemoryError, MemoryLimits}};\n\npub const MEMORY_CONTRACT: &str = \"{contract}\";\npub const MEMORY_RECIPE_VERSION: &str = \"{version}\";\n\npub fn create_memory_arena() -> Result<MemoryArena, MemoryError> {{\n    MemoryArena::new(MemoryLimits {{\n        max_total_bytes: {max_total}usize,\n        max_buffer_bytes: {max_buffer}usize,\n        max_buffers: {max_buffers}usize,\n        max_owners: {max_owners}usize,\n    }})\n}}\n",
            contract = plan.contract,
            recipe = plan.recipe,
            version = plan.recipe_version,
            max_total = self.max_total_bytes,
            max_buffer = self.max_buffer_bytes,
            max_buffers = self.max_buffers,
            max_owners = self.max_owners,
        );
        Ok(MemoryArtifact {
            contract: plan.contract,
            recipe: plan.recipe,
            recipe_version: plan.recipe_version,
            canonical_ir: plan.canonical_ir,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn declaration() -> MemoryDeclaration {
        MemoryDeclaration {
            max_total_bytes: 1_048_576,
            max_buffer_bytes: 65_536,
            max_buffers: 32,
            max_owners: 8,
            shared_across_processes: false,
            persistent: false,
            hardware_backed: false,
            zero_copy: false,
        }
    }

    fn capabilities() -> RuntimeCapabilities {
        RuntimeCapabilities {
            memory_runtime: true,
            pointer_width_bits: usize::BITS,
        }
    }

    #[test]
    fn normalization_rejects_invalid_limits_and_unsupported_requirements() {
        let mut d = declaration();
        d.max_total_bytes = 0;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("memory limits must be positive"))
        );
        d = declaration();
        d.max_buffer_bytes = d.max_total_bytes + 1;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic(
                "maximum buffer size cannot exceed maximum total bytes"
            ))
        );
        let mut d = declaration();
        d.shared_across_processes = true;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("shared memory is not supported"))
        );
        let mut d = declaration();
        d.persistent = true;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("persistent memory is not supported"))
        );
        let mut d = declaration();
        d.hardware_backed = true;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("hardware-backed memory is not supported"))
        );
        let mut d = declaration();
        d.zero_copy = true;
        assert_eq!(
            d.normalize(),
            Err(Diagnostic("zero-copy buffers are not supported"))
        );
    }

    #[test]
    fn canonical_form_is_stable_and_records_all_limits() {
        let ir = declaration().normalize().unwrap();
        let expected = "contract=goya.memory.buffers.process_local.v1\nmax_total_bytes=1048576\nmax_buffer_bytes=65536\nmax_buffers=32\nmax_owners=8\nshared_across_processes=false\npersistent=false\nhardware_backed=false\nzero_copy=false\n";
        assert_eq!(ir.canonical(), expected);
        assert_eq!(ir, declaration().normalize().unwrap());
        let mut changed = declaration();
        changed.max_buffers += 1;
        assert_ne!(ir.canonical(), changed.normalize().unwrap().canonical());
    }

    #[test]
    fn negotiation_fails_closed_for_unsupported_targets_and_missing_runtime() {
        let ir = declaration().normalize().unwrap();
        for target in [Target::Browser, Target::Node, Target::Native] {
            assert_eq!(
                ir.negotiate(target, capabilities()),
                Err(Diagnostic(
                    "only the process-local memory target is supported"
                ))
            );
        }
        assert_eq!(
            ir.negotiate(Target::ProcessLocal, RuntimeCapabilities::default()),
            Err(Diagnostic("target runtime requires memory_runtime"))
        );
        assert_eq!(
            ir.negotiate(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    memory_runtime: true,
                    pointer_width_bits: 0,
                }
            ),
            Err(Diagnostic(
                "target pointer width must be 16, 32, or 64 bits"
            ))
        );
        assert_eq!(
            ir.negotiate(Target::ProcessLocal, capabilities())
                .unwrap()
                .recipe,
            RECIPE
        );
    }

    #[test]
    fn lowering_is_reproducible_and_binds_the_validated_limits() {
        let ir = declaration().normalize().unwrap();
        let artifact = ir.lower(Target::ProcessLocal, capabilities()).unwrap();
        assert_eq!(
            artifact,
            ir.lower(Target::ProcessLocal, capabilities()).unwrap()
        );
        assert_eq!(artifact.contract, CONTRACT);
        assert_eq!(artifact.recipe_version, RECIPE_VERSION);
        assert_eq!(artifact.canonical_ir, ir.canonical());
        assert!(artifact.source.contains("MemoryArena::new(MemoryLimits"));
        assert!(artifact.source.contains("max_total_bytes: 1048576usize"));
        assert!(artifact.source.contains("max_buffer_bytes: 65536usize"));
        assert!(artifact.source.contains("max_buffers: 32usize"));
        assert!(artifact.source.contains("max_owners: 8usize"));
        assert!(ir
            .lower(Target::ProcessLocal, RuntimeCapabilities::default())
            .is_err());
    }

    #[test]
    fn negotiation_checks_declared_limits_against_target_pointer_width() {
        let mut d = declaration();
        d.max_total_bytes = u32::MAX as u64 + 1;
        d.max_buffer_bytes = u32::MAX as u64 + 1;
        let ir = d.normalize().unwrap();
        assert_eq!(
            ir.negotiate(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    memory_runtime: true,
                    pointer_width_bits: 32,
                }
            ),
            Err(Diagnostic(
                "memory limit cannot be represented by target pointer width"
            ))
        );
        assert!(ir
            .negotiate(
                Target::ProcessLocal,
                RuntimeCapabilities {
                    memory_runtime: true,
                    pointer_width_bits: 64,
                }
            )
            .is_ok());
    }

    #[test]
    fn generated_factory_compiles_and_runs_with_declared_limits() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower(Target::ProcessLocal, capabilities())
            .unwrap();
        let runtime_path = format!("{}/src/memory_runtime.rs", env!("CARGO_MANIFEST_DIR"));
        let harness = format!(
            "#[path = {runtime_path:?}] mod memory_runtime;\n{factory}\nfn main() {{\n    assert_eq!(MEMORY_CONTRACT, {contract:?});\n    let mut arena = create_memory_arena().unwrap();\n    let owner = arena.create_owner().unwrap();\n    let buffer = arena.allocate(&owner, 4).unwrap();\n    assert_eq!(arena.stats().allocated_bytes, 4);\n    assert_eq!(arena.read(&owner, &buffer).unwrap(), [0; 4]);\n}}\n",
            runtime_path = runtime_path,
            factory = artifact.source,
            contract = CONTRACT,
        );
        let executable = std::env::temp_dir().join(format!(
            "goya-generated-memory-factory-{}",
            std::process::id()
        ));
        let mut compiler = Command::new("rustc")
            .args([
                "--edition=2021",
                "--crate-name",
                "generated_memory_factory",
                "-o",
            ])
            .arg(&executable)
            .arg("-")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let output = compiler.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "generated factory did not compile: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let run = Command::new(&executable).status().unwrap();
        let _ = std::fs::remove_file(&executable);
        assert!(run.success(), "generated factory did not run successfully");
    }

    /// Run with `cargo test -- --ignored --nocapture` for a construction throughput sample.
    #[test]
    #[ignore = "manual microbenchmark"]
    fn benchmark_memory_factory_construction() {
        let ir = declaration().normalize().unwrap();
        let started = Instant::now();
        for _ in 0..100_000 {
            let artifact = ir.lower(Target::ProcessLocal, capabilities()).unwrap();
            std::hint::black_box(artifact);
        }
        eprintln!(
            "100,000 normalized memory factory constructions: {:?}",
            started.elapsed()
        );
    }
}
