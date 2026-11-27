# Programme verification #1 — `window_lifecycle` → representable / `MAPPED_TO_NATIVE_WINDOW_HOST`

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19.
Status: Audit (2026-11-19). This document audits a capability-map claim
against the workspace's actual APIs and recorded evidence. It verifies
nothing beyond what is cited, admits no release, and does not amend the
contract register, the capability map, or any evidence artefact. GY-COVERAGE/1
invariant applies unchanged: "No `verified` status is producible under this
revision; representability is not equivalence"
(`docs/disparates/contracts/coverage-1.md`).

Format note: this is programme verification #1; later verifications should
reuse this header shape (Directive / Date / Status: Audit / non-admission),
the claim-then-evidence-then-verdict structure, and the limitation section.

## 1. The claim

The rule, verbatim from `default_map()` (map v1 + applied revision 2; the
window rule is untouched by revision 2), `crates/goya-disparate-coverage/src/lib.rs:163-169`:

```rust
rule(
    Some(ElementKind::WindowLifecycle),
    RepresentabilityStatus::Representable,
    Some("MAPPED_TO_NATIVE_WINDOW_HOST"),
    "GY-WAYLAND/1; goya-window-management; Sash (Windows) and Emperor parity programme",
    "application window lifecycle maps to the native windowing host",
),
```

The rule selector is kind-only: `semantics: {}` matches every
`window_lifecycle` element (`CapabilityMap::match_rule`,
`crates/goya-disparate-coverage/src/lib.rs:101-108`). The grounding table row
is "Window lifecycle | Representable | GY-WAYLAND/1; goya-window-management;
Sash/Emperor parity" (`docs/disparates/contracts/coverage-1.md`).

### Elements the rule was applied to

| Corpus | window_lifecycle elements | App-slice elements | Source |
|---|---|---|---|
| Reactotron phase 1 | 1 (`win-0001`, `src/main/index.ts:30`) | 1 | `evidence/disparates/reactotron-phase1/reference.assessed.fair.json` |
| electron-fiddle phase 2 | 45 (`win-0001`..`win-0045`) | 1 (`win-0003`, `src/main/windows.ts:37`) | `evidence/disparates/electron-fiddle-phase2/extraction.assessed.fair.json` |

