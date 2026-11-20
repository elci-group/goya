//! A composition facade for the Blue Moon Bluetooth family: one negotiated
//! artifact that pairs the bounded authorisation guard with the bounded BLE
//! central scan-and-link session.
//!
//! The guard declaration is validated before any central bound is examined,
//! the guard facilities are required before the central facilities, and the
//! generated factory constructs the guard before the central session and
//! fails closed through `?`: no session pair can be constructed unless the
//! guard declaration normalized. The artifact binds to this crate's
//! process-local Bluetooth runtime. It does not inspect a permission
//! manager, drive a radio, or contact a peripheral, and successful
//! construction is not proof that any permission was inspected or that any
//! peripheral was authorised. Hosts on Apple hardware report their real
//! facilities through [`RuntimeCapabilities`].

use std::fmt;

pub use crate::bluetooth_guard_caprichio::{BluetoothGuardDeclaration, BluetoothGuardIr};

pub const CONTRACT: &str = "goya.hardware.bluetooth.session.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_AUTHORISED_PERIPHERALS: u16 =
    crate::bluetooth_guard_caprichio::MAX_AUTHORISED_PERIPHERALS;
pub const MAX_USAGE_DESCRIPTION_BYTES: usize =
    crate::bluetooth_guard_caprichio::MAX_USAGE_DESCRIPTION_BYTES;
pub const MAX_SCAN_MS: u32 = crate::bluetooth_runtime::MAX_SCAN_MS;
pub const MAX_SERVICE_UUIDS: usize = crate::bluetooth_runtime::MAX_SERVICE_UUIDS;
pub const MAX_SERVICE_UUID_BYTES: usize = crate::bluetooth_runtime::MAX_SERVICE_UUID_BYTES;
pub const MAX_PERIPHERALS: u16 = crate::bluetooth_runtime::MAX_PERIPHERALS;
pub const MAX_CONNECTION_TIMEOUT_MS: u32 = crate::bluetooth_runtime::MAX_CONNECTION_TIMEOUT_MS;
pub const MAX_RECONNECT_ATTEMPTS: u8 = crate::bluetooth_runtime::MAX_RECONNECT_ATTEMPTS;

