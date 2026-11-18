//! Blue Moon Bluetooth capability metadata and privacy-key planning for the
//! Neptune Apple pipeline.
//!
//! This module is the first compiler-side consumption of the goya-caprichios
//! Blue Moon contracts. It generates the capability-inventory entries and the
//! Apple privacy usage-description planning data (directive §9.2) from the
//! exact contract IDs and validated declarations, failing closed on unknown
//! contracts and on generic or placeholder usage descriptions.
//!
//! No iOS build is admitted here: the plan and fragment are preview data.
//! Nothing is written into an Info.plist, no entitlement is granted, no
//! framework is linked, and every emitted entry carries the crate-wide
//! `apple_validation_not_run` status.

use crate::Diagnostic;
use goya_caprichios::bluetooth_runtime::validate_usage_description;
use goya_caprichios::bluetooth_runtime::BluetoothError;
use serde_json::{json, Value};

/// The modern Apple Bluetooth privacy usage-description key.
pub const BLUETOOTH_USAGE_KEY: &str = "NSBluetoothAlwaysUsageDescription";
/// The legacy key older deployment targets may require (directive §9.2).
pub const BLUETOOTH_LEGACY_USAGE_KEY: &str = "NSBluetoothPeripheralUsageDescription";

/// The implemented Blue Moon contracts with their registry recipe names. The
/// table is checked against `goya_caprichios::registry::supported_contracts`
/// in tests; an entry that drifts from the registry fails the suite.
const BLUETOOTH_CONTRACTS: &[(&str, &str)] = &[
    (
        "goya.hardware.bluetooth.access.v1",
        "apple-ble-access-bounded-mapping",
    ),
    (
        "goya.hardware.bluetooth.central.v1",
        "apple-ble-central-bounded-scan",
    ),
    (
        "goya.hardware.bluetooth.continuity.v1",
        "apple-ble-continuity-bounded-restoration",
    ),
    (
        "goya.hardware.bluetooth.gatt.v1",
        "apple-ble-gatt-bounded-commands",
    ),
    (
        "goya.hardware.bluetooth.guard.v1",
        "apple-ble-guard-bounded-authorisation",
    ),
    (
        "goya.hardware.bluetooth.profile.v1",
        "apple-ble-profile-bounded-classification",
    ),
    (
        "goya.hardware.bluetooth.stream.v1",
        "apple-ble-stream-bounded-notifications",
    ),
];

/// The implemented Blue Moon contract table, in lexical order: contract ID
/// and registry recipe name. Exposed so the service protocol can default to
/// all implemented contracts.
pub fn contract_table() -> &'static [(&'static str, &'static str)] {
    BLUETOOTH_CONTRACTS
}

/// Bundle-level Bluetooth input for [`crate::preview_bundle_with_bluetooth`]:
/// one Guard-validated usage description and the implemented contracts to
/// admit into the capability inventory. An empty contract list defaults to
/// all seven in lexical order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothBundleInput {
    pub usage_description: String,
    pub contracts: Vec<String>,
}

/// Planned Apple Bluetooth privacy declarations. Background modes are always
/// empty: no background execution is claimed (the directive's illustrative
/// IR carries `background_execution: false`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothPrivacyPlan {
    usage_key: &'static str,
    usage_value: String,
    legacy_usage_key: &'static str,
    background_modes: Vec<&'static str>,
}

impl BluetoothPrivacyPlan {
    pub fn usage_key(&self) -> &'static str {
        self.usage_key
    }

    pub fn usage_value(&self) -> &str {
        &self.usage_value
    }

    pub fn legacy_usage_key(&self) -> &'static str {
        self.legacy_usage_key
    }

    pub fn background_modes(&self) -> &[&'static str] {
        &self.background_modes
    }

    pub fn to_json(&self) -> Value {
        json!({
            "usage_key": self.usage_key,
            "usage_value": self.usage_value,
            "legacy_usage_key": self.legacy_usage_key,
            "background_modes": self.background_modes,
            "info_plist_fragment": info_plist_privacy_fragment(self),
        })
    }
}

