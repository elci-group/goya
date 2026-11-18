//! A bounded Snapdragon SoC neural-acceleration capability.
//!
//! The negotiated artifact is a checked delegate plan and a factory binding
//! to this crate's plan runtime. It does not compile models, claim DSP
//! access, guarantee scheduling, or prove that inference ran. Hosts on
//! Snapdragon hardware report their real Hexagon facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.neural_accel.snapdragon.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_LAYERS: u32 = crate::snapdragon_runtime::MAX_LAYERS;
pub const MAX_BATCH: u32 = crate::snapdragon_runtime::MAX_BATCH;
pub const MAX_WEIGHT_BYTES: u64 = crate::snapdragon_runtime::MAX_WEIGHT_BYTES;

/// Published accelerator facts for one Snapdragon SoC generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoCFacts {
    /// Qualcomm model number (for example `SM8475`), or `unpublished`.
    pub soc: &'static str,
    /// Snapdragon series digit: `8`, `7`, `6` or `4`.
    pub series: &'static str,
    /// Documented Hexagon DSP/NPU generation, or `none`.
    pub hexagon_dsp: &'static str,
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

/// Catalog order is release order. Facts are published figures only;
/// `npu_tops` is `None` wherever Qualcomm published no figure.
pub const GENERATIONS: &[(&str, SoCFacts)] = &[];

/// Per-generation lookup. Unknown generations return None; the catalog never
/// guesses.
pub fn facts_for(soc: &str) -> Option<&'static SoCFacts> {
    GENERATIONS
        .iter()
        .find(|(name, _)| *name == soc)
        .map(|(_, facts)| facts)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// Snapdragon SoC hardware (handsets, tablets, XR).
    Snapdragon,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host reports a Hexagon DSP/NPU is present and addressable.
    pub hexagon_dsp: bool,
    /// Host provides a neural delegate (for example SNPE or QNN style).
    pub neural_delegate: bool,
    /// The model graph is Int8 quantized.
    pub int8_model: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapdragonAcceleratorDeclaration {
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
pub struct SnapdragonAcceleratorIr {
    soc: String,
    layers: u32,
    batch: u32,
    max_weight_bytes: u64,
    facts: &'static SoCFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapdragonAcceleratorDiagnostic {
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
    MissingHexagonDsp,
    MissingNeuralDelegate,
    MissingInt8Model,
    MissingUtf8Strings,
}

impl fmt::Display for SnapdragonAcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadSoc => "generation is not in the Snapdragon accelerator catalog",
            Self::LayersZero => "layer count must be positive",
            Self::LayersTooLarge => "layer count exceeds the supported bound",
            Self::BatchZero => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::WeightBytesZero => "weight bound must be positive",
            Self::WeightBytesTooLarge => "weight bound exceeds the supported bound",
            Self::ExecutionsOverflow => "total executions do not fit the supported bound",
            Self::UnsupportedTarget => {
                "Snapdragon accelerator supports only Snapdragon targets"
            }
            Self::UnsupportedAccelerator => {
                "generation has no documented Hexagon DSP or NPU"
            }
            Self::MissingHexagonDsp => "target runtime must report a Hexagon DSP",
            Self::MissingNeuralDelegate => "target runtime must provide a neural delegate",
            Self::MissingInt8Model => "model graph must be Int8 quantized for delegation",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for SnapdragonAcceleratorDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a DSP ran
/// inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapdragonAcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl SnapdragonAcceleratorDeclaration {
    pub fn normalize(self) -> Result<SnapdragonAcceleratorIr, SnapdragonAcceleratorDiagnostic> {
        let facts = facts_for(&self.soc).ok_or(SnapdragonAcceleratorDiagnostic::BadSoc)?;
        let plan = crate::snapdragon_runtime::HexagonDelegatePlan::new(
            self.layers,
            self.batch,
            self.max_weight_bytes,
        )
        .map_err(|error| match error {
            crate::snapdragon_runtime::HexagonError::ZeroLayers => {
                SnapdragonAcceleratorDiagnostic::LayersZero
            }
            crate::snapdragon_runtime::HexagonError::LayersTooLarge => {
                SnapdragonAcceleratorDiagnostic::LayersTooLarge
            }
            crate::snapdragon_runtime::HexagonError::ZeroBatch => {
                SnapdragonAcceleratorDiagnostic::BatchZero
            }
            crate::snapdragon_runtime::HexagonError::BatchTooLarge => {
                SnapdragonAcceleratorDiagnostic::BatchTooLarge
            }
            crate::snapdragon_runtime::HexagonError::ZeroWeightBytes => {
                SnapdragonAcceleratorDiagnostic::WeightBytesZero
            }
            crate::snapdragon_runtime::HexagonError::WeightBytesTooLarge => {
                SnapdragonAcceleratorDiagnostic::WeightBytesTooLarge
            }
            crate::snapdragon_runtime::HexagonError::LayerBatchOverflow => {
                SnapdragonAcceleratorDiagnostic::ExecutionsOverflow
            }
        })?;
        debug_assert_eq!(plan.layers(), self.layers);
        debug_assert_eq!(plan.batch(), self.batch);
        debug_assert_eq!(plan.max_weight_bytes(), self.max_weight_bytes);
        Ok(SnapdragonAcceleratorIr {
            soc: self.soc,
            layers: self.layers,
            batch: self.batch,
            max_weight_bytes: self.max_weight_bytes,
            facts,
        })
    }
}

