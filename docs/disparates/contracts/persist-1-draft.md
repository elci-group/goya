# GY-PERSIST/1 — Scoped, authority-gated key-value persistence (draft)

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19.
Status: Draft — proposal requiring programme-owner ratification; unimplemented; no crate exists; coverage rules depending on it are pending and may not emit.

## Purpose

GY-PERSIST/1 (name reserved by
[capabilities/persistence-scoping.md](../capabilities/persistence-scoping.md);
no such crate or register entry exists, checked 2026-11-19) is the proposed
contract for one durable, version-tagged state map scoped to a single granted
root, migrated across schema versions by GY-HOTSWAP/1 chains. It drafts from
that capability analysis and from one observed fact about the corpora: the
`PERSISTENCE_CAPABILITY_UNRESOLVED` reason code is currently two different
questions wearing one code, and only one of them is persistence.

Corpus evidence (element-level, from the assessed FAIR documents;
`state_store` and `filesystem_operation` elements carry provenance only —
file and line, no semantics facets):

| Corpus | Reason-code total | Key-value store (`state_store`) | File access (`filesystem_operation`) |
|---|---|---|---|
| Reactotron phase 1 (345 elements) | 12 | 3 — store-0001 (`src/main/menu.ts:2`), store-0002 (`src/renderer/config.ts:1`), store-0003 (`src/renderer/contexts/Standalone/index.tsx:133`) | 9 — fs-0001..0006 in `scripts/` (build tooling); fs-0007..0009 in `src/renderer/` (application) |
| electron-fiddle phase 2 (401 elements) | 127 | 34 — 19 application (`src/renderer/state.ts`, `src/renderer/versions.ts`) + 15 in `tests/` | 93 — 22 application (`src/`) + 65 in `tests/` + 6 in `tools/` |

What the real populations need, per kind:

- **Application key-value config** (Reactotron store-0001..0003; fiddle's 19
  `src/` state_store elements): typed key-value documents read on start and
  read-modify-written on settings change, surviving restart — the analysis
  verified source calls against the pinned Reactotron corpus (a `serverPort`,
  MCP redaction pattern lists, and read-modify-write with defaults merge at
  `Standalone/index.tsx:133ff`). This is exactly a durable GY-STATE/1
  committed snapshot: the committed document, given a disk home. **This is
  what GY-PERSIST/1 provides.**
- **Application file access** (Reactotron fs-0007..0009; fiddle's 22 `src/`
  fs elements): arbitrary-path reads on plugin, import and data-file paths.
  Not persistence; belongs to the separate
  `NO_GOYA_CAPABILITY_FILESYSTEM` question (below). **Out of scope here.**
- **Build/test tooling** (Reactotron fs-0001..0006 in `scripts/`; fiddle's 65
  `tests/` + 6 `tools/` fs and 15 `tests/` state_store elements):
  reconstruction never targets build tooling; per the corpus scan-scope
  finding (`docs/disparates/corpus/electron-fiddle.md`,
  [revisions/map-v2-proposal.md](../revisions/map-v2-proposal.md) §1.2) these
  are an upper bound on app-slice demand, to be reclassified
  `OUT_OF_RECONSTRUCTION_SCOPE` once the pending scope mechanism lands.
  **Out of scope here, and not Goya-side demand.**

Note on templates vs app for fiddle's 127: none of them sit in
`static/templates/**` or `src/content/**` — the shipped example-fiddle data
that dominates fiddle's other deficits contributes **zero** state_store and
zero fs elements. The 127 splits as 41 application (`src/`) and 86 tooling
(`tests/`/`tools/`); the genuine app-slice persistence deficit is therefore
19 key-value store + 22 file-access elements in `src/`. Reactotron's genuine
slice is 3 + 3, with its 6 `scripts/` elements tooling throughout.

## Provides (proposed; names reserved, not implemented)

- `crates/goya-persist` (reserved) —
  `pub const PERSIST_CONTRACT: &str = "GY-PERSIST/1"`. No code exists under
  this draft.
