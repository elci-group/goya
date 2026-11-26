# GY-DESKINT/1 — Desktop OS integration: URI delegation and modal message dialogs (draft)

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19.
Status: Draft — proposal requiring programme-owner ratification; unimplemented; no crate exists; coverage rules depending on it are pending and may not emit.

Drafted from `docs/disparates/capabilities/desktop-os-integration.md` (recommendation
option 2), following the capability model of GY-CLIPBOARD/1 (`crates/goya-clipboard`;
`docs/contracts.md`): authority checked before dispatch, capability objects, no
ambient authority.

## Purpose

GY-DESKINT/1 proposes the narrowest honest desktop OS-integration capability: two
operations — **one-shot URI delegation** (`shell.openExternal` essence) and the
**modal message dialog** (`dialog.showMessageBox` essence) — each gated by an
explicit capability grant, each fallible with structured errors, each producing a
structured diagnostic. The scope is exactly what the corpus grounds; everything
else the reason code `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` suggests
(notifications, tray) is named out of scope below, because no corpus element
exercises it.

This draft ratifies nothing: no crate, no register entry, no map rule, no emitted
coverage code. The reserved mapping codes `MAPPED_TO_URI_DELEGATION` and
`MAPPED_TO_MODAL_DIALOG` stay unemittable until this contract is ratified **and**
the introducing map revision lands (`revisions/map-v2-proposal.md` §5, §8 step 5).

### Corpus evidence (the grounding)

**Reactotron phase 1** — `evidence/disparates/reactotron-phase1/
reference.assessed.fair.json` (345 elements; 12 `os_api_call`, all
`unrepresentable` / `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION`; corpus pin
`acdd7c73e7ecf9d8d79f6fcf9201a48aefaf270a`). Method identities below were
verified against the pinned corpus sources on 2026-11-19; the FAIR records only
the coarse `api` facet.

| Class | Count | Elements and provenance (all verified in pinned sources) |
|---|---|---|
| `api=shell` → `shell.openExternal` | 7 | api-0001 `src/main/menu.ts:154`; api-0004..0007 `src/renderer/pages/help/index.tsx:51,55,59,63`; api-0008 `src/renderer/pages/home/welcome.tsx:25`; api-0011 `src/renderer/pages/timeline/index.tsx:119` |
| `api=dialog` → `dialog.showMessageBox` | 2 | api-0002, api-0003 `src/main/utils.ts:92,112` |
| `api=clipboard` | 3 | api-0009, api-0010 `src/renderer/pages/state/Snapshots.tsx:86,175`; api-0012 `src/renderer/pages/timeline/index.tsx:217` — **out of scope**: served by GY-CLIPBOARD/1; map revision 2's applyable rule reclassifies them (`MAPPED_TO_CLIPBOARD_CAPABILITY`) |

Notification elements: 0. Tray elements: 0. The reason code over-describes this
deficit; 9 of the 12 blocked elements ground exactly the two operations this
contract provides.

**electron-fiddle phase 2** — `evidence/disparates/electron-fiddle-phase2/
extraction.assessed.fair.json` (401 elements; 66 `os_api_call`; pin `82fb8290…`,
tag v0.4.0). Populations split by provenance prefix, with the app-owned (`src/**`)
method identities verified against the pinned sources on 2026-11-19:

