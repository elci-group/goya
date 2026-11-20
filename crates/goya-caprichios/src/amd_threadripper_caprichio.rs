//! A bounded AMD Ryzen Threadripper HEDT compute capability.
//!
//! The negotiated artifact is a checked worker plan and a factory binding
//! to this crate's plan runtime. It does not spawn threads, execute SIMD,
//! guarantee scheduling, or prove that a CPU executed work. Hosts on
//! Threadripper platforms report their real facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.cpu_compute.amd_threadripper.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORKER_THREADS: u32 = crate::amd_threadripper_runtime::MAX_WORKER_THREADS;
pub const MAX_BATCH: u32 = crate::amd_threadripper_runtime::MAX_BATCH;
pub const MAX_WORKSPACE_BYTES: u64 = crate::amd_threadripper_runtime::MAX_WORKSPACE_BYTES;

/// The instruction set a declaration plans for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Isa {
    Sse3,
    Avx,
    Avx2,
    Avx512,
}

impl Isa {
    fn label(self) -> &'static str {
        match self {
            Self::Sse3 => "sse3",
            Self::Avx => "avx",
            Self::Avx2 => "avx2",
            Self::Avx512 => "avx512",
        }
    }
}

/// Published CPU facts for one AMD Ryzen Threadripper generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadripperFacts {
    /// Family label (for example `Threadripper 2000`, `Threadripper Pro 7000WX`).
    pub family: &'static str,
    /// Core architecture name (for example `Zen`, `Zen 5`).
    pub arch: &'static str,
    /// Socket label AMD publishes; the platform is noted in parentheses where
    /// AMD publishes the same socket label for two platforms.
    pub socket: &'static str,
    /// Lowest core count across the family's launch-generation SKUs.
    pub cores_min: u32,
    /// Highest core count across the family's launch-generation SKUs.
    pub cores_max: u32,
    /// Hardware threads per core; two everywhere, via SMT.
    pub threads_per_core: u32,
    /// Multithreading technology label (`SMT`).
    pub thread_tech: &'static str,
    /// Published memory channels for the platform's socket.
    pub mem_channels: u32,
    /// Documented AVX support (every catalog generation).
    pub avx: bool,
    /// Documented AVX2 support (every catalog generation).
    pub avx2: bool,
    /// Documented AVX-512 support (Zen 4 generations onward only).
    pub avx512: bool,
    /// Process node, as published for the generation's silicon.
    pub process: &'static str,
    /// First release year.
    pub year: u16,
}

