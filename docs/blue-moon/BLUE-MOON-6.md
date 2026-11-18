# BLUE-MOON-6 — Blue Moon Bluetooth access contract

Contract identifier: `goya.hardware.bluetooth.access.v1`. Status:
**Candidate**, 18 November 2026. Architecture approval is required before a
consumer may cite it as a frozen contract (`docs/contracts.md`).
Implementation: `crates/goya-caprichios/src/bluetooth_access_caprichio.rs`
(typed declaration, canonical IR, fail-closed lowering),
`crates/goya-caprichios/src/bluetooth_runtime.rs` (`InputMappingSession`),
`crates/goya-caprichios/src/registry.rs` (negotiation).

BLUE-MOON-6 covers the BM-03 Access core as one bounded recipe: alternative
Bluetooth input devices mapped to semantic application actions (the Gestures
integration point), with single/double/long-press recognition from raw
press/release events, debouncing, accessible labelling, and hard protection
of system-managed accessibility devices.

## Boundary

- The generated artifact is a Rust factory that constructs an
  `bluetooth_runtime::InputMappingSession`. No hardware event is received,
  no semantic action is dispatched into an application, and no assistive
  technology is linked by this crate.
- Hosts report their real accessibility facilities through
  `RuntimeCapabilities` (`accessibility_event_surface`,
  `accessible_labels_supported`, `utf8_strings`).
- The session derives event kinds from host-supplied monotonic timestamps.
  It produces no hardware events and never intercepts protected streams.

## Pipeline

```text
BluetoothAccessDeclaration
        │  normalize (binding table, bounds, protected-stream refusal)
        ▼
BluetoothAccessIr  ── canonical_for(target) ──▶ canonical IR text
        │  lower_checked(target, RuntimeCapabilities)
        ▼
BluetoothAccessArtifact { contract, recipe_version, canonical_ir, source }
        │  registry::negotiate
        ▼
NegotiatedCapability { id, artifact: Artifact::BluetoothAccess }
```

Targets: `Ios` and `AppleSiliconMac` lower. Recipe:
`apple-ble-access-bounded-mapping@0.0.1`.

## Declaration shape

| Field | Type | Bound / rule | Rejects |
| --- | --- | --- | --- |
| `bindings` | `Vec<InputBinding>` | 1–16, unique `(source, event)` keys, each with an accessible label (≤ 128 bytes, non-placeholder) | `NoBindings`, `TooManyBindings`, `DuplicateBinding`, `EmptyLabel`, `LabelTooLong`, `BadActionName` |
| `debounce_ms` | `u32` | 0–1000 (0 disables suppression) | `DebounceBoundTooLarge` |
| `long_press_threshold_ms` | `u32` | 1–5000 | `ZeroLongPressBound`, `LongPressBoundTooLarge` |
| `double_press_window_ms` | `u32` | 1–2000 | `ZeroDoublePressBound`, `DoublePressBoundTooLarge` |
| `intercept_system_managed` | `bool` | must be `false` | `SystemManagedDevicesProtected` |

`InputSource` covers accessibility switches, adaptive controllers, game
controllers and custom BLE buttons. Bluetooth keyboards, wearables and
hearing devices follow system input and audio paths and are deliberately not
modelled — system-managed accessibility devices remain system-managed
(directive §BM-03). `SemanticAction` is `Select`, `Confirm`, `NavigateNext`,
`NavigatePrevious`, `Back` or a bounded ASCII `Custom` name.

## Session semantics

- `press(source, timestamp)` / `release(source, timestamp)` — raw event
  intake with monotonic host timestamps. Press-while-pressed and release
  without press fail with `WrongState`; a release before its press fails
  with `InvalidTimestamp`.
- Recognition at release: duration ≥ threshold → `LongPress`; release
  within the double-press window of a previous single → `DoublePress`;
  otherwise `SinglePress`. Recognition dispatches independently — the model
  never defers or retracts an earlier dispatch (deferred-resolution needs
  async timers and is out of scope for the process-local model).
- Debounce: a repeated identical `(source, kind)` dispatch within the
  window is suppressed and counted.
- Unmapped events fail with `UnmappedEvent` and are counted — unsupported
  capabilities are reported, never silently ignored (directive §2).
- Every dispatched event carries its semantic action and accessible label.

## Canonical IR

`canonical_for` emits: `contract`, `recipe_version`, `target`,
`session=ble_access_bounded_mapping`, `bindings`, `debounce_ms`,
`long_press_threshold_ms`, `double_press_window_ms`,
`system_managed_devices=untouched`, `protected_streams_intercepted=none`.

## Emission guarantees

1. Identical inputs produce byte-identical canonical IR and source text; the
   rustc-piped harness test proves the generated factory compiles and
   behaves against `bluetooth_runtime.rs`.
2. Fail-closed normalization: intercepting declarations and duplicate or
   unlabelled bindings are rejected before negotiation.
3. Runtime unit tests pin recognition (single/double/long), label and
   action validation, bound enforcement, unmapped-event reporting, debounce
   accounting, and timestamp monotonicity.

## Not claimed

Real hardware event intake, VoiceOver labelling delivery, Gestures
integration inside the Goya compiler (phase 5), or hardware conformance.
