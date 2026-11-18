# Blue Moon roadmap

Status: proposed, 18 November 2026 (phases 1–4 complete; phase 5 started).
This is a plan, not evidence. Nothing on this page claims Bluetooth hardware
conformance, native Core Bluetooth execution, or compiler integration that
does not yet exist.

Blue Moon is the Apple Bluetooth accessibility and hardware-interoperability
programme: a family of Caprichios enabling Goya-authored applications to
discover, connect to, and interact with supported Bluetooth peripherals
across Apple platforms. The directive calls for eight caprichios (Discovery,
Link, Access, Control, Stream, Guard, Continuity, Profile), native Swift
adapters, physical hardware conformance, comparative benchmarks and eight
demonstration applications.

## Reconciliation with the repository as it exists

The directive was written against the Goya ecosystem without inspection of
this repository. Audited on 18 November 2026:

| Directive assumption | Repository reality |
| --- | --- |
| Caprichios are a family tree of manifests under `caprichios/apple/blue-moon/` | `crates/goya-caprichios` is a single std-only crate; each caprichio is a module with a typed declaration, canonical IR, fail-closed lowering and a generated Rust factory. There is no family manifest or Swift template system. |
| Neptune consumes Blue Moon contracts and generates Swift + entitlements | Neptune (`crates/neptune`) does not reference caprichios at all; its capability metadata is inline JSON marked `apple_validation_not_run`. |
| Bluetooth declared in Goya source (`bluetooth Sensor { … }`) | The Goya grammar (`crates/goya-lang`) has no device/hardware declarations; effect kinds are `network/filesystem/clock/random/global_mutable`. |
| Hardware-in-the-loop validation on physical iPhone/iPad/Mac | This development environment is Linux. Physical Apple hardware and programmable BLE peripherals are prerequisites that must be provisioned before phases 4+ can produce evidence. |
| 8 demonstration applications | No Goya application packaging exists for Apple targets yet (Neptune has no worker, signing or device run). |

The implementing strategy therefore follows the repository's own proven
pattern first: bounded, deterministic, fail-closed construction contracts with
process-local runtime models, negotiated by the registry — the same pattern
already shipped for Touch ID, Face ID, Windows Hello, Apple Pay, Google Pay
and Samsung Pay. Native Core Bluetooth execution requires Neptune-side
integration and real Apple hardware; both are later phases, and no phase
claims them before evidence exists.

Incident note, 18 November 2026: an automated versioning daemon (`kaptaind`)
rewrote repository history mid-session and swept the working tree; the first
Blue Moon increment was rebuilt from session records in the same day, and the
sweep's partial state (committed repairs but missing new files) was
reconciled before phase 2.

## Definition of done for the programme

A Blue Moon phase is done when it has executable tests, machine-readable gate
results and documented implementation evidence that name exactly what is and
is not proven — per the directive: no mocked implementations reported as
native, no platform support claimed from successful compilation alone.

## Phases

### Phase 0 — Repository audit (complete)

Exit: this document. Establishes the reconciliation table above.

### Phase 1 — Foundational contracts (complete 18 November 2026; rebuilt after the kaptaind sweep)

Implemented as BLUE-MOON-1 (`docs/blue-moon/BLUE-MOON-1.md`):

- [x] `bluetooth_caprichio` — typed `BluetoothCentralDeclaration` → canonical
      IR → fail-closed `lower_checked` → generated factory; stable
      `Diagnostic` variants; targets iOS and Apple Silicon Mac.
- [x] `bluetooth_runtime::CentralSession` — bounded scan accounting with
      duplicate suppression and RSSI validation; peripheral state machine
      keeping detected/recognised/connected/operational/disconnected
      distinct; bounded peripheral set; bounded reconnection with
      deterministic exponential backoff; stable session identifiers.
- [x] Registry negotiation of `goya.hardware.bluetooth.central.v1`
      (fail-closed on missing facilities, unsupported targets, unfiltered
      scans).
- [x] Conformance tests: crate lib suite including the rustc-piped
      generated-factory harness.