| Class | src (app-owned) | template (`static/templates/**`) | tests (`tests/**`) | total |
|---|---|---|---|---|
| `api=shell` | 10 — 7 `shell.openExternal` (api-0005..0007 `src/main/menu.ts:58,64,70`; api-0008, api-0009 `src/main/windows.ts:55,60`; api-0011 `src/renderer/components/dialog-token.tsx:108`; api-0014 `src/renderer/components/settings-credits.tsx:45`) + 3 `shell.showItemInFolder` (api-0010 `commands-runner.tsx:264`; api-0015, api-0016 `settings-general-appearance.tsx:129,150`) — the `showItemInFolder` three are **not** URI delegation | 6 (api-0023..0025 `Shell/renderer.js:4,8,12`; api-0033..0035 `shell/renderer.js:4,8,12`) | 7 (api-0051..0053 `menu-spec.ts:102,107,112`; api-0063 `dialog-token-spec.tsx:116`; api-0064 `settings-credits-spec.tsx:65`; api-0065, api-0066 `settings-general-appearance-spec.tsx:58,83`) | 23 |
| `api=dialog` | 5 — 2 `dialog.showMessageBox` (api-0001 `src/main/dialogs.ts:23`; api-0004 `src/main/files.ts:91`) + 2 `dialog.showOpenDialog` (api-0002, api-0003 `files.ts:21,39`) + api-0017 (`src/renderer/file-manager.ts:87`, whose recorded line is a comment; the flow is the save/open-dialog path) — the file-chooser three are **not** modal message dialogs | 2 (api-0021 `Dialog/main.js:18`; api-0030 `dialog/main.js:18`) | 17 (api-0037 `dialogs-spec.ts:22`; api-0038..0050 `files-spec.ts`; api-0054..0056 `menu-spec.ts:209,219,229`) | 24 |
| `api=clipboard` | 2 | 6 | 6 | 14 — **out of scope** (GY-CLIPBOARD/1) |
| `api=notification` | 0 | 2 (api-0022, api-0032) | 0 | 2 — **out of scope** |
| `api=tray` | 0 | 3 (api-0026, api-0031, api-0036) | 0 | 3 — **out of scope** |

Grounded app-owned, in-contract-class demand across both corpora: **14 verified
`shell.openExternal` sites** (7 + 7) and **4 verified `dialog.showMessageBox`
sites** (2 + 2). Template and tests populations carry only the coarse `api`
facet (extractor 0.1.0 is a line-oriented scanner, ADR 0001); they are template
data and tooling, not app-owned demand. The method-level mixes discovered inside
fiddle's `src` populations are recorded here because they constrain the pending
map rules (see *Map-revision unlock*, granularity hazard).

No desktop OS-integration contract and no crate providing one exists in
`crates/` or the register (checked 2026-11-19: `goya-clipboard`,
`goya-window-management`, `goya-wayland`; nothing for shell open, dialogs,
notifications, or tray).

## Provides (proposed; names reserved, not implemented)

- `crates/goya-deskint` — `pub const DESKINT_CONTRACT: &str = "GY-DESKINT/1"`.
  No code exists under this draft; the name is reserved, as GY-REPLAY/1's is in
  the roadmap crate family.
- `DeskintCapability` — the permission classes, mirroring `ClipboardCapability`:

```rust
/// Permission classes for desktop OS integration. Distinct grants: holding
/// one implies nothing about the other. No blanket capability exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeskintCapability {
    /// May hand a URI to the operating system's default handler (one-shot).
    UriDelegation,
    /// May surface a modal message dialog and await the user's choice.
    ModalDialog,
}

/// Policy decision: which capabilities a principal holds.
/// Mirrors GY-CLIPBOARD/1's `CapabilityPolicy`; `DenyAll` is the safe default.
pub trait DeskintPolicy {
    fn permits(&self, principal: PrincipalId, capability: DeskintCapability) -> bool;
}

/// Identity of the requesting actor (same shape as GY-CLIPBOARD/1's
/// `PrincipalId`; cross-contract unification, if ever wanted, is a
/// shared-types revision, not this contract's concern).
pub struct PrincipalId(pub u64);
```

- Request and result types — every operation takes an explicit request carrying
  its principal; there is no ambient entry point:

```rust
/// A one-shot URI delegation request.
pub struct OpenExternalRequest<'a> {
    pub uri: &'a str,
    pub principal: PrincipalId,
}

/// Marker that the URI was accepted **for delegation**. Not an outcome:
/// this contract surfaces nothing about the URI's fate after hand-off.
pub struct DelegationReceipt;

/// One button of a modal message dialog.
pub struct DialogButton {
    /// Stable identifier; the only thing a `DialogChoice` may return.
    pub id: String,
    /// App-authored display text.
    pub label: String,
    pub is_default: bool,
}

/// A modal message dialog request. All text is app-authored content.
pub struct MessageDialogRequest {
    pub title: String,
    pub message: String,
    /// Finite, non-empty, ordered button set.
    pub buttons: Vec<DialogButton>,
    pub principal: PrincipalId,
}

/// The user's discrete selection: an environment input event.
pub struct DialogChoice {
    /// Identifies a button by stable id — never an index, never free text.
    pub selected_button_id: String,
}
```