/// Catalog order is chronological launch order; the `year` field records
/// each generation's launch year. Facts come from AMD product pages, driver
/// pages, and press releases. Desktop AM4/AM5 Ryzen processors, AMD APUs,
/// and AMD EPYC server processors are excluded: they are covered by their
/// own caprichios, never guessed here. AMD publishes the sTR5 socket for
/// both the TRX50 HEDT and WRX90 workstation platforms from Zen 4 onward,
/// so that label is recorded with the platform in parentheses; earlier
/// generations moved between dedicated sockets (TR4, sTRX4, sWRX8).
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, ThreadripperFacts)] = &[
    ("Threadripper 1000",       ThreadripperFacts { family: "Threadripper 1000",       arch: "Zen",   socket: "TR4",           cores_min: 8,  cores_max: 16, threads_per_core: 2, thread_tech: "SMT", mem_channels: 4, avx: true, avx2: true, avx512: false, process: "GlobalFoundries 14nm",    year: 2017 }),
    ("Threadripper 2000",       ThreadripperFacts { family: "Threadripper 2000",       arch: "Zen+",  socket: "TR4",           cores_min: 12, cores_max: 32, threads_per_core: 2, thread_tech: "SMT", mem_channels: 4, avx: true, avx2: true, avx512: false, process: "GlobalFoundries 12nm",    year: 2018 }),
    ("Threadripper 3000",       ThreadripperFacts { family: "Threadripper 3000",       arch: "Zen 2", socket: "sTRX4",         cores_min: 24, cores_max: 64, threads_per_core: 2, thread_tech: "SMT", mem_channels: 4, avx: true, avx2: true, avx512: false, process: "TSMC 7nm / GF 12nm IOD",  year: 2019 }),
    ("Threadripper Pro 5000WX", ThreadripperFacts { family: "Threadripper Pro 5000WX", arch: "Zen 3", socket: "sWRX8",         cores_min: 12, cores_max: 64, threads_per_core: 2, thread_tech: "SMT", mem_channels: 8, avx: true, avx2: true, avx512: false, process: "TSMC 7nm",                year: 2022 }),
    ("Threadripper 7000",       ThreadripperFacts { family: "Threadripper 7000",       arch: "Zen 4", socket: "sTR5 (TRX50)",  cores_min: 24, cores_max: 64, threads_per_core: 2, thread_tech: "SMT", mem_channels: 4, avx: true, avx2: true, avx512: true,  process: "TSMC 5nm",                year: 2023 }),
    ("Threadripper Pro 7000WX", ThreadripperFacts { family: "Threadripper Pro 7000WX", arch: "Zen 4", socket: "sTR5 (WRX90)",  cores_min: 12, cores_max: 96, threads_per_core: 2, thread_tech: "SMT", mem_channels: 8, avx: true, avx2: true, avx512: true,  process: "TSMC 5nm",                year: 2023 }),
    ("Threadripper 9000",       ThreadripperFacts { family: "Threadripper 9000",       arch: "Zen 5", socket: "sTR5 (TRX50)",  cores_min: 24, cores_max: 64, threads_per_core: 2, thread_tech: "SMT", mem_channels: 4, avx: true, avx2: true, avx512: true,  process: "TSMC 4nm / TSMC 6nm IOD", year: 2025 }),
    ("Threadripper Pro 9000WX", ThreadripperFacts { family: "Threadripper Pro 9000WX", arch: "Zen 5", socket: "sTR5 (WRX90)",  cores_min: 12, cores_max: 96, threads_per_core: 2, thread_tech: "SMT", mem_channels: 8, avx: true, avx2: true, avx512: true,  process: "TSMC 4nm / TSMC 6nm IOD", year: 2025 }),
];

