//! A bounded iPhone device-accelerator capability.
//!
//! The negotiated artifact is a checked dispatch plan and a factory binding
//! to this crate's plan runtime. It does not claim GPU execution, driver
//! access, or prove that a shader was compiled. Hosts report their real
//! facilities through [`IphoneAcceleratorRuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.accelerator.iphone.v1";
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
    ("iPhone", DeviceAccelerator { chip: "Samsung S5L8900", gpu_cores: None, metal_family: None }),
    ("iPhone 3G", DeviceAccelerator { chip: "Samsung S5L8920", gpu_cores: None, metal_family: None }),
    ("iPhone 3GS", DeviceAccelerator { chip: "Samsung S5L8930", gpu_cores: None, metal_family: None }),
    ("iPhone 4", DeviceAccelerator { chip: "A4", gpu_cores: None, metal_family: None }),
    ("iPhone 4s", DeviceAccelerator { chip: "A5", gpu_cores: Some(2), metal_family: None }),
    ("iPhone 5", DeviceAccelerator { chip: "A6", gpu_cores: Some(3), metal_family: None }),
    ("iPhone 5c", DeviceAccelerator { chip: "A6", gpu_cores: Some(3), metal_family: None }),
    ("iPhone 5s", DeviceAccelerator { chip: "A7", gpu_cores: Some(4), metal_family: Some(1) }),
    ("iPhone 6", DeviceAccelerator { chip: "A8", gpu_cores: Some(4), metal_family: Some(2) }),
    ("iPhone 6 Plus", DeviceAccelerator { chip: "A8", gpu_cores: Some(4), metal_family: Some(2) }),
    ("iPhone 6s", DeviceAccelerator { chip: "A9", gpu_cores: Some(6), metal_family: Some(3) }),
    ("iPhone 6s Plus", DeviceAccelerator { chip: "A9", gpu_cores: Some(6), metal_family: Some(3) }),
    ("iPhone SE (1st generation)", DeviceAccelerator { chip: "A9", gpu_cores: Some(6), metal_family: Some(3) }),
    ("iPhone 7", DeviceAccelerator { chip: "A10 Fusion", gpu_cores: Some(6), metal_family: Some(4) }),
    ("iPhone 7 Plus", DeviceAccelerator { chip: "A10 Fusion", gpu_cores: Some(6), metal_family: Some(4) }),
    ("iPhone 8", DeviceAccelerator { chip: "A11 Bionic", gpu_cores: Some(3), metal_family: Some(5) }),
    ("iPhone 8 Plus", DeviceAccelerator { chip: "A11 Bionic", gpu_cores: Some(3), metal_family: Some(5) }),
    ("iPhone X", DeviceAccelerator { chip: "A11 Bionic", gpu_cores: Some(3), metal_family: Some(5) }),
    ("iPhone XR", DeviceAccelerator { chip: "A12 Bionic", gpu_cores: Some(4), metal_family: Some(6) }),
    ("iPhone XS", DeviceAccelerator { chip: "A12 Bionic", gpu_cores: Some(4), metal_family: Some(6) }),
    ("iPhone XS Max", DeviceAccelerator { chip: "A12 Bionic", gpu_cores: Some(4), metal_family: Some(6) }),
    ("iPhone 11", DeviceAccelerator { chip: "A13 Bionic", gpu_cores: Some(4), metal_family: Some(7) }),
    ("iPhone 11 Pro", DeviceAccelerator { chip: "A13 Bionic", gpu_cores: Some(4), metal_family: Some(7) }),
    ("iPhone 11 Pro Max", DeviceAccelerator { chip: "A13 Bionic", gpu_cores: Some(4), metal_family: Some(7) }),
    ("iPhone SE (2nd generation)", DeviceAccelerator { chip: "A13 Bionic", gpu_cores: Some(4), metal_family: Some(7) }),
    ("iPhone 12 mini", DeviceAccelerator { chip: "A14 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPhone 12", DeviceAccelerator { chip: "A14 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPhone 12 Pro", DeviceAccelerator { chip: "A14 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPhone 12 Pro Max", DeviceAccelerator { chip: "A14 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPhone 13 mini", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPhone 13", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPhone 13 Pro", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone 13 Pro Max", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone SE (3rd generation)", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(4), metal_family: Some(8) }),
    ("iPhone 14", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone 14 Plus", DeviceAccelerator { chip: "A15 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone 14 Pro", DeviceAccelerator { chip: "A16 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone 14 Pro Max", DeviceAccelerator { chip: "A16 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone 15", DeviceAccelerator { chip: "A16 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone 15 Plus", DeviceAccelerator { chip: "A16 Bionic", gpu_cores: Some(5), metal_family: Some(8) }),
    ("iPhone 15 Pro", DeviceAccelerator { chip: "A17 Pro", gpu_cores: Some(6), metal_family: Some(9) }),
    ("iPhone 15 Pro Max", DeviceAccelerator { chip: "A17 Pro", gpu_cores: Some(6), metal_family: Some(9) }),
    ("iPhone 16", DeviceAccelerator { chip: "A18", gpu_cores: Some(5), metal_family: Some(9) }),
    ("iPhone 16 Plus", DeviceAccelerator { chip: "A18", gpu_cores: Some(5), metal_family: Some(9) }),
    ("iPhone 16 Pro", DeviceAccelerator { chip: "A18 Pro", gpu_cores: Some(6), metal_family: Some(9) }),
    ("iPhone 16 Pro Max", DeviceAccelerator { chip: "A18 Pro", gpu_cores: Some(6), metal_family: Some(9) }),
    ("iPhone 16e", DeviceAccelerator { chip: "A18", gpu_cores: Some(4), metal_family: Some(9) }),
];

/// Per-model lookup. Unknown models return None; the registry never guesses.
pub fn accelerator_for(model: &str) -> Option<&'static DeviceAccelerator> {
    MODELS.iter().find(|(name, _)| *name == model).map(|(_, accelerator)| accelerator)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IphoneAcceleratorTarget {
    /// The device itself (device or simulator build of this category).
    Device,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IphoneAcceleratorRuntimeCapabilities {
    pub gpu_driver: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IphoneAcceleratorDeclaration {
    /// Exact model name from the catalog; unknown models never normalize.
    pub model: String,
    /// Threads per threadgroup; a power of two, from one through
    /// `MAX_THREADS_PER_GROUP` (the Metal limit).
    pub threads_per_group: u32,
    /// Threadgroups dispatched, from one through `MAX_GROUPS`.
    pub groups: u32,
    /// Bound on any single argument buffer, from one through
    /// `MAX_BUFFER_BYTES`.
    pub max_buffer_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IphoneAcceleratorIr {
    model: String,
    threads_per_group: u32,
    groups: u32,
    max_buffer_bytes: u64,
    accelerator: &'static DeviceAccelerator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IphoneAcceleratorDiagnostic {
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

impl fmt::Display for IphoneAcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadModel => "model is not in the iPhone accelerator catalog",
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
            Self::UnsupportedTarget => "iPhone accelerator supports only device targets",
            Self::UnsupportedAccelerator => {
                "device has no documented Metal accelerator family"
            }
            Self::MissingGpuDriver => "target runtime must report a GPU driver",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for IphoneAcceleratorDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a GPU ran
/// the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IphoneAcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl IphoneAcceleratorDeclaration {
    pub fn normalize(self) -> Result<IphoneAcceleratorIr, IphoneAcceleratorDiagnostic> {
        let accelerator =
            accelerator_for(&self.model).ok_or(IphoneAcceleratorDiagnostic::BadModel)?;
        let plan = crate::apple_runtime::MetalDispatchPlan::new(
            self.threads_per_group,
            self.groups,
            self.max_buffer_bytes,
        )
        .map_err(|error| match error {
            crate::apple_runtime::MetalError::ZeroThreadsPerGroup => {
                IphoneAcceleratorDiagnostic::ThreadsPerGroupZero
            }
            crate::apple_runtime::MetalError::ThreadsPerGroupNotPowerOfTwo => {
                IphoneAcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo
            }
            crate::apple_runtime::MetalError::ThreadsPerGroupTooLarge => {
                IphoneAcceleratorDiagnostic::ThreadsPerGroupTooLarge
            }
            crate::apple_runtime::MetalError::ZeroGroups => {
                IphoneAcceleratorDiagnostic::GroupsZero
            }
            crate::apple_runtime::MetalError::GroupsTooLarge => {
                IphoneAcceleratorDiagnostic::GroupsTooLarge
            }
            crate::apple_runtime::MetalError::ZeroBufferBytes => {
                IphoneAcceleratorDiagnostic::BufferBytesZero
            }
            crate::apple_runtime::MetalError::BufferTooLarge => {
                IphoneAcceleratorDiagnostic::BufferBytesTooLarge
            }
            crate::apple_runtime::MetalError::ThreadCountOverflow => {
                IphoneAcceleratorDiagnostic::ThreadCountOverflow
            }
        })?;
        debug_assert_eq!(plan.threads_per_group(), self.threads_per_group);
        debug_assert_eq!(plan.groups(), self.groups);
        debug_assert_eq!(plan.max_buffer_bytes(), self.max_buffer_bytes);
        Ok(IphoneAcceleratorIr {
            model: self.model,
            threads_per_group: self.threads_per_group,
            groups: self.groups,
            max_buffer_bytes: self.max_buffer_bytes,
            accelerator,
        })
    }
}

impl IphoneAcceleratorIr {
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

    fn canonical(&self) -> String {
        let gpu_cores = self
            .accelerator
            .gpu_cores
            .map(|cores| cores.to_string())
            .unwrap_or_else(|| "unpublished".to_string());
        let metal_family = self
            .accelerator
            .metal_family
            .map(|family| family.to_string())
            .unwrap_or_else(|| "none".to_string());
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=device\ncategory=iphone\nmodel={}\nchip={}\ngpu_cores={gpu_cores}\nmetal_family={metal_family}\nthreads_per_group={}\ngroups={}\ntotal_threads={}\nmax_buffer_bytes={}\nexecution=none_attempted\n",
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
        target: IphoneAcceleratorTarget,
        capabilities: IphoneAcceleratorRuntimeCapabilities,
    ) -> Result<IphoneAcceleratorArtifact, IphoneAcceleratorDiagnostic> {
        if target != IphoneAcceleratorTarget::Device {
            return Err(IphoneAcceleratorDiagnostic::UnsupportedTarget);
        }
        if !capabilities.gpu_driver {
            return Err(IphoneAcceleratorDiagnostic::MissingGpuDriver);
        }
        if !capabilities.utf8_strings {
            return Err(IphoneAcceleratorDiagnostic::MissingUtf8Strings);
        }
        let metal_family =
            self.accelerator.metal_family.filter(|family| *family >= 1);
        if metal_family.is_none() {
            return Err(IphoneAcceleratorDiagnostic::UnsupportedAccelerator);
        }
        let chip = self.accelerator.chip;
        let cores_expr = match self.accelerator.gpu_cores {
            Some(cores) => format!("Some({cores}u16)"),
            None => "None".to_string(),
        };
        let family = metal_family.unwrap_or(0);
        Ok(IphoneAcceleratorArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; iphone accelerator, no GPU execution is attempted.\npub fn make_dispatch_plan() -> Result<crate::apple_runtime::MetalDispatchPlan, crate::apple_runtime::MetalError> {{\n    crate::apple_runtime::MetalDispatchPlan::new({}u32, {}u32, {}u64)\n}}\n\npub fn make_accelerator_profile() -> Result<crate::apple_runtime::AcceleratorProfile, crate::apple_runtime::AcceleratorError> {{\n    crate::apple_runtime::AcceleratorProfile::new({chip:?}, {cores_expr}, Some({family}u8))\n}}\n",
                self.threads_per_group, self.groups, self.max_buffer_bytes,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPABILITIES: IphoneAcceleratorRuntimeCapabilities =
        IphoneAcceleratorRuntimeCapabilities {
            gpu_driver: true,
            utf8_strings: true,
        };

    fn declaration(model: &str, threads_per_group: u32, groups: u32) -> IphoneAcceleratorDeclaration {
        IphoneAcceleratorDeclaration {
            model: model.to_string(),
            threads_per_group,
            groups,
            max_buffer_bytes: 4096,
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
                Some(family) => assert!(
                    (1..=9).contains(&family),
                    "metal family out of documented range for {name}"
                ),
            }
            if accelerator.metal_family.is_some() {
                assert!(
                    accelerator.gpu_cores.is_some(),
                    "metal-capable model {name} must publish gpu_cores"
                );
            }
        }
    }

    #[test]
    fn accelerator_for_resolves_every_model_and_rejects_unknown() {
        for (name, _) in MODELS {
            assert!(accelerator_for(name).is_some(), "{name} must resolve");
        }
        assert!(accelerator_for("unknown device").is_none());
        assert!(accelerator_for("IPHONE 15 PRO").is_none());
        assert!(accelerator_for("iPhone 15 pro").is_none());
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_artifact() {
        let model = "iPhone 15 Pro";
        let accelerator = accelerator_for(model).unwrap();
        let ir = IphoneAcceleratorDeclaration {
            model: model.to_string(),
            threads_per_group: 256,
            groups: 8,
            max_buffer_bytes: 8192,
        }
        .normalize()
        .expect("deterministic model must normalize");
        assert_eq!(ir.accelerator(), accelerator);
        assert_eq!(ir.model(), model);
        assert_eq!(ir.total_threads(), 2048);
        let first = ir.lower_checked(IphoneAcceleratorTarget::Device, CAPABILITIES).unwrap();
        let second = ir.lower_checked(IphoneAcceleratorTarget::Device, CAPABILITIES).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert_eq!(first.recipe_version, RECIPE_VERSION);
        assert!(first.canonical_ir.contains("target=device\n"));
        assert!(first.canonical_ir.contains("category=iphone\n"));
        assert!(first.canonical_ir.contains(&format!("model={model}\n")));
        assert!(first.canonical_ir.contains(&format!("chip={}\n", accelerator.chip)));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first
            .canonical_ir
            .contains(&format!("metal_family={}\n", accelerator.metal_family.unwrap())));
        assert!(first.source.contains("make_dispatch_plan"));
        assert!(first.source.contains("make_accelerator_profile"));
        assert!(first
            .source
            .contains("MetalDispatchPlan::new(256u32, 8u32, 8192u64)"));
    }

    #[test]
    fn catalog_covers_release_history() {
        for (name, accelerator) in MODELS {
            let ir = declaration(name, 256, 8).normalize();
            if accelerator.metal_family.is_some() {
                let artifact = ir
                    .expect("metal-capable model must normalize")
                    .lower_checked(IphoneAcceleratorTarget::Device, CAPABILITIES)
                    .expect("metal-capable model must lower");
                assert!(artifact.source.contains("make_dispatch_plan"));
            } else if let Ok(ir) = ir {
                assert_eq!(
                    ir.lower_checked(IphoneAcceleratorTarget::Device, CAPABILITIES),
                    Err(IphoneAcceleratorDiagnostic::UnsupportedAccelerator)
                );
            }
        }
        // The catalog does include pre-Metal models; at least these must
        // normalize but refuse to lower.
        for name in ["iPhone", "iPhone 4", "iPhone 5"] {
            let ir = declaration(name, 64, 4).normalize().unwrap();
            assert_eq!(
                ir.lower_checked(IphoneAcceleratorTarget::Device, CAPABILITIES),
                Err(IphoneAcceleratorDiagnostic::UnsupportedAccelerator)
            );
        }
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration("iPhone 15 Pro", 64, 4).normalize().unwrap();
        for target in [
            IphoneAcceleratorTarget::CrossPlatform,
            IphoneAcceleratorTarget::Browser,
            IphoneAcceleratorTarget::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, CAPABILITIES),
                Err(IphoneAcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                IphoneAcceleratorTarget::Device,
                IphoneAcceleratorRuntimeCapabilities {
                    gpu_driver: false,
                    utf8_strings: true,
                }
            ),
            Err(IphoneAcceleratorDiagnostic::MissingGpuDriver)
        );
        assert_eq!(
            ir.lower_checked(
                IphoneAcceleratorTarget::Device,
                IphoneAcceleratorRuntimeCapabilities {
                    gpu_driver: true,
                    utf8_strings: false,
                }
            ),
            Err(IphoneAcceleratorDiagnostic::MissingUtf8Strings)
        );
    }

    #[test]
    fn rejects_unknown_model_and_bad_dispatch_bounds() {
        assert_eq!(
            declaration("unknown device", 64, 4).normalize(),
            Err(IphoneAcceleratorDiagnostic::BadModel)
        );
        assert_eq!(
            declaration("iPhone 15 Pro", 0, 4).normalize(),
            Err(IphoneAcceleratorDiagnostic::ThreadsPerGroupZero)
        );
        assert_eq!(
            declaration("iPhone 15 Pro", 63, 4).normalize(),
            Err(IphoneAcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo)
        );
        assert_eq!(
            declaration("iPhone 15 Pro", MAX_THREADS_PER_GROUP * 2, 4).normalize(),
            Err(IphoneAcceleratorDiagnostic::ThreadsPerGroupTooLarge)
        );
        assert_eq!(
            declaration("iPhone 15 Pro", 64, 0).normalize(),
            Err(IphoneAcceleratorDiagnostic::GroupsZero)
        );
        assert_eq!(
            declaration("iPhone 15 Pro", 64, MAX_GROUPS + 1).normalize(),
            Err(IphoneAcceleratorDiagnostic::GroupsTooLarge)
        );
        let zero_buffer = IphoneAcceleratorDeclaration {
            model: "iPhone 15 Pro".to_string(),
            threads_per_group: 64,
            groups: 4,
            max_buffer_bytes: 0,
        };
        assert_eq!(
            zero_buffer.normalize(),
            Err(IphoneAcceleratorDiagnostic::BufferBytesZero)
        );
        let huge_buffer = IphoneAcceleratorDeclaration {
            model: "iPhone 15 Pro".to_string(),
            threads_per_group: 64,
            groups: 4,
            max_buffer_bytes: MAX_BUFFER_BYTES + 1,
        };
        assert_eq!(
            huge_buffer.normalize(),
            Err(IphoneAcceleratorDiagnostic::BufferBytesTooLarge)
        );
        // `ThreadCountOverflow` is unreachable through normalize because
        // `MAX_GROUPS * MAX_THREADS_PER_GROUP` fits a `u32`; the full valid
        // bound is accepted.
        assert!(declaration("iPhone 15 Pro", MAX_THREADS_PER_GROUP, MAX_GROUPS)
            .normalize()
            .is_ok());
    }

    #[test]
    fn generated_factories_compile_and_run() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let ir = IphoneAcceleratorDeclaration {
            model: "iPhone 15 Pro".to_string(),
            threads_per_group: 256,
            groups: 8,
            max_buffer_bytes: 8192,
        }
        .normalize()
        .expect("deterministic model must normalize");
        assert_eq!(ir.total_threads(), 2048);
        assert_eq!(ir.accelerator().chip, "A17 Pro");
        assert_eq!(ir.accelerator().gpu_cores, Some(6));
        assert_eq!(ir.accelerator().metal_family, Some(9));
        let artifact = ir
            .lower_checked(IphoneAcceleratorTarget::Device, CAPABILITIES)
            .expect("deterministic model must lower");
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let plan = make_dispatch_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.threads_per_group(), 256);\n    assert_eq!(plan.groups(), 8);\n    assert_eq!(plan.total_threads(), 2048);\n    assert_eq!(plan.max_buffer_bytes(), 8192);\n    let profile = make_accelerator_profile().expect(\"factory should construct profile\");\n    assert_eq!(profile.chip(), \"A17 Pro\");\n    assert_eq!(profile.gpu_cores(), Some(6u16));\n    assert_eq!(profile.metal_family(), Some(9u8));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-iphone-accelerator-factory-{}-{nonce}",
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
            .expect("run generated dispatch factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
