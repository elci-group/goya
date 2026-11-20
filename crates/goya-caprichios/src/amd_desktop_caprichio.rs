//! A bounded AMD desktop CPU-compute capability.
//!
//! The negotiated artifact is a checked worker plan and a factory binding
//! to this crate's plan runtime. It does not spawn threads, execute SIMD,
//! or prove that a CPU executed work. Hosts on AMD desktop CPUs report
//! their real facilities through [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.cpu_compute.amd_desktop.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORKER_THREADS: u32 = crate::amd_desktop_runtime::MAX_WORKER_THREADS;
pub const MAX_BATCH: u32 = crate::amd_desktop_runtime::MAX_BATCH;
pub const MAX_WORKSPACE_BYTES: u64 = crate::amd_desktop_runtime::MAX_WORKSPACE_BYTES;

/// Published CPU facts for one AMD desktop generation (family level).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuFacts {
    /// Architecture name (for example `K10`, `Zen 4`).
    pub arch: &'static str,
    /// Desktop socket the family shipped on; historical dual labels use the
    /// `AM2+/AM3` style.
    pub socket: &'static str,
    /// Lowest marketed core count in the desktop lineup.
    pub cores_min: u32,
    /// Highest marketed core count in the desktop lineup.
    pub cores_max: u32,
    /// Hardware threads per documented core (`1`, or `2` where the
    /// generation documented module (CMT) or SMT threading).
    pub threads_per_core: u32,
    /// Threading technology: `none`, `CMT` (module threading), or `SMT`.
    pub thread_tech: &'static str,
    /// Documented AVX ISA support.
    pub avx: bool,
    /// Documented AVX2 ISA support.
    pub avx2: bool,
    /// Documented AVX-512 ISA support.
    pub avx512: bool,
    /// Vendor/marketing process node string (for example `TSMC 5nm`).
    pub process: &'static str,
    /// First desktop launch year of the family.
    pub year: u16,
}

/// Catalog order is chronological (family launch year); each entry is a
/// family-level desktop generation, not a SKU, and the `year` field records
/// the family's first desktop launch year. Facts come from AMD product
/// briefs and the public product record. APU families (A4/A6/A8/A10 and the
/// Athlon 200GE) are excluded: they live in their own APU caprichio, and
/// HEDT Threadripper and server EPYC live in their own caprichios as well.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, CpuFacts)] = &[
    ("Phenom II",       CpuFacts { arch: "K10",        socket: "AM2+/AM3", cores_min: 2, cores_max: 6,  threads_per_core: 1, thread_tech: "none", avx: false, avx2: false, avx512: false, process: "45nm SOI", year: 2008 }),
    ("Athlon II",       CpuFacts { arch: "K10",        socket: "AM2+/AM3", cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: false, avx2: false, avx512: false, process: "45nm SOI", year: 2009 }),
    ("FX (Bulldozer)",  CpuFacts { arch: "Bulldozer",  socket: "AM3+",     cores_min: 4, cores_max: 8,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: false, avx512: false, process: "32nm SOI", year: 2011 }),
    ("FX (Piledriver)", CpuFacts { arch: "Piledriver", socket: "AM3+",     cores_min: 4, cores_max: 8,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: false, avx512: false, process: "32nm SOI", year: 2012 }),
    ("Athlon X4",       CpuFacts { arch: "Excavator",  socket: "FM2+/AM4", cores_min: 4, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: true,  avx512: false, process: "GF 28nm",  year: 2016 }),
    ("Ryzen 1000",      CpuFacts { arch: "Zen",        socket: "AM4",      cores_min: 4, cores_max: 8,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "GF 14nm",  year: 2017 }),
    ("Ryzen 2000",      CpuFacts { arch: "Zen+",       socket: "AM4",      cores_min: 4, cores_max: 8,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "GF 12nm",  year: 2018 }),
    ("Ryzen 3000",      CpuFacts { arch: "Zen 2",      socket: "AM4",      cores_min: 6, cores_max: 16, threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "TSMC 7nm", year: 2019 }),
    ("Ryzen 5000",      CpuFacts { arch: "Zen 3",      socket: "AM4",      cores_min: 6, cores_max: 16, threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "TSMC 7nm", year: 2020 }),
    ("Ryzen 7000",      CpuFacts { arch: "Zen 4",      socket: "AM5",      cores_min: 6, cores_max: 16, threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: true,  process: "TSMC 5nm", year: 2022 }),
    ("Ryzen 9000",      CpuFacts { arch: "Zen 5",      socket: "AM5",      cores_min: 6, cores_max: 16, threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: true,  process: "TSMC 4nm", year: 2024 }),
];

