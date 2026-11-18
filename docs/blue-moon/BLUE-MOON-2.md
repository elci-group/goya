# BLUE-MOON-2 — Blue Moon GATT characteristic command contract

Contract identifier: `goya.hardware.bluetooth.gatt.v1`. Status:
**Candidate**, 18 November 2026. Architecture approval is required before a
consumer may cite it as a frozen contract (`docs/contracts.md`).
Implementation: `crates/goya-caprichios/src/bluetooth_control_caprichio.rs`
(typed declaration, canonical IR, fail-closed lowering),
`crates/goya-caprichios/src/bluetooth_runtime.rs` (`GattSession`),
`crates/goya-caprichios/src/registry.rs` (negotiation).

BLUE-MOON-2 covers the BM-04 Control core as one bounded recipe: profile-
defined characteristic policies, planned reads, writes in acknowledged and
unacknowledged modes, a bounded pending-acknowledgement ledger, and value
validation for host-supplied observed data.

## Boundary

- The generated artifact is a Rust factory that constructs a
  `bluetooth_runtime::GattSession`. No characteristic operation runs, no
  command is encoded onto a radio, and no Core Bluetooth framework is linked
  by this crate.
- Hosts report their real GATT facilities through `RuntimeCapabilities`
  (`core_bluetooth_gatt`, `bluetooth_powered`, `bluetooth_permission`,
  `utf8_strings`); negotiation fails closed on every missing facility.
- A planned write is transport accounting only. The canonical IR carries
  `physical_execution=none` and the artifact documentation states the same:
  directive §BM-04's rule that a successful Bluetooth write must not be
  represented as successful physical execution is structural here, not a
  note.

## Pipeline

```text
BluetoothGattDeclaration
        │  normalize (count, uniqueness, timeout bounds)
        ▼
BluetoothGattIr  ── canonical_for(target) ──▶ canonical IR text
        │  lower_checked(target, RuntimeCapabilities)
        ▼
BluetoothGattArtifact { contract, recipe_version, canonical_ir, source }
        │  registry::negotiate
        ▼
NegotiatedCapability { id, artifact: Artifact::BluetoothGatt }
```

Targets: `Ios` and `AppleSiliconMac` lower. Recipe:
`apple-ble-gatt-bounded-commands@0.0.1`.

## Declaration shape

| Field | Type | Bound | Rejects |
| --- | --- | --- | --- |
| `characteristics` | `Vec<CharacteristicPolicy>` | 1–64, unique UUIDs | `NoCharacteristics`, `TooManyCharacteristics`, `DuplicateCharacteristic` |
| `max_command_timeout_ms` | `u32` | 1–60000 | `ZeroCommandTimeout`, `CommandTimeoutTooLarge` |

`CharacteristicPolicy::new(uuid, access, max_value_bytes)` validates its own
inputs: non-empty UUID ≤ 64 bytes (`EmptyServiceUuid`, `ServiceUuidTooLong`),
value bound 1–512 bytes (`ZeroValueBound`, `ValueTooLarge`). `GattAccess` is
`Read`, `Write` or `ReadWrite`; every operation checks the policy before
proceeding (`UnsupportedOperation`).

## Session semantics

- `read(uuid)` plans a read and returns a `ReadReceipt` (stable command id
  `cmd-<n>`, characteristic UUID, value bound). The observed value is
  supplied by the host and validated by `validate_value` against the
  characteristic bound (malformed-payload mitigation, fail closed).
- `write(uuid, mode, value)` issues a write under `WriteMode::Acknowledged`
  or `WriteMode::Unacknowledged`. Acknowledged writes enter a bounded ledger
  (`MAX_PENDING_COMMANDS = 256`) until `acknowledge(command_id)` or
  `cancel(command_id)` resolves them; an unacknowledged write resolves at
  issuance and is never confirmed. Over-bound values fail closed before any
  ledger entry.
- Values are never produced by this model. The host reports observed data;
  the session only validates bounds. Device-specific command encoding,
  response decoding beyond bound validation, and actuator safety policies
  remain profile-level concerns (roadmap phase 4).

## Canonical IR

`canonical_for` emits: `contract`, `recipe_version`, `target`,
`session=ble_gatt_bounded_commands`, `max_command_timeout_ms`,
`characteristics` (`uuid:access:bound` comma-joined in declaration order),
`commands_issued=none`, `physical_execution=none`.

## Emission guarantees

1. Identical inputs produce byte-identical canonical IR and source text; the
   rustc-piped harness test proves the generated factory compiles and
   behaves against `bluetooth_runtime.rs`.
2. Fail-closed negotiation: unsupported targets and missing facilities reject
   with stable `Diagnostic` variants.
3. The 24 runtime unit tests pin the state machine: per-operation policy
   checks, ledger bounds, explicit resolution, and value validation.

## Not claimed

Any real characteristic operation, pairing, bonding, encrypted transport
behaviour, physical actuation, or hardware conformance. Stream transports
are BLUE-MOON-3; native Core Bluetooth execution remains phase 6.
