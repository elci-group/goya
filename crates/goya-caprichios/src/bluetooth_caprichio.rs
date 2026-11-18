//! A bounded BLE central discovery-and-link capability for iOS and Apple
//! Silicon Mac hosts: the Blue Moon Discovery and Link core.
//!
//! The negotiated artifact binds to this crate's process-local Bluetooth
//! runtime. It does not drive a radio, discover a peripheral, or prove that
//! Core Bluetooth is linked in a consumer crate. Hosts on Apple hardware
//! report their real Bluetooth facilities through [`RuntimeCapabilities`].
//! Peripherals that are merely detected are never reported as connected, and
//! a successful connection is never reported as an operational one.

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.bluetooth.central.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_SCAN_MS: u32 = crate::bluetooth_runtime::MAX_SCAN_MS;
pub const MAX_SERVICE_UUIDS: usize = crate::bluetooth_runtime::MAX_SERVICE_UUIDS;
pub const MAX_SERVICE_UUID_BYTES: usize = crate::bluetooth_runtime::MAX_SERVICE_UUID_BYTES;
pub const MAX_PERIPHERALS: u16 = crate::bluetooth_runtime::MAX_PERIPHERALS;
pub const MAX_CONNECTION_TIMEOUT_MS: u32 = crate::bluetooth_runtime::MAX_CONNECTION_TIMEOUT_MS;
pub const MAX_RECONNECT_ATTEMPTS: u8 = crate::bluetooth_runtime::MAX_RECONNECT_ATTEMPTS;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothCentralDeclaration {
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
pub struct BluetoothCentralIr {
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

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
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
    MissingCoreBluetoothCentral,
    MissingBluetoothPower,
    MissingBluetoothPermission,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoServiceUuids => {
                "at least one service UUID filter is required; scans are restricted \
                 to declared application requirements"
            }
            Self::EmptyServiceUuid => "service UUID filter must not be empty",
            Self::ServiceUuidTooLong => "service UUID filter exceeds the supported bound",
            Self::TooManyServiceUuids => {
                "service UUID filter count exceeds the supported bound"
            }
            Self::ZeroScanBound => "discovery timeout bound must be positive",
            Self::ScanBoundTooLarge => "discovery timeout bound exceeds the supported bound",
            Self::ZeroPeripheralBound => "peripheral bound must be positive",
            Self::PeripheralBoundTooLarge => "peripheral bound exceeds the supported bound",
            Self::ZeroConnectionTimeout => "connection timeout bound must be positive",
            Self::ConnectionTimeoutTooLarge => {
                "connection timeout bound exceeds the supported bound"
            }
            Self::ReconnectBoundTooLarge => {
                "reconnect attempt bound exceeds the supported bound"
            }
            Self::UnsupportedTarget => {
                "the BLE central factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingCoreBluetoothCentral => {
                "target runtime must provide a BLE central role"
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
/// that a radio was driven, or that any peripheral was contacted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothCentralArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl BluetoothCentralDeclaration {
    pub fn normalize(self) -> Result<BluetoothCentralIr, Diagnostic> {
        if self.service_uuids.is_empty() {
            return Err(Diagnostic::NoServiceUuids);
        }
        if self.service_uuids.len() > MAX_SERVICE_UUIDS {
            return Err(Diagnostic::TooManyServiceUuids);
        }
        if self
            .service_uuids
            .iter()
            .any(|uuid| uuid.is_empty())
        {
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
        Ok(BluetoothCentralIr {
            service_uuids: self.service_uuids,
            max_scan_ms: self.max_scan_ms,
            max_peripherals: self.max_peripherals,
            connection_timeout_ms: self.connection_timeout_ms,
            max_reconnect_attempts: self.max_reconnect_attempts,
        })
    }
}

impl BluetoothCentralIr {
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
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=ble_central_scan\nmax_scan_ms={}\nservice_uuids={}\nmax_peripherals={}\nconnection_timeout_ms={}\nmax_reconnect_attempts={}\nradio_driven=none\nperipherals_connected=none\n",
            Self::target_label(target),
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
    ) -> Result<BluetoothCentralArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
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
        Ok(BluetoothCentralArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=apple-ble-central-bounded-scan; {} BLE central scan session, no radio is driven and no peripheral is contacted.\npub fn make_central_session() -> Result<crate::bluetooth_runtime::CentralSession, crate::bluetooth_runtime::BluetoothError> {{\n    crate::bluetooth_runtime::CentralSession::new(vec![{}], {}u32, {}u16, {}u32, {}u8)\n}}\n",
                Self::target_label(target),
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

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        core_bluetooth_central: true,
        bluetooth_powered: true,
        bluetooth_permission: true,
        utf8_strings: true,
    };

    fn declaration() -> BluetoothCentralDeclaration {
        BluetoothCentralDeclaration {
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
        assert_eq!(ir.service_uuids(), &["181A".to_owned()]);
        assert_eq!(ir.max_scan_ms(), 10_000);
        assert_eq!(ir.max_peripherals(), 2);
        assert_eq!(ir.connection_timeout_ms(), 10_000);
        assert_eq!(ir.max_reconnect_attempts(), 2);
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first.canonical_ir.contains("session=ble_central_scan\n"));
        assert!(first.canonical_ir.contains("max_scan_ms=10000\n"));
        assert!(first.canonical_ir.contains("service_uuids=181A\n"));
        assert!(first.canonical_ir.contains("max_peripherals=2\n"));
        assert!(first.canonical_ir.contains("connection_timeout_ms=10000\n"));
        assert!(first.canonical_ir.contains("max_reconnect_attempts=2\n"));
        assert!(first.canonical_ir.contains("radio_driven=none\n"));
        assert!(first.canonical_ir.contains("peripherals_connected=none\n"));
        assert!(first.source.contains(
            "CentralSession::new(vec![\"181A\".to_owned()], 10000u32, 2u16, 10000u32, 2u8)"
        ));
    }

    #[test]
    fn multiple_filters_keep_canonical_order_and_apple_silicon_lowers_with_its_own_label() {
        let declaration = BluetoothCentralDeclaration {
            service_uuids: vec!["181A".to_owned(), "180F".to_owned()],
            ..declaration()
        };
        let artifact = declaration
            .normalize()
            .unwrap()
            .lower_checked(Target::AppleSiliconMac, IOS)
            .unwrap();
        assert!(artifact
            .canonical_ir
            .contains("target=apple_silicon_mac\n"));
        assert!(artifact.canonical_ir.contains("service_uuids=181A,180F\n"));
        assert!(artifact.source.contains("apple_silicon_mac BLE central scan"));
        assert!(artifact.source.contains("\"180F\".to_owned()"));
    }

    #[test]
    fn rejects_unfiltered_scans_and_bad_bounds() {
        assert_eq!(
            BluetoothCentralDeclaration {
                service_uuids: Vec::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NoServiceUuids)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                service_uuids: vec![String::new()],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::EmptyServiceUuid)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                service_uuids: vec!["u".repeat(MAX_SERVICE_UUID_BYTES + 1)],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ServiceUuidTooLong)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                service_uuids: vec!["a".to_owned(); MAX_SERVICE_UUIDS + 1],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::TooManyServiceUuids)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                max_scan_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroScanBound)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                max_scan_ms: MAX_SCAN_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ScanBoundTooLarge)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                max_peripherals: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroPeripheralBound)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                max_peripherals: MAX_PERIPHERALS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::PeripheralBoundTooLarge)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                connection_timeout_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroConnectionTimeout)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
                connection_timeout_ms: MAX_CONNECTION_TIMEOUT_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ConnectionTimeoutTooLarge)
        );
        assert_eq!(
            BluetoothCentralDeclaration {
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
            Err(Diagnostic::MissingCoreBluetoothCentral)
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
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nfn main() {{\n    let mut session = make_central_session().expect(\"factory should construct session\");\n    assert_eq!(session.max_scan_ms(), 10000);\n    assert_eq!(session.max_peripherals(), 2);\n    assert_eq!(session.connection_timeout_ms(), 10000);\n    assert_eq!(session.max_reconnect_attempts(), 2);\n    assert_eq!(session.service_uuids(), [\"181A\".to_owned()]);\n    assert_eq!(session.report_advertisement(\"AA:BB\", -59, &[\"181A\"]), Ok(bluetooth_runtime::ScanOutcome::Recognised));\n    assert_eq!(session.report_advertisement(\"CC:DD\", -71, &[\"FF00\"]), Ok(bluetooth_runtime::ScanOutcome::Discovered));\n    assert_eq!(session.report_advertisement(\"AA:BB\", -62, &[\"181A\"]), Ok(bluetooth_runtime::ScanOutcome::Duplicate));\n    assert_eq!(session.peripheral_state(\"AA:BB\"), Ok(bluetooth_runtime::PeripheralState::Recognised));\n    assert_eq!(session.connect(\"CC:DD\"), Err(bluetooth_runtime::BluetoothError::WrongState));\n    let first = session.connect(\"AA:BB\").expect(\"recognised peripheral connects\");\n    assert_eq!(first, \"conn-AA:BB-1\");\n    session.mark_operational(\"AA:BB\").unwrap();\n    assert_eq!(session.peripheral_state(\"AA:BB\"), Ok(bluetooth_runtime::PeripheralState::Operational));\n    session.observe_disconnect(\"AA:BB\").unwrap();\n    let plan = session.reconnect(\"AA:BB\").expect(\"reconnect within bound\");\n    assert_eq!(plan.attempt(), 1);\n    assert_eq!(plan.backoff_ms(), 100);\n    session.observe_disconnect(\"AA:BB\").unwrap();\n    session.reconnect(\"AA:BB\").unwrap();\n    session.observe_disconnect(\"AA:BB\").unwrap();\n    assert_eq!(session.reconnect(\"AA:BB\"), Err(bluetooth_runtime::BluetoothError::NoRetryRemaining));\n    assert_eq!(session.report_advertisement(\"EE:FF\", -80, &[]), Err(bluetooth_runtime::BluetoothError::PeripheralBoundReached));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-central-factory-{}-{nonce}",
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
            .expect("run generated Bluetooth central factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
