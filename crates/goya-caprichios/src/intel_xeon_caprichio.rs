//! A bounded Intel Xeon server and workstation compute capability.
//!
//! The negotiated artifact is a checked worker plan and a factory binding
//! to this crate's plan runtime. It does not spawn threads, execute SIMD,
//! guarantee scheduling, or prove that a CPU executed work. Hosts on Xeon
//! platforms report their real facilities through [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.cpu_compute.intel_xeon.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORKER_THREADS: u32 = crate::intel_xeon_runtime::MAX_WORKER_THREADS;
pub const MAX_BATCH: u32 = crate::intel_xeon_runtime::MAX_BATCH;
pub const MAX_WORKSPACE_BYTES: u64 = crate::intel_xeon_runtime::MAX_WORKSPACE_BYTES;

/// Published processor facts for one Intel Xeon server or workstation
/// generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XeonFacts {
    /// Microarchitecture as Intel names it; E-core-only lines carry their
    /// E-core label (for example `Crestmont (E-core)`), and P-core-only
    /// lines carry their P-core label.
    pub arch: &'static str,
    /// Server or workstation socket package (`LGA1366` through `LGA7529`).
    pub socket: &'static str,
    /// Intel family name (for example `Xeon Scalable 4th gen (Sapphire Rapids)`).
    pub generation: &'static str,
    /// Smallest core count Intel published as a family SKU.
    pub cores_min: u32,
    /// Largest core count Intel published as a family SKU.
    pub cores_max: u32,
    /// Threads per core with Intel Hyper-Threading enabled. Sierra Forest
    /// E-cores run one thread per core; entry SKUs of some families ship
    /// with Hyper-Threading disabled while the family label records two.
    pub threads_per_core: u32,
    /// Intel's own name for its multithreading technology (`SMT`, called
    /// Hyper-Threading by Intel), or `none` on one-thread-per-core lines.
    pub thread_tech: &'static str,
    /// Memory channels the platform exposes per socket.
    pub mem_channels: u32,
    /// AMX matrix extensions, documented from Sapphire Rapids (2023)
    /// onward. Sierra Forest's Crestmont E-cores have none.
    pub amx: bool,
    /// AVX instruction-set support (documented from Sandy Bridge onward).
    pub avx: bool,
    /// AVX2 instruction-set support (documented from Haswell onward).
    pub avx2: bool,
    /// AVX-512 support, documented on server parts from Skylake-SP (2017)
    /// onward. Sierra Forest's Crestmont E-cores stop at AVX2 plus
    /// AVX-VNNI, so AVX-512 stays false there.
    pub avx512: bool,
    /// Compute-die manufacturing process node.
    pub process: &'static str,
    /// First release year.
    pub year: u16,
}

