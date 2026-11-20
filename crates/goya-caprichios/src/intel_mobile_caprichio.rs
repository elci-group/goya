//! A bounded Intel mobile CPU-compute capability.
//!
//! The negotiated artifact is a checked worker plan and a factory binding
//! to this crate's plan runtime. It does not spawn threads, execute SIMD,
//! drive the integrated GPU, or prove that a CPU executed work. Hosts on
//! Intel mobile CPUs report their real facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.cpu_compute.intel_mobile.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORKER_THREADS: u32 = crate::intel_mobile_runtime::MAX_WORKER_THREADS;
pub const MAX_BATCH: u32 = crate::intel_mobile_runtime::MAX_BATCH;
pub const MAX_WORKSPACE_BYTES: u64 = crate::intel_mobile_runtime::MAX_WORKSPACE_BYTES;

/// Published CPU facts for one Intel mobile generation (family level).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MobileCpuFacts {
    /// Intel microarchitecture label (for example `Sunny Cove`), or the
    /// `/`-joined P/E-core labels on hybrid families.
    pub arch: &'static str,
    /// Package/socket label Intel publishes; `/`-joined where a family
    /// shipped on several packages.
    pub package: &'static str,
    /// Integrated Intel graphics architecture or product label; `none`
    /// where the family shipped no on-package graphics, and chipset-based
    /// GMA labeled as the platform published it (Penryn).
    pub igpu_arch: &'static str,
    /// Floor of the published iGPU execution-unit range; `None` where
    /// Intel never published a family-level figure (never guessed).
    pub eu_min: Option<u32>,
    /// Upper half of the published iGPU execution-unit range, or `None`.
    pub eu_max: Option<u32>,
    /// Vendor-published lowest physical core count in the mobile lineup.
    pub cores_min: u32,
    /// Vendor-published highest physical core count in the mobile lineup
    /// (hybrid families count P, E, and LP-E cores as Intel does).
    pub cores_max: u32,
    /// OS-visible threads per core on the generation's SMT-capable cores
    /// (`2` where Hyper-Threading ships, `1` where it does not; hybrid
    /// E/LP-E cores are single-threaded).
    pub threads_per_core: u32,
    /// Vendor-documented maximum OS-visible hardware threads for the
    /// family (Intel publishes Total Threads per SKU); equals cores_max
    /// × threads_per_core on homogeneous generations, and hybrid P+E
    /// generations sit below that product.
    pub threads_max: u32,
    /// Threading technology: `none` or `SMT`.
    pub thread_tech: &'static str,
    /// Documented AVX ISA support.
    pub avx: bool,
    /// Documented AVX2 ISA support.
    pub avx2: bool,
    /// Documented AVX-512 ISA support as shipped in production silicon.
    /// Only Ice Lake and Tiger Lake mobile shipped AVX-512 enabled; the
    /// hybrid generations (Alder/Raptor/Meteor/Lunar/Arrow Lake) disable
    /// or fuse it off in production.
    pub avx512: bool,
    /// Vendor-published process node string (Intel nodes, or the TSMC
    /// compute-tile node for disaggregated families).
    pub process: &'static str,
    /// First mobile launch year of the family.
    pub year: u16,
}