**Template contamination caveat (fiddle).** 44 of the 45 fiddle elements are
not app code: 42 in `static/templates/**` (`win-0004`..`win-0045`) and 2 in
`src/content/**` (`win-0001`, `win-0002`). These are Electron API example
fiddles shipped as application data, "displayed and run inside the fiddle
sandbox, never executed as Electron Fiddle's own process code"
(`docs/disparates/corpus/electron-fiddle.md`, "Scan-scope finding recorded for
the programme"). The extractor "has no exclusion mechanism, so template
constructs are extracted alongside app constructs"; the run's honesty notes
call the counts "an upper bound on app-slice semantics"
(`evidence/disparates/electron-fiddle-phase2/run.json`). Only `win-0003`
(`src/main/windows.ts:37`) is the app's single `new BrowserWindow(` site.

**Extractor scope caveat (both corpora).** The extractor records a
`window_lifecycle` element **only** at `new BrowserWindow(` sites, with
**empty semantics** — no operation facet of any kind
(`crates/goya-disparate-electron/src/lib.rs:369-378`; verified: all 46
elements carry `semantics == {}` in the raw FAIRs). The corpus therefore
contains evidence about window **creation** sites only; no show/close/resize/
minimize/maximize/focus/title construct from any foreign app was extracted.
The claim's per-operation coverage cannot be grounded in corpus data and is
audited below directly against the named Goya crates.

## 2. Per-operation evidence

### 2.1 GY-WAYLAND/1 — `crates/goya-wayland` (Linux/Wayland)

Public surface relevant to window lifecycle (all citations
`crates/goya-wayland/src/lib.rs`):

| Operation | API found today | Citation | Note |
|---|---|---|---|
| create | `GoyaWindow::create(title: &str, width: i32, height: i32)` — configured xdg-shell toplevel | `lib.rs:1288` (`toplevel.set_title` at `lib.rs:1334`) | Title is settable only here |
| show | None as a separate operation; first show is a side effect of `create`'s initial toplevel commit | no `show`/`hide` method exists (method list `lib.rs:1286-2141`) | Electron deferred-show / hide / show-inactive semantics have no equivalent |
| close (app→window) | `request_close()` → `xdg_toplevel::destroy` | `lib.rs:2084-2091` | |
| close (window→app) | `close_requested()`, `PlatformEvent::CloseRequested` | `lib.rs:2048-2050`, `lib.rs:518` | |
| resize (compositor→app) | `PlatformEvent::Resized(i32, i32)` | `lib.rs:515-516` | |
| resize (app→compositor) | **none**; `size()` is a read-only accessor, "fixed in this minimal scope" | `lib.rs:2137-2140` | Gap |
| minimize | `request_minimize()` → `set_minimized`, "when supported" | `lib.rs:2073-2081` | Compositor-decided; unqualified (see Emperor) |
| maximize / restore | `request_maximize()` / `request_restore()` → `set_maximized` / `unset_maximized` | `lib.rs:2052-2071` | Unqualified (see Emperor) |
| focus | **none** — no activation request, no focus event in `PlatformEvent` (`lib.rs:455-519`); internal keyboard-focus handling only | grep over `lib.rs`: no `set_focus`/xdg-activation; `lib.rs:1262-1272` resets compose state on focus loss | Gap |
| title (post-create) | **none** — no `set_title` after `create` | `lib.rs:1334` is the only `set_title` call site | Gap |
| second toplevel | Unassessed: one `GoyaWindow` is one toplevel; a second toplevel is recorded as "not wired for EMP-M1", not as a proven API absence | `docs/emperor/GY-EMP-001.md` (§4 host list); `docs/emperor/gap-ledger.md` EMP-M1-18 | Gap (multi-window apps) |

Declined-behaviour contract: `WaylandError::NoDisplay` graceful degradation,
the application "remains operational headless"
(`docs/contracts.md`, GY-WAYLAND/1 section).

Live precedents: GY-WAYLAND-001 "✓ live Wayland surface + runtime pointer
adapter" (`docs/contracts.md:79`); the exemplar applications' `--live` mode
"opens a window with dirty-redraw presentation" and "live Wayland windows
[are] verified (dirty-redraw)" for all four apps
(`docs/contracts.md:771,780`); Emperor's gap ledger records one measured
launch: "One debug ELF ... opened a Wayland window and presented one frame.
The smoke did not press the button."
(`docs/emperor/gap-ledger.md`, EMP-M1-01). No precedent exercised
minimize/maximize/focus/title/programmatic-resize paths.

### 2.2 goya-window-management

Complete public API (`crates/goya-window-management/src/lib.rs`, 192 lines):

- `WindowAction { Close, Maximize, Restore, Minimize, NewTab }`
  (`lib.rs:5-11`) — **no Create, Show, Resize, Focus, or SetTitle variant.**
- `WindowCapability { Close, Maximize, Minimize, NewTab }` (`lib.rs:14-19`).
- `trait WindowManager { capabilities(); request(action) }` (`lib.rs:27-30`).
- `LinuxWindowManager::detect()` matches Hyprland/Sway/i3/X11 environments and
  dispatches **external CLI tools** (`hyprctl`, `swaymsg`, `i3-msg`, `wmctrl`)
  against the **active** window, not an owned window handle
  (`lib.rs:50-78`, `lib.rs:102-146`). `Minimize` and `NewTab` are unsupported
  on every Linux adapter (`lib.rs:107,116,125,144`).
- `HostWindowManager::new(capabilities, dispatch)` — generic adapter for
  foreign window owners (`lib.rs:159-174`). No consumer exists elsewhere in
  the workspace (grep over `crates/`, 2026-11-19).
- Non-Linux backend is `UnsupportedWindowManager` with empty capabilities;
  every action returns `Unsupported`
  (`crates/goya-wayland/src/window_management/unsupported.rs:8-16`).

Contribution to the claim: close/maximize/restore/minimize request plumbing
only. It provides nothing for create, show, resize, focus, or title, and on
Windows it provides nothing at all.

### 2.3 Sash (Windows arm of the evidence string)

Sash's generated-code surface (static read of
`crates/sash/src/ir_codegen.rs`): `CreateWindowExW` (`:529,:660`),
`ShowWindow(hwnd, SW_SHOWNORMAL)` (`:550,:677`), `WM_DESTROY` /
`PostQuitMessage` / `DestroyWindow` (`:418-419,:722,:1036`), `WM_SIZE` and
`WM_DPICHANGED` handlers, `SetWindowPos` declared (`:419-420,:555,:706,:710`).
Absent from codegen: minimize/maximize requests, `SetFocus`/activation,
post-create title (`SetWindowTextW`), hide/show after initial show.

