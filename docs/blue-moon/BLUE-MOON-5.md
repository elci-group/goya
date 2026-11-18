# BLUE-MOON-5 — Blue Moon Bluetooth continuity contract

Contract identifier: `goya.hardware.bluetooth.continuity.v1`. Status:
**Candidate**, 18 November 2026. Architecture approval is required before a
consumer may cite it as a frozen contract (`docs/contracts.md`).
Implementation: `crates/goya-caprichios/src/bluetooth_continuity_caprichio.rs`
(typed declaration, canonical IR, fail-closed lowering),
`crates/goya-caprichios/src/bluetooth_runtime.rs` (`ContinuityLedger`),
`crates/goya-caprichios/src/registry.rs` (negotiation).

BLUE-MOON-5 covers the BM-07 Continuity core as one bounded recipe: persisted
user-approved associations with restoration intent, compatible-subscription
lists, explicit suspension handling (Bluetooth disabled, application
suspended), intent planning, and reconciliation of persisted intent with
observed devices.

## Boundary

- The generated artifact is a Rust factory that constructs a
  `bluetooth_runtime::ContinuityLedger`. Nothing is written to disk, no
  restoration callback is registered, and no relaunch is observed.
- Hosts report their real facilities through `RuntimeCapabilities`
  (`bluetooth_state_restoration`, `persistent_storage`,
  `bluetooth_power_monitoring`, `utf8_strings`). The restoration and storage
  facilities are **conditional**: required only when the declaration sets
  `persist_across_launches = true`; a session-scoped ledger requires only
  power monitoring.
- Apple's Bluetooth state restoration is conditional and does not guarantee
  indefinite execution or relaunch under every termination condition
  (directive §BM-07). The ledger models that restriction structurally:
  `restore_intent` produces a *plan*, the canonical IR carries
  `relaunch_promised=none`, and no phase of this contract claims a
  restoration outcome.

## Pipeline

```text
BluetoothContinuityDeclaration
        │  normalize (bounds; zero subscriptions valid)
        ▼
BluetoothContinuityIr  ── canonical_for(target) ──▶ canonical IR text
        │  lower_checked(target, RuntimeCapabilities)   ← conditional facilities
        ▼
BluetoothContinuityArtifact { contract, recipe_version, canonical_ir, source }
        │  registry::negotiate
        ▼
NegotiatedCapability { id, artifact: Artifact::BluetoothContinuity }
```

Targets: `Ios` and `AppleSiliconMac` lower. Recipe:
`apple-ble-continuity-bounded-restoration@0.0.1`.

## Declaration shape

| Field | Type | Bound / rule | Rejects |
| --- | --- | --- | --- |
| `max_associations` | `u16` | 1–16 | `ZeroAssociationBound`, `AssociationBoundTooLarge` |
| `max_subscriptions_per_association` | `u16` | 0–8 (0 = association without subscriptions) | `SubscriptionBoundTooLarge` |
| `persist_across_launches` | `bool` | `true` requires restoration + storage facilities | conditional diagnostics |

## Ledger semantics

- `record(peripheral_id, intent, subscriptions)` — records or updates a
  user-approved association with `AssociationIntent::AutoReconnect` or
  `OnDemand` and a bounded compatible-subscription list (each UUID
  validated). Updates keep the lifecycle state and bump the revision.
- `observe_bluetooth_disabled()` / `observe_app_suspended()` — every active
  association suspends with its reason; already-suspended associations keep
  their first reason.
- `restore_intent(peripheral_id)` — plans restoration from the persisted
  record (intent + subscriptions). Planning is accounting only; the counter
  `restorations_planned` grows, and nothing else is claimed.
- `reconcile(peripheral_id, observed)` — reconciliation of persisted intent
  with observed devices: only an *observed* device restores a suspended
  association to active (bumping its revision); an unobserved one keeps its
  suspension; active associations are unchanged.
- `forget(peripheral_id)` — destructive removal.
- Association set is bounded; over-bound records fail with
  `AssociationBoundReached` rather than evicting.

## Canonical IR

`canonical_for` emits: `contract`, `recipe_version`, `target`,
`session=ble_continuity_bounded_restoration`, `max_associations`,
`max_subscriptions_per_association`, `persist_across_launches`,
`associations_recorded=none`, `restoration_planned=none`,
`relaunch_promised=none`.

## Emission guarantees

1. Identical inputs produce byte-identical canonical IR and source text; the
   rustc-piped harness test proves the generated factory compiles and
   behaves against `bluetooth_runtime.rs`.
2. Conditional facility resolution: cross-launch persistence fails closed
   without restoration or storage; session-scoped ledgers do not require
   them.
3. Runtime unit tests pin recording, revisioning, suspension precedence,
   plan-vs-outcome separation, reconciliation, bounds and forget.

## Not claimed

Disk persistence, native state-restoration registration, relaunch handling,
subscription re-establishment against real peripherals, or hardware
conformance. Native adapters remain roadmap phase 6.