/// A validated guard declaration paired with the central scan-and-link
/// bounds. Scanning stays restricted to declared service UUID filters and
/// reconnection stays bounded; the guard half is mandatory and is validated
/// first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothSessionDeclaration {
    /// The mandatory Blue Moon Guard declaration: an application-specific
    /// privacy usage description, a bounded authorised-peripheral set and
    /// mandatory explicit selection. Validated before every central bound.
    pub guard: BluetoothGuardDeclaration,
    /// Service UUID filters the scan is restricted to. At least one filter is
    /// required: scanning is restricted to declared application requirements.
    pub service_uuids: Vec<String>,
    /// Discovery timeout bound, from one through `MAX_SCAN_MS`.
    pub max_scan_ms: u32,
    /// Bounded tracked-peripheral set, from one through `MAX_PERIPHERALS`.
    pub max_peripherals: u16,
    /// Connection timeout bound, from one through `MAX_CONNECTION_TIMEOUT_MS`.
    pub connection_timeout_ms: u32,
    /// Bounded reconnection attempts per peripheral; zero disables
    /// reconnection.
    pub max_reconnect_attempts: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothSessionIr {
    guard: BluetoothGuardIr,
    service_uuids: Vec<String>,
    max_scan_ms: u32,
    max_peripherals: u16,
    connection_timeout_ms: u32,
    max_reconnect_attempts: u8,
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

/// Explicitly reported host facilities required by this lowering. The guard
/// facilities come first and the central facilities after them; the default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host runtime can report the application's Bluetooth permission state.
    pub bluetooth_permission_query: bool,
    /// Host provides bounded persistent storage for the authorisation ledger.
    pub bluetooth_authorisation_store: bool,
    /// Host runtime provides a BLE central role through its Bluetooth stack.
    pub core_bluetooth_central: bool,
    /// Host Bluetooth adapter is powered and available.
    pub bluetooth_powered: bool,
    /// The application-level Bluetooth permission has been granted.
    pub bluetooth_permission: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    /// The guard declaration was rejected. The wrapped kinds are the guard
    /// contract's own diagnostics; the guard is always validated before any
    /// central bound and before any facility check.
    Guard(crate::bluetooth_guard_caprichio::Diagnostic),
    NoServiceUuids,
    EmptyServiceUuid,
    ServiceUuidTooLong,
    TooManyServiceUuids,
    ZeroScanBound,
    ScanBoundTooLarge,
    ZeroPeripheralBound,
    PeripheralBoundTooLarge,
    ZeroConnectionTimeout,
    ConnectionTimeoutTooLarge,
    ReconnectBoundTooLarge,
    UnsupportedTarget,
    MissingBluetoothPermissionQuery,
    MissingBluetoothAuthorisationStore,
    MissingCoreBluetoothCentral,
    MissingBluetoothPower,
    MissingBluetoothPermission,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Guard(diagnostic) => diagnostic.fmt(f),
            Self::NoServiceUuids => f.write_str(
                "at least one service UUID filter is required; scans are restricted \
                 to declared application requirements",
            ),
            Self::EmptyServiceUuid => f.write_str("service UUID filter must not be empty"),
            Self::ServiceUuidTooLong => {
                f.write_str("service UUID filter exceeds the supported bound")
            }
            Self::TooManyServiceUuids => {
                f.write_str("service UUID filter count exceeds the supported bound")
            }
            Self::ZeroScanBound => f.write_str("discovery timeout bound must be positive"),
            Self::ScanBoundTooLarge => {
                f.write_str("discovery timeout bound exceeds the supported bound")
            }
            Self::ZeroPeripheralBound => f.write_str("peripheral bound must be positive"),
            Self::PeripheralBoundTooLarge => {
                f.write_str("peripheral bound exceeds the supported bound")
            }
            Self::ZeroConnectionTimeout => f.write_str("connection timeout bound must be positive"),
            Self::ConnectionTimeoutTooLarge => {
                f.write_str("connection timeout bound exceeds the supported bound")
            }
            Self::ReconnectBoundTooLarge => {
                f.write_str("reconnect attempt bound exceeds the supported bound")
            }
            Self::UnsupportedTarget => f.write_str(
                "the Bluetooth session factory supports only iOS and Apple Silicon Mac targets",
            ),
            Self::MissingBluetoothPermissionQuery => f.write_str(
                "target runtime must report the application's Bluetooth permission state",
            ),
            Self::MissingBluetoothAuthorisationStore => f.write_str(
                "target runtime must provide bounded storage for the authorisation ledger",
            ),
            Self::MissingCoreBluetoothCentral => {
                f.write_str("target runtime must provide a BLE central role")
            }
            Self::MissingBluetoothPower => f.write_str("target Bluetooth adapter must be powered"),
            Self::MissingBluetoothPermission => {
                f.write_str("the application-level Bluetooth permission must be granted")
            }
            Self::MissingUtf8Strings => f.write_str("target runtime must provide UTF-8 strings"),
        }
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled,
/// that any permission was inspected, that a radio was driven, or that any
/// peripheral was contacted or authorised.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothSessionArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl BluetoothSessionDeclaration {
    /// Normalize the guard declaration first, then the central bounds. A
    /// rejected guard short-circuits: no central bound is examined.
    pub fn normalize(self) -> Result<BluetoothSessionIr, Diagnostic> {
        let guard = self.guard.normalize().map_err(Diagnostic::Guard)?;
        if self.service_uuids.is_empty() {
            return Err(Diagnostic::NoServiceUuids);
        }
        if self.service_uuids.len() > MAX_SERVICE_UUIDS {
            return Err(Diagnostic::TooManyServiceUuids);
        }
        if self.service_uuids.iter().any(|uuid| uuid.is_empty()) {
            return Err(Diagnostic::EmptyServiceUuid);
        }
        if self
            .service_uuids
            .iter()
            .any(|uuid| uuid.len() > MAX_SERVICE_UUID_BYTES)
        {
            return Err(Diagnostic::ServiceUuidTooLong);
        }
        if self.max_scan_ms == 0 {
            return Err(Diagnostic::ZeroScanBound);
        }
        if self.max_scan_ms > MAX_SCAN_MS {
            return Err(Diagnostic::ScanBoundTooLarge);
        }
        if self.max_peripherals == 0 {
            return Err(Diagnostic::ZeroPeripheralBound);
        }
        if self.max_peripherals > MAX_PERIPHERALS {
            return Err(Diagnostic::PeripheralBoundTooLarge);
        }
        if self.connection_timeout_ms == 0 {
            return Err(Diagnostic::ZeroConnectionTimeout);
        }
        if self.connection_timeout_ms > MAX_CONNECTION_TIMEOUT_MS {
            return Err(Diagnostic::ConnectionTimeoutTooLarge);
        }
        if self.max_reconnect_attempts > MAX_RECONNECT_ATTEMPTS {
            return Err(Diagnostic::ReconnectBoundTooLarge);
        }
        Ok(BluetoothSessionIr {
            guard,
            service_uuids: self.service_uuids,
            max_scan_ms: self.max_scan_ms,
            max_peripherals: self.max_peripherals,
            connection_timeout_ms: self.connection_timeout_ms,
            max_reconnect_attempts: self.max_reconnect_attempts,
        })
    }
}

