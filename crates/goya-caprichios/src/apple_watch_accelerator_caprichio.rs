//! A bounded Apple Watch device-accelerator capability.
//!
//! The negotiated artifact is an inventory binding to this crate's
//! accelerator runtime. It does not claim GPU execution, driver access, or
//! prove that a shader was compiled. Hosts report their real facilities
//! through [`AppleWatchAcceleratorRuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.accelerator.apple_watch.v1";
pub const RECIPE_VERSION: &str = "0.0.1";

/// Published accelerator facts for one device model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceAccelerator {
    pub chip: &'static str,
    pub gpu_cores: Option<u16>,
    pub metal_family: Option<u8>,
}

/// Catalog order is release order. watchOS exposes no Metal, so `gpu_cores`
/// and `metal_family` are unpublished (`None`) for every S-series SiP.
pub const MODELS: &[(&str, DeviceAccelerator)] = &[
    (
        "Apple Watch (1st generation)",
        DeviceAccelerator {
            chip: "S1",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 1",
        DeviceAccelerator {
            chip: "S1P",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 2",
        DeviceAccelerator {
            chip: "S2",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 3",
        DeviceAccelerator {
            chip: "S3",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 4",
        DeviceAccelerator {
            chip: "S4",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 5",
        DeviceAccelerator {
            chip: "S5",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch SE",
        DeviceAccelerator {
            chip: "S5",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 6",
        DeviceAccelerator {
            chip: "S6",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 7",
        DeviceAccelerator {
            chip: "S7",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 8",
        DeviceAccelerator {
            chip: "S8",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Ultra",
        DeviceAccelerator {
            chip: "S8",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch SE (2nd generation)",
        DeviceAccelerator {
            chip: "S8",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 9",
        DeviceAccelerator {
            chip: "S9",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Ultra 2",
        DeviceAccelerator {
            chip: "S9",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "Apple Watch Series 10",
        DeviceAccelerator {
            chip: "S10",
            gpu_cores: None,
            metal_family: None,
        },
    ),
];

/// Per-model lookup. Unknown models return None; the registry never guesses.
pub fn accelerator_for(model: &str) -> Option<&'static DeviceAccelerator> {
    MODELS
        .iter()
        .find(|(name, _)| *name == model)
        .map(|(_, accelerator)| accelerator)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppleWatchAcceleratorTarget {
    /// The watch itself (device or simulator build).
    Device,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppleWatchAcceleratorRuntimeCapabilities {
    pub utf8_strings: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppleWatchAcceleratorDiagnostic {
    BadModel,
    UnsupportedTarget,
    MissingUtf8Strings,
}

impl fmt::Display for AppleWatchAcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadModel => "model is not in the Apple Watch accelerator catalog",
            Self::UnsupportedTarget => "the accelerator inventory supports only the device target",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for AppleWatchAcceleratorDiagnostic {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppleWatchAcceleratorDeclaration {
    pub model: String,
}

/// Negotiated accelerator inventory design and source text. The artifact is a
/// profile binding and a factory definition; successful construction is not
/// proof that a GPU ran anything.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppleWatchAcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppleWatchAcceleratorIr {
    model: String,
    accelerator: &'static DeviceAccelerator,
}

impl AppleWatchAcceleratorDeclaration {
    pub fn normalize(self) -> Result<AppleWatchAcceleratorIr, AppleWatchAcceleratorDiagnostic> {
        let accelerator =
            accelerator_for(&self.model).ok_or(AppleWatchAcceleratorDiagnostic::BadModel)?;
        Ok(AppleWatchAcceleratorIr {
            model: self.model,
            accelerator,
        })
    }
}

impl AppleWatchAcceleratorIr {
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn accelerator(&self) -> &'static DeviceAccelerator {
        self.accelerator
    }

    pub fn canonical(&self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=device\ncategory=apple_watch\nmodel={}\nchip={}\ngpu_cores=unpublished\nmetal_family=none\nexecution=none_attempted\n",
            self.model, self.accelerator.chip
        )
    }

    pub fn lower_checked(
        &self,
        target: AppleWatchAcceleratorTarget,
        capabilities: AppleWatchAcceleratorRuntimeCapabilities,
    ) -> Result<AppleWatchAcceleratorArtifact, AppleWatchAcceleratorDiagnostic> {
        if target != AppleWatchAcceleratorTarget::Device {
            return Err(AppleWatchAcceleratorDiagnostic::UnsupportedTarget);
        }
        if !capabilities.utf8_strings {
            return Err(AppleWatchAcceleratorDiagnostic::MissingUtf8Strings);
        }
        Ok(AppleWatchAcceleratorArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; Apple Watch accelerator inventory, watchOS exposes no Metal GPU.\npub fn make_accelerator_profile() -> Result<crate::apple_runtime::AcceleratorProfile, crate::apple_runtime::AcceleratorError> {{\n    crate::apple_runtime::AcceleratorProfile::new({:?}, None, None)\n}}\n",
                self.accelerator.chip
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATCH: AppleWatchAcceleratorRuntimeCapabilities =
        AppleWatchAcceleratorRuntimeCapabilities {
            utf8_strings: true,
        };

    fn declaration(model: &str) -> AppleWatchAcceleratorDeclaration {
        AppleWatchAcceleratorDeclaration {
            model: model.to_string(),
        }
    }

    #[test]
    fn catalog_is_complete_and_coherent() {
        let mut seen = std::collections::HashSet::new();
        for (name, accelerator) in MODELS {
            assert!(!name.is_empty(), "model name must be non-empty");
            assert!(seen.insert(*name), "duplicate model name: {name}");
            assert!(!accelerator.chip.is_empty(), "chip must be non-empty");
            if let Some(family) = accelerator.metal_family {
                assert!((1..=9).contains(&family), "metal_family out of range");
            }
        }
    }

    #[test]
    fn accelerator_for_resolves_every_model_and_rejects_unknown() {
        for (name, _) in MODELS {
            assert!(
                accelerator_for(name).is_some(),
                "catalog name must resolve: {name}"
            );
        }
        assert!(accelerator_for("unknown device").is_none());
        // Exact match only: no case folding, no trimming.
        let upper = MODELS[0].0.to_uppercase();
        assert!(accelerator_for(&upper).is_none());
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_artifact() {
        let (name, accelerator) = MODELS[0];
        let ir = declaration(name).normalize().unwrap();
        assert_eq!(ir.model(), name);
        assert_eq!(ir.accelerator(), accelerator);
        let first = ir
            .lower_checked(AppleWatchAcceleratorTarget::Device, WATCH)
            .unwrap();
        let second = ir
            .lower_checked(AppleWatchAcceleratorTarget::Device, WATCH)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert_eq!(first.recipe_version, RECIPE_VERSION);
        assert!(first.canonical_ir.contains("target=device\n"));
        assert!(first.canonical_ir.contains("category=apple_watch\n"));
        assert!(first
            .canonical_ir
            .contains(&format!("model={name}\n")));
        assert!(first
            .canonical_ir
            .contains(&format!("chip={}\n", accelerator.chip)));
        assert!(first.canonical_ir.contains("gpu_cores=unpublished\n"));
        assert!(first.canonical_ir.contains("metal_family=none\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .source
            .contains("pub fn make_accelerator_profile()"));
        assert!(first
            .source
            .contains(&format!("AcceleratorProfile::new({:?}, None, None)", accelerator.chip)));
    }

    #[test]
    fn catalog_covers_release_history() {
        for (name, _) in MODELS {
            let artifact = declaration(name)
                .normalize()
                .unwrap_or_else(|_| panic!("catalog model must normalize: {name}"))
                .lower_checked(AppleWatchAcceleratorTarget::Device, WATCH)
                .unwrap_or_else(|_| panic!("catalog model must lower: {name}"));
            assert_eq!(artifact.recipe_version, RECIPE_VERSION);
            assert!(artifact.canonical_ir.contains("category=apple_watch\n"));
            assert!(artifact.canonical_ir.contains("execution=none_attempted\n"));
        }
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration("Apple Watch Series 9").normalize().unwrap();
        for target in [
            AppleWatchAcceleratorTarget::CrossPlatform,
            AppleWatchAcceleratorTarget::Browser,
            AppleWatchAcceleratorTarget::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, WATCH),
                Err(AppleWatchAcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                AppleWatchAcceleratorTarget::Device,
                AppleWatchAcceleratorRuntimeCapabilities { utf8_strings: false }
            ),
            Err(AppleWatchAcceleratorDiagnostic::MissingUtf8Strings)
        );
    }

    #[test]
    fn rejects_unknown_model_with_bad_model() {
        assert_eq!(
            declaration("Apple Watch Series 11").normalize(),
            Err(AppleWatchAcceleratorDiagnostic::BadModel)
        );
        assert_eq!(
            declaration("").normalize(),
            Err(AppleWatchAcceleratorDiagnostic::BadModel)
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_apple_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration("Apple Watch Series 9")
            .normalize()
            .unwrap()
            .lower_checked(AppleWatchAcceleratorTarget::Device, WATCH)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let profile = make_accelerator_profile().expect(\"factory should construct profile\");\n    assert_eq!(profile.chip(), \"S9\");\n    assert_eq!(profile.gpu_cores(), None);\n    assert_eq!(profile.metal_family(), None);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-apple-watch-accelerator-factory-{}-{nonce}",
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
            .expect("run generated accelerator factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
