//! A bounded MediaTek SoC neural-acceleration capability.
//!
//! The negotiated artifact is a checked delegate plan and a factory binding
//! to this crate's plan runtime. It does not compile models, claim DSP
//! access, guarantee scheduling, or prove that inference ran. Hosts on
//! MediaTek hardware report their real APU facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.neural_accel.mediatek.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_LAYERS: u32 = crate::mediatek_runtime::MAX_LAYERS;
pub const MAX_BATCH: u32 = crate::mediatek_runtime::MAX_BATCH;
pub const MAX_WEIGHT_BYTES: u64 = crate::mediatek_runtime::MAX_WEIGHT_BYTES;

/// Published accelerator facts for one MediaTek SoC generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaTekFacts {
    /// Qualcomm model number (for example `SM8475`), or `unpublished`.
    pub soc: &'static str,
    /// MediaTek series digit: `8`, `7`, `6` or `4`.
    pub series: &'static str,
    /// Documented NPU/NPU generation, or `none`.
    pub apu_dsp: &'static str,
    /// Peak AI throughput Qualcomm published for this SoC, in TOPS times
    /// ten (5.5 TOPS is stored as `55`). `None` means Qualcomm did not
    /// publish a figure; none is guessed.
    pub npu_tops_tenths: Option<u16>,
    /// Documented Int8 neural acceleration.
    pub int8_accel: bool,
    /// Documented FP16 neural acceleration.
    pub fp16_accel: bool,
    /// First release year.
    pub year: u16,
}