- `PersistRoot` — one named scope: a host-application data directory plus a
  stable identity. The entire authority of the contract is expressed in this
  type: one root, named explicitly, no wildcards.
- `PersistCapability` — granted by the host application for **exactly one**
  `PersistRoot`. Every store operation checks its grant; denial is a
  structured error, never a panic or a silent fallback.
- `PersistStore` — load/open the document at a root under a valid grant;
  namespaced key-value operations (below); close. Loading reads the stored
  document bytes, verifies the version tag, runs any required migration
  chain, and adopts the result as the opening committed snapshot of an
  internal `goya_state::Store`.
- `MemoryPersistStore` — memory-only test substitute with injected failure
  modes, mirroring `MemoryClipboard` in GY-CLIPBOARD/1. No grant, no disk
  access; tests never touch the filesystem.
- `PersistError` — structured fallibility for every operation (denied
  grant; unmigratable stored version with both versions named; IO failure;
  bound exceeded, once a bound exists — see open questions). Errors carry
  the store's untouched state; nothing returns partial results.

### API shape

Operations are namespaced key-value reads and writes over the one document,
returning fallible structured results (all names proposed):

- `get(ns, key) -> Result<Option<Value>, PersistError>`
- `set(ns, key, value) -> Result<u64, PersistError>` — returns the new
  revision
- `delete(ns, key) -> Result<u64, PersistError>` — returns the new revision
- `scan(ns) -> Result<Vec<(String, Value)>, PersistError>` — the
  namespace's entries in canonical key order

Reuse discipline — this contract invents no parallel state machinery:

- **Values are `goya_state::Value`** (Int/Text/Bool) and the document is a
  `goya_state::Map` (`HashMap<String, Value>`), imported from
  `crates/goya-state`. No new value type.
- **The revision is GY-STATE/1's revision.** Each `set`/`delete` opens a
  `Store::begin()` transaction, stages the write, and publishes it with
  `Transaction::commit(invariant)`; the returned revision is the store's
  `revision()` after that commit. GY-STATE/1 bumps the revision counter only
  on successful commit, so *every successful write produces exactly one new
  revision* is inherited behaviour, not new machinery. The persisted
  document carries the revision with it, and a reopened store resumes the
  counter from the stored value.
- **Namespaces** are a key-space partition inside the single document (the
  foreign per-concern store instances — menu, config, standalone — each map
  to one namespace). The document itself remains the atomic unit: one swap,
  one version tag, one root. This follows the analysis's semantic essence —
  a single atomically replaced document, not a multi-document store.
- **Bound (open).** Values are bounded — GY-CLIPBOARD/1 size-limits every
  write and this contract should too — but the number, and whether the bound
  is per value, per namespace, or per document, is undecided and recorded
  under open questions. For comparison, GY-STATE/1 sets no size limit in
  code: its discipline is structural (atomic swap, invariant-checked
  commit), not quantitative. GY-PERSIST/1 cannot inherit that silence
  because the disk document needs a worst-case size for atomic-replace
  planning; the bound is a ratification decision, not an implementation
  accident.
- **Value-model fidelity (open).** Observed foreign stores hold nested JSON
  documents; `goya_state::Value` is scalar. Serialising a nested value into
  `Value::Text` is a denormalisation this draft does not decide on; a
  nested-value revision of GY-STATE/1, if ever needed, is that contract's
  revision, not this one's invention.

## Consumes

- **GY-STATE/1** (`crates/goya-state`) — the document and the revision
  model. `Store`, `Transaction`, `Value`, `Map`, `revision()`,
  `CommitRejected` are used as-is. The in-memory committed snapshot is never
  mutated in place; persistence adds exactly one new thing, the disk write
  discipline.