impl SnapdragonAcceleratorIr {
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

    pub fn facts(&self) -> &'static SoCFacts {
        self.facts
    }

    fn canonical(&self) -> String {
        let tops = self
            .facts
            .npu_tops
            .map(|tops| tops.to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=snapdragon\nsoc={}\nmodel={}\nhexagon_dsp={}\nnpu_tops={tops}\nint8_accel={}\nfp16_accel={}\nyear={}\nlayers={}\nbatch={}\ntotal_executions={}\nmax_weight_bytes={}\nexecution=none_attempted\n",
            self.soc,
            self.facts.soc,
            self.facts.hexagon_dsp,
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
    ) -> Result<SnapdragonAcceleratorArtifact, SnapdragonAcceleratorDiagnostic> {
        if target != Target::Snapdragon {
            return Err(SnapdragonAcceleratorDiagnostic::UnsupportedTarget);
        }
        if !capabilities.hexagon_dsp {
            return Err(SnapdragonAcceleratorDiagnostic::MissingHexagonDsp);
        }
        if !capabilities.neural_delegate {
            return Err(SnapdragonAcceleratorDiagnostic::MissingNeuralDelegate);
        }
        if !capabilities.utf8_strings {
            return Err(SnapdragonAcceleratorDiagnostic::MissingUtf8Strings);
        }
        if capabilities.int8_model && !self.facts.int8_accel {
            return Err(SnapdragonAcceleratorDiagnostic::UnsupportedAccelerator);
        }
        if !capabilities.int8_model {
            return Err(SnapdragonAcceleratorDiagnostic::MissingInt8Model);
        }
        Ok(SnapdragonAcceleratorArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; snapdragon accelerator, no inference is attempted.\npub fn make_delegate_plan() -> Result<crate::snapdragon_runtime::HexagonDelegatePlan, crate::snapdragon_runtime::HexagonError> {{\n    crate::snapdragon_runtime::HexagonDelegatePlan::new({}u32, {}u32, {}u64)\n}}\n",
                self.layers, self.batch, self.max_weight_bytes,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES: RuntimeCapabilities = RuntimeCapabilities {
        hexagon_dsp: true,
        neural_delegate: true,
        int8_model: true,
        utf8_strings: true,
    };

    fn declaration(soc: &str, layers: u32, batch: u32) -> SnapdragonAcceleratorDeclaration {
        SnapdragonAcceleratorDeclaration {
            soc: soc.to_string(),
            layers,
            batch,
            max_weight_bytes: 4096,
        }
    }

    fn first_generation() -> &'static str {
        GENERATIONS[0].0
    }

    #[test]
    fn catalog_entries_have_consistent_facts() {
        assert!(!GENERATIONS.is_empty(), "catalog must not be empty");
        let mut seen = std::collections::BTreeSet::new();
        for (name, facts) in GENERATIONS {
            assert!(seen.insert(*name), "duplicate generation {name}");
            assert!(matches!(facts.series, "8" | "7" | "6" | "4"));
            assert!(
                facts.hexagon_dsp == "none" || facts.hexagon_dsp.starts_with("Hexagon"),
                "unexpected DSP label {}",
                facts.hexagon_dsp
            );
            if facts.hexagon_dsp == "none" {
                assert!(!facts.int8_accel && !facts.fp16_accel);
            }
            assert!((2013..=2030).contains(&facts.year));
            assert!(!facts.soc.is_empty());
        }
        // Release order must be non-decreasing in year.
        for window in GENERATIONS.windows(2) {
            assert!(window[0].1.year <= window[1].1.year);
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_deterministically() {
        let soc = first_generation();
        let ir = declaration(soc, 32, 8).normalize().unwrap();
        assert_eq!(ir.layers(), 32);
        assert_eq!(ir.batch(), 8);
        assert_eq!(ir.total_executions(), 256);
        let first = ir.lower_checked(Target::Snapdragon, CAPABILITIES).unwrap();
        let second = ir.lower_checked(Target::Snapdragon, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=snapdragon\n"));
        assert!(first.canonical_ir.contains("layers=32\n"));
        assert!(first.canonical_ir.contains("total_executions=256\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("HexagonDelegatePlan::new(32u32, 8u32, 4096u64)"));
    }

    #[test]
    fn rejects_unknown_generations_and_bad_bounds() {
        assert_eq!(
            declaration("Pineapple", 32, 8).normalize(),
            Err(SnapdragonAcceleratorDiagnostic::BadSoc)
        );
        let soc = first_generation();
        assert_eq!(
            declaration(soc, 0, 8).normalize(),
            Err(SnapdragonAcceleratorDiagnostic::LayersZero)
        );
        assert_eq!(
            declaration(soc, MAX_LAYERS + 1, 8).normalize(),
            Err(SnapdragonAcceleratorDiagnostic::LayersTooLarge)
        );
        assert_eq!(
            declaration(soc, 32, 0).normalize(),
            Err(SnapdragonAcceleratorDiagnostic::BatchZero)
        );
        assert_eq!(
            declaration(soc, 32, MAX_BATCH + 1).normalize(),
            Err(SnapdragonAcceleratorDiagnostic::BatchTooLarge)
        );
        let mut zero_weight = declaration(soc, 32, 8);
        zero_weight.max_weight_bytes = 0;
        assert_eq!(
            zero_weight.normalize(),
            Err(SnapdragonAcceleratorDiagnostic::WeightBytesZero)
        );
        let mut huge_weight = declaration(soc, 32, 8);
        huge_weight.max_weight_bytes = MAX_WEIGHT_BYTES + 1;
        assert_eq!(
            huge_weight.normalize(),
            Err(SnapdragonAcceleratorDiagnostic::WeightBytesTooLarge)
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
                Err(SnapdragonAcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    hexagon_dsp: false,
                    ..CAPABILITIES
                },
                SnapdragonAcceleratorDiagnostic::MissingHexagonDsp,
            ),
            (
                RuntimeCapabilities {
                    neural_delegate: false,
                    ..CAPABILITIES
                },
                SnapdragonAcceleratorDiagnostic::MissingNeuralDelegate,
            ),
            (
                RuntimeCapabilities {
                    int8_model: false,
                    ..CAPABILITIES
                },
                SnapdragonAcceleratorDiagnostic::MissingInt8Model,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..CAPABILITIES
                },
                SnapdragonAcceleratorDiagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::Snapdragon, missing),
                Err(expected)
            );
        }
    }

    #[test]
    fn every_generation_with_a_dsp_lowers_and_rejects_non_int8_only_silicon() {
        let mut lowered = 0;
        for (name, facts) in GENERATIONS {
            let ir = declaration(name, 4, 2).normalize().unwrap();
            if facts.hexagon_dsp == "none" {
                assert_eq!(
                    ir.lower_checked(Target::Snapdragon, CAPABILITIES),
                    Err(SnapdragonAcceleratorDiagnostic::UnsupportedAccelerator)
                );
                continue;
            }
            ir.lower_checked(Target::Snapdragon, CAPABILITIES).unwrap();
            lowered += 1;
        }
        assert!(lowered > 0, "catalog must lower at least one generation");
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_snapdragon_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let soc = first_generation();
        let artifact = declaration(soc, 128, 3)
            .normalize()
            .unwrap()
            .lower_checked(Target::Snapdragon, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/snapdragon_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod snapdragon_runtime;\n{}\nfn main() {{\n    let plan = make_delegate_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.layers(), 128);\n    assert_eq!(plan.batch(), 3);\n    assert_eq!(plan.total_executions(), 384);\n    assert_eq!(plan.max_weight_bytes(), 4096);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-snapdragon-factory-{}-{nonce}",
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