/// Per-generation lookup. Unknown generations return None; the catalog never
/// guesses.
pub fn facts_for(cpu: &str) -> Option<&'static CpuFacts> {
    GENERATIONS
        .iter()
        .find(|(name, _)| *name == cpu)
        .map(|(_, facts)| facts)
}

/// The ISA extension the planned workers are compiled for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Isa {
    Sse3,
    Avx,
    Avx2,
    Avx512,
}

impl Isa {
    fn as_str(self) -> &'static str {
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
    /// AMD desktop CPU hardware (post-2008 socketed desktop families).
    AmdDesktop,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports an AMD desktop CPU is present and provides an OS
    /// thread API.
    pub x86_64_runtime: bool,
    /// Host reports AVX support in its CPU feature set.
    pub avx: bool,
    /// Host reports AVX2 support in its CPU feature set.
    pub avx2: bool,
    /// Host reports AVX-512 support in its CPU feature set.
    pub avx512: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdDesktopDeclaration {
    /// Exact generation name from the catalog; unknown names never normalize.
    pub cpu: String,
    /// Worker threads, from one through `MAX_WORKER_THREADS` and within the
    /// generation's documented hardware thread count.
    pub worker_threads: u32,
    /// Batch size, from one through `MAX_BATCH`.
    pub batch: u32,
    /// Bound on the workspace the workers may address, from one through
    /// `MAX_WORKSPACE_BYTES`.
    pub workspace_bytes: u64,
    /// The ISA extension the planned workers are compiled for.
    pub isa: Isa,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdDesktopIr {
    cpu: String,
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
    isa: Isa,
    facts: &'static CpuFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmdDesktopDiagnostic {
    BadCpu,
    WorkerThreadsZero,
    WorkerThreadsTooLarge,
    BatchZero,
    BatchTooLarge,
    WorkspaceBytesZero,
    WorkspaceBytesTooLarge,
    WorkerOverflow,
    /// Planned worker threads cannot exceed the generation's documented
    /// hardware thread count.
    ThreadsExceedGeneration,
    UnsupportedTarget,
    UnsupportedAccelerator,
    MissingX86_64Runtime,
    MissingAvx,
    MissingAvx2,
    MissingAvx512,
    MissingUtf8Strings,
}

impl fmt::Display for AmdDesktopDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadCpu => "generation is not in the AMD desktop CPU catalog",
            Self::WorkerThreadsZero => "worker thread count must be positive",
            Self::WorkerThreadsTooLarge => "worker thread count exceeds the supported bound",
            Self::BatchZero => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::WorkspaceBytesZero => "workspace byte bound must be positive",
            Self::WorkspaceBytesTooLarge => "workspace byte bound exceeds the supported bound",
            Self::WorkerOverflow => "total worker executions do not fit the supported bound",
            Self::ThreadsExceedGeneration => {
                "planned worker threads exceed the generation's documented hardware thread count"
            }
            Self::UnsupportedTarget => "AMD desktop CPU compute supports only AMD desktop targets",
            Self::UnsupportedAccelerator => {
                "generation does not document the required ISA extension"
            }
            Self::MissingX86_64Runtime => {
                "target runtime must report an AMD desktop CPU with an OS thread API"
            }
            Self::MissingAvx => "target runtime must report AVX support",
            Self::MissingAvx2 => "target runtime must report AVX2 support",
            Self::MissingAvx512 => "target runtime must report AVX-512 support",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for AmdDesktopDiagnostic {}

/// Negotiated worker design and source text. The artifact is a plan and a
/// factory definition; successful construction is not proof that a CPU ran
/// the workers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdDesktopArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl AmdDesktopDeclaration {
    pub fn normalize(self) -> Result<AmdDesktopIr, AmdDesktopDiagnostic> {
        let facts = facts_for(&self.cpu).ok_or(AmdDesktopDiagnostic::BadCpu)?;
        let plan = crate::amd_desktop_runtime::WorkerPlan::new(
            self.worker_threads,
            self.batch,
            self.workspace_bytes,
        )
        .map_err(|error| match error {
            crate::amd_desktop_runtime::WorkerError::ZeroWorkerThreads => {
                AmdDesktopDiagnostic::WorkerThreadsZero
            }
            crate::amd_desktop_runtime::WorkerError::WorkerThreadsTooLarge => {
                AmdDesktopDiagnostic::WorkerThreadsTooLarge
            }
            crate::amd_desktop_runtime::WorkerError::ZeroBatch => AmdDesktopDiagnostic::BatchZero,
            crate::amd_desktop_runtime::WorkerError::BatchTooLarge => {
                AmdDesktopDiagnostic::BatchTooLarge
            }
            crate::amd_desktop_runtime::WorkerError::ZeroWorkspaceBytes => {
                AmdDesktopDiagnostic::WorkspaceBytesZero
            }
            crate::amd_desktop_runtime::WorkerError::WorkspaceBytesTooLarge => {
                AmdDesktopDiagnostic::WorkspaceBytesTooLarge
            }
            crate::amd_desktop_runtime::WorkerError::WorkerOverflow => {
                AmdDesktopDiagnostic::WorkerOverflow
            }
        })?;
        // Planned worker threads cannot exceed the generation's documented
        // hardware thread count.
        let hardware_threads = u64::from(facts.cores_max) * u64::from(facts.threads_per_core);
        if u64::from(self.worker_threads) > hardware_threads {
            return Err(AmdDesktopDiagnostic::ThreadsExceedGeneration);
        }
        debug_assert_eq!(plan.worker_threads(), self.worker_threads);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.workspace_bytes(), self.workspace_bytes);
        Ok(AmdDesktopIr {
            cpu: self.cpu,
            worker_threads: self.worker_threads,
            batch: self.batch,
            workspace_bytes: self.workspace_bytes,
            isa: self.isa,
            facts,
        })
    }
}

