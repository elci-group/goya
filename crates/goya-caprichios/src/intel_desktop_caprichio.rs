//! A bounded Intel desktop CPU-compute capability.
//!
//! The negotiated artifact is a checked worker plan and a factory binding
//! to this crate's plan runtime. It does not spawn threads, execute SIMD,
//! or prove that a CPU executed work. Hosts on Intel desktop CPUs report
//! their real facilities through [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.cpu_compute.intel_desktop.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORKER_THREADS: u32 = crate::intel_desktop_runtime::MAX_WORKER_THREADS;
pub const MAX_BATCH: u32 = crate::intel_desktop_runtime::MAX_BATCH;
pub const MAX_WORKSPACE_BYTES: u64 = crate::intel_desktop_runtime::MAX_WORKSPACE_BYTES;

/// Published CPU facts for one Intel desktop generation (family level).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuFacts {
    /// Architecture name (for example `Nehalem`, `Raptor Lake`).
    pub arch: &'static str,
    /// Desktop socket the family shipped on.
    pub socket: &'static str,
    /// Lowest marketed core count in the desktop lineup.
    pub cores_min: u32,
    /// Highest marketed core count in the desktop lineup.
    pub cores_max: u32,
    /// OS-visible hardware threads per documented core (`1` or `2`).
    /// The `cores_max * threads_per_core` product is an upper bound
    /// only: hybrid generations expose Hyper-Threading on P-cores
    /// alone, so the documented total (`threads_max`) sits below it.
    pub threads_per_core: u32,
    /// Vendor-documented maximum OS-visible hardware threads for the
    /// family (Intel publishes Total Threads per SKU); equals cores_max
    /// × threads_per_core on homogeneous generations, and hybrid P+E
    /// generations sit below that product.
    pub threads_max: u32,
    /// Threading technology: `none` or `SMT`. Intel brands its SMT as
    /// Hyper-Threading; the house string is `SMT`.
    pub thread_tech: &'static str,
    /// Documented AVX ISA support.
    pub avx: bool,
    /// Documented AVX2 ISA support.
    pub avx2: bool,
    /// Documented AVX-512 ISA support on shipping desktop parts.
    pub avx512: bool,
    /// Vendor/foundry process node string (for example `Intel 7`).
    pub process: &'static str,
    /// First desktop launch year of the family.
    pub year: u16,
}

