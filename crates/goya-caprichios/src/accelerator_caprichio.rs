//! Shared engine for the per-model device-accelerator caprichios.
//!
//! Every iPhone, iPad, iPod touch, and Apple Watch model is its own
//! caprichio: a dedicated module with its own contract ID of the form
//! `goya.hardware.accelerator.<category>.<model_slug>.v1`, stamped out by
//! [`accelerator_models!`] from the per-category model files. This module
//! holds the shared declaration, IR, diagnostics, and lowering; the
//! per-model modules hold only published catalog facts.
//!
//! The negotiated artifact binds to this crate's process-local runtimes. It
//! does not claim GPU execution, driver access, or prove that a shader was
//! compiled; hosts report their real facilities through
//! [`AcceleratorRuntimeCapabilities`].

/// Macros used by the per-category model files.
#[macro_export]
macro_rules! accelerator_models {
    ($category:literal, $(($mod_name:ident, $model:literal, $chip:literal, $cores:expr, $family:expr)),* $(,)?) => {
        $(
            /// One device model's accelerator caprichio: its own contract ID
            /// and published catalog facts.
            pub mod $mod_name {
                pub const CONTRACT: &str = concat!(
                    "goya.hardware.accelerator.",
                    $category,
                    ".",
                    stringify!($mod_name),
                    ".v1"
                );
                pub const MODEL: &str = $model;
                pub const ACCELERATOR: $crate::accelerator_caprichio::DeviceAccelerator =
                    $crate::accelerator_caprichio::DeviceAccelerator {
                        chip: $chip,
                        gpu_cores: $cores,
                        metal_family: $family,
                    };
            }
        )*
        pub const CATEGORY_MODELS: &[$crate::accelerator_caprichio::ModelEntry] = &[
            $(
                $crate::accelerator_caprichio::ModelEntry {
                    contract: $mod_name::CONTRACT,
                    category: $category,
                    model: $model,
                    accelerator: $crate::accelerator_caprichio::DeviceAccelerator {
                        chip: $chip,
                        gpu_cores: $cores,
                        metal_family: $family,
                    },
                },
            )*
        ];
    };
}

use std::fmt;

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

