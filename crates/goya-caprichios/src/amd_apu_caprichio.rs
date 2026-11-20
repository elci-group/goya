//! A bounded AMD APU CPU-compute capability.
//!
//! The negotiated artifact is a checked worker plan and a factory binding
//! to this crate's plan runtime. It does not spawn threads, execute SIMD,
//! drive the integrated GPU, or prove that any silicon executed work.
//! Hosts on AMD APUs report their real facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.cpu_compute.amd_apu.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_WORKER_THREADS: u32 = crate::amd_apu_runtime::MAX_WORKER_THREADS;
pub const MAX_BATCH: u32 = crate::amd_apu_runtime::MAX_BATCH;
pub const MAX_WORKSPACE_BYTES: u64 = crate::amd_apu_runtime::MAX_WORKSPACE_BYTES;

/// Published platform facts for one AMD APU family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApuFacts {
    /// CPU-side microarchitecture label AMD uses (for example `Zen 3+`).
    pub arch: &'static str,
    /// Package/socket label; `/`-joined where a family shipped on several.
    pub socket: &'static str,
    /// Integrated Radeon graphics architecture label.
    pub igpu_arch: &'static str,
    /// Documented iGPU compute-unit range for the family; `None` where AMD
    /// published no family-level figure (never guessed).
    pub cu_min: Option<u32>,
    /// Upper half of the documented iGPU compute-unit range, or `None`.
    pub cu_max: Option<u32>,
    /// Vendor-published CPU core range for the family.
    pub cores_min: u32,
    /// Upper half of the vendor-published CPU core range.
    pub cores_max: u32,
    /// Published hardware threads per CPU core.
    pub threads_per_core: u32,
    /// `none` for single-threaded cores (including in-order Bobcat and
    /// clustered CMT designs), `SMT` for simultaneous multithreading.
    pub thread_tech: &'static str,
    /// Documented AVX support.
    pub avx: bool,
    /// Documented AVX2 support.
    pub avx2: bool,
    /// Documented AVX-512 support (Zen 4 and later).
    pub avx512: bool,
    /// Foundry process node as AMD published it.
    pub process: &'static str,
    /// First release year.
    pub year: u16,
}

