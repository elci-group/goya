# Persistence scoping — capability decision analysis

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19.
Status: Proposal (decision required). Not a ratified contract; nothing here
amends the contract register or the capability map.

## Purpose

Resolve `PERSISTENCE_CAPABILITY_UNRESOLVED` (12 elements, Reactotron phase 1)
by deciding whether Goya needs a scoped, authority-gated disk-persistence
contract, what its authority model is, and what these elements actually
require. Analysis only; no code changes.

## Observed foreign usage (evidence)

12 of 345 elements carry this reason code. Element-level breakdown from
`evidence/disparates/reactotron-phase1/reference.assessed.fair.json`
(source-call detail verified against the pinned corpus):

| Class | Count | Elements | Observed usage |
|---|---|---|---|
| `electron-store` key-value config | 3 | store-0001 (`src/main/menu.ts:2`), store-0002 (`src/renderer/config.ts:1`), store-0003 (`src/renderer/contexts/Standalone/index.tsx:133`) | typed key-value document (`serverPort`, MCP redaction pattern lists), read-modify-write with defaults merge (`config.set(...)` at `Standalone/index.tsx:133ff`) |
| `fs` in build tooling | 6 | fs-0001 (`scripts/build.release.js:19`), fs-0002..0006 (`scripts/release.artifacts.js:41,52,85,90,199`) | `existsSync`, `readFile`, `writeFileSync` against release artefacts |
| `fs` in application code | 3 | fs-0007 (`src/renderer/pages/reactNative/Overlay.tsx:80`), fs-0008, fs-0009 (`src/renderer/pages/timeline/index.tsx:125,220`) | file reads (`readFile`) on plugin/import paths |

The 12 elements are **two different questions wearing one reason code**:
key-value application state (3), and unstructured file access (9). Only the
first is a persistence question. The second is a filesystem-access question —
GY-COVERAGE/1 already notes "no filesystem contract in register (checked
2026-11-19)" — and 6 of the 9 sit in `scripts/`, which is build tooling, not
application semantics a migration would reconstruct.

GY-STATE/1 provides in-memory revisioned transactions only; GY-HOTSWAP/1
migrates a `goya_state::Map` across state-contract versions. **Neither
declares disk persistence** (checked 2026-11-19, contract register). No
persistence or filesystem crate exists in `crates/` (checked 2026-11-19:
crate listing shows `goya-state`, `goya-hotswap`; nothing else).
GY-CLIPBOARD/1 is the house precedent for authority-gated system access:
distinct capability grants per operation mode, provider trait, memory
substitute, platform backend deferred to the windowing host work.

## Semantic essence

- **Key-value config store** — a durable, typed, atomically replaced
  document of named values; read on start, read-modify-write on settings
  change; survives restart. Essence: *the GY-STATE/1 committed snapshot,
  given a durable home*. The store's semantics (atomic swap, single
  document, version tag) map directly onto existing Goya primitives plus
  exactly one new thing: a disk write discipline.
- **Unstructured file I/O** — arbitrary paths, arbitrary formats. This is a
  general filesystem-access capability, not persistence; it must not be
  smuggled into a persistence contract, and its demand here is mostly
  build-tooling, which reconstruction never targets.

## Options

1. **General file-persistence contract** (open/write/read anywhere) —
   rejected: unbounded authority, no precedent in the register, and 9 of 12
   observed elements don't need it.
2. **Scoped key-value persistence contract** — one durable, version-tagged
   document store, rooted at a fixed scope (application data directory),
   authority-gated per scope; GY-HOTSWAP/1 chains migrate it across
   versions. General file I/O remains out of scope, decided separately.
3. **Defer** — keep `PERSISTENCE_CAPABILITY_UNRESOLVED` open until phase 2
   corpora force the question.
4. **Declare unrepresentable** — wrong: without durable config, no migrated
   application survives restart; this is not an edge capability.

## Recommendation

Option 2: adopt a scoped, authority-gated key-value persistence contract
(proposed name GY-PERSIST/1, name reserved by this document only — no such
crate or contract exists, checked 2026-11-19).

Rationale:

- The element-level split is decisive: the 3 `electron-store` elements are
  exactly a durable GY-STATE/1 snapshot; the 6 `scripts/` fs elements are
  out of reconstruction scope regardless of the decision; the 3 application
  `fs` elements belong to a future filesystem discussion, evidenced
  separately. Grounding: provenance table above.
- The authority model is already established house practice (GY-CLIPBOARD/1):
  `PersistCapability` grant bound to one named scope; no grant, no disk
  access; memory-only fallback for tests. Nothing about that needs invention.
- GY-HOTSWAP/1 already solves the hard half — versioned migration of the
  state document with total-failure semantics. A persistence contract
  without version tags would duplicate that problem badly. Scoping + version
  tags make persistence a thin disk discipline over existing contracts.
- Deferral (option 3) is untenable specifically because the code is
  "decision pending": the decision is now made with 12 elements of evidence
  instead of zero.

## What a GY-PERSIST/1 draft must specify

- **Provides:** `PersistStore` — load/scope/version-tag read, atomic
  replace-on-commit write of a `goya_state::Map`; `PersistCapability`
  grant (one scope per grant, no wildcard scopes); `MemoryPersistStore`
  test substitute with injected failure modes.
- **Authority model:** capability-gated, scoped. A grant names the scope
  (the store root); writes outside the granted scope fail without partial
  state, mirroring GY-HOTSWAP/1's no-partial-result rule. No grant covers
  two scopes; creating a scope is a distinct, operator-level act.
- **Durability discipline:** every commit is atomic (write-temporary-then-
  rename or equivalent), deterministic on replay, and bumps the stored
  version tag only on success — the disk analogue of `GY-STATE/1`'s
  `revision()` invariant.
- **Consumes:** GY-STATE/1 (the document), GY-HOTSWAP/1 (version migration
  chain runs before any stored document at an older version is accepted).
- **Scope exclusions declared up front:** no arbitrary-path file I/O, no
  database/SQLite, no multi-document stores, no network-backed storage.
  Each requires corpus evidence and its own decision.
- **Invariants:** a crashed write leaves the previous document intact;
  load of an un-migratable version fails loudly (no silent defaults);
  identical operation sequences yield byte-identical documents.

## Map revisions required (decision required, not applied)

- Split the reason code at the next map revision: key-value store elements
  → `representable` under GY-PERSIST/1 once ratified (new mapping code);
  application-code `fs` elements → a filesystem-access code (new
  `NO_GOYA_CAPABILITY_FILESYSTEM` or equivalent; codes are added, never
  renamed); build-tooling `fs`/process elements in `scripts/` → a new
  out-of-scope code (proposed `OUT_OF_RECONSTRUCTION_SCOPE`) so build
  tooling stops generating Goya-side demand.
- Until the revision lands, `PERSISTENCE_CAPABILITY_UNRESOLVED` remains
  emitted as-is; consumers should read it as three sub-decisions per the
  table above, not one capability.

## Related observation (recorded, not decided)

The same build-tooling/out-of-scope question applies to
`NO_GOYA_CAPABILITY_CHILD_PROCESS` (9 elements: proc-0001, proc-0002 in
`scripts/`; proc-0003..0009 in `src/main/utils.ts`). A scope split would
reduce that deficit from 9 to 7 genuine application elements. Follow-up
analysis document not yet written.