The programme's own honest lines on this arm:

- Phase 2 status: "Unmet. **Windows execution is BLOCKED.**"
  (`docs/parity/programme.md`, Phases table).
- "Sash admits Enter, Tab, and the copy/paste policy in generated code.
  **That image has not been launched.**" (`docs/parity/programme.md`, Equivalence
  table, Platform-native row).
- "`linux_executable` is headless: `parity-counter` runs the admitted add and
  does not open a Wayland window. `goya-wayland` remains a live host for other
  native shells and is not bound to this scene." (`docs/parity/programme.md`,
  What the gate holds).
- "Sash's cross-linked x86, x64, and ARM64 images remain **unqualified until a
  Windows launch**." (`docs/parity/programme.md`, closing paragraph).
- Emperor side agrees: "Windows launch of the Sash image is not a Linux result
  and has not been observed." (`docs/emperor/gap-ledger.md`, EMP-M1-02,
  Blocked); "Windows execution stays BLOCKED." (`docs/emperor/README.md`).

### 2.4 Emperor's role

Emperor is the Goya-to-Linux compiler; it consumes `goya-wayland` as the
presentation host rather than providing a second window API
(`docs/emperor/README.md`, Binding decisions; `docs/emperor/GY-EMP-001.md`,
stage 8 "Link `emperor-runtime` and the Wayland host"). Its honest lines bound
what the "Emperor parity programme" arm of the evidence string may claim:

- "Implemented for the single toplevel, pointer, keyboard, clipboard policy,
  resize without reflow, and present-on-demand wiring ... **A Wayland launch
  is not certified. EMP-M1 is not certified.**" (`docs/emperor/GY-EMP-001.md`,
  Repository status).
- "Emperor SHALL NOT assume it can place or focus a window when the compositor
  refuses. EMP-M1 does not call a position request."
  (`docs/emperor/GY-EMP-001.md`).
- "The following host capabilities are required by the full window contract
  and are **not wired for EMP-M1**: destruction beyond the host close path,
  maximise, fullscreen, a second toplevel, popups, menus, dialogs,
  drag-and-drop, qualified IME, fractional scale, output hotplug, and **a
  tested focus-activation protocol. They stay Unmet.**"
  (`docs/emperor/GY-EMP-001.md`).
- "No second top-level window. EMP-M1 opens one toplevel. Popups, menus,
  dialogs, fullscreen, and **maximise are not qualified**."
  (`docs/emperor/gap-ledger.md`, EMP-M1-18).

Emperor's role in the evidence string is therefore a *consumer* whose records
confirm the host's create/close/resize-event path on one compositor, once,
and explicitly disqualify the remaining operations.

## 3. Verdict per platform

Representability asserts Goya can express the semantics. Every lifecycle
operation with no found API is an explicit gap.

### Linux / Wayland — **partially evidenced**

