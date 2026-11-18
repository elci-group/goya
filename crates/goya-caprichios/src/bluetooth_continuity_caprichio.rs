//! A bounded Bluetooth lifecycle-restoration capability for iOS and Apple
//! Silicon Mac hosts: the Blue Moon Continuity core.
//!
//! The negotiated artifact binds to this crate's process-local Bluetooth
//! runtime. It does not persist anything to disk, restore a real session, or
//! prove that a host relaunch occurred. Hosts on Apple hardware report their
//! real persistence and restoration facilities through
//! [`RuntimeCapabilities`]. Apple's Bluetooth state restoration is
//! conditional: the ledger records restoration intent and models suspension,
//! resumption and reconciliation explicitly, and it never promises relaunch
//! or restoration success.

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.bluetooth.continuity.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_ASSOCIATIONS: u16 = crate::bluetooth_runtime::MAX_ASSOCIATIONS;
pub const MAX_SUBSCRIPTIONS_PER_ASSOCIATION: u16 =
    crate::bluetooth_runtime::MAX_SUBSCRIPTIONS_PER_ASSOCIATION;
pub const MAX_SERVICE_UUID_BYTES: usize = crate::bluetooth_runtime::MAX_SERVICE_UUID_BYTES;

pub use crate::bluetooth_runtime::{
    AssociationIntent, AssociationState, ContinuityLedger, Reconciliation, RestorationIntent,
    SuspensionReason,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothContinuityDeclaration {
    /// Bounded persisted-association ledger, from one through
    /// `MAX_ASSOCIATIONS`.
    pub max_associations: u16,
    /// Bounded compatible-subscription list per association, from zero
    /// (association without subscriptions) through
    /// `MAX_SUBSCRIPTIONS_PER_ASSOCIATION`.
    pub max_subscriptions_per_association: u16,
    /// Request restoration across application launches. When `true`, the
    /// host must report both state-restoration and persistent-storage
    /// facilities; when `false`, the ledger is session-scoped only.
    pub persist_across_launches: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothContinuityIr {
    max_associations: u16,
    max_subscriptions_per_association: u16,
    persist_across_launches: bool,
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
/// restoration and persistence facilities are conditional: only required
/// when the declaration asks for cross-launch persistence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host runtime supports conditional Bluetooth state restoration.
    pub bluetooth_state_restoration: bool,
    /// Host provides persistent storage for the association ledger.
    pub persistent_storage: bool,
    /// Host reports Bluetooth power transitions (disabled, re-enabled).
    pub bluetooth_power_monitoring: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroAssociationBound,
    AssociationBoundTooLarge,
    SubscriptionBoundTooLarge,
    UnsupportedTarget,
    MissingBluetoothStateRestoration,
    MissingPersistentStorage,
    MissingBluetoothPowerMonitoring,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroAssociationBound => "association bound must be positive",
            Self::AssociationBoundTooLarge => {
                "association bound exceeds the supported bound"
            }
            Self::SubscriptionBoundTooLarge => {
                "per-association subscription bound exceeds the supported bound"
            }
            Self::UnsupportedTarget => {
                "the continuity factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingBluetoothStateRestoration => {
                "cross-launch persistence requires conditional state restoration"
            }
            Self::MissingPersistentStorage => {
                "cross-launch persistence requires persistent storage for the ledger"
            }
            Self::MissingBluetoothPowerMonitoring => {
                "target runtime must report Bluetooth power transitions"
            }
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled,
/// that anything persisted, or that any restoration ever occurred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothContinuityArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl BluetoothContinuityDeclaration {
    pub fn normalize(self) -> Result<BluetoothContinuityIr, Diagnostic> {
        if self.max_associations == 0 {
            return Err(Diagnostic::ZeroAssociationBound);
        }
        if self.max_associations > MAX_ASSOCIATIONS {
            return Err(Diagnostic::AssociationBoundTooLarge);
        }
        // Zero subscriptions per association is valid; the bound only caps.
        if self.max_subscriptions_per_association > MAX_SUBSCRIPTIONS_PER_ASSOCIATION {
            return Err(Diagnostic::SubscriptionBoundTooLarge);
        }
        Ok(BluetoothContinuityIr {
            max_associations: self.max_associations,
            max_subscriptions_per_association: self.max_subscriptions_per_association,
            persist_across_launches: self.persist_across_launches,
        })
    }
}

impl BluetoothContinuityIr {
    pub fn max_associations(&self) -> u16 {
        self.max_associations
    }

    pub fn max_subscriptions_per_association(&self) -> u16 {
        self.max_subscriptions_per_association
    }

    pub fn persist_across_launches(&self) -> bool {
        self.persist_across_launches
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
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=ble_continuity_bounded_restoration\nmax_associations={}\nmax_subscriptions_per_association={}\npersist_across_launches={}\nassociations_recorded=none\nrestoration_planned=none\nrelaunch_promised=none\n",
            Self::target_label(target),
            self.max_associations,
            self.max_subscriptions_per_association,
            self.persist_across_launches
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BluetoothContinuityArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.bluetooth_power_monitoring {
            return Err(Diagnostic::MissingBluetoothPowerMonitoring);
        }
        if self.persist_across_launches {
            if !capabilities.bluetooth_state_restoration {
                return Err(Diagnostic::MissingBluetoothStateRestoration);
            }
            if !capabilities.persistent_storage {
                return Err(Diagnostic::MissingPersistentStorage);
            }
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        Ok(BluetoothContinuityArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=apple-ble-continuity-bounded-restoration; {} continuity ledger, nothing persists, no restoration runs and no relaunch is promised.\npub fn make_continuity_ledger() -> Result<crate::bluetooth_runtime::ContinuityLedger, crate::bluetooth_runtime::BluetoothError> {{\n    crate::bluetooth_runtime::ContinuityLedger::new({}u16, {}u16)\n}}\n",
                Self::target_label(target),
                self.max_associations,
                self.max_subscriptions_per_association
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        bluetooth_state_restoration: true,
        persistent_storage: true,
        bluetooth_power_monitoring: true,
        utf8_strings: true,
    };

    fn declaration() -> BluetoothContinuityDeclaration {
        BluetoothContinuityDeclaration {
            max_associations: 4,
            max_subscriptions_per_association: 2,
            persist_across_launches: true,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.max_associations(), 4);
        assert_eq!(ir.max_subscriptions_per_association(), 2);
        assert!(ir.persist_across_launches());
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first
            .canonical_ir
            .contains("session=ble_continuity_bounded_restoration\n"));
        assert!(first.canonical_ir.contains("max_associations=4\n"));
        assert!(first
            .canonical_ir
            .contains("max_subscriptions_per_association=2\n"));
        assert!(first.canonical_ir.contains("persist_across_launches=true\n"));
        assert!(first.canonical_ir.contains("associations_recorded=none\n"));
        assert!(first.canonical_ir.contains("restoration_planned=none\n"));
        assert!(first.canonical_ir.contains("relaunch_promised=none\n"));
        assert!(first.source.contains("ContinuityLedger::new(4u16, 2u16)"));
    }

    #[test]
    fn zero_subscriptions_are_valid_but_bounds_are_enforced() {
        assert_eq!(
            BluetoothContinuityDeclaration {
                max_subscriptions_per_association: 0,
                ..declaration()
            }
            .normalize()
            .unwrap()
            .max_subscriptions_per_association(),
            0
        );
        assert_eq!(
            BluetoothContinuityDeclaration {
                max_associations: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroAssociationBound)
        );
        assert_eq!(
            BluetoothContinuityDeclaration {
                max_associations: MAX_ASSOCIATIONS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::AssociationBoundTooLarge)
        );
        assert_eq!(
            BluetoothContinuityDeclaration {
                max_subscriptions_per_association: MAX_SUBSCRIPTIONS_PER_ASSOCIATION + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::SubscriptionBoundTooLarge)
        );
    }

    #[test]
    fn persistence_facilities_are_conditional_on_the_declaration() {
        let ir = declaration().normalize().unwrap();
        let mut without_restoration = IOS;
        without_restoration.bluetooth_state_restoration = false;
        assert_eq!(
            ir.lower_checked(Target::Ios, without_restoration),
            Err(Diagnostic::MissingBluetoothStateRestoration)
        );
        let mut without_storage = IOS;
        without_storage.persistent_storage = false;
        assert_eq!(
            ir.lower_checked(Target::Ios, without_storage),
            Err(Diagnostic::MissingPersistentStorage)
        );
        // A session-scoped declaration requires neither facility.
        let session_scoped = BluetoothContinuityDeclaration {
            persist_across_launches: false,
            ..declaration()
        };
        let ir = session_scoped.normalize().unwrap();
        assert_eq!(
            ir.lower_checked(Target::Ios, without_restoration)
                .map(|artifact| artifact.contract),
            Ok(CONTRACT)
        );
        assert_eq!(
            ir.lower_checked(Target::Ios, without_storage)
                .map(|artifact| artifact.contract),
            Ok(CONTRACT)
        );
        let mut without_monitoring = IOS;
        without_monitoring.bluetooth_power_monitoring = false;
        assert_eq!(
            ir.lower_checked(Target::Ios, without_monitoring),
            Err(Diagnostic::MissingBluetoothPowerMonitoring)
        );
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, IOS),
                Err(Diagnostic::UnsupportedTarget)
            );
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
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nuse bluetooth_runtime::{{AssociationIntent, AssociationState, BluetoothError, SuspensionReason}};\nfn main() {{\n    let mut ledger = make_continuity_ledger().expect(\"factory should construct ledger\");\n    assert_eq!(ledger.max_associations(), 4);\n    assert_eq!(ledger.max_subscriptions(), 2);\n    ledger.record(\"AA:BB\", AssociationIntent::AutoReconnect, &[\"181A\".to_owned()]).expect(\"record association\");\n    assert_eq!(ledger.association_state(\"AA:BB\"), Ok(AssociationState::Active));\n    assert_eq!(ledger.revision(\"AA:BB\"), Ok(1));\n    ledger.record(\"AA:BB\", AssociationIntent::OnDemand, &[]).expect(\"update association\");\n    assert_eq!(ledger.association_intent(\"AA:BB\"), Ok(AssociationIntent::OnDemand));\n    assert_eq!(ledger.revision(\"AA:BB\"), Ok(2));\n    ledger.observe_bluetooth_disabled();\n    assert_eq!(ledger.association_state(\"AA:BB\"), Ok(AssociationState::Suspended(SuspensionReason::BluetoothDisabled)));\n    let plan = ledger.restore_intent(\"AA:BB\").expect(\"restoration planned\");\n    assert_eq!(plan.intent(), AssociationIntent::OnDemand);\n    assert_eq!(plan.subscriptions(), &[] as &[String]);\n    assert_eq!(ledger.restorations_planned(), 1);\n    assert_eq!(ledger.reconcile(\"AA:BB\", false), Ok(bluetooth_runtime::Reconciliation::Unchanged));\n    assert_eq!(ledger.reconcile(\"AA:BB\", true), Ok(bluetooth_runtime::Reconciliation::Restored));\n    assert_eq!(ledger.association_state(\"AA:BB\"), Ok(AssociationState::Active));\n    ledger.record(\"CC:DD\", AssociationIntent::AutoReconnect, &[]).unwrap();\n    ledger.record(\"EE:FF\", AssociationIntent::AutoReconnect, &[]).unwrap();\n    ledger.record(\"GG:GG\", AssociationIntent::AutoReconnect, &[]).unwrap();\n    assert_eq!(ledger.record(\"HH:HH\", AssociationIntent::AutoReconnect, &[]), Err(BluetoothError::AssociationBoundReached));\n    ledger.forget(\"CC:DD\").unwrap();\n    assert_eq!(ledger.forget(\"CC:DD\"), Err(BluetoothError::UnknownPeripheral));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-continuity-factory-{}-{nonce}",
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
            .expect("run generated Bluetooth continuity factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