impl BluetoothSessionIr {
    pub fn guard(&self) -> &BluetoothGuardIr {
        &self.guard
    }

    pub fn service_uuids(&self) -> &[String] {
        &self.service_uuids
    }

    pub fn max_scan_ms(&self) -> u32 {
        self.max_scan_ms
    }

    pub fn max_peripherals(&self) -> u16 {
        self.max_peripherals
    }

    pub fn connection_timeout_ms(&self) -> u32 {
        self.connection_timeout_ms
    }

    pub fn max_reconnect_attempts(&self) -> u8 {
        self.max_reconnect_attempts
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
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=bluemoon_bounded_session\nusage_description_bytes={}\nmax_authorised_peripherals={}\nexplicit_selection=required\nmax_scan_ms={}\nservice_uuids={}\nmax_peripherals={}\nconnection_timeout_ms={}\nmax_reconnect_attempts={}\nauthorisations_granted=none\nperipherals_connected=none\n",
            Self::target_label(target),
            self.guard.usage_description().len(),
            self.guard.max_authorised_peripherals(),
            self.max_scan_ms,
            self.service_uuids.join(","),
            self.max_peripherals,
            self.connection_timeout_ms,
            self.max_reconnect_attempts
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BluetoothSessionArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.bluetooth_permission_query {
            return Err(Diagnostic::MissingBluetoothPermissionQuery);
        }
        if !capabilities.bluetooth_authorisation_store {
            return Err(Diagnostic::MissingBluetoothAuthorisationStore);
        }
        if !capabilities.core_bluetooth_central {
            return Err(Diagnostic::MissingCoreBluetoothCentral);
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
        let uuid_arguments = self
            .service_uuids
            .iter()
            .map(|uuid| format!("{uuid:?}.to_owned()"))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(BluetoothSessionArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=bluemoon-bounded-session; {} Bluetooth guard and central session, no permission is inspected, no radio is driven and no peripheral is contacted.\npub fn make_bluemoon_session() -> Result<(crate::bluetooth_runtime::GuardSession, crate::bluetooth_runtime::CentralSession), crate::bluetooth_runtime::BluetoothError> {{\n    let guard = crate::bluetooth_runtime::GuardSession::new({}u16)?;\n    let central = crate::bluetooth_runtime::CentralSession::new(vec![{}], {}u32, {}u16, {}u32, {}u8)?;\n    Ok((guard, central))\n}}\n",
                Self::target_label(target),
                self.guard.max_authorised_peripherals(),
                uuid_arguments,
                self.max_scan_ms,
                self.max_peripherals,
                self.connection_timeout_ms,
                self.max_reconnect_attempts
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bluetooth_guard_caprichio::Diagnostic as GuardDiagnostic;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        bluetooth_permission_query: true,
        bluetooth_authorisation_store: true,
        core_bluetooth_central: true,
        bluetooth_powered: true,
        bluetooth_permission: true,
        utf8_strings: true,
    };

    fn guard_declaration() -> BluetoothGuardDeclaration {
        BluetoothGuardDeclaration {
            usage_description: "Connects to your temperature sensor to show readings.".to_owned(),
            max_authorised_peripherals: 4,
            enforce_explicit_selection: true,
        }
    }

    fn declaration() -> BluetoothSessionDeclaration {
        BluetoothSessionDeclaration {
            guard: guard_declaration(),
            service_uuids: vec!["181A".to_owned()],
            max_scan_ms: 10_000,
            max_peripherals: 2,
            connection_timeout_ms: 10_000,
            max_reconnect_attempts: 2,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.guard().usage_description(),
            "Connects to your temperature sensor to show readings."
        );
        assert_eq!(ir.guard().max_authorised_peripherals(), 4);
        assert_eq!(ir.service_uuids(), &["181A".to_owned()]);
        assert_eq!(ir.max_scan_ms(), 10_000);
        assert_eq!(ir.max_peripherals(), 2);
        assert_eq!(ir.connection_timeout_ms(), 10_000);
        assert_eq!(ir.max_reconnect_attempts(), 2);
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert_eq!(first.recipe_version, RECIPE_VERSION);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first
            .canonical_ir
            .contains("session=bluemoon_bounded_session\n"));
        assert!(first.canonical_ir.contains("usage_description_bytes=53\n"));
        assert!(first
            .canonical_ir
            .contains("max_authorised_peripherals=4\n"));
        assert!(first.canonical_ir.contains("explicit_selection=required\n"));
        assert!(first.canonical_ir.contains("max_scan_ms=10000\n"));
        assert!(first.canonical_ir.contains("service_uuids=181A\n"));
        assert!(first.canonical_ir.contains("max_peripherals=2\n"));
        assert!(first.canonical_ir.contains("connection_timeout_ms=10000\n"));
        assert!(first.canonical_ir.contains("max_reconnect_attempts=2\n"));
        assert!(first.canonical_ir.contains("authorisations_granted=none\n"));
        assert!(first
            .canonical_ir
            .ends_with("authorisations_granted=none\nperipherals_connected=none\n"));
        assert!(first.source.contains("pub fn make_bluemoon_session()"));
        assert!(first
            .source
            .contains("let guard = crate::bluetooth_runtime::GuardSession::new(4u16)?;"));
        assert!(first.source.contains(
            "let central = crate::bluetooth_runtime::CentralSession::new(vec![\"181A\".to_owned()], 10000u32, 2u16, 10000u32, 2u8)?;"
        ));
        assert!(first.source.contains("Ok((guard, central))"));
    }

    #[test]
    fn multiple_filters_keep_canonical_order_and_apple_silicon_lowers_with_its_own_label() {
        let declaration = BluetoothSessionDeclaration {
            service_uuids: vec!["181A".to_owned(), "180F".to_owned()],
            ..declaration()
        };
        let artifact = declaration
            .normalize()
            .unwrap()
            .lower_checked(Target::AppleSiliconMac, IOS)
            .unwrap();
        assert!(artifact.canonical_ir.contains("target=apple_silicon_mac\n"));
        assert!(artifact.canonical_ir.contains("service_uuids=181A,180F\n"));
        assert!(artifact
            .source
            .contains("apple_silicon_mac Bluetooth guard and central session"));
        assert!(artifact.source.contains("\"180F\".to_owned()"));
    }

    #[test]
    fn rejects_invalid_guard_declarations_before_central_bounds() {
        assert_eq!(
            BluetoothSessionDeclaration {
                guard: BluetoothGuardDeclaration {
                    usage_description: "uses bluetooth".to_owned(),
                    ..guard_declaration()
                },
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::Guard(GuardDiagnostic::GenericUsageDescription))
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                guard: BluetoothGuardDeclaration {
                    usage_description: String::new(),
                    ..guard_declaration()
                },
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::Guard(GuardDiagnostic::GenericUsageDescription))
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                guard: BluetoothGuardDeclaration {
                    usage_description: "d".repeat(MAX_USAGE_DESCRIPTION_BYTES + 1),
                    ..guard_declaration()
                },
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::Guard(GuardDiagnostic::UsageDescriptionTooLong))
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                guard: BluetoothGuardDeclaration {
                    max_authorised_peripherals: 0,
                    ..guard_declaration()
                },
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::Guard(GuardDiagnostic::ZeroAuthorisationBound))
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                guard: BluetoothGuardDeclaration {
                    max_authorised_peripherals: MAX_AUTHORISED_PERIPHERALS + 1,
                    ..guard_declaration()
                },
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::Guard(
                GuardDiagnostic::AuthorisationBoundTooLarge
            ))
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                guard: BluetoothGuardDeclaration {
                    enforce_explicit_selection: false,
                    ..guard_declaration()
                },
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::Guard(
                GuardDiagnostic::ExplicitSelectionCannotBeDisabled
            ))
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                guard: BluetoothGuardDeclaration {
                    usage_description: "uses bluetooth".to_owned(),
                    ..guard_declaration()
                },
                service_uuids: Vec::new(),
                max_scan_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::Guard(GuardDiagnostic::GenericUsageDescription))
        );
    }

    #[test]
    fn rejects_unfiltered_scans_and_bad_bounds() {
        assert_eq!(
            BluetoothSessionDeclaration {
                service_uuids: Vec::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NoServiceUuids)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                service_uuids: vec![String::new()],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::EmptyServiceUuid)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                service_uuids: vec!["u".repeat(MAX_SERVICE_UUID_BYTES + 1)],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ServiceUuidTooLong)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                service_uuids: vec!["a".to_owned(); MAX_SERVICE_UUIDS + 1],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::TooManyServiceUuids)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                max_scan_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroScanBound)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                max_scan_ms: MAX_SCAN_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ScanBoundTooLarge)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                max_peripherals: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroPeripheralBound)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                max_peripherals: MAX_PERIPHERALS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::PeripheralBoundTooLarge)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                connection_timeout_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroConnectionTimeout)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                connection_timeout_ms: MAX_CONNECTION_TIMEOUT_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ConnectionTimeoutTooLarge)
        );
        assert_eq!(
            BluetoothSessionDeclaration {
                max_reconnect_attempts: MAX_RECONNECT_ATTEMPTS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ReconnectBoundTooLarge)
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
                    core_bluetooth_central: false,
                    ..IOS
                },
                Diagnostic::MissingCoreBluetoothCentral,
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
        assert_eq!(
            ir.lower_checked(Target::Ios, RuntimeCapabilities::default()),
            Err(Diagnostic::MissingBluetoothPermissionQuery)
        );
        assert_eq!(
            ir.lower_checked(
                Target::Ios,
                RuntimeCapabilities {
                    bluetooth_permission_query: false,
                    core_bluetooth_central: false,
                    ..IOS
                }
            ),
            Err(Diagnostic::MissingBluetoothPermissionQuery)
        );
        assert_eq!(
            ir.lower_checked(
                Target::Ios,
                RuntimeCapabilities {
                    bluetooth_authorisation_store: false,
                    bluetooth_powered: false,
                    ..IOS
                }
            ),
            Err(Diagnostic::MissingBluetoothAuthorisationStore)
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
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nfn main() {{\n    let (mut guard, mut session) = make_bluemoon_session().expect(\"factory should construct session pair\");\n    assert_eq!(guard.max_authorised_peripherals(), 4);\n    assert!(!guard.is_authorised(\"AA:BB\"));\n    guard.authorise(\"AA:BB\").expect(\"explicit selection authorises\");\n    assert_eq!(guard.authorised_count(), 1);\n    let decision = guard.check_connection(\"AA:BB\", bluetooth_runtime::PermissionState::Granted).expect(\"granted and explicitly selected\");\n    assert_eq!(decision.peripheral_id(), \"AA:BB\");\n    assert!(decision.explicit_selection());\n    assert_eq!(guard.decisions_granted(), 1);\n    assert_eq!(session.max_scan_ms(), 10000);\n    assert_eq!(session.max_peripherals(), 2);\n    assert_eq!(session.connection_timeout_ms(), 10000);\n    assert_eq!(session.max_reconnect_attempts(), 2);\n    assert_eq!(session.service_uuids(), [\"181A\".to_owned()]);\n    assert_eq!(session.report_advertisement(\"CC:DD\", -71, &[\"FF00\"]), Ok(bluetooth_runtime::ScanOutcome::Discovered));\n    assert_eq!(session.connect(\"CC:DD\"), Err(bluetooth_runtime::BluetoothError::WrongState));\n    assert_eq!(session.report_advertisement(\"AA:BB\", -59, &[\"181A\"]), Ok(bluetooth_runtime::ScanOutcome::Recognised));\n    let session_id = session.connect(\"AA:BB\").expect(\"recognised peripheral connects\");\n    assert_eq!(session_id, \"conn-AA:BB-1\");\n    assert_eq!(session.session_id(\"AA:BB\"), Ok(Some(\"conn-AA:BB-1\".to_owned())));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-session-factory-{}-{nonce}",
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
            .expect("run generated Bluetooth session factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
