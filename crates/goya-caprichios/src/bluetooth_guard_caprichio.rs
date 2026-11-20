//! A bounded Bluetooth security and authorisation capability for iOS and
//! Apple Silicon Mac hosts: the Blue Moon Guard core.
//!
//! The negotiated artifact binds to this crate's process-local Bluetooth
//! runtime. It does not inspect a real permission manager, verify device
//! identity, or prove that a privacy usage description was ever generated.
//! Hosts on Apple hardware report their real permission and storage
//! facilities through [`RuntimeCapabilities`]. Advertised names, service
//! UUIDs and manufacturer identifiers are never treated as proof of
//! identity: only explicit application selection authorises a peripheral,
//! and every connection decision fails closed.

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.bluetooth.guard.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_AUTHORISED_PERIPHERALS: u16 = crate::bluetooth_runtime::MAX_AUTHORISED_PERIPHERALS;
pub const MAX_USAGE_DESCRIPTION_BYTES: usize =
    crate::bluetooth_runtime::MAX_USAGE_DESCRIPTION_BYTES;

pub use crate::bluetooth_runtime::{
    validate_usage_description, ConnectionDecision, GuardSession, PermissionState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothGuardDeclaration {
    /// Application-specific Bluetooth privacy usage description. Empty,
    /// generic and placeholder descriptions are rejected (directive §9.2).
    pub usage_description: String,
    /// Bounded explicitly-authorised peripheral set, from one through
    /// `MAX_AUTHORISED_PERIPHERALS`.
    pub max_authorised_peripherals: u16,
    /// Must be `true`: explicit selection cannot be disabled. A declaration
    /// that asks for promiscuous authorisation is rejected at normalization.
    pub enforce_explicit_selection: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothGuardIr {
    usage_description: String,
    max_authorised_peripherals: u16,
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
    /// Host runtime can report the application's Bluetooth permission state.
    pub bluetooth_permission_query: bool,
    /// Host provides bounded persistent storage for the authorisation ledger.
    pub bluetooth_authorisation_store: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    GenericUsageDescription,
    UsageDescriptionTooLong,
    ZeroAuthorisationBound,
    AuthorisationBoundTooLarge,
    ExplicitSelectionCannotBeDisabled,
    UnsupportedTarget,
    MissingBluetoothPermissionQuery,
    MissingBluetoothAuthorisationStore,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::GenericUsageDescription => {
                "privacy usage description is empty, generic or a placeholder; \
                 explanations must be application-specific"
            }
            Self::UsageDescriptionTooLong => {
                "privacy usage description exceeds the supported bound"
            }
            Self::ZeroAuthorisationBound => "authorised-peripheral bound must be positive",
            Self::AuthorisationBoundTooLarge => {
                "authorised-peripheral bound exceeds the supported bound"
            }
            Self::ExplicitSelectionCannotBeDisabled => {
                "explicit peripheral selection is mandatory and cannot be disabled"
            }
            Self::UnsupportedTarget => {
                "the Bluetooth guard factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingBluetoothPermissionQuery => {
                "target runtime must report the application's Bluetooth permission state"
            }
            Self::MissingBluetoothAuthorisationStore => {
                "target runtime must provide bounded storage for the authorisation ledger"
            }
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled,
/// that any permission was inspected, or that any peripheral was authorised.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothGuardArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl BluetoothGuardDeclaration {
    pub fn normalize(self) -> Result<BluetoothGuardIr, Diagnostic> {
        if !self.enforce_explicit_selection {
            return Err(Diagnostic::ExplicitSelectionCannotBeDisabled);
        }
        validate_usage_description(&self.usage_description).map_err(|error| match error {
            crate::bluetooth_runtime::BluetoothError::GenericUsageDescription => {
                Diagnostic::GenericUsageDescription
            }
            crate::bluetooth_runtime::BluetoothError::UsageDescriptionTooLong => {
                Diagnostic::UsageDescriptionTooLong
            }
            _ => Diagnostic::GenericUsageDescription,
        })?;
        if self.max_authorised_peripherals == 0 {
            return Err(Diagnostic::ZeroAuthorisationBound);
        }
        if self.max_authorised_peripherals > MAX_AUTHORISED_PERIPHERALS {
            return Err(Diagnostic::AuthorisationBoundTooLarge);
        }
        Ok(BluetoothGuardIr {
            usage_description: self.usage_description,
            max_authorised_peripherals: self.max_authorised_peripherals,
        })
    }
}

impl BluetoothGuardIr {
    pub fn usage_description(&self) -> &str {
        &self.usage_description
    }

    pub fn max_authorised_peripherals(&self) -> u16 {
        self.max_authorised_peripherals
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
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=ble_guard_bounded_authorisation\nusage_description_bytes={}\nmax_authorised_peripherals={}\nexplicit_selection=required\nusage_keys_generated=none\nauthorisations_granted=none\n",
            Self::target_label(target),
            self.usage_description.len(),
            self.max_authorised_peripherals
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BluetoothGuardArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.bluetooth_permission_query {
            return Err(Diagnostic::MissingBluetoothPermissionQuery);
        }
        if !capabilities.bluetooth_authorisation_store {
            return Err(Diagnostic::MissingBluetoothAuthorisationStore);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        Ok(BluetoothGuardArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=apple-ble-guard-bounded-authorisation; {} Bluetooth guard session, no permission is inspected, no peripheral is authorised and no identity is verified.\npub fn make_guard_session() -> Result<crate::bluetooth_runtime::GuardSession, crate::bluetooth_runtime::BluetoothError> {{\n    crate::bluetooth_runtime::GuardSession::new({}u16)\n}}\n",
                Self::target_label(target),
                self.max_authorised_peripherals
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        bluetooth_permission_query: true,
        bluetooth_authorisation_store: true,
        utf8_strings: true,
    };

    fn declaration() -> BluetoothGuardDeclaration {
        BluetoothGuardDeclaration {
            usage_description: "Connects to your temperature sensor to show readings.".to_owned(),
            max_authorised_peripherals: 4,
            enforce_explicit_selection: true,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.usage_description(),
            "Connects to your temperature sensor to show readings."
        );
        assert_eq!(ir.max_authorised_peripherals(), 4);
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first
            .canonical_ir
            .contains("session=ble_guard_bounded_authorisation\n"));
        assert!(first.canonical_ir.contains("usage_description_bytes=53\n"));
        assert!(first
            .canonical_ir
            .contains("max_authorised_peripherals=4\n"));
        assert!(first.canonical_ir.contains("explicit_selection=required\n"));
        assert!(first.canonical_ir.contains("usage_keys_generated=none\n"));
        assert!(first.canonical_ir.contains("authorisations_granted=none\n"));
        assert!(first.source.contains("GuardSession::new(4u16)"));
    }

    #[test]
    fn rejects_generic_descriptions_bad_bounds_and_disabled_selection() {
        assert_eq!(
            BluetoothGuardDeclaration {
                usage_description: "uses bluetooth".to_owned(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::GenericUsageDescription)
        );
        assert_eq!(
            BluetoothGuardDeclaration {
                usage_description: String::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::GenericUsageDescription)
        );
        assert_eq!(
            BluetoothGuardDeclaration {
                usage_description: "d".repeat(MAX_USAGE_DESCRIPTION_BYTES + 1),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::UsageDescriptionTooLong)
        );
        assert_eq!(
            BluetoothGuardDeclaration {
                max_authorised_peripherals: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroAuthorisationBound)
        );
        assert_eq!(
            BluetoothGuardDeclaration {
                max_authorised_peripherals: MAX_AUTHORISED_PERIPHERALS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::AuthorisationBoundTooLarge)
        );
        assert_eq!(
            BluetoothGuardDeclaration {
                enforce_explicit_selection: false,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ExplicitSelectionCannotBeDisabled)
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
                    bluetooth_permission_query: false,
                    ..IOS
                },
                Diagnostic::MissingBluetoothPermissionQuery,
            ),
            (
                RuntimeCapabilities {
                    bluetooth_authorisation_store: false,
                    ..IOS
                },
                Diagnostic::MissingBluetoothAuthorisationStore,
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
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nuse bluetooth_runtime::{{BluetoothError, PermissionState}};\nfn main() {{\n    let mut guard = make_guard_session().expect(\"factory should construct session\");\n    assert_eq!(guard.max_authorised_peripherals(), 4);\n    assert_eq!(guard.authorise(\"\"), Err(BluetoothError::EmptyPeripheralId));\n    assert!(!guard.is_authorised(\"AA:BB\"));\n    guard.authorise(\"AA:BB\").unwrap();\n    guard.authorise(\"AA:BB\").unwrap();\n    assert_eq!(guard.authorised_count(), 1);\n    let decision = guard.check_connection(\"AA:BB\", PermissionState::Granted).expect(\"granted and selected\");\n    assert_eq!(decision.peripheral_id(), \"AA:BB\");\n    assert!(decision.explicit_selection());\n    assert_eq!(guard.check_connection(\"CC:DD\", PermissionState::Granted), Err(BluetoothError::NotAuthorised));\n    assert_eq!(guard.check_connection(\"AA:BB\", PermissionState::NotDetermined), Err(BluetoothError::PermissionUndetermined));\n    assert_eq!(guard.check_connection(\"AA:BB\", PermissionState::Denied), Err(BluetoothError::PermissionDenied));\n    assert_eq!(guard.decisions_granted(), 1);\n    assert_eq!(guard.decisions_denied(), 3);\n    guard.validate_advertisement_payload(&[0u8; 31]).unwrap();\n    assert_eq!(guard.validate_advertisement_payload(&[0u8; 32]), Err(BluetoothError::AdvertisementTooLarge));\n    assert_eq!(guard.advertisements_validated(), 1);\n    assert_eq!(guard.advertisements_rejected(), 1);\n    guard.revoke(\"AA:BB\").unwrap();\n    assert!(!guard.is_authorised(\"AA:BB\"));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-guard-factory-{}-{nonce}",
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
            .expect("run generated Bluetooth guard factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
