//! A bounded iPad device-accelerator capability.
//!
//! The negotiated artifact is a checked dispatch plan and an accelerator
//! profile bound to this crate's plan runtime. It does not claim GPU
//! execution, driver access, or prove that a shader was compiled. Hosts
//! report their real facilities through [`IpadAcceleratorRuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.accelerator.ipad.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_THREADS_PER_GROUP: u32 = crate::apple_runtime::MAX_THREADS_PER_GROUP;
pub const MAX_GROUPS: u32 = crate::apple_runtime::MAX_GROUPS;
pub const MAX_BUFFER_BYTES: u64 = crate::apple_runtime::MAX_BUFFER_BYTES;

/// Published accelerator facts for one device model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceAccelerator {
    pub chip: &'static str,
    pub gpu_cores: Option<u16>,
    pub metal_family: Option<u8>,
}

/// Catalog order is release order. `metal_family` is the documented
/// MTLGPUFamilyAppleN level, conservatively rounded; None = no Metal support.
pub const MODELS: &[(&str, DeviceAccelerator)] = &[
    ("iPad", DeviceAccelerator { chip: "A4", gpu_cores: None, metal_family: None }),
    ("iPad 2", DeviceAccelerator { chip: "A5", gpu_cores: Some(2), metal_family: None }),
    ("iPad (3rd generation)", DeviceAccelerator { chip: "A5X", gpu_cores: Some(4), metal_family: None }),
    ("iPad (4th generation)", DeviceAccelerator { chip: "A6X", gpu_cores: Some(4), metal_family: None }),
    ("iPad Air", DeviceAccelerator { chip: "A7", gpu_cores: Some(4), metal_family: Some(1) }),
    ("iPad Air 2", DeviceAccelerator { chip: "A8X", gpu_cores: Some(8), metal_family: Some(2) }),
    ("iPad mini", DeviceAccelerator { chip: "A5", gpu_cores: Some(2), metal_family: None }),
    ("iPad mini 2", DeviceAccelerator { chip: "A7", gpu_cores: Some(4), metal_family: Some(1) }),
    ("iPad mini 3", DeviceAccelerator { chip: "A7", gpu_cores: Some(4), metal_family: Some(1) }),
    ("iPad mini 4", DeviceAccelerator { chip: "A8X", gpu_cores: Some(8), metal_family: Some(2) }),
    ("iPad Pro 9.7-inch", DeviceAccelerator { chip: "A9X", gpu_cores: Some(12), metal_family: Some(3) }),
    ("iPad Pro 12.9-inch (1st generation)", DeviceAccelerator { chip: "A9X", gpu_cores: Some(12), metal_family: Some(3) }),
    ("iPad Pro 10.5-inch", DeviceAccelerator { chip: "A10X", gpu_cores: Some(12), metal_family: Some(4) }),
    ("iPad Pro 12.9-inch (2nd generation)", DeviceAccelerator { chip: "A10X", gpu_cores: Some(12), metal_family: Some(4) }),
    ("iPad (5th generation)", DeviceAccelerator { chip: "A9", gpu_cores: Some(6), metal_family: Some(3) }),
    ("iPad (6th generation)", DeviceAccelerator { chip: "A10 Fusion", gpu_cores: Some(6), metal_family: Some(4) }),
    ("iPad (7th generation)", DeviceAccelerator { chip: "A10 Fusion", gpu_cores: Some(6), metal_family: Some(4) }),
    ("iPad Air (3rd generation)", DeviceAccelerator { chip: "A12 Bionic", gpu_cores: Some(4), metal_family: Some(6) }),
    ("iPad (8th generation)", DeviceAccelerator { chip: "A12 Bionic", gpu_cores: Some(4), metal_family: Some(6) }),
    ("iPad (9th generation)", DeviceAccelerator { chip: "A13 Bionic", gpu_cores: Some(4), metal_family: Some(7) }),
    ("iPad (10th generation)", DeviceAccelerator { chip: "A14 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPad (11th generation)", DeviceAccelerator { chip: "A16", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPad Air (4th generation)", DeviceAccelerator { chip: "A14 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPad Air (5th generation)", DeviceAccelerator { chip: "M1", gpu_cores: Some(8), metal_family: Some(7) }),
    ("iPad Air 11-inch (M2)", DeviceAccelerator { chip: "M2", gpu_cores: Some(9), metal_family: Some(8) }),
    ("iPad Air 13-inch (M2)", DeviceAccelerator { chip: "M2", gpu_cores: Some(9), metal_family: Some(8) }),
    ("iPad Air 11-inch (M3)", DeviceAccelerator { chip: "M3", gpu_cores: Some(9), metal_family: Some(9) }),
    ("iPad Air 13-inch (M3)", DeviceAccelerator { chip: "M3", gpu_cores: Some(9), metal_family: Some(9) }),
    ("iPad mini (5th generation)", DeviceAccelerator { chip: "A12 Bionic", gpu_cores: Some(4), metal_family: Some(6) }),
    ("iPad mini (6th generation)", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPad mini (7th generation)", DeviceAccelerator { chip: "A17 Pro", gpu_cores: Some(6), metal_family: Some(9) }),
    ("iPad Pro 11-inch (1st generation)", DeviceAccelerator { chip: "A12X Bionic", gpu_cores: Some(7), metal_family: Some(6) }),
    ("iPad Pro 12.9-inch (3rd generation)", DeviceAccelerator { chip: "A12X Bionic", gpu_cores: Some(7), metal_family: Some(6) }),
    ("iPad Pro 11-inch (2nd generation)", DeviceAccelerator { chip: "A12Z Bionic", gpu_cores: Some(8), metal_family: Some(6) }),
    ("iPad Pro 12.9-inch (4th generation)", DeviceAccelerator { chip: "A12Z Bionic", gpu_cores: Some(8), metal_family: Some(6) }),
    ("iPad Pro 11-inch (3rd generation)", DeviceAccelerator { chip: "M1", gpu_cores: Some(8), metal_family: Some(7) }),
    ("iPad Pro 12.9-inch (5th generation)", DeviceAccelerator { chip: "M1", gpu_cores: Some(8), metal_family: Some(7) }),
    ("iPad Pro 11-inch (4th generation)", DeviceAccelerator { chip: "M2", gpu_cores: Some(10), metal_family: Some(8) }),
    ("iPad Pro 12.9-inch (6th generation)", DeviceAccelerator { chip: "M2", gpu_cores: Some(10), metal_family: Some(8) }),
    ("iPad Pro 11-inch (M4)", DeviceAccelerator { chip: "M4", gpu_cores: Some(9), metal_family: Some(9) }),
    ("iPad Pro 13-inch (M4)", DeviceAccelerator { chip: "M4", gpu_cores: Some(10), metal_family: Some(9) }),
];

/// Per-model lookup. Unknown models return None; the registry never guesses.
pub fn accelerator_for(model: &str) -> Option<&'static DeviceAccelerator> {
    MODELS
        .iter()
        .find(|(name, _)| *name == model)
        .map(|(_, accelerator)| accelerator)
}

/// The lowering target for an iPad accelerator artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpadAcceleratorTarget {
    /// The device itself (device or simulator build of this category).
    Device,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IpadAcceleratorRuntimeCapabilities {
    pub gpu_driver: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpadAcceleratorDeclaration {
    pub model: String,
    pub threads_per_group: u32,
    pub groups: u32,
    pub max_buffer_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpadAcceleratorIr {
    model: String,
    threads_per_group: u32,
    groups: u32,
    max_buffer_bytes: u64,
    accelerator: &'static DeviceAccelerator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpadAcceleratorDiagnostic {
    BadModel,
    ThreadsPerGroupZero,
    ThreadsPerGroupNotPowerOfTwo,
    ThreadsPerGroupTooLarge,
    GroupsZero,
    GroupsTooLarge,
    BufferBytesZero,
    BufferBytesTooLarge,
    ThreadCountOverflow,
    UnsupportedTarget,
    UnsupportedAccelerator,
    MissingGpuDriver,
    MissingUtf8Strings,
}

impl fmt::Display for IpadAcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadModel => "model is not in the iPad accelerator catalog",
            Self::ThreadsPerGroupZero => "threads per threadgroup must be positive",
            Self::ThreadsPerGroupNotPowerOfTwo => {
                "threads per threadgroup must be a power of two"
            }
            Self::ThreadsPerGroupTooLarge => {
                "threads per threadgroup exceeds the Metal limit of 1024"
            }
            Self::GroupsZero => "threadgroup count must be positive",
            Self::GroupsTooLarge => "threadgroup count exceeds the supported bound",
            Self::BufferBytesZero => "buffer bound must be positive",
            Self::BufferBytesTooLarge => "buffer bound exceeds the supported bound",
            Self::ThreadCountOverflow => "total thread count does not fit a u32 dispatch",
            Self::UnsupportedTarget => "iPad accelerator supports only the device target",
            Self::UnsupportedAccelerator => {
                "this iPad model publishes no Metal-capable accelerator"
            }
            Self::MissingGpuDriver => "target runtime must report a GPU driver",
            Self::MissingUtf8Strings => "target runtime must support UTF-8 strings",
        })
    }
}

impl std::error::Error for IpadAcceleratorDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a GPU ran
/// the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpadAcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl IpadAcceleratorDeclaration {
    pub fn normalize(self) -> Result<IpadAcceleratorIr, IpadAcceleratorDiagnostic> {
        let accelerator =
            accelerator_for(&self.model).ok_or(IpadAcceleratorDiagnostic::BadModel)?;
        let plan = crate::apple_runtime::MetalDispatchPlan::new(
            self.threads_per_group,
            self.groups,
            self.max_buffer_bytes,
        )
        .map_err(|error| match error {
            crate::apple_runtime::MetalError::ZeroThreadsPerGroup => {
                IpadAcceleratorDiagnostic::ThreadsPerGroupZero
            }
            crate::apple_runtime::MetalError::ThreadsPerGroupNotPowerOfTwo => {
                IpadAcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo
            }
            crate::apple_runtime::MetalError::ThreadsPerGroupTooLarge => {
                IpadAcceleratorDiagnostic::ThreadsPerGroupTooLarge
            }
            crate::apple_runtime::MetalError::ZeroGroups => {
                IpadAcceleratorDiagnostic::GroupsZero
            }
            crate::apple_runtime::MetalError::GroupsTooLarge => {
                IpadAcceleratorDiagnostic::GroupsTooLarge
            }
            crate::apple_runtime::MetalError::ZeroBufferBytes => {
                IpadAcceleratorDiagnostic::BufferBytesZero
            }
            crate::apple_runtime::MetalError::BufferTooLarge => {
                IpadAcceleratorDiagnostic::BufferBytesTooLarge
            }
            crate::apple_runtime::MetalError::ThreadCountOverflow => {
                IpadAcceleratorDiagnostic::ThreadCountOverflow
            }
        })?;
        Ok(IpadAcceleratorIr {
            model: self.model,
            threads_per_group: plan.threads_per_group(),
            groups: plan.groups(),
            max_buffer_bytes: plan.max_buffer_bytes(),
            accelerator,
        })
    }
}

