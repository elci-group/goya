//! A bounded iPod touch device-accelerator capability.
//!
//! The negotiated artifact is a checked dispatch plan and a factory binding to
//! this crate's plan runtime. It does not claim GPU execution, driver access,
//! or prove that a shader was compiled. Hosts report their real facilities
//! through [`IpodTouchAcceleratorRuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.accelerator.ipod_touch.v1";
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
    (
        "iPod touch (1st generation)",
        DeviceAccelerator {
            chip: "Samsung S5L8900",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "iPod touch (2nd generation)",
        DeviceAccelerator {
            chip: "Samsung S5L8920",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "iPod touch (3rd generation)",
        DeviceAccelerator {
            chip: "Samsung S5L8922",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "iPod touch (4th generation)",
        DeviceAccelerator {
            chip: "A4",
            gpu_cores: None,
            metal_family: None,
        },
    ),
    (
        "iPod touch (5th generation)",
        DeviceAccelerator {
            chip: "A5",
            gpu_cores: Some(2),
            metal_family: None,
        },
    ),
    (
        "iPod touch (6th generation)",
        DeviceAccelerator {
            chip: "A8",
            gpu_cores: Some(4),
            metal_family: Some(2),
        },
    ),
    (
        "iPod touch (7th generation)",
        DeviceAccelerator {
            chip: "A10 Fusion",
            gpu_cores: Some(6),
            metal_family: Some(4),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpodTouchAcceleratorDeclaration {
    /// Device model name, matched exactly against the catalog.
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
pub struct IpodTouchAcceleratorIr {
    model: &'static str,
    accelerator: &'static DeviceAccelerator,
    threads_per_group: u32,
    groups: u32,
    max_buffer_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpodTouchAcceleratorTarget {
    /// The device itself (device or simulator build).
    Device,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IpodTouchAcceleratorRuntimeCapabilities {
    pub gpu_driver: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpodTouchAcceleratorDiagnostic {
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

impl fmt::Display for IpodTouchAcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadModel => "device model is not in the iPod touch catalog",
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
            Self::UnsupportedTarget => "iPod touch accelerator supports only the device target",
            Self::UnsupportedAccelerator => {
                "device model does not publish a Metal GPU family level"
            }
            Self::MissingGpuDriver => "target runtime must report a GPU driver",
            Self::MissingUtf8Strings => "target runtime must report UTF-8 string support",
        })
    }
}

impl std::error::Error for IpodTouchAcceleratorDiagnostic {}

/// Negotiated accelerator design and source text. The artifact is a plan and
/// a factory definition; successful construction is not proof that a GPU ran
/// the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IpodTouchAcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl IpodTouchAcceleratorDeclaration {
    pub fn normalize(self) -> Result<IpodTouchAcceleratorIr, IpodTouchAcceleratorDiagnostic> {
        let (model, accelerator) = MODELS
            .iter()
            .find(|(name, _)| *name == self.model)
            .ok_or(IpodTouchAcceleratorDiagnostic::BadModel)?;
        let plan = crate::apple_runtime::MetalDispatchPlan::new(
            self.threads_per_group,
            self.groups,
            self.max_buffer_bytes,
        )
        .map_err(|error| match error {
            crate::apple_runtime::MetalError::ZeroThreadsPerGroup => {
                IpodTouchAcceleratorDiagnostic::ThreadsPerGroupZero
            }
            crate::apple_runtime::MetalError::ThreadsPerGroupNotPowerOfTwo => {
                IpodTouchAcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo
            }
            crate::apple_runtime::MetalError::ThreadsPerGroupTooLarge => {
                IpodTouchAcceleratorDiagnostic::ThreadsPerGroupTooLarge
            }
            crate::apple_runtime::MetalError::ZeroGroups => {
                IpodTouchAcceleratorDiagnostic::GroupsZero
            }
            crate::apple_runtime::MetalError::GroupsTooLarge => {
                IpodTouchAcceleratorDiagnostic::GroupsTooLarge
            }
            crate::apple_runtime::MetalError::ZeroBufferBytes => {
                IpodTouchAcceleratorDiagnostic::BufferBytesZero
            }
            crate::apple_runtime::MetalError::BufferTooLarge => {
                IpodTouchAcceleratorDiagnostic::BufferBytesTooLarge
            }
            crate::apple_runtime::MetalError::ThreadCountOverflow => {
                IpodTouchAcceleratorDiagnostic::ThreadCountOverflow
            }
        })?;
        Ok(IpodTouchAcceleratorIr {
            model,
            accelerator,
            threads_per_group: plan.threads_per_group(),
            groups: plan.groups(),
            max_buffer_bytes: plan.max_buffer_bytes(),
        })
    }
}

impl IpodTouchAcceleratorIr {
    pub fn model(&self) -> &'static str {
        self.model
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

    pub fn canonical(&self) -> String {
        let gpu_cores = match self.accelerator.gpu_cores {
            Some(cores) => cores.to_string(),
            None => "unpublished".to_string(),
        };
        let metal_family = match self.accelerator.metal_family {
            Some(family) => family.to_string(),
            None => "none".to_string(),
        };
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget=device\ncategory=ipod_touch\nmodel={}\nchip={}\ngpu_cores={gpu_cores}\nmetal_family={metal_family}\nthreads_per_group={}\ngroups={}\ntotal_threads={}\nmax_buffer_bytes={}\nexecution=none_attempted\n",
            self.model,
            self.accelerator.chip,
            self.threads_per_group,
            self.groups,
            self.total_threads(),
            self.max_buffer_bytes
        )
    }

    pub fn lower_checked(
        &self,
        target: IpodTouchAcceleratorTarget,
        capabilities: IpodTouchAcceleratorRuntimeCapabilities,
    ) -> Result<IpodTouchAcceleratorArtifact, IpodTouchAcceleratorDiagnostic> {
        if target != IpodTouchAcceleratorTarget::Device {
            return Err(IpodTouchAcceleratorDiagnostic::UnsupportedTarget);
        }
        if !capabilities.gpu_driver {
            return Err(IpodTouchAcceleratorDiagnostic::MissingGpuDriver);
        }
        if !capabilities.utf8_strings {
            return Err(IpodTouchAcceleratorDiagnostic::MissingUtf8Strings);
        }
        if self.accelerator.metal_family.map_or(true, |family| family < 1) {
            return Err(IpodTouchAcceleratorDiagnostic::UnsupportedAccelerator);
        }
        let cores_expr = match self.accelerator.gpu_cores {
            Some(cores) => format!("Some({cores}u16)"),
            None => "None".to_string(),
        };
        let family = self.accelerator.metal_family.expect("family checked above");
        Ok(IpodTouchAcceleratorArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical(),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; ipod_touch accelerator, no GPU execution is attempted.\npub fn make_dispatch_plan() -> Result<crate::apple_runtime::MetalDispatchPlan, crate::apple_runtime::MetalError> {{\n    crate::apple_runtime::MetalDispatchPlan::new({}u32, {}u32, {}u64)\n}}\n\npub fn make_accelerator_profile() -> Result<crate::apple_runtime::AcceleratorProfile, crate::apple_runtime::AcceleratorError> {{\n    crate::apple_runtime::AcceleratorProfile::new({:?}, {cores_expr}, Some({family}u8))\n}}\n",
                self.threads_per_group,
                self.groups,
                self.max_buffer_bytes,
                self.accelerator.chip,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPOD_TOUCH: IpodTouchAcceleratorRuntimeCapabilities =
        IpodTouchAcceleratorRuntimeCapabilities {
            gpu_driver: true,
            utf8_strings: true,
        };

    fn declaration(model: &str, threads_per_group: u32, groups: u32) -> IpodTouchAcceleratorDeclaration {
        IpodTouchAcceleratorDeclaration {
            model: model.to_string(),
            threads_per_group,
            groups,
            max_buffer_bytes: 4096,
        }
    }

    fn first_metal_capable() -> &'static str {
        MODELS
            .iter()
            .find(|(_, accelerator)| accelerator.metal_family.is_some())
            .map(|(name, _)| *name)
            .expect("catalog must contain a Metal-capable model")
    }

    #[test]
    fn catalog_is_complete_and_coherent() {
        let mut seen: Vec<&str> = Vec::new();
        for (name, accelerator) in MODELS {
            assert!(!name.is_empty(), "model name must not be empty");
            assert!(!seen.contains(name), "model names must be unique: {name}");
            seen.push(name);
            assert!(!accelerator.chip.is_empty(), "chip must not be empty");
            match accelerator.metal_family {
                Some(family) => {
                    assert!(
                        (1..=9).contains(&family),
                        "metal_family must be within 1..=9"
                    );
                    assert!(
                        accelerator.gpu_cores.is_some(),
                        "Metal-capable models must publish gpu_cores"
                    );
                }
                None => {}
            }
        }
        assert!(!MODELS.is_empty());
    }

    #[test]
    fn accelerator_for_resolves_every_model_and_rejects_unknown() {
        for (name, accelerator) in MODELS {
            assert_eq!(accelerator_for(name), Some(accelerator));
        }
        assert_eq!(accelerator_for("unknown device"), None);
        assert_eq!(accelerator_for("ipod touch (7th generation)"), None);
        assert_eq!(accelerator_for(""), None);
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_artifact() {
        let model = first_metal_capable();
        let ir = declaration(model, 64, 16).normalize().unwrap();
        assert_eq!(ir.threads_per_group(), 64);
        assert_eq!(ir.groups(), 16);
        assert_eq!(ir.total_threads(), 1024);
        assert_eq!(ir.max_buffer_bytes(), 4096);
        assert_eq!(ir.model(), model);
        let first = ir
            .lower_checked(IpodTouchAcceleratorTarget::Device, IPOD_TOUCH)
            .unwrap();
        let second = ir
            .lower_checked(IpodTouchAcceleratorTarget::Device, IPOD_TOUCH)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert_eq!(first.recipe_version, RECIPE_VERSION);
        let accelerator = accelerator_for(model).unwrap();
        assert!(first.canonical_ir.contains("target=device\n"));
        assert!(first.canonical_ir.contains("category=ipod_touch\n"));
        assert!(first.canonical_ir.contains(&format!("model={model}\n")));
        assert!(first
            .canonical_ir
            .contains(&format!("chip={}\n", accelerator.chip)));
        assert!(first
            .canonical_ir
            .contains(&format!("metal_family={}\n", accelerator.metal_family.unwrap())));
        assert!(first.canonical_ir.contains("threads_per_group=64\n"));
        assert!(first.canonical_ir.contains("groups=16\n"));
        assert!(first.canonical_ir.contains("total_threads=1024\n"));
        assert!(first.canonical_ir.contains("max_buffer_bytes=4096\n"));
        assert!(first.canonical_ir.contains("execution=none_attempted\n"));
        assert!(first.source.contains("make_dispatch_plan"));
        assert!(first.source.contains("make_accelerator_profile"));
        assert!(first
            .source
            .contains("MetalDispatchPlan::new(64u32, 16u32, 4096u64)"));
    }

    #[test]
    fn catalog_covers_release_history() {
        for (name, accelerator) in MODELS {
            let ir = declaration(name, 64, 16).normalize().unwrap();
            assert_eq!(ir.accelerator(), accelerator);
            let result = ir.lower_checked(IpodTouchAcceleratorTarget::Device, IPOD_TOUCH);
            if accelerator.metal_family.is_some() {
                assert!(result.is_ok(), "{name} must lower to an artifact");
            } else {
                assert_eq!(result, Err(IpodTouchAcceleratorDiagnostic::UnsupportedAccelerator));
            }
        }
        // The catalog genuinely contains both shapes.
        assert!(MODELS.iter().any(|(_, a)| a.metal_family.is_some()));
        assert!(MODELS.iter().any(|(_, a)| a.metal_family.is_none()));
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration(first_metal_capable(), 64, 16)
            .normalize()
            .unwrap();
        for target in [
            IpodTouchAcceleratorTarget::CrossPlatform,
            IpodTouchAcceleratorTarget::Browser,
            IpodTouchAcceleratorTarget::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, IPOD_TOUCH),
                Err(IpodTouchAcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                IpodTouchAcceleratorTarget::Device,
                IpodTouchAcceleratorRuntimeCapabilities {
                    gpu_driver: false,
                    utf8_strings: true,
                }
            ),
            Err(IpodTouchAcceleratorDiagnostic::MissingGpuDriver)
        );
        assert_eq!(
            ir.lower_checked(
                IpodTouchAcceleratorTarget::Device,
                IpodTouchAcceleratorRuntimeCapabilities {
                    gpu_driver: true,
                    utf8_strings: false,
                }
            ),
            Err(IpodTouchAcceleratorDiagnostic::MissingUtf8Strings)
        );
        assert_eq!(
            ir.lower_checked(
                IpodTouchAcceleratorTarget::Device,
                IpodTouchAcceleratorRuntimeCapabilities::default()
            ),
            Err(IpodTouchAcceleratorDiagnostic::MissingGpuDriver)
        );
    }

    #[test]
    fn generated_factories_compile_and_run() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration("iPod touch (7th generation)", 64, 16)
            .normalize()
            .unwrap()
            .lower_checked(IpodTouchAcceleratorTarget::Device, IPOD_TOUCH)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let plan = make_dispatch_plan().expect(\"factory should construct plan\");\n    assert_eq!(plan.threads_per_group(), 64);\n    assert_eq!(plan.groups(), 16);\n    assert_eq!(plan.total_threads(), 1024);\n    assert_eq!(plan.max_buffer_bytes(), 4096);\n    let profile = make_accelerator_profile().expect(\"factory should construct profile\");\n    assert_eq!(profile.chip(), \"A10 Fusion\");\n    assert_eq!(profile.gpu_cores(), Some(6));\n    assert_eq!(profile.metal_family(), Some(4));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-ipod-touch-accelerator-factory-{}-{nonce}",
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

    #[test]
    fn rejects_unknown_model_and_bad_dispatch_bounds() {
        assert_eq!(
            declaration("unknown device", 64, 16).normalize(),
            Err(IpodTouchAcceleratorDiagnostic::BadModel)
        );
        assert_eq!(
            declaration(first_metal_capable(), 0, 16).normalize(),
            Err(IpodTouchAcceleratorDiagnostic::ThreadsPerGroupZero)
        );
        assert_eq!(
            declaration(first_metal_capable(), 63, 16).normalize(),
            Err(IpodTouchAcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo)
        );
        assert_eq!(
            declaration(first_metal_capable(), 2048, 16).normalize(),
            Err(IpodTouchAcceleratorDiagnostic::ThreadsPerGroupTooLarge)
        );
        assert_eq!(
            declaration(first_metal_capable(), 64, 0).normalize(),
            Err(IpodTouchAcceleratorDiagnostic::GroupsZero)
        );
        assert_eq!(
            declaration(first_metal_capable(), 64, MAX_GROUPS + 1).normalize(),
            Err(IpodTouchAcceleratorDiagnostic::GroupsTooLarge)
        );
        let mut zero_buffer = declaration(first_metal_capable(), 64, 16);
        zero_buffer.max_buffer_bytes = 0;
        assert_eq!(
            zero_buffer.normalize(),
            Err(IpodTouchAcceleratorDiagnostic::BufferBytesZero)
        );
        let mut huge_buffer = declaration(first_metal_capable(), 64, 16);
        huge_buffer.max_buffer_bytes = MAX_BUFFER_BYTES + 1;
        assert_eq!(
            huge_buffer.normalize(),
            Err(IpodTouchAcceleratorDiagnostic::BufferBytesTooLarge)
        );
        // `MAX_GROUPS * MAX_THREADS_PER_GROUP` fits a `u32`, so the overflow
        // guard stays defensive through normalize; the full valid bound is
        // accepted.
        assert!(declaration(first_metal_capable(), 1024, MAX_GROUPS)
            .normalize()
            .is_ok());
    }
}