- Structured errors — every operation is fallible; a denial is a value, never a
  fallback:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeskintError {
    /// Principal lacks the capability class for this operation.
    CapabilityDenied,
    /// URI scheme is not on the allow-list (default deny).
    UriSchemeRejected { scheme: String },
    /// URI could not be parsed at all.
    UriMalformed,
    /// Dialog request invalid: empty message, no buttons, oversize text.
    DialogInvalid { reason: DialogInvalidReason },
    /// No mediation route on this host. Structured; never a fallback to a
    /// direct, unmediated mechanism.
    HostUnavailable { platform: HostPlatform },
}
```

- Structured diagnostics — exactly one per operation attempt, granted or
  denied, redactable under GY-PROVENANCE/1 conventions:

```rust
pub struct DeskintDiagnostic {
    pub operation: DeskintOperation,        // OpenExternal | MessageDialog
    pub principal: PrincipalId,
    pub capability_consulted: DeskintCapability,
    pub outcome: DeskintOutcome,            // Accepted | Chosen { id } | Denied | Rejected { .. } | Unavailable { .. }
    pub trace_id: TraceId,                  // GY-PROVENANCE/1
    pub redactions: Vec<RedactionRecord>,   // GY-PROVENANCE/1
}
```

- `DeskintProvider` — the platform abstraction. The two methods below are the
  contract's entire dispatch surface:

```rust
pub trait DeskintProvider {
    /// One-shot URI delegation. `Ok` means accepted for delegation —
    /// nothing more (see Rules).
    fn open_external(&self, request: OpenExternalRequest)
        -> Result<DelegationReceipt, DeskintError>;

    /// Modal message dialog. Blocks the calling thread until the user
    /// chooses (see Rules for the exact blocking semantics).
    fn message_dialog(&self, request: MessageDialogRequest)
        -> Result<DialogChoice, DeskintError>;

    /// The structured diagnostic of the most recent operation attempt.
    fn last_diagnostic(&self) -> Option<DeskintDiagnostic>;
}
```

- `MemoryDeskint<'a>` — capability-gated, in-memory provider for tests and
  verification (mirroring `MemoryClipboard`): consults the policy before
  recording any invocation, records every attempt's diagnostic, and takes
  scripted dialog choices so the contract is deterministically testable. It
  performs no host interaction whatsoever.

## Consumes

- GY-PROVENANCE/1 — `TraceId`, `RedactionRecord` and the redaction conventions
  for diagnostics; the same dependency GY-CLIPBOARD/1 takes.
- Nothing else. No filesystem, no network, no process, no window-host types, no
  foreign framework runtime. Platform mediation (portals, dialog toolkits) is a
  backend concern behind `DeskintProvider`, landing via adjacent platform work
  exactly as GY-CLIPBOARD/1's data-device backend lands with GY-WAYLAND-001;
  it is a dependency of implementations, never of this contract revision.

## Rules

### Authority model (both operations)

- **Authority is checked before dispatch.** Every call carries a
  `PrincipalId`; the provider consults the `DeskintPolicy` for the operation's
  capability class before any validation, host contact or side effect. A
  failed check returns `DeskintError::CapabilityDenied` and a denial
  diagnostic; nothing else happens.
- **No ambient authority.** The trait above is the complete dispatch surface:
  there is no free function, global, or default path that opens a URI or shows
  a dialog without a request object and a policy check.
- **No blanket grant.** `UriDelegation` and `ModalDialog` are independent.
  A principal holding both holds two grants, not one.

### URI delegation (`open_external`)