/// Per-generation lookup. Unknown generations return None; the catalog never
/// guesses.
pub fn facts_for(cpu: &str) -> Option<&'static ThreadripperFacts> {
    GENERATIONS
        .iter()
        .find(|(name, _)| *name == cpu)
        .map(|(_, facts)| facts)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// AMD Ryzen Threadripper HEDT hardware (TR4, sTRX4, sWRX8, TRX50 and
    /// WRX90 platforms on the sTR5 socket).
    AmdThreadripper,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports a Threadripper CPU is present and provides an OS thread API.
    pub x86_64_runtime: bool,
    /// Host reports AVX support on the executing CPU.
    pub avx: bool,
    /// Host reports AVX2 support on the executing CPU.
    pub avx2: bool,
    /// Host reports AVX-512 support on the executing CPU.
    pub avx512: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdThreadripperDeclaration {
    /// Exact generation name from the catalog; unknown names never normalize.
    pub cpu: String,
    /// Worker threads dispatched, from one through `MAX_WORKER_THREADS` and
    /// at most the generation's `cores_max * threads_per_core`.
    pub worker_threads: u32,
    /// Batch size, from one through `MAX_BATCH`.
    pub batch: u32,
    /// Bound on per-execution workspace, from one through
    /// `MAX_WORKSPACE_BYTES`.
    pub workspace_bytes: u64,
    /// Instruction set the plan assumes.
    pub isa: Isa,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdThreadripperIr {
    cpu: String,
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
    isa: Isa,
    facts: &'static ThreadripperFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmdThreadripperDiagnostic {
    BadCpu,
    ZeroWorkerThreads,
    WorkerThreadsTooLarge,
    ZeroBatch,
    BatchTooLarge,
    ZeroWorkspaceBytes,
    WorkspaceBytesTooLarge,
    WorkerOverflow,
    ThreadsExceedGeneration,
    UnsupportedTarget,
    UnsupportedAccelerator,
    MissingX86_64Runtime,
    MissingAvx,
    MissingAvx2,
    MissingAvx512,
    MissingUtf8Strings,
}

impl fmt::Display for AmdThreadripperDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadCpu => "generation is not in the AMD Threadripper catalog",
            Self::ZeroWorkerThreads => "worker thread count must be positive",
            Self::WorkerThreadsTooLarge => "worker thread count exceeds the supported bound of 192",
            Self::ZeroBatch => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::ZeroWorkspaceBytes => "workspace bound must be positive",
            Self::WorkspaceBytesTooLarge => "workspace bound exceeds the supported bound",
            Self::WorkerOverflow => "total worker executions do not fit the supported bound",
            Self::ThreadsExceedGeneration => {
                "worker threads exceed the generation's cores times threads per core"
            }
            Self::UnsupportedTarget => {
                "AMD Threadripper accelerator supports only Threadripper targets"
            }
            Self::UnsupportedAccelerator => {
                "generation does not document the requested instruction set"
            }
            Self::MissingX86_64Runtime => {
                "target runtime must report a Threadripper CPU with an OS thread API"
            }
            Self::MissingAvx => "target runtime must report AVX support",
            Self::MissingAvx2 => "target runtime must report AVX2 support",
            Self::MissingAvx512 => "target runtime must report AVX-512 support",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for AmdThreadripperDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a CPU ran
/// the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdThreadripperArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl AmdThreadripperDeclaration {
    pub fn normalize(self) -> Result<AmdThreadripperIr, AmdThreadripperDiagnostic> {
        let facts = facts_for(&self.cpu).ok_or(AmdThreadripperDiagnostic::BadCpu)?;
        let plan = crate::amd_threadripper_runtime::WorkerPlan::new(
            self.worker_threads,
            self.batch,
            self.workspace_bytes,
        )
        .map_err(|error| match error {
            crate::amd_threadripper_runtime::WorkerError::ZeroWorkerThreads => {
                AmdThreadripperDiagnostic::ZeroWorkerThreads
            }
            crate::amd_threadripper_runtime::WorkerError::WorkerThreadsTooLarge => {
                AmdThreadripperDiagnostic::WorkerThreadsTooLarge
            }
            crate::amd_threadripper_runtime::WorkerError::ZeroBatch => {
                AmdThreadripperDiagnostic::ZeroBatch
            }
            crate::amd_threadripper_runtime::WorkerError::BatchTooLarge => {
                AmdThreadripperDiagnostic::BatchTooLarge
            }
            crate::amd_threadripper_runtime::WorkerError::ZeroWorkspaceBytes => {
                AmdThreadripperDiagnostic::ZeroWorkspaceBytes
            }
            crate::amd_threadripper_runtime::WorkerError::WorkspaceBytesTooLarge => {
                AmdThreadripperDiagnostic::WorkspaceBytesTooLarge
            }
            crate::amd_threadripper_runtime::WorkerError::WorkerOverflow => {
                AmdThreadripperDiagnostic::WorkerOverflow
            }
        })?;
        debug_assert_eq!(plan.worker_threads(), self.worker_threads);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.workspace_bytes(), self.workspace_bytes);
        if u64::from(self.worker_threads)
            > u64::from(facts.cores_max) * u64::from(facts.threads_per_core)
        {
            return Err(AmdThreadripperDiagnostic::ThreadsExceedGeneration);
        }
        Ok(AmdThreadripperIr {
            cpu: self.cpu,
            worker_threads: self.worker_threads,
            batch: self.batch,
            workspace_bytes: self.workspace_bytes,
            isa: self.isa,
            facts,
        })
    }
}