impl IpadAcceleratorIr {
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn threads_per_group(&self) -> u32 {
        self.threads_per_group
    }

    pub fn groups(&self) -> u32 {
        self.groups
    }

    pub fn max_buffer_bytes(&self) -> u64 {
        self.max_buffer_bytes
    }

    pub fn total_threads(&self) -> u32 {
        self.groups * self.threads_per_group
    }

    pub fn accelerator(&self) -> &'static DeviceAccelerator {
        self.accelerator
    }

    fn canonical_ir(&self) -> String {
        let cores = match self.accelerator.gpu_cores {
            Some(cores) => cores.to_string(),
            None => "unpublished".to_string(),
        };
        let family = match self.accelerator.metal_family {
            Some(family) => family.to_string(),
            None => "none".to_string(),
        };
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=device\ncategory=ipad\nmodel={}\nchip={}\ngpu_cores={cores}\nmetal_family={family}\nthreads_per_group={}\ngroups={}\ntotal_threads={}\nmax_buffer_bytes={}\nexecution=none_attempted\n",
            self.model,
            self.accelerator.chip,
            self.threads_per_group,
            self.groups,
            self.total_threads(),
            self.max_buffer_bytes,
        )
    }

    pub fn lower_checked(
        &self,
        target: IpadAcceleratorTarget,
        capabilities: IpadAcceleratorRuntimeCapabilities,
    ) -> Result<IpadAcceleratorArtifact, IpadAcceleratorDiagnostic> {
        if target != IpadAcceleratorTarget::Device {
            return Err(IpadAcceleratorDiagnostic::UnsupportedTarget);
        }
        if !capabilities.gpu_driver {
            return Err(IpadAcceleratorDiagnostic::MissingGpuDriver);
        }
        if !capabilities.utf8_strings {
            return Err(IpadAcceleratorDiagnostic::MissingUtf8Strings);
        }
        if self.accelerator.metal_family.is_none_or(|family| family < 1) {
            return Err(IpadAcceleratorDiagnostic::UnsupportedAccelerator);
        }
        Ok(IpadAcceleratorArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_ir(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; iPad accelerator, no GPU execution is attempted.\npub fn make_dispatch_plan() -> Result<crate::apple_runtime::MetalDispatchPlan, crate::apple_runtime::MetalError> {{\n    crate::apple_runtime::MetalDispatchPlan::new({}u32, {}u32, {}u64)\n}}\n\npub fn make_accelerator_profile() -> Result<crate::apple_runtime::AcceleratorProfile, crate::apple_runtime::AcceleratorError> {{\n    crate::apple_runtime::AcceleratorProfile::new({:?}, Some({}u16), Some({}u8))\n}}\n",
                self.threads_per_group,
                self.groups,
                self.max_buffer_bytes,
                self.accelerator.chip,
                self.accelerator.gpu_cores.expect("metal-capable models publish gpu_cores"),
                self.accelerator.metal_family.expect("checked above"),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES: IpadAcceleratorRuntimeCapabilities = IpadAcceleratorRuntimeCapabilities {
        gpu_driver: true,
        utf8_strings: true,
    };

    fn declaration(model: &str, threads_per_group: u32, groups: u32) -> IpadAcceleratorDeclaration {
        IpadAcceleratorDeclaration {
            model: model.to_string(),
            threads_per_group,
            groups,
            max_buffer_bytes: 8192,
        }
    }

    #[test]
    fn catalog_is_complete_and_coherent() {
        let mut seen = std::collections::BTreeSet::new();
        for (name, accelerator) in MODELS {
            assert!(!name.is_empty());
            assert!(seen.insert(*name), "duplicate model name: {name}");
            assert!(!accelerator.chip.is_empty());
            match accelerator.metal_family {
                None => {}
                Some(family) => {
                    assert!((1..=9).contains(&family));
                    assert!(accelerator.gpu_cores.is_some());
                }
            }
        }
    }

    #[test]
    fn accelerator_for_resolves_every_model_and_rejects_unknown() {
        for (name, accelerator) in MODELS {
            let found = accelerator_for(name).expect("catalog name must resolve");
            assert_eq!(found, accelerator);
        }
        assert!(accelerator_for("unknown device").is_none());
        // Matching is exact; no case folding.
        assert!(accelerator_for("ipad air (5th generation)").is_none());
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_artifact() {
        let ir = declaration("iPad Air (5th generation)", 256, 8).normalize().unwrap();
        assert_eq!(ir.model(), "iPad Air (5th generation)");
        assert_eq!(ir.threads_per_group(), 256);
        assert_eq!(ir.groups(), 8);
        assert_eq!(ir.max_buffer_bytes(), 8192);
        assert_eq!(ir.total_threads(), 2048);
        assert_eq!(ir.accelerator().chip, "M1");
        assert_eq!(ir.accelerator().gpu_cores, Some(8));
        assert_eq!(ir.accelerator().metal_family, Some(7));
        let first = ir.lower_checked(IpadAcceleratorTarget::Device, CAPABILITIES).unwrap();
        let second = ir.lower_checked(IpadAcceleratorTarget::Device, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=device\n"));
        assert!(first.canonical_ir.contains("category=ipad\n"));
        assert!(first.canonical_ir.contains("model=iPad Air (5th generation)\n"));
        assert!(first.canonical_ir.contains("chip=M1\n"));
        assert!(first.canonical_ir.contains("metal_family=7\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first.source.contains("pub fn make_dispatch_plan()"));
        assert!(first.source.contains("pub fn make_accelerator_profile()"));
        assert!(first
            .source
            .contains("MetalDispatchPlan::new(256u32, 8u32, 8192u64)"));
    }

    #[test]
    fn catalog_covers_release_history() {
        for (name, accelerator) in MODELS {
            let ir = declaration(name, 256, 4).normalize().unwrap();
            let result = ir.lower_checked(IpadAcceleratorTarget::Device, CAPABILITIES);
            if accelerator.metal_family.is_some() {
                let artifact = result.expect("Metal-capable model must lower");
                assert_eq!(artifact.recipe_version, RECIPE_VERSION);
            } else {
                assert_eq!(result, Err(IpadAcceleratorDiagnostic::UnsupportedAccelerator));
            }
        }
        // The known non-Metal models are exercised above, but assert at least
        // one explicitly so the catalog cannot silently drop them.
        let pre_metal = declaration("iPad 2", 256, 4).normalize().unwrap();
        assert_eq!(
            pre_metal.lower_checked(IpadAcceleratorTarget::Device, CAPABILITIES),
            Err(IpadAcceleratorDiagnostic::UnsupportedAccelerator)
        );
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration("iPad Air (5th generation)", 256, 4).normalize().unwrap();
        for target in [
            IpadAcceleratorTarget::CrossPlatform,
            IpadAcceleratorTarget::Browser,
            IpadAcceleratorTarget::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(IpadAcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                IpadAcceleratorTarget::Device,
                IpadAcceleratorRuntimeCapabilities {
                    gpu_driver: false,
                    utf8_strings: true
                }
            ),
            Err(IpadAcceleratorDiagnostic::MissingGpuDriver)
        );
        assert_eq!(
            ir.lower_checked(
                IpadAcceleratorTarget::Device,
                IpadAcceleratorRuntimeCapabilities {
                    gpu_driver: true,
                    utf8_strings: false
                }
            ),
            Err(IpadAcceleratorDiagnostic::MissingUtf8Strings)
        );
    }

    #[test]
    fn rejects_unknown_model_and_bad_dispatch_bounds() {
        assert_eq!(
            declaration("unknown device", 256, 4).normalize(),
            Err(IpadAcceleratorDiagnostic::BadModel)
        );
        assert_eq!(
            declaration("iPad Air (5th generation)", 0, 4).normalize(),
            Err(IpadAcceleratorDiagnostic::ThreadsPerGroupZero)
        );
        assert_eq!(
            declaration("iPad Air (5th generation)", 63, 4).normalize(),
            Err(IpadAcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo)
        );
        assert_eq!(
            declaration("iPad Air (5th generation)", 2048, 4).normalize(),
            Err(IpadAcceleratorDiagnostic::ThreadsPerGroupTooLarge)
        );
        assert_eq!(
            declaration("iPad Air (5th generation)", 256, 0).normalize(),
            Err(IpadAcceleratorDiagnostic::GroupsZero)
        );
        assert_eq!(
            declaration("iPad Air (5th generation)", 256, MAX_GROUPS + 1).normalize(),
            Err(IpadAcceleratorDiagnostic::GroupsTooLarge)
        );
        let mut zero_buffer = declaration("iPad Air (5th generation)", 256, 4);
        zero_buffer.max_buffer_bytes = 0;
        assert_eq!(
            zero_buffer.normalize(),
            Err(IpadAcceleratorDiagnostic::BufferBytesZero)
        );
        let mut huge_buffer = declaration("iPad Air (5th generation)", 256, 4);
        huge_buffer.max_buffer_bytes = MAX_BUFFER_BYTES + 1;
        assert_eq!(
            huge_buffer.normalize(),
            Err(IpadAcceleratorDiagnostic::BufferBytesTooLarge)
        );
        // `MAX_GROUPS * MAX_THREADS_PER_GROUP` fits a `u32`, so the overflow
        // guard stays defensive; the full valid bound is accepted.
        assert!(declaration("iPad Air (5th generation)", MAX_THREADS_PER_GROUP, MAX_GROUPS)
            .normalize()
            .is_ok());
    }

    #[test]
    fn generated_factories_compile_and_run() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration("iPad Air (5th generation)", 256, 8)
            .normalize()
            .unwrap()
            .lower_checked(IpadAcceleratorTarget::Device, CAPABILITIES)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let plan = make_dispatch_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.threads_per_group(), 256);\n    assert_eq!(plan.groups(), 8);\n    assert_eq!(plan.total_threads(), 2048);\n    assert_eq!(plan.max_buffer_bytes(), 8192);\n    let profile = make_accelerator_profile().expect(\"factory should construct profile\");\n    assert_eq!(profile.chip(), \"M1\");\n    assert_eq!(profile.gpu_cores(), Some(8));\n    assert_eq!(profile.metal_family(), Some(7));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-ipad-accelerator-factory-{}-{nonce}",
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
            .expect("run generated accelerator factories");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