/// Catalog order is chronological by launch year across the server and
/// workstation platform blocks; the `year` field records each generation's
/// true launch year. Facts come from Intel product pages (ARK), platform
/// briefs, datasheets and press releases corroborated by independent
/// coverage; figures Intel did not publish are not guessed, and the
/// dual-generation Nehalem-EX/Westmere-EX row records both codenames in its
/// label. The catalog starts after the pre-2008 Harpertown (Xeon 5400)
/// generation. Desktop Core and mobile parts are out of scope here: this
/// crate models them in their own caprichios. Cooper Lake shipped on June
/// 18, 2020 as the 4P/8P half of "3rd gen" Xeon Scalable (bfloat16 DL
/// Boost) and shares that branding with Ice Lake-SP, so both labels name
/// the platform split. Xeon W-3500/W-2500 (August 2024) are a Sapphire
/// Rapids refresh — Intel shipped no Emerald Rapids workstation line, and
/// the label records that. Sierra Forest-AP (6900E, up to 288 E-cores) was
/// announced but never launched, so it is excluded; Clearwater Forest and
/// Diamond Rapids are excluded until Intel publishes corroborated shipping
/// facts.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, XeonFacts)] = &[
    // LGA1366 platform generations (DDR3)
    ("Xeon 5500 (Nehalem-EP)",   XeonFacts { arch: "Nehalem",                    socket: "LGA1366",  generation: "Xeon 5500 (Nehalem-EP)",   cores_min: 2,  cores_max: 4,   threads_per_core: 2, thread_tech: "SMT", mem_channels: 3,  amx: false, avx: false, avx2: false, avx512: false, process: "45 nm", year: 2009 }),
    ("Xeon 5600 (Westmere-EP)",  XeonFacts { arch: "Westmere",                   socket: "LGA1366",  generation: "Xeon 5600 (Westmere-EP)",  cores_min: 4,  cores_max: 6,   threads_per_core: 2, thread_tech: "SMT", mem_channels: 3,  amx: false, avx: false, avx2: false, avx512: false, process: "32 nm", year: 2010 }),
    // Westmere adds AES-NI, but both pre-Sandy-Bridge EP generations stop at SSE4.2.
    // LGA1567 multi-socket generation (DDR3); Nehalem-EX launched 2010 and the
    // Westmere-EX E7 refresh (10-core) followed on the same socket in 2011.
    ("Xeon 7500/E7 (Nehalem-EX → Westmere-EX)", XeonFacts { arch: "Nehalem-EX / Westmere-EX", socket: "LGA1567", generation: "Xeon 7500/E7 (Nehalem-EX → Westmere-EX)", cores_min: 4, cores_max: 10, threads_per_core: 2, thread_tech: "SMT", mem_channels: 4, amx: false, avx: false, avx2: false, avx512: false, process: "45 nm / 32 nm", year: 2010 }),
    // LGA2011 EP generations (DDR3)
    ("Xeon E5 v1 (Sandy Bridge-EP)", XeonFacts { arch: "Sandy Bridge",          socket: "LGA2011",  generation: "Xeon E5 v1 (Sandy Bridge-EP)", cores_min: 4,  cores_max: 8,   threads_per_core: 2, thread_tech: "SMT", mem_channels: 4,  amx: false, avx: true,  avx2: false, avx512: false, process: "32 nm", year: 2012 }),
    ("Xeon E5 v2 (Ivy Bridge-EP)",   XeonFacts { arch: "Ivy Bridge",            socket: "LGA2011",  generation: "Xeon E5 v2 (Ivy Bridge-EP)",   cores_min: 4,  cores_max: 12,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 4,  amx: false, avx: true,  avx2: false, avx512: false, process: "22 nm", year: 2013 }),
    // LGA2011-3 EP generations (DDR4); AVX2 starts with Haswell.
    ("Xeon E5 v3 (Haswell-EP)",      XeonFacts { arch: "Haswell",               socket: "LGA2011-3", generation: "Xeon E5 v3 (Haswell-EP)",     cores_min: 4,  cores_max: 18,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 4,  amx: false, avx: true,  avx2: true,  avx512: false, process: "22 nm", year: 2014 }),
    // Broadwell-EP tops out at the 22-core E5-2699 v4; the Broadwell-EX E7
    // refresh reached 24 cores on the same socket but the EP family label
    // records the E5 line's published range.
    ("Xeon E5 v4 (Broadwell-EP)",    XeonFacts { arch: "Broadwell",             socket: "LGA2011-3", generation: "Xeon E5 v4 (Broadwell-EP)",   cores_min: 4,  cores_max: 22,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 4,  amx: false, avx: true,  avx2: true,  avx512: false, process: "14 nm", year: 2016 }),
    // LGA3647 server generations (six-channel DDR4); server AVX-512 starts here.
    ("Xeon Scalable 1st gen (Skylake-SP)", XeonFacts { arch: "Skylake",         socket: "LGA3647",  generation: "Xeon Scalable 1st gen (Skylake-SP)", cores_min: 6,  cores_max: 28,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 6,  amx: false, avx: true,  avx2: true,  avx512: true,  process: "14 nm", year: 2017 }),
    // LGA2066 workstation generation (quad-channel DDR4); entry W-2100/W-2104
    // SKUs ship with Hyper-Threading disabled while the family records two.
    ("Xeon W-2100 (Skylake-W)",      XeonFacts { arch: "Skylake",               socket: "LGA2066",  generation: "Xeon W-2100 (Skylake-W)",     cores_min: 2,  cores_max: 18,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 4,  amx: false, avx: true,  avx2: true,  avx512: true,  process: "14 nm", year: 2017 }),
    // AVX-512 VNNI (DL Boost) starts with Cascade Lake.
    ("Xeon Scalable 2nd gen (Cascade Lake)", XeonFacts { arch: "Cascade Lake",  socket: "LGA3647",  generation: "Xeon Scalable 2nd gen (Cascade Lake)", cores_min: 6,  cores_max: 28,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 6,  amx: false, avx: true,  avx2: true,  avx512: true,  process: "14 nm", year: 2019 }),
    ("Xeon W-2200 (Cascade Lake-W)", XeonFacts { arch: "Cascade Lake",          socket: "LGA2066",  generation: "Xeon W-2200 (Cascade Lake-W)", cores_min: 6,  cores_max: 18,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 4,  amx: false, avx: true,  avx2: true,  avx512: true,  process: "14 nm", year: 2019 }),
    // LGA4189 Cooper Lake generation (six-channel DDR4): the 4P/8P half of
    // "3rd gen" Xeon Scalable, adding bfloat16 DL Boost on the same AVX-512
    // ISA as Cascade Lake.
    ("Xeon Scalable 3rd gen 4P/8P (Cooper Lake)", XeonFacts { arch: "Cooper Lake", socket: "LGA4189", generation: "Xeon Scalable 3rd gen 4P/8P (Cooper Lake)", cores_min: 24, cores_max: 28, threads_per_core: 2, thread_tech: "SMT", mem_channels: 6, amx: false, avx: true, avx2: true, avx512: true, process: "14 nm", year: 2020 }),
    // LGA4189 Whitley generation (eight-channel DDR4); the mainstream half
    // of "3rd gen" Xeon Scalable.
    ("Xeon Scalable 3rd gen (Ice Lake-SP)", XeonFacts { arch: "Ice Lake",        socket: "LGA4189",  generation: "Xeon Scalable 3rd gen (Ice Lake-SP)", cores_min: 8,  cores_max: 40,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  amx: false, avx: true,  avx2: true,  avx512: true,  process: "10 nm", year: 2021 }),
    ("Xeon W-3300 (Ice Lake-W)",     XeonFacts { arch: "Ice Lake",              socket: "LGA4189",  generation: "Xeon W-3300 (Ice Lake-W)",    cores_min: 12, cores_max: 38,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  amx: false, avx: true,  avx2: true,  avx512: true,  process: "10 nm", year: 2021 }),
    // LGA4677 generations (DDR5); AMX matrix extensions start with
    // Sapphire Rapids together with AVX-512 FP16.
    ("Xeon Scalable 4th gen (Sapphire Rapids)", XeonFacts { arch: "Sapphire Rapids", socket: "LGA4677", generation: "Xeon Scalable 4th gen (Sapphire Rapids)", cores_min: 8,  cores_max: 60,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  amx: true,  avx: true,  avx2: true,  avx512: true,  process: "Intel 7", year: 2023 }),
    ("Xeon W-3400 (Sapphire Rapids W)", XeonFacts { arch: "Sapphire Rapids",     socket: "LGA4677",  generation: "Xeon W-3400 (Sapphire Rapids W)", cores_min: 12, cores_max: 56,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  amx: true,  avx: true,  avx2: true,  avx512: true,  process: "Intel 7", year: 2023 }),
    // The W-2400 mainstream workstation platform exposes four DDR5 channels
    // where the W-3400 exposes eight.
    ("Xeon W-2400 (Sapphire Rapids W)", XeonFacts { arch: "Sapphire Rapids",     socket: "LGA4677",  generation: "Xeon W-2400 (Sapphire Rapids W)", cores_min: 8,  cores_max: 24,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 4,  amx: true,  avx: true,  avx2: true,  avx512: true,  process: "Intel 7", year: 2023 }),
    ("Xeon Scalable 5th gen (Emerald Rapids)", XeonFacts { arch: "Emerald Rapids", socket: "LGA4677", generation: "Xeon Scalable 5th gen (Emerald Rapids)", cores_min: 8, cores_max: 64,  threads_per_core: 2, thread_tech: "SMT", mem_channels: 8,  amx: true,  avx: true,  avx2: true,  avx512: true,  process: "Intel 7", year: 2023 }),
    // First E-core-only Xeon generation (Xeon 6 6700E, June 2024): Crestmont
    // E-cores run one thread per core (no Hyper-Threading) and stop at
    // AVX2/AVX-VNNI, so AVX-512 and AMX stay false. The announced but never
    // launched 288-core Sierra Forest-AP (6900E) is excluded.
    ("Xeon 6 6700E (Sierra Forest-SP)", XeonFacts { arch: "Crestmont (E-core)",  socket: "LGA4710",  generation: "Xeon 6 6700E (Sierra Forest-SP)", cores_min: 64, cores_max: 144, threads_per_core: 1, thread_tech: "none", mem_channels: 8,  amx: false, avx: true,  avx2: true,  avx512: false, process: "Intel 3", year: 2024 }),
    // August 2024 Sapphire Rapids refresh for workstations: Intel shipped
    // no Emerald Rapids workstation line, so this family label records the
    // refresh; W-3500 exposes eight DDR5 channels and W-2500 four.
    ("Xeon W-3500/2500 (Sapphire Rapids W refresh)", XeonFacts { arch: "Sapphire Rapids", socket: "LGA4677", generation: "Xeon W-3500/2500 (Sapphire Rapids W refresh)", cores_min: 8, cores_max: 60, threads_per_core: 2, thread_tech: "SMT", mem_channels: 8, amx: true, avx: true, avx2: true, avx512: true, process: "Intel 7", year: 2024 }),
    // LGA7529 Granite Rapids-AP generation: twelve DDR5 channels with
    // MRDIMM support (up to DDR5-8800) on the 6900P SKUs.
    ("Xeon 6 6900P (Granite Rapids-AP)", XeonFacts { arch: "Redwood Cove (P-core)", socket: "LGA7529", generation: "Xeon 6 6900P (Granite Rapids-AP)", cores_min: 72, cores_max: 128, threads_per_core: 2, thread_tech: "SMT", mem_channels: 12, amx: true, avx: true, avx2: true, avx512: true, process: "Intel 3 / Intel 7", year: 2024 }),
    // LGA4710 Granite Rapids-SP generation (February 2025): eight DDR5
    // channels on the 6700P/6500P SKUs, up to 86 cores.
    ("Xeon 6 6700P/6500P (Granite Rapids-SP)", XeonFacts { arch: "Redwood Cove (P-core)", socket: "LGA4710", generation: "Xeon 6 6700P/6500P (Granite Rapids-SP)", cores_min: 8, cores_max: 86, threads_per_core: 2, thread_tech: "SMT", mem_channels: 8, amx: true, avx: true, avx2: true, avx512: true, process: "Intel 3 / Intel 7", year: 2025 }),
];

