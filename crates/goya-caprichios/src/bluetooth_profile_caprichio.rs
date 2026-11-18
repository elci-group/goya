//! A bounded versioned device-profile capability for iOS and Apple Silicon
//! Mac hosts: the Blue Moon Profile core.
//!
//! The negotiated artifact binds to this crate's process-local Bluetooth
//! runtime. It does not discover a device, enumerate real services, or prove
//! that any peripheral matches a profile. Hosts on Apple hardware report
//! their real facilities through [`RuntimeCapabilities`]. Profiles are
//! validated against the category invariants the applicable Bluetooth
//! specifications define (standard characteristic UUIDs and properties);
//! actual device behaviour is never verified here.

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.bluetooth.profile.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_PROFILES: usize = crate::bluetooth_runtime::MAX_PROFILES;
pub const MAX_PROFILE_ID_BYTES: usize = crate::bluetooth_runtime::MAX_PROFILE_ID_BYTES;
pub const MAX_PROFILE_CHARACTERISTICS: usize =
    crate::bluetooth_runtime::MAX_PROFILE_CHARACTERISTICS;

pub use crate::bluetooth_runtime::{
    DeviceProfileRecord, ProfileCategory, ProfileCharacteristic, ProfileProperties, ProfileRegistry,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothProfileDeclaration {
    /// Dotted ASCII profile identifier, unique in the negotiated registry.
    pub profile_id: String,
    /// Versioned profile category.
    pub category: ProfileCategory,
    /// Three dot-separated numeric segments (`MAJOR.MINOR.PATCH`).
    pub version: String,
    /// Characteristics the category specification requires.
    pub required: Vec<ProfileCharacteristic>,
    /// Characteristics the device may additionally expose.
    pub optional: Vec<ProfileCharacteristic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothProfileIr {
    record: DeviceProfileRecord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// iOS device with a Bluetooth-capable radio.
    Ios,
    /// Apple Silicon Mac with a Bluetooth-capable radio.
    AppleSiliconMac,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    BadProfileId,
    ProfileIdTooLong,
    BadProfileVersion,
    NoCharacteristics,
    TooManyCharacteristics,
    DuplicateCharacteristic,
    CategoryInvariantViolated,
    UnsupportedTarget,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadProfileId => {
                "profile identifier must be a non-empty dotted ASCII name within the bound"
            }
            Self::ProfileIdTooLong => "profile identifier exceeds the supported bound",
            Self::BadProfileVersion => {
                "profile version must be three dot-separated numeric segments"
            }
            Self::NoCharacteristics => {
                "this category requires at least one required characteristic"
            }
            Self::TooManyCharacteristics => {
                "profile characteristic count exceeds the supported bound"
            }
            Self::DuplicateCharacteristic => {
                "a characteristic UUID was declared more than once"
            }
            Self::CategoryInvariantViolated => {
                "profile does not satisfy its category's documented invariants"
            }
            Self::UnsupportedTarget => {
                "the Bluetooth profile factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled,
/// that any device was classified, or that a real peripheral matches the
/// profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothProfileArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

/// The directive's initial reference profiles: one versioned definition per
/// category, each satisfying its category invariants. Reference profiles are
/// starting points for declarations, not claims about any specific device.
pub fn reference_profiles() -> Vec<DeviceProfileRecord> {
    fn required(
        uuid: &str,
        read: bool,
        write: bool,
        notify: bool,
        bound: usize,
    ) -> ProfileCharacteristic {
        ProfileCharacteristic::new(
            uuid.to_owned(),
            ProfileProperties::new(read, write, notify, false),
            bound,
        )
        .unwrap()
    }
    fn optional(
        uuid: &str,
        read: bool,
        write: bool,
        notify: bool,
        bound: usize,
    ) -> ProfileCharacteristic {
        required(uuid, read, write, notify, bound)
    }
    fn record(
        id: &str,
        category: ProfileCategory,
        required: Vec<ProfileCharacteristic>,
        optional: Vec<ProfileCharacteristic>,
    ) -> DeviceProfileRecord {
        DeviceProfileRecord::new(id.to_owned(), category, "1.0.0".to_owned(), required, optional)
            .unwrap()
    }
    vec![
        record(
            "bluetooth.generic-gatt",
            ProfileCategory::GenericGatt,
            vec![required("2A00", true, false, false, 64)],
            vec![],
        ),
        record(
            "bluetooth.battery",
            ProfileCategory::Battery,
            vec![required("2A19", true, false, false, 1)],
            vec![],
        ),
        record(
            "bluetooth.environmental",
            ProfileCategory::Environmental,
            vec![required("2A6E", true, false, false, 512)],
            vec![optional("2A6F", true, false, false, 512)],
        ),
        record(
            "bluetooth.heart-rate",
            ProfileCategory::HeartRate,
            vec![required("2A37", false, false, true, 512)],
            vec![
                optional("2A38", true, false, false, 16),
                optional("2A39", false, true, false, 16),
            ],
        ),
        record(
            "bluetooth.hid",
            ProfileCategory::Hid,
            vec![required("2A4D", false, false, true, 512)],
            vec![optional("2A4E", true, false, false, 8)],
        ),
        record(
            "bluetooth.controller",
            ProfileCategory::Controller,
            vec![required("2A4D", false, false, true, 512)],
            vec![optional("2A4E", true, false, false, 8)],
        ),
        record(
            "bluetooth.accessibility-switch",
            ProfileCategory::AccessibilitySwitch,
            vec![required("FF00", false, false, true, 16)],
            vec![optional("FF01", false, true, false, 16)],
        ),
        record(
            "bluetooth.serial",
            ProfileCategory::Serial,
            vec![
                required("6E400002", false, true, false, 512),
                required("6E400003", false, false, true, 512),
            ],
            vec![],
        ),
        record(
            "bluetooth.audio",
            ProfileCategory::Audio,
            vec![],
            vec![],
        ),
        record(
            "bluetooth.custom",
            ProfileCategory::Custom,
            vec![required("FF10", true, true, false, 256)],
            vec![optional("FF11", false, false, true, 256)],
        ),
    ]
}

impl BluetoothProfileDeclaration {
    pub fn normalize(self) -> Result<BluetoothProfileIr, Diagnostic> {
        let record = DeviceProfileRecord::new(
            self.profile_id,
            self.category,
            self.version,
            self.required,
            self.optional,
        )
        .map_err(|error| match error {
            crate::bluetooth_runtime::BluetoothError::BadProfileId => Diagnostic::BadProfileId,
            crate::bluetooth_runtime::BluetoothError::ProfileIdTooLong => {
                Diagnostic::ProfileIdTooLong
            }
            crate::bluetooth_runtime::BluetoothError::BadProfileVersion => {
                Diagnostic::BadProfileVersion
            }
            crate::bluetooth_runtime::BluetoothError::TooManyCharacteristics => {
                Diagnostic::TooManyCharacteristics
            }
            crate::bluetooth_runtime::BluetoothError::DuplicateCharacteristic => {
                Diagnostic::DuplicateCharacteristic
            }
            _ => Diagnostic::BadProfileId,
        })?;
        // Category-specific structural rules (generic GATT and custom
        // profiles need at least one required characteristic) surface here
        // so a declaration fails before negotiation, not only at registration.
        let structural = match record.category() {
            ProfileCategory::GenericGatt | ProfileCategory::Custom => {
                if record.required().is_empty() {
                    Err(Diagnostic::NoCharacteristics)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        };
        structural?;
        record
            .validate_category_invariants()
            .map_err(|_| Diagnostic::CategoryInvariantViolated)?;
        Ok(BluetoothProfileIr { record })
    }
}

impl BluetoothProfileIr {
    pub fn record(&self) -> &DeviceProfileRecord {
        &self.record
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::Ios => "ios",
            Target::AppleSiliconMac => "apple_silicon_mac",
            Target::CrossPlatform | Target::Browser | Target::Node => "unsupported",
        }
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=ble_profile_bounded_classification\nprofile_id={}\ncategory={}\nversion={}\nrequired={}\noptional={}\nprofiles_registered=none\ndevices_classified=none\nspec_validation=category_invariants_only\n",
            Self::target_label(target),
            self.record.id(),
            self.record.category(),
            self.record.version(),
            self.record.required().len(),
            self.record.optional().len()
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BluetoothProfileArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        let record = &self.record;
        fn characteristic_argument(characteristic: &ProfileCharacteristic) -> String {
            let properties = characteristic.properties();
            format!(
                "crate::bluetooth_runtime::ProfileCharacteristic::new({uuid:?}.to_owned(), crate::bluetooth_runtime::ProfileProperties::new({read}, {write}, {notify}, {indicate}), {bound}usize)?,",
                uuid = characteristic.uuid(),
                read = properties.read,
                write = properties.write,
                notify = properties.notify,
                indicate = properties.indicate,
                bound = characteristic.max_value_bytes()
            )
        }
        let required_arguments = record
            .required()
            .iter()
            .map(|characteristic| characteristic_argument(characteristic))
            .collect::<Vec<_>>()
            .join("\n    ");
        let optional_arguments = record
            .optional()
            .iter()
            .map(|characteristic| characteristic_argument(characteristic))
            .collect::<Vec<_>>()
            .join("\n    ");
        Ok(BluetoothProfileArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=apple-ble-profile-bounded-classification; {} profile registry, no device is discovered or classified.\npub fn make_profile_registry() -> Result<crate::bluetooth_runtime::ProfileRegistry, crate::bluetooth_runtime::BluetoothError> {{\n    let mut registry = crate::bluetooth_runtime::ProfileRegistry::new({max}usize)?;\n    registry.register(crate::bluetooth_runtime::DeviceProfileRecord::new({id:?}.to_owned(), crate::bluetooth_runtime::ProfileCategory::{category}, {version:?}.to_owned(), vec![\n    {required_arguments}\n    ], vec![\n    {optional_arguments}\n    ])?)?;\n    Ok(registry)\n}}\n",
                Self::target_label(target),
                max = MAX_PROFILES,
                id = record.id(),
                category = match record.category() {
                    ProfileCategory::GenericGatt => "GenericGatt",
                    ProfileCategory::Battery => "Battery",
                    ProfileCategory::Environmental => "Environmental",
                    ProfileCategory::HeartRate => "HeartRate",
                    ProfileCategory::Hid => "Hid",
                    ProfileCategory::Controller => "Controller",
                    ProfileCategory::AccessibilitySwitch => "AccessibilitySwitch",
                    ProfileCategory::Serial => "Serial",
                    ProfileCategory::Audio => "Audio",
                    ProfileCategory::Custom => "Custom",
                },
                version = record.version(),
                required_arguments,
                optional_arguments
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities { utf8_strings: true };

    fn required(
        uuid: &str,
        read: bool,
        write: bool,
        notify: bool,
        bound: usize,
    ) -> ProfileCharacteristic {
        ProfileCharacteristic::new(
            uuid.to_owned(),
            ProfileProperties::new(read, write, notify, false),
            bound,
        )
        .unwrap()
    }

    fn declaration() -> BluetoothProfileDeclaration {
        BluetoothProfileDeclaration {
            profile_id: "com.goya.profile.test".to_owned(),
            category: ProfileCategory::Battery,
            version: "1.0.0".to_owned(),
            required: vec![required("2A19", true, false, false, 1)],
            optional: vec![],
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.record().id(), "com.goya.profile.test");
        assert_eq!(ir.record().category(), ProfileCategory::Battery);
        assert_eq!(ir.record().version(), "1.0.0");
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first
            .canonical_ir
            .contains("session=ble_profile_bounded_classification\n"));
        assert!(first.canonical_ir.contains("profile_id=com.goya.profile.test\n"));
        assert!(first.canonical_ir.contains("category=battery\n"));
        assert!(first.canonical_ir.contains("version=1.0.0\n"));
        assert!(first.canonical_ir.contains("required=1\n"));
        assert!(first.canonical_ir.contains("optional=0\n"));
        assert!(first.canonical_ir.contains("profiles_registered=none\n"));
        assert!(first.canonical_ir.contains("devices_classified=none\n"));
        assert!(first
            .canonical_ir
            .contains("spec_validation=category_invariants_only\n"));
        assert!(first.source.contains(
            "DeviceProfileRecord::new(\"com.goya.profile.test\".to_owned(), crate::bluetooth_runtime::ProfileCategory::Battery, \"1.0.0\".to_owned()"
        ));
        assert!(first
            .source
            .contains("ProfileCharacteristic::new(\"2A19\".to_owned(), crate::bluetooth_runtime::ProfileProperties::new(true, false, false, false), 1usize)?"));
    }

    #[test]
    fn reference_profiles_normalize_against_their_categories() {
        let mut registry = ProfileRegistry::new(MAX_PROFILES).unwrap();
        for reference in reference_profiles() {
            let declaration = BluetoothProfileDeclaration {
                profile_id: reference.id().to_owned(),
                category: reference.category(),
                version: reference.version().to_owned(),
                required: reference.required().to_vec(),
                optional: reference.optional().to_vec(),
            };
            let ir = declaration.normalize().unwrap();
            assert_eq!(ir.record(), &reference);
            registry.register(reference).unwrap();
        }
        assert_eq!(reference_profiles().len(), 10);
        assert_eq!(registry.len(), 10);
        assert_eq!(
            registry.profile("bluetooth.environmental").unwrap().category(),
            ProfileCategory::Environmental
        );
        assert_eq!(
            registry.profile("bluetooth.accessibility-switch").unwrap().category(),
            ProfileCategory::AccessibilitySwitch
        );
    }

    #[test]
    fn rejects_bad_ids_versions_and_invariant_violations() {
        assert_eq!(
            BluetoothProfileDeclaration {
                profile_id: "bad id!".to_owned(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadProfileId)
        );
        assert_eq!(
            BluetoothProfileDeclaration {
                version: "1.0".to_owned(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadProfileVersion)
        );
        assert_eq!(
            BluetoothProfileDeclaration {
                required: vec![],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::CategoryInvariantViolated)
        );
        // Generic GATT needs at least one required characteristic.
        assert_eq!(
            BluetoothProfileDeclaration {
                category: ProfileCategory::GenericGatt,
                required: vec![],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NoCharacteristics)
        );
        assert_eq!(
            BluetoothProfileDeclaration {
                required: vec![required("2A19", false, true, false, 1)],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::CategoryInvariantViolated)
        );
        assert_eq!(
            BluetoothProfileDeclaration {
                required: vec![required("2A19", true, false, false, 1)],
                optional: vec![required("2A19", true, false, false, 1)],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::DuplicateCharacteristic)
        );
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration().normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, IOS),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(Target::Ios, RuntimeCapabilities::default()),
            Err(Diagnostic::MissingUtf8Strings)
        );
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_bluetooth_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::Ios, IOS)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bluetooth_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nuse bluetooth_runtime::{{BluetoothError, ProfileCategory}};\nfn main() {{\n    let mut registry = make_profile_registry().expect(\"factory should construct registry\");\n    assert_eq!(registry.max_profiles(), 16);\n    let profile = registry.profile(\"com.goya.profile.test\").expect(\"profile registered\");\n    assert_eq!(profile.category(), ProfileCategory::Battery);\n    assert_eq!(profile.version(), \"1.0.0\");\n    assert_eq!(profile.required().len(), 1);\n    assert_eq!(profile.required()[0].uuid(), \"2A19\");\n    assert!(profile.required()[0].properties().read);\n    assert_eq!(registry.profile(\"com.goya.missing\"), Err(BluetoothError::UnknownProfile));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-profile-factory-{}-{nonce}",
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
            .expect("run generated Bluetooth profile factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