| Operation | Verdict | Basis |
|---|---|---|
| create | Evidenced | `GoyaWindow::create` (`lib.rs:1288`) + three live precedents (§2.1) |
| close | Evidenced (close path only) | `request_close` / `close_requested` / `CloseRequested` (`lib.rs:2048,2084,518`); Emperor: "destruction beyond the host close path" not wired |
| resize (event) | Evidenced (event only) | `PlatformEvent::Resized` (`lib.rs:516`) |
| minimize / maximize / restore | Partially | APIs exist (`lib.rs:2054,2064,2074`) but compositor-decided and unqualified — "maximise ... not qualified" (EMP-M1-18); never exercised by any precedent |
| show / hide | **Gap** | no API (§2.1) |
| resize (programmatic set) | **Gap** | no API; `size()` read-only ("fixed in this minimal scope") |
| focus / activation | **Gap** | no API, no event; "a tested focus-activation protocol. They stay Unmet" (GY-EMP-001.md) |
| title (post-create) | **Gap** | no API after `create` |
| multi-window | **Gap** | unwired and unqualified, not exercised (GY-EMP-001.md host list, EMP-M1-18) |

### Windows — **not evidenced**

No Sash image has ever been launched ("Windows execution is BLOCKED",
`docs/parity/programme.md`). The audited window surface is static generated
code covering 4 of the 8 operations (create, show-once, destroy,
WM_SIZE/WM_DPICHANGED event); minimize, maximize, focus, title-change, and
hide are absent from codegen (§2.3). The evidence string's "Sash (Windows)"
arm names a programme with zero executable window evidence; `goya-window-management`'s
non-Linux backend is `UnsupportedWindowManager` (§2.2).

### As applied to the corpus elements — the mis-classification risk is latent, not realised

The only extracted window_lifecycle construct is a creation site, and creation
is the one operation that is evidenced on Linux. The 46 classified elements
are not individually wrong. The defect is in the **rule**, not yet in the
data: a kind-only rule with empty semantics will also classify any future
extractor revision that records show/focus/resize/title constructs, for which
Goya has no found API on any platform.

## 4. Consequences — proposed narrowed rule (DRAFT ONLY, NOT APPLIED)

Governance: per GY-COVERAGE/1, "the map must grow by contract revision, never
silently", and "assessment policy changes are map revisions, not code
changes" (`docs/disparates/contracts/coverage-1.md`). The mechanics —
format-version-1 data, first-match-wins facet insertion, the register rule
that a reason code lands in the same revision that introduces it, and the
requirement that pending rules stay unapplied until their precondition — are
those of `docs/disparates/revisions/map-v2-proposal.md` (§2, §3, §5, §6
amendment B). This section is DATA for a future revision; nothing here has
been applied to the crate, the register, or the evidence.

### Option A — interim evidence-string narrowing (map data only, loads today)

Replace the window rule's `goya_evidence`/`detail` at the next map revision;
status unchanged (creation sites remain representable):

```json
{
  "kind": "window_lifecycle",
  "semantics": {},
  "status": "representable",
  "reason": "MAPPED_TO_NATIVE_WINDOW_HOST",
  "goya_evidence": "GY-WAYLAND/1 GoyaWindow::create/request_close/close_requested + PlatformEvent::{Resized,CloseRequested} (single Wayland toplevel; live per GY-WAYLAND-001 and Emperor EMP-M1-01); Sash codegen CreateWindowExW/ShowWindow/WM_DESTROY (never launched, Windows execution BLOCKED). No found host API: show/hide, programmatic resize, focus activation, post-create title (docs/disparates/verifications/window-host-audit.md, checked 2026-11-19)",
  "detail": "the extracted construct (window creation) maps to the native windowing host; lifecycle operations beyond creation are unassessed by the current extractor and carry the gap list of programme verification #1"
}
```

Note: under map-revision discipline this is an *amendment* of an existing
rule's evidence wording, not an insertion; the map-v2 precedent for such a
change is the pending narrowing of the coarse desktop-OS rule
(`docs/disparates/revisions/map-v2-proposal.md` §5.2, kept out of revision 2
for the same reason it belongs here: wording changes ride with a revision).

### Option B — per-operation facets at the next extractor revision (preferred)