/// One registered per-model caprichio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelEntry {
    pub contract: &'static str,
    pub category: &'static str,
    pub model: &'static str,
    pub accelerator: DeviceAccelerator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceleratorTarget {
    /// The device itself (device or simulator build of this category).
    Device,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by the lowerings. The default
/// is deliberately unsupported until capabilities are supplied. The
/// inventory-only lowering ignores `gpu_driver`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AcceleratorRuntimeCapabilities {
    pub gpu_driver: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorDeclaration {
    /// Exact model name from the per-model caprichio's catalog; unknown
    /// models never normalize, and the negotiated contract ID must name the
    /// same model.
    pub model: String,
    /// Threads per threadgroup; a power of two, from one through
    /// `MAX_THREADS_PER_GROUP`. Required for dispatch-capable models.
    pub threads_per_group: u32,
    /// Threadgroups dispatched, from one through `MAX_GROUPS`.
    pub groups: u32,
    /// Bound on any single argument buffer, from one through
    /// `MAX_BUFFER_BYTES`.
    pub max_buffer_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorIr {
    contract: &'static str,
    entry: &'static ModelEntry,
    threads_per_group: u32,
    groups: u32,
    max_buffer_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceleratorDiagnostic {
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
    MissingGpuDriver,
    MissingUtf8Strings,
}

impl fmt::Display for AcceleratorDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadModel => "model must name the same device as the negotiated contract",
            Self::ThreadsPerGroupZero => "threads per threadgroup must be positive",
            Self::ThreadsPerGroupNotPowerOfTwo => "threads per threadgroup must be a power of two",
            Self::ThreadsPerGroupTooLarge => {
                "threads per threadgroup exceeds the Metal limit of 1024"
            }
            Self::GroupsZero => "threadgroup count must be positive",
            Self::GroupsTooLarge => "threadgroup count exceeds the supported bound",
            Self::BufferBytesZero => "buffer bound must be positive",
            Self::BufferBytesTooLarge => "buffer bound exceeds the supported bound",
            Self::ThreadCountOverflow => "total thread count does not fit a u32 dispatch",
            Self::UnsupportedTarget => "device accelerators support only the Device target",
            Self::MissingGpuDriver => "target must report a GPU driver",
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for AcceleratorDiagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that a GPU ran the dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl AcceleratorDeclaration {
    /// Normalize against the entry named by the negotiated contract ID. The
    /// declaration's model must match the entry's model exactly. Dispatch
    /// bounds are only validated for Metal-capable models; inventory-only
    /// models (no Metal family) ignore them.
    pub fn normalize(
        self,
        entry: &'static ModelEntry,
    ) -> Result<AcceleratorIr, AcceleratorDiagnostic> {
        if self.model != entry.model {
            return Err(AcceleratorDiagnostic::BadModel);
        }
        let dispatch_capable = entry.accelerator.metal_family.is_some();
        if dispatch_capable {
            match crate::apple_runtime::MetalDispatchPlan::new(
                self.threads_per_group,
                self.groups,
                self.max_buffer_bytes,
            ) {
                Ok(plan) => {
                    let _ = plan;
                }
                Err(error) => {
                    return Err(map_dispatch_error(error));
                }
            }
        }
        Ok(AcceleratorIr {
            contract: entry.contract,
            entry,
            threads_per_group: self.threads_per_group,
            groups: self.groups,
            max_buffer_bytes: self.max_buffer_bytes,
        })
    }
}

fn map_dispatch_error(error: crate::apple_runtime::MetalError) -> AcceleratorDiagnostic {
    use crate::apple_runtime::MetalError;
    match error {
        MetalError::ZeroThreadsPerGroup => AcceleratorDiagnostic::ThreadsPerGroupZero,
        MetalError::ThreadsPerGroupNotPowerOfTwo => {
            AcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo
        }
        MetalError::ThreadsPerGroupTooLarge => AcceleratorDiagnostic::ThreadsPerGroupTooLarge,
        MetalError::ZeroGroups => AcceleratorDiagnostic::GroupsZero,
        MetalError::GroupsTooLarge => AcceleratorDiagnostic::GroupsTooLarge,
        MetalError::ZeroBufferBytes => AcceleratorDiagnostic::BufferBytesZero,
        MetalError::BufferTooLarge => AcceleratorDiagnostic::BufferBytesTooLarge,
        MetalError::ThreadCountOverflow => AcceleratorDiagnostic::ThreadCountOverflow,
    }
}

impl AcceleratorIr {
    pub fn contract(&self) -> &'static str {
        self.contract
    }

    pub fn entry(&self) -> &'static ModelEntry {
        self.entry
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

    /// True when the model lowers to a checked Metal dispatch plan; false for
    /// inventory-only models.
    pub fn dispatch_capable(&self) -> bool {
        self.entry
            .accelerator
            .metal_family
            .is_some_and(|family| family >= 1)
    }

    fn facts_ir(&self, category: &'static str, model: &str, chip: &str) -> String {
        let accelerator = self.entry.accelerator;
        let cores = accelerator
            .gpu_cores
            .map(|cores| cores.to_string())
            .unwrap_or_else(|| "unpublished".to_owned());
        let family = accelerator
            .metal_family
            .map(|family| family.to_string())
            .unwrap_or_else(|| "none".to_owned());
        format!(
            "category={category}\nmodel={model}\nchip={chip}\ngpu_cores={cores}\nmetal_family={family}\n"
        )
    }

    pub fn canonical(&self) -> String {
        let entry = self.entry;
        if self.dispatch_capable() {
            format!(
                "contract={}\nrecipe_version={RECIPE_VERSION}\ntarget=device\n{}threads_per_group={}\ngroups={}\ntotal_threads={}\nmax_buffer_bytes={}\nexecution=none_attempted\n",
                self.contract,
                self.facts_ir(entry.category, entry.model, entry.accelerator.chip),
                self.threads_per_group,
                self.groups,
                self.groups * self.threads_per_group,
                self.max_buffer_bytes
            )
        } else {
            format!(
                "contract={}\nrecipe_version={RECIPE_VERSION}\ntarget=device\n{}execution=none_attempted\n",
                self.contract,
                self.facts_ir(entry.category, entry.model, entry.accelerator.chip)
            )
        }
    }

    pub fn lower_checked(
        &self,
        target: AcceleratorTarget,
        capabilities: AcceleratorRuntimeCapabilities,
    ) -> Result<AcceleratorArtifact, AcceleratorDiagnostic> {
        if target != AcceleratorTarget::Device {
            return Err(AcceleratorDiagnostic::UnsupportedTarget);
        }
        let entry = self.entry;
        // Dispatch-capable models require a GPU driver; inventory-only
        // models do not, so the driver check comes first for those.
        if self.dispatch_capable() && !capabilities.gpu_driver {
            return Err(AcceleratorDiagnostic::MissingGpuDriver);
        }
        if !capabilities.utf8_strings {
            return Err(AcceleratorDiagnostic::MissingUtf8Strings);
        }
        if self.dispatch_capable() {
            let profile_facts = format!(
                "AcceleratorProfile::new({:?}, {}, {})",
                entry.accelerator.chip,
                cores_expr(entry.accelerator.gpu_cores),
                family_expr(entry.accelerator.metal_family)
            );
            Ok(AcceleratorArtifact {
                contract: self.contract,
                recipe_version: RECIPE_VERSION,
                canonical_ir: self.canonical(),
                source: format!(
                    "// {}; recipe={RECIPE_VERSION}; {} accelerator, no GPU execution is attempted.\npub fn make_dispatch_plan() -> Result<crate::apple_runtime::MetalDispatchPlan, crate::apple_runtime::MetalError> {{\n    crate::apple_runtime::MetalDispatchPlan::new({}u32, {}u32, {}u64)\n}}\n\npub fn make_accelerator_profile() -> Result<crate::apple_runtime::AcceleratorProfile, crate::apple_runtime::AcceleratorError> {{\n    crate::apple_runtime::{}\n}}\n",
                    self.contract,
                    entry.category,
                    self.threads_per_group,
                    self.groups,
                    self.max_buffer_bytes,
                    profile_facts
                ),
            })
        } else {
            Ok(AcceleratorArtifact {
                contract: self.contract,
                recipe_version: RECIPE_VERSION,
                canonical_ir: self.canonical(),
                source: format!(
                    "// {}; recipe={RECIPE_VERSION}; {} accelerator inventory, this device exposes no Metal GPU.\npub fn make_accelerator_profile() -> Result<crate::apple_runtime::AcceleratorProfile, crate::apple_runtime::AcceleratorError> {{\n    crate::apple_runtime::AcceleratorProfile::new({:?}, None, None)\n}}\n",
                    self.contract,
                    entry.category,
                    entry.accelerator.chip
                ),
            })
        }
    }
}

fn cores_expr(cores: Option<u16>) -> String {
    match cores {
        Some(cores) => format!("Some({cores}u16)"),
        None => "None".to_owned(),
    }
}

fn family_expr(family: Option<u8>) -> String {
    match family {
        Some(family) => format!("Some({family}u8)"),
        None => "None".to_owned(),
    }
}

/// Every registered per-model accelerator caprichio, in catalog order
/// (iphone, ipad, ipod_touch, apple_watch).
pub fn all_models() -> impl Iterator<Item = &'static ModelEntry> {
    crate::iphone_accelerator_models::CATEGORY_MODELS
        .iter()
        .chain(crate::ipad_accelerator_models::CATEGORY_MODELS.iter())
        .chain(crate::ipod_touch_accelerator_models::CATEGORY_MODELS.iter())
        .chain(crate::apple_watch_accelerator_models::CATEGORY_MODELS.iter())
}

/// Exact-match contract ID lookup across all four categories. Unknown IDs
/// return None; the registry never guesses.
pub fn find_model_contract(contract: &str) -> Option<&'static ModelEntry> {
    all_models().find(|entry| entry.contract == contract)
}

