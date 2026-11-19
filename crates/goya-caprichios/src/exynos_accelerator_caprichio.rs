//! A bounded Exynos SoC neural-acceleration capability.
//!
//! The negotiated artifact is a checked delegate plan and a factory binding
//! to this crate's plan runtime. It does not compile models, claim DSP
//! access, guarantee scheduling, or prove that inference ran. Hosts on
//! Exynos hardware report their real npu facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.neural_accel.exynos.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_LAYERS: u32 = crate::exynos_runtime::MAX_LAYERS;
pub const MAX_BATCH: u32 = crate::exynos_runtime::MAX_BATCH;
pub const MAX_WEIGHT_BYTES: u64 = crate::exynos_runtime::MAX_WEIGHT_BYTES;

/// Published accelerator facts for one Exynos SoC generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExynosFacts {
    /// Samsung part number (for example `S5E9955`), or `unpublished`.
    pub soc: &'static str,
    /// Series tag; all cataloged Exynos flagships are `8`-series contemporaries.
    pub series: &'static str,
    /// Documented NPU/NPU generation, or `none`.
    pub npu_dsp: &'static str,
    /// Peak AI throughput Samsung published for this SoC, in TOPS times
    /// ten (26 TOPS is stored as `260`). `None` means Samsung did not
    /// publish a figure; none is guessed.
    pub npu_tops_tenths: Option<u16>,
    /// Documented Int8 neural acceleration.
    pub int8_accel: bool,
    /// Documented FP16 neural acceleration.
    pub fp16_accel: bool,
    /// First release year.
    pub year: u16,
}

/// Catalog order is series-major (8, 7, 6, 4); within a series, release
/// order. Facts come from Samsung product briefs and official Exynos
/// documentation; figures Samsung did not publish are `None`, never
/// guessed. Exynos 2300 is excluded: designed but never shipped
/// commercially. Each entry is the Samsung flagship contemporary with a
/// Snapdragon 8-series generation.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, ExynosFacts)] = &[
    ("Exynos 7420",  ExynosFacts { series: "8", soc: "S5E7420",  npu_dsp: "none",                               npu_tops_tenths: None,      int8_accel: false, fp16_accel: false, year: 2015 }),
    ("Exynos 8890",  ExynosFacts { series: "8", soc: "S5E8890",  npu_dsp: "none",                               npu_tops_tenths: None,      int8_accel: false, fp16_accel: false, year: 2016 }),
    ("Exynos 8895",  ExynosFacts { series: "8", soc: "S5E8895",  npu_dsp: "none", npu_tops_tenths: None, int8_accel: false, fp16_accel: false, year: 2017 }), // camera VPU only, no public SDK
    ("Exynos 9810",  ExynosFacts { series: "8", soc: "S5E9810",  npu_dsp: "none",                               npu_tops_tenths: None,      int8_accel: false, fp16_accel: false, year: 2018 }),
    ("Exynos 9820",  ExynosFacts { series: "8", soc: "S5E9820",  npu_dsp: "NPU (first Exynos NPU)",             npu_tops_tenths: None,      int8_accel: false, fp16_accel: false, year: 2019 }),
    ("Exynos 9825",  ExynosFacts { series: "8", soc: "S5E9825",  npu_dsp: "NPU (9820-generation)",              npu_tops_tenths: None,      int8_accel: false, fp16_accel: false, year: 2019 }),
    ("Exynos 990",   ExynosFacts { series: "8", soc: "S5E9830",  npu_dsp: "NPU (dual-core) + DSP",              npu_tops_tenths: None,      int8_accel: false, fp16_accel: false, year: 2020 }),
    ("Exynos 2100",  ExynosFacts { series: "8", soc: "S5E9840",  npu_dsp: "NPU (tri-core) + DSP",               npu_tops_tenths: Some(260), int8_accel: false, fp16_accel: false, year: 2021 }),
    ("Exynos 2200",  ExynosFacts { series: "8", soc: "S5E9925",  npu_dsp: "NPU + DSP (INT8/INT16/FP16)",        npu_tops_tenths: None,      int8_accel: true,  fp16_accel: true,  year: 2022 }),
    ("Exynos 2400",  ExynosFacts { series: "8", soc: "S5E9240",  npu_dsp: "NPU (GNPU + SNPU, 17K MAC)",              npu_tops_tenths: None,      int8_accel: true,  fp16_accel: true,  year: 2024 }),
    ("Exynos 2500",  ExynosFacts { series: "8", soc: "S5E9955",  npu_dsp: "NPU (dual 12K-MAC clusters)",        npu_tops_tenths: Some(590), int8_accel: true,  fp16_accel: true,  year: 2025 }),
];