The extractor currently records creation sites only
(`crates/goya-disparate-electron/src/lib.rs:369-378`). An extractor revision
(every corpus honesty note already anticipates one: "a full-parser extractor
revision would be a new extractor version") records an operation facet on
`window_lifecycle` elements, e.g. `semantics {"op": "create" | "show" |
"close" | "resize" | "minimize" | "maximize" | "focus" | "title"}`. The map
then scopes by facet, format version 1 unchanged:

```json
{
  "kind": "window_lifecycle",
  "semantics": { "op": "create" },
  "status": "representable",
  "reason": "MAPPED_TO_NATIVE_WINDOW_HOST",
  "goya_evidence": "<Option A evidence string>",
  "detail": "window creation maps to the native windowing host"
},
{
  "kind": "window_lifecycle",
  "semantics": { "op": "close" },
  "status": "representable",
  "reason": "MAPPED_TO_NATIVE_WINDOW_HOST",
  "goya_evidence": "<Option A evidence string>",
  "detail": "the close path (request and CloseRequested) maps to the native windowing host"
},
{
  "kind": "window_lifecycle",
  "semantics": { "op": "PENDING-EXTRACTOR-REVISION" },
  "status": "undetermined",
  "reason": "WINDOW_HOST_OPERATION_UNWIRED",
  "goya_evidence": "no host API found for this operation on any platform (docs/disparates/verifications/window-host-audit.md, checked 2026-11-19)",
  "detail": "window lifecycle operation with no found Goya API (show/hide, programmatic resize, focus activation, post-create title); becomes a host capability deficit or is served by a host revision"
}
```

Preconditions and cautions, per the map-v2 governance pattern:

- `WINDOW_HOST_OPERATION_UNWIRED` is a **new reason code**: it must be
  registered in `docs/disparates/reason-codes.md` and categorised in
  `category_of`/`blocking_side_of`
  (`crates/goya-disparate-coverage/src/lib.rs:499-534`) in the same revision
  that first emits it (amendment-B rule). Until then no map may emit it.
- The kind-only rule at position 6 of `default_map()` must be **replaced by**
  the facet rules, not preceded by them — first match wins, so an unscoped
  window rule would shadow every `op` rule.
- Removing or narrowing `MAPPED_TO_NATIVE_WINDOW_HOST`'s coarse form is a
  historical-compatibility decision for that revision: existing reason codes
  are never renamed, and already-assessed documents keep their recorded
  classifications (`docs/disparates/contracts/coverage-1.md`, Rules).
- Alternatively the same revision may add host APIs first (show/hide, resize
  set, focus via a tested activation protocol, title) and re-audit; the gap
  list of §3 is the work list either way.

## 5. Audit limitations

- **Static API read only.** Verdicts rest on reading public API items at this
  worktree's revision (paths and line numbers as of 2026-11-19). No symbol
  was executed.
- **No live window.** No window was launched and no Wayland display was
  opened; this audit performed no runtime check. Live behaviour is cited only
  from recorded precedents (GY-WAYLAND-001, exemplar `--live` runs,
  EMP-M1-01).
- **Compositor-decided semantics untested here.** `request_minimize`/
  `request_maximize` are requests the compositor may decline; "when supported"
  (`crates/goya-wayland/src/lib.rs:2073`) cannot be audited headlessly.
- **Extractor blind spots bound the corpus evidence.** The extractor records
  `window_lifecycle` only at `new BrowserWindow(` sites with empty semantics;
  foreign per-operation usage (win.show, setTitle, focus) is absent from the
  corpus, so the audit cannot measure observed foreign demand per operation —
  only host supply.
- **Windows evidence is generated-code text, not execution.** Sash's
  `ir_codegen.rs` was read as source; the emitted image has never run
  ("Windows execution is BLOCKED").
- **One-compositor precedent.** The single measured Wayland launch
  (EMP-M1-01) did not record the compositor, and the compositor matrix is
  Blocked (`docs/emperor/gap-ledger.md`, EMP-M1-07).
- **Line numbers drift.** Cited line numbers are exact for this revision and
  are navigation aids, not contract.
- **This audit admits nothing.** It records what the evidence string can and
  cannot support today; it grants no release, ratifies no contract, and does
  not certify representability on any platform.
