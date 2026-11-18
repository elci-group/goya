# BLUE-MOON-7 — Blue Moon Bluetooth device profile contract

Contract identifier: `goya.hardware.bluetooth.profile.v1`. Status:
**Candidate**, 18 November 2026. Architecture approval is required before a
consumer may cite it as a frozen contract (`docs/contracts.md`).
Implementation: `crates/goya-caprichios/src/bluetooth_profile_caprichio.rs`
(typed declaration, reference profiles, fail-closed lowering),
`crates/goya-caprichios/src/bluetooth_runtime.rs` (`ProfileRegistry`,
`DeviceProfileRecord`, `ProfileCharacteristic`, `ProfileProperties`),
`crates/goya-caprichios/src/registry.rs` (negotiation).

BLUE-MOON-7 covers the BM-08 Profile core: a versioned device-profile schema
describing the externally observable capabilities of a class of peripherals
independently of any consuming application, plus the directive's ten
reference profiles.

## Boundary

- The generated artifact is a Rust factory that constructs a
  `bluetooth_runtime::ProfileRegistry` with one profile registered. No
  device is discovered, enumerated or classified against real hardware.
- Profiles are validated against **category invariants** — the standard
  characteristic UUIDs and properties the applicable Bluetooth
  specifications assign to each category. The canonical IR states
  `spec_validation=category_invariants_only` and `devices_classified=none`:
  actual device behaviour is never verified by this contract.

## Pipeline

```text
BluetoothProfileDeclaration
        │  normalize (id/version/characteristic validation, invariants)
        ▼
BluetoothProfileIr  ── canonical_for(target) ──▶ canonical IR text
        │  lower_checked(target, RuntimeCapabilities)
        ▼
BluetoothProfileArtifact { contract, recipe_version, canonical_ir, source }
        │  registry::negotiate
        ▼
NegotiatedCapability { id, artifact: Artifact::BluetoothProfile }
```

Targets: `Ios` and `AppleSiliconMac` lower. Recipe:
`apple-ble-profile-bounded-classification@0.0.1`.

## Declaration shape

| Field | Type | Bound / rule | Rejects |
| --- | --- | --- | --- |
| `profile_id` | `String` | dotted ASCII name ≤ 64 bytes | `BadProfileId`, `ProfileIdTooLong` |
| `category` | `ProfileCategory` | one of the ten categories | invariant diagnostics |
| `version` | `String` | `MAJOR.MINOR.PATCH`, numeric segments, ≤ 32 bytes | `BadProfileVersion` |
| `required` / `optional` | `Vec<ProfileCharacteristic>` | ≤ 16 each, UUIDs unique across both | `TooManyCharacteristics`, `DuplicateCharacteristic` |

`ProfileCharacteristic` carries a UUID, observable `ProfileProperties`
(`read`/`write`/`notify`/`indicate`, modelled on GATT characteristic
properties) and a value bound. Profile characteristics are descriptions,
not enforcement policies; GATT command enforcement remains BLUE-MOON-2's
`CharacteristicPolicy`.

## Category invariants

| Category | Required invariant |
| --- | --- |
| GenericGatt, Custom | at least one required characteristic |
| Battery | required `2A19` with read (Battery Level) |
| Environmental | required `2A6E` with read (Temperature, the directive's illustrative `int16LE` characteristic) |
| HeartRate | required `2A37` with notify (Heart Rate Measurement) |
| Hid, Controller | required `2A4D` with notify (HID Report) |
| AccessibilitySwitch | at least one required characteristic with notify |
| Serial | required `6E400002` with write and `6E400003` with notify (Nordic UART RX/TX) |
| Audio | none — audio capability is system-exposed rather than peripheral-defined |

## Reference profiles

`reference_profiles()` returns the directive's ten categories as versioned
`1.0.0` definitions (`bluetooth.generic-gatt`, `bluetooth.battery`,
`bluetooth.environmental`, `bluetooth.heart-rate`, `bluetooth.hid`,
`bluetooth.controller`, `bluetooth.accessibility-switch`, `bluetooth.serial`,
`bluetooth.audio`, `bluetooth.custom`), each satisfying its category
invariants. The conformance test normalizes all ten and registers them into
one bounded registry. Reference profiles are starting points for
declarations, not claims about any specific device.

## Session semantics

`ProfileRegistry::register` re-validates the record and its invariants and
enforces identifier uniqueness and the registry bound (≤ 16);
`profile(id)` looks up by exact identifier; `len`/`is_empty` observe the
set. Records are immutable after registration.

## Canonical IR

`canonical_for` emits: `contract`, `recipe_version`, `target`,
`session=ble_profile_bounded_classification`, `profile_id`, `category`,
`version`, `required`, `optional`, `profiles_registered=none`,
`devices_classified=none`, `spec_validation=category_invariants_only`.

## Emission guarantees

1. Identical inputs produce byte-identical canonical IR and source text; the
   rustc-piped harness test proves the generated factory compiles, registers
   the profile, and answers exact lookups.
2. Fail-closed normalization: invariant violations (including wrong
   characteristic properties) are rejected before negotiation.
3. Runtime unit tests pin id/version/characteristic validation, the full
   invariant table, registry bounds, and duplicate rejection.

## Not claimed

Device discovery or classification against real hardware, firmware-level
behaviour verification, or hardware conformance.