/// Per-generation lookup. Unknown generations return None; the catalog never
/// guesses.
pub fn facts_for(soc: &str) -> Option<&'static ExynosFacts> {
    GENERATIONS
        .iter()
        .find(|(name, _)| *name == soc)
        .map(|(_, facts)| facts)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// Exynos SoC hardware (handsets, tablets, XR).
    Exynos,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports an NPU/NPU is present and addressable.
    pub npu_dsp: bool,
    /// Host provides a neural delegate (for example SNPE or QNN style).
    pub neural_delegate: bool,
    /// The model graph is Int8 quantized.
    pub int8_model: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExynosAcceleratorDeclaration {
    /// Exact generation name from the catalog; unknown names never normalize.
    pub soc: String,
    /// Layers in the planned graph, from one through `MAX_LAYERS`.
    pub layers: u32,
    /// Batch size, from one through `MAX_BATCH`.
    pub batch: u32,
    /// Bound on any single weight blob, from one through `MAX_WEIGHT_BYTES`.
    pub max_weight_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExynosAcceleratorIr {
    soc: String,
    layers: u32,
    batch: u32,
    max_weight_bytes: u64,
    facts: &'static ExynosFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExynosAcceleratorDiagnostic {
    BadSoc,
    LayersZero,
    LayersTooLarge,
    BatchZero,
    BatchTooLarge,
    WeightBytesZero,
    WeightBytesTooLarge,
    ExecutionsOverflow,
    UnsupportedTarget,
    UnsupportedAccelerator,
    MissingnpuDsp,
    MissingNeuralDelegate,
    MissingInt8Model,
    MissingUtf8Strings,
}

impl fmt::Display for ExynosAcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadSoc => "generation is not in the Exynos accelerator catalog",
            Self::LayersZero => "layer count must be positive",
            Self::LayersTooLarge => "layer count exceeds the supported bound",
            Self::BatchZero => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::WeightBytesZero => "weight bound must be positive",
            Self::WeightBytesTooLarge => "weight bound exceeds the supported bound",
            Self::ExecutionsOverflow => "total executions do not fit the supported bound",
            Self::UnsupportedTarget => {
                "Exynos accelerator supports only Exynos targets"
            }
            Self::UnsupportedAccelerator => {
                "generation has no documented NPU or NPU"
            }
            Self::MissingnpuDsp => "target runtime must report an NPU",
            Self::MissingNeuralDelegate => "target runtime must provide a neural delegate",
            Self::MissingInt8Model => "model graph must be Int8 quantized for delegation",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for ExynosAcceleratorDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a DSP ran
/// inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExynosAcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl ExynosAcceleratorDeclaration {
    pub fn normalize(self) -> Result<ExynosAcceleratorIr, ExynosAcceleratorDiagnostic> {
        let facts = facts_for(&self.soc).ok_or(ExynosAcceleratorDiagnostic::BadSoc)?;
        let plan = crate::exynos_runtime::NpuDelegatePlan::new(
            self.layers,
            self.batch,
            self.max_weight_bytes,
        )
        .map_err(|error| match error {
            crate::exynos_runtime::NpuError::ZeroLayers => {
                ExynosAcceleratorDiagnostic::LayersZero
            }
            crate::exynos_runtime::NpuError::LayersTooLarge => {
                ExynosAcceleratorDiagnostic::LayersTooLarge
            }
            crate::exynos_runtime::NpuError::ZeroBatch => {
                ExynosAcceleratorDiagnostic::BatchZero
            }
            crate::exynos_runtime::NpuError::BatchTooLarge => {
                ExynosAcceleratorDiagnostic::BatchTooLarge
            }
            crate::exynos_runtime::NpuError::ZeroWeightBytes => {
                ExynosAcceleratorDiagnostic::WeightBytesZero
            }
            crate::exynos_runtime::NpuError::WeightBytesTooLarge => {
                ExynosAcceleratorDiagnostic::WeightBytesTooLarge
            }
            crate::exynos_runtime::NpuError::LayerBatchOverflow => {
                ExynosAcceleratorDiagnostic::ExecutionsOverflow
            }
        })?;
        debug_assert_eq!(plan.layers(), self.layers);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.max_weight_bytes(), self.max_weight_bytes);
        Ok(ExynosAcceleratorIr {
            soc: self.soc,
            layers: self.layers,
            batch: self.batch,
            max_weight_bytes: self.max_weight_bytes,
            facts,
        })
    }
}

