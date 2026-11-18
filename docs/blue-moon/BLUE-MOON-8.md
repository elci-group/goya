# BLUE-MOON-8 — Neptune Bluetooth capability metadata and privacy-key planning

Status: **Candidate**, 18 November 2026. This is the first compiler-side
consumption of the Blue Moon contracts.
Implementation: `crates/neptune/src/bluetooth.rs` (module),
`crates/neptune/src/service.rs` (`plan-bluetooth-privacy` operation),
dependency `goya-caprichios` (new for Neptune).

## What it does

- `bluetooth::plan_bluetooth_privacy(description)` — plans the Apple
  Bluetooth privacy declarations (directive §9.2) from one
  application-specific usage description, validated with Blue Moon Guard's
  fail-closed rules: the modern key (`NSBluetoothAlwaysUsageDescription`),
  the legacy key for older deployment targets
  (`NSBluetoothPeripheralUsageDescription`), and an always-empty
  `background_modes` list (no background execution is claimed). Empty,
  generic and placeholder descriptions are rejected with `NPT-BM-E002`.
- `bluetooth::bluetooth_capability_entry/entries` — capability-inventory
  entries for the seven implemented Blue Moon contracts, in the existing
  `capabilities.json` shape (ten fields: `semantic_id`, `adapter`,
  `status`, `minimum_os`, `frameworks: ["CoreBluetooth"]`, `entitlements`,
  `usage_description_keys`, `privacy_declarations: "owner_review_required"`,
  `test_status: "apple_validation_not_run"`, `fallback: "reject"`). Unknown
  contract IDs fail closed with `NPT-BM-E001`; draft IDs and typos are never
  silently emitted. The table is test-checked against
  `goya_caprichios::registry::supported_contracts()`.
- `bluetooth::info_plist_privacy_fragment(plan)` — a deterministic,
  XML-escaped property-list *fragment* carrying both usage keys. This is
  preview data for a future plist emitter; no Info.plist is generated or
  written.
- Service protocol: `plan-bluetooth-privacy` accepts `usage_description`
  (required string) and `contracts` (optional array; defaults to all seven
  implemented contracts in lexical order) and returns the plan and entries
  under `stage: "bluetooth_privacy"`, or `FAIL` diagnostics with the
  `NPT-BM-E00x` codes.

## What it does not do

No Info.plist or entitlement generation into builds, no Swift or
CoreBluetooth emission for Bluetooth, no scene-level Bluetooth declarations,
and no admission of an iOS build. Every entry carries
`apple_validation_not_run`. The `capabilities.json` artifact of the existing
static preview is unchanged.

## Grammar reconciliation (open item)

The Goya-language Bluetooth declaration (directive §6) requires a
`bluetooth` effect kind. `EffectKind` is part of the frozen `GY-DETERMINISM/1`
contract (`crates/goya-determinism/src/lib.rs`) with stable canonical
serialisation, consumed by `goya-lang` (parser, scene), `goya-executable`
(D1 VM, D2 VM, D2 journal, bytecode) and `goya-vee`. Adding a variant is a
contract revision — spec §2 text, the `as_str` canonical serialisation, and
every consumer — and must go through `docs/contracts.md` approval, not an
incremental patch. The audit trail for that revision is recorded here and in
the roadmap; no grammar change was made.