/// Test helper used by every per-model module: the model's contract must
/// normalize and lower deterministically with the given dispatch bounds,
/// producing an artifact that names the model and never attempts execution.
#[cfg(test)]
pub fn assert_model_contract(
    contract: &'static str,
    model: &'static str,
    threads_per_group: u32,
    groups: u32,
    max_buffer_bytes: u64,
) {
    let entry = find_model_contract(contract).expect("per-model contract must be registered");
    assert_eq!(entry.model, model);
    let declaration = AcceleratorDeclaration {
        model: model.to_owned(),
        threads_per_group,
        groups,
        max_buffer_bytes,
    };
    let first = declaration
        .clone()
        .normalize(entry)
        .unwrap()
        .lower_checked(
            AcceleratorTarget::Device,
            AcceleratorRuntimeCapabilities {
                gpu_driver: true,
                utf8_strings: true,
            },
        )
        .unwrap();
    let second = declaration
        .normalize(entry)
        .unwrap()
        .lower_checked(
            AcceleratorTarget::Device,
            AcceleratorRuntimeCapabilities {
                gpu_driver: true,
                utf8_strings: true,
            },
        )
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.contract, contract);
    assert!(first.canonical_ir.contains(&format!("model={model}\n")));
    assert!(first.canonical_ir.contains("execution=none_attempted\n"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apple_watch_accelerator_models::apple_watch_series_9;
    use crate::iphone_accelerator_models::iphone_15_pro;

    #[test]
    fn every_model_contract_is_registered_unique_and_well_named() {
        let mut seen = std::collections::BTreeSet::new();
        for entry in all_models() {
            assert!(!entry.model.is_empty());
            assert!(!entry.accelerator.chip.is_empty());
            assert!(entry
                .accelerator
                .metal_family
                .is_none_or(|family| (1..=9).contains(&family)));
            if entry.accelerator.metal_family.is_some() {
                assert!(entry.accelerator.gpu_cores.is_some());
            }
            assert!(entry
                .contract
                .starts_with(&format!("goya.hardware.accelerator.{}.", entry.category)));
            assert!(entry.contract.ends_with(".v1"));
            assert!(seen.insert(entry.contract), "duplicate {}", entry.contract);
        }
        assert!(seen.len() > 100, "catalog should cover all four categories");
    }

    #[test]
    fn find_model_contract_resolves_every_entry_and_rejects_unknown() {
        for entry in all_models() {
            assert_eq!(
                find_model_contract(entry.contract).map(|found| found.model),
                Some(entry.model)
            );
        }
        assert!(find_model_contract("goya.hardware.accelerator.iphone.unknown_model.v1").is_none());
        assert!(find_model_contract("goya.network.fetch.v1").is_none());
    }

    #[test]
    fn dispatch_capable_models_normalize_and_inventory_models_skip_dispatch_bounds() {
        let entry = find_model_contract(iphone_15_pro::CONTRACT).unwrap();
        let good = AcceleratorDeclaration {
            model: iphone_15_pro::MODEL.to_owned(),
            threads_per_group: 256,
            groups: 8,
            max_buffer_bytes: 8192,
        };
        assert!(good.clone().normalize(entry).is_ok());
        // Dispatch bounds are validated for Metal-capable models.
        assert_eq!(
            AcceleratorDeclaration {
                model: iphone_15_pro::MODEL.to_owned(),
                threads_per_group: 0,
                ..good.clone()
            }
            .normalize(entry),
            Err(AcceleratorDiagnostic::ThreadsPerGroupZero)
        );
        assert_eq!(
            AcceleratorDeclaration {
                model: iphone_15_pro::MODEL.to_owned(),
                threads_per_group: 63,
                ..good.clone()
            }
            .normalize(entry),
            Err(AcceleratorDiagnostic::ThreadsPerGroupNotPowerOfTwo)
        );
        // Inventory-only models ignore dispatch bounds entirely.
        let watch_entry = find_model_contract(apple_watch_series_9::CONTRACT).unwrap();
        assert!(!AcceleratorDeclaration {
            model: apple_watch_series_9::MODEL.to_owned(),
            threads_per_group: 0,
            groups: 0,
            max_buffer_bytes: 0,
        }
        .normalize(watch_entry)
        .is_err());
    }

    #[test]
    fn declaration_must_name_the_contracted_model() {
        let entry = find_model_contract(iphone_15_pro::CONTRACT).unwrap();
        let wrong_model = AcceleratorDeclaration {
            model: "iPhone 16 Pro".to_owned(),
            threads_per_group: 256,
            groups: 8,
            max_buffer_bytes: 8192,
        };
        assert_eq!(
            wrong_model.clone().normalize(entry),
            Err(AcceleratorDiagnostic::BadModel)
        );
        assert_eq!(
            AcceleratorDeclaration {
                model: String::new(),
                ..wrong_model.clone()
            }
            .normalize(entry),
            Err(AcceleratorDiagnostic::BadModel)
        );
    }

    #[test]
    fn lowering_rejects_other_targets_and_missing_facilities() {
        let entry = find_model_contract(iphone_15_pro::CONTRACT).unwrap();
        let ir = AcceleratorDeclaration {
            model: iphone_15_pro::MODEL.to_owned(),
            threads_per_group: 256,
            groups: 8,
            max_buffer_bytes: 8192,
        }
        .normalize(entry)
        .unwrap();
        for target in [
            AcceleratorTarget::CrossPlatform,
            AcceleratorTarget::Browser,
            AcceleratorTarget::Node,
        ] {
            assert_eq!(
                ir.lower_checked(
                    target,
                    AcceleratorRuntimeCapabilities {
                        gpu_driver: true,
                        utf8_strings: true
                    }
                ),
                Err(AcceleratorDiagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                AcceleratorTarget::Device,
                AcceleratorRuntimeCapabilities::default()
            ),
            Err(AcceleratorDiagnostic::MissingGpuDriver)
        );
        assert_eq!(
            ir.lower_checked(
                AcceleratorTarget::Device,
                AcceleratorRuntimeCapabilities {
                    gpu_driver: true,
                    utf8_strings: false
                }
            ),
            Err(AcceleratorDiagnostic::MissingUtf8Strings)
        );
        // Inventory-only models do not need a GPU driver.
        let watch_entry = find_model_contract(apple_watch_series_9::CONTRACT).unwrap();
        let watch_ir = AcceleratorDeclaration {
            model: apple_watch_series_9::MODEL.to_owned(),
            threads_per_group: 0,
            groups: 0,
            max_buffer_bytes: 0,
        }
        .normalize(watch_entry)
        .unwrap();
        assert!(watch_ir
            .lower_checked(
                AcceleratorTarget::Device,
                AcceleratorRuntimeCapabilities {
                    gpu_driver: false,
                    utf8_strings: true
                }
            )
            .is_ok());
    }

    #[test]
    fn iphone_dispatch_artifact_and_watch_inventory_artifact_are_deterministic() {
        let iphone_entry = find_model_contract(iphone_15_pro::CONTRACT).unwrap();
        let iphone = AcceleratorDeclaration {
            model: iphone_15_pro::MODEL.to_owned(),
            threads_per_group: 256,
            groups: 8,
            max_buffer_bytes: 8192,
        }
        .normalize(iphone_entry)
        .unwrap()
        .lower_checked(
            AcceleratorTarget::Device,
            AcceleratorRuntimeCapabilities {
                gpu_driver: true,
                utf8_strings: true,
            },
        )
        .unwrap();
        assert_eq!(iphone.recipe_version, RECIPE_VERSION);
        assert!(iphone.canonical_ir.contains("metal_family=9\n"));
        assert!(iphone.canonical_ir.contains("gpu_cores=6\n"));
        assert!(iphone.canonical_ir.contains("threads_per_group=256\n"));
        assert!(iphone.canonical_ir.contains("total_threads=2048\n"));
        assert!(iphone
            .source
            .contains("MetalDispatchPlan::new(256u32, 8u32, 8192u64)"));
        assert!(iphone
            .source
            .contains("AcceleratorProfile::new(\"A17 Pro\", Some(6u16), Some(9u8))"));

        let watch_entry = find_model_contract(apple_watch_series_9::CONTRACT).unwrap();
        let watch = AcceleratorDeclaration {
            model: apple_watch_series_9::MODEL.to_owned(),
            threads_per_group: 0,
            groups: 0,
            max_buffer_bytes: 0,
        }
        .normalize(watch_entry)
        .unwrap()
        .lower_checked(
            AcceleratorTarget::Device,
            AcceleratorRuntimeCapabilities {
                gpu_driver: false,
                utf8_strings: true,
            },
        )
        .unwrap();
        assert!(watch.canonical_ir.contains("gpu_cores=unpublished\n"));
        assert!(watch.canonical_ir.contains("metal_family=none\n"));
        assert!(watch
            .source
            .contains("AcceleratorProfile::new(\"S9\", None, None)"));
        assert!(!watch.source.contains("make_dispatch_plan"));
    }

    #[test]
    fn generated_factories_compile_and_run() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());

        let iphone_entry = find_model_contract(iphone_15_pro::CONTRACT).unwrap();
        let iphone = AcceleratorDeclaration {
            model: iphone_15_pro::MODEL.to_owned(),
            threads_per_group: 256,
            groups: 8,
            max_buffer_bytes: 8192,
        }
        .normalize(iphone_entry)
        .unwrap()
        .lower_checked(
            AcceleratorTarget::Device,
            AcceleratorRuntimeCapabilities {
                gpu_driver: true,
                utf8_strings: true,
            },
        )
        .unwrap();
        let watch_entry = find_model_contract(apple_watch_series_9::CONTRACT).unwrap();
        let watch = AcceleratorDeclaration {
            model: apple_watch_series_9::MODEL.to_owned(),
            threads_per_group: 0,
            groups: 0,
            max_buffer_bytes: 0,
        }
        .normalize(watch_entry)
        .unwrap()
        .lower_checked(
            AcceleratorTarget::Device,
            AcceleratorRuntimeCapabilities {
                gpu_driver: false,
                utf8_strings: true,
            },
        )
        .unwrap();

        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\n{}\nfn main() {{\n    let plan = iphone_plan().expect(\"dispatch factory should construct plan\");\n    assert_eq!(plan.threads_per_group(), 256);\n    assert_eq!(plan.groups(), 8);\n    assert_eq!(plan.total_threads(), 2048);\n    let profile = iphone_profile().expect(\"profile factory should construct profile\");\n    assert_eq!(profile.chip(), \"A17 Pro\");\n    assert_eq!(profile.gpu_cores(), Some(6));\n    assert_eq!(profile.metal_family(), Some(9));\n    let watch_profile = watch_profile().expect(\"watch factory should construct profile\");\n    assert_eq!(watch_profile.chip(), \"S9\");\n    assert_eq!(watch_profile.gpu_cores(), None);\n    assert_eq!(watch_profile.metal_family(), None);\n}}\n",
            iphone.source.replace("pub fn make_dispatch_plan", "pub fn iphone_plan")
                .replace("pub fn make_accelerator_profile", "pub fn iphone_profile"),
            watch.source.replace("pub fn make_accelerator_profile", "pub fn watch_profile"),
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-accelerator-factory-{}-{nonce}",
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
