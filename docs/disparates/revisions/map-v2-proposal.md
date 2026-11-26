# Capability-map revision 2 — apply-ready proposal (GY-COVERAGE/1)

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19.
Status: **Proposal — pending programme-owner application.** This document is
DATA, not applied code: nothing in it amends the contract register, the
`goya-disparate-coverage` crate, or any evidence artefact. A programme owner
applies the applyable subset (§4) as described in §8; the pending subset (§5)
must **not** be applied until its stated precondition is met.

Consolidates three evidence-backed findings into one before/after rule-set:

1. **Clipboard mis-classification** (Reactotron; 3 elements) — fixed now.
2. **Out-of-reconstruction-scope populations** (fiddle; 194 elements) —
   mechanism specified; applyable only after a schema amendment or an
   extractor revision.
3. **Coarse desktop-OS-integration split** (per-class codes) — shell/dialog
   rules drafted but pending GY-DESKINT/1 ratification; only the clipboard
   rule is applyable now because GY-CLIPBOARD/1 is real
   (`crates/goya-clipboard`, register checked 2026-11-19).

Facet keys in every rule below were verified against the actual FAIR JSONs
(2026-11-19): the os_api_call semantics key is **`api`**, with string values
`shell` | `dialog` | `clipboard` | `notification` | `tray` (Reactotron:
7/2/3/0/0; fiddle: 23/24/14/2/3). No other semantics facet distinguishes
template or tooling files — the complete semantics key sets are Reactotron
`{api, prop, hook, channel, side, verb, mechanism, selector, component}` and
fiddle `{api, prop, channel, side, verb, resolution, component}`; neither
corpus records a path or scan-scope facet.

## 1. Evidence

### 1.1 Finding 1 — clipboard elements are mis-classified

`evidence/disparates/reactotron-phase1/reference.deficit.json`: 345 elements;
`NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` blocks 12 `os_api_call`. The raw
`reference.fair.json` records all three clipboard elements with semantics
exactly `{"api": "clipboard"}`:

| id | provenance | semantics |
|---|---|---|
| api-0009 | `src/renderer/pages/state/Snapshots.tsx:86` | `{"api": "clipboard"}` |
| api-0010 | `src/renderer/pages/state/Snapshots.tsx:175` | `{"api": "clipboard"}` |
| api-0012 | `src/renderer/pages/timeline/index.tsx:217` | `{"api": "clipboard"}` |

GY-CLIPBOARD/1 exists (`crates/goya-clipboard`; `CLIPBOARD_CONTRACT =
"GY-CLIPBOARD/1"`; register checked 2026-11-19) and provides capability-gated
clipboard write — `ClipboardCapability::Verbatim` / `DeepContext` behind a
`CapabilityPolicy`, verbatim and Deep Copy modes, writes gated by capability
checks and output-size limits. Per
`docs/disparates/capabilities/desktop-os-integration.md` ("Three of the
twelve elements (clipboard) are mis-classified"), these belong under
GY-CLIPBOARD/1 evidence, not a desktop-integration absence code. This is a
capability-map revision, not a contract change.

The same `api=clipboard` facet exists in fiddle: 14 os_api_call elements
(2 app `src/renderer/components/dialog-token.tsx:112,119`; 6 template
`static/templates/Clipboard/**` + `static/templates/clipboard/**`; 6 test
`tests/renderer/components/dialog-token-spec.tsx:42,45,53,56,64,67`). The
rule proposed in §4 is kind+semantics scoped and therefore classifies all 14;
that is honest interim behaviour — see the scope caveat in §3.2.

### 1.2 Finding 2 — out-of-reconstruction-scope populations

`evidence/disparates/electron-fiddle-phase2/` (401 elements, pin
`82fb8290…`, tag v0.4.0). Grouping blocked-kind elements by provenance path
prefix (verified against `extraction.fair.json`):

| Path prefix | fs | os_api | state_store | child_process | window_lifecycle | ipc_channel | dynamic require | total |
|---|---|---|---|---|---|---|---|---|
| `static/templates/**` | 0 | 19 | 0 | 0 | 42 | 8 | 0 | 69 |
| `src/content/**` | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 2 |
| `tests/**` | 65 | 30 | 15 | 2 | 0 | 0 | 4 | 116 |
| `tools/**` | 6 | 0 | 0 | 1 | 0 | 0 | 0 | 7 |
| **total** | **71** | **49** | **15** | **3** | **44** | **8** | **4** | **194** |

194 of 401 elements (48%) are shipped example-fiddle data
(`static/templates/**`, `src/content/**`) or build/test tooling
(`tests/**`, `tools/**`) — application data and build tooling, never
migration targets. Note a corpus difference from the phase-1 framing: fiddle
has no `scripts/**` population (its tooling lives in `tests/**` and
`tools/**`); Reactotron's build tooling is `scripts/**` (fs 6 in
`scripts/build.release.js` + `scripts/release.artifacts.js`, child_process 2),
8 elements under the same principle.