/// Per-generation lookup. Unknown names return None; the catalog never
/// guesses.
pub fn facts_for(cpu: &str) -> Option<&'static XeonFacts> {
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
    /// Intel Xeon server and workstation hardware (LGA1366 through LGA7529
    /// platforms).
    IntelXeon,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports a Xeon CPU is present and provides an OS thread API.
    pub x86_64_runtime: bool,
    pub avx: bool,
    pub avx2: bool,
    pub avx512: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntelXeonDeclaration {
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
pub struct IntelXeonIr {
    cpu: String,
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
    isa: Isa,
    facts: &'static XeonFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntelXeonDiagnostic {
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

impl fmt::Display for IntelXeonDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadCpu => "CPU is not in the Intel Xeon catalog",
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
            Self::UnsupportedTarget => "Intel Xeon compute supports only Xeon targets",
            Self::UnsupportedAccelerator => {
                "generation does not implement the requested ISA extensions"
            }
            Self::MissingX86_64Runtime => "target runtime must report a Xeon CPU with a thread API",
            Self::MissingAvx => "target runtime must report AVX support",
            Self::MissingAvx2 => "target runtime must report AVX2 support",
            Self::MissingAvx512 => "target runtime must report AVX-512 support",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for IntelXeonDiagnostic {}

/// Negotiated worker design and source text. The artifact is a plan and a
/// factory definition; successful construction is not proof that a CPU ran
/// the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntelXeonArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl IntelXeonDeclaration {
    pub fn normalize(self) -> Result<IntelXeonIr, IntelXeonDiagnostic> {
        let facts = facts_for(&self.cpu).ok_or(IntelXeonDiagnostic::BadCpu)?;
        let plan = crate::intel_xeon_runtime::WorkerPlan::new(
            self.worker_threads,
            self.batch,
            self.workspace_bytes,
        )
        .map_err(|error| match error {
            crate::intel_xeon_runtime::WorkerError::ZeroWorkerThreads => {
                IntelXeonDiagnostic::WorkerThreadsZero
            }
            crate::intel_xeon_runtime::WorkerError::WorkerThreadsTooLarge => {
                IntelXeonDiagnostic::WorkerThreadsTooLarge
            }
            crate::intel_xeon_runtime::WorkerError::ZeroBatch => IntelXeonDiagnostic::BatchZero,
            crate::intel_xeon_runtime::WorkerError::BatchTooLarge => {
                IntelXeonDiagnostic::BatchTooLarge
            }
            crate::intel_xeon_runtime::WorkerError::ZeroWorkspaceBytes => {
                IntelXeonDiagnostic::WorkspaceBytesZero
            }
            crate::intel_xeon_runtime::WorkerError::WorkspaceBytesTooLarge => {
                IntelXeonDiagnostic::WorkspaceBytesTooLarge
            }
            crate::intel_xeon_runtime::WorkerError::WorkerOverflow => {
                IntelXeonDiagnostic::WorkerOverflow
            }
        })?;
        debug_assert_eq!(plan.worker_threads(), self.worker_threads);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.workspace_bytes(), self.workspace_bytes);
        if u64::from(self.worker_threads)
            > u64::from(facts.cores_max) * u64::from(facts.threads_per_core)
        {
            return Err(IntelXeonDiagnostic::ThreadsExceedGeneration);
        }
        Ok(IntelXeonIr {
            cpu: self.cpu,
            worker_threads: self.worker_threads,
            batch: self.batch,
            workspace_bytes: self.workspace_bytes,
            isa: self.isa,
            facts,
        })
    }
}

