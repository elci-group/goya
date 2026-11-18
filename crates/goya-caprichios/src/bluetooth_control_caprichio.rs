//! A bounded GATT characteristic-command capability for iOS and Apple
//! Silicon Mac hosts: the Blue Moon Control core.
//!
//! The negotiated artifact binds to this crate's process-local Bluetooth
//! runtime. It does not contact a peripheral, read a characteristic, or
//! prove that Core Bluetooth is linked in a consumer crate. Hosts on Apple
//! hardware report their real GATT facilities through [`RuntimeCapabilities`]
//! and report observed values back through the session's validation surface.
//! A planned or acknowledged write is transport accounting only: it is never
//! reported as physical execution of the command.

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.bluetooth.gatt.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_CHARACTERISTICS: usize = crate::bluetooth_runtime::MAX_CHARACTERISTICS;
pub const MAX_VALUE_BYTES: usize = crate::bluetooth_runtime::MAX_VALUE_BYTES;
pub const MAX_GATT_TIMEOUT_MS: u32 = crate::bluetooth_runtime::MAX_GATT_TIMEOUT_MS;
pub const MAX_SERVICE_UUID_BYTES: usize = crate::bluetooth_runtime::MAX_SERVICE_UUID_BYTES;

pub use crate::bluetooth_runtime::{CharacteristicPolicy, GattAccess, WriteMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothGattDeclaration {
    /// Profile-defined characteristic policies. Every operation fails closed
    /// against the policy's access and value bound.
    pub characteristics: Vec<CharacteristicPolicy>,
    /// Command timeout bound, from one through `MAX_GATT_TIMEOUT_MS`.
    pub max_command_timeout_ms: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothGattIr {
    characteristics: Vec<CharacteristicPolicy>,
    max_command_timeout_ms: u32,
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
    /// Host runtime provides a GATT client surface through its Bluetooth stack.
    pub core_bluetooth_gatt: bool,
    /// Host Bluetooth adapter is powered and available.
    pub bluetooth_powered: bool,
    /// The application-level Bluetooth permission has been granted.
    pub bluetooth_permission: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    NoCharacteristics,
    TooManyCharacteristics,
    DuplicateCharacteristic,
    ZeroCommandTimeout,
    CommandTimeoutTooLarge,
    UnsupportedTarget,
    MissingCoreBluetoothGatt,
    MissingBluetoothPower,
    MissingBluetoothPermission,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoCharacteristics => {
                "at least one characteristic policy is required; GATT operations \
                 are restricted to declared application requirements"
            }
            Self::TooManyCharacteristics => {
                "characteristic policy count exceeds the supported bound"
            }
            Self::DuplicateCharacteristic => {
                "a characteristic UUID was declared more than once"
            }
            Self::ZeroCommandTimeout => "command timeout bound must be positive",
            Self::CommandTimeoutTooLarge => {
                "command timeout bound exceeds the supported bound"
            }
            Self::UnsupportedTarget => {
                "the GATT command factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingCoreBluetoothGatt => {
                "target runtime must provide a GATT client surface"
            }
            Self::MissingBluetoothPower => "target Bluetooth adapter must be powered",
            Self::MissingBluetoothPermission => {
                "the application-level Bluetooth permission must be granted"
            }
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled,
/// that any characteristic operation ran, or that a command physically
/// executed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothGattArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl BluetoothGattDeclaration {
    pub fn normalize(self) -> Result<BluetoothGattIr, Diagnostic> {
        if self.characteristics.is_empty() {
            return Err(Diagnostic::NoCharacteristics);
        }
        if self.characteristics.len() > MAX_CHARACTERISTICS {
            return Err(Diagnostic::TooManyCharacteristics);
        }
        let mut seen = Vec::with_capacity(self.characteristics.len());
        for policy in &self.characteristics {
            if seen.iter().any(|uuid: &String| uuid == policy.uuid()) {
                return Err(Diagnostic::DuplicateCharacteristic);
            }
            seen.push(policy.uuid().to_owned());
        }
        if self.max_command_timeout_ms == 0 {
            return Err(Diagnostic::ZeroCommandTimeout);
        }
        if self.max_command_timeout_ms > MAX_GATT_TIMEOUT_MS {
            return Err(Diagnostic::CommandTimeoutTooLarge);
        }
        Ok(BluetoothGattIr {
            characteristics: self.characteristics,
            max_command_timeout_ms: self.max_command_timeout_ms,
        })
    }
}

impl BluetoothGattIr {
    pub fn characteristics(&self) -> &[CharacteristicPolicy] {
        &self.characteristics
    }

    pub fn max_command_timeout_ms(&self) -> u32 {
        self.max_command_timeout_ms
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::Ios => "ios",
            Target::AppleSiliconMac => "apple_silicon_mac",
            Target::CrossPlatform | Target::Browser | Target::Node => "unsupported",
        }
    }

    fn characteristics_label(&self) -> String {
        self.characteristics
            .iter()
            .map(|policy| {
                format!(
                    "{}:{}:{}",
                    policy.uuid(),
                    policy.access().label(),
                    policy.max_value_bytes()
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=ble_gatt_bounded_commands\nmax_command_timeout_ms={}\ncharacteristics={}\ncommands_issued=none\nphysical_execution=none\n",
            Self::target_label(target),
            self.max_command_timeout_ms,
            self.characteristics_label()
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BluetoothGattArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.core_bluetooth_gatt {
            return Err(Diagnostic::MissingCoreBluetoothGatt);
        }
        if !capabilities.bluetooth_powered {
            return Err(Diagnostic::MissingBluetoothPower);
        }
        if !capabilities.bluetooth_permission {
            return Err(Diagnostic::MissingBluetoothPermission);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        let policy_arguments = self
            .characteristics
            .iter()
            .map(|policy| {
                format!(
                    "crate::bluetooth_runtime::CharacteristicPolicy::new({uuid:?}.to_owned(), crate::bluetooth_runtime::GattAccess::{access}, {bound}usize)?",
                    uuid = policy.uuid(),
                    access = match policy.access() {
                        GattAccess::Read => "Read",
                        GattAccess::Write => "Write",
                        GattAccess::ReadWrite => "ReadWrite",
                    },
                    bound = policy.max_value_bytes()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(BluetoothGattArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=apple-ble-gatt-bounded-commands; {} GATT command session, no characteristic operation runs and no command physically executes.\npub fn make_gatt_session() -> Result<crate::bluetooth_runtime::GattSession, crate::bluetooth_runtime::BluetoothError> {{\n    crate::bluetooth_runtime::GattSession::new(vec![{}], {}u32)\n}}\n",
                Self::target_label(target),
                policy_arguments,
                self.max_command_timeout_ms
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        core_bluetooth_gatt: true,
        bluetooth_powered: true,
        bluetooth_permission: true,
        utf8_strings: true,
    };

    fn declaration() -> BluetoothGattDeclaration {
        BluetoothGattDeclaration {
            characteristics: vec![
                CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::ReadWrite, 512).unwrap(),
                CharacteristicPolicy::new("2A37".to_owned(), GattAccess::Write, 64).unwrap(),
            ],
            max_command_timeout_ms: 30_000,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.characteristics().len(), 2);
        assert_eq!(ir.characteristics()[0].uuid(), "2A6E");
        assert_eq!(ir.characteristics()[0].access(), GattAccess::ReadWrite);
        assert_eq!(ir.max_command_timeout_ms(), 30_000);
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first.canonical_ir.contains("session=ble_gatt_bounded_commands\n"));
        assert!(first
            .canonical_ir
            .contains("characteristics=2A6E:read_write:512,2A37:write:64\n"));
        assert!(first.canonical_ir.contains("commands_issued=none\n"));
        assert!(first.canonical_ir.contains("physical_execution=none\n"));
        assert!(first.source.contains(
            "CharacteristicPolicy::new(\"2A6E\".to_owned(), crate::bluetooth_runtime::GattAccess::ReadWrite, 512usize)?"
        ));
        assert!(first.source.contains(
            "CharacteristicPolicy::new(\"2A37\".to_owned(), crate::bluetooth_runtime::GattAccess::Write, 64usize)?"
        ));
    }

    #[test]
    fn rejects_missing_duplicate_and_overbound_declarations() {
        assert_eq!(
            BluetoothGattDeclaration {
                characteristics: Vec::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NoCharacteristics)
        );
        assert_eq!(
            BluetoothGattDeclaration {
                characteristics: vec![
                    CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Read, 16).unwrap(),
                    CharacteristicPolicy::new("2A6E".to_owned(), GattAccess::Write, 16).unwrap(),
                ],
                max_command_timeout_ms: 30_000,
            }
            .normalize(),
            Err(Diagnostic::DuplicateCharacteristic)
        );
        assert_eq!(
            BluetoothGattDeclaration {
                characteristics: (0..=MAX_CHARACTERISTICS)
                    .map(|index| {
                        CharacteristicPolicy::new(format!("{index:04}"), GattAccess::Read, 16)
                            .unwrap()
                    })
                    .collect(),
                max_command_timeout_ms: 30_000,
            }
            .normalize(),
            Err(Diagnostic::TooManyCharacteristics)
        );
        assert_eq!(
            BluetoothGattDeclaration {
                max_command_timeout_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroCommandTimeout)
        );
        assert_eq!(
            BluetoothGattDeclaration {
                max_command_timeout_ms: MAX_GATT_TIMEOUT_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::CommandTimeoutTooLarge)
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
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    core_bluetooth_gatt: false,
                    ..IOS
                },
                Diagnostic::MissingCoreBluetoothGatt,
            ),
            (
                RuntimeCapabilities {
                    bluetooth_powered: false,
                    ..IOS
                },
                Diagnostic::MissingBluetoothPower,
            ),
            (
                RuntimeCapabilities {
                    bluetooth_permission: false,
                    ..IOS
                },
                Diagnostic::MissingBluetoothPermission,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..IOS
                },
                Diagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::Ios, missing), Err(expected));
        }
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
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nuse bluetooth_runtime::{{BluetoothError, GattAccess, WriteMode}};\nfn main() {{\n    let mut session = make_gatt_session().expect(\"factory should construct session\");\n    assert_eq!(session.max_command_timeout_ms(), 30000);\n    assert_eq!(session.characteristics(), 2);\n    assert_eq!(session.access(\"2A6E\"), Ok(GattAccess::ReadWrite));\n    assert_eq!(session.max_value_bytes(\"2A6E\"), Ok(512));\n    assert_eq!(session.read(\"2A37\"), Err(BluetoothError::UnsupportedOperation));\n    let receipt = session.read(\"2A6E\").expect(\"read permitted\");\n    assert_eq!(receipt.uuid(), \"2A6E\");\n    assert_eq!(receipt.max_value_bytes(), 512);\n    let report = session.write(\"2A6E\", WriteMode::Acknowledged, &[1, 2]).expect(\"write permitted\");\n    assert_eq!(report.mode(), WriteMode::Acknowledged);\n    assert_eq!(report.timeout_ms(), 30000);\n    assert_eq!(session.pending_acknowledgements(), 1);\n    assert_eq!(session.write(\"2A6E\", WriteMode::Acknowledged, &[0u8; 513]), Err(BluetoothError::ValueTooLarge));\n    let unacknowledged = session.write(\"2A37\", WriteMode::Unacknowledged, &[7]).expect(\"write permitted\");\n    assert_eq!(unacknowledged.mode(), WriteMode::Unacknowledged);\n    assert_eq!(session.pending_acknowledgements(), 1);\n    assert_eq!(session.acknowledge(unacknowledged.command_id()), Err(BluetoothError::UnknownCommand));\n    session.acknowledge(report.command_id()).unwrap();\n    assert_eq!(session.pending_acknowledgements(), 0);\n    session.validate_value(\"2A37\", &[0u8; 64]).unwrap();\n    assert_eq!(session.validate_value(\"2A37\", &[0u8; 65]), Err(BluetoothError::ValueTooLarge));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-gatt-factory-{}-{nonce}",
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
            .expect("run generated Bluetooth GATT factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