/// Catalog scope is post-2008 APU families, one entry per family in
/// chronological launch order. Desktop-only pure-CPU families and the
/// Threadripper and EPYC lines are excluded; they have their own
/// caprichios. Gorgon Point is excluded: AMD has published no
/// shipping-family facts for it as of today, and the catalog never
/// guesses. Facts come from AMD product documentation and corroborating
/// public sources; figures AMD did not publish are `None`, never guessed.
/// Multi-socket families and architecture conflicts are recorded in the
/// label itself.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, ApuFacts)] = &[
    // Terascale-era APUs: K10 cores (FM1) and in-order Bobcat cores (FT1)
    ("Llano",         ApuFacts { arch: "K10",        socket: "FM1",          igpu_arch: "Terascale 2",       cu_min: None,    cu_max: None,    cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: false, avx2: false, avx512: false, process: "32 nm", year: 2011 }),
    ("Brazos",        ApuFacts { arch: "Bobcat",     socket: "FT1 (BGA)",    igpu_arch: "Terascale 2",       cu_min: None,    cu_max: None,    cores_min: 1, cores_max: 2,  threads_per_core: 1, thread_tech: "none", avx: false, avx2: false, avx512: false, process: "40 nm", year: 2011 }),
    // Piledriver Terascale 3 APUs (FM2); CMT cores are single-threaded
    ("Trinity",       ApuFacts { arch: "Piledriver", socket: "FM2",          igpu_arch: "Terascale 3",       cu_min: None,    cu_max: None,    cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: false, avx512: false, process: "32 nm", year: 2012 }),
    ("Richland",      ApuFacts { arch: "Piledriver", socket: "FM2",          igpu_arch: "Terascale 3",       cu_min: None,    cu_max: None,    cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: false, avx512: false, process: "32 nm", year: 2013 }),
    // Steamroller GCN 2nd gen APUs (FM2+), three to eight GPU CUs
    ("Kaveri",        ApuFacts { arch: "Steamroller", socket: "FM2+",        igpu_arch: "GCN 2nd gen",       cu_min: Some(3), cu_max: Some(8), cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: false, avx512: false, process: "28 nm", year: 2014 }),
    ("Godavari",      ApuFacts { arch: "Steamroller", socket: "FM2+",        igpu_arch: "GCN 2nd gen",       cu_min: Some(3), cu_max: Some(8), cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: false, avx512: false, process: "28 nm", year: 2015 }),
    // Excavator GCN 3rd gen APUs; the first CMT family with AVX2, shipped
    // on FP4 (mobile), FM2+ and AM4 (desktop)
    ("Carrizo",       ApuFacts { arch: "Excavator",  socket: "FP4/FM2+/AM4", igpu_arch: "GCN 3rd gen",       cu_min: Some(4), cu_max: Some(8), cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: true,  avx512: false, process: "28 nm", year: 2015 }),
    ("Bristol Ridge", ApuFacts { arch: "Excavator",  socket: "AM4/FP4",      igpu_arch: "GCN 3rd gen",       cu_min: Some(4), cu_max: Some(8), cores_min: 2, cores_max: 4,  threads_per_core: 1, thread_tech: "none", avx: true,  avx2: true,  avx512: false, process: "28 nm", year: 2016 }),
    // Zen Vega APUs (GCN 5th gen): FP5 mobile plus AM4 desktop parts;
    // SMT is published for the family even where SKUs disable it
    ("Raven Ridge",   ApuFacts { arch: "Zen",        socket: "FP5/AM4",      igpu_arch: "GCN 5th gen (Vega)", cu_min: Some(3), cu_max: Some(11), cores_min: 2, cores_max: 4, threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "14 nm", year: 2017 }),
    ("Picasso",       ApuFacts { arch: "Zen+",       socket: "FP5/AM4",      igpu_arch: "GCN 5th gen (Vega)", cu_min: Some(3), cu_max: Some(11), cores_min: 2, cores_max: 4, threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "12 nm", year: 2019 }),
    ("Renoir",        ApuFacts { arch: "Zen 2",      socket: "FP6/AM4",      igpu_arch: "GCN 5th gen (Vega)", cu_min: Some(5), cu_max: Some(8), cores_min: 4, cores_max: 8,  threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "7 nm", year: 2020 }),
    ("Lucienne",      ApuFacts { arch: "Zen 2",      socket: "FP6",          igpu_arch: "GCN 5th gen (Vega)", cu_min: Some(6), cu_max: Some(8), cores_min: 4, cores_max: 8,  threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "7 nm", year: 2021 }),
    ("Cezanne",       ApuFacts { arch: "Zen 3",      socket: "FP6/AM4",      igpu_arch: "GCN 5th gen (Vega)", cu_min: Some(6), cu_max: Some(8), cores_min: 4, cores_max: 8,  threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "7 nm", year: 2021 }),
    ("Barcelo",       ApuFacts { arch: "Zen 3",      socket: "FP6",          igpu_arch: "GCN 5th gen (Vega)", cu_min: Some(3), cu_max: Some(8), cores_min: 2, cores_max: 8,  threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "7 nm", year: 2022 }),
    // Zen 3+ RDNA 2 APUs; Rembrandt-R refreshes use the FP7r2 package
    ("Rembrandt",     ApuFacts { arch: "Zen 3+",     socket: "FP7/FP7r2",    igpu_arch: "RDNA 2",            cu_min: Some(6), cu_max: Some(12), cores_min: 6, cores_max: 8, threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "6 nm", year: 2022 }),
    // Entry-level Zen 2 RDNA 2 APUs, two Radeon 610M compute units
    ("Mendocino",     ApuFacts { arch: "Zen 2",      socket: "FT6",          igpu_arch: "RDNA 2",            cu_min: Some(2), cu_max: Some(2), cores_min: 2, cores_max: 4,  threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: false, process: "6 nm", year: 2022 }),
    // Zen 4 RDNA 3 APUs: the first family with AVX-512
    ("Phoenix",       ApuFacts { arch: "Zen 4",      socket: "FP8",          igpu_arch: "RDNA 3",            cu_min: Some(4), cu_max: Some(12), cores_min: 4, cores_max: 8, threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: true,  process: "4 nm", year: 2023 }),
    ("Hawk Point",    ApuFacts { arch: "Zen 4",      socket: "FP8",          igpu_arch: "RDNA 3",            cu_min: Some(4), cu_max: Some(12), cores_min: 4, cores_max: 8, threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: true,  process: "4 nm", year: 2024 }),
    // Zen 5 RDNA 3.5 APUs
    ("Strix Point",   ApuFacts { arch: "Zen 5",      socket: "FP8",          igpu_arch: "RDNA 3.5",          cu_min: Some(12), cu_max: Some(16), cores_min: 10, cores_max: 12, threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true, avx512: true, process: "4 nm", year: 2024 }),
    ("Krackan Point", ApuFacts { arch: "Zen 5",      socket: "FP8",          igpu_arch: "RDNA 3.5",          cu_min: Some(2), cu_max: Some(8), cores_min: 4, cores_max: 8,  threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true,  avx512: true,  process: "4 nm", year: 2025 }),
    ("Strix Halo",    ApuFacts { arch: "Zen 5",      socket: "FP11",         igpu_arch: "RDNA 3.5",          cu_min: Some(32), cu_max: Some(40), cores_min: 8, cores_max: 16, threads_per_core: 2, thread_tech: "SMT", avx: true, avx2: true, avx512: true, process: "4 nm", year: 2025 }),
];

