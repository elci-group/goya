//! A bounded characteristic-notification stream capability for iOS and Apple
//! Silicon Mac hosts: the Blue Moon Stream core.
//!
//! The negotiated artifact binds to this crate's process-local Bluetooth
//! runtime. It does not subscribe to a peripheral, receive a notification,
//! or prove that Core Bluetooth is linked in a consumer crate. Hosts on
//! Apple hardware report their real GATT notification facilities through
//! [`RuntimeCapabilities`]; only the transports a declaration actually uses
//! are required. The stream never claims guaranteed delivery: notifications
//! are unacknowledged at the attribute layer, and indications are confirmed
//! only through explicitly reported confirmations.

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.bluetooth.stream.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_STREAM_CHARACTERISTICS: usize = crate::bluetooth_runtime::MAX_STREAM_CHARACTERISTICS;
pub const MAX_NOTIFICATION_BYTES: usize = crate::bluetooth_runtime::MAX_NOTIFICATION_BYTES;
pub const MAX_BUFFERED_NOTIFICATIONS: u16 = crate::bluetooth_runtime::MAX_BUFFERED_NOTIFICATIONS;
pub const MAX_SERVICE_UUID_BYTES: usize = crate::bluetooth_runtime::MAX_SERVICE_UUID_BYTES;