- [x] Baseline repair: the in-flight Windows Hello, Google Pay and Samsung
      Pay registry wiring was non-compiling (non-exhaustive match) and
      panicked on negotiation (missing lowering arms); both fixed and covered
      by `windows_hello_google_pay_and_samsung_pay_contracts_lower_without_panicking`.

### Phase 2 — Control and Stream contracts (complete 18 November 2026)

- [x] BLUE-MOON-2 (`docs/blue-moon/BLUE-MOON-2.md`,
      `goya.hardware.bluetooth.gatt.v1`): `bluetooth_control_caprichio` with
      profile-defined `CharacteristicPolicy` constraints (access + value
      bound), planned reads, acknowledged/unacknowledged writes, a bounded
      pending-acknowledgement ledger, and explicit `physical_execution=none`
      semantics — a write is never reported as physical execution.
- [x] BLUE-MOON-3 (`docs/blue-moon/BLUE-MOON-3.md`,
      `goya.hardware.bluetooth.stream.v1`): `bluetooth_stream_caprichio` with
      fixed per-characteristic transports, bounded FIFO intake, fail-closed
      backpressure with drop accounting, deterministic subscription
      cancellation, indication confirmations, per-stream observability, and
      strict `decode_i16_le` typed decoding.
- [x] Conditional facility resolution demonstrated: a notifications-only
      declaration does not require indication support; both transports
      required only when declared.
- [x] Both extend `bluetooth_runtime` and follow the BLUE-MOON-1 pattern;
      each has a rustc-piped generated-factory conformance test.
- [ ] Profile-defined device constraint schema beyond characteristic bounds
      (moved to phase 4 with the Profile caprichio).

### Phase 3 — Guard and Continuity contracts (complete 18 November 2026)

- [x] BLUE-MOON-4 (`docs/blue-moon/BLUE-MOON-4.md`,
      `goya.hardware.bluetooth.guard.v1`): `bluetooth_guard_caprichio` with
      permission-state decisions that fail closed (undetermined, denied,
      unselected), explicit-only peripheral authorisation with a bounded
      ledger, generic-placeholder privacy-description rejection (directive
      §9.2), mandatory explicit selection that cannot be disabled, bounded
      advertisement-payload validation, and audited decision counters.
- [x] BLUE-MOON-5 (`docs/blue-moon/BLUE-MOON-5.md`,
      `goya.hardware.bluetooth.continuity.v1`): `bluetooth_continuity_caprichio`
      with a bounded persisted-association ledger (intent + compatible
      subscriptions + revisions), suspension handling for Bluetooth power
      and app lifecycle, restoration *planning* separated from outcomes, and
      observed-device reconciliation; cross-launch persistence requires
      restoration and storage facilities only when declared, and the
      canonical IR states `relaunch_promised=none` per the directive's
      conditional-restoration rule.
- [x] Both extend `bluetooth_runtime` and follow the BLUE-MOON-1 pattern;
      each has a rustc-piped generated-factory conformance test.

### Phase 4 — Access (Gestures integration) and Profile schema (complete 18 November 2026)

- [x] BLUE-MOON-6 (`docs/blue-moon/BLUE-MOON-6.md`,
      `goya.hardware.bluetooth.access.v1`): `bluetooth_access_caprichio` with
      semantic input mappings (Select/Confirm/Navigate/Back/Custom) over
      accessibility switches, adaptive controllers, game controllers and
      custom buttons; single/double/long-press recognition from raw
      press/release events with host-supplied monotonic timestamps;
      debouncing; accessible labels on every binding; unmapped events
      reported, never ignored; and structural protection of system-managed
      accessibility devices (`intercept_system_managed` declarations are
      rejected; keyboards/wearables/hearing devices follow system paths and
      are not modelled).
