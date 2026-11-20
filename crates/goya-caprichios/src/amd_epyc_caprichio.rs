//! A bounded AMD EPYC server compute capability.
//!
//! The negotiated artifact is a checked worker plan and a factory binding
//! to this crate's plan runtime. It does not spawn threads, execute SIMD,
//! guarantee scheduling, or prove that a CPU executed work. Hosts on EPYC
//! platforms report their real facilities through [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.cpu_compute.amd_epyc.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORKER_THREADS: u32 = crate::amd_epyc_runtime::MAX_WORKER_THREADS;
pub const MAX_BATCH: u32 = crate::amd_epyc_runtime::MAX_BATCH;
pub const MAX_WORKSPACE_BYTES: u64 = crate::amd_epyc_runtime::MAX_WORKSPACE_BYTES;

/// Published processor facts for one AMD EPYC server generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EpycFacts {
    /// Microarchitecture as AMD names it; dense-core variants carry the
    /// `c` suffix, and mixed-silicon lines record AMD's own mixed label.
    pub arch: &'static str,
    /// Server socket package (`SP3`, `SP5` or `SP6`).
    pub socket: &'static str,
    /// AMD series name (for example `EPYC 9004 (Genoa)`).
    pub generation: &'static str,
    /// Smallest core count AMD published as a family SKU.
    pub cores_min: u32,
    /// Largest core count AMD published as a family SKU.
    pub cores_max: u32,
    /// Threads per core with the vendor's multithreading enabled.
    pub threads_per_core: u32,
    /// AMD's own name for its multithreading technology (`SMT`).
    pub thread_tech: &'static str,
    /// Memory channels the platform exposes per socket.
    pub mem_channels: u32,
    /// AVX instruction-set support.
    pub avx: bool,
    /// AVX2 instruction-set support.
    pub avx2: bool,
    /// AVX-512 support, documented from Zen 4 onward.
    pub avx512: bool,
    /// Cache technology (`none`, or `3D V-Cache` on Milan-X).
    pub cache_tech: &'static str,
    /// Compute-die manufacturing process node.
    pub process: &'static str,
    /// First release year.
    pub year: u16,
}

/// Catalog order is chronological by launch year; the `year` field records
/// each generation's true launch year. Facts come from AMD product pages,
/// datasheets and press releases corroborated by independent coverage;
/// figures AMD did not publish are not guessed, and mixed-silicon lines
/// record AMD's own naming in the arch label. Desktop Ryzen, mobile APU
/// and Threadripper parts are out of scope here: this crate models them in
/// their own caprichios. Zen 6 ("Venice") is excluded until AMD publishes
/// shipping facts.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, EpycFacts)] = &[
    // SP3 platform generations (DDR4)
    ("EPYC 7001 (Naples)",   EpycFacts { arch: "Zen",                    socket: "SP3", generation: "EPYC 7001 (Naples)",   cores_min: 8,  cores_max: 32,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  avx: true,  avx2: true,  avx512: false, cache_tech: "none",       process: "14 nm",       year: 2017 }),
    ("EPYC 7002 (Rome)",     EpycFacts { arch: "Zen 2",                  socket: "SP3", generation: "EPYC 7002 (Rome)",     cores_min: 8,  cores_max: 64,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  avx: true,  avx2: true,  avx512: false, cache_tech: "none",       process: "7 nm",        year: 2019 }),
    ("EPYC 7003 (Milan)",    EpycFacts { arch: "Zen 3",                  socket: "SP3", generation: "EPYC 7003 (Milan)",    cores_min: 8,  cores_max: 64,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  avx: true,  avx2: true,  avx512: false, cache_tech: "none",       process: "7 nm",        year: 2021 }),
    ("EPYC 7003X (Milan-X)", EpycFacts { arch: "Zen 3",                  socket: "SP3", generation: "EPYC 7003X (Milan-X)", cores_min: 16, cores_max: 64,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  avx: true,  avx2: true,  avx512: false, cache_tech: "3D V-Cache", process: "7 nm",        year: 2022 }),
    // SP5 platform generations (DDR5)
    ("EPYC 9004 (Genoa)",    EpycFacts { arch: "Zen 4",                  socket: "SP5", generation: "EPYC 9004 (Genoa)",    cores_min: 16, cores_max: 96,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 12, avx: true,  avx2: true,  avx512: true,  cache_tech: "none",       process: "5 nm",        year: 2022 }),
    ("EPYC 8003 (Bergamo)",  EpycFacts { arch: "Zen 4c",                 socket: "SP5", generation: "EPYC 8003 (Bergamo)",  cores_min: 112, cores_max: 128, threads_per_core: 2, thread_tech: "SMT", mem_channels: 12, avx: true,  avx2: true,  avx512: true,  cache_tech: "none",       process: "5 nm",        year: 2023 }),
    ("EPYC 8004 (Siena)",    EpycFacts { arch: "Zen 4c",                 socket: "SP6", generation: "EPYC 8004 (Siena)",    cores_min: 8,  cores_max: 64,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 6,  avx: true,  avx2: true,  avx512: true,  cache_tech: "none",       process: "5 nm",        year: 2023 }),
    ("EPYC 9005 (Turin)",    EpycFacts { arch: "Zen 5 / Zen 5c (dense)", socket: "SP5", generation: "EPYC 9005 (Turin)",    cores_min: 8,  cores_max: 192, threads_per_core: 2, thread_tech: "SMT", mem_channels: 12, avx: true,  avx2: true,  avx512: true,  cache_tech: "none",       process: "4 nm / 3 nm", year: 2024 }),
];