impl IntelXeonIr {
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

    pub fn facts(&self) -> &'static XeonFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=intel_xeon\ncpu={}\ngeneration={}\narch={}\nsocket={}\ncores_min={}\ncores_max={}\nthreads_per_core={}\nthread_tech={}\nmem_channels={}\namx={}\navx={}\navx2={}\navx512={}\nprocess={}\nyear={}\nisa={}\nworker_threads={}\nbatch={}\ntotal_worker_executions={}\nworkspace_bytes={}\nexecution=none_attempted\n",
            self.cpu,
            self.facts.generation,
            self.facts.arch,
            self.facts.socket,
            self.facts.cores_min,
            self.facts.cores_max,
            self.facts.threads_per_core,
            self.facts.thread_tech,
            self.facts.mem_channels,
            self.facts.amx,
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
    ) -> Result<IntelXeonArtifact, IntelXeonDiagnostic> {
        if target != Target::IntelXeon {
            return Err(IntelXeonDiagnostic::UnsupportedTarget);
        }
        if !capabilities.x86_64_runtime {
            return Err(IntelXeonDiagnostic::MissingX86_64Runtime);
        }
        if !capabilities.utf8_strings {
            return Err(IntelXeonDiagnostic::MissingUtf8Strings);
        }
        match self.isa {
            Isa::Sse3 => {}
            Isa::Avx => {
                if !capabilities.avx {
                    return Err(IntelXeonDiagnostic::MissingAvx);
                }
                if !self.facts.avx {
                    return Err(IntelXeonDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx2 => {
                if !capabilities.avx2 {
                    return Err(IntelXeonDiagnostic::MissingAvx2);
                }
                if !self.facts.avx2 {
                    return Err(IntelXeonDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx512 => {
                if !capabilities.avx512 {
                    return Err(IntelXeonDiagnostic::MissingAvx512);
                }
                if !self.facts.avx512 {
                    return Err(IntelXeonDiagnostic::UnsupportedAccelerator);
                }
            }
        }
        Ok(IntelXeonArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; Intel Xeon compute, no worker threads are spawned.\npub fn make_worker_plan() -> Result<crate::intel_xeon_runtime::WorkerPlan, crate::intel_xeon_runtime::WorkerError> {{\n    crate::intel_xeon_runtime::WorkerPlan::new({}u32, {}u32, {}u64)\n}}\n",
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

    fn declaration(cpu: &str, worker_threads: u32, batch: u32) -> IntelXeonDeclaration {
        IntelXeonDeclaration {
            cpu: cpu.to_string(),
            worker_threads,
            batch,
            workspace_bytes: 4096,
            isa: Isa::Avx512,
        }
    }

    fn first_avx512_cpu() -> &'static str {
        // Tests lower AVX-512 plans, so start from the first catalog
        // generation with documented AVX-512 support (Skylake-SP, 2017).
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
            assert!(matches!(facts.year, 2008..=2030), "{name} launch year");
            assert!(
                previous_year <= facts.year,
                "catalog must stay chronological at {name}"
            );
            previous_year = facts.year;
            assert!(
                matches!(
                    facts.socket,
                    "LGA1366"
                        | "LGA1567"
                        | "LGA2011"
                        | "LGA2011-3"
                        | "LGA2066"
                        | "LGA3647"
                        | "LGA4189"
                        | "LGA4677"
                        | "LGA4710"
                        | "LGA7529"
                ),
                "socket/channel mismatch on {name}"
            );
            match facts.socket {
                "LGA1366" => assert_eq!(facts.mem_channels, 3, "{name} channels"),
                "LGA1567" | "LGA2011" | "LGA2011-3" | "LGA2066" => {
                    assert_eq!(facts.mem_channels, 4, "{name} channels")
                }
                "LGA3647" => assert_eq!(facts.mem_channels, 6, "{name} channels"),
                "LGA4189" => {
                    assert!(matches!(facts.mem_channels, 6 | 8), "{name} channels")
                }
                "LGA4677" => {
                    assert!(matches!(facts.mem_channels, 4 | 8), "{name} channels")
                }
                "LGA4710" => assert_eq!(facts.mem_channels, 8, "{name} channels"),
                "LGA7529" => assert_eq!(facts.mem_channels, 12, "{name} channels"),
                other => panic!("unexpected socket {other} on {name}"),
            }
            assert!(facts.cores_min > 0 && facts.cores_min <= facts.cores_max);
            assert!(
                matches!(facts.threads_per_core, 1 | 2),
                "{name} threads per core"
            );
            match facts.threads_per_core {
                2 => assert_eq!(facts.thread_tech, "SMT", "{name} thread tech"),
                1 => assert_eq!(facts.thread_tech, "none", "{name} thread tech"),
                other => panic!("unexpected threads_per_core {other} on {name}"),
            }
            assert!(
                u64::from(facts.cores_max) * u64::from(facts.threads_per_core)
                    <= u64::from(MAX_WORKER_THREADS),
                "{name} thread count must fit the supported bound"
            );
            if facts.avx512 {
                assert!(
                    facts.avx2 && facts.avx,
                    "AVX-512 implies AVX2/AVX on {name}"
                );
            }
            if facts.avx2 {
                assert!(facts.avx, "AVX2 implies AVX on {name}");
            }
            if facts.amx {
                assert!(
                    facts.avx512 && facts.year >= 2023,
                    "AMX is documented only on Sapphire Rapids and later on {name}"
                );
            }
            if !facts.avx512 {
                assert!(!facts.amx, "AMX without AVX-512 on {name}");
            }
            assert!(!facts.arch.is_empty());
            assert!(!facts.process.is_empty());
            assert!(!facts.generation.is_empty());
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let cpu = "Xeon Scalable 4th gen (Sapphire Rapids)";
        let ir = declaration(cpu, 120, 4).normalize().unwrap();
        assert_eq!(ir.cpu(), cpu);
        assert_eq!(ir.worker_threads(), 120);
        assert_eq!(ir.batch(), 4);
        assert_eq!(ir.total_worker_executions(), 480);
        assert_eq!(ir.isa(), Isa::Avx512);
        assert_eq!(ir.facts().socket, "LGA4677");
        assert!(ir.facts().amx);
        let first = ir.lower_checked(Target::IntelXeon, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::IntelXeon, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=intel_xeon\n"));
        assert!(first.canonical_ir.contains("worker_threads=120\n"));
        assert!(first.canonical_ir.contains("total_worker_executions=480\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("WorkerPlan::new(120u32, 4u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_cpus_and_bad_bounds() {
        assert_eq!(
            declaration("Xeon W-9999 (Unobtainium)", 128, 8).normalize(),
            Err(IntelXeonDiagnostic::BadCpu)
        );
        let cpu = first_avx512_cpu();
        assert_eq!(
            declaration(cpu, 0, 8).normalize(),
            Err(IntelXeonDiagnostic::WorkerThreadsZero)
        );
        assert_eq!(
            declaration(cpu, MAX_WORKER_THREADS + 1, 8).normalize(),
            Err(IntelXeonDiagnostic::WorkerThreadsTooLarge)
        );
        assert_eq!(
            declaration(cpu, 128, 0).normalize(),
            Err(IntelXeonDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(cpu, 128, MAX_BATCH + 1).normalize(),
            Err(IntelXeonDiagnostic::BatchTooLarge)
        );
        let mut zero_workspace = declaration(cpu, 128, 8);
        zero_workspace.workspace_bytes = 0;
        assert_eq!(
            zero_workspace.normalize(),
            Err(IntelXeonDiagnostic::WorkspaceBytesZero)
        );
        let mut huge_workspace = declaration(cpu, 128, 8);
        huge_workspace.workspace_bytes = MAX_WORKSPACE_BYTES + 1;
        assert_eq!(
            huge_workspace.normalize(),
            Err(IntelXeonDiagnostic::WorkspaceBytesTooLarge)
        );
        // Nehalem-EP ships at most 4 cores / 8 threads: 9 requested threads
        // pass the plan bound but exceed the generation.
        assert_eq!(
            declaration("Xeon 5500 (Nehalem-EP)", 9, 8).normalize(),
            Err(IntelXeonDiagnostic::ThreadsExceedGeneration)
        );
        assert!(declaration("Xeon 5500 (Nehalem-EP)", 8, 8)
            .normalize()
            .is_ok());
        // Sierra Forest runs one thread per core across its 144 cores.
        assert_eq!(
            declaration("Xeon 6 6700E (Sierra Forest-SP)", 145, 8).normalize(),
            Err(IntelXeonDiagnostic::ThreadsExceedGeneration)
        );
        assert!(
            declaration("Xeon 6 6700E (Sierra Forest-SP)", 144, MAX_BATCH)
                .normalize()
                .is_ok()
        );
        // The Granite Rapids-AP top SKU bound (128 cores times 2 threads)
        // is representable at the full plan and batch bounds.
        assert!(
            declaration("Xeon 6 6900P (Granite Rapids-AP)", 256, MAX_BATCH)
                .normalize()
                .is_ok()
        );
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let cpu = first_avx512_cpu();
        let ir = declaration(cpu, 128, 8).normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(IntelXeonDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    x86_64_runtime: false,
                    ..CAPABILITIES
                },
                IntelXeonDiagnostic::MissingX86_64Runtime,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                IntelXeonDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::IntelXeon, missing), Err(expected));
        }
        for (isa, missing, expected) in [
            (
                Isa::Avx,
                RuntimeCapabilities {
                    avx: false,
                    ..CAPABILITIES
                },
                IntelXeonDiagnostic::MissingAvx,
            ),
            (
                Isa::Avx2,
                RuntimeCapabilities {
                    avx2: false,
                    ..CAPABILITIES
                },
                IntelXeonDiagnostic::MissingAvx2,
            ),
            (
                Isa::Avx512,
                RuntimeCapabilities {
                    avx512: false,
                    ..CAPABILITIES
                },
                IntelXeonDiagnostic::MissingAvx512,
            ),
        ] {
            let mut isa_declaration = declaration(cpu, 128, 8);
            isa_declaration.isa = isa;
            let isa_ir = isa_declaration.normalize().unwrap();
            assert_eq!(
                isa_ir.lower_checked(Target::IntelXeon, missing),
                Err(expected)
            );
        }
        // Requesting AVX or AVX-512 on pre-Sandy-Bridge silicon fails
        // closed on the accelerator, not the host.
        let mut nehalem_avx = declaration("Xeon 5500 (Nehalem-EP)", 4, 8);
        nehalem_avx.isa = Isa::Avx;
        let nehalem_avx = nehalem_avx.normalize().unwrap();
        assert_eq!(
            nehalem_avx.lower_checked(Target::IntelXeon, CAPABILITIES),
            Err(IntelXeonDiagnostic::UnsupportedAccelerator)
        );
        let nehalem_ir = declaration("Xeon 5500 (Nehalem-EP)", 4, 8)
            .normalize()
            .unwrap();
        assert_eq!(
            nehalem_ir.lower_checked(Target::IntelXeon, CAPABILITIES),
            Err(IntelXeonDiagnostic::UnsupportedAccelerator)
        );
    }

    #[test]
    fn every_generation_lowers_sse3_and_avx512_fails_closed_pre_sandy_bridge() {
        let mut lowered_sse3 = 0;
        let mut lowered_avx512 = 0;
        let mut failed_avx512 = 0;
        for (name, facts) in GENERATIONS {
            let mut sse3_declaration = declaration(name, 4, 2);
            sse3_declaration.isa = Isa::Sse3;
            sse3_declaration
                .normalize()
                .unwrap()
                .lower_checked(Target::IntelXeon, CAPABILITIES)
                .unwrap();
            lowered_sse3 += 1;
            let ir = declaration(name, 4, 2).normalize().unwrap();
            if !facts.avx512 {
                assert_eq!(
                    ir.lower_checked(Target::IntelXeon, CAPABILITIES),
                    Err(IntelXeonDiagnostic::UnsupportedAccelerator)
                );
                failed_avx512 += 1;
                continue;
            }
            ir.lower_checked(Target::IntelXeon, CAPABILITIES).unwrap();
            lowered_avx512 += 1;
        }
        assert!(
            lowered_sse3 > 0,
            "catalog must lower at least one SSE3 generation"
        );
        assert!(
            lowered_avx512 > 0,
            "catalog must lower at least one AVX-512 generation"
        );
        // Nehalem through Broadwell-EP and the E-core Sierra Forest lines
        // never satisfy an AVX-512 plan.
        assert!(
            failed_avx512 >= 7,
            "pre-AVX-512 generations must fail closed: {failed_avx512}"
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_intel_xeon_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let cpu = "Xeon Scalable 4th gen (Sapphire Rapids)";
        let artifact = declaration(cpu, 120, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::IntelXeon, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/intel_xeon_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod intel_xeon_runtime;\n{}\nfn main() {{\n    let plan = make_worker_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.worker_threads(), 120);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_worker_executions(), 360);\n    assert_eq!(plan.workspace_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-intel-xeon-factory-{}-{nonce}",
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
