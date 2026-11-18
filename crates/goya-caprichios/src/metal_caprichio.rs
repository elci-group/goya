//! A bounded Apple Silicon Metal GPU-dispatch capability.
//!
//! The negotiated artifact is a checked dispatch plan and a factory binding to
//! this crate's plan runtime. It does not compile shaders, claim GPU access,
//! guarantee scheduling, or prove that a Metal device is present. Hosts on
//! Apple Silicon hardware report their real Metal facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.gpu_compute.apple_silicon_metal.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_THREADS_PER_GROUP: u32 = crate::apple_runtime::MAX_THREADS_PER_GROUP;
pub const MAX_GROUPS: u32 = crate::apple_runtime::MAX_GROUPS;
pub const MAX_BUFFER_BYTES: u64 = crate::apple_runtime::MAX_BUFFER_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchDeclaration {
    /// Threads per threadgroup; a power of two, from one through
    /// `MAX_THREADS_PER_GROUP` (the Metal limit).
    pub threads_per_group: u32,
    /// Threadgroups dispatched, from one through `MAX_GROUPS`.
    pub groups: u32,
    /// Bound on any single argument buffer, from one through
    /// `MAX_BUFFER_BYTES`.
    pub max_buffer_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchIr {
    threads_per_group: u32,
    groups: u32,
    max_buffer_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// Apple Silicon (M-series) GPU over Metal.
    AppleSilicon,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub metal_device: bool,
    pub command_queue: bool,
    /// Unified memory architecture is available on the target GPU.
    pub unified_memory: bool,
    /// Metal Shading Language version 3.0 support.
    pub msl_version_3: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ThreadsPerGroupZero,
    ThreadsPerGroupNotPowerOfTwo,
    ThreadsPerGroupTooLarge,
    GroupsZero,
    GroupsTooLarge,
    BufferBytesZero,
    BufferBytesTooLarge,
    ThreadCountOverflow,
    UnsupportedTarget,
    MissingMetalDevice,
    MissingCommandQueue,
    MissingUnifiedMemory,
    MissingMslVersion3,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ThreadsPerGroupZero => "threads per threadgroup must be positive",
            Self::ThreadsPerGroupNotPowerOfTwo => {
                "threads per threadgroup must be a power of two"
            }
            Self::ThreadsPerGroupTooLarge => {
                "threads per threadgroup exceeds the Metal limit of 1024"
            }
            Self::GroupsZero => "threadgroup count must be positive",
            Self::GroupsTooLarge => "threadgroup count exceeds the supported bound",
            Self::BufferBytesZero => "buffer bound must be positive",
            Self::BufferBytesTooLarge => "buffer bound exceeds the supported bound",
            Self::ThreadCountOverflow => "total thread count does not fit a u32 dispatch",
            Self::UnsupportedTarget => "Metal dispatch supports only Apple Silicon",
            Self::MissingMetalDevice => "target runtime must report a Metal device",
            Self::MissingCommandQueue => "target runtime must provide a command queue",
            Self::MissingUnifiedMemory => "target GPU must report unified memory",
            Self::MissingMslVersion3 => "target runtime must support MSL 3.0",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated dispatch design and source text. The artifact is a plan and a
/// factory definition; successful construction is not proof that a GPU ran
/// the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub total_threads: u32,
    pub source: String,
}

impl DispatchDeclaration {
    pub fn normalize(self) -> Result<DispatchIr, Diagnostic> {
        if self.threads_per_group == 0 {
            return Err(Diagnostic::ThreadsPerGroupZero);
        }
        if !self.threads_per_group.is_power_of_two() {
            return Err(Diagnostic::ThreadsPerGroupNotPowerOfTwo);
        }
        if self.threads_per_group > MAX_THREADS_PER_GROUP {
            return Err(Diagnostic::ThreadsPerGroupTooLarge);
        }
        if self.groups == 0 {
            return Err(Diagnostic::GroupsZero);
        }
        if self.groups > MAX_GROUPS {
            return Err(Diagnostic::GroupsTooLarge);
        }
        if self.max_buffer_bytes == 0 {
            return Err(Diagnostic::BufferBytesZero);
        }
        if self.max_buffer_bytes > MAX_BUFFER_BYTES {
            return Err(Diagnostic::BufferBytesTooLarge);
        }
        if u32::try_from(self.groups as u64 * self.threads_per_group as u64).is_err() {
            return Err(Diagnostic::ThreadCountOverflow);
        }
        Ok(DispatchIr {
            threads_per_group: self.threads_per_group,
            groups: self.groups,
            max_buffer_bytes: self.max_buffer_bytes,
        })
    }
}

impl DispatchIr {
    pub fn threads_per_group(&self) -> u32 {
        self.threads_per_group
    }

    pub fn groups(&self) -> u32 {
        self.groups
    }

    pub fn max_buffer_bytes(&self) -> u64 {
        self.max_buffer_bytes
    }

    pub fn total_threads(&self) -> u32 {
        self.groups * self.threads_per_group
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=apple_silicon_metal\nthreads_per_group={}\ngroups={}\ntotal_threads={}\nmax_buffer_bytes={}\nmemory=unified\nscheduling=none_guaranteed\n",
            self.threads_per_group,
            self.groups,
            self.total_threads(),
            self.max_buffer_bytes
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<DispatchArtifact, Diagnostic> {
        if target != Target::AppleSilicon {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.metal_device {
            return Err(Diagnostic::MissingMetalDevice);
        }
        if !capabilities.command_queue {
            return Err(Diagnostic::MissingCommandQueue);
        }
        if !capabilities.unified_memory {
            return Err(Diagnostic::MissingUnifiedMemory);
        }
        if !capabilities.msl_version_3 {
            return Err(Diagnostic::MissingMslVersion3);
        }
        Ok(DispatchArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            total_threads: self.total_threads(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; checked dispatch plan only, no GPU execution is attempted.\npub fn make_dispatch_plan() -> Result<crate::apple_runtime::MetalDispatchPlan, crate::apple_runtime::MetalError> {{\n    crate::apple_runtime::MetalDispatchPlan::new({}u32, {}u32, {}u64)\n}}\n",
                self.threads_per_group, self.groups, self.max_buffer_bytes
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLE_SILICON: RuntimeCapabilities = RuntimeCapabilities {
        metal_device: true,
        command_queue: true,
        unified_memory: true,
        msl_version_3: true,
    };

    fn declaration(threads_per_group: u32, groups: u32) -> DispatchDeclaration {
        DispatchDeclaration {
            threads_per_group,
            groups,
            max_buffer_bytes: 4096,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_plan() {
        let ir = declaration(64, 32).normalize().unwrap();
        assert_eq!(ir.threads_per_group(), 64);
        assert_eq!(ir.groups(), 32);
        assert_eq!(ir.total_threads(), 2048);
        let first = ir.lower_checked(Target::AppleSilicon, APPLE_SILICON).unwrap();
        let second = ir.lower_checked(Target::AppleSilicon, APPLE_SILICON).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert_eq!(first.total_threads, 2048);
        assert!(first.canonical_ir.contains("target=apple_silicon_metal\n"));
        assert!(first.canonical_ir.contains("threads_per_group=64\n"));
        assert!(first.canonical_ir.contains("total_threads=2048\n"));
        assert!(first.canonical_ir.contains("memory=unified\n"));
        assert!(first
            .canonical_ir
            .contains("scheduling=none_guaranteed\n"));
        assert!(first
            .source
            .contains("MetalDispatchPlan::new(64u32, 32u32, 4096u64)"));
    }

    #[test]
    fn rejects_bad_threadgroup_and_buffer_bounds() {
        assert_eq!(
            declaration(0, 32).normalize(),
            Err(Diagnostic::ThreadsPerGroupZero)
        );
        assert_eq!(
            declaration(63, 32).normalize(),
            Err(Diagnostic::ThreadsPerGroupNotPowerOfTwo)
        );
        assert_eq!(
            declaration(MAX_THREADS_PER_GROUP * 2, 32).normalize(),
            Err(Diagnostic::ThreadsPerGroupTooLarge)
        );
        assert_eq!(
            declaration(64, 0).normalize(),
            Err(Diagnostic::GroupsZero)
        );
        assert_eq!(
            declaration(64, MAX_GROUPS + 1).normalize(),
            Err(Diagnostic::GroupsTooLarge)
        );
        let mut zero_buffer = declaration(64, 32);
        zero_buffer.max_buffer_bytes = 0;
        assert_eq!(zero_buffer.normalize(), Err(Diagnostic::BufferBytesZero));
        let mut huge_buffer = declaration(64, 32);
        huge_buffer.max_buffer_bytes = MAX_BUFFER_BYTES + 1;
        assert_eq!(
            huge_buffer.normalize(),
            Err(Diagnostic::BufferBytesTooLarge)
        );
        // `MAX_GROUPS * MAX_THREADS_PER_GROUP` fits a `u32`, so the overflow
        // guard stays defensive; the full valid bound is accepted.
        assert!(declaration(MAX_THREADS_PER_GROUP, MAX_GROUPS).normalize().is_ok());
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration(64, 32).normalize().unwrap();
        for target in [
            Target::CrossPlatform,
            Target::Browser,
            Target::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, APPLE_SILICON),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    metal_device: false,
                    ..APPLE_SILICON
                },
                Diagnostic::MissingMetalDevice,
            ),
            (
                RuntimeCapabilities {
                    command_queue: false,
                    ..APPLE_SILICON
                },
                Diagnostic::MissingCommandQueue,
            ),
            (
                RuntimeCapabilities {
                    unified_memory: false,
                    ..APPLE_SILICON
                },
                Diagnostic::MissingUnifiedMemory,
            ),
            (
                RuntimeCapabilities {
                    msl_version_3: false,
                    ..APPLE_SILICON
                },
                Diagnostic::MissingMslVersion3,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::AppleSilicon, missing),
                Err(expected)
            );
        }
    }

    #[test]
    fn generated_factory_matches_dispatch_runtime_constructor_contract() {
        let artifact = declaration(256, 4)
            .normalize()
            .unwrap()
            .lower_checked(Target::AppleSilicon, APPLE_SILICON)
            .unwrap();
        let plan = crate::apple_runtime::MetalDispatchPlan::new(256, 4, 4096).unwrap();
        assert_eq!(plan.total_threads(), 1024);
        assert!(artifact.source.contains("MetalDispatchPlan::new(256u32, 4u32, 4096u64)"));
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_apple_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration(128, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::AppleSilicon, APPLE_SILICON)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let plan = make_dispatch_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.threads_per_group(), 128);\n    assert_eq!(plan.groups(), 3);\n    assert_eq!(plan.total_threads(), 384);\n    assert_eq!(plan.max_buffer_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-metal-factory-{}-{nonce}",
            std::process::id()
        ));
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
            .expect("run generated dispatch factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
