//! A bounded NVIDIA CUDA GPU-compute capability.
//!
//! The negotiated artifact is a checked launch plan and a factory binding
//! to this crate's plan runtime. It does not compile kernels, claim driver
//! access, guarantee scheduling, or prove that a GPU executed work. Hosts
//! with NVIDIA hardware report their real CUDA facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.gpu_compute.nvidia_cuda.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_THREADS_PER_BLOCK: u32 = crate::nvidia_runtime::MAX_THREADS_PER_BLOCK;
pub const MAX_BLOCKS: u32 = crate::nvidia_runtime::MAX_BLOCKS;
pub const MAX_SHARED_MEM_BYTES: u64 = crate::nvidia_runtime::MAX_SHARED_MEM_BYTES;

/// Published accelerator facts for one NVIDIA GPU SKU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuFacts {
    /// Product-series tag: `10`, `16`, `20`, `30`, `40`, or `iGPU`.
    pub series: &'static str,
    /// Chip code (for example `GP107`, `TU102`, `AD104`).
    pub chip: &'static str,
    /// Architecture name (for example `Pascal`, `Ada Lovelace`).
    pub arch: &'static str,
    /// CUDA compute capability as NVIDIA publishes it (for example `8.9`).
    pub cc: &'static str,
    /// Published CUDA core count; `None` where NVIDIA published none.
    pub cuda_cores: Option<u32>,
    /// Published Tensor Core count; `None` for parts without Tensor Cores.
    pub tensor_cores: Option<u32>,
    /// Published VRAM, in bytes.
    pub vram_bytes: Option<u64>,
    /// Documented Int8 Tensor Core acceleration.
    pub int8_tensor: bool,
    /// Documented FP16 Tensor Core acceleration.
    pub fp16_tensor: bool,
    /// First release year.
    pub year: u16,
}