/// Per-generation lookup. Unknown names return None; the catalog never
/// guesses.
pub fn facts_for(cpu: &str) -> Option<&'static EpycFacts> {
    GENERATIONS
        .iter()
        .find(|(name, _)| *name == cpu)
        .map(|(_, facts)| facts)
}

/// Requested instruction-set baseline for the planned worker dispatch.
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// AMD EPYC server hardware (SP3, SP5 and SP6 platforms).
    AmdEpyc,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports an EPYC CPU is present and provides an OS thread API.
    pub x86_64_runtime: bool,
    pub avx: bool,
    pub avx2: bool,
    pub avx512: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdEpycDeclaration {
    /// Exact generation name from the catalog; unknown names never normalize.
    pub cpu: String,
    /// Worker threads dispatched, from one through `MAX_WORKER_THREADS`,
    /// and never above the generation's published thread count.
    pub worker_threads: u32,
    /// Batch size, from one through `MAX_BATCH`.
    pub batch: u32,
    /// Bound on per-plan workspace, from one through `MAX_WORKSPACE_BYTES`.
    pub workspace_bytes: u64,
    /// Instruction-set baseline the plan is compiled against.
    pub isa: Isa,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdEpycIr {
    cpu: String,
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
    isa: Isa,
    facts: &'static EpycFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmdEpycDiagnostic {
    BadCpu,
    WorkerThreadsZero,
    WorkerThreadsTooLarge,
    BatchZero,
    BatchTooLarge,
    WorkspaceBytesZero,
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

impl fmt::Display for AmdEpycDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadCpu => "CPU is not in the AMD EPYC catalog",
            Self::WorkerThreadsZero => "worker thread count must be positive",
            Self::WorkerThreadsTooLarge => "worker thread count exceeds the supported bound",
            Self::BatchZero => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::WorkspaceBytesZero => "workspace bound must be positive",
            Self::WorkspaceBytesTooLarge => "workspace bound exceeds the supported bound",
            Self::WorkerOverflow => "total worker executions do not fit the supported bound",
            Self::ThreadsExceedGeneration => {
                "worker threads exceed the generation's published thread count"
            }
            Self::UnsupportedTarget => "AMD EPYC compute supports only EPYC targets",
            Self::UnsupportedAccelerator => {
                "generation does not implement the requested ISA extensions"
            }
            Self::MissingX86_64Runtime => {
                "target runtime must report an EPYC CPU with a thread API"
            }
            Self::MissingAvx => "target runtime must report AVX support",
            Self::MissingAvx2 => "target runtime must report AVX2 support",
            Self::MissingAvx512 => "target runtime must report AVX-512 support",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for AmdEpycDiagnostic {}

/// Negotiated worker design and source text. The artifact is a plan and a
/// factory definition; successful construction is not proof that a CPU ran
/// the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdEpycArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl AmdEpycDeclaration {
    pub fn normalize(self) -> Result<AmdEpycIr, AmdEpycDiagnostic> {
        let facts = facts_for(&self.cpu).ok_or(AmdEpycDiagnostic::BadCpu)?;
        let plan = crate::amd_epyc_runtime::WorkerPlan::new(
            self.worker_threads,
            self.batch,
            self.workspace_bytes,
        )
        .map_err(|error| match error {
            crate::amd_epyc_runtime::WorkerError::ZeroWorkerThreads => {
                AmdEpycDiagnostic::WorkerThreadsZero
            }
            crate::amd_epyc_runtime::WorkerError::WorkerThreadsTooLarge => {
                AmdEpycDiagnostic::WorkerThreadsTooLarge
            }
            crate::amd_epyc_runtime::WorkerError::ZeroBatch => AmdEpycDiagnostic::BatchZero,
            crate::amd_epyc_runtime::WorkerError::BatchTooLarge => AmdEpycDiagnostic::BatchTooLarge,
            crate::amd_epyc_runtime::WorkerError::ZeroWorkspaceBytes => {
                AmdEpycDiagnostic::WorkspaceBytesZero
            }
            crate::amd_epyc_runtime::WorkerError::WorkspaceBytesTooLarge => {
                AmdEpycDiagnostic::WorkspaceBytesTooLarge
            }
            crate::amd_epyc_runtime::WorkerError::WorkerOverflow => {
                AmdEpycDiagnostic::WorkerOverflow
            }
        })?;
        debug_assert_eq!(plan.worker_threads(), self.worker_threads);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.workspace_bytes(), self.workspace_bytes);
        if self.worker_threads > facts.cores_max * facts.threads_per_core {
            return Err(AmdEpycDiagnostic::ThreadsExceedGeneration);
        }
        Ok(AmdEpycIr {
            cpu: self.cpu,
            worker_threads: self.worker_threads,
            batch: self.batch,
            workspace_bytes: self.workspace_bytes,
            isa: self.isa,
            facts,
        })
    }
}