impl AmdDesktopIr {
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

    pub fn facts(&self) -> &'static CpuFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=amd_desktop\ncpu={}\narch={}\nsocket={}\ncores_min={}\ncores_max={}\nthreads_per_core={}\nthread_tech={}\navx={}\navx2={}\navx512={}\nprocess={}\nyear={}\nisa={}\nworker_threads={}\nbatch={}\ntotal_worker_executions={}\nworkspace_bytes={}\nexecution=none_attempted\n",
            self.cpu,
            self.facts.arch,
            self.facts.socket,
            self.facts.cores_min,
            self.facts.cores_max,
            self.facts.threads_per_core,
            self.facts.thread_tech,
            self.facts.avx,
            self.facts.avx2,
            self.facts.avx512,
            self.facts.process,
            self.facts.year,
            self.isa.as_str(),
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
    ) -> Result<AmdDesktopArtifact, AmdDesktopDiagnostic> {
        if target != Target::AmdDesktop {
            return Err(AmdDesktopDiagnostic::UnsupportedTarget);
        }
        if !capabilities.x86_64_runtime {
            return Err(AmdDesktopDiagnostic::MissingX86_64Runtime);
        }
        if !capabilities.utf8_strings {
            return Err(AmdDesktopDiagnostic::MissingUtf8Strings);
        }
        match self.isa {
            Isa::Sse3 => {}
            Isa::Avx => {
                if !capabilities.avx {
                    return Err(AmdDesktopDiagnostic::MissingAvx);
                }
                if !self.facts.avx {
                    return Err(AmdDesktopDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx2 => {
                if !capabilities.avx2 {
                    return Err(AmdDesktopDiagnostic::MissingAvx2);
                }
                if !self.facts.avx2 {
                    return Err(AmdDesktopDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx512 => {
                if !capabilities.avx512 {
                    return Err(AmdDesktopDiagnostic::MissingAvx512);
                }
                if !self.facts.avx512 {
                    return Err(AmdDesktopDiagnostic::UnsupportedAccelerator);
                }
            }
        }
        Ok(AmdDesktopArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; AMD desktop CPU worker plan, no worker execution is attempted.\npub fn make_worker_plan() -> Result<crate::amd_desktop_runtime::WorkerPlan, crate::amd_desktop_runtime::WorkerError> {{\n    crate::amd_desktop_runtime::WorkerPlan::new({}u32, {}u32, {}u64)\n}}\n",
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

    fn declaration(cpu: &str, worker_threads: u32, batch: u32) -> AmdDesktopDeclaration {
        AmdDesktopDeclaration {
            cpu: cpu.to_string(),
            worker_threads,
            batch,
            workspace_bytes: 4096,
            isa: Isa::Sse3,
        }
    }

    fn first_avx512_generation() -> &'static str {
        // Tests lower AVX-512 plans, so start from the first generation with
        // documented AVX-512 support.
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
        for (name, facts) in GENERATIONS {
            assert!(seen.insert(*name), "duplicate generation {name}");
            assert!(matches!(
                facts.arch,
                "K10"
                    | "Bulldozer"
                    | "Piledriver"
                    | "Excavator"
                    | "Zen"
                    | "Zen+"
                    | "Zen 2"
                    | "Zen 3"
                    | "Zen 4"
                    | "Zen 5"
            ));
            assert!(matches!(
                facts.socket,
                "AM2+/AM3" | "AM3+" | "AM4" | "AM5" | "FM2+/AM4"
            ));
            assert!((2008..=2030).contains(&facts.year));
            assert!(facts.cores_min >= 1 && facts.cores_min <= facts.cores_max);
            assert!(facts.threads_per_core >= 1);
            assert!(
                u64::from(facts.cores_max) * u64::from(facts.threads_per_core)
                    <= u64::from(MAX_WORKER_THREADS),
                "{name} thread count must fit the supported bound"
            );
            assert_eq!(
                facts.thread_tech == "none",
                facts.threads_per_core == 1,
                "thread tech must be none exactly when there is one thread per core"
            );
            assert!(!facts.thread_tech.is_empty());
            assert!(!facts.process.is_empty());
            if !facts.avx {
                assert!(!facts.avx2 && !facts.avx512, "{name} has no AVX");
            }
            if !facts.avx2 {
                assert!(!facts.avx512, "{name} has no AVX2");
            }
            if facts.avx512 {
                assert!(matches!(facts.arch, "Zen 4" | "Zen 5"));
            }
        }
        // Family blocks appear in chronological order and never regress.
        let years: Vec<u16> = GENERATIONS.iter().map(|(_, facts)| facts.year).collect();
        assert!(years.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let mut decl = declaration("Ryzen 7000", 8, 4);
        decl.isa = Isa::Avx512;
        let ir = decl.normalize().unwrap();
        assert_eq!(ir.cpu(), "Ryzen 7000");
        assert_eq!(ir.worker_threads(), 8);
        assert_eq!(ir.batch(), 4);
        assert_eq!(ir.isa(), Isa::Avx512);
        assert_eq!(ir.total_worker_executions(), 32);
        assert_eq!(ir.facts().arch, "Zen 4");
        let first = ir.lower_checked(Target::AmdDesktop, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::AmdDesktop, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=amd_desktop\n"));
        assert!(first.canonical_ir.contains("arch=Zen 4\n"));
        assert!(first.canonical_ir.contains("worker_threads=8\n"));
        assert!(first.canonical_ir.contains("total_worker_executions=32\n"));
        assert!(first.canonical_ir.contains("isa=avx512\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("WorkerPlan::new(8u32, 4u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_cpus_and_bad_bounds() {
        assert_eq!(
            declaration("Ryzen FFT", 8, 4).normalize(),
            Err(AmdDesktopDiagnostic::BadCpu)
        );
        let cpu = first_avx512_generation();
        assert_eq!(
            declaration(cpu, 0, 4).normalize(),
            Err(AmdDesktopDiagnostic::WorkerThreadsZero)
        );
        assert_eq!(
            declaration(cpu, MAX_WORKER_THREADS + 1, 4).normalize(),
            Err(AmdDesktopDiagnostic::WorkerThreadsTooLarge)
        );
        assert_eq!(
            declaration(cpu, 8, 0).normalize(),
            Err(AmdDesktopDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(cpu, 8, MAX_BATCH + 1).normalize(),
            Err(AmdDesktopDiagnostic::BatchTooLarge)
        );
        let mut zero_workspace = declaration(cpu, 8, 4);
        zero_workspace.workspace_bytes = 0;
        assert_eq!(
            zero_workspace.normalize(),
            Err(AmdDesktopDiagnostic::WorkspaceBytesZero)
        );
        let mut huge_workspace = declaration(cpu, 8, 4);
        huge_workspace.workspace_bytes = MAX_WORKSPACE_BYTES + 1;
        assert_eq!(
            huge_workspace.normalize(),
            Err(AmdDesktopDiagnostic::WorkspaceBytesTooLarge)
        );
        // A Phenom II documents 6 cores and one thread per core, so eight
        // planned worker threads exceed the generation's hardware threads.
        assert_eq!(
            declaration("Phenom II", 8, 4).normalize(),
            Err(AmdDesktopDiagnostic::ThreadsExceedGeneration)
        );
        assert!(declaration("Phenom II", 6, 4).normalize().is_ok());
        assert!(declaration(cpu, MAX_WORKER_THREADS, MAX_BATCH)
            .normalize()
            .is_ok());
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let cpu = first_avx512_generation();
        let ir = declaration(cpu, 8, 4).normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(AmdDesktopDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    x86_64_runtime: false,
                    ..CAPABILITIES
                },
                AmdDesktopDiagnostic::MissingX86_64Runtime,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                AmdDesktopDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::AmdDesktop, missing), Err(expected));
        }
        // Each ISA diagnostic is exercised on its own negotiated IR.
        let mut avx_decl = declaration(cpu, 8, 4);
        avx_decl.isa = Isa::Avx;
        let avx_ir = avx_decl.normalize().unwrap();
        assert_eq!(
            avx_ir.lower_checked(
                Target::AmdDesktop,
                RuntimeCapabilities {
                    avx: false,
                    ..CAPABILITIES
                }
            ),
            Err(AmdDesktopDiagnostic::MissingAvx)
        );
        let mut avx2_decl = declaration(cpu, 8, 4);
        avx2_decl.isa = Isa::Avx2;
        let avx2_ir = avx2_decl.normalize().unwrap();
        assert_eq!(
            avx2_ir.lower_checked(
                Target::AmdDesktop,
                RuntimeCapabilities {
                    avx2: false,
                    ..CAPABILITIES
                }
            ),
            Err(AmdDesktopDiagnostic::MissingAvx2)
        );
        let mut avx512_decl = declaration(cpu, 8, 4);
        avx512_decl.isa = Isa::Avx512;
        let avx512_ir = avx512_decl.normalize().unwrap();
        assert_eq!(
            avx512_ir.lower_checked(
                Target::AmdDesktop,
                RuntimeCapabilities {
                    avx512: false,
                    ..CAPABILITIES
                }
            ),
            Err(AmdDesktopDiagnostic::MissingAvx512)
        );
        // Silicon without documented AVX-512 fails closed even with a fully
        // capable host.
        let mut old_decl = declaration("Phenom II", 6, 4);
        old_decl.isa = Isa::Avx512;
        let old_ir = old_decl.normalize().unwrap();
        assert_eq!(
            old_ir.lower_checked(Target::AmdDesktop, CAPABILITIES),
            Err(AmdDesktopDiagnostic::UnsupportedAccelerator)
        );
    }

    #[test]
    fn every_generation_lowers_on_sse3_and_avx512_fails_closed_before_zen4() {
        let mut lowered_sse3 = 0;
        let mut lowered_avx512 = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 2, 2).normalize().unwrap();
            ir.lower_checked(Target::AmdDesktop, CAPABILITIES).unwrap();
            lowered_sse3 += 1;
            let mut decl = declaration(name, 2, 2);
            decl.isa = Isa::Avx512;
            let ir = decl.normalize().unwrap();
            if !facts.avx512 {
                assert_eq!(
                    ir.lower_checked(Target::AmdDesktop, CAPABILITIES),
                    Err(AmdDesktopDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::AmdDesktop, CAPABILITIES).unwrap();
            lowered_avx512 += 1;
        }
        assert_eq!(lowered_sse3, GENERATIONS.len(), "SSE3 lowers everywhere");
        assert!(
            lowered_avx512 >= 2,
            "Zen 4 and Zen 5 must lower AVX-512 plans"
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_amd_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let cpu = first_avx512_generation();
        let mut decl = declaration(cpu, 8, 3);
        decl.isa = Isa::Avx512;
        let artifact = decl
            .normalize()
            .unwrap()
            .lower_checked(Target::AmdDesktop, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/amd_desktop_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod amd_desktop_runtime;\n{}\nfn main() {{\n    let plan = make_worker_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.worker_threads(), 8);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_worker_executions(), 24);\n    assert_eq!(plan.workspace_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-amd-desktop-factory-{}-{nonce}",
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
            .expect("run generated worker-plan factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
