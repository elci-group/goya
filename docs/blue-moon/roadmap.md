# Blue Moon roadmap

Status: proposed, 18 November 2026 (phase 2 complete same day). This is a
plan, not evidence. Nothing on this page claims Bluetooth hardware
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

### Phase 3 — Guard and Continuity contracts (next)

- [ ] `goya.hardware.bluetooth.guard.v1` — permission-state modelling,
      accessory authorisation accounting, least-privilege declaration
      validation (rejecting empty/generic usage descriptions), bounded
      advertisement payload validation.
- [ ] `goya.hardware.bluetooth.continuity.v1` — persisted association and
      restoration-intent modelling with explicit handling of Apple's
      conditional restoration guarantees (no indefinite execution claimed).

### Phase 4 — Access (Gestures integration) and Profile schema

- [ ] `goya.hardware.bluetooth.access.v1` — semantic input mapping from
      supported hardware events, debouncing, long-press detection where raw
      events are available; system-managed accessibility devices remain
      system-managed.
- [ ] Versioned device profile schema and reference profiles (Generic GATT,
      Battery, Environmental, Heart Rate, HID, Controller, Accessibility
      Switch, Serial, Audio, Custom), building on BLUE-MOON-2's
      characteristic policies.

### Phase 5 — Goya language and Neptune integration

- [ ] Bluetooth declarations in `goya-lang` (grammar, semantic validation).
- [ ] Neptune consumes registry-negotiated artifacts; entitlement and
      privacy-key generation (`NSBluetoothAlwaysUsageDescription` and
      platform variants); capability resolution in the compiler pipeline.
- [ ] This phase requires the Neptune build pipeline to exist for real
      (`docs/neptune-roadmap.md` N-phases); no date is claimed.

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

Exactly three implemented Bluetooth contracts:
`goya.hardware.bluetooth.central.v1` (BLUE-MOON-1),
`goya.hardware.bluetooth.gatt.v1` (BLUE-MOON-2) and
`goya.hardware.bluetooth.stream.v1` (BLUE-MOON-3). The 80 draft catalog
entries remain unimplemented design intent. No other Bluetooth capability —
advertising, pairing, profiles, audio routing, watchOS/tvOS/visionOS targets
— is implemented, and none may be reported as such.