/// Plan the Apple Bluetooth privacy declarations from an application-specific
/// usage description. The description is validated with Blue Moon Guard's
/// fail-closed rules (directive §9.2): empty, generic and placeholder
/// descriptions are rejected with `NPT-BM-E002`.
pub fn plan_bluetooth_privacy(usage_description: &str) -> Result<BluetoothPrivacyPlan, Diagnostic> {
    validate_usage_description(usage_description).map_err(|error| Diagnostic {
        code: "NPT-BM-E002",
        message: bluetooth_message(error),
        range: None,
    })?;
    Ok(BluetoothPrivacyPlan {
        usage_key: BLUETOOTH_USAGE_KEY,
        usage_value: usage_description.to_owned(),
        legacy_usage_key: BLUETOOTH_LEGACY_USAGE_KEY,
        background_modes: Vec::new(),
    })
}

fn bluetooth_message(error: BluetoothError) -> String {
    match error {
        BluetoothError::GenericUsageDescription => {
            "privacy usage description is empty, generic or a placeholder; \
                 explanations must be application-specific"
                .to_owned()
        }
        BluetoothError::UsageDescriptionTooLong => {
            "privacy usage description exceeds the supported bound".to_owned()
        }
        _ => error.to_string(),
    }
}

/// The capability-inventory entry for one implemented Blue Moon contract.
/// Unknown contract IDs fail closed with `NPT-BM-E001`; draft catalog IDs and
/// typos are never silently emitted. The entry shape matches the existing
/// `capabilities.json` per-capability objects.
pub fn bluetooth_capability_entry(contract: &str) -> Result<Value, Diagnostic> {
    let Some((_, recipe)) = BLUETOOTH_CONTRACTS
        .iter()
        .find(|(known, _)| *known == contract)
    else {
        return Err(Diagnostic {
            code: "NPT-BM-E001",
            message: format!(
                "`{contract}` is not an implemented Blue Moon contract; \
                 unknown capabilities are rejected, not ignored"
            ),
            range: None,
        });
    };
    Ok(json!({
        "semantic_id": contract,
        "adapter": recipe,
        "status": "construction",
        "minimum_os": null,
        "frameworks": ["CoreBluetooth"],
        "entitlements": [],
        "usage_description_keys": [BLUETOOTH_USAGE_KEY],
        "privacy_declarations": "owner_review_required",
        "test_status": "apple_validation_not_run",
        "fallback": "reject",
    }))
}

/// Capability-inventory entries for several contracts, order preserved.
/// All-or-nothing: one unknown contract rejects the whole request.
pub fn bluetooth_capability_entries(contracts: &[&str]) -> Result<Vec<Value>, Diagnostic> {
    contracts
        .iter()
        .map(|contract| bluetooth_capability_entry(contract))
        .collect()
}