impl AmdEpycIr {
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

    pub fn facts(&self) -> &'static EpycFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=amd_epyc\ncpu={}\ngeneration={}\narch={}\nsocket={}\ncores_min={}\ncores_max={}\nthreads_per_core={}\nthread_tech={}\nmem_channels={}\ncache_tech={}\navx={}\navx2={}\navx512={}\nprocess={}\nyear={}\nisa={}\nworker_threads={}\nbatch={}\ntotal_worker_executions={}\nworkspace_bytes={}\nexecution=none_attempted\n",
            self.cpu,
            self.facts.generation,
            self.facts.arch,
            self.facts.socket,
            self.facts.cores_min,
            self.facts.cores_max,
            self.facts.threads_per_core,
            self.facts.thread_tech,
            self.facts.mem_channels,
            self.facts.cache_tech,
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
    ) -> Result<AmdEpycArtifact, AmdEpycDiagnostic> {
        if target != Target::AmdEpyc {
            return Err(AmdEpycDiagnostic::UnsupportedTarget);
        }
        if !capabilities.x86_64_runtime {
            return Err(AmdEpycDiagnostic::MissingX86_64Runtime);
        }
        if !capabilities.utf8_strings {
            return Err(AmdEpycDiagnostic::MissingUtf8Strings);
        }
        match self.isa {
            Isa::Sse3 => {}
            Isa::Avx => {
                if !capabilities.avx {
                    return Err(AmdEpycDiagnostic::MissingAvx);
                }
                if !self.facts.avx {
                    return Err(AmdEpycDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx2 => {
                if !capabilities.avx2 {
                    return Err(AmdEpycDiagnostic::MissingAvx2);
                }
                if !self.facts.avx2 {
                    return Err(AmdEpycDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx512 => {
                if !capabilities.avx512 {
                    return Err(AmdEpycDiagnostic::MissingAvx512);
                }
                if !self.facts.avx512 {
                    return Err(AmdEpycDiagnostic::UnsupportedAccelerator);
                }
            }
        }
        Ok(AmdEpycArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; AMD EPYC compute, no worker threads are spawned.\npub fn make_worker_plan() -> Result<crate::amd_epyc_runtime::WorkerPlan, crate::amd_epyc_runtime::WorkerError> {{\n    crate::amd_epyc_runtime::WorkerPlan::new({}u32, {}u32, {}u64)\n}}\n",
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

    fn declaration(cpu: &str, worker_threads: u32, batch: u32) -> AmdEpycDeclaration {
        AmdEpycDeclaration {
            cpu: cpu.to_string(),
            worker_threads,
            batch,
            workspace_bytes: 4096,
            isa: Isa::Avx512,
        }
    }

    fn first_avx512_cpu() -> &'static str {
        // Tests lower AVX-512 plans, so start from the first catalog
        // generation with documented AVX-512 support.
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
            assert!(matches!(facts.socket, "SP3" | "SP5" | "SP6"));
            assert!((2017..=2030).contains(&facts.year));
            assert!(
                previous_year <= facts.year,
                "catalog must stay chronological at {name}"
            );
            previous_year = facts.year;
            assert!(
                matches!(
                    (facts.socket, facts.mem_channels),
                    ("SP3", 8) | ("SP5", 12) | ("SP6", 6)
                ),
                "socket/channel mismatch on {name}"
            );
            assert!(facts.cores_min > 0 && facts.cores_min <= facts.cores_max);
            assert_eq!(facts.threads_per_core, 2);
            assert_eq!(facts.thread_tech, "SMT");
            if facts.avx512 {
                assert!(
                    facts.avx2 && facts.avx,
                    "AVX-512 implies AVX2/AVX on {name}"
                );
            }
            if facts.avx2 {
                assert!(facts.avx, "AVX2 implies AVX on {name}");
            }
            assert!(facts.cache_tech == "none" || facts.cache_tech == "3D V-Cache");
            if facts.cache_tech == "3D V-Cache" {
                assert!(
                    name.contains("Milan-X"),
                    "3D V-Cache recorded outside Milan-X: {name}"
                );
            }
            assert!(!facts.arch.is_empty());
            assert!(!facts.process.is_empty());
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let cpu = first_avx512_cpu();
        let ir = declaration(cpu, 128, 8).normalize().unwrap();
        assert_eq!(ir.cpu(), cpu);
        assert_eq!(ir.worker_threads(), 128);
        assert_eq!(ir.batch(), 8);
        assert_eq!(ir.total_worker_executions(), 1024);
        assert_eq!(ir.isa(), Isa::Avx512);
        let first = ir.lower_checked(Target::AmdEpyc, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::AmdEpyc, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=amd_epyc\n"));
        assert!(first.canonical_ir.contains("worker_threads=128\n"));
        assert!(first
            .canonical_ir
            .contains("total_worker_executions=1024\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("WorkerPlan::new(128u32, 8u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_cpus_and_bad_bounds() {
        assert_eq!(
            declaration("EPYC FFT", 128, 8).normalize(),
            Err(AmdEpycDiagnostic::BadCpu)
        );
        let cpu = first_avx512_cpu();
        assert_eq!(
            declaration(cpu, 0, 8).normalize(),
            Err(AmdEpycDiagnostic::WorkerThreadsZero)
        );
        assert_eq!(
            declaration(cpu, MAX_WORKER_THREADS + 1, 8).normalize(),
            Err(AmdEpycDiagnostic::WorkerThreadsTooLarge)
        );
        assert_eq!(
            declaration(cpu, 128, 0).normalize(),
            Err(AmdEpycDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(cpu, 128, MAX_BATCH + 1).normalize(),
            Err(AmdEpycDiagnostic::BatchTooLarge)
        );
        let mut zero_workspace = declaration(cpu, 128, 8);
        zero_workspace.workspace_bytes = 0;
        assert_eq!(
            zero_workspace.normalize(),
            Err(AmdEpycDiagnostic::WorkspaceBytesZero)
        );
        let mut huge_workspace = declaration(cpu, 128, 8);
        huge_workspace.workspace_bytes = MAX_WORKSPACE_BYTES + 1;
        assert_eq!(
            huge_workspace.normalize(),
            Err(AmdEpycDiagnostic::WorkspaceBytesTooLarge)
        );
        // Naples ships at most 32 cores / 64 threads: 65 requested threads
        // pass the plan bound but exceed the generation.
        assert_eq!(
            declaration("EPYC 7001 (Naples)", 65, 8).normalize(),
            Err(AmdEpycDiagnostic::ThreadsExceedGeneration)
        );
        // The dense Turin top SKU bound (192 cores times 2 threads) is
        // representable at the full batch bound.
        assert!(declaration("EPYC 9005 (Turin)", 384, MAX_BATCH)
            .normalize()
            .is_ok());
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let cpu = first_avx512_cpu();
        let ir = declaration(cpu, 128, 8).normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(AmdEpycDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    x86_64_runtime: false,
                    ..CAPABILITIES
                },
                AmdEpycDiagnostic::MissingX86_64Runtime,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                AmdEpycDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::AmdEpyc, missing), Err(expected));
        }
        for (isa, missing, expected) in [
            (
                Isa::Avx,
                RuntimeCapabilities {
                    avx: false,
                    ..CAPABILITIES
                },
                AmdEpycDiagnostic::MissingAvx,
            ),
            (
                Isa::Avx2,
                RuntimeCapabilities {
                    avx2: false,
                    ..CAPABILITIES
                },
                AmdEpycDiagnostic::MissingAvx2,
            ),
            (
                Isa::Avx512,
                RuntimeCapabilities {
                    avx512: false,
                    ..CAPABILITIES
                },
                AmdEpycDiagnostic::MissingAvx512,
            ),
        ] {
            let mut isa_declaration = declaration(cpu, 128, 8);
            isa_declaration.isa = isa;
            let isa_ir = isa_declaration.normalize().unwrap();
            assert_eq!(
                isa_ir.lower_checked(Target::AmdEpyc, missing),
                Err(expected)
            );
        }
        // Requesting AVX-512 on pre-Zen-4 silicon fails closed on the
        // accelerator, not the host.
        let naples_ir = declaration("EPYC 7001 (Naples)", 32, 8)
            .normalize()
            .unwrap();
        assert_eq!(
            naples_ir.lower_checked(Target::AmdEpyc, CAPABILITIES),
            Err(AmdEpycDiagnostic::UnsupportedAccelerator)
        );
    }

    #[test]
    fn every_generation_lowers_with_sse3_and_avx512_fails_closed_pre_zen4() {
        let mut lowered = 0;
        for (name, facts) in GENERATIONS {
            let mut sse3_declaration = declaration(name, 4, 2);
            sse3_declaration.isa = Isa::Sse3;
            sse3_declaration
                .normalize()
                .unwrap()
                .lower_checked(Target::AmdEpyc, CAPABILITIES)
                .unwrap();
            let ir = declaration(name, 4, 2).normalize().unwrap();
            if !facts.avx512 {
                assert_eq!(
                    ir.lower_checked(Target::AmdEpyc, CAPABILITIES),
                    Err(AmdEpycDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::AmdEpyc, CAPABILITIES).unwrap();
            lowered += 1;
        }
        assert!(
            lowered > 0,
            "catalog must lower at least one AVX-512 generation"
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_amd_epyc_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let cpu = first_avx512_cpu();
        let artifact = declaration(cpu, 128, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::AmdEpyc, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/amd_epyc_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod amd_epyc_runtime;\n{}\nfn main() {{\n    let plan = make_worker_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.worker_threads(), 128);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_worker_executions(), 384);\n    assert_eq!(plan.workspace_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-amd-epyc-factory-{}-{nonce}",
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