- **GY-HOTSWAP/1** (`crates/goya-hotswap`, `state_migration`) — schema
  migration. What hotswap actually provides, no more: `migrate_state` applies
  explicit, ordered one-version steps (`StateMigrationOp::{Rename, Remove,
  SetDefault}`) to a **cloned in-memory `goya_state::Map`** and returns the
  migrated map only when a chain covering every version from `from` to `to`
  succeeds; gaps, invalid steps, missing rename sources and rename
  collisions return `MigrationError` with no partial state.
  `propose_migrated_swap` runs the chain, publishes via
  `Transaction::stage_snapshot` + `commit`, and activates only after the
  commit returns `Ok`. What hotswap does **not** provide, and this contract
  must supply: it neither reads nor writes disk, discovers no stored version
  tag, and defines no on-disk encoding. The version numbers are
  caller-supplied — under GY-PERSIST/1, the caller is the loaded store,
  handing hotswap the version read from the document and the candidate's
  target version.

## Durability and migration

- **Version-tagged stores.** Every persisted document carries a schema
  version tag alongside the map and the revision. A store opened at a tag
  older than the running application's target version is migrated before
  first use; a store at the target version loads directly; a store at a
  version **newer** than the application understands is a structured load
  error (`VersionUnmigratable`), mirroring `SwapSupervisor::propose_swap`'s
  rejection of an unmigrated higher state-contract version. Downgrade is
  never performed silently.
- **Migration runs the GY-HOTSWAP/1 chain and nothing else.** The chain is
  explicit, ordered, one-version steps supplied by the application; no
  inferred or best-effort migration exists. A failed migration is a
  structured error naming the failing step and the `MigrationError`; the
  store stays at its old version, the previous document on disk is untouched,
  and no silent defaults are substituted. **No silent migrations** is a
  rule, not a preference: an un-migratable store fails loudly or not at all.
- **Durability discipline (design requirement).** Every commit is atomic on
  disk — write-temporary-then-rename or equivalent — and the stored version
  tag and revision advance only on success: the disk analogue of
  GY-STATE/1's `revision()` invariant, which bumps only after the staged
  snapshot passes its invariant. A failed or interrupted write leaves the
  previous document intact and loadable.
- **Honest durability boundary.** No crate exists to prove the discipline.
  What a crash between the in-memory commit and the durable replace leaves
  observable, whether rename-over-replace is atomic on every supported
  platform, and whether the temporary file is fsynced (and its directory
  entry on Linux) before the replace — all of that is **unproven** and
  stated as an open question, not claimed. This draft requires the
  discipline; it does not claim the guarantee.

## Authority model

- **No ambient persistence.** There is no implicit store, no default
  location, no environment-derived path a reconstruction may write to.
  Nothing persists without a `PersistCapability` in hand — the same
  structural rule as GY-CLIPBOARD/1, where every write is gated by a
  capability grant and `Verbatim` vs `DeepContext` are distinct grants.
