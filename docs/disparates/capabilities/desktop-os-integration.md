# Desktop OS integration — capability decision analysis

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19.
Status: Proposal (decision required). Not a ratified contract; nothing here
amends the contract register or the capability map.

## Purpose

Decide whether the `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` deficit
(12 elements, Reactotron phase 1, `evidence/disparates/reactotron-phase1/
reference.deficit.json`) demands a new Goya capability contract, and of what
shape. Analysis only; no code changes.

## Observed foreign usage (evidence)

12 of 345 elements are blocked under this reason code. Element-level
breakdown from `reference.assessed.fair.json` (ids and provenance cited
verbatim):

| API class | Count | Elements |
|---|---|---|
| `shell.openExternal` | 7 | api-0001 (`src/main/menu.ts:154`), api-0004..0007 (`src/renderer/pages/help/index.tsx:51,55,59,63`), api-0008 (`src/renderer/pages/home/welcome.tsx:25`), api-0011 (`src/renderer/pages/timeline/index.tsx:119`) |
| `dialog.showMessageBox` | 2 | api-0002, api-0003 (`src/main/utils.ts:92,112`) |
| `clipboard.writeText` | 3 | api-0009, api-0010 (`src/renderer/pages/state/Snapshots.tsx:86,175`), api-0012 (`src/renderer/pages/timeline/index.tsx:217`) |

**Zero notification elements and zero tray elements are observed** in the
corpus. The reason code names four API classes; the evidence supports two and
a half. The code over-describes the deficit.

**Three of the twelve elements (clipboard) are mis-classified.**
GY-CLIPBOARD/1 exists (`crates/goya-clipboard`; contract register,
checked 2026-11-19) and provides capability-gated clipboard write. These
elements should be `representable` under GY-CLIPBOARD/1 evidence, not blocked
under a desktop-integration code. This is a capability-map revision, not a
contract change.

No desktop OS-integration contract, and no crate providing one, exists in
`crates/` or the register (checked 2026-11-19: crate listing contains
`goya-clipboard`, `goya-window-management`, `goya-wayland`; nothing for
notifications, dialogs, shell open, or tray).

## Semantic essence

- **`shell.openExternal`** — hand a URI to the operating system's default
  handler and take no further part in its fate. Fire-and-forget; the
  application never sees an outcome. Essence: *one-shot URI delegation*.
- **`dialog.showMessageBox`** — a modal system dialog with a message, a
  button set, and a blocking wait for the user's choice. Essence:
  *synchronous user decision surfaced through the host OS, returning a
  discrete selection*. Determinism note: the result depends on the user;
  the contract must classify it as an environment input event, like any
  other interaction, not as a nondeterminism leak into rendering logic.
- **`clipboard.writeText`** — already covered by GY-CLIPBOARD/1 (Verbatim
  grant). No new capability is required.
- **Notifications, tray** — not observed. Their semantic essences
  (async attention request; persistent OS chrome with an event source) are
  materially larger and are not grounded by this corpus.

## Options

1. **New GY-DESKINT contract** covering notifications + dialogs + shell +
   tray as one surface — matches the reason code, exceeds the evidence.
2. **Narrow GY-DESKINT/1 draft** covering only URI delegation and modal
   message dialogs, leaving notifications and tray explicitly out of scope
   pending observed demand.
3. **Defer** — hold as `unrepresentable` until a corpus nominates an
   application that needs more.
4. **Declare permanently unrepresentable** — rejected on its face: URI
   delegation is a trivial OS call with a narrow, testable surface.

## Recommendation

Option 2, with a concurrent map revision for clipboard.

Rationale:

- 9 of 12 blocked elements are grounded demand for exactly two operations
  (7 URI opens, 2 message dialogs — table above, with provenance).
  3 of 12 are already served by GY-CLIPBOARD/1 and are mis-coded.
- The operations are small, independently testable, and carry no rendering
  or determinism risk; they belong in the same family as GY-CLIPBOARD/1:
  gated system access behind a provider trait with an in-memory test
  substitute.
- Deferral (option 3) blocks nothing today — Reactotron is a representability
  corpus, not a shipping target — but the whole point of deficit discovery
  is to convert observed blockers into contracts while the evidence is
  fresh. The evidence here is specific and bounded; deferring records less
  information than acting on it.
- A broad contract (option 1) would specify notification and tray semantics
  no corpus element exercises. That is speculative demand, which the
  programme explicitly forgoes (roadmap, "Capability deficit discovery").

## What a GY-DESKINT/1 draft must specify

- **Provides:** `DeskintProvider` trait; `open_external(&str) -> Result<(),
  OpenRejected>`; `message_dialog(MessageDialog) -> DialogChoice` (or an
  async equivalent decided at drafting time); `MemoryDeskint` test provider
  recording invocations.
- **Authority model:** an explicit `DeskintCapability` grant, per operation
  class, mirroring `ClipboardCapability`/`CapabilityPolicy` in
  GY-CLIPBOARD/1. URI open is a distinct grant from dialog display. No
  blanket grant.
- **Scope exclusions declared up front:** no notifications, no tray, no
  file-open/save dialogs, no default-application configuration. Each is a
  candidate for a later revision only with corpus evidence.
- **Invariants:** `open_external` performs no local execution of the URI and
  validates scheme against an allow-list (http/https/mailto minimum);
  `MemoryDeskint` makes the contract deterministically testable; dialog
  results are inputs, never cached state.
- **Consumes:** nothing beyond core types; platform backends land via
  GY-WAYLAND-001 adjacent work, as GY-CLIPBOARD/1's data-device backend does.
- **Stability:** Candidate from birth; the register entry follows the draft,
  not this document.

## Map revisions required (decision required, not applied)

- Reclassify api-0009, api-0010, api-0012 to `representable` with
  `GY-CLIPBOARD/1` evidence (new mapping code, e.g.
  `MAPPED_TO_CLIPBOARD_CAPABILITY`; reason codes grow by revision and are
  never renamed).
- On ratification of GY-DESKINT/1: `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION`
  remains valid for historical documents; new assessments emit per-class
  codes so the deficit report stops over-reporting notification/tray demand.
- No change for notifications/tray: absence of demand is recorded by not
  creating the capability, not by a code.