impl ExynosAcceleratorIr {
    pub fn soc(&self) -> &str {
        &self.soc
    }

    pub fn layers(&self) -> u32 {
        self.layers
    }

    pub fn batch(&self) -> u32 {
        self.batch
    }

    pub fn max_weight_bytes(&self) -> u64 {
        self.max_weight_bytes
    }

    pub fn total_executions(&self) -> u64 {
        u64::from(self.layers) * u64::from(self.batch)
    }

    pub fn facts(&self) -> &'static ExynosFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        let tops = self
            .facts
            .npu_tops_tenths
            .map(|tenths| format!("{}.{}", tenths / 10, tenths % 10))
            .unwrap_or_else(|| "unpublished".to_string());
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=exynos\nsoc={}\nmodel={}\nnpu_dsp={}\nnpu_tops={tops}\nint8_accel={}\nfp16_accel={}\nyear={}\nlayers={}\nbatch={}\ntotal_executions={}\nmax_weight_bytes={}\nexecution=none_attempted\n",
            self.soc,
            self.facts.soc,
            self.facts.npu_dsp,
            self.facts.int8_accel,
            self.facts.fp16_accel,
            self.facts.year,
            self.layers,
            self.batch,
            self.total_executions(),
            self.max_weight_bytes,
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<ExynosAcceleratorArtifact, ExynosAcceleratorDiagnostic> {
        if target != Target::Exynos {
            return Err(ExynosAcceleratorDiagnostic::UnsupportedTarget);
        }
        if !capabilities.npu_dsp {
            return Err(ExynosAcceleratorDiagnostic::MissingnpuDsp);
        }
        if !capabilities.neural_delegate {
            return Err(ExynosAcceleratorDiagnostic::MissingNeuralDelegate);
        }
        if !capabilities.utf8_strings {
            return Err(ExynosAcceleratorDiagnostic::MissingUtf8Strings);
        }
        if capabilities.int8_model && !self.facts.int8_accel {
            return Err(ExynosAcceleratorDiagnostic::UnsupportedAccelerator);
        }
        if !capabilities.int8_model {
            return Err(ExynosAcceleratorDiagnostic::MissingInt8Model);
        }
        Ok(ExynosAcceleratorArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; exynos accelerator, no inference is attempted.\npub fn make_delegate_plan() -> Result<crate::exynos_runtime::NpuDelegatePlan, crate::exynos_runtime::NpuError> {{\n    crate::exynos_runtime::NpuDelegatePlan::new({}u32, {}u32, {}u64)\n}}\n",
                self.layers, self.batch, self.max_weight_bytes,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES: RuntimeCapabilities = RuntimeCapabilities {
        npu_dsp: true,
        neural_delegate: true,
        int8_model: true,
        utf8_strings: true,
    };

    fn declaration(soc: &str, layers: u32, batch: u32) -> ExynosAcceleratorDeclaration {
        ExynosAcceleratorDeclaration {
            soc: soc.to_string(),
            layers,
            batch,
            max_weight_bytes: 4096,
        }
    }

    fn first_generation() -> &'static str {
        // Tests lower plans, so start from the first generation with a DSP
        // and documented Int8 acceleration.
        GENERATIONS
            .iter()
            .find(|(_, facts)| facts.npu_dsp != "none" && facts.int8_accel)
            .map(|(name, _)| *name)
            .expect("catalog must contain a DSP-capable generation")
    }

    #[test]
    fn catalog_entries_have_consistent_facts() {
        assert!(!GENERATIONS.is_empty(), "catalog must not be empty");
        let mut seen = std::collections::BTreeSet::new();
        for (name, facts) in GENERATIONS {
            assert!(seen.insert(*name), "duplicate generation {name}");
            assert!(matches!(facts.series, "8" | "7" | "6" | "4"));
            assert!(
                facts.npu_dsp == "none" || facts.npu_dsp.starts_with("NPU"),
                "unexpected DSP label {}",
                facts.npu_dsp
            );
            if facts.npu_dsp == "none" || !facts.int8_accel {
                assert!(!facts.int8_accel && !facts.fp16_accel);
            }
            assert!((2013..=2030).contains(&facts.year));
            assert!(!facts.soc.is_empty());
        }
        // Series blocks appear in 8, 7, 6, 4 order and never interleave.
        let series_order: Vec<&str> = GENERATIONS.iter().map(|(_, f)| f.series).collect();
        let mut compact: Vec<&str> = series_order.clone();
        compact.dedup();
        assert_eq!(compact.len(), 1, "exynos catalog is one series block");
        // Published TOPS figures, when present, are positive tenths.
        for (_, facts) in GENERATIONS {
            if let Some(tenths) = facts.npu_tops_tenths {
                assert!(tenths > 0, "published TOPS must be positive");
            }
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let soc = first_generation();
        let ir = declaration(soc, 32, 8).normalize().unwrap();
        assert_eq!(ir.layers(), 32);
        assert_eq!(ir.batch(), 8);
        assert_eq!(ir.total_executions(), 256);
        let first = ir.lower_checked(Target::Exynos, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::Exynos, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=exynos\n"));
        assert!(first.canonical_ir.contains("layers=32\n"));
        assert!(first.canonical_ir.contains("total_executions=256\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("NpuDelegatePlan::new(32u32, 8u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_generations_and_bad_bounds() {
        assert_eq!(
            declaration("Pineapple", 32, 8).normalize(),
            Err(ExynosAcceleratorDiagnostic::BadSoc)
        );
        let soc = first_generation();
        assert_eq!(
            declaration(soc, 0, 8).normalize(),
            Err(ExynosAcceleratorDiagnostic::LayersZero)
        );
        assert_eq!(
            declaration(soc, MAX_LAYERS + 1, 8).normalize(),
            Err(ExynosAcceleratorDiagnostic::LayersTooLarge)
        );
        assert_eq!(
            declaration(soc, 32, 0).normalize(),
            Err(ExynosAcceleratorDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(soc, 32, MAX_BATCH + 1).normalize(),
            Err(ExynosAcceleratorDiagnostic::BatchTooLarge)
        );
        let mut zero_weight = declaration(soc, 32, 8);
        zero_weight.max_weight_bytes = 0;
        assert_eq!(
            zero_weight.normalize(),
            Err(ExynosAcceleratorDiagnostic::WeightBytesZero)
        );
        let mut huge_weight = declaration(soc, 32, 8);
        huge_weight.max_weight_bytes = MAX_WEIGHT_BYTES + 1;
        assert_eq!(
            huge_weight.normalize(),
            Err(ExynosAcceleratorDiagnostic::WeightBytesTooLarge)
        );
        assert!(declaration(soc, MAX_LAYERS, MAX_BATCH).normalize().is_ok());
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let soc = first_generation();
        let ir = declaration(soc, 32, 8).normalize().unwrap();
        for target in [
            Target::CrossPlatform,
            Target::Browser,
            Target::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(ExynosAcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    npu_dsp: false,
                    ..CAPABILITIES
                },
                ExynosAcceleratorDiagnostic::MissingnpuDsp,
            ),
            (
                RuntimeCapabilities {
                    neural_delegate: false,
                    ..CAPABILITIES
                },
                ExynosAcceleratorDiagnostic::MissingNeuralDelegate,
            ),
            (
                RuntimeCapabilities {
                    int8_model: false,
                    ..CAPABILITIES
                },
                ExynosAcceleratorDiagnostic::MissingInt8Model,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                ExynosAcceleratorDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::Exynos, missing),
                Err(expected)
            );
        }
    }

    #[test]
    fn every_generation_with_a_dsp_lowers_and_rejects_non_int8_only_silicon() {
        let mut lowered = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 4, 2).normalize().unwrap();
            if facts.npu_dsp == "none" || !facts.int8_accel {
                assert_eq!(
                    ir.lower_checked(Target::Exynos, CAPABILITIES),
                    Err(ExynosAcceleratorDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::Exynos, CAPABILITIES).unwrap();
            lowered += 1;
        }
        assert!(lowered > 0, "catalog must lower at least one generation");
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_exynos_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let soc = first_generation();
        let artifact = declaration(soc, 128, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::Exynos, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/exynos_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod exynos_runtime;\n{}\nfn main() {{\n    let plan = make_delegate_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.layers(), 128);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_executions(), 384);\n    assert_eq!(plan.max_weight_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-exynos-factory-{}-{nonce}",
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
            .expect("run generated delegate factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