/// Catalog order is tier/model-number order within each series block;
/// series blocks appear in ascending order, and the `year` field records
/// each SKU's true launch year. Facts come from NVIDIA's CUDA GPU list and
/// the public GeForce record; disputed multi-SKU cards are recorded under
/// their dominant SKU with variants noted in the chip label.
#[rustfmt::skip]
pub const GPUS: &[(&str, GpuFacts)] = &[
    // GTX 10 series (Pascal)
    ("GTX 1050",         GpuFacts { series: "10", chip: "GP107", arch: "Pascal", cc: "6.1", cuda_cores: Some(640),  tensor_cores: None,     vram_bytes: Some(2 << 30),  int8_tensor: false, fp16_tensor: false, year: 2016 }),
    ("GTX 1050 Ti",      GpuFacts { series: "10", chip: "GP107", arch: "Pascal", cc: "6.1", cuda_cores: Some(768),  tensor_cores: None,     vram_bytes: Some(4 << 30),  int8_tensor: false, fp16_tensor: false, year: 2016 }),
    ("GTX 1060",         GpuFacts { series: "10", chip: "GP106", arch: "Pascal", cc: "6.1", cuda_cores: Some(1280), tensor_cores: None,     vram_bytes: Some(6 << 30),  int8_tensor: false, fp16_tensor: false, year: 2016 }),
    ("GTX 1070",         GpuFacts { series: "10", chip: "GP104", arch: "Pascal", cc: "6.1", cuda_cores: Some(1920), tensor_cores: None,     vram_bytes: Some(8 << 30),  int8_tensor: false, fp16_tensor: false, year: 2016 }),
    ("GTX 1070 Ti",      GpuFacts { series: "10", chip: "GP104", arch: "Pascal", cc: "6.1", cuda_cores: Some(2432), tensor_cores: None,     vram_bytes: Some(8 << 30),  int8_tensor: false, fp16_tensor: false, year: 2017 }),
    ("GTX 1080",         GpuFacts { series: "10", chip: "GP104", arch: "Pascal", cc: "6.1", cuda_cores: Some(2560), tensor_cores: None,     vram_bytes: Some(8 << 30),  int8_tensor: false, fp16_tensor: false, year: 2016 }),
    ("GTX 1080 Ti",      GpuFacts { series: "10", chip: "GP102", arch: "Pascal", cc: "6.1", cuda_cores: Some(3584), tensor_cores: None,     vram_bytes: Some(11 << 30), int8_tensor: false, fp16_tensor: false, year: 2017 }),
    ("Titan X (Pascal)", GpuFacts { series: "10", chip: "GP102", arch: "Pascal", cc: "6.1", cuda_cores: Some(3584), tensor_cores: None,     vram_bytes: Some(12 << 30), int8_tensor: false, fp16_tensor: false, year: 2016 }),
    // GTX 16 series (Turing without Tensor Cores)
    ("GTX 1650 Mobile",  GpuFacts { series: "16", chip: "TU117", arch: "Turing", cc: "7.5", cuda_cores: Some(896),  tensor_cores: None,     vram_bytes: Some(4 << 30),  int8_tensor: false, fp16_tensor: false, year: 2019 }),
    // RTX 20 series (Turing)
    ("RTX 2060",         GpuFacts { series: "20", chip: "TU106", arch: "Turing", cc: "7.5", cuda_cores: Some(1920), tensor_cores: Some(240), vram_bytes: Some(6 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2019 }),
    ("RTX 2060 Super",   GpuFacts { series: "20", chip: "TU106", arch: "Turing", cc: "7.5", cuda_cores: Some(2176), tensor_cores: Some(272), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2019 }),
    ("RTX 2070",         GpuFacts { series: "20", chip: "TU106", arch: "Turing", cc: "7.5", cuda_cores: Some(2304), tensor_cores: Some(288), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2018 }),
    ("RTX 2070 Super",   GpuFacts { series: "20", chip: "TU104", arch: "Turing", cc: "7.5", cuda_cores: Some(2560), tensor_cores: Some(320), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2019 }),
    ("RTX 2080",         GpuFacts { series: "20", chip: "TU104", arch: "Turing", cc: "7.5", cuda_cores: Some(2944), tensor_cores: Some(368), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2018 }),
    ("RTX 2080 Super",   GpuFacts { series: "20", chip: "TU104", arch: "Turing", cc: "7.5", cuda_cores: Some(3072), tensor_cores: Some(384), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2019 }),
    ("RTX 2080 Ti",      GpuFacts { series: "20", chip: "TU102", arch: "Turing", cc: "7.5", cuda_cores: Some(4352), tensor_cores: Some(544), vram_bytes: Some(11 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2018 }),
    ("Titan RTX",        GpuFacts { series: "20", chip: "TU102", arch: "Turing", cc: "7.5", cuda_cores: Some(4608), tensor_cores: Some(576), vram_bytes: Some(24 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2018 }),
    ("RTX 2060 Mobile",  GpuFacts { series: "20", chip: "TU106", arch: "Turing", cc: "7.5", cuda_cores: Some(1920), tensor_cores: Some(240), vram_bytes: Some(6 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2019 }),
    // RTX 30 series (Ampere)
    ("RTX 3050 8GB",     GpuFacts { series: "30", chip: "GA106", arch: "Ampere", cc: "8.6", cuda_cores: Some(2560),  tensor_cores: Some(80),  vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2022 }),
    ("RTX 3050 6GB",     GpuFacts { series: "30", chip: "GA107", arch: "Ampere", cc: "8.6", cuda_cores: Some(2304),  tensor_cores: Some(72),  vram_bytes: Some(6 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2024 }),
    ("RTX 3060",         GpuFacts { series: "30", chip: "GA106", arch: "Ampere", cc: "8.6", cuda_cores: Some(3584),  tensor_cores: Some(112), vram_bytes: Some(12 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2021 }),
    ("RTX 3060 Ti",      GpuFacts { series: "30", chip: "GA104", arch: "Ampere", cc: "8.6", cuda_cores: Some(4864),  tensor_cores: Some(152), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2020 }),
    ("RTX 3070",         GpuFacts { series: "30", chip: "GA104", arch: "Ampere", cc: "8.6", cuda_cores: Some(5888),  tensor_cores: Some(184), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2020 }),
    ("RTX 3070 Ti",      GpuFacts { series: "30", chip: "GA104", arch: "Ampere", cc: "8.6", cuda_cores: Some(6144),  tensor_cores: Some(192), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2021 }),
    ("RTX 3080",         GpuFacts { series: "30", chip: "GA102", arch: "Ampere", cc: "8.6", cuda_cores: Some(8704),  tensor_cores: Some(272), vram_bytes: Some(10 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2020 }),
    ("RTX 3080 Ti",      GpuFacts { series: "30", chip: "GA102", arch: "Ampere", cc: "8.6", cuda_cores: Some(10240), tensor_cores: Some(320), vram_bytes: Some(12 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2021 }),
    ("RTX 3090",         GpuFacts { series: "30", chip: "GA102", arch: "Ampere", cc: "8.6", cuda_cores: Some(10496), tensor_cores: Some(328), vram_bytes: Some(24 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2020 }),
    ("RTX 3090 Ti",      GpuFacts { series: "30", chip: "GA102", arch: "Ampere", cc: "8.6", cuda_cores: Some(10752), tensor_cores: Some(336), vram_bytes: Some(24 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2022 }),
    ("RTX 3060 Mobile",  GpuFacts { series: "30", chip: "GA106", arch: "Ampere", cc: "8.6", cuda_cores: Some(3840),  tensor_cores: Some(120), vram_bytes: Some(6 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2021 }),
    // RTX 40 series (Ada Lovelace)
    ("RTX 4060",         GpuFacts { series: "40", chip: "AD107", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(3072),  tensor_cores: Some(96),  vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2023 }),
    ("RTX 4060 Ti 8GB",  GpuFacts { series: "40", chip: "AD106", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(4352),  tensor_cores: Some(136), vram_bytes: Some(8 << 30),  int8_tensor: true,  fp16_tensor: true,  year: 2023 }),
    ("RTX 4060 Ti 16GB", GpuFacts { series: "40", chip: "AD106", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(4352),  tensor_cores: Some(136), vram_bytes: Some(16 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2023 }),
    ("RTX 4070",         GpuFacts { series: "40", chip: "AD104", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(5888),  tensor_cores: Some(184), vram_bytes: Some(12 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2023 }),
    ("RTX 4070 Super",   GpuFacts { series: "40", chip: "AD104", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(7168),  tensor_cores: Some(224), vram_bytes: Some(12 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2024 }),
    ("RTX 4070 Ti",      GpuFacts { series: "40", chip: "AD104", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(7680),  tensor_cores: Some(240), vram_bytes: Some(12 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2023 }),
    ("RTX 4070 Ti Super", GpuFacts { series: "40", chip: "AD103", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(8448), tensor_cores: Some(264), vram_bytes: Some(16 << 30), int8_tensor: true, fp16_tensor: true,  year: 2024 }),
    ("RTX 4080",         GpuFacts { series: "40", chip: "AD103", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(9728),  tensor_cores: Some(304), vram_bytes: Some(16 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2022 }),
    ("RTX 4080 Super",   GpuFacts { series: "40", chip: "AD103", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(10240), tensor_cores: Some(320), vram_bytes: Some(16 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2024 }),
    ("RTX 4090",         GpuFacts { series: "40", chip: "AD102", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(16384), tensor_cores: Some(512), vram_bytes: Some(24 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2022 }),
    ("RTX 4090 Mobile",  GpuFacts { series: "40", chip: "AD103", arch: "Ada Lovelace", cc: "8.9", cuda_cores: Some(9728),  tensor_cores: Some(304), vram_bytes: Some(16 << 30), int8_tensor: true,  fp16_tensor: true,  year: 2023 }),
    // Integrated Blackwell (DGX Spark). The Switch 2 T239 is excluded:
    // NVIDIA has never documented its chip code, compute capability or
    // core counts.
    ("GB10 (DGX Spark)", GpuFacts { series: "iGPU", chip: "GB10", arch: "Blackwell", cc: "12.1", cuda_cores: Some(6144), tensor_cores: Some(204), vram_bytes: Some(128 << 30), int8_tensor: true, fp16_tensor: true, year: 2025 }),
];

/// Per-GPU lookup. Unknown SKUs return None; the catalog never guesses.
pub fn facts_for(gpu: &str) -> Option<&'static GpuFacts> {
    GPUS.iter().find(|(name, _)| *name == gpu).map(|(_, facts)| facts)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// NVIDIA CUDA hardware (GeForce, Titan, integrated Blackwell parts).
    NvidiaCuda,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports a CUDA-capable device is present and addressable.
    pub cuda_device: bool,
    /// Host provides the CUDA runtime/driver API surface.
    pub cuda_runtime_api: bool,
    /// The planned kernel graph is Int8 quantized for Tensor Core paths.
    pub int8_model: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NvidiaCudaDeclaration {
    /// Exact GPU name from the catalog; unknown names never normalize.
    pub gpu: String,
    /// Threads per block; a power of two, from one through
    /// `MAX_THREADS_PER_BLOCK` (the CUDA limit).
    pub threads_per_block: u32,
    /// Blocks dispatched, from one through `MAX_BLOCKS`.
    pub blocks: u32,
    /// Bound on per-block dynamic shared memory, from one through
    /// `MAX_SHARED_MEM_BYTES`.
    pub max_shared_mem_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NvidiaCudaIr {
    gpu: String,
    threads_per_block: u32,
    blocks: u32,
    max_shared_mem_bytes: u64,
    facts: &'static GpuFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NvidiaCudaDiagnostic {
    BadGpu,
    ThreadsPerBlockZero,
    ThreadsPerBlockNotPowerOfTwo,
    ThreadsPerBlockTooLarge,
    BlocksZero,
    BlocksTooLarge,
    SharedMemBytesZero,
    SharedMemTooLarge,
    ThreadCountOverflow,
    UnsupportedTarget,
    UnsupportedAccelerator,
    MissingCudaDevice,
    MissingCudaRuntimeApi,
    MissingInt8Model,
    MissingUtf8Strings,
}

impl fmt::Display for NvidiaCudaDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadGpu => "GPU is not in the NVIDIA CUDA catalog",
            Self::ThreadsPerBlockZero => "threads per block must be positive",
            Self::ThreadsPerBlockNotPowerOfTwo => {
                "threads per block must be a power of two"
            }
            Self::ThreadsPerBlockTooLarge => {
                "threads per block exceeds the CUDA limit of 1024"
            }
            Self::BlocksZero => "block count must be positive",
            Self::BlocksTooLarge => "block count exceeds the supported bound",
            Self::SharedMemBytesZero => "shared memory bound must be positive",
            Self::SharedMemTooLarge => "shared memory bound exceeds the supported bound",
            Self::ThreadCountOverflow => "total thread count does not fit the supported bound",
            Self::UnsupportedTarget => "NVIDIA CUDA accelerator supports only CUDA targets",
            Self::UnsupportedAccelerator => {
                "GPU has no Tensor Cores for Int8-delegated workloads"
            }
            Self::MissingCudaDevice => "target runtime must report a CUDA device",
            Self::MissingCudaRuntimeApi => "target runtime must provide the CUDA runtime API",
            Self::MissingInt8Model => "kernel graph must be Int8 quantized for delegation",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for NvidiaCudaDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a GPU ran
/// the launch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NvidiaCudaArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl NvidiaCudaDeclaration {
    pub fn normalize(self) -> Result<NvidiaCudaIr, NvidiaCudaDiagnostic> {
        let facts = facts_for(&self.gpu).ok_or(NvidiaCudaDiagnostic::BadGpu)?;
        let plan = crate::nvidia_runtime::CudaLaunchPlan::new(
            self.threads_per_block,
            self.blocks,
            self.max_shared_mem_bytes,
        )
        .map_err(|error| match error {
            crate::nvidia_runtime::CudaError::ZeroThreadsPerBlock => {
                NvidiaCudaDiagnostic::ThreadsPerBlockZero
            }
            crate::nvidia_runtime::CudaError::ThreadsPerBlockNotPowerOfTwo => {
                NvidiaCudaDiagnostic::ThreadsPerBlockNotPowerOfTwo
            }
            crate::nvidia_runtime::CudaError::ThreadsPerBlockTooLarge => {
                NvidiaCudaDiagnostic::ThreadsPerBlockTooLarge
            }
            crate::nvidia_runtime::CudaError::ZeroBlocks => {
                NvidiaCudaDiagnostic::BlocksZero
            }
            crate::nvidia_runtime::CudaError::BlocksTooLarge => {
                NvidiaCudaDiagnostic::BlocksTooLarge
            }
            crate::nvidia_runtime::CudaError::ZeroSharedMemBytes => {
                NvidiaCudaDiagnostic::SharedMemBytesZero
            }
            crate::nvidia_runtime::CudaError::SharedMemTooLarge => {
                NvidiaCudaDiagnostic::SharedMemTooLarge
            }
            crate::nvidia_runtime::CudaError::BlockThreadOverflow => {
                NvidiaCudaDiagnostic::ThreadCountOverflow
            }
        })?;
        debug_assert_eq!(plan.threads_per_block(), self.threads_per_block);
        debug_assert_eq!(plan.blocks(), self.blocks);
        debug_assert_eq!(plan.max_shared_mem_bytes(), self.max_shared_mem_bytes);
        Ok(NvidiaCudaIr {
            gpu: self.gpu,
            threads_per_block: self.threads_per_block,
            blocks: self.blocks,
            max_shared_mem_bytes: self.max_shared_mem_bytes,
            facts,
        })
    }
}

impl NvidiaCudaIr {
    pub fn gpu(&self) -> &str {
        &self.gpu
    }

    pub fn threads_per_block(&self) -> u32 {
        self.threads_per_block
    }

    pub fn blocks(&self) -> u32 {
        self.blocks
    }

    pub fn max_shared_mem_bytes(&self) -> u64 {
        self.max_shared_mem_bytes
    }

    pub fn total_threads(&self) -> u64 {
        u64::from(self.blocks) * u64::from(self.threads_per_block)
    }

    pub fn facts(&self) -> &'static GpuFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        let cores = self
            .facts
            .cuda_cores
            .map(|cores| cores.to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        let tensor = self
            .facts
            .tensor_cores
            .map(|tensor| tensor.to_string())
            .unwrap_or_else(|| "none".to_string());
        let vram_gib = self
            .facts
            .vram_bytes
            .map(|bytes| (bytes >> 30).to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=nvidia_cuda\ngpu={}\nchip={}\narch={}\ncompute_capability={}\ncuda_cores={cores}\ntensor_cores={tensor}\nvram_gib={vram_gib}\nint8_tensor={}\nfp16_tensor={}\nyear={}\nthreads_per_block={}\nblocks={}\ntotal_threads={}\nmax_shared_mem_bytes={}\nexecution=none_attempted\n",
            self.gpu,
            self.facts.chip,
            self.facts.arch,
            self.facts.cc,
            self.facts.int8_tensor,
            self.facts.fp16_tensor,
            self.facts.year,
            self.threads_per_block,
            self.blocks,
            self.total_threads(),
            self.max_shared_mem_bytes,
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<NvidiaCudaArtifact, NvidiaCudaDiagnostic> {
        if target != Target::NvidiaCuda {
            return Err(NvidiaCudaDiagnostic::UnsupportedTarget);
        }
        if !capabilities.cuda_device {
            return Err(NvidiaCudaDiagnostic::MissingCudaDevice);
        }
        if !capabilities.cuda_runtime_api {
            return Err(NvidiaCudaDiagnostic::MissingCudaRuntimeApi);
        }
        if !capabilities.utf8_strings {
            return Err(NvidiaCudaDiagnostic::MissingUtf8Strings);
        }
        if capabilities.int8_model && !self.facts.int8_tensor {
            return Err(NvidiaCudaDiagnostic::UnsupportedAccelerator);
        }
        if !capabilities.int8_model {
            return Err(NvidiaCudaDiagnostic::MissingInt8Model);
        }
        Ok(NvidiaCudaArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; NVIDIA CUDA accelerator, no kernel launch is attempted.\npub fn make_launch_plan() -> Result<crate::nvidia_runtime::CudaLaunchPlan, crate::nvidia_runtime::CudaError> {{\n    crate::nvidia_runtime::CudaLaunchPlan::new({}u32, {}u32, {}u64)\n}}\n",
                self.threads_per_block, self.blocks, self.max_shared_mem_bytes,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES: RuntimeCapabilities = RuntimeCapabilities {
        cuda_device: true,
        cuda_runtime_api: true,
        int8_model: true,
        utf8_strings: true,
    };

    fn declaration(gpu: &str, threads_per_block: u32, blocks: u32) -> NvidiaCudaDeclaration {
        NvidiaCudaDeclaration {
            gpu: gpu.to_string(),
            threads_per_block,
            blocks,
            max_shared_mem_bytes: 4096,
        }
    }

    fn first_tensor_gpu() -> &'static str {
        // Tests lower launch plans for Int8-delegated work, so start from the
        // first catalog entry with Tensor Cores.
        GPUS
            .iter()
            .find(|(_, facts)| facts.int8_tensor)
            .map(|(name, _)| *name)
            .expect("catalog must contain a Tensor Core GPU")
    }

    #[test]
    fn catalog_entries_have_consistent_facts() {
        assert!(!GPUS.is_empty(), "catalog must not be empty");
        let mut seen = std::collections::BTreeSet::new();
        for (name, facts) in GPUS {
            assert!(seen.insert(*name), "duplicate GPU {name}");
            assert!(matches!(facts.series, "10" | "16" | "20" | "30" | "40" | "iGPU"));
            assert!(facts.cc.contains('.'), "compute capability must look like n.m");
            assert!((2013..=2030).contains(&facts.year));
            assert!(!facts.chip.is_empty());
            if facts.tensor_cores.is_none() {
                assert!(!facts.int8_tensor && !facts.fp16_tensor);
            }
        }
        // Series blocks appear in ascending order and never interleave.
        let series_order: Vec<&str> = GPUS.iter().map(|(_, f)| f.series).collect();
        let mut compact: Vec<&str> = series_order.clone();
        compact.dedup();
        assert_eq!(
            compact,
            ["10", "16", "20", "30", "40", "iGPU"]
                .iter()
                .copied()
                .filter(|s| GPUS.iter().any(|(_, f)| f.series == *s))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let gpu = first_tensor_gpu();
        let ir = declaration(gpu, 256, 8).normalize().unwrap();
        assert_eq!(ir.threads_per_block(), 256);
        assert_eq!(ir.blocks(), 8);
        assert_eq!(ir.total_threads(), 2048);
        let first = ir.lower_checked(Target::NvidiaCuda, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::NvidiaCuda, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=nvidia_cuda\n"));
        assert!(first.canonical_ir.contains("threads_per_block=256\n"));
        assert!(first.canonical_ir.contains("total_threads=2048\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("CudaLaunchPlan::new(256u32, 8u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_gpus_and_bad_bounds() {
        assert_eq!(
            declaration("GeForce FFT", 256, 8).normalize(),
            Err(NvidiaCudaDiagnostic::BadGpu)
        );
        let gpu = first_tensor_gpu();
        assert_eq!(
            declaration(gpu, 0, 8).normalize(),
            Err(NvidiaCudaDiagnostic::ThreadsPerBlockZero)
        );
        assert_eq!(
            declaration(gpu, 63, 8).normalize(),
            Err(NvidiaCudaDiagnostic::ThreadsPerBlockNotPowerOfTwo)
        );
        assert_eq!(
            declaration(gpu, MAX_THREADS_PER_BLOCK * 2, 8).normalize(),
            Err(NvidiaCudaDiagnostic::ThreadsPerBlockTooLarge)
        );
        assert_eq!(
            declaration(gpu, 256, 0).normalize(),
            Err(NvidiaCudaDiagnostic::BlocksZero)
        );
        assert_eq!(
            declaration(gpu, 256, MAX_BLOCKS + 1).normalize(),
            Err(NvidiaCudaDiagnostic::BlocksTooLarge)
        );
        let mut zero_shared = declaration(gpu, 256, 8);
        zero_shared.max_shared_mem_bytes = 0;
        assert_eq!(
            zero_shared.normalize(),
            Err(NvidiaCudaDiagnostic::SharedMemBytesZero)
        );
        let mut huge_shared = declaration(gpu, 256, 8);
        huge_shared.max_shared_mem_bytes = MAX_SHARED_MEM_BYTES + 1;
        assert_eq!(
            huge_shared.normalize(),
            Err(NvidiaCudaDiagnostic::SharedMemTooLarge)
        );
        assert!(declaration(gpu, MAX_THREADS_PER_BLOCK, MAX_BLOCKS).normalize().is_ok());
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let gpu = first_tensor_gpu();
        let ir = declaration(gpu, 256, 8).normalize().unwrap();
        for target in [
            Target::CrossPlatform,
            Target::Browser,
            Target::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(NvidiaCudaDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    cuda_device: false,
                    ..CAPABILITIES
                },
                NvidiaCudaDiagnostic::MissingCudaDevice,
            ),
            (
                RuntimeCapabilities {
                    cuda_runtime_api: false,
                    ..CAPABILITIES
                },
                NvidiaCudaDiagnostic::MissingCudaRuntimeApi,
            ),
            (
                RuntimeCapabilities {
                    int8_model: false,
                    ..CAPABILITIES
                },
                NvidiaCudaDiagnostic::MissingInt8Model,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                NvidiaCudaDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::NvidiaCuda, missing),
                Err(expected)
            );
        }
    }

    #[test]
    fn every_gpu_with_tensor_cores_lowers_and_non_tensor_silicon_fails_closed() {
        let mut lowered = 0;
        for (name, facts) in GPUS {
            let ir = declaration(name, 64, 2).normalize().unwrap();
            if !facts.int8_tensor {
                assert_eq!(
                    ir.lower_checked(Target::NvidiaCuda, CAPABILITIES),
                    Err(NvidiaCudaDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::NvidiaCuda, CAPABILITIES).unwrap();
            lowered += 1;
        }
        assert!(lowered > 0, "catalog must lower at least one GPU");
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_nvidia_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let gpu = first_tensor_gpu();
        let artifact = declaration(gpu, 128, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::NvidiaCuda, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/nvidia_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod nvidia_runtime;\n{}\nfn main() {{\n    let plan = make_launch_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.threads_per_block(), 128);\n    assert_eq!(plan.blocks(), 3);\n    assert_eq!(plan.total_threads(), 384);\n    assert_eq!(plan.max_shared_mem_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-nvidia-factory-{}-{nonce}",
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
            .expect("run generated launch factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