- [x] BLUE-MOON-7 (`docs/blue-moon/BLUE-MOON-7.md`,
      `goya.hardware.bluetooth.profile.v1`): versioned device-profile schema
      (`DeviceProfileRecord` + `ProfileCharacteristic` with observable GATT
      properties) validated against per-category specification invariants,
      a bounded `ProfileRegistry`, and all ten directive reference profiles
      (`bluetooth.generic-gatt` … `bluetooth.custom`), each proven to
      normalize and register in conformance tests.
- [x] Both extend `bluetooth_runtime` and follow the BLUE-MOON-1 pattern;
      each has a rustc-piped generated-factory conformance test.

### Phase 5 — Goya language and Neptune integration (in progress)

- [x] BLUE-MOON-8 (`docs/blue-moon/BLUE-MOON-8.md`): Neptune consumes
      goya-caprichios for the first time. `crates/neptune/src/bluetooth.rs`
      generates capability-inventory entries for all seven implemented Blue
      Moon contracts (existing `capabilities.json` shape, `CoreBluetooth`
      frameworks, fail-closed on unknown IDs with `NPT-BM-E001`) and plans
      Apple privacy declarations (§9.2) from Guard-validated usage
      descriptions (`NSBluetoothAlwaysUsageDescription` +
      `NSBluetoothPeripheralUsageDescription`, empty background modes,
      generic/placeholder descriptions rejected with `NPT-BM-E002`), plus a
      deterministic Info.plist privacy *fragment*. Exposed through the
      service protocol as `plan-bluetooth-privacy`.
- [ ] Bluetooth declarations in `goya-lang` (grammar, semantic validation):
      **contract-gated**. `EffectKind` is part of the frozen
      `GY-DETERMINISM/1` contract with stable canonical serialisation
      (`goya-determinism`), consumed by `goya-lang`, `goya-executable`
      (D1/D2 VMs, D2 journal, bytecode) and `goya-vee`. Adding a `bluetooth`
      variant is a contract revision requiring `docs/contracts.md` approval,
      not an incremental patch.
- [ ] Capability resolution inside the compile pipeline and entitlement /
      plist *generation into builds*: requires the Neptune build pipeline to
      exist for real (`docs/neptune-roadmap.md` N-phases); no date is
      claimed.

### Phase 6 — Native Core Bluetooth adapters and physical conformance

Prerequisites: Apple toolchain and physical hardware provisioned (iPhone,
iPad, Apple Silicon Mac, programmable BLE peripheral such as an nRF52/ESP32
fixture, adaptive input hardware). Until then this phase is blocked, and
blocked mandatory gates prevent production qualification (directive §16).

- [ ] Native Swift adapters replacing process-local runtime models for
      real hardware paths.
- [ ] Hardware-in-the-loop fault scenarios (malformed notifications, abrupt
      disconnects, power transitions, permission revocation).

### Phase 7 — Qualification

- [ ] Comparative benchmarks against hand-written Swift baselines (directive
      §12 thresholds; ≥30 repetitions; median/p95/p99 recording).
- [ ] Release gates BM-G01–BM-G15 with PASS/FAIL/BLOCKED/NOT_APPLICABLE
      classifications.

## Current supported surface

Exactly seven implemented Bluetooth contracts:
`goya.hardware.bluetooth.access.v1` (BLUE-MOON-6),
`goya.hardware.bluetooth.central.v1` (BLUE-MOON-1),
`goya.hardware.bluetooth.continuity.v1` (BLUE-MOON-5),
`goya.hardware.bluetooth.gatt.v1` (BLUE-MOON-2),
`goya.hardware.bluetooth.guard.v1` (BLUE-MOON-4),
`goya.hardware.bluetooth.profile.v1` (BLUE-MOON-7) and
`goya.hardware.bluetooth.stream.v1` (BLUE-MOON-3). This covers six of the
directive's eight caprichios (Discovery, Link, Access, Control, Stream,
Guard, Continuity, Profile) as deterministic construction contracts; only
native execution is outstanding. The 80 draft catalog entries remain
unimplemented design intent. No other Bluetooth capability — advertising,
pairing, audio routing, watchOS/tvOS/visionOS targets — is implemented, and
none may be reported as such.