/// Catalog order is chronological (family launch year); each entry is a
/// family-level desktop generation, not a SKU, and the `year` field records
/// the family's first desktop launch year. Facts come from Intel product
/// pages (ARK) and the public product record. Mobile Core lines and Xeon
/// server/workstation parts are excluded: they live in their own
/// caprichios. Pre-2008 Conroe (65nm Core 2) is excluded as well. The HEDT
/// Core-X block is grouped after the mainstream block by design; its rows
/// are desktop silicon.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, CpuFacts)] = &[
    ("Core 2 (Penryn)",              CpuFacts { arch: "Penryn",         socket: "LGA775",  cores_min: 2,  cores_max: 4,  threads_per_core: 1, threads_max: 4,  thread_tech: "none", avx: false, avx2: false, avx512: false, process: "45nm",     year: 2008 }),
    ("Nehalem (Core i7-900)",        CpuFacts { arch: "Nehalem",        socket: "LGA1366", cores_min: 4,  cores_max: 4,  threads_per_core: 2, threads_max: 8,  thread_tech: "SMT",  avx: false, avx2: false, avx512: false, process: "45nm",     year: 2008 }),
    // Gulftown six-cores; AES-NI arrived with this generation. The
    // dual-core Clarkdale parts shipped on LGA1156 and are out of scope
    // for this LGA1366 row.
    ("Westmere",                     CpuFacts { arch: "Westmere",       socket: "LGA1366", cores_min: 6,  cores_max: 6,  threads_per_core: 2, threads_max: 12, thread_tech: "SMT",  avx: false, avx2: false, avx512: false, process: "32nm",     year: 2010 }),
    ("Sandy Bridge",                 CpuFacts { arch: "Sandy Bridge",   socket: "LGA1155", cores_min: 4,  cores_max: 4,  threads_per_core: 2, threads_max: 8,  thread_tech: "SMT",  avx: true,  avx2: false, avx512: false, process: "32nm",     year: 2011 }),
    ("Ivy Bridge",                   CpuFacts { arch: "Ivy Bridge",     socket: "LGA1155", cores_min: 4,  cores_max: 4,  threads_per_core: 2, threads_max: 8,  thread_tech: "SMT",  avx: true,  avx2: false, avx512: false, process: "22nm",     year: 2012 }),
    ("Haswell",                      CpuFacts { arch: "Haswell",        socket: "LGA1150", cores_min: 4,  cores_max: 4,  threads_per_core: 2, threads_max: 8,  thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "22nm",     year: 2013 }),
    // Desktop Broadwell SKUs (i7-5775C class) shipped mid-2015; 2014 is
    // the family's first launch year (Core M).
    ("Broadwell",                    CpuFacts { arch: "Broadwell",      socket: "LGA1150", cores_min: 4,  cores_max: 4,  threads_per_core: 2, threads_max: 8,  thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm",     year: 2014 }),
    // AVX-512 was never enabled on mainstream Skylake desktop parts.
    ("Skylake",                      CpuFacts { arch: "Skylake",        socket: "LGA1151", cores_min: 4,  cores_max: 4,  threads_per_core: 2, threads_max: 8,  thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm",     year: 2015 }),
    ("Kaby Lake",                    CpuFacts { arch: "Kaby Lake",      socket: "LGA1151", cores_min: 4,  cores_max: 4,  threads_per_core: 2, threads_max: 8,  thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm",     year: 2017 }),
    ("Coffee Lake",                  CpuFacts { arch: "Coffee Lake",    socket: "LGA1151", cores_min: 4,  cores_max: 8,  threads_per_core: 2, threads_max: 16, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm",     year: 2017 }),
    ("Comet Lake",                   CpuFacts { arch: "Comet Lake",     socket: "LGA1200", cores_min: 4,  cores_max: 10, threads_per_core: 2, threads_max: 20, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm",     year: 2020 }),
    // First mainstream desktop generation with AVX-512.
    ("Rocket Lake",                  CpuFacts { arch: "Rocket Lake",    socket: "LGA1200", cores_min: 8,  cores_max: 8,  threads_per_core: 2, threads_max: 16, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: true,  process: "14nm",     year: 2021 }),
    // Hybrid 8P+8E flagship (i9 class); Hyper-Threading exists on P-cores
    // only, so the family documents 24 OS-visible threads (threads_max),
    // below the 32 upper bound the cores_max * threads_per_core product
    // gives. AVX-512 is fused off on production parts (the E-cores never
    // implemented it, and Intel fused it off in silicon going forward).
    ("Alder Lake",                   CpuFacts { arch: "Alder Lake",     socket: "LGA1700", cores_min: 16, cores_max: 16, threads_per_core: 2, threads_max: 24, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "Intel 7",  year: 2021 }),
    // Hybrid 8P+16E flagship: 32 OS-visible threads (8 Hyper-Threaded
    // P-cores + 16 E-cores), recorded as threads_max; the homogeneous
    // cores_max * threads_per_core product (48) is only an upper bound.
    // AVX-512 remains disabled.
    ("Raptor Lake",                  CpuFacts { arch: "Raptor Lake",    socket: "LGA1700", cores_min: 24, cores_max: 24, threads_per_core: 2, threads_max: 32, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "Intel 7",  year: 2022 }),
    // Hyper-Threading removed (Lion Cove): 24 cores / 24 threads, one
    // thread per core. Lion Cove P-cores implement AVX-512, but it is
    // disabled on shipping parts because Skymont E-cores lack it. The
    // compute tile moved from the cancelled Intel 20A node to TSMC N3B.
    ("Arrow Lake (Core Ultra 200S)", CpuFacts { arch: "Arrow Lake",     socket: "LGA1851", cores_min: 24, cores_max: 24, threads_per_core: 1, threads_max: 24, thread_tech: "none", avx: true,  avx2: true,  avx512: false, process: "TSMC N3B", year: 2024 }),
    // HEDT block (desktop silicon). The row covers the 6-18 core
    // X-series parts; the 4-core i7-7640X/7740X are Kaby-derived and out
    // of scope. 18 cores with SMT document 36 hardware threads; the v1
    // plan bound of 36 (Cascade Lake-X 18C/36T) covers exactly that,
    // and the catalog records the hardware fact rather than trimming it.
    ("Core-X (Skylake-X)",           CpuFacts { arch: "Skylake-X",      socket: "LGA2066", cores_min: 6,  cores_max: 18, threads_per_core: 2, threads_max: 36, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: true,  process: "14nm",     year: 2017 }),
    ("Cascade Lake-X",               CpuFacts { arch: "Cascade Lake-X", socket: "LGA2066", cores_min: 8,  cores_max: 18, threads_per_core: 2, threads_max: 36, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: true,  process: "14nm",     year: 2019 }),
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
    /// Intel desktop CPU hardware (post-2008 socketed desktop families).
    IntelDesktop,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports an Intel desktop CPU is present and provides an OS
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
pub struct IntelDesktopDeclaration {
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
pub struct IntelDesktopIr {
    cpu: String,
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
    isa: Isa,
    facts: &'static CpuFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntelDesktopDiagnostic {
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

impl fmt::Display for IntelDesktopDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadCpu => "generation is not in the Intel desktop CPU catalog",
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
            Self::UnsupportedTarget => {
                "Intel desktop CPU compute supports only Intel desktop targets"
            }
            Self::UnsupportedAccelerator => {
                "generation does not document the required ISA extension"
            }
            Self::MissingX86_64Runtime => {
                "target runtime must report an Intel desktop CPU with an OS thread API"
            }
            Self::MissingAvx => "target runtime must report AVX support",
            Self::MissingAvx2 => "target runtime must report AVX2 support",
            Self::MissingAvx512 => "target runtime must report AVX-512 support",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for IntelDesktopDiagnostic {}

/// Negotiated worker design and source text. The artifact is a plan and a
/// factory definition; successful construction is not proof that a CPU ran
/// the workers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntelDesktopArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl IntelDesktopDeclaration {
    pub fn normalize(self) -> Result<IntelDesktopIr, IntelDesktopDiagnostic> {
        let facts = facts_for(&self.cpu).ok_or(IntelDesktopDiagnostic::BadCpu)?;
        let plan = crate::intel_desktop_runtime::WorkerPlan::new(
            self.worker_threads,
            self.batch,
            self.workspace_bytes,
        )
        .map_err(|error| match error {
            crate::intel_desktop_runtime::WorkerError::ZeroWorkerThreads => {
                IntelDesktopDiagnostic::WorkerThreadsZero
            }
            crate::intel_desktop_runtime::WorkerError::WorkerThreadsTooLarge => {
                IntelDesktopDiagnostic::WorkerThreadsTooLarge
            }
            crate::intel_desktop_runtime::WorkerError::ZeroBatch => {
                IntelDesktopDiagnostic::BatchZero
            }
            crate::intel_desktop_runtime::WorkerError::BatchTooLarge => {
                IntelDesktopDiagnostic::BatchTooLarge
            }
            crate::intel_desktop_runtime::WorkerError::ZeroWorkspaceBytes => {
                IntelDesktopDiagnostic::WorkspaceBytesZero
            }
            crate::intel_desktop_runtime::WorkerError::WorkspaceBytesTooLarge => {
                IntelDesktopDiagnostic::WorkspaceBytesTooLarge
            }
            crate::intel_desktop_runtime::WorkerError::WorkerOverflow => {
                IntelDesktopDiagnostic::WorkerOverflow
            }
        })?;
        // Planned worker threads cannot exceed the generation's documented
        // hardware thread count.
        if u64::from(self.worker_threads) > u64::from(facts.threads_max) {
            return Err(IntelDesktopDiagnostic::ThreadsExceedGeneration);
        }
        debug_assert_eq!(plan.worker_threads(), self.worker_threads);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.workspace_bytes(), self.workspace_bytes);
        Ok(IntelDesktopIr {
            cpu: self.cpu,
            worker_threads: self.worker_threads,
            batch: self.batch,
            workspace_bytes: self.workspace_bytes,
            isa: self.isa,
            facts,
        })
    }
}

impl IntelDesktopIr {
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
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=intel_desktop\ncpu={}\narch={}\nsocket={}\ncores_min={}\ncores_max={}\nthreads_per_core={}\nthreads_max={}\nthread_tech={}\navx={}\navx2={}\navx512={}\nprocess={}\nyear={}\nisa={}\nworker_threads={}\nbatch={}\ntotal_worker_executions={}\nworkspace_bytes={}\nexecution=none_attempted\n",
            self.cpu,
            self.facts.arch,
            self.facts.socket,
            self.facts.cores_min,
            self.facts.cores_max,
            self.facts.threads_per_core,
            self.facts.threads_max,
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
    ) -> Result<IntelDesktopArtifact, IntelDesktopDiagnostic> {
        if target != Target::IntelDesktop {
            return Err(IntelDesktopDiagnostic::UnsupportedTarget);
        }
        if !capabilities.x86_64_runtime {
            return Err(IntelDesktopDiagnostic::MissingX86_64Runtime);
        }
        if !capabilities.utf8_strings {
            return Err(IntelDesktopDiagnostic::MissingUtf8Strings);
        }
        match self.isa {
            Isa::Sse3 => {}
            Isa::Avx => {
                if !capabilities.avx {
                    return Err(IntelDesktopDiagnostic::MissingAvx);
                }
                if !self.facts.avx {
                    return Err(IntelDesktopDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx2 => {
                if !capabilities.avx2 {
                    return Err(IntelDesktopDiagnostic::MissingAvx2);
                }
                if !self.facts.avx2 {
                    return Err(IntelDesktopDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx512 => {
                if !capabilities.avx512 {
                    return Err(IntelDesktopDiagnostic::MissingAvx512);
                }
                if !self.facts.avx512 {
                    return Err(IntelDesktopDiagnostic::UnsupportedAccelerator);
                }
            }
        }
        Ok(IntelDesktopArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; Intel desktop CPU worker plan, no worker execution is attempted.\npub fn make_worker_plan() -> Result<crate::intel_desktop_runtime::WorkerPlan, crate::intel_desktop_runtime::WorkerError> {{\n    crate::intel_desktop_runtime::WorkerPlan::new({}u32, {}u32, {}u64)\n}}\n",
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

    fn declaration(cpu: &str, worker_threads: u32, batch: u32) -> IntelDesktopDeclaration {
        IntelDesktopDeclaration {
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

    /// Number of leading mainstream rows before the HEDT Core-X block.
    const MAINSTREAM_ROWS: usize = 15;

    #[test]
    fn catalog_entries_have_consistent_facts() {
        assert!(!GENERATIONS.is_empty(), "catalog must not be empty");
        assert_eq!(
            GENERATIONS.len(),
            MAINSTREAM_ROWS + 2,
            "15 mainstream rows plus the two HEDT rows"
        );
        let mut seen = std::collections::BTreeSet::new();
        for (name, facts) in GENERATIONS {
            assert!(seen.insert(*name), "duplicate generation {name}");
            assert!(matches!(
                facts.arch,
                "Penryn"
                    | "Nehalem"
                    | "Westmere"
                    | "Sandy Bridge"
                    | "Ivy Bridge"
                    | "Haswell"
                    | "Broadwell"
                    | "Skylake"
                    | "Kaby Lake"
                    | "Coffee Lake"
                    | "Comet Lake"
                    | "Rocket Lake"
                    | "Alder Lake"
                    | "Raptor Lake"
                    | "Arrow Lake"
                    | "Skylake-X"
                    | "Cascade Lake-X"
            ));
            assert!(matches!(
                facts.socket,
                "LGA775"
                    | "LGA1366"
                    | "LGA1155"
                    | "LGA1150"
                    | "LGA1151"
                    | "LGA1200"
                    | "LGA1700"
                    | "LGA1851"
                    | "LGA2066"
            ));
            assert!((2008..=2030).contains(&facts.year));
            assert!(facts.cores_min >= 1 && facts.cores_min <= facts.cores_max);
            assert!(facts.threads_per_core >= 1);
            let hardware_threads = u64::from(facts.cores_max) * u64::from(facts.threads_per_core);
            match *name {
                // Raptor Lake's homogeneous 24 * 2 = 48 upper bound exceeds
                // the v1 cap, but the part really exposes 32 OS threads
                // (8 Hyper-Threaded P-cores + 16 E-cores), so planning at
                // the cap stays within hardware.
                "Raptor Lake" => assert_eq!(hardware_threads, 48),
                // HEDT rows document 18 cores with SMT (36 hardware
                // threads), above the 32-thread v1 plan cap; the runtime
                // bound fails closed there, and the catalog keeps the
                // hardware fact.
                "Core-X (Skylake-X)" | "Cascade Lake-X" => {
                    assert_eq!(hardware_threads, 36, "{name} HEDT hardware threads")
                }
                _ => assert!(
                    hardware_threads <= u64::from(MAX_WORKER_THREADS),
                    "{name} thread count must fit the supported bound"
                ),
            }
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
                assert!(matches!(
                    facts.arch,
                    "Rocket Lake" | "Skylake-X" | "Cascade Lake-X"
                ));
            }
        }
        // Each block is chronological and never regresses; the HEDT block
        // is grouped after the mainstream block, so its older launch
        // years follow newer mainstream ones by design.
        let mainstream: Vec<u16> = GENERATIONS[..MAINSTREAM_ROWS]
            .iter()
            .map(|(_, facts)| facts.year)
            .collect();
        assert!(mainstream.windows(2).all(|pair| pair[0] <= pair[1]));
        let hedt: Vec<u16> = GENERATIONS[MAINSTREAM_ROWS..]
            .iter()
            .map(|(_, facts)| facts.year)
            .collect();
        assert!(hedt.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let mut decl = declaration("Rocket Lake", 8, 4);
        decl.isa = Isa::Avx512;
        let ir = decl.normalize().unwrap();
        assert_eq!(ir.cpu(), "Rocket Lake");
        assert_eq!(ir.worker_threads(), 8);
        assert_eq!(ir.batch(), 4);
        assert_eq!(ir.isa(), Isa::Avx512);
        assert_eq!(ir.total_worker_executions(), 32);
        assert_eq!(ir.facts().arch, "Rocket Lake");
        let first = ir
            .lower_checked(Target::IntelDesktop, CAPABILITIES)
            .unwrap();
        let second = ir
            .lower_checked(Target::IntelDesktop, CAPABILITIES)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=intel_desktop\n"));
        assert!(first.canonical_ir.contains("arch=Rocket Lake\n"));
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
            declaration("Pentium FFT", 8, 4).normalize(),
            Err(IntelDesktopDiagnostic::BadCpu)
        );
        let cpu = first_avx512_generation();
        assert_eq!(
            declaration(cpu, 0, 4).normalize(),
            Err(IntelDesktopDiagnostic::WorkerThreadsZero)
        );
        assert_eq!(
            declaration(cpu, MAX_WORKER_THREADS + 1, 4).normalize(),
            Err(IntelDesktopDiagnostic::WorkerThreadsTooLarge)
        );
        assert_eq!(
            declaration(cpu, 8, 0).normalize(),
            Err(IntelDesktopDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(cpu, 8, MAX_BATCH + 1).normalize(),
            Err(IntelDesktopDiagnostic::BatchTooLarge)
        );
        let mut zero_workspace = declaration(cpu, 8, 4);
        zero_workspace.workspace_bytes = 0;
        assert_eq!(
            zero_workspace.normalize(),
            Err(IntelDesktopDiagnostic::WorkspaceBytesZero)
        );
        let mut huge_workspace = declaration(cpu, 8, 4);
        huge_workspace.workspace_bytes = MAX_WORKSPACE_BYTES + 1;
        assert_eq!(
            huge_workspace.normalize(),
            Err(IntelDesktopDiagnostic::WorkspaceBytesTooLarge)
        );
        // Core 2 (Penryn) documents four cores and one thread per core, so
        // eight planned worker threads exceed the generation's hardware
        // threads.
        assert_eq!(
            declaration("Core 2 (Penryn)", 8, 4).normalize(),
            Err(IntelDesktopDiagnostic::ThreadsExceedGeneration)
        );
        assert!(declaration("Core 2 (Penryn)", 4, 4).normalize().is_ok());
        assert!(declaration("Raptor Lake", MAX_WORKER_THREADS, MAX_BATCH)
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
                Err(IntelDesktopDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    x86_64_runtime: false,
                    ..CAPABILITIES
                },
                IntelDesktopDiagnostic::MissingX86_64Runtime,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                IntelDesktopDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::IntelDesktop, missing),
                Err(expected)
            );
        }
        // Each ISA diagnostic is exercised on its own negotiated IR.
        let mut avx_decl = declaration(cpu, 8, 4);
        avx_decl.isa = Isa::Avx;
        let avx_ir = avx_decl.normalize().unwrap();
        assert_eq!(
            avx_ir.lower_checked(
                Target::IntelDesktop,
                RuntimeCapabilities {
                    avx: false,
                    ..CAPABILITIES
                }
            ),
            Err(IntelDesktopDiagnostic::MissingAvx)
        );
        let mut avx2_decl = declaration(cpu, 8, 4);
        avx2_decl.isa = Isa::Avx2;
        let avx2_ir = avx2_decl.normalize().unwrap();
        assert_eq!(
            avx2_ir.lower_checked(
                Target::IntelDesktop,
                RuntimeCapabilities {
                    avx2: false,
                    ..CAPABILITIES
                }
            ),
            Err(IntelDesktopDiagnostic::MissingAvx2)
        );
        let mut avx512_decl = declaration(cpu, 8, 4);
        avx512_decl.isa = Isa::Avx512;
        let avx512_ir = avx512_decl.normalize().unwrap();
        assert_eq!(
            avx512_ir.lower_checked(
                Target::IntelDesktop,
                RuntimeCapabilities {
                    avx512: false,
                    ..CAPABILITIES
                }
            ),
            Err(IntelDesktopDiagnostic::MissingAvx512)
        );
        // Silicon without documented AVX-512 fails closed even with a fully
        // capable host.
        let mut old_decl = declaration("Core 2 (Penryn)", 4, 4);
        old_decl.isa = Isa::Avx512;
        let old_ir = old_decl.normalize().unwrap();
        assert_eq!(
            old_ir.lower_checked(Target::IntelDesktop, CAPABILITIES),
            Err(IntelDesktopDiagnostic::UnsupportedAccelerator)
        );
    }

    #[test]
    fn every_generation_lowers_on_sse3_and_avx512_fails_closed_where_absent() {
        let mut lowered_sse3 = 0;
        let mut lowered_avx512 = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 2, 2).normalize().unwrap();
            ir.lower_checked(Target::IntelDesktop, CAPABILITIES)
                .unwrap();
            lowered_sse3 += 1;
            let mut decl = declaration(name, 2, 2);
            decl.isa = Isa::Avx512;
            let ir = decl.normalize().unwrap();
            if !facts.avx512 {
                assert_eq!(
                    ir.lower_checked(Target::IntelDesktop, CAPABILITIES),
                    Err(IntelDesktopDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::IntelDesktop, CAPABILITIES)
                .unwrap();
            lowered_avx512 += 1;
        }
        assert_eq!(lowered_sse3, GENERATIONS.len(), "SSE3 lowers everywhere");
        assert_eq!(
            lowered_avx512, 3,
            "Rocket Lake, Core-X (Skylake-X) and Cascade Lake-X must lower AVX-512 plans"
        );
        // Alder Lake, Raptor Lake and Arrow Lake document AVX-512 as fused
        // off or disabled, so they fail closed above.
        for name in ["Alder Lake", "Raptor Lake", "Arrow Lake (Core Ultra 200S)"] {
            assert!(
                !facts_for(name).expect("row must exist").avx512,
                "{name} must record AVX-512 as unavailable"
            );
        }
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_intel_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let cpu = first_avx512_generation();
        let mut decl = declaration(cpu, 8, 3);
        decl.isa = Isa::Avx512;
        let artifact = decl
            .normalize()
            .unwrap()
            .lower_checked(Target::IntelDesktop, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/intel_desktop_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod intel_desktop_runtime;\n{}\nfn main() {{\n    let plan = make_worker_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.worker_threads(), 8);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_worker_executions(), 24);\n    assert_eq!(plan.workspace_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-intel-desktop-factory-{}-{nonce}",
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