/// Escape one string for inclusion in an XML property-list fragment.
fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// The Info.plist privacy fragment for one plan: the modern key and the
/// legacy key, both carrying the validated application-specific description.
/// This is a fragment for a future plist emitter; no Info.plist is generated
/// or written by this crate.
pub fn info_plist_privacy_fragment(plan: &BluetoothPrivacyPlan) -> String {
    let usage = xml_escape(plan.usage_value());
    format!(
        "<key>{}</key>\n<string>{}</string>\n<key>{}</key>\n<string>{}</string>\n",
        plan.usage_key(),
        usage,
        plan.legacy_usage_key(),
        usage
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_table_matches_the_registry() {
        for (contract, recipe) in BLUETOOTH_CONTRACTS {
            assert!(
                goya_caprichios::registry::supported_contracts().contains(contract),
                "{contract} is planned by neptune but not implemented in the registry"
            );
            assert!(recipe.starts_with("apple-ble-"), "{recipe} is not an Apple BLE recipe");
        }
        assert_eq!(BLUETOOTH_CONTRACTS.len(), 7);
    }

    #[test]
    fn privacy_plan_rejects_generic_descriptions() {
        for description in ["", "   ", "bluetooth", "uses bluetooth", "required", "TODO", "!!!"] {
            let plan = plan_bluetooth_privacy(description);
            assert_eq!(plan.as_ref().err().map(|d| d.code), Some("NPT-BM-E002"));
        }
        assert_eq!(
            plan_bluetooth_privacy(&"d".repeat(257))
                .err()
                .map(|d| d.code),
            Some("NPT-BM-E002")
        );
    }

    #[test]
    fn privacy_plan_is_deterministic_and_claims_no_background_execution() {
        let plan = plan_bluetooth_privacy(
            "Connects to your temperature sensor to show readings.",
        )
        .unwrap();
        assert_eq!(plan.usage_key(), "NSBluetoothAlwaysUsageDescription");
        assert_eq!(
            plan.usage_value(),
            "Connects to your temperature sensor to show readings."
        );
        assert_eq!(plan.legacy_usage_key(), "NSBluetoothPeripheralUsageDescription");
        assert!(plan.background_modes().is_empty());
        assert_eq!(plan.to_json(), plan.to_json());
    }

    #[test]
    fn capability_entries_cover_the_seven_contracts_and_fail_closed() {
        for (contract, recipe) in BLUETOOTH_CONTRACTS {
            let entry = bluetooth_capability_entry(contract).unwrap();
            assert_eq!(entry["semantic_id"], *contract);
            assert_eq!(entry["adapter"], *recipe);
            assert_eq!(entry["frameworks"], json!(["CoreBluetooth"]));
            assert_eq!(entry["usage_description_keys"], json!(["NSBluetoothAlwaysUsageDescription"]));
            assert_eq!(entry["entitlements"], json!([]));
            assert_eq!(entry["test_status"], "apple_validation_not_run");
            assert_eq!(entry["fallback"], "reject");
            assert_eq!(entry["privacy_declarations"], "owner_review_required");
            // Shape compatibility with the existing capabilities.json objects.
            let object = entry.as_object().unwrap();
            assert_eq!(object.len(), 10);
            for field in [
                "semantic_id",
                "adapter",
                "status",
                "minimum_os",
                "frameworks",
                "entitlements",
                "usage_description_keys",
                "privacy_declarations",
                "test_status",
                "fallback",
            ] {
                assert!(object.contains_key(field), "missing capability field {field}");
            }
        }
        for unknown in [
            "goya.hardware.bluetooth.pairing.v1",
            "hardware.bluetooth",
            "goya.hardware.bluetooth.v1",
            "",
        ] {
            let entry = bluetooth_capability_entry(unknown);
            assert_eq!(entry.as_ref().err().map(|d| d.code), Some("NPT-BM-E001"));
        }
    }

    #[test]
    fn entries_are_ordered_and_all_or_nothing() {
        let entries = bluetooth_capability_entries(&[
            "goya.hardware.bluetooth.guard.v1",
            "goya.hardware.bluetooth.central.v1",
        ])
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["semantic_id"], "goya.hardware.bluetooth.guard.v1");
        assert_eq!(entries[1]["semantic_id"], "goya.hardware.bluetooth.central.v1");
        let rejected = bluetooth_capability_entries(&[
            "goya.hardware.bluetooth.guard.v1",
            "future.unknown.v1",
        ]);
        assert_eq!(rejected.as_ref().err().map(|d| d.code), Some("NPT-BM-E001"));
    }

    #[test]
    fn info_plist_fragment_escapes_and_is_deterministic() {
        let plan = plan_bluetooth_privacy(
            "Connects to your <temperature> & \"humidity\" sensor.",
        )
        .unwrap();
        let fragment = info_plist_privacy_fragment(&plan);
        assert_eq!(fragment, info_plist_privacy_fragment(&plan));
        assert!(fragment.contains("<key>NSBluetoothAlwaysUsageDescription</key>"));
        assert!(fragment.contains("<key>NSBluetoothPeripheralUsageDescription</key>"));
        assert!(fragment.contains(
            "<string>Connects to your &lt;temperature&gt; &amp; &quot;humidity&quot; sensor.</string>"
        ));
        // Exactly two escaped strings: the usage value appears twice.
        assert_eq!(fragment.matches("<string>").count(), 2);
        assert_eq!(fragment.matches("</string>").count(), 2);
    }
}