- **The capability names its root and is granted by the host application.**
  A grant binds one `PersistCapability` to one explicit `PersistRoot`;
  the host application (the operator's delegate) performs the grant, and
  creating a new scope is a distinct, operator-level act, never a
  side-effect of application code wanting one.
- **One root per grant; no traversal.** A grant never covers two scopes,
  never covers a wildcard, and never covers a parent of a scope. Operations
  address only the granted root's document; there is no operation that
  escapes the namespace into the key space of another root, and a root's
  document cannot encode a reference that widens its own authority. Writes
  outside the granted scope fail without partial state — the
  GY-HOTSWAP/1 no-partial-result rule applied to authority.
- **Deny is structured.** An operation without a valid grant for its root
  returns `PersistError::Denied`; it never panics, never falls back to
  memory transparently, and never degrades to a quieter failure. The
  memory substitute exists for tests and is chosen explicitly, not granted
  implicitly.

## Determinism

- **Reads are deterministic given store bytes.** Loading a document is a
  pure function of its bytes: the same bytes yield the same map, version tag
  and revision; `scan` returns entries in canonical key order; nothing in a
  read consults time, locale, environment or RNG — the GY-LAYOUT/1 purity
  rule carried to persistence.
- **Identical operation sequences yield byte-identical documents.** This is
  an invariant the future implementation must demonstrate (canonical
  serialisation, fixed key order), not a claim this draft can make — the
  on-disk format is an open question and byte-identity is defined by it.
- **Write durability boundaries, stated honestly:** guaranteed — a write
  this contract reports as successful has passed its invariant, swapped the
  in-memory snapshot atomically, bumped the revision, and replaced the disk
  document atomically-or-not-at-all, with the previous document intact on
  any reported failure. Not guaranteed (unproven, open): the observable
  state of the store after a crash at an arbitrary point inside the durable
  replace, and cross-platform identity of that crash behaviour. Replay
  determinism under GY-REPLAY/1 scenarios that seed and read persisted
  stores depends on exactly these unproven properties; nothing in that
  contract's trace vocabulary may assume them until they are demonstrated.

## Scope exclusions (declared up front)

- No arbitrary-path file I/O — that is the filesystem-access question, not
  persistence.
- No database/SQLite, no multi-document stores, no network-backed storage.
  Each would need corpus evidence and its own decision; none has any.
- No cache-without-durability mode, no preferences sync, no multi-root
  fan-out. The contract is one document, one root, one grant.

## Relationship to foreign constructs

- **electron-store-style config maps to this contract.** Reactotron
  store-0001..0003 and fiddle's application state_store elements are the
  demand: named key-value documents with defaults, read-modify-write
  lifecycles, restart survival. On ratification these become `representable`
  under the new mapping code (below).
- **Raw fs access does NOT map to this contract.** The `filesystem_operation`
  elements — Reactotron fs-0007..0009 and fiddle's 22 `src/` elements — are
  unstructured path-addressed access, and this contract must not absorb them
  to make a deficit number look better: a scoped key-value document is not
  a general file API, and smuggling file I/O into a persistence grant would
  break the authority model above. Their disposition is the separate
  `NO_GOYA_CAPABILITY_FILESYSTEM` question (spelling as proposed in the
  reason-code register's pending list via
  [revisions/map-v2-proposal.md](../revisions/map-v2-proposal.md) amendment
  B2) — explicitly out of scope here, requiring its own capability analysis.
- **Build/test tooling is neither.** Reactotron's `scripts/` fs elements and
  fiddle's `tests/`/`tools/` populations await the pending
  `OUT_OF_RECONSTRUCTION_SCOPE` mechanism (map-v2-proposal.md §5.3); they
  generate no Goya-side demand and this contract claims none of them.

## Map-revision unlock (on ratification)

Per GY-COVERAGE/1's rules — `representable` is claimed only with named Goya
contract evidence, and the map grows by revision, never silently — nothing
below may emit before this contract is ratified and the revision lands.
Until then `PERSISTENCE_CAPABILITY_UNRESOLVED` remains emitted as-is
(Reactotron 12, fiddle 127), and consumers should read it as the three
sub-decisions above, not one capability. The revision mechanics follow the
established precedent in [revisions/map-v2-proposal.md](../revisions/map-v2-proposal.md):
rules are data (kind + semantics-facet selectors, first match wins), and a
reason code is registered in the same revision that introduces it
(map-v2 §6 amendment B discipline; register governance in
`docs/disparates/reason-codes.md`).

1. **Key-value store → representable.** `state_store` elements gain a rule
   emitting `representable` / **`MAPPED_TO_SCOPED_PERSISTENCE`**
   (register-pending, proposed spelling), with `goya_evidence` rewritten to
   the ratified contract's actual names — the map-v2 §5.2 discipline applied
   to evidence-bearing rules. Under map-v2's pending scope mechanism (§5.3),
   scope exclusion evaluates before kind rules, so the code applies to the
   application slice (Reactotron 3; fiddle 19) while tooling-population
   state_store elements (fiddle `tests/`, 15) fall to
   `OUT_OF_RECONSTRUCTION_SCOPE`.
2. **The split proceeds.** Application `filesystem_operation` elements move
   to `NO_GOYA_CAPABILITY_FILESYSTEM` (new code; codes are added, never
   renamed — `PERSISTENCE_CAPABILITY_UNRESOLVED` stays valid for historical
   documents). Build-tooling populations move to
   `OUT_OF_RECONSTRUCTION_SCOPE` via the pending P3 mechanism (extractor
   scan-scope facet preferred, map-schema `scope_exclusions` interim route),
   which is that proposal's decision, not this one.
3. **Expected effect on the deficit.** Following map-v2 §7's pending
   projections: Reactotron `PERSISTENCE_CAPABILITY_UNRESOLVED` 12 → 0
   (3 mapped, 3 filesystem, 6 out-of-scope); fiddle 127 → 0
   (19 mapped, 22 filesystem, 86 out-of-scope). These are projections of a
   future combined revision, not predictions of any single applyable rule
   today.

## Open questions

1. **On-disk format.** Canonical-JSON house conventions (GY-FAIR/1
   serialisation) are the natural candidate; the encoding of the version
   tag, the revision counter, and namespaces inside the one document is
   undecided. Byte-identity of documents is defined by this decision.
2. **Value bound.** Per-value vs per-namespace vs per-document bound, and
   the number, are a ratification decision (GY-CLIPBOARD/1 size-limits;
   GY-STATE/1 sets no limit in code). The bound-exceeded error variant
   cannot be finalised until this lands.
3. **Concurrent access.** GY-STATE/1 enforces single-writer structurally
   within one process (`begin` returns `None` while a transaction is open).
   Two processes opening the same root is unresolved: no locking discipline,
   no last-writer-wins rule, no detection, is specified. Until decided, the
   honest position is single-process-per-root, unenforced across processes.
4. **Encryption at rest.** Observed foreign stores write plaintext; whether
   Goya requires at-rest encryption, and what key custody would mean for a
   local data directory, is undecided.
5. **Windows/Linux path parity under Sash/Emperor.** Root resolution
   (per-user application-data directories) differs per platform; rename
   atomicity, fsync and directory-entry durability guarantees differ between
   POSIX and Windows; whether the durability guarantee is identical across
   Sash (Windows) and Emperor/Linux hosts, or explicitly platform-scoped,
   is undecided and must be settled before any crash-consistency claim.
6. **Value-model fidelity.** Nested values (foreign stores hold JSON
   objects/arrays) against GY-STATE/1's scalar `Value` — see Provides.

## Invariants

- No disk access occurs without a valid `PersistCapability` for the exact
  root; denial is structured and leaves nothing behind.
- One grant, one root; no wildcard, no parent, no traversal; creating a
  scope is a distinct operator-level act.
- Every successful write produces exactly one new revision (inherited from
  GY-STATE/1's commit discipline); a rejected commit leaves state, revision
  and disk document untouched.
- The stored version tag advances only on a successful, migration-clean
  commit; migrations run GY-HOTSWAP/1 chains and nothing else; a failed
  migration is a structured error and the store stays at its old version.
- A store at a version newer than the application understands fails to load,
  loudly; downgrade never happens silently.
- A crashed write leaves the previous document intact and loadable
  (design requirement; the crash-window proof is an open question).
- Reads are deterministic given store bytes; identical operation sequences
  yield byte-identical documents (requirement pending the format decision).
- No tooling emits `MAPPED_TO_SCOPED_PERSISTENCE` or any split reason code
  before ratification and the map revision land; the refusal is the
  coverage-rule discipline, not a convention.

## Stability

Draft — proposal requiring programme-owner ratification; unimplemented; no
crate exists; coverage rules depending on it are pending and may not emit.
Drafted from [capabilities/persistence-scoping.md](../capabilities/persistence-scoping.md)
(option 2 of 4; the analysis's reservation of the GY-PERSIST name is the
only prior use). Everything above — API names, the namespace model, the
bound, the durability discipline, the mapping code's spelling — is proposal
until ratified. The authority model follows the ratified-register precedent
of GY-CLIPBOARD/1 and the migration model follows the implemented
GY-HOTSWAP/1; both are cited as consumed facts, not re-decided here. The
contract register and the capability map are amended only by their own
revisions, on ratification.