/// Per-family lookup. Unknown families return None; the catalog never
/// guesses.
pub fn facts_for(apu: &str) -> Option<&'static ApuFacts> {
    GENERATIONS
        .iter()
        .find(|(name, _)| *name == apu)
        .map(|(_, facts)| facts)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Isa {
    /// Baseline x86 SIMD floor every catalog family implements.
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
    /// AMD APU hardware (Ryzen/Athlon APU families with Radeon graphics).
    AmdApu,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports an x86-64 execution environment for the workers.
    pub x86_64_runtime: bool,
    /// Host reports AVX support.
    pub avx: bool,
    /// Host reports AVX2 support.
    pub avx2: bool,
    /// Host reports AVX-512 support.
    pub avx512: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdApuDeclaration {
    /// Exact family name from the catalog; unknown names never normalize.
    pub apu: String,
    /// Worker threads, from one through `MAX_WORKER_THREADS` and never more
    /// than the generation's published threads.
    pub worker_threads: u32,
    /// Batch size, from one through `MAX_BATCH`.
    pub batch: u32,
    /// Bound on per-batch workspace bytes, from one through
    /// `MAX_WORKSPACE_BYTES`.
    pub workspace_bytes: u64,
    /// Requested instruction-set floor for the planned workers.
    pub isa: Isa,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdApuIr {
    apu: String,
    worker_threads: u32,
    batch: u32,
    workspace_bytes: u64,
    isa: Isa,
    facts: &'static ApuFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmdApuDiagnostic {
    BadApu,
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

impl fmt::Display for AmdApuDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadApu => "APU family is not in the AMD APU catalog",
            Self::ZeroWorkerThreads => "worker thread count must be positive",
            Self::WorkerThreadsTooLarge => {
                "worker thread count exceeds the supported bound"
            }
            Self::ZeroBatch => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::ZeroWorkspaceBytes => "workspace bound must be positive",
            Self::WorkspaceBytesTooLarge => {
                "workspace bound exceeds the supported bound"
            }
            Self::WorkerOverflow => {
                "total worker executions do not fit the supported bound"
            }
            Self::ThreadsExceedGeneration => {
                "worker thread count exceeds the generation's published cores times threads per core"
            }
            Self::UnsupportedTarget => {
                "AMD APU compute capability supports only AMD APU targets"
            }
            Self::UnsupportedAccelerator => {
                "generation does not implement the requested ISA"
            }
            Self::MissingX86_64Runtime => {
                "target runtime must report an x86-64 execution environment"
            }
            Self::MissingAvx => "target runtime must report AVX support",
            Self::MissingAvx2 => "target runtime must report AVX2 support",
            Self::MissingAvx512 => "target runtime must report AVX-512 support",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for AmdApuDiagnostic {}

/// Negotiated design and source text. The artifact is a plan and a factory
/// definition; successful construction is not proof that CPU workers ran
/// the plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmdApuArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl AmdApuDeclaration {
    pub fn normalize(self) -> Result<AmdApuIr, AmdApuDiagnostic> {
        let facts = facts_for(&self.apu).ok_or(AmdApuDiagnostic::BadApu)?;
        let plan = crate::amd_apu_runtime::WorkerPlan::new(
            self.worker_threads,
            self.batch,
            self.workspace_bytes,
        )
        .map_err(|error| match error {
            crate::amd_apu_runtime::WorkerError::ZeroWorkerThreads => {
                AmdApuDiagnostic::ZeroWorkerThreads
            }
            crate::amd_apu_runtime::WorkerError::WorkerThreadsTooLarge => {
                AmdApuDiagnostic::WorkerThreadsTooLarge
            }
            crate::amd_apu_runtime::WorkerError::ZeroBatch => AmdApuDiagnostic::ZeroBatch,
            crate::amd_apu_runtime::WorkerError::BatchTooLarge => AmdApuDiagnostic::BatchTooLarge,
            crate::amd_apu_runtime::WorkerError::ZeroWorkspaceBytes => {
                AmdApuDiagnostic::ZeroWorkspaceBytes
            }
            crate::amd_apu_runtime::WorkerError::WorkspaceBytesTooLarge => {
                AmdApuDiagnostic::WorkspaceBytesTooLarge
            }
            crate::amd_apu_runtime::WorkerError::WorkerOverflow => AmdApuDiagnostic::WorkerOverflow,
        })?;
        debug_assert_eq!(plan.worker_threads(), self.worker_threads);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.workspace_bytes(), self.workspace_bytes);
        if self.worker_threads > facts.cores_max.saturating_mul(facts.threads_per_core) {
            return Err(AmdApuDiagnostic::ThreadsExceedGeneration);
        }
        Ok(AmdApuIr {
            apu: self.apu,
            worker_threads: self.worker_threads,
            batch: self.batch,
            workspace_bytes: self.workspace_bytes,
            isa: self.isa,
            facts,
        })
    }
}

impl AmdApuIr {
    pub fn apu(&self) -> &str {
        &self.apu
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

    pub fn facts(&self) -> &'static ApuFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        let cu_min = self
            .facts
            .cu_min
            .map(|units| units.to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        let cu_max = self
            .facts
            .cu_max
            .map(|units| units.to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=amd_apu\napu={}\narch={}\nsocket={}\nigpu_arch={}\nigpu_cu_min={cu_min}\nigpu_cu_max={cu_max}\ncores_min={}\ncores_max={}\nthreads_per_core={}\nthread_tech={}\navx={}\navx2={}\navx512={}\nprocess={}\nyear={}\nisa={}\nworker_threads={}\nbatch={}\ntotal_worker_executions={}\nworkspace_bytes={}\nexecution=none_attempted\n",
            self.apu,
            self.facts.arch,
            self.facts.socket,
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
    ) -> Result<AmdApuArtifact, AmdApuDiagnostic> {
        if target != Target::AmdApu {
            return Err(AmdApuDiagnostic::UnsupportedTarget);
        }
        if !capabilities.x86_64_runtime {
            return Err(AmdApuDiagnostic::MissingX86_64Runtime);
        }
        if !capabilities.utf8_strings {
            return Err(AmdApuDiagnostic::MissingUtf8Strings);
        }
        match self.isa {
            Isa::Sse3 => {}
            Isa::Avx => {
                if !capabilities.avx {
                    return Err(AmdApuDiagnostic::MissingAvx);
                }
                if !self.facts.avx {
                    return Err(AmdApuDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx2 => {
                if !capabilities.avx2 {
                    return Err(AmdApuDiagnostic::MissingAvx2);
                }
                if !self.facts.avx2 {
                    return Err(AmdApuDiagnostic::UnsupportedAccelerator);
                }
            }
            Isa::Avx512 => {
                if !capabilities.avx512 {
                    return Err(AmdApuDiagnostic::MissingAvx512);
                }
                if !self.facts.avx512 {
                    return Err(AmdApuDiagnostic::UnsupportedAccelerator);
                }
            }
        }
        Ok(AmdApuArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; AMD APU compute capability, no worker threads are spawned.\npub fn make_worker_plan() -> Result<crate::amd_apu_runtime::WorkerPlan, crate::amd_apu_runtime::WorkerError> {{\n    crate::amd_apu_runtime::WorkerPlan::new({}u32, {}u32, {}u64)\n}}\n",
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

    fn declaration(apu: &str, worker_threads: u32, batch: u32, isa: Isa) -> AmdApuDeclaration {
        AmdApuDeclaration {
            apu: apu.to_string(),
            worker_threads,
            batch,
            workspace_bytes: 4096,
            isa,
        }
    }

    fn first_avx512_generation() -> &'static str {
        // Tests lower AVX-512-era plans, so start from the first family
        // with documented AVX-512.
        GENERATIONS
            .iter()
            .find(|(_, facts)| facts.avx512)
            .map(|(name, _)| *name)
            .expect("catalog must contain an AVX-512 family")
    }

    #[test]
    fn catalog_entries_have_consistent_facts() {
        assert!(!GENERATIONS.is_empty(), "catalog must not be empty");
        let mut seen = std::collections::BTreeSet::new();
        let mut previous_year = 0;
        for (name, facts) in GENERATIONS {
            assert!(seen.insert(*name), "duplicate APU family {name}");
            assert!((2011..=2030).contains(&facts.year));
            assert!(facts.year >= previous_year, "catalog must be chronological");
            previous_year = facts.year;
            assert!(!facts.arch.is_empty());
            assert!(!facts.socket.is_empty());
            assert!(!facts.igpu_arch.is_empty());
            assert!(!facts.process.is_empty());
            assert!(facts.cores_min <= facts.cores_max);
            assert!(
                facts.threads_per_core == 1 || facts.threads_per_core == 2,
                "threads per core must be 1 or 2"
            );
            assert_eq!(
                facts.thread_tech,
                if facts.threads_per_core == 1 {
                    "none"
                } else {
                    "SMT"
                },
                "thread_tech must match threads_per_core"
            );
            assert!(
                facts.cores_max.saturating_mul(facts.threads_per_core) <= MAX_WORKER_THREADS,
                "generation threads must fit the runtime bound"
            );
            // AVX2 implies AVX; AVX-512 implies AVX2 and AVX.
            if facts.avx512 {
                assert!(facts.avx2 && facts.avx);
            }
            if facts.avx2 {
                assert!(facts.avx);
            }
            match (facts.cu_min, facts.cu_max) {
                (None, None) => {}
                (Some(min), Some(max)) => assert!(min <= max, "CU range must be ordered"),
                _ => panic!("CU range must be fully published or fully unpublished"),
            }
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let apu = first_avx512_generation();
        let ir = declaration(apu, 4, 8, Isa::Avx2).normalize().unwrap();
        assert_eq!(ir.worker_threads(), 4);
        assert_eq!(ir.batch(), 8);
        assert_eq!(ir.total_worker_executions(), 32);
        let first = ir.lower_checked(Target::AmdApu, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::AmdApu, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=amd_apu\n"));
        assert!(first.canonical_ir.contains("worker_threads=4\n"));
        assert!(first.canonical_ir.contains("total_worker_executions=32\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("WorkerPlan::new(4u32, 8u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_apus_and_bad_bounds() {
        assert_eq!(
            declaration("Gorgon Point", 4, 8, Isa::Sse3).normalize(),
            Err(AmdApuDiagnostic::BadApu)
        );
        // Brazos publishes 2 single-threaded cores, so four workers exceed
        // the generation even though they fit the global bound.
        assert_eq!(
            declaration("Brazos", 4, 8, Isa::Sse3).normalize(),
            Err(AmdApuDiagnostic::ThreadsExceedGeneration)
        );
        let apu = first_avx512_generation();
        assert_eq!(
            declaration(apu, 0, 8, Isa::Sse3).normalize(),
            Err(AmdApuDiagnostic::ZeroWorkerThreads)
        );
        assert_eq!(
            declaration(apu, MAX_WORKER_THREADS + 1, 8, Isa::Sse3).normalize(),
            Err(AmdApuDiagnostic::WorkerThreadsTooLarge)
        );
        assert_eq!(
            declaration(apu, 4, 0, Isa::Sse3).normalize(),
            Err(AmdApuDiagnostic::ZeroBatch)
        );
        assert_eq!(
            declaration(apu, 4, MAX_BATCH + 1, Isa::Sse3).normalize(),
            Err(AmdApuDiagnostic::BatchTooLarge)
        );
        let mut zero_workspace = declaration(apu, 4, 8, Isa::Sse3);
        zero_workspace.workspace_bytes = 0;
        assert_eq!(
            zero_workspace.normalize(),
            Err(AmdApuDiagnostic::ZeroWorkspaceBytes)
        );
        let mut huge_workspace = declaration(apu, 4, 8, Isa::Sse3);
        huge_workspace.workspace_bytes = MAX_WORKSPACE_BYTES + 1;
        assert_eq!(
            huge_workspace.normalize(),
            Err(AmdApuDiagnostic::WorkspaceBytesTooLarge)
        );
        // Strix Halo publishes 16 SMT cores, exactly the global bound.
        assert!(
            declaration("Strix Halo", MAX_WORKER_THREADS, MAX_BATCH, Isa::Sse3)
                .normalize()
                .is_ok()
        );
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let apu = first_avx512_generation();
        let ir = declaration(apu, 4, 8, Isa::Avx512).normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(AmdApuDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    x86_64_runtime: false,
                    ..CAPABILITIES
                },
                AmdApuDiagnostic::MissingX86_64Runtime,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                AmdApuDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::AmdApu, missing), Err(expected));
        }
        // ISA negotiation: each level asks for its own runtime facility and
        // fails closed when the generation lacks the hardware support.
        for (isa, flag, missing, unsupported_apu) in [
            (
                Isa::Avx,
                RuntimeCapabilities {
                    avx: false,
                    ..CAPABILITIES
                },
                AmdApuDiagnostic::MissingAvx,
                "Brazos",
            ),
            (
                Isa::Avx2,
                RuntimeCapabilities {
                    avx2: false,
                    ..CAPABILITIES
                },
                AmdApuDiagnostic::MissingAvx2,
                "Kaveri",
            ),
            (
                Isa::Avx512,
                RuntimeCapabilities {
                    avx512: false,
                    ..CAPABILITIES
                },
                AmdApuDiagnostic::MissingAvx512,
                "Cezanne",
            ),
        ] {
            let ir = declaration(apu, 4, 8, isa).normalize().unwrap();
            assert_eq!(ir.lower_checked(Target::AmdApu, flag), Err(missing));
            let older = declaration(unsupported_apu, 2, 2, isa).normalize().unwrap();
            assert_eq!(
                older.lower_checked(Target::AmdApu, CAPABILITIES),
                Err(AmdApuDiagnostic::UnsupportedAccelerator)
            );
        }
        // The iGPU facility was never a negotiated requirement for CPU plans.
        ir.lower_checked(Target::AmdApu, CAPABILITIES).unwrap();
    }

    #[test]
    fn every_generation_lowers_sse3_and_avx512_fails_closed_before_zen_4() {
        let mut lowered = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 1, 2, Isa::Sse3).normalize().unwrap();
            ir.lower_checked(Target::AmdApu, CAPABILITIES).unwrap();
            let ir = declaration(name, 1, 2, Isa::Avx512).normalize().unwrap();
            if !facts.avx512 {
                assert_eq!(
                    ir.lower_checked(Target::AmdApu, CAPABILITIES),
                    Err(AmdApuDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::AmdApu, CAPABILITIES).unwrap();
            lowered += 1;
        }
        assert_eq!(lowered, 5, "Zen 4 and Zen 5 families lower AVX-512 plans");
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_amd_apu_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let apu = first_avx512_generation();
        let artifact = declaration(apu, 4, 3, Isa::Sse3)
            .normalize()
            .unwrap()
            .lower_checked(Target::AmdApu, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/amd_apu_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod amd_apu_runtime;\n{}\nfn main() {{\n    let plan = make_worker_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.worker_threads(), 4);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_worker_executions(), 12);\n    assert_eq!(plan.workspace_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-amd-apu-factory-{}-{nonce}",
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