impl AmdThreadripperIr {
    pub fn cpu(&self) -> &str {
        &self.cpu
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

    pub fn isa(&self) -> Isa {
        self.isa
    }

    pub fn total_worker_executions(&self) -> u64 {
        u64::from(self.worker_threads) * u64::from(self.batch)
    }

    pub fn facts(&self) -> &'static ThreadripperFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=amd_threadripper\ncpu={}\nfamily={}\narch={}\nsocket={}\ncores_min={}\ncores_max={}\nthreads_per_core={}\nthread_tech={}\nmem_channels={}\navx={}\navx2={}\navx512={}\nprocess={}\nyear={}\nisa={}\nworker_threads={}\nbatch={}\ntotal_worker_executions={}\nworkspace_bytes={}\nexecution=none_attempted\n",
            self.cpu,
            self.facts.family,
            self.facts.arch,
            self.facts.socket,
            self.facts.cores_min,
            self.facts.cores_max,
            self.facts.threads_per_core,
            self.facts.thread_tech,
            self.facts.mem_channels,
            self.facts.avx,
            self.facts.avx2,
            self.facts.avx512,
            self.facts.process,
            self.facts.year,
            self.isa.label(),
            self.worker_threads,
            self.batch,
            self.total_worker_executions(),
            self.workspace_bytes,
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<AmdThreadripperArtifact, AmdThreadripperDiagnostic> {
        if target != Target::AmdThreadripper {
            return Err(AmdThreadripperDiagnostic::UnsupportedTarget);
        }
        if !capabilities.x86_64_runtime {
            return Err(AmdThreadripperDiagnostic::MissingX86_64Runtime);
        }
        if !capabilities.utf8_strings {
            return Err(AmdThreadripperDiagnostic::MissingUtf8Strings);
        }
        match self.isa {
            Isa::Sse3 => {}
            Isa::Avx => {
                if !capabilities.avx {
                    return Err(AmdThreadripperDiagnostic::MissingAvx);
                }
                if !self.facts.avx {
                    return Err(AmdThreadripperDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx2 => {
                if !capabilities.avx2 {
                    return Err(AmdThreadripperDiagnostic::MissingAvx2);
                }
                if !self.facts.avx2 {
                    return Err(AmdThreadripperDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx512 => {
                if !capabilities.avx512 {
                    return Err(AmdThreadripperDiagnostic::MissingAvx512);
                }
                if !self.facts.avx512 {
                    return Err(AmdThreadripperDiagnostic::UnsupportedAccelerator);
                }
            }
        }
        Ok(AmdThreadripperArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; AMD Threadripper accelerator, no worker dispatch is attempted.\npub fn make_worker_plan() -> Result<crate::amd_threadripper_runtime::WorkerPlan, crate::amd_threadripper_runtime::WorkerError> {{\n    crate::amd_threadripper_runtime::WorkerPlan::new({}u32, {}u32, {}u64)\n}}\n",
                self.worker_threads, self.batch, self.workspace_bytes,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES: RuntimeCapabilities = RuntimeCapabilities {
        x86_64_runtime: true,
        avx: true,
        avx2: true,
        avx512: true,
        utf8_strings: true,
    };

    fn declaration(cpu: &str, worker_threads: u32, batch: u32) -> AmdThreadripperDeclaration {
        AmdThreadripperDeclaration {
            cpu: cpu.to_string(),
            worker_threads,
            batch,
            workspace_bytes: 4096,
            isa: Isa::Sse3,
        }
    }

    fn first_avx512_cpu() -> &'static str {
        // Tests lower AVX-512 plans, so start from the first generation that
        // documents AVX-512 (Zen 4, "Threadripper 7000").
        GENERATIONS
            .iter()
            .find(|(_, facts)| facts.avx512)
            .map(|(name, _)| *name)
            .expect("catalog must contain an AVX-512 generation")
    }

    #[test]
    fn catalog_entries_have_consistent_facts() {
        assert!(!GENERATIONS.is_empty(), "catalog must not be empty");
        let mut seen = std::collections::BTreeSet::new();
        let mut previous_year = 0;
        for (name, facts) in GENERATIONS {
            assert!(seen.insert(*name), "duplicate generation {name}");
            assert!(matches!(
                facts.arch,
                "Zen" | "Zen+" | "Zen 2" | "Zen 3" | "Zen 4" | "Zen 5"
            ));
            assert!(!facts.socket.is_empty());
            assert!(facts.cores_min >= 8, "{name} must have at least 8 cores");
            assert!(facts.cores_max <= 96, "{name} must have at most 96 cores");
            assert!(facts.cores_min <= facts.cores_max, "{name} core range");
            assert_eq!(facts.threads_per_core, 2, "SMT is two threads per core");
            assert_eq!(facts.thread_tech, "SMT");
            assert!(
                u64::from(facts.cores_max) * u64::from(facts.threads_per_core)
                    <= u64::from(MAX_WORKER_THREADS),
                "{name} thread count must fit the supported bound"
            );
            assert!(matches!(facts.mem_channels, 4 | 8), "{name} channels");
            assert!(facts.avx && facts.avx2, "{name} must support AVX and AVX2");
            if facts.avx512 {
                assert!(facts.avx2 && facts.avx, "AVX-512 implies AVX2 implies AVX");
            }
            assert!(!facts.process.is_empty());
            assert!((2017..=2030).contains(&facts.year), "{name} launch year");
            assert!(facts.year >= previous_year, "catalog must be chronological");
            previous_year = facts.year;
        }
        // The catalog never lists desktop AM4/AM5 Ryzen, APUs, or EPYC.
        for (name, _) in GENERATIONS {
            assert!(name.contains("Threadripper"), "unexpected entry {name}");
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let cpu = first_avx512_cpu();
        let ir = AmdThreadripperDeclaration {
            isa: Isa::Avx512,
            ..declaration(cpu, 64, 4)
        }
        .normalize()
        .unwrap();
        assert_eq!(ir.worker_threads(), 64);
        assert_eq!(ir.batch(), 4);
        assert_eq!(ir.total_worker_executions(), 256);
        let first = ir
            .lower_checked(Target::AmdThreadripper, CAPABILITIES)
            .unwrap();
        let second = ir
            .lower_checked(Target::AmdThreadripper, CAPABILITIES)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=amd_threadripper\n"));
        assert!(first.canonical_ir.contains("isa=avx512\n"));
        assert!(first.canonical_ir.contains("worker_threads=64\n"));
        assert!(first.canonical_ir.contains("total_worker_executions=256\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("WorkerPlan::new(64u32, 4u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_cpus_and_bad_bounds() {
        assert_eq!(
            declaration("Ryzen FFT", 64, 8).normalize(),
            Err(AmdThreadripperDiagnostic::BadCpu)
        );
        let cpu = first_avx512_cpu();
        assert_eq!(
            declaration(cpu, 0, 8).normalize(),
            Err(AmdThreadripperDiagnostic::ZeroWorkerThreads)
        );
        assert_eq!(
            declaration(cpu, MAX_WORKER_THREADS + 1, 8).normalize(),
            Err(AmdThreadripperDiagnostic::WorkerThreadsTooLarge)
        );
        assert_eq!(
            declaration(cpu, 64, 0).normalize(),
            Err(AmdThreadripperDiagnostic::ZeroBatch)
        );
        assert_eq!(
            declaration(cpu, 64, MAX_BATCH + 1).normalize(),
            Err(AmdThreadripperDiagnostic::BatchTooLarge)
        );
        let mut zero_workspace = declaration(cpu, 64, 8);
        zero_workspace.workspace_bytes = 0;
        assert_eq!(
            zero_workspace.normalize(),
            Err(AmdThreadripperDiagnostic::ZeroWorkspaceBytes)
        );
        let mut huge_workspace = declaration(cpu, 64, 8);
        huge_workspace.workspace_bytes = MAX_WORKSPACE_BYTES + 1;
        assert_eq!(
            huge_workspace.normalize(),
            Err(AmdThreadripperDiagnostic::WorkspaceBytesTooLarge)
        );
        // "Threadripper 1000" tops out at 16 cores times two SMT threads.
        assert_eq!(
            declaration("Threadripper 1000", 16 * 2 + 1, 8).normalize(),
            Err(AmdThreadripperDiagnostic::ThreadsExceedGeneration)
        );
        assert!(declaration(cpu, MAX_WORKER_THREADS, MAX_BATCH)
            .normalize()
            .is_err());
        assert!(declaration("Threadripper 1000", 16 * 2, 8)
            .normalize()
            .is_ok());
        assert!(AmdThreadripperDeclaration {
            isa: Isa::Avx512,
            ..declaration("Threadripper Pro 9000WX", MAX_WORKER_THREADS, 1)
        }
        .normalize()
        .is_ok());
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let cpu = first_avx512_cpu();
        let ir = AmdThreadripperDeclaration {
            isa: Isa::Avx512,
            ..declaration(cpu, 16, 4)
        }
        .normalize()
        .unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(AmdThreadripperDiagnostic::UnsupportedTarget)
            );
        }
        for (isa, missing, expected) in [
            (
                Isa::Sse3,
                RuntimeCapabilities {
                    x86_64_runtime: false,
                    ..CAPABILITIES
                },
                AmdThreadripperDiagnostic::MissingX86_64Runtime,
            ),
            (
                Isa::Sse3,
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                AmdThreadripperDiagnostic::MissingUtf8Strings,
            ),
            (
                Isa::Avx,
                RuntimeCapabilities {
                    avx: false,
                    ..CAPABILITIES
                },
                AmdThreadripperDiagnostic::MissingAvx,
            ),
            (
                Isa::Avx2,
                RuntimeCapabilities {
                    avx2: false,
                    ..CAPABILITIES
                },
                AmdThreadripperDiagnostic::MissingAvx2,
            ),
            (
                Isa::Avx512,
                RuntimeCapabilities {
                    avx512: false,
                    ..CAPABILITIES
                },
                AmdThreadripperDiagnostic::MissingAvx512,
            ),
        ] {
            let ir = AmdThreadripperDeclaration {
                isa,
                ..declaration(cpu, 16, 4)
            }
            .normalize()
            .unwrap();
            assert_eq!(
                ir.lower_checked(Target::AmdThreadripper, missing),
                Err(expected)
            );
        }
        // A pre-Zen-4 generation never satisfies an AVX-512 plan.
        let pre_zen4 = GENERATIONS
            .iter()
            .find(|(_, facts)| !facts.avx512)
            .map(|(name, _)| *name)
            .expect("catalog must contain a pre-AVX-512 generation");
        let ir_old = AmdThreadripperDeclaration {
            isa: Isa::Avx512,
            ..declaration(pre_zen4, 16, 4)
        }
        .normalize()
        .unwrap();
        assert_eq!(
            ir_old.lower_checked(Target::AmdThreadripper, CAPABILITIES),
            Err(AmdThreadripperDiagnostic::UnsupportedAccelerator)
        );
    }

    #[test]
    fn every_generation_lowers_with_sse3_and_avx512_fails_closed_pre_zen4() {
        let mut lowered_sse3 = 0;
        let mut lowered_avx512 = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 2, 2).normalize().unwrap();
            ir.lower_checked(Target::AmdThreadripper, CAPABILITIES)
                .unwrap();
            lowered_sse3 += 1;
            let ir512 = AmdThreadripperDeclaration {
                isa: Isa::Avx512,
                ..declaration(name, 2, 2)
            }
            .normalize()
            .unwrap();
            if facts.avx512 {
                ir512
                    .lower_checked(Target::AmdThreadripper, CAPABILITIES)
                    .unwrap();
                lowered_avx512 += 1;
            } else {
                assert_eq!(
                    ir512.lower_checked(Target::AmdThreadripper, CAPABILITIES),
                    Err(AmdThreadripperDiagnostic::UnsupportedAccelerator)
                );
            }
        }
        assert!(
            lowered_sse3 > 0,
            "catalog must lower at least one generation"
        );
        assert!(
            lowered_avx512 > 0,
            "catalog must lower one AVX-512 generation"
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_threadripper_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let cpu = first_avx512_cpu();
        let artifact = declaration(cpu, 96, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::AmdThreadripper, CAPABILITIES)
            .unwrap();
        let runtime_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/amd_threadripper_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod amd_threadripper_runtime;\n{}\nfn main() {{\n    let plan = make_worker_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.worker_threads(), 96);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_worker_executions(), 288);\n    assert_eq!(plan.workspace_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-amd-threadripper-factory-{}-{nonce}",
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
            .expect("run generated worker factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