/// Catalog order is chronological (family launch year); each entry is a
/// family-level mobile generation, not a SKU, and the `year` field records
/// the family's first mobile launch year. Facts come from Intel ARK
/// specification pages and the corroborating public record; figures Intel
/// never published at family level are `None`, never guessed. Desktop
/// families and the Xeon server/workstation lines are excluded: they have
/// their own caprichios. Wildcat Lake and Nova Lake are excluded: Intel
/// has published no shipping-family facts for them as of today, and the
/// catalog never guesses. Alder Lake-N (single-cluster E-core parts on
/// BGA945) is likewise outside this hybrid-family contract's scope.
/// Launch windows and AVX-512 production status conflicts are recorded in
/// the row comments themselves.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, MobileCpuFacts)] = &[
    // Mainstream dual-core lineup; graphics were chipset-based (GM45
    // GMA X4500), so there is no on-package GPU and no family-level EU
    // figure. The quad-core Core 2 Extreme QX9300 sits outside this row.
    ("Penryn (mobile)",       MobileCpuFacts { arch: "Penryn",                package: "Socket P/PGA478",              igpu_arch: "GMA X4500",                 eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 2,  threads_per_core: 1, thread_tech: "none", avx: false, avx2: false, avx512: false, process: "45nm",                     year: 2008 }),
    // First mobile quad-core Core i7 (Extreme) family; discrete-graphics
    // platform with no on-package GPU.
    ("Clarksfield",           MobileCpuFacts { arch: "Nehalem",               package: "Socket G1 (rPGA988A)",         igpu_arch: "none",                      eu_min: None,     eu_max: None,     cores_min: 4,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: false, avx2: false, avx512: false, process: "45nm",                     year: 2009 }),
    // First mobile family with an on-package graphics die.
    ("Arrandale",             MobileCpuFacts { arch: "Westmere",              package: "Socket G1 (rPGA988A)/BGA1288", igpu_arch: "HD Graphics",               eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 2,  threads_per_core: 2, thread_tech: "SMT",  avx: false, avx2: false, avx512: false, process: "32nm/45nm",                year: 2010 }),
    ("Sandy Bridge (mobile)", MobileCpuFacts { arch: "Sandy Bridge",          package: "Socket G2 (rPGA988B)/BGA1023", igpu_arch: "HD Graphics 3000",          eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: false, avx512: false, process: "32nm",                     year: 2011 }),
    ("Ivy Bridge (mobile)",   MobileCpuFacts { arch: "Ivy Bridge",            package: "Socket G2 (rPGA988B)/BGA1023", igpu_arch: "HD Graphics 4000",          eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: false, avx512: false, process: "22nm",                     year: 2012 }),
    // First mobile AVX2 family; Iris Pro 5200 (GT3e with eDRAM) optional.
    ("Haswell (mobile)",      MobileCpuFacts { arch: "Haswell",               package: "Socket G3 (rPGA947)/BGA1364",  igpu_arch: "HD Graphics/Iris Pro 5200", eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "22nm",                     year: 2013 }),
    // Core M shipped late 2014; the U-series (Iris 6100) January 2015.
    ("Broadwell-U",           MobileCpuFacts { arch: "Broadwell",             package: "BGA1168",                      igpu_arch: "HD Graphics/Iris 6100",     eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 2,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm",                     year: 2014 }),
    ("Skylake (mobile)",      MobileCpuFacts { arch: "Skylake",               package: "BGA1440/BGA1356",              igpu_arch: "HD Graphics 520/530",       eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm",                     year: 2015 }),
    // U/Y-series shipped August 2016; the H-series January 2017. Skylake-
    // derived core on the 14nm+ refinement.
    ("Kaby Lake (mobile)",    MobileCpuFacts { arch: "Skylake",               package: "BGA1440/BGA1356",              igpu_arch: "HD Graphics 620/630",       eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm+",                    year: 2016 }),
    ("Whiskey Lake",          MobileCpuFacts { arch: "Skylake",               package: "BGA1528",                      igpu_arch: "UHD Graphics 620",          eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm+",                    year: 2018 }),
    // The 8-core ceiling comes from the 2019 9th-gen H refresh
    // (Core i9-9980HK) on the same package and family core.
    ("Coffee Lake-H",         MobileCpuFacts { arch: "Skylake",               package: "BGA1440",                      igpu_arch: "UHD Graphics 630",          eu_min: None,     eu_max: None,     cores_min: 4,  cores_max: 8,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm++",                   year: 2018 }),
    // First 10nm client family and the first mobile family to ship
    // AVX-512 enabled. Intel published the Gen11 EU range: 32 (G1)
    // through 64 (G7).
    ("Ice Lake",              MobileCpuFacts { arch: "Sunny Cove",            package: "BGA1526",                      igpu_arch: "Iris Plus G4/G7",           eu_min: Some(32), eu_max: Some(64), cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: true,  process: "10nm",                     year: 2019 }),
    // U-series shipped August 2019; the 8-core H-series April 2020.
    // Skylake-derived core on the 14nm++ refinement.
    ("Comet Lake (mobile)",   MobileCpuFacts { arch: "Skylake",               package: "BGA1440/BGA1528",              igpu_arch: "UHD Graphics",              eu_min: None,     eu_max: None,     cores_min: 2,  cores_max: 8,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "14nm++",                   year: 2019 }),
    // AVX-512 shipped enabled (Willow Cove). Intel published the Xe-LP
    // EU range: 80 (G4) through 96 (G7).
    ("Tiger Lake",            MobileCpuFacts { arch: "Willow Cove",           package: "BGA1782",                      igpu_arch: "Iris Xe",                   eu_min: Some(80), eu_max: Some(96), cores_min: 2,  cores_max: 4,  threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: true,  process: "10nm SuperFin",            year: 2020 }),
    // Hybrid P/E parts. AVX-512 was fused off in production silicon
    // (early-2022 and later); only early-engineering parts exposed it.
    ("Alder Lake (mobile)",   MobileCpuFacts { arch: "Golden Cove/Gracemont", package: "BGA1744",                      igpu_arch: "Iris Xe/UHD Graphics",      eu_min: None,     eu_max: None,     cores_min: 6,  cores_max: 16, threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "Intel 7",                  year: 2022 }),
    // Up to 24C HX (8P+16E, 32 OS threads); AVX-512 stays disabled.
    ("Raptor Lake (mobile)",  MobileCpuFacts { arch: "Raptor Cove/Gracemont", package: "BGA1744",                      igpu_arch: "Iris Xe/UHD Graphics",      eu_min: None,     eu_max: None,     cores_min: 6,  cores_max: 24, threads_per_core: 2, thread_tech: "SMT",  avx: true,  avx2: true,  avx512: false, process: "Intel 7",                  year: 2023 }),
    // Core Ultra 100 hybrid with LP-E pairs on the SoC die. Launch SKUs
    // published 4-8 Arc Xe-LPG cores (64-128 EU at 16 EU per core);
    // AVX-512 (Redwood Cove-capable) is disabled in production.
    ("Meteor Lake (Core Ultra 100)", MobileCpuFacts { arch: "Redwood Cove/Crestmont", package: "FCBGA2049",         igpu_arch: "Arc (Xe LPG)",              eu_min: Some(64), eu_max: Some(128), cores_min: 12, cores_max: 16, threads_per_core: 2, thread_tech: "SMT", avx: true,  avx2: true,  avx512: false, process: "Intel 4",                  year: 2023 }),
    // 4P + 4 LP-E: Core Ultra (Series 2) ships without Hyper-Threading
    // (cores equal threads), and Lion Cove's AVX-512 is disabled in this
    // generation. Arc 130V/140V are 7/8 Xe2 cores (56/64 EU).
    ("Lunar Lake (Core Ultra 200V)", MobileCpuFacts { arch: "Lion Cove/Skymont", package: "FCBGA2833",                 igpu_arch: "Arc 130V/140V (Xe2)",       eu_min: Some(56), eu_max: Some(64), cores_min: 8,  cores_max: 8,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: true,  avx512: false, process: "TSMC N4B (compute tile)",  year: 2024 }),
    // H (FCBGA2049, Arc 140T with 8 Xe cores) and HX (FCBGA2114, 4-Xe-
    // core Arc) ship without Hyper-Threading (24C HX is 24 threads);
    // AVX-512 stays disabled.
    ("Arrow Lake-H/HX (Core Ultra 200H/HX)", MobileCpuFacts { arch: "Lion Cove/Skymont", package: "FCBGA2049/FCBGA2114", igpu_arch: "Arc (Xe LPG)",         eu_min: Some(64), eu_max: Some(128), cores_min: 14, cores_max: 24, threads_per_core: 1, thread_tech: "none", avx: true,  avx2: true,  avx512: false, process: "TSMC N3B (compute tile)",  year: 2025 }),
];

/// Per-generation lookup. Unknown generations return None; the catalog never
/// guesses.
pub fn facts_for(cpu: &str) -> Option<&'static MobileCpuFacts> {
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
    /// Intel mobile CPU hardware (post-2008 mobile Core families).
    IntelMobile,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports an Intel mobile CPU is present and provides an OS
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
pub struct IntelMobileDeclaration {
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
pub struct IntelMobileIr {
    cpu: String,
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
    isa: Isa,
    facts: &'static MobileCpuFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntelMobileDiagnostic {
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

impl fmt::Display for IntelMobileDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadCpu => "generation is not in the Intel mobile CPU catalog",
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
                "Intel mobile CPU compute supports only Intel mobile targets"
            }
            Self::UnsupportedAccelerator => {
                "generation does not document the required ISA extension"
            }
            Self::MissingX86_64Runtime => {
                "target runtime must report an Intel mobile CPU with an OS thread API"
            }
            Self::MissingAvx => "target runtime must report AVX support",
            Self::MissingAvx2 => "target runtime must report AVX2 support",
            Self::MissingAvx512 => "target runtime must report AVX-512 support",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for IntelMobileDiagnostic {}

/// Negotiated worker design and source text. The artifact is a plan and a
/// factory definition; successful construction is not proof that a CPU ran
/// the workers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntelMobileArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl IntelMobileDeclaration {
    pub fn normalize(self) -> Result<IntelMobileIr, IntelMobileDiagnostic> {
        let facts = facts_for(&self.cpu).ok_or(IntelMobileDiagnostic::BadCpu)?;
        let plan = crate::intel_mobile_runtime::WorkerPlan::new(
            self.worker_threads,
            self.batch,
            self.workspace_bytes,
        )
        .map_err(|error| match error {
            crate::intel_mobile_runtime::WorkerError::ZeroWorkerThreads => {
                IntelMobileDiagnostic::WorkerThreadsZero
            }
            crate::intel_mobile_runtime::WorkerError::WorkerThreadsTooLarge => {
                IntelMobileDiagnostic::WorkerThreadsTooLarge
            }
            crate::intel_mobile_runtime::WorkerError::ZeroBatch => IntelMobileDiagnostic::BatchZero,
            crate::intel_mobile_runtime::WorkerError::BatchTooLarge => {
                IntelMobileDiagnostic::BatchTooLarge
            }
            crate::intel_mobile_runtime::WorkerError::ZeroWorkspaceBytes => {
                IntelMobileDiagnostic::WorkspaceBytesZero
            }
            crate::intel_mobile_runtime::WorkerError::WorkspaceBytesTooLarge => {
                IntelMobileDiagnostic::WorkspaceBytesTooLarge
            }
            crate::intel_mobile_runtime::WorkerError::WorkerOverflow => {
                IntelMobileDiagnostic::WorkerOverflow
            }
        })?;
        // Planned worker threads cannot exceed the generation's documented
        // hardware thread count.
        if u64::from(self.worker_threads)
            > u64::from(facts.cores_max) * u64::from(facts.threads_per_core)
        {
            return Err(IntelMobileDiagnostic::ThreadsExceedGeneration);
        }
        debug_assert_eq!(plan.worker_threads(), self.worker_threads);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.workspace_bytes(), self.workspace_bytes);
        Ok(IntelMobileIr {
            cpu: self.cpu,
            worker_threads: self.worker_threads,
            batch: self.batch,
            workspace_bytes: self.workspace_bytes,
            isa: self.isa,
            facts,
        })
    }
}

impl IntelMobileIr {
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

    pub fn facts(&self) -> &'static MobileCpuFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        let eu_min = self
            .facts
            .eu_min
            .map(|units| units.to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        let eu_max = self
            .facts
            .eu_max
            .map(|units| units.to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=intel_mobile\ncpu={}\narch={}\npackage={}\nigpu_arch={}\nigpu_eu_min={eu_min}\nigpu_eu_max={eu_max}\ncores_min={}\ncores_max={}\nthreads_per_core={}\nthread_tech={}\navx={}\navx2={}\navx512={}\nprocess={}\nyear={}\nisa={}\nworker_threads={}\nbatch={}\ntotal_worker_executions={}\nworkspace_bytes={}\nexecution=none_attempted\n",
            self.cpu,
            self.facts.arch,
            self.facts.package,
            self.facts.igpu_arch,
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
    ) -> Result<IntelMobileArtifact, IntelMobileDiagnostic> {
        if target != Target::IntelMobile {
            return Err(IntelMobileDiagnostic::UnsupportedTarget);
        }
        if !capabilities.x86_64_runtime {
            return Err(IntelMobileDiagnostic::MissingX86_64Runtime);
        }
        if !capabilities.utf8_strings {
            return Err(IntelMobileDiagnostic::MissingUtf8Strings);
        }
        match self.isa {
            Isa::Sse3 => {}
            Isa::Avx => {
                if !capabilities.avx {
                    return Err(IntelMobileDiagnostic::MissingAvx);
                }
                if !self.facts.avx {
                    return Err(IntelMobileDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx2 => {
                if !capabilities.avx2 {
                    return Err(IntelMobileDiagnostic::MissingAvx2);
                }
                if !self.facts.avx2 {
                    return Err(IntelMobileDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx512 => {
                if !capabilities.avx512 {
                    return Err(IntelMobileDiagnostic::MissingAvx512);
                }
                if !self.facts.avx512 {
                    return Err(IntelMobileDiagnostic::UnsupportedAccelerator);
                }
            }
        }
        Ok(IntelMobileArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; Intel mobile CPU worker plan, no worker execution is attempted.\npub fn make_worker_plan() -> Result<crate::intel_mobile_runtime::WorkerPlan, crate::intel_mobile_runtime::WorkerError> {{\n    crate::intel_mobile_runtime::WorkerPlan::new({}u32, {}u32, {}u64)\n}}\n",
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

    fn declaration(cpu: &str, worker_threads: u32, batch: u32) -> IntelMobileDeclaration {
        IntelMobileDeclaration {
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
                "Penryn"
                    | "Nehalem"
                    | "Westmere"
                    | "Sandy Bridge"
                    | "Ivy Bridge"
                    | "Haswell"
                    | "Broadwell"
                    | "Skylake"
                    | "Sunny Cove"
                    | "Willow Cove"
                    | "Golden Cove/Gracemont"
                    | "Raptor Cove/Gracemont"
                    | "Redwood Cove/Crestmont"
                    | "Lion Cove/Skymont"
            ));
            assert!(matches!(
                facts.package,
                "Socket P/PGA478"
                    | "Socket G1 (rPGA988A)"
                    | "Socket G1 (rPGA988A)/BGA1288"
                    | "Socket G2 (rPGA988B)/BGA1023"
                    | "Socket G3 (rPGA947)/BGA1364"
                    | "BGA1168"
                    | "BGA1440/BGA1356"
                    | "BGA1528"
                    | "BGA1440"
                    | "BGA1526"
                    | "BGA1440/BGA1528"
                    | "BGA1782"
                    | "BGA1744"
                    | "FCBGA2049"
                    | "FCBGA2833"
                    | "FCBGA2049/FCBGA2114"
            ));
            assert!((2008..=2030).contains(&facts.year));
            assert!(facts.cores_min >= 1 && facts.cores_min <= facts.cores_max);
            assert!(facts.threads_per_core >= 1);
            assert!(!facts.igpu_arch.is_empty());
            assert!(!facts.process.is_empty());
            assert_eq!(
                facts.thread_tech,
                if facts.threads_per_core == 1 {
                    "none"
                } else {
                    "SMT"
                },
                "thread tech must be none exactly when there is one thread per core"
            );
            if !facts.avx {
                assert!(!facts.avx2 && !facts.avx512, "{name} has no AVX");
            }
            if facts.avx2 {
                assert!(facts.avx);
            } else {
                assert!(!facts.avx512, "{name} has no AVX2");
            }
            if facts.avx512 {
                assert!(matches!(facts.arch, "Sunny Cove" | "Willow Cove"));
            }
            match (facts.eu_min, facts.eu_max) {
                (None, None) => {}
                (Some(min), Some(max)) => {
                    assert!(min >= 1, "{name} EU counts must be positive");
                    assert!(min <= max, "{name} EU range must be ordered");
                }
                _ => panic!("EU range must be fully published or fully unpublished"),
            }
            // The supported worker bound must cover the generation's
            // documented OS-visible hardware thread count, widened to u64
            // before multiplying. SMT hybrid families mix two-thread P-cores
            // with single-threaded E/LP-E cores, so cores_max *
            // threads_per_core is only a conservative ceiling; the largest
            // published SKU totals are asserted directly for those rows.
            let ceiling = u64::from(facts.cores_max) * u64::from(facts.threads_per_core);
            let os_threads = match *name {
                "Alder Lake (mobile)" => 24,          // Core i9-12900HX: 8P+8E
                "Raptor Lake (mobile)" => 32,         // Core i9-14900HX: 8P+16E
                "Meteor Lake (Core Ultra 100)" => 22, // Core Ultra 9 185H: 6P+8E+2LP-E
                _ => ceiling,
            };
            assert!(
                os_threads <= u64::from(MAX_WORKER_THREADS),
                "{name} thread count must fit the supported bound"
            );
            assert!(
                ceiling >= os_threads,
                "{name} ceiling must cover its published thread count"
            );
        }
        let years: Vec<u16> = GENERATIONS.iter().map(|(_, facts)| facts.year).collect();
        assert!(years.windows(2).all(|pair| pair[0] <= pair[1]));
        // Only the verified 10nm Core families shipped AVX-512 enabled;
        // every hybrid generation must fail closed.
        let avx512_names: Vec<&str> = GENERATIONS
            .iter()
            .filter(|(_, facts)| facts.avx512)
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            avx512_names,
            ["Ice Lake", "Tiger Lake"],
            "AVX-512 rows must match the verified shipping families"
        );
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let mut decl = declaration("Ice Lake", 8, 4);
        decl.isa = Isa::Avx512;
        let ir = decl.normalize().unwrap();
        assert_eq!(ir.cpu(), "Ice Lake");
        assert_eq!(ir.worker_threads(), 8);
        assert_eq!(ir.batch(), 4);
        assert_eq!(ir.isa(), Isa::Avx512);
        assert_eq!(ir.total_worker_executions(), 32);
        assert_eq!(ir.facts().arch, "Sunny Cove");
        assert_eq!(ir.facts().igpu_arch, "Iris Plus G4/G7");
        let first = ir.lower_checked(Target::IntelMobile, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::IntelMobile, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=intel_mobile\n"));
        assert!(first.canonical_ir.contains("arch=Sunny Cove\n"));
        assert!(first.canonical_ir.contains("package=BGA1526\n"));
        assert!(first.canonical_ir.contains("igpu_arch=Iris Plus G4/G7\n"));
        assert!(first.canonical_ir.contains("igpu_eu_min=32\n"));
        assert!(first.canonical_ir.contains("igpu_eu_max=64\n"));
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
            declaration("Pentium M", 8, 4).normalize(),
            Err(IntelMobileDiagnostic::BadCpu)
        );
        let cpu = first_avx512_generation();
        assert_eq!(
            declaration(cpu, 0, 4).normalize(),
            Err(IntelMobileDiagnostic::WorkerThreadsZero)
        );
        assert_eq!(
            declaration(cpu, MAX_WORKER_THREADS + 1, 4).normalize(),
            Err(IntelMobileDiagnostic::WorkerThreadsTooLarge)
        );
        assert_eq!(
            declaration(cpu, 8, 0).normalize(),
            Err(IntelMobileDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(cpu, 8, MAX_BATCH + 1).normalize(),
            Err(IntelMobileDiagnostic::BatchTooLarge)
        );
        let mut zero_workspace = declaration(cpu, 8, 4);
        zero_workspace.workspace_bytes = 0;
        assert_eq!(
            zero_workspace.normalize(),
            Err(IntelMobileDiagnostic::WorkspaceBytesZero)
        );
        let mut huge_workspace = declaration(cpu, 8, 4);
        huge_workspace.workspace_bytes = MAX_WORKSPACE_BYTES + 1;
        assert_eq!(
            huge_workspace.normalize(),
            Err(IntelMobileDiagnostic::WorkspaceBytesTooLarge)
        );
        // Broadwell-U documents 2 cores with Hyper-Threading, so eight
        // planned worker threads exceed the generation's four hardware
        // threads.
        assert_eq!(
            declaration("Broadwell-U", 8, 4).normalize(),
            Err(IntelMobileDiagnostic::ThreadsExceedGeneration)
        );
        assert!(declaration("Broadwell-U", 4, 4).normalize().is_ok());
        // The 32-thread bound exists for the Core i9-14900HX class (48
        // documented ceiling threads), so the full bound must normalize
        // there even though most mobile generations top out far lower.
        assert!(
            declaration("Raptor Lake (mobile)", MAX_WORKER_THREADS, MAX_BATCH)
                .normalize()
                .is_ok()
        );
        // WorkerOverflow stays unreachable within the catalog bounds: even
        // MAX_WORKER_THREADS * MAX_BATCH fits u32::MAX.
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let cpu = first_avx512_generation();
        let ir = declaration(cpu, 8, 4).normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(IntelMobileDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    x86_64_runtime: false,
                    ..CAPABILITIES
                },
                IntelMobileDiagnostic::MissingX86_64Runtime,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                IntelMobileDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::IntelMobile, missing),
                Err(expected)
            );
        }
        // Each ISA diagnostic is exercised on its own negotiated IR.
        let mut avx_decl = declaration(cpu, 8, 4);
        avx_decl.isa = Isa::Avx;
        let avx_ir = avx_decl.normalize().unwrap();
        assert_eq!(
            avx_ir.lower_checked(
                Target::IntelMobile,
                RuntimeCapabilities {
                    avx: false,
                    ..CAPABILITIES
                }
            ),
            Err(IntelMobileDiagnostic::MissingAvx)
        );
        let mut avx2_decl = declaration(cpu, 8, 4);
        avx2_decl.isa = Isa::Avx2;
        let avx2_ir = avx2_decl.normalize().unwrap();
        assert_eq!(
            avx2_ir.lower_checked(
                Target::IntelMobile,
                RuntimeCapabilities {
                    avx2: false,
                    ..CAPABILITIES
                }
            ),
            Err(IntelMobileDiagnostic::MissingAvx2)
        );
        let mut avx512_decl = declaration(cpu, 8, 4);
        avx512_decl.isa = Isa::Avx512;
        let avx512_ir = avx512_decl.normalize().unwrap();
        assert_eq!(
            avx512_ir.lower_checked(
                Target::IntelMobile,
                RuntimeCapabilities {
                    avx512: false,
                    ..CAPABILITIES
                }
            ),
            Err(IntelMobileDiagnostic::MissingAvx512)
        );
        // Silicon without documented ISA support fails closed even with a
        // fully capable host: Clarksfield has no AVX, Sandy Bridge mobile
        // no AVX2, and Haswell mobile no AVX-512.
        let unsupported = [
            (
                Isa::Avx,
                "Clarksfield",
                IntelMobileDiagnostic::UnsupportedAccelerator,
            ),
            (
                Isa::Avx2,
                "Sandy Bridge (mobile)",
                IntelMobileDiagnostic::UnsupportedAccelerator,
            ),
            (
                Isa::Avx512,
                "Haswell (mobile)",
                IntelMobileDiagnostic::UnsupportedAccelerator,
            ),
        ];
        for (isa, row, expected) in unsupported {
            let mut decl = declaration(row, 2, 2);
            decl.isa = isa;
            let row_ir = decl.normalize().unwrap();
            assert_eq!(
                row_ir.lower_checked(Target::IntelMobile, CAPABILITIES),
                Err(expected)
            );
        }
    }

    #[test]
    fn every_generation_lowers_on_sse3_and_avx512_fails_closed_before_ice_lake() {
        let mut lowered_sse3 = 0;
        let mut lowered_avx512 = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 2, 2).normalize().unwrap();
            ir.lower_checked(Target::IntelMobile, CAPABILITIES).unwrap();
            lowered_sse3 += 1;
            let mut decl = declaration(name, 2, 2);
            decl.isa = Isa::Avx512;
            let ir = decl.normalize().unwrap();
            if !facts.avx512 {
                assert_eq!(
                    ir.lower_checked(Target::IntelMobile, CAPABILITIES),
                    Err(IntelMobileDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::IntelMobile, CAPABILITIES).unwrap();
            lowered_avx512 += 1;
        }
        assert_eq!(lowered_sse3, GENERATIONS.len(), "SSE3 lowers everywhere");
        assert!(
            lowered_avx512 >= 2,
            "Ice Lake and Tiger Lake must lower AVX-512 plans"
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_intel_mobile_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let cpu = first_avx512_generation();
        let mut decl = declaration(cpu, 8, 3);
        decl.isa = Isa::Avx512;
        let artifact = decl
            .normalize()
            .unwrap()
            .lower_checked(Target::IntelMobile, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/intel_mobile_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod intel_mobile_runtime;\n{}\nfn main() {{\n    let plan = make_worker_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.worker_threads(), 8);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_worker_executions(), 24);\n    assert_eq!(plan.workspace_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-intel-mobile-factory-{}-{nonce}",
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