- **One-shot, fire-and-forget.** The provider hands the URI to the operating
  system's default handler and takes no further part in its fate. `Ok
  (DelegationReceipt)` means *accepted for delegation*; it is not, and must
  never be presented as, evidence that the URI was opened, rendered, or
  handled. No outcome, callback, or status is surfaced — that is the semantic
  essence the corpus grounds, not an implementation shortcut.
- **No local execution.** The provider never executes, spawns, resolves or
  follows the URI itself, and never retries a rejected delegation through an
  alternate mechanism. Delegation to the OS default handler is the only effect.
- **Scheme allow-list, default deny.** The URI must parse and its scheme must
  appear on the configured allow-list (http, https and mailto are the minimum
  the capability analysis names). A scheme outside the list is
  `UriSchemeRejected { scheme }` — a structured rejection, never a silent
  no-op, never a fallback to another handler. Where the allow-list lives and
  who may extend it is deliberately open (see Open questions); whatever the
  answer, unknown schemes deny by default.
- URIs may embed credentials or tokens in their query fragments; payloads are
  subject to the same redaction discipline as any other diagnostic payload
  before a diagnostic is recorded.

### Modal message dialog (`message_dialog`)

- **Blocking semantics, stated exactly.** The call blocks the calling thread
  until the user selects a button. The synchronous result is the user's
  discrete selection, returned as `DialogChoice { selected_button_id }` —
  a stable button identifier, never an index alone, never free text. There is
  no timeout, no default answer on absence, and no programmatic dismissal in
  v1: an environment where the user cannot answer is a host concern, and a
  host that cannot present the dialog returns `HostUnavailable`, it does not
  invent a choice.
- **The choice is an environment input event.** It depends on the user, like
  a key press or pointer event. It is classified with every other interaction
  input — it is not a nondeterminism leak into rendering logic, and it is
  never cached: dialog results are inputs, never state. Re-asking re-blocks;
  a recorded choice is never re-emitted.
- **Dialog text is app-authored content.** Title, message and button labels
  are authored by the application, subject to host layout and accessibility
  constraints: the host may reflow or truncate within declared bounds
  (oversize text is `DialogInvalid`, refused before any host contact), must
  present the full button set in order, and must never interpret dialog text
  as instructions — the provider assigns no markup, command or code semantics
  to it. The provider renders what the app authored or refuses; it never
  composes content of its own.
- Invalid requests (empty message, empty button set, oversize text, duplicate
  button ids) are `DialogInvalid { reason }` — structured, pre-flight, and
  diagnosable like any other denial.

### Determinism and diagnostics (both operations)

- **Exactly one structured diagnostic per attempt**, granted or denied:
  operation, principal, capability consulted, outcome, trace id, redactions.
  Diagnostics are canonical and redactable under GY-PROVENANCE/1 rules; the
  diagnostic stream is the contract's audit surface — a denied attempt is as
  visible as a granted one.
- **A denied capability is a structured denial, never a fallback.** Denial
  never degrades into a different operation, a warning-only path, a silent
  success, or a direct unmediated mechanism. `HostUnavailable` is the same
  discipline applied to the host: no mediation route is a recorded outcome,
  not an excuse to bypass mediation.
- **`MemoryDeskint` makes the contract deterministically testable:** fixed
  policy, scripted choices, recorded invocations, byte-stable diagnostics.
  Identical requests against identical policy and scripts yield identical
  provider answers and identical diagnostics. Real backends may add
  platform-tagged fields to diagnostics; they may never change a decision.

## Platform note (unqualified)

Everything in this section is **unqualified**: it anticipates routes and cites
what exists; it asserts no capability that exists and no platform behaviour.

- **Linux/Wayland first.** The anticipated mediation route for URI delegation
  and dialogs is xdg-desktop-portal, for portability across compositor and
  sandbox contexts. What actually exists today: Emperor's Linux desktop
  contract (GY-EMP-LINUX/1, `docs/emperor/contracts/gy-emp-linux-1.md`) names
  portals in its full surface — "URI opening … and portals" — and explicitly
  blocks them for its cycle: "Portals are not started. … Nothing in this
  contract is a qualified desktop integration"; "XDG Desktop Portals, including
  file chooser, URI open, and notification portals" are listed Blocked. No
  portal-binding code exists anywhere in the programme's crates.
- **Windows via the Sash parity programme.** Windows backend work, if this
  contract ever ratifies a backend, is contingent on the parity programme
  (`docs/parity/programme.md`), whose second milestone is recorded Unmet with
  "Windows execution is BLOCKED". No Windows desktop-integration surface is
  specified or implemented.
- This contract names no dialog toolkit, no portal API version, and no
  platform claim beyond the citations above. A platform note in a future
  ratified revision is where qualified claims would first be allowed.

## Map-revision unlock (what ratification enables)

On ratification, the pending rules recorded in
`docs/disparates/revisions/map-v2-proposal.md` §5.2 (rules P1/P2) become
emittable, per that document's §4/§8:

- `os_api_call` with semantics `{"api": "shell"}` → `representable`,
  reason `MAPPED_TO_URI_DELEGATION`, evidence GY-DESKINT/1
  `DeskintProvider::open_external` (the `goya_evidence` string must be
  rewritten from its PENDING-RATIFICATION placeholder to the ratified
  contract's actual names before application).
- `os_api_call` with semantics `{"api": "dialog"}` → `representable`,
  reason `MAPPED_TO_MODAL_DIALOG`, evidence GY-DESKINT/1
  `DeskintProvider::message_dialog` (same rewrite requirement).
- Both rules insert **between** the clipboard rule and the coarse desktop-OS
  rule (first match wins; `api=clipboard` must never reach them). Until they
  are applied, the codes may not be emitted by any crate — they are reserved
  in `docs/disparates/reason-codes.md` ("Pending proposals"), and the
  introducing revision must register them in the same revision that first
  emits them (register governance).
- **The coarse code remains valid for historical documents.**
  `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` is never renamed; it stays the
  classification of already-assessed documents, and after application remains
  the catch-all for the classes this contract excludes — notifications and
  tray — with its evidence/detail narrowed to them. Absence of demand for
  those classes is recorded by not creating the capability, not by a new code.
- **Granularity hazard, recorded honestly (decision required at application).**
  The pending rules select on the coarse `api` facet; this draft's
  corpus-level verification (2026-11-19, pinned sources) shows the facet is
  coarser than the contract's operation classes. In fiddle `src/**`, the rules
  as drafted would emit `MAPPED_TO_URI_DELEGATION` for 3 `shell.showItemInFolder`
  elements and `MAPPED_TO_MODAL_DIALOG` for 2 `dialog.showOpenDialog` elements
  plus 1 comment-line attribution (api-0017) — six elements outside this
  contract's two classes — and would classify all 47 fiddle shell/dialog
  elements including template and test copies (the accepted-interim precedent
  of map-v2 §3.2). The applying revision must choose, and record: accept the
  module-level approximation as documented interim behaviour, or defer
  per-class emission until AST-grade extraction (ADR 0001) records a
  method-level facet that rules can select on. This contract supports neither
  choice; it only requires the choice be explicit. Map-v2 §7's projection
  ("fiddle src `shell` 10 + `dialog` 5") is refined accordingly: of those 15
  app-owned elements, 9 are in-contract-class (7 `openExternal`, 2
  `showMessageBox`); Reactotron's 9 are all in-contract-class.

## Out of scope

Each exclusion is a candidate for a later revision only with corpus evidence.
Named so the contract's silence is explicit:

| Excluded | Reason |
|---|---|
| Notifications | Async attention requests, materially larger semantics (delivery, actions, persistence). Zero Reactotron elements; fiddle's 2 are template data (`static/templates/**`), not app-owned demand. |
| Tray (status items) | Persistent OS chrome with an event source — menus, icons, event loops. Zero Reactotron elements; fiddle's 3 are template-only. |
| System tray icons | The visual half of the tray surface; excluded with it, for the same absent-demand reason. |
| Auto-update | Update feeds and installers are a security-sensitive lifecycle surface no corpus element exercises; named Blocked in the Emperor threat model's unverified-integration list. Nothing here may grow into it by accident. |
| Global shortcuts | An input-grabbing surface with its own theft/conflict discipline (cf. GY-CHR-001 §6's rule that shortcuts are never stolen from embedded terminals). No observed demand. |
| File open/save dialogs (`dialog.showOpenDialog`/`showSaveDialog`) | Observed in fiddle `src/**` (files.ts:21,39; the flow api-0017 attributes) but they are file-chooser semantics — user-selected filesystem access — which requires the persistence-scoping decision first (`capabilities/persistence-scoping.md`, `PERSISTENCE_CAPABILITY_UNRESOLVED`). This contract's `ModalDialog` is message-only; no file-dialog method may hide behind it. |
| Default-application configuration | Setting handlers is configuration authority, not delegation. No observed demand. |

## Open questions (honest)

1. **Portal availability on target platforms.** Is an xdg-desktop-portal
   implementation present and running on every environment a Goya application
   must serve? Under Flatpak it is mandatory; under a bare Wayland session it
   may be absent; Emperor blocks portals entirely this cycle. The drafted
   answer — a missing route is `HostUnavailable`, structured and recorded,
   never a direct-execution fallback — needs a decision on what, if anything,
   a non-portal Linux backend may be.
2. **Dialog toolkit dependency (and sync-vs-async shape).** Which toolkit
   renders the modal on each platform (portal Dialog API, native toolkit,
   something in-reach of the pure-Rust stance ADR 0001 applied to the
   extractor)? And does the blocking signature drafted above survive that
   choice, or must v1 provide an async equivalent (the capability analysis
   explicitly left `message_dialog -> DialogChoice` "or an async equivalent
   decided at drafting time")? Blocking-forever-on-a-thread has runtime
   integration costs this draft does not resolve.
3. **Allow-list source.** Who authors the URI scheme allow-list — the contract
   (fixed minimum http/https/mailto), a per-application manifest, or a
   `DeskintPolicy` implementor? Who may extend it — contract revision, data,
   or policy code — and is policy ever finer than scheme (host, port, path)?
   This draft's recommendation: scheme-only, extension by contract revision,
   default deny; the recommendation is not a decision.
4. **Method-level facets for the pending rules.** Whether the applying map
   revision accepts the coarse `api` facet approximation (with the six
   out-of-class fiddle `src` elements it would mis-map, recorded above) or
   waits for AST-grade extraction (ADR 0001) to emit a method facet the rules
   can select on. Recorded as the granularity hazard above; owned by the
   programme owner at application time.

## Invariants

- No ambient authority: the provider trait is the complete dispatch surface;
  both methods take a principal-bearing request and consult policy before any
  effect. There is no blanket capability; `UriDelegation` and `ModalDialog`
  are independent grants.
- `open_external` performs no local execution of the URI, resolves no handler,
  and validates the scheme against a default-deny allow-list. `Ok` means
  accepted for delegation and nothing more.
- `message_dialog` blocks until a real user choice exists; the result is an
  environment input event, returned once, never cached and never re-emitted;
  dialog text is app-authored content, bounded, rendered or refused, never
  interpreted.
- Every denial and every rejection is structured (`DeskintError`), recorded in
  a structured, redactable diagnostic, and terminal for that attempt: no
  fallback operation, no silent success, no unmediated retry.
- Exactly one structured diagnostic per operation attempt; diagnostics are
  canonical and redact payloads under GY-PROVENANCE/1 rules.
- `MemoryDeskint` yields identical answers and byte-stable diagnostics for
  identical requests, policy and scripts.
- No `verified`, `translated` or any representability claim is producible by
  this contract; it is a capability provider, not an assessment authority.
- Nothing in this draft amends the contract register, the capability map, or
  any evidence artefact; the reserved codes remain unemittable until
  ratification and map application, in that order.

## Stability

Draft. Everything in this document — the capability enumeration, the trait
surface, the error and diagnostic schemas, the platform note, the map-unlock
conditions — is proposal until ratified. The permission-class enumeration is
expected to freeze at v1 with exactly two variants; scope grows narrower
never broader without corpus evidence, and each out-of-scope row above may be
revisited only by a revision that cites demand. Ratification requires, in
order: programme-owner ratification of this draft; a register entry in
`docs/contracts.md`; a `goya-deskint` crate implementing the authority model
with `MemoryDeskint` and its tests; and only then the map revision applying
rules P1/P2 with evidence strings rewritten to the ratified names. The status
line at the head of this document is the authoritative statement of what
exists until all four steps are done.