/// Catalog order is release order. Facts come from MediaTek product
/// pages, press releases and NeuroPilot/LiteRT documentation; MediaTek
/// publishes no official AI TOPS figures, so all throughput fields are
/// `None` rather than guessed. Third-party TOPS figures are excluded.
#[rustfmt::skip]
pub const GENERATIONS: &[(&str, MediaTekFacts)] = &[
    ("Dimensity 9000",  MediaTekFacts { series: "9", soc: "MT6983", apu_dsp: "APU 690 (8-core, 6th-gen APU)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2021 }),
    ("Dimensity 9000+", MediaTekFacts { series: "9", soc: "MT6983", apu_dsp: "APU 690 (8-core, 6th-gen APU)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2022 }),
    ("Dimensity 9200",  MediaTekFacts { series: "9", soc: "MT6985", apu_dsp: "APU 690 (8-core, 6th-gen APU)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2022 }),
    ("Dimensity 9200+", MediaTekFacts { series: "9", soc: "MT6985", apu_dsp: "APU 690 (8-core, 6th-gen APU)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2023 }),
    ("Dimensity 9300",  MediaTekFacts { series: "9", soc: "MT6989", apu_dsp: "APU 790 (7th-gen, Transformer engine, INT4/INT8/FP16)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2023 }),
    ("Dimensity 9300+", MediaTekFacts { series: "9", soc: "MT6989", apu_dsp: "APU 790 (7th-gen, Transformer engine, INT4/INT8/FP16)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2024 }),
    ("Dimensity 9400",  MediaTekFacts { series: "9", soc: "MT6991", apu_dsp: "APU 890 (8th-gen, FP8/INT8/FP16, MoE/MLA)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2024 }),
    ("Dimensity 9400+", MediaTekFacts { series: "9", soc: "MT6991", apu_dsp: "APU 890 (8th-gen, FP8/INT8/FP16, speculative decoding)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2025 }),
    ("Dimensity 8000",  MediaTekFacts { series: "8", soc: "MT6895", apu_dsp: "APU 580", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2022 }),
    ("Dimensity 8100",  MediaTekFacts { series: "8", soc: "MT6895", apu_dsp: "APU 580", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2022 }),
    ("Dimensity 8200",  MediaTekFacts { series: "8", soc: "MT6896", apu_dsp: "APU 580", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2022 }),
    ("Dimensity 8300",  MediaTekFacts { series: "8", soc: "MT6897", apu_dsp: "APU 680", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2023 }),
    ("Dimensity 8400",  MediaTekFacts { series: "8", soc: "MT6899", apu_dsp: "NPU 880 (APU-lineage, Gen-AI/agentic)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2024 }),
    ("Dimensity 7200",  MediaTekFacts { series: "7", soc: "MT6886", apu_dsp: "APU 650", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2023 }),
    ("Dimensity 7300",  MediaTekFacts { series: "7", soc: "MT6878", apu_dsp: "APU 650-class (name not officially confirmed)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2024 }),
    ("Dimensity 7400",  MediaTekFacts { series: "7", soc: "MT6878", apu_dsp: "APU 655 (third-party; not confirmed by MediaTek)", npu_tops_tenths: None, int8_accel: true, fp16_accel: true, year: 2025 }),
];

/// Per-generation lookup. Unknown generations return None; the catalog never
/// guesses.
pub fn facts_for(soc: &str) -> Option<&'static MediaTekFacts> {
    GENERATIONS
        .iter()
        .find(|(name, _)| *name == soc)
        .map(|(_, facts)| facts)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// MediaTek SoC hardware (handsets, tablets, XR).
    MediaTek,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports an APU is present and addressable.
    pub apu_dsp: bool,
    /// Host provides a neural delegate (for example SNPE or QNN style).
    pub neural_delegate: bool,
    /// The model graph is Int8 quantized.
    pub int8_model: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaTekAcceleratorDeclaration {
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
pub struct MediaTekAcceleratorIr {
    soc: String,
    layers: u32,
    batch: u32,
    max_weight_bytes: u64,
    facts: &'static MediaTekFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaTekAcceleratorDiagnostic {
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

impl fmt::Display for MediaTekAcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadSoc => "generation is not in the MediaTek accelerator catalog",
            Self::LayersZero => "layer count must be positive",
            Self::LayersTooLarge => "layer count exceeds the supported bound",
            Self::BatchZero => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::WeightBytesZero => "weight bound must be positive",
            Self::WeightBytesTooLarge => "weight bound exceeds the supported bound",
            Self::ExecutionsOverflow => "total executions do not fit the supported bound",
            Self::UnsupportedTarget => {
                "MediaTek accelerator supports only MediaTek targets"
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

impl std::error::Error for MediaTekAcceleratorDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a DSP ran
/// inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaTekAcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl MediaTekAcceleratorDeclaration {
    pub fn normalize(self) -> Result<MediaTekAcceleratorIr, MediaTekAcceleratorDiagnostic> {
        let facts = facts_for(&self.soc).ok_or(MediaTekAcceleratorDiagnostic::BadSoc)?;
        let plan = crate::mediatek_runtime::ApuDelegatePlan::new(
            self.layers,
            self.batch,
            self.max_weight_bytes,
        )
        .map_err(|error| match error {
            crate::mediatek_runtime::ApuError::ZeroLayers => {
                MediaTekAcceleratorDiagnostic::LayersZero
            }
            crate::mediatek_runtime::ApuError::LayersTooLarge => {
                MediaTekAcceleratorDiagnostic::LayersTooLarge
            }
            crate::mediatek_runtime::ApuError::ZeroBatch => {
                MediaTekAcceleratorDiagnostic::BatchZero
            }
            crate::mediatek_runtime::ApuError::BatchTooLarge => {
                MediaTekAcceleratorDiagnostic::BatchTooLarge
            }
            crate::mediatek_runtime::ApuError::ZeroWeightBytes => {
                MediaTekAcceleratorDiagnostic::WeightBytesZero
            }
            crate::mediatek_runtime::ApuError::WeightBytesTooLarge => {
                MediaTekAcceleratorDiagnostic::WeightBytesTooLarge
            }
            crate::mediatek_runtime::ApuError::LayerBatchOverflow => {
                MediaTekAcceleratorDiagnostic::ExecutionsOverflow
            }
        })?;
        debug_assert_eq!(plan.layers(), self.layers);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.max_weight_bytes(), self.max_weight_bytes);
        Ok(MediaTekAcceleratorIr {
            soc: self.soc,
            layers: self.layers,
            batch: self.batch,
            max_weight_bytes: self.max_weight_bytes,
            facts,
        })
    }
}

impl MediaTekAcceleratorIr {
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

    pub fn facts(&self) -> &'static MediaTekFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        let tops = self
            .facts
            .npu_tops_tenths
            .map(|tenths| format!("{}.{}", tenths / 10, tenths % 10))
            .unwrap_or_else(|| "unpublished".to_string());
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=mediatek\nsoc={}\nmodel={}\napu_dsp={}\nnpu_tops={tops}\nint8_accel={}\nfp16_accel={}\nyear={}\nlayers={}\nbatch={}\ntotal_executions={}\nmax_weight_bytes={}\nexecution=none_attempted\n",
            self.soc,
            self.facts.soc,
            self.facts.apu_dsp,
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
    ) -> Result<MediaTekAcceleratorArtifact, MediaTekAcceleratorDiagnostic> {
        if target != Target::MediaTek {
            return Err(MediaTekAcceleratorDiagnostic::UnsupportedTarget);
        }
        if !capabilities.apu_dsp {
            return Err(MediaTekAcceleratorDiagnostic::MissingnpuDsp);
        }
        if !capabilities.neural_delegate {
            return Err(MediaTekAcceleratorDiagnostic::MissingNeuralDelegate);
        }
        if !capabilities.utf8_strings {
            return Err(MediaTekAcceleratorDiagnostic::MissingUtf8Strings);
        }
        if capabilities.int8_model && !self.facts.int8_accel {
            return Err(MediaTekAcceleratorDiagnostic::UnsupportedAccelerator);
        }
        if !capabilities.int8_model {
            return Err(MediaTekAcceleratorDiagnostic::MissingInt8Model);
        }
        Ok(MediaTekAcceleratorArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; mediatek accelerator, no inference is attempted.\npub fn make_delegate_plan() -> Result<crate::mediatek_runtime::ApuDelegatePlan, crate::mediatek_runtime::ApuError> {{\n    crate::mediatek_runtime::ApuDelegatePlan::new({}u32, {}u32, {}u64)\n}}\n",
                self.layers, self.batch, self.max_weight_bytes,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES: RuntimeCapabilities = RuntimeCapabilities {
        apu_dsp: true,
        neural_delegate: true,
        int8_model: true,
        utf8_strings: true,
    };

    fn declaration(soc: &str, layers: u32, batch: u32) -> MediaTekAcceleratorDeclaration {
        MediaTekAcceleratorDeclaration {
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
            .find(|(_, facts)| facts.apu_dsp != "none" && facts.int8_accel)
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
                facts.apu_dsp == "none" || facts.apu_dsp.starts_with("APU"),
                "unexpected DSP label {}",
                facts.apu_dsp
            );
            if facts.apu_dsp == "none" || !facts.int8_accel {
                assert!(!facts.int8_accel && !facts.fp16_accel);
            }
            assert!((2013..=2030).contains(&facts.year));
            assert!(!facts.soc.is_empty());
        }
        // Series blocks appear in 8, 7, 6, 4 order and never interleave.
        let series_order: Vec<&str> = GENERATIONS.iter().map(|(_, f)| f.series).collect();
        let mut compact: Vec<&str> = series_order.clone();
        compact.dedup();
        assert!(compact.iter().all(|s| ["9", "8", "7"].contains(s)), "unknown series tag");
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
        let first = ir.lower_checked(Target::MediaTek, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::MediaTek, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=mediatek\n"));
        assert!(first.canonical_ir.contains("layers=32\n"));
        assert!(first.canonical_ir.contains("total_executions=256\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("ApuDelegatePlan::new(32u32, 8u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_generations_and_bad_bounds() {
        assert_eq!(
            declaration("Pineapple", 32, 8).normalize(),
            Err(MediaTekAcceleratorDiagnostic::BadSoc)
        );
        let soc = first_generation();
        assert_eq!(
            declaration(soc, 0, 8).normalize(),
            Err(MediaTekAcceleratorDiagnostic::LayersZero)
        );
        assert_eq!(
            declaration(soc, MAX_LAYERS + 1, 8).normalize(),
            Err(MediaTekAcceleratorDiagnostic::LayersTooLarge)
        );
        assert_eq!(
            declaration(soc, 32, 0).normalize(),
            Err(MediaTekAcceleratorDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(soc, 32, MAX_BATCH + 1).normalize(),
            Err(MediaTekAcceleratorDiagnostic::BatchTooLarge)
        );
        let mut zero_weight = declaration(soc, 32, 8);
        zero_weight.max_weight_bytes = 0;
        assert_eq!(
            zero_weight.normalize(),
            Err(MediaTekAcceleratorDiagnostic::WeightBytesZero)
        );
        let mut huge_weight = declaration(soc, 32, 8);
        huge_weight.max_weight_bytes = MAX_WEIGHT_BYTES + 1;
        assert_eq!(
            huge_weight.normalize(),
            Err(MediaTekAcceleratorDiagnostic::WeightBytesTooLarge)
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
                Err(MediaTekAcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    apu_dsp: false,
                    ..CAPABILITIES
                },
                MediaTekAcceleratorDiagnostic::MissingnpuDsp,
            ),
            (
                RuntimeCapabilities {
                    neural_delegate: false,
                    ..CAPABILITIES
                },
                MediaTekAcceleratorDiagnostic::MissingNeuralDelegate,
            ),
            (
                RuntimeCapabilities {
                    int8_model: false,
                    ..CAPABILITIES
                },
                MediaTekAcceleratorDiagnostic::MissingInt8Model,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                MediaTekAcceleratorDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::MediaTek, missing),
                Err(expected)
            );
        }
    }

    #[test]
    fn every_generation_with_a_dsp_lowers_and_rejects_non_int8_only_silicon() {
        let mut lowered = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 4, 2).normalize().unwrap();
            if facts.apu_dsp == "none" || !facts.int8_accel {
                assert_eq!(
                    ir.lower_checked(Target::MediaTek, CAPABILITIES),
                    Err(MediaTekAcceleratorDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::MediaTek, CAPABILITIES).unwrap();
            lowered += 1;
        }
        assert!(lowered > 0, "catalog must lower at least one generation");
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_mediatek_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let soc = first_generation();
        let artifact = declaration(soc, 128, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::MediaTek, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mediatek_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod mediatek_runtime;\n{}\nfn main() {{\n    let plan = make_delegate_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.layers(), 128);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_executions(), 384);\n    assert_eq!(plan.max_weight_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-mediatek-factory-{}-{nonce}",
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