pub use crate::bluetooth_runtime::{
    BufferedNotification, NotificationTransport, ReceiveOutcome, StreamCharacteristic,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothStreamDeclaration {
    /// Declared subscribable characteristics with fixed transports. At least
    /// one is required; UUIDs must be unique.
    pub characteristics: Vec<StreamCharacteristic>,
    /// Negotiated transport limit applied to every payload, from one through
    /// `MAX_NOTIFICATION_BYTES`.
    pub max_notification_bytes: usize,
    /// Bounded notification buffer, from one through
    /// `MAX_BUFFERED_NOTIFICATIONS`. A full buffer fails closed with drop
    /// accounting instead of evicting or growing unbounded.
    pub max_buffered_notifications: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothStreamIr {
    characteristics: Vec<StreamCharacteristic>,
    max_notification_bytes: usize,
    max_buffered_notifications: u16,
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

/// Explicitly reported host facilities required by this lowering. The
/// notification and indication facilities are conditional: only the
/// transports a declaration actually uses are required.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host runtime provides a GATT client surface through its Bluetooth stack.
    pub core_bluetooth_gatt: bool,
    /// Host GATT surface supports characteristic notifications.
    pub characteristic_notifications: bool,
    /// Host GATT surface supports characteristic indications.
    pub characteristic_indications: bool,
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
    ZeroNotificationBytes,
    NotificationBytesTooLarge,
    ZeroBufferedNotifications,
    BufferedNotificationsTooLarge,
    UnsupportedTarget,
    MissingCoreBluetoothGatt,
    MissingCharacteristicNotifications,
    MissingCharacteristicIndications,
    MissingBluetoothPower,
    MissingBluetoothPermission,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoCharacteristics => {
                "at least one subscribed characteristic is required; streams are \
                 restricted to declared application requirements"
            }
            Self::TooManyCharacteristics => {
                "subscribed characteristic count exceeds the supported bound"
            }
            Self::DuplicateCharacteristic => "a characteristic UUID was declared more than once",
            Self::ZeroNotificationBytes => "notification payload bound must be positive",
            Self::NotificationBytesTooLarge => {
                "notification payload bound exceeds the supported bound"
            }
            Self::ZeroBufferedNotifications => "buffered notification bound must be positive",
            Self::BufferedNotificationsTooLarge => {
                "buffered notification bound exceeds the supported bound"
            }
            Self::UnsupportedTarget => {
                "the notification stream factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingCoreBluetoothGatt => "target runtime must provide a GATT client surface",
            Self::MissingCharacteristicNotifications => {
                "the declaration uses notifications and the target runtime must support them"
            }
            Self::MissingCharacteristicIndications => {
                "the declaration uses indications and the target runtime must support them"
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
/// that any subscription was established, or that any notification arrived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothStreamArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl BluetoothStreamDeclaration {
    pub fn normalize(self) -> Result<BluetoothStreamIr, Diagnostic> {
        if self.characteristics.is_empty() {
            return Err(Diagnostic::NoCharacteristics);
        }
        if self.characteristics.len() > MAX_STREAM_CHARACTERISTICS {
            return Err(Diagnostic::TooManyCharacteristics);
        }
        let mut seen = Vec::with_capacity(self.characteristics.len());
        for characteristic in &self.characteristics {
            if seen
                .iter()
                .any(|uuid: &String| uuid == characteristic.uuid())
            {
                return Err(Diagnostic::DuplicateCharacteristic);
            }
            seen.push(characteristic.uuid().to_owned());
        }
        if self.max_notification_bytes == 0 {
            return Err(Diagnostic::ZeroNotificationBytes);
        }
        if self.max_notification_bytes > MAX_NOTIFICATION_BYTES {
            return Err(Diagnostic::NotificationBytesTooLarge);
        }
        if self.max_buffered_notifications == 0 {
            return Err(Diagnostic::ZeroBufferedNotifications);
        }
        if self.max_buffered_notifications > MAX_BUFFERED_NOTIFICATIONS {
            return Err(Diagnostic::BufferedNotificationsTooLarge);
        }
        Ok(BluetoothStreamIr {
            characteristics: self.characteristics,
            max_notification_bytes: self.max_notification_bytes,
            max_buffered_notifications: self.max_buffered_notifications,
        })
    }
}

impl BluetoothStreamIr {
    pub fn characteristics(&self) -> &[StreamCharacteristic] {
        &self.characteristics
    }

    pub fn max_notification_bytes(&self) -> usize {
        self.max_notification_bytes
    }

    pub fn max_buffered_notifications(&self) -> u16 {
        self.max_buffered_notifications
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
            .map(|characteristic| {
                format!(
                    "{}:{}",
                    characteristic.uuid(),
                    characteristic.transport().label()
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    fn uses_notifications(&self) -> bool {
        self.characteristics
            .iter()
            .any(|characteristic| characteristic.transport() == NotificationTransport::Notification)
    }

    fn uses_indications(&self) -> bool {
        self.characteristics
            .iter()
            .any(|characteristic| characteristic.transport() == NotificationTransport::Indication)
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=ble_stream_bounded_notifications\nmax_notification_bytes={}\nmax_buffered_notifications={}\ncharacteristics={}\nnotifications_received=none\ndelivery_guarantee=none\n",
            Self::target_label(target),
            self.max_notification_bytes,
            self.max_buffered_notifications,
            self.characteristics_label()
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BluetoothStreamArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.core_bluetooth_gatt {
            return Err(Diagnostic::MissingCoreBluetoothGatt);
        }
        if self.uses_notifications() && !capabilities.characteristic_notifications {
            return Err(Diagnostic::MissingCharacteristicNotifications);
        }
        if self.uses_indications() && !capabilities.characteristic_indications {
            return Err(Diagnostic::MissingCharacteristicIndications);
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
        let characteristic_arguments = self
            .characteristics
            .iter()
            .map(|characteristic| {
                format!(
                    "crate::bluetooth_runtime::StreamCharacteristic::new({uuid:?}.to_owned(), crate::bluetooth_runtime::NotificationTransport::{transport})?",
                    uuid = characteristic.uuid(),
                    transport = match characteristic.transport() {
                        NotificationTransport::Notification => "Notification",
                        NotificationTransport::Indication => "Indication",
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(BluetoothStreamArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=apple-ble-stream-bounded-notifications; {} notification stream session, no subscription is established, no notification arrives and delivery is never guaranteed.\npub fn make_stream_session() -> Result<crate::bluetooth_runtime::StreamSession, crate::bluetooth_runtime::BluetoothError> {{\n    crate::bluetooth_runtime::StreamSession::new(vec![{}], {}usize, {}u16)\n}}\n",
                Self::target_label(target),
                characteristic_arguments,
                self.max_notification_bytes,
                self.max_buffered_notifications
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        core_bluetooth_gatt: true,
        characteristic_notifications: true,
        characteristic_indications: true,
        bluetooth_powered: true,
        bluetooth_permission: true,
        utf8_strings: true,
    };

    fn declaration() -> BluetoothStreamDeclaration {
        BluetoothStreamDeclaration {
            characteristics: vec![
                StreamCharacteristic::new("2A6E".to_owned(), NotificationTransport::Notification)
                    .unwrap(),
                StreamCharacteristic::new("2A37".to_owned(), NotificationTransport::Indication)
                    .unwrap(),
            ],
            max_notification_bytes: 256,
            max_buffered_notifications: 2,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.characteristics().len(), 2);
        assert_eq!(ir.max_notification_bytes(), 256);
        assert_eq!(ir.max_buffered_notifications(), 2);
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first
            .canonical_ir
            .contains("session=ble_stream_bounded_notifications\n"));
        assert!(first.canonical_ir.contains("max_notification_bytes=256\n"));
        assert!(first
            .canonical_ir
            .contains("max_buffered_notifications=2\n"));
        assert!(first
            .canonical_ir
            .contains("characteristics=2A6E:notification,2A37:indication\n"));
        assert!(first.canonical_ir.contains("notifications_received=none\n"));
        assert!(first.canonical_ir.contains("delivery_guarantee=none\n"));
        assert!(first.source.contains(
            "StreamCharacteristic::new(\"2A6E\".to_owned(), crate::bluetooth_runtime::NotificationTransport::Notification)?"
        ));
        assert!(first.source.contains(", 256usize, 2u16)"));
    }

    #[test]
    fn rejects_missing_duplicate_and_overbound_declarations() {
        assert_eq!(
            BluetoothStreamDeclaration {
                characteristics: Vec::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NoCharacteristics)
        );
        assert_eq!(
            BluetoothStreamDeclaration {
                characteristics: vec![
                    StreamCharacteristic::new(
                        "2A6E".to_owned(),
                        NotificationTransport::Notification
                    )
                    .unwrap(),
                    StreamCharacteristic::new("2A6E".to_owned(), NotificationTransport::Indication)
                        .unwrap(),
                ],
                max_notification_bytes: 256,
                max_buffered_notifications: 2,
            }
            .normalize(),
            Err(Diagnostic::DuplicateCharacteristic)
        );
        assert_eq!(
            BluetoothStreamDeclaration {
                characteristics: (0..=MAX_STREAM_CHARACTERISTICS)
                    .map(|index| StreamCharacteristic::new(
                        format!("{index:04}"),
                        NotificationTransport::Notification
                    )
                    .unwrap())
                    .collect(),
                max_notification_bytes: 256,
                max_buffered_notifications: 2,
            }
            .normalize(),
            Err(Diagnostic::TooManyCharacteristics)
        );
        assert_eq!(
            BluetoothStreamDeclaration {
                max_notification_bytes: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroNotificationBytes)
        );
        assert_eq!(
            BluetoothStreamDeclaration {
                max_notification_bytes: MAX_NOTIFICATION_BYTES + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NotificationBytesTooLarge)
        );
        assert_eq!(
            BluetoothStreamDeclaration {
                max_buffered_notifications: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroBufferedNotifications)
        );
        assert_eq!(
            BluetoothStreamDeclaration {
                max_buffered_notifications: MAX_BUFFERED_NOTIFICATIONS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BufferedNotificationsTooLarge)
        );
    }

    #[test]
    fn transport_facilities_are_conditional_on_the_declaration() {
        let ir = declaration().normalize().unwrap();
        // The mixed declaration needs both transports.
        let mut notifications_only = IOS;
        notifications_only.characteristic_indications = false;
        assert_eq!(
            ir.lower_checked(Target::Ios, notifications_only),
            Err(Diagnostic::MissingCharacteristicIndications)
        );
        let mut indications_only = IOS;
        indications_only.characteristic_notifications = false;
        assert_eq!(
            ir.lower_checked(Target::Ios, indications_only),
            Err(Diagnostic::MissingCharacteristicNotifications)
        );
        // A notifications-only declaration does not require indications.
        let notifications_declaration = BluetoothStreamDeclaration {
            characteristics: vec![StreamCharacteristic::new(
                "2A6E".to_owned(),
                NotificationTransport::Notification,
            )
            .unwrap()],
            max_notification_bytes: 256,
            max_buffered_notifications: 2,
        };
        let ir = notifications_declaration.normalize().unwrap();
        assert_eq!(
            ir.lower_checked(Target::Ios, notifications_only)
                .map(|artifact| artifact.contract),
            Ok(CONTRACT)
        );
        let mut without_gatt = notifications_only;
        without_gatt.core_bluetooth_gatt = false;
        assert_eq!(
            ir.lower_checked(Target::Ios, without_gatt),
            Err(Diagnostic::MissingCoreBluetoothGatt)
        );
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, IOS),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(Target::Ios, RuntimeCapabilities::default()),
            Err(Diagnostic::MissingCoreBluetoothGatt)
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
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nuse bluetooth_runtime::{{BluetoothError, NotificationTransport}};\nfn main() {{\n    let mut session = make_stream_session().expect(\"factory should construct session\");\n    assert_eq!(session.max_notification_bytes(), 256);\n    assert_eq!(session.max_buffered_notifications(), 2);\n    assert_eq!(session.characteristics(), 2);\n    assert_eq!(session.transport(\"2A6E\"), Ok(NotificationTransport::Notification));\n    assert_eq!(session.transport(\"2A37\"), Ok(NotificationTransport::Indication));\n    assert_eq!(session.receive(\"2A6E\", &[1]), Err(BluetoothError::WrongState));\n    session.subscribe(\"2A6E\").unwrap();\n    session.subscribe(\"2A37\").unwrap();\n    let first = session.receive(\"2A6E\", &[1, 2]).expect(\"intake accepted\");\n    let second = session.receive(\"2A6E\", &[3, 4]).expect(\"intake accepted\");\n    assert_eq!(first.ordinal(), 1);\n    assert_eq!(second.ordinal(), 2);\n    assert_eq!(session.receive(\"2A6E\", &[5, 6]), Err(BluetoothError::BufferExhausted));\n    assert_eq!(session.dropped(), 1);\n    let taken = session.take().expect(\"intake order preserved\");\n    assert_eq!(taken.ordinal(), 1);\n    assert_eq!(taken.payload(), &[1, 2]);\n    assert_eq!(session.take().expect(\"intake order preserved\").ordinal(), 2);\n    let indication = session.receive(\"2A37\", &[7, 8]).expect(\"intake accepted\");\n    assert_eq!(indication.ordinal(), 3);\n    assert_eq!(session.pending_confirmations(), 1);\n    assert_eq!(session.confirm(first.ordinal()), Err(BluetoothError::UnknownOrdinal));\n    session.confirm(indication.ordinal()).unwrap();\n    assert_eq!(session.take().expect(\"intake order preserved\").ordinal(), 3);\n    session.cancel(\"2A6E\").unwrap();\n    assert_eq!(session.cancel(\"2A6E\"), Err(BluetoothError::WrongState));\n    assert_eq!(session.received(), 3);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-stream-factory-{}-{nonce}",
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
            .expect("run generated Bluetooth stream factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
