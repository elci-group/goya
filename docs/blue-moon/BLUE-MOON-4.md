# BLUE-MOON-4 — Blue Moon Bluetooth guard contract

Contract identifier: `goya.hardware.bluetooth.guard.v1`. Status:
**Candidate**, 18 November 2026. Architecture approval is required before a
consumer may cite it as a frozen contract (`docs/contracts.md`).
Implementation: `crates/goya-caprichios/src/bluetooth_guard_caprichio.rs`
(typed declaration, canonical IR, fail-closed lowering),
`crates/goya-caprichios/src/bluetooth_runtime.rs` (`GuardSession`,
`validate_usage_description`), `crates/goya-caprichios/src/registry.rs`
(negotiation).

BLUE-MOON-4 covers the BM-06 Guard core as one bounded recipe: application-
level permission-state decisions, explicit-only peripheral authorisation with
a bounded ledger, least-privilege privacy-description validation, bounded
advertisement-payload validation, and auditable decision accounting.

## Boundary

- The generated artifact is a Rust factory that constructs a
  `bluetooth_runtime::GuardSession`. No permission manager is queried, no
  device identity is verified, no usage-description string is generated, and
  no Core Bluetooth framework is linked by this crate.
- Hosts report their real permission and storage facilities through
  `RuntimeCapabilities` (`bluetooth_permission_query`,
  `bluetooth_authorisation_store`, `utf8_strings`).
- Guard records policy decisions only. Per the directive's threat model,
  advertised names, service UUIDs and manufacturer identifiers are never
  treated as cryptographic proof of identity; authorisation comes solely
  from explicit application selection.

## Pipeline

```text
BluetoothGuardDeclaration
        │  normalize (description validation, bounds, mandatory selection)
        ▼
BluetoothGuardIr  ── canonical_for(target) ──▶ canonical IR text
        │  lower_checked(target, RuntimeCapabilities)
        ▼
BluetoothGuardArtifact { contract, recipe_version, canonical_ir, source }
        │  registry::negotiate
        ▼
NegotiatedCapability { id, artifact: Artifact::BluetoothGuard }
```

Targets: `Ios` and `AppleSiliconMac` lower. Recipe:
`apple-ble-guard-bounded-authorisation@0.0.1`.

## Declaration shape

| Field | Type | Bound / rule | Rejects |
| --- | --- | --- | --- |
| `usage_description` | `String` | ≤ 256 bytes; non-empty; not in the generic-placeholder set; must contain alphanumeric text | `GenericUsageDescription`, `UsageDescriptionTooLong` |
| `max_authorised_peripherals` | `u16` | 1–16 | `ZeroAuthorisationBound`, `AuthorisationBoundTooLarge` |
| `enforce_explicit_selection` | `bool` | must be `true` | `ExplicitSelectionCannotBeDisabled` |

The generic-placeholder set (`GENERIC_USAGE_DESCRIPTIONS`) rejects — among
others — empty text, `bluetooth`, `uses bluetooth`, `required`, `todo` and
punctuation-only strings, compared case-insensitively after trimming. The
directive's §9.2 rule that empty or generic descriptions are rejected in
production builds is enforced at normalization.

## Session semantics

- `authorise(peripheral_id)` — explicit selection only; idempotent; the
  authorised set is bounded and over-bound selections fail with
  `AuthorisationBoundReached`. `revoke` removes; `is_authorised` inspects.
- `check_connection(peripheral_id, permission)` — the mandatory fail-closed
  gate: `Granted` + explicitly selected → `ConnectionDecision`;
  `NotDetermined` → `PermissionUndetermined`; `Denied` →
  `PermissionDenied`; unselected → `NotAuthorised`. Granted and denied
  decisions are both counted (`decisions_granted` / `decisions_denied`) for
  audit.
- `validate_advertisement_payload(payload)` — bounded-parsing mitigation:
  legacy BLE advertisements are ≤ 31 bytes (`MAX_ADVERTISEMENT_BYTES`);
  accepted and rejected payloads are counted.
- Cross-component enforcement ("no other Blue Moon component may bypass
  Guard") is a phase-5 compiler-integration concern; within this crate Guard
  is the only authorisation surface and other sessions do not grant
  anything.

## Canonical IR

`canonical_for` emits: `contract`, `recipe_version`, `target`,
`session=ble_guard_bounded_authorisation`, `usage_description_bytes`,
`max_authorised_peripherals`, `explicit_selection=required`,
`usage_keys_generated=none` (entitlement/usage-key generation is phase 5),
`authorisations_granted=none`.

## Emission guarantees

1. Identical inputs produce byte-identical canonical IR and source text; the
   rustc-piped harness test proves the generated factory compiles and
   behaves against `bluetooth_runtime.rs`.
2. Fail-closed normalization: generic descriptions and disabled explicit
   selection are rejected before any negotiation.
3. Runtime unit tests pin the decision matrix, ledger bounds, idempotent
   re-selection, payload bounds and audit counters.

## Not claimed

Permission manager integration, Keychain-backed ledgers, identity
verification, pairing, entitlement generation, or hardware conformance.
