# BLUE-MOON-1 — Blue Moon BLE central discovery and link contract

Contract identifier: `goya.hardware.bluetooth.central.v1`. Status:
**Candidate**, 18 November 2026. Architecture approval is required before a
consumer may cite it as a frozen contract (`docs/contracts.md`).
Implementation: `crates/goya-caprichios/src/bluetooth_caprichio.rs` (typed
declaration, canonical IR, fail-closed lowering), `crates/goya-caprichios/src/bluetooth_runtime.rs`
(process-local runtime), `crates/goya-caprichios/src/registry.rs` (negotiation).

BLUE-MOON-1 is the first implemented contract of the Blue Moon family (the
Apple Bluetooth accessibility and hardware-interoperability programme). It
covers the BM-01 Discovery and BM-02 Link core as one bounded recipe: a BLE
central scan restricted to declared service UUID filters, a peripheral
lifecycle that keeps detection, recognition, connection and operation as
distinct states, and a bounded connection lifecycle with deterministic
reconnection backoff.

## Boundary

This contract is a deterministic construction boundary, not a Bluetooth stack.
It is what the existing Apple hardware caprichios are for Touch ID, Face ID,
Apple Pay and Windows Hello, extended to Bluetooth.

- The generated artifact is a Rust factory that constructs a
  `bluetooth_runtime::CentralSession`. No radio is driven, no advertisement is
  received, no peripheral is contacted and no Core Bluetooth framework is
  linked by this crate.
- Hosts running on real Apple hardware report their real facilities through
  `RuntimeCapabilities` (`core_bluetooth_central`, `bluetooth_powered`,
  `bluetooth_permission`, `utf8_strings`). Negotiation fails closed on every
  missing facility; the default `RuntimeCapabilities` supports nothing.
- A constructed session proves nothing about hardware: no discovery latency,
  no connection success rate, no reconnection behaviour on real firmware, and
  no platform conformance is claimed or measurable from this recipe. The
  directive's hardware-in-the-loop requirements (physical iPhone, programmable
  BLE peripheral) are not satisfied by this contract and are tracked in the
  roadmap.

## Pipeline

```text
BluetoothCentralDeclaration
        │  normalize (fail-closed bounds validation)
        ▼
BluetoothCentralIr  ── canonical_for(target) ──▶ canonical IR text
        │  lower_checked(target, RuntimeCapabilities)
        ▼
BluetoothCentralArtifact { contract, recipe_version, canonical_ir, source }
        │  registry::negotiate (contract ID resolved in lexical order)
        ▼
NegotiatedCapability { id, artifact: Artifact::Bluetooth }
```

Targets: `Ios` and `AppleSiliconMac` lower; `CrossPlatform`, `Browser` and
`Node` fail with `UnsupportedTarget`. Recipe:
`apple-ble-central-bounded-scan@0.0.1`.

## Declaration shape

| Field | Type | Bound | Rejects |
| --- | --- | --- | --- |
| `service_uuids` | `Vec<String>` | 1–8 filters, each ≤ 64 bytes | `NoServiceUuids`, `TooManyServiceUuids`, `EmptyServiceUuid`, `ServiceUuidTooLong` |
| `max_scan_ms` | `u32` | 1–300000 | `ZeroScanBound`, `ScanBoundTooLarge` |
| `max_peripherals` | `u16` | 1–32 | `ZeroPeripheralBound`, `PeripheralBoundTooLarge` |
| `connection_timeout_ms` | `u32` | 1–60000 | `ZeroConnectionTimeout`, `ConnectionTimeoutTooLarge` |
| `max_reconnect_attempts` | `u8` | 0–8 (0 disables reconnection) | `ReconnectBoundTooLarge` |

An empty service UUID filter is rejected by design: scanning is restricted to
declared application requirements (Blue Moon directive §10).

## Peripheral lifecycle

`Detected → Recognised → Connected → Operational`, with `Disconnected`
reachable from `Connected`/`Operational` and `ReconnectPlan` returning a
disconnected peripheral to `Connected`. The states are never interchangeable:
a detected peripheral is not connectable (connections are authorised only for
peripherals whose advertisements matched a declared service UUID), a connected
peripheral is not operational until `mark_operational`, and a disconnected
peripheral needs `reconnect`, not `connect`. Duplicate advertisement reports
are suppressed; the latest RSSI (validated to the Bluetooth range
−127..0 dBm) is kept. The tracked-peripheral set is bounded by
`max_peripherals`; excess reports fail with `PeripheralBoundReached` rather
than evicting or growing unbounded.

Reconnection attempts are bounded per peripheral. Backoff doubles from 100 ms
and saturates at 5000 ms; the attempt ordinal and backoff are returned as a
`ReconnectPlan` so callers schedule the wait. Connections carry stable,
deterministic application-level session identifiers (`conn-<peripheral>-<n>`).

## Canonical IR

`canonical_for` emits, in order: `contract`, `recipe_version`, `target`,
`session=ble_central_scan`, `max_scan_ms`, `service_uuids` (comma-joined in
declaration order), `max_peripherals`, `connection_timeout_ms`,
`max_reconnect_attempts`, `radio_driven=none`, `peripherals_connected=none`.
The final two lines are the bounded-disclaimer contract lines asserted by the
conformance tests, in the manner of Apple Pay's `funds_moved=none`.

## Emission guarantees

1. Identical declaration, target and capability inputs produce byte-identical
   canonical IR and source text; the generated factory compiles and behaves
   against `bluetooth_runtime.rs` (the rustc-piped harness test
   `generated_factory_compiles_and_runs_against_bluetooth_runtime` proves
   compilation and behaviour, not hardware).
2. Every unsupported target and every missing host facility fails closed with
   a stable `Diagnostic` variant; nothing degrades silently.
3. Registry negotiation of the contract ID is deterministic (lexical order)
   and rejects duplicate, unknown, draft-catalog, missing-declaration and
   unexpected-declaration requests with typed errors.

## Diagnostics

Compile-level diagnostics are the `Diagnostic` enum. They correspond to the
directive's stable-ID scheme as follows; the BM- identifiers are recorded
here for traceability and are not emitted by this crate.

| Diagnostic | Directive mapping |
| --- | --- |
| `UnsupportedTarget` | BM-E001 (unsupported target capability) |
| `MissingBluetoothPermission` | BM-E002/BM-R002 (privacy declaration / permission denied) |
| `NoServiceUuids`, bounds violations | BM-E004 (invalid device profile inputs) |
| `MissingCoreBluetoothCentral`, `MissingBluetoothPower` | BM-E007/BM-R001 (unresolved framework / Bluetooth unavailable) |
| `PeripheralBoundReached`, `NoRetryRemaining` | BM-R009-adjacent resource bounds |

The directive's runtime diagnostics (BM-R003 discovery timeout, BM-R004
connection timeout, BM-R005 disconnection) model radio events; they require
the native Core Bluetooth adapter and are not claimed by BLUE-MOON-1.

## Not claimed

Native Core Bluetooth execution, AccessorySetupKit or External Accessory
integration, GATT characteristic operations, notifications, background state
restoration, entitlement or `NSBluetoothAlwaysUsageDescription` generation,
and any physical-device conformance. GATT operations are BLUE-MOON-2;
notifications are BLUE-MOON-3. The remaining phases are
`docs/blue-moon/roadmap.md`.