`docs/disparates/corpus/electron-fiddle.md` ("Scan-scope finding recorded for
the programme") and the run.json honesty notes quantify and declare this: the
extractor has no scan-scope mechanism, so the counts are an upper bound on
app-slice semantics, preserved rather than silently dropped per the
FAIR-preserve-ignorance rule. A future mechanism should be
manifest-declared data directories, "rather than ad-hoc path exclusions".
The assessed FAIR carries file provenance for every element, so the
population is mechanically identifiable; what is missing is a place to
declare the exclusion.

### 1.3 Finding 3 — coarse desktop-OS-integration code over-describes the deficit

`docs/disparates/capabilities/desktop-os-integration.md` recommends (option
2) a narrow GY-DESKINT/1 (URI delegation + modal message dialogs only) plus a
concurrent clipboard map revision, with per-class reason codes on
ratification: 9 of 12 Reactotron blocked elements are grounded demand for
exactly two operations (7 `shell.openExternal`, 2 `dialog.showMessageBox`);
3 of 12 are already served by GY-CLIPBOARD/1. Zero notification and zero tray
elements exist in the Reactotron corpus. Fiddle adds os_api_call classes
`notification` (2) and `tray` (3) — both entirely inside
`static/templates/**`, i.e. template data, not app-owned demand.

Status: GY-DESKINT/1 does not exist (no crate, no register entry; checked
2026-11-19). Therefore the shell/dialog split rules are recorded here as
pending; only the clipboard rule is applyable now.

## 2. Map-revision mechanics (why the shape below)

- `Rule` selectors match on optional element `kind` plus a **subset of
  semantics facets** (`CapabilityMap::match_rule`); first match wins; only
  `extracted` elements are assessed; every outcome requires a register-form
  reason code and non-empty `goya_evidence` (`validate`).
- The map `version` must equal `MAP_VERSION` ("1") under current code. The
  applyable revision in §4 is **format-version 1** data: it loads today via
  `CapabilityMap::from_json` / `load_map` with zero code changes
  ("map revision 2" = second revision of the map, not format version 2).
- Provenance `file` is **not** a semantics facet, so no version-1 rule can
  select on path; §5.3/§6 specify what Finding 2 requires.

## 3. The before/after rule-set

### 3.1 Applyable now — full v2 `rules` array (format version 1)

Exactly the `CapabilityMap::rules` order after application: the ten v1
`default_map()` rules verbatim, with the clipboard rule inserted immediately
before the coarse desktop-OS rule (first match wins, so `api=clipboard`
elements match rule 10, everything else os_api_call falls through to rule
11). No existing rule is altered, moved, or removed.

```json
{
  "version": "1",
  "rules": [
    {
      "kind": "component",
      "semantics": {},
      "status": "representable",
      "reason": "MAPPED_TO_SCENE_DECLARATION",
      "goya_evidence": "GY-LANG/1 scene syntax; GY-IR/1 nodes",
      "detail": "component structure maps to scene node declarations"
    },
    {
      "kind": "event_handler",
      "semantics": {},
      "status": "representable",
      "reason": "MAPPED_TO_ACTION_DECLARATION",
      "goya_evidence": "GY-IR/1 actions; GY-RUNTIME/1 event dispatch",
      "detail": "interaction handlers map to declared actions"
    },
    {
      "kind": "ipc_channel",
      "semantics": {},
      "status": "representable",
      "reason": "MAPPED_TO_ACTION_DISPATCH",
      "goya_evidence": "GY-IR/1 actions; GY-RUNTIME/1 dispatch",
      "detail": "renderer/main process messaging dissolves in Goya's single-process model; the channel's requests and responses become actions; this is architectural simplification, not 1:1 transport mapping"
    },
    {
      "kind": "hook",
      "semantics": {},
      "status": "undetermined",
      "reason": "HOOK_DATAFLOW_NOT_EXTRACTED",
      "goya_evidence": "GY-STATE/1 covers value state; side-effect scheduling has no assessed equivalent",
      "detail": "the extractor records hook occurrence sites without data-flow; useState maps to state transactions, effects do not map mechanically"
    },
    {
      "kind": "style_rule",
      "semantics": {},
      "status": "representable",
      "reason": "MAPPED_TO_SCENE_STYLING",
      "goya_evidence": "GY-CHROMA/1 theming; GY-LAYOUT/1 deterministic layout",
      "detail": "static visual declarations map to scene styling and layout constraints"
    },
    {
      "kind": "window_lifecycle",
      "semantics": {},
      "status": "representable",
      "reason": "MAPPED_TO_NATIVE_WINDOW_HOST",
      "goya_evidence": "GY-WAYLAND/1; goya-window-management; Sash (Windows) and Emperor parity programme",
      "detail": "application window lifecycle maps to the native windowing host"
    },
    {
      "kind": "filesystem_operation",
      "semantics": {},
      "status": "undetermined",
      "reason": "PERSISTENCE_CAPABILITY_UNRESOLVED",
      "goya_evidence": "no filesystem-access contract exists in the register (checked 2026-11-19)",
      "detail": "Goya has no declared user-filesystem capability; mapping requires a scoping and authority decision first"
    },
    {
      "kind": "child_process",
      "semantics": {},
      "status": "unrepresentable",
      "reason": "NO_GOYA_CAPABILITY_CHILD_PROCESS",
      "goya_evidence": "no process-spawn capability exists in the register (checked 2026-11-19)",
      "detail": "Goya provides no child-process model; foreign spawn/exec logic is blocked"
    },
    {
      "kind": "state_store",
      "semantics": {},
      "status": "undetermined",
      "reason": "PERSISTENCE_CAPABILITY_UNRESOLVED",
      "goya_evidence": "GY-STATE/1 (in-memory transactions) and GY-HOTSWAP/1 (persisted-state migration) do not declare a disk-persistence contract",
      "detail": "electron-store style key-value persistence has no declared equivalent"
    },
    {
      "kind": "os_api_call",
      "semantics": { "api": "clipboard" },
      "status": "representable",
      "reason": "MAPPED_TO_CLIPBOARD_CAPABILITY",
      "goya_evidence": "GY-CLIPBOARD/1 (crates/goya-clipboard): capability-gated clipboard write, Verbatim and Deep Copy modes behind ClipboardCapability/CapabilityPolicy",
      "detail": "clipboard write calls map to GY-CLIPBOARD/1 capability-gated clipboard write; removes api=clipboard elements from the desktop-OS-integration deficit"
    },
    {
      "kind": "os_api_call",
      "semantics": {},
      "status": "unrepresentable",
      "reason": "NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION",
      "goya_evidence": "no desktop OS-integration contract (notifications, dialogs, shell, tray) exists in the register (checked 2026-11-19)",
      "detail": "desktop OS integration is absent; each specific API is a candidate capability deficit"
    }
  ]
}
```

Delta from v1: exactly one inserted rule (position 10 of 11). The coarse
rule's `goya_evidence`/`detail` are kept byte-identical so the change is a
pure insertion; narrowing its wording is part of the pending GY-DESKINT/1
revision (§5.2), not this one.

### 3.2 Scope caveat on the clipboard rule (accepted interim behaviour)

The rule is kind+semantics scoped; it cannot be path-scoped under format
version 1. Applied to fiddle it reclassifies 14 `api=clipboard` elements,
12 of which live in template/test files (§1.1). This is accepted: the
elements remain extracted facts, the classification ("this construct is
representable under GY-CLIPBOARD/1") is true of them, and the population
correction belongs to the scope mechanism (§5.3), not to a clipboard-specific
carve-out. No attempt is made to subtract template counts post hoc, per the
run.json honesty notes (unauditable).

## 4. Applyable-now vs pending — summary

| Rule | Status | Precondition |
|---|---|---|
| os_api_call `api=clipboard` → `MAPPED_TO_CLIPBOARD_CAPABILITY` | **Applyable now** | none — GY-CLIPBOARD/1 exists (§4) |
| os_api_call `api=shell` → `MAPPED_TO_URI_DELEGATION` | Pending | GY-DESKINT/1 ratified (§5.2) |
| os_api_call `api=dialog` → `MAPPED_TO_MODAL_DIALOG` | Pending | GY-DESKINT/1 ratified (§5.2) |
| Scope exclusion → `OUT_OF_RECONSTRUCTION_SCOPE` | Pending | coverage-1.md amendment A (schema) or extractor scan-scope revision (§5.3, §6) |

**Only the clipboard rule may be applied now.** The pending rules are
recorded so the ratification steps are mechanical, not exploratory.

## 5. Pending rules (recorded, NOT to be applied)

### 5.1 Why they are pending

- `representable` is claimed only with named Goya contract evidence
  (coverage-1.md). No GY-DESKINT/1 contract or crate exists (checked
  2026-11-19); a shell/dialog `representable` rule today would be an
  evidence-free mapping — exactly what the contract forbids.
- `OUT_OF_RECONSTRUCTION_SCOPE` cannot be emitted by any version-1 rule:
  selectors cannot see `provenance.file`. Emitting it requires either the
  map-schema amendment (§6, amendment A) or an extractor scan-scope revision
  that records a semantics facet (§5.3).

### 5.2 Pending rules P1/P2 — desktop-OS split, on GY-DESKINT/1 ratification

To be inserted **between** the clipboard rule and the coarse desktop-OS rule
(clipboard stays first so `api=clipboard` never reaches them). Drafted as
they would appear; `goya_evidence` must be rewritten to the ratified
contract's actual names before application:

```json
[
  {
    "kind": "os_api_call",
    "semantics": { "api": "shell" },
    "status": "representable",
    "reason": "MAPPED_TO_URI_DELEGATION",
    "goya_evidence": "GY-DESKINT/1 DeskintProvider::open_external (PENDING RATIFICATION — no such contract exists, checked 2026-11-19)",
    "detail": "one-shot URI delegation to the OS default handler maps to GY-DESKINT/1 scheme-allow-listed open_external; fire-and-forget, no outcome surfaced to the application"
  },
  {
    "kind": "os_api_call",
    "semantics": { "api": "dialog" },
    "status": "representable",
    "reason": "MAPPED_TO_MODAL_DIALOG",
    "goya_evidence": "GY-DESKINT/1 DeskintProvider::message_dialog (PENDING RATIFICATION — no such contract exists, checked 2026-11-19)",
    "detail": "modal message dialog with blocking user choice maps to GY-DESKINT/1 message_dialog; the user's choice is an environment input event, not cached state"
  }
]
```

On ratification the coarse rule (§3.1 rule 11) remains as the catch-all for
`notification` / `tray` (the classes the narrow contract excludes; absence of
demand is recorded by not creating the capability, not by a code), with its
`goya_evidence`/`detail` narrowed to notifications/tray. The reason code
`NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` is never renamed — it stays valid
for historical documents; new assessments emit per-class codes.

### 5.3 Pending mechanism P3 — out-of-reconstruction scope

Long-term ownership is settled by the corpus document: **the extractor owns
scan-scope** (manifest-declared data directories), not the map. Map revision
2 therefore does not smuggle path matching into rule selectors. Two routes to
an emittable `OUT_OF_RECONSTRUCTION_SCOPE`, in preference order:

**(a) Extractor-facet route — no map-schema change.** An extractor revision
records a semantics facet on scanned elements, e.g.
`"scan_scope": "template-data"` / `"scan_scope": "build-tooling"`
(vocabulary is the extractor revision's to define). Because the facet is an
ordinary semantics value, a version-1-compatible rule then implements the
disposition with zero schema change:

```json
{
  "kind": null,
  "semantics": { "scan_scope": "PENDING-EXTRACTOR-REVISION" },
  "status": "unrepresentable",
  "reason": "OUT_OF_RECONSTRUCTION_SCOPE",
  "goya_evidence": "corpus scope declaration: manifest-declared data directories (extractor revision), per docs/disparates/corpus/electron-fiddle.md",
  "detail": "construct extracted from a path outside the declared reconstruction scope (shipped application data / build-test tooling); asserts nothing about Goya capability demand"
}
```

This rule, when its facet exists, takes **position 1** (before all kind
rules) so scope wins over every later disposition. Facet matching keeps the
map corpus-agnostic — the corpus-specific fact (which directories are data)
lives in the FAIR manifest, where it belongs.

**(b) Interim map-schema route — `scope_exclusions` field.** If scope
correction is needed before the extractor revision, the declaration lives at
map level:

```json
{
  "version": "2",
  "scope_exclusions": ["static/templates/", "src/content/", "tests/", "tools/"],
  "rules": [ "... §3.1 rules ..." ]
}
```

(Fiddle values shown; a Reactotron instance would declare
`["scripts/"]`. Paths are relative to the assessed document's
`source.root`.) Semantics, evaluation order, and version handling are fixed
by the coverage-1.md amendment A in §6. **Hazard:** under current serde,
unknown fields are silently dropped — a `scope_exclusions`-bearing map
marked `version: "1"` would load today and *silently ignore* the exclusions.
The version bump to `"2"` is what makes pre-amendment code fail loudly
(`validate` rejects unknown versions) instead. Do not ship the field under
version `"1"`.

**(c) What remains possible without any schema change:** registering the
code and its category now (amendment B in §6 is documentation-only), so that
reports carrying `OUT_OF_RECONSTRUCTION_SCOPE` — from the extractor-facet
route — are interpretable the day they appear. What is **not** possible
without change: emitting the code for the already-extracted fiddle and
Reactotron corpora via map data alone, and applying any scope correction to
the 4 extractor-`undetermined` dynamic-require elements in `tests/**` —
assessment only upgrades `extracted` elements, so those keep their extractor
classification until the extractor itself handles scope.

## 6. Schema-amendment text required (coverage-1.md)

Three amendments. B is documentation-only and may land immediately; A and C
accompany the `scope_exclusions` schema change and must land as code + text
together (the map is data, but the field is schema).

### Amendment A — map format v2: `scope_exclusions`

In **Provides**, replace the `CapabilityMap` bullet with:

> - `CapabilityMap` — versioned rule set (`version` + ordered `rules`, first
>   match wins, plus `scope_exclusions` in format version 2), serialisable,
>   loadable from JSON (`from_json` / `load_map`), `validate`.

In **Rules**, insert after the "Only `extracted` elements are assessed"
bullet:

> - Map format version 2 adds a map-level `scope_exclusions` field: an
>   ordered list of path-prefix strings relative to the assessed document's
>   `source.root`. An element is **scope-excluded** when its
>   `provenance.file` equals an entry or begins with an entry followed by
>   `/` (path-component boundary; `static/templates/` does not match
>   `static/templates-extra/…`). Scope exclusion is evaluated **before** the
>   `rules` list; a scope-excluded element is assessed `unrepresentable`
>   with reason `OUT_OF_RECONSTRUCTION_SCOPE`, `goya_evidence` naming the
>   matched exclusion entry, and detail naming the matched prefix and file
>   path. Scope exclusions apply only to elements whose status is
>   `extracted`; existing classifications are preserved as with rules.
> - A `version: "2"` map must carry `scope_exclusions` (possibly empty);
>   `validate` continues to accept `version: "1"` maps, treated as empty
>   exclusions. Implementations must reject all other versions: loading a
>   version-2 map on pre-amendment code must fail loudly, never silently
>   ignore the field.

### Amendment B — reason-code register (documentation-only, may land now)

Register governance (`docs/disparates/reason-codes.md`): a code is registered
**in the same revision that introduces it**; a code emitted by any crate
before it appears in the register is a defect in that revision. Since map
revision 2 introduces `MAPPED_TO_CLIPBOARD_CAPABILITY`, its application
**must** include this amendment.

**B1 — coverage-1.md.** In **Rules**, extend the representable mapping-code
list:

> (`MAPPED_TO_SCENE_DECLARATION`, `MAPPED_TO_ACTION_DECLARATION`,
> `MAPPED_TO_ACTION_DISPATCH`, `MAPPED_TO_SCENE_STYLING`,
> `MAPPED_TO_NATIVE_WINDOW_HOST`, `MAPPED_TO_CLIPBOARD_CAPABILITY`;
> `MAPPED_TO_URI_DELEGATION` and `MAPPED_TO_MODAL_DIALOG` are reserved,
> pending GY-DESKINT/1 ratification and may not be emitted until then.)

And add a third blocking category after the existing two:

> - **Corpus-scope exclusions (`OUT_OF_RECONSTRUCTION_SCOPE`)** — constructs
>   extracted from paths declared outside the reconstruction scope (shipped
>   application data, build tooling, test tooling). They assert nothing
>   about Goya capability demand and nothing about programme debt;
>   consumers of the deficit report must exclude this category from
>   capability prioritisation and debt accounting.

**B2 — reason-codes.md.** Three edits, in the same revision:

Move `MAPPED_TO_CLIPBOARD_CAPABILITY` from "Pending proposals" into the
Register table:

> | `MAPPED_TO_CLIPBOARD_CAPABILITY` | `mapping` | Clipboard write calls map to GY-CLIPBOARD/1 capability-gated clipboard write (Verbatim / Deep Copy behind `ClipboardCapability`/`CapabilityPolicy`). | Map revision 2 (`revisions/map-v2-proposal.md`) | refreshed phase-1/phase-2 deficit runs after application (predicted: reactotron 3, fiddle 14 os_api_call) |

Update the `capabilities/desktop-os-integration.md` pending bullet to:

> - `capabilities/desktop-os-integration.md` (Status: Proposal; drafted as
>   pending rules in `revisions/map-v2-proposal.md` §5.2):
>   `MAPPED_TO_URI_DELEGATION`, `MAPPED_TO_MODAL_DIALOG` — registered only
>   when GY-DESKINT/1 ratifies and the introducing revision lands.

Update the `capabilities/persistence-scoping.md` pending bullet to cite the
interim disposition spec:

> - `capabilities/persistence-scoping.md` (Status: Proposal):
>   `NO_GOYA_CAPABILITY_FILESYSTEM`, `OUT_OF_RECONSTRUCTION_SCOPE` (proposed
>   spelling), plus the split of `PERSISTENCE_CAPABILITY_UNRESOLVED`.
>   `OUT_OF_RECONSTRUCTION_SCOPE`'s emission mechanics and evaluation order
>   are specified in `revisions/map-v2-proposal.md` §5.3; its introducing
>   revision must also add a category (`corpus-scope-exclusion`) to the
>   Categories table — none of the five current categories fits it.

### Amendment C — grounding table (with the code change)

In **V1 capability-map findings**, split the last row:

> | Clipboard write (`api=clipboard`) | Representable | GY-CLIPBOARD/1 (map revision 2) |
> | Shell open / message dialog | Unrepresentable until GY-DESKINT/1 | per-class codes reserved (`MAPPED_TO_URI_DELEGATION`, `MAPPED_TO_MODAL_DIALOG`) |
> | Notifications, tray | Unrepresentable | no capability, no observed app-owned demand |

## 7. Expected impact (applying ONLY the applyable rule)

Predictions are reasoned over the assessed FAIR counts in §1 (all affected
elements carry `api=clipboard` and status `extracted` in the raw FAIR
documents, so `assess` upgrades each exactly once; no other rule matches
them first). No code or evidence was modified to compute them.

**Operational note:** the prediction assumes re-running `disparate-assess`
against the **raw** extraction FAIRs (`reference.fair.json`,
`extraction.fair.json`). Re-running against the already-assessed documents
is a no-op for these elements: assessment preserves existing
classifications.

### Reactotron (345 elements, unchanged total)

| Metric | v1 (measured) | v2 predicted | delta |
|---|---|---|---|
| representable | 249 | **252** | +3 |
| undetermined | 75 | 75 | — |
| unrepresentable | 21 | **18** | −3 |
| `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` | 12 | **9** (shell 7, dialog 2) | −3 |
| `HOOK_DATAFLOW_NOT_EXTRACTED` | 63 | 63 | — |
| `PERSISTENCE_CAPABILITY_UNRESOLVED` | 12 | 12 | — |
| `NO_GOYA_CAPABILITY_CHILD_PROCESS` | 9 | 9 | — |

Reclassified: api-0009, api-0010, api-0012 → `representable`
(`MAPPED_TO_CLIPBOARD_CAPABILITY`, GY-CLIPBOARD/1 evidence). Predicted
deficit ordering (count desc, ties keep first-encounter order):
**HOOK 63 → PERSISTENCE 12 → DESKINT 9 → CHILD 9** (DESKINT before CHILD by
first-encounter order: api-0001 precedes proc-0001). The DESKINT files list
shrinks by one entry — `src/renderer/pages/state/Snapshots.tsx` drops out
(both its DESKINT elements were clipboard); `timeline/index.tsx` stays
(api-0011, `shell.openExternal` at line 119, remains under the coarse rule).

### electron-fiddle (401 elements, unchanged total)

| Metric | v1 (measured) | v2 predicted | delta |
|---|---|---|---|
| representable | 169 | **183** | +14 |
| undetermined | 156 | 156 | — |
| unrepresentable | 76 | **62** | −14 |
| `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` | 66 | **52** (dialog 24, shell 23, notification 2, tray 3) | −14 |
| `PERSISTENCE_CAPABILITY_UNRESOLVED` | 127 | 127 | — |
| `DYNAMIC_REQUIRE_UNRESOLVED` | 29 | 29 | — |
| `NO_GOYA_CAPABILITY_CHILD_PROCESS` | 10 | 10 | — |

Reclassified: the 14 `api=clipboard` os_api_call elements (§1.1), including
12 in template/test files — accepted interim behaviour (§3.2). Predicted
deficit ordering: PERSISTENCE 127 → DESKINT 52 → DYNAMIC_REQUIRE 29 → CHILD
10. The DESKINT files list shrinks by four entries —
`static/templates/Clipboard/main.js`, `static/templates/Clipboard/renderer.js`,
`static/templates/clipboard/main.js`, `static/templates/clipboard/renderer.js`
drop out (every os_api_call in them is clipboard); every other DESKINT file
retains at least one non-clipboard element (e.g.
`tests/renderer/components/dialog-token-spec.tsx` keeps one `shell`).

### Pending-rule projections (NOT predictions of this revision; context only)

Recorded so the value of the pending items is quantified; not achievable by
applying map revision 2:

- **GY-DESKINT/1 ratified (P1/P2 applied):** the app-slice OS-integration
  deficit goes to **zero in both corpora** — Reactotron shell 7 + dialog 2
  and fiddle src `shell` 10 + `dialog` 5 become representable; the remaining
  coarse-rule population is fiddle notification 2 + tray 3, all in
  `static/templates/**` (template data, and zero Reactotron elements).
- **Scope mechanism landed (P3):** 190 of fiddle's 194 out-of-scope elements
  (§1.2) become `OUT_OF_RECONSTRUCTION_SCOPE` (the other 4 are
  extractor-`undetermined`, outside the map's reach, §5.3(c)); Reactotron's 8
  `scripts/**` elements likewise. Fiddle DESKINT falls 52 → 3 (only the two
  template `notification` + three `tray` os_api_call, if the coarse rule
  still precedes the scope rule; with the scope rule first, to 0 — order is
  an application decision recorded at that revision), PERSISTENCE
  127 → 41 (fs src 22 + state_store src 19), CHILD_PROCESS 10 → 7,
  DYNAMIC_REQUIRE 29 → 25 (map-reachable population).

## 8. Application checklist (for the programme owner)

1. Apply §3.1: insert the single clipboard rule into `default_map()` (or
   load the §3.1 map via `load_map`) — format version stays `"1"`.
2. Re-run `disparate-assess` over the raw corpora FAIRs; refresh
   `reference.deficit.json` / `extraction.deficit.json` and comparison.md
   from the new runs; expect the §7 numbers.
3. Add a regression test alongside the existing
   `semantics_facet_matching_applies_first_match_wins` test: an
   `os_api_call` with `{"api":"clipboard"}` classifies
   `MAPPED_TO_CLIPBOARD_CAPABILITY`, one with `{"api":"shell"}` still falls
   to the coarse rule.
4. Land amendment B (§6) in the same revision as step 1 — B1 in
   coverage-1.md, and B2 in `docs/disparates/reason-codes.md` (register
   governance: the code and its introducing revision land together; the
   refreshed evidence runs from step 2 are the first-evidence citation).
5. Do **not** apply §5 pending rules until their preconditions: GY-DESKINT/1
   ratified (P1/P2, with `goya_evidence` rewritten to the ratified names) or
   amendment A + schema code landed / extractor scan-scope revision shipped
   (P3). A v2-shaped map must never be loaded under version `"1"`.
