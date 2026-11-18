# BLUE-MOON-3 — Blue Moon characteristic notification stream contract

Contract identifier: `goya.hardware.bluetooth.stream.v1`. Status:
**Candidate**, 18 November 2026. Architecture approval is required before a
consumer may cite it as a frozen contract (`docs/contracts.md`).
Implementation: `crates/goya-caprichios/src/bluetooth_stream_caprichio.rs`
(typed declaration, canonical IR, fail-closed lowering),
`crates/goya-caprichios/src/bluetooth_runtime.rs` (`StreamSession`),
`crates/goya-caprichios/src/registry.rs` (negotiation).

BLUE-MOON-3 covers the BM-05 Stream core as one bounded recipe: declared
subscribable characteristics with fixed transports, bounded FIFO intake with
fail-closed backpressure, deterministic subscription cancellation, indication
confirmations, intake observability, and a typed little-endian `int16`
decoder matching the directive's illustrative environmental-sensor encoding.

## Boundary

- The generated artifact is a Rust factory that constructs a
  `bluetooth_runtime::StreamSession`. No subscription is established, no
  notification arrives, and no Core Bluetooth framework is linked by this
  crate.
- Hosts report their real notification facilities through
  `RuntimeCapabilities`. The transport facilities are **conditional** (the
  directive §7.2 CONDITIONAL state): only the transports the declaration
  actually uses are required — a notifications-only declaration does not
  demand indication support and vice versa.
- Delivery is never guaranteed. Notifications are unacknowledged at the
  attribute layer; indications are confirmed only through explicitly
  reported confirmations. The canonical IR carries `delivery_guarantee=none`.

## Pipeline

```text
BluetoothStreamDeclaration
        │  normalize (count, uniqueness, bounds)
        ▼
BluetoothStreamIr  ── canonical_for(target) ──▶ canonical IR text
        │  lower_checked(target, RuntimeCapabilities)   ← conditional facilities
        ▼
BluetoothStreamArtifact { contract, recipe_version, canonical_ir, source }
        │  registry::negotiate
        ▼
NegotiatedCapability { id, artifact: Artifact::BluetoothStream }
```

Targets: `Ios` and `AppleSiliconMac` lower. Recipe:
`apple-ble-stream-bounded-notifications@0.0.1`.

## Declaration shape

| Field | Type | Bound | Rejects |
| --- | --- | --- | --- |
| `characteristics` | `Vec<StreamCharacteristic>` | 1–16, unique UUIDs, transport fixed per characteristic | `NoCharacteristics`, `TooManyCharacteristics`, `DuplicateCharacteristic` |
| `max_notification_bytes` | `usize` | 1–512 (negotiated transport limit) | `ZeroNotificationBytes`, `NotificationBytesTooLarge` |
| `max_buffered_notifications` | `u16` | 1–256 | `ZeroBufferedNotifications`, `BufferedNotificationsTooLarge` |

## Session semantics

- `subscribe(uuid)` activates one declared subscription; the transport is
  fixed by the declaration. A real host must still verify the characteristic
  supports that transport before enabling a genuine subscription (directive
  §6.2 note); this model records the declared policy only.
- `receive(uuid, payload)` accepts an active subscription's notification:
  over-limit payloads fail closed (`PayloadTooLarge`); a full buffer rejects
  the notification with `BufferExhausted` and counts the drop (BM-R009
  adjacent) instead of evicting or growing unbounded. Accepted items get
  monotonically increasing ordinals, so intake order is observable and
  preserved by `take()`.
- `confirm(ordinal)` records an attribute-layer indication confirmation;
  only indication transports create pending confirmations, and confirming a
  notification ordinal fails with `UnknownOrdinal`.
- `cancel(uuid)` deactivates a subscription and deterministically discards
  its buffered notifications and pending confirmations; cancelled items are
  counted separately from drops. Deactivated subscriptions may be
  re-subscribed.
- Observability: `received()`, `dropped()`, `cancelled()`, `buffered()`, and
  per-subscription `received_from` / `dropped_from`.
- `decode_i16_le(payload)` decodes a two-byte little-endian value and rejects
  any other length with `MalformedPayload`; scaling to engineering units is
  application-level.

## Canonical IR

`canonical_for` emits: `contract`, `recipe_version`, `target`,
`session=ble_stream_bounded_notifications`, `max_notification_bytes`,
`max_buffered_notifications`, `characteristics`
(`uuid:transport` comma-joined in declaration order),
`notifications_received=none`, `delivery_guarantee=none`.

## Emission guarantees

1. Identical inputs produce byte-identical canonical IR and source text; the
   rustc-piped harness test proves the generated factory compiles and
   behaves against `bluetooth_runtime.rs`.
2. Conditional facility resolution: lowering fails closed only on transports
   the declaration uses; unsupported targets and missing shared facilities
   always reject.
3. The runtime unit tests pin bounded intake, ordering, backpressure
   accounting, confirmation semantics, cancellation cleanup, and decoder
   strictness.

## Not claimed

Throughput, latency, delivery, reassembly of fragmented ATT payloads,
ordering guarantees beyond intake accounting, background execution, or any
hardware conformance. Native Core Bluetooth execution remains roadmap
phase 6.
