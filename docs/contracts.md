# Goya — Contract Declarations (Wave 1)

Status: Candidate (shared between active workstreams; architecture approval
required for changes). See GOYA directive §8.3.

## GY-IR/1 — Typed IR + capability schema (`crates/goya-ir`)

- Provides: `Module { nodes: Vec<(NodeId, Node)>, root: Option<NodeId> }`,
  `Node` (Column/Row/Text/Button/Spacer), `NodeId`, `ActionId`,
  `Capability`, `parse(&str) -> Result<Module, ParseError>`.
- Invariants: node ids are unique per module; `root` must reference a
  declared node; parsing is total (never panics on user input).
- Stability: Candidate. Parser grammar may grow (nested syntax, attributes)
  but the `Module`/`Node` shape is frozen for Wave 1.

## GY-STATE/1 — Revisioned state transactions (`crates/goya-state`)

- Provides: `Store` (Clone, thread-safe), `Transaction`, `Value`
  (Int/Text/Bool), `CommitRejected`.
- Protocol: `begin()` → `set()`* or one `stage_snapshot()` → `commit(invariant)`;
  drop without commit = rollback. `stage_snapshot` replaces the staged map,
  so a hot-swap migration can remove and rename keys. Single-writer enforced
  (`begin` returns `None` if a transaction is open). Committed snapshot is
  never mutated in place; commit swaps the staged copy atomically and bumps
  `revision()` only on success.
- Invariants: reads outside a transaction always observe the last committed
  revision; a rejected commit leaves state and revision untouched.

## GY-LAYOUT/1 — Deterministic layout (`crates/goya-layout`)

- Consumes: GY-IR/1.
- Provides: `layout(&Module, Constraints) -> Frame`, `Rect`, millipoint
  integer geometry (1 pt = 1000 mp).
- Invariants: layout is a pure function of (module, constraints) — no time,
  locale, RNG or environment dependence; identical inputs yield identical
  frames (unit-tested).

## GY-RUNTIME/1 — Event dispatch pipeline (`crates/goya-runtime`)

- Consumes: GY-IR/1, GY-STATE/1, GY-LAYOUT/1.
- Provides: `Runtime::new(module, store, constraints)`, `handle(Event) -> bool`
  (false = quit), `hit_test(x_mp, y_mp) -> Option<ActionId>`,
  `render_list() -> Vec<Primitive>`.
  `hit_test_target(x_mp, y_mp)` distinguishes button actions from editable
  input focus targets while preserving reverse draw-order hit testing.
- Invariants: events are processed synchronously; layout is recomputed only
  when the state revision advanced; unknown actions are inert. `TextBuffer`
  edits extended grapheme clusters, maintains selection and bounded undo/redo;
  declared `Node::Input` fields share this model for keyboard, IME batch, and
  accessibility value updates. Platform selection and composition offsets are
  UTF-8 bytes; selection and composition ranges expand to complete graphemes.
  A composition region is independent of the selection; active IME preedit
  visually replaces that region, falling back to the selection or caret when
  no region is set. `input_snapshot` returns displayed text (including
  preedit), selection, caret, and composition offsets in that same text.
  `FinishComposition` commits the preedit, while focus loss cancels it.
  `CommitInputText` applies text and its caret position atomically. Platform
  surrounding deletion preserves the selected range. `ScrollState` provides
  clamped two-axis content/viewport offsets. `Runtime::set_scroll_geometry`
  registers or updates scroll geometry for a declared node, and
  `Runtime::scroll_state` reads its current state. `Event::Scroll` applies
  content-space deltas to a registered node; positive deltas move toward the
  content's right and bottom edges. Platform hosts convert their native delta
  units before dispatch. Unknown or unregistered nodes are ignored, and
  geometry changes clamp existing offsets.

## Capability schema (Wave 1 form)

`Capability` enumerates the grantable rights (window, text-input, network,
filesystem, tts, motion, sensors, graphics3d). Wave 2 adds a manifest-level
grant/deny enforcement layer; Wave 1 only defines the vocabulary.

## Work-package DAG (Wave 1, completed nodes marked)

```
GY-IR-001 ✓ typed IR + parser            GY-STATE-001 ✓ transaction engine
GY-LAYOUT-001 ✓ deterministic layout     GY-RUNTIME-001 ✓ event pipeline
GY-HELLO-001 ✓ first integrated proof
GY-WAYLAND-001 ✓ live Wayland surface + runtime pointer adapter
GY-TEST-001 (partial): GY-IR/1 parser conformance and 20,000-input deterministic
mutation fuzzing are present, along with public SDK integration coverage for
parse/lower, frame geometry, canonical action IDs, hit testing, and runtime
dispatch. A second public SDK flow now covers proportional geometry at two
viewport sizes, valid and rejected scene swaps, state/render preservation on
rejection, and primitive geometry. A third verifies that an editable input
node preserves its role, placeholder, frame geometry, and rendered text through
the public SDK/runtime path. Authored D1/D2-to-VEE loopback acceptance is also
available. Broader platform/runtime conformance coverage remains open.
```

## Ownership registry (Wave 1)

| Path | Owner |
|---|---|
| crates/goya-ir/** | Swarm A (Language) |
| crates/goya-state/**, crates/goya-runtime/** | Swarm B (Runtime) |
| crates/goya-layout/** | Swarm C (Presentation) |
| crates/goya-hello/** | Swarm H (Integration) |

---

## GY-CHROMA/1 — Chromatic relativism engine (`crates/goya-chroma`)

Status: Candidate (Chromatic Swarm). Extension GY-CHR-001.

- Provides: `Oklch`/`Rgb8` with sRGB↔OKLCH conversion and chroma-shrinking
  gamut mapping; `PropertyRelation` (Absolute/Delta/Scale/Polar); `Polarity`;
  `Theme`/`ThemeRole`/`Harmony`/`WcagLevel`; `resolve(&Theme, Polarity)`.
- Resolution sequence (fixed, §5): anchor → inheritance → polarity →
  harmony → constraints → gamut-mapped composited output.
- Polarity model: `polar(d)` is a signed lightness relationship whose sign
  flips with theme polarity; the grey-line reflection `L' = Lg − p(L − Lg)`
  guards role sides around `GREY_LINE = 0.6`. Brand-locked roles preserve
  hue and chroma. Polarity is **not** an RGB complement.
- Constraints: contrast is enforced on the composited sRGB pair (WCAG AA /
  AAA) by deterministic lightness search in both directions; if unreachable,
  the resolver returns `ResolveError::ContrastUnreachable` — it never fakes
  compliance.
- Invariants: `resolve` is a pure function of (theme, polarity); unit-tested
  for determinism, hue preservation across polarity, and AA compliance in
  both modes.

## GY-PROVENANCE/1 — Text provenance (`crates/goya-provenance`)

Status: Candidate (Provenance Swarm).

- Provides: `SelectionRange` (clamping, char-boundary-safe), `SourceReference`,
  `TraceId`, `DiagnosticRecord`, `DeepCopyBundle`, `DeepCopyLimits`,
  `build_bundle`, `redact`/`RedactionRecord`.
- Policy: bundles are bounded (`max_bytes`, enforced on the rendered form,
  truncating diagnostics before ever touching the selected text); secrets
  (emails, bearer/API tokens, ≥32-char hex runs) are redacted with recorded
  reasons; traces/logs can be excluded per consent flags.
- Invariants: selection never panics on out-of-range or mid-codepoint
  offsets; an oversized selection fails loudly (`SelectionTooLarge`).

## GY-CLIPBOARD/1 — Dual-mode clipboard (`crates/goya-clipboard`)

Status: Candidate (Provenance Swarm).

- Consumes: GY-PROVENANCE/1.
- Provides: `CopyMode` (Verbatim / Deep), `CopyRequest`, `ClipboardProvider`
  trait, `MemoryClipboard`, `ClipboardCapability` + `CapabilityPolicy`.
- Contract: Verbatim copies exactly the selected visible text. Deep writes
  the provenance bundle as `text/plain` plus a structured
  `application/x-goya-provenance` payload. Every write is capability-gated
  (`Verbatim` vs `DeepContext` are distinct grants) and size-limited.
- Platform note: the Linux compositor data-device backend lands with
  GY-WAYLAND-001; shortcut registration must be remappable and must not
  steal Ctrl+Shift+C from embedded terminals.

## Ownership registry additions

| Path | Owner |
|---|---|
| crates/goya-chroma/** | Chromatic Swarm |
| crates/goya-provenance/**, crates/goya-clipboard/** | Provenance Swarm |

---

# Wave 2 — GY-SON-001, GY-NSB-001, GY-RPI-001, GY-EDC-001, GY-X402-001, GY-AIM-001

Implemented by a 10-agent concurrent swarm (J1–J10). All contracts Candidate.

## GY-SONIC/1 — Sonic events & policy (`crates/goya-sonic`, Swarm J)
`SonicEvent` with geometry-revision guard, `SonicResolver` trait, `SonicPolicy`/`PolicyEngine` (token-bucket rate limiting, mono/muted/spatial flags). Stale geometry revisions are rejected, never guessed.

## GY-SPATIAL-AUDIO/1 — Geometry → acoustics (`crates/goya-spatial-audio`, Swarm J)
Equal-power stereo `stereo_gains(p)` (power ≈ 1 across the sweep), viewport-relative `pan_from_rect`, bounded inverse distance law with floor, smoothed `StereoPan` trajectory, `fold_mono` accessibility fold-down.

## GY-AUDIO-DSP/1 + GY-AUDIO-LINUX/1 (`Swarm J`)
Deterministic decaying-sine `Tap`, priority-evicting `Mixer` (lower numeric = more urgent; graceful overload), hard + smoothed `Limiter`. Linux output includes `NullOutput` and a `pw-cat` PipeWire adapter; when the device/player is unavailable, the app remains operational.

## GY-BRIDGE-PHYSICS/1 + GY-BRIDGE/1 — NSB (`Swarm J`)
Fixed-1/240s semi-implicit spring (deterministic, dt-split-invariant), `clamp_overshoot` bites only on genuine overshoot (approach travel passes unclamped), retarget preserves position+velocity (interruption contract), reduced-motion snap, `interactive()` false the instant authoritative state exits even mid-animation. `Bridgeable` implemented for f64 and Oklch (shortest hue path).

## GY-RPI/1 — Probability field (`crates/goya-rpi`, Swarm K)
Conditional context-relative probabilities (count-based Level-1), Brier calibration, `net_value = pB − C − λW` ranking, `Preparation` valid only against matching state+geometry revisions, Level-2 learning requires enabled AND consent (default off).

## GY-SPECULATION/1 — Budgeted speculation (`crates/goya-speculation`, Swarm K)
Mesut-style budget never overruns; cache lookups valid only at matching revisions; commit returns Commit/Recompute/Reject; uncommitted speculative work dropped at commit; greedy value-per-cost scheduler skips unaffordable positive-value items without blocking.

## GY-DETERMINISM/1 + GY-EXECUTABLE/1 — EDC (`Swarm`)
D0/D1/D2/N classification with rejection of unresolved claims (a D0 claim with effects is rejected, never silently pure), versioned `ExecutionProof` (compiler/schema/policy/target), idempotent artifact registry, capability-escalation detection (artifacts cannot acquire privileges), staleness on schema/policy drift, compilation cost model.

## GY-X402/1 + GY-ECONOMICS/1 — Native x402 (`Swarm`)
Exact-integer `Amount` (never floats for money), challenge validation with asset+recipient allowlists (recipient substitution refused), lifecycle where Timeout/ReceiptLost → `Ambiguous` (never Failed — reconciliation before duplicate charges), `Allowance` with per-payment approval policy, `inherited_allowance` = MIN of ancestors (permissions narrow, never expand), immutable `IntentRecord`.

## GY-SETTLEMENT/1 — Reconciliation (`crates/goya-settlement`, Swarm)
Append-only ledger, Verified→Settled / Lost→NeedsInvestigation / DuplicateSuspected→DuplicateBlocked, payload-ref idempotency blocks duplicate authorisation, open-obligation audit, secret-free diagnostic redaction.

## GY-ARTIFACT/1 + GY-WATERMARK/1 — AIM (`Swarm L`)
Three separate identities (byte hash FNV-1a, perceptual 8×8 block-mean hash, provenance ref); `finalise` is the only constructor path (hash computed last); keyed mid-frequency 8×8 DCT embedding with per-position nonlinear keystream (wrong keys rejected — the naive key⊕position seed was GF(2)-linear and collided), majority-vote detection with confidence threshold and false-positive fail-closed.

## Ownership registry (Wave 2)
| Path | Owner |
|---|---|
| crates/goya-sonic, goya-spatial-audio, goya-audio-dsp, goya-audio-linux | Swarm J (Spatial Sonic) |
| crates/goya-bridge, goya-bridge-physics | Swarm J4 (NSB) |
| crates/goya-rpi, goya-speculation | Swarm K (Anticipatory Expression) |
| crates/goya-determinism, goya-executable | Swarm (EDC) |
| crates/goya-x402, goya-economics, goya-settlement | Swarm (x402) |
| crates/goya-artifact, goya-watermark | Swarm L (Artifact Identity) |

---

## GY-WAYLAND/1 — Native windowing host abstraction (`crates/goya-wayland`)

Status: Candidate (Swarm D — Platform). Completes GY-WAYLAND-001, the gate to release 0.1.

- Provides: `GoyaWindow::create(title, w, h)` → configured xdg-shell toplevel;
  `present(&[Primitive])` renders the runtime's render list into a
  shared-memory (wl_shm/memfd) buffer; `poll()`/`pump()` yield
  `PlatformEvent::{PointerMove, PointerPress, PointerRelease, Configured,
  CloseRequested}` with surface-local pixel coordinates.
- Consumers map pixels→millipoints (`MP_PER_PX = 250`) and route presses
  through `goya_runtime::Runtime::hit_test`.
- Policy shift, deliberate and recorded: the host layer depends on the
  official Smithay client bindings (wayland-client, wayland-protocols,
  rustix) rather than a hand-rolled wire protocol. The zero-dependency
  policy continues to apply to the foundation crates.
- Graceful degradation: `WaylandError::NoDisplay` when no compositor is
  reachable — the application remains operational headless.
- Keyboard: compositor XKB_V1 maps translate key presses into Unicode, with
  locale-based XKB Compose for dead-key sequences. The built-in US mapping is
  retained as a bounded fallback.
- IME: when `zwp_text_input_manager_v3` is present, callers enable the text
  input for focused fields via `set_text_input_active`. The compositor's
  preedit and commit_string arrive as `PlatformEvent::Preedit(Option<String>)`
  and `PlatformEvent::CommitText(String)` (commit text is delivered whole, not
  synthesized as KeyChar events); hosts insert at their own cursor. An earlier
  divergent `ImeBatch` design was removed in GY-ZCODE-006. Without this
  optional protocol, XKB/Compose remains available.

## Direction-aware spacer refinement (GY-LAYOUT/1 amendment)

A `Spacer` inside a `Row` is now a horizontal gap along the parent's axis
(previously it only contributed height, making it invisible in rows).
Deterministic and covered by `row_spacer_is_a_horizontal_gap`. The ARSEM
three-button acceptance geometry depends on it.

## Deferred-work DAG (explicit wave assignments)

| Work package | Wave | Unblocks |
|---|---|---|
| ~~GY-WAYLAND-001 live surface~~ | 1 (done) | 0.1 release gate |
| PipeWire audio adapter (`goya-audio-linux` backend) | 2a (implemented) | real ARSEM output; host/device smoke recorded |
| Hot-swap interface reconciliation + Tov CLI adapter | 2b (implemented) | release 0.2; CLI and compatibility tests |
| EDC canonical lowering + D0 bytecode/reference VM + D1/D2 journals | 2c (implemented) | Lowered scenes are complete trees and compile through the D0 reference VM, including an `@input` whose children are linked by a synthetic column. Authored source covers the bounded single-button D1 and static-journal D2 subsets; canonical decoders, journals, the SDK facade, and the loopback VEE smokes are in place. Branches, loops, comparisons, and authored D0 belong to a separate language contract. |
| VEE submission API + bounded D0 data bridge | 3a (implemented) | D0 and authored D1/D2 loopback smokes completed on 2026-11-12 with Landlock enforced on this host. VEE sets `no_new_privs` before `landlock_restrict_self` and still allows the system interpreter and loader to execute; a child test denies reads outside the work directory. The 2026-11-04 smokes fell back to rlimits after `EPERM`. Production coverage still depends on the deployed kernel and VEE build. D0 stdout may prefix sandbox diagnostics |
| RUGID / 3form / Mesut / Theosis adapters | 3b (implemented) | release 0.3–0.4; local companion-tool smokes |
| C2PA signing + explicit trust-aware verification | 3c (implemented, partial) | official-tool signing and an ephemeral trusted-root verification smoke passed; production signer identity, trust anchors/allowlist, and key custody remain release-owner decisions |
| HRTF profile loader + multichannel output | 3d (implemented, partial) | bounded SOFA converter and attributed 64-direction Viking HRTF fixture are included; additional subjects/full-dataset evaluation remains caller-supplied |
| RPI Level-2 learned anticipation (opt-in) | 3e (implemented) | anticipatory runtime |
| GTK/AT-SPI live-refresh bridge + accessibility conformance | 4 (bridge implemented, conformance open) | Linux AT-SPI tree/actions were observed; Orca logged no focus target, so navigation/announcements and WCAG evidence across supported platforms remain open |
| SDK, Studio CLI/browser preview, reproducible package + offline installer | 4 (implemented, partial) | browser editor has a geometry inspector and canvas drag; signed offline packages are supported; full IDE, publisher key custody/distribution, and release publication remain open |
| Watermark corpus + real codec evaluation | 4 (implemented, limited claim) | synthetic and caller-supplied local-image corpora plus MJPEG/PNG/H.264/MPEG-4 paths are available; a larger licensed photo corpus and evaluation across broader codec settings/builds remain |

## GY-HOTSWAP/1 — Persisted-state migration

`goya_hotswap::state_migration::migrate_state` applies explicit, ordered
one-version steps to a cloned `goya_state::Map`. Steps can rename a key,
remove a key, or add a default value. A chain must cover every version from
the stored version to the candidate's target version; gaps, invalid version
steps, missing rename sources, and rename collisions fail without returning
partial state. `SwapSupervisor::propose_swap` still rejects an unmigrated
higher state-contract version. `propose_migrated_swap` (and the reconciled
supervisor's matching method) runs the chain, asks the caller to publish the
snapshot with `Transaction::stage_snapshot` and `commit`, and activates the
candidate only after that commit returns `Ok`. A failed check, migration, or
commit leaves the supervisor unchanged.

## Assurance note

`goya-ir::parse` is now under a deterministic 20 000-input mutation fuzzer
(totality + determinism + root-integrity assertions). Its first run caught
`root N` with an undeclared N being accepted — fixed in the same commit.

---

## GY-LANG/1 — Scene language, RUGID-compatible (`crates/goya-lang`)

Status: Candidate. Architecture decision: the Goya surface language adopts
the RUGID scene grammar (github.com/elci-group/RUGID) in place of the
Wave-1 line-oriented module DSL. `goya-ir`'s programmatic API is unchanged;
both paths feed the same runtime/layout/render pipeline.

- Grammar: `@ontology <name> { ... }` root; `@cell <name> { ... }` surfaces;
  `@button <name> { ... }` interactive elements; `//` comments; inline
  `prop: value;` sequences; property blocks terminated by newline or `;`.
- Geometry: `position: (x, y, z)` / `extent: (w, h, d)` in absolute points,
  parent-origin-relative; `position_p` / `extent_p` in 0..1 fractions of
  the parent. Undeclared extent fills the parent. Geometry clamps into the
  parent. Millipoint scale matches GY-LAYOUT/1 (1 pt = 1000 mp).
- Appearance: `fill`/`stroke` hex colours (`#rgb`, `#rrggbb`, `#rrggbbaa`),
  `stroke-width`, `text`, `text-size`, `text-anchor: start|middle|end`,
  `scroll: true|false`, and `wrap: <positive integer columns>`. Studio previews
  wrap text to the node width by default (including long unbroken tokens);
  `scroll: true` makes the node a clipped, scrollable viewport for its text
  and child nodes. Descendants remain nested in the preview DOM, and layout
  preserves their overflow extent inside a scroll-enabled parent. `wrap`
  remains an explicit deterministic column-wrap override.
- Authoring extensions: `@template` blocks remain parsed and skipped;
  unknown visual properties are consumed and ignored. `@interaction` supports
  bounded D1 and static-journal D2 forms. Both require one
  `trigger: button.<name>.activate`, typed `state` and `input` blocks, and
  straight-line `set key = expr;` assignments. D1 uses `class: D1` and accepts
  scalar literals, `state.key`, `input.key`, and checked integer `+`.
  D2 uses `class: D2`, declares `effects { network; ... }`, then static requests
  such as `lookup: effect network capability "network" request "GET /items"
  result int64;`. Request results may be `int64`, `text`, or `bool`; expressions
  can read them as `effect.lookup` and may also use the D1 scalar/state/input
  forms and checked integer `+`. Capability strings must exactly match the
  declared EffectKind name. Each declared effect must be used by a request;
  each request must be referenced exactly once. Requests are emitted in their
  declaration order and replayed only from a caller-supplied `EffectJournal`;
  each request string literal's contents are encoded verbatim as UTF-8 bytes;
  requests have no dynamic interpolation. The compiler and wrapper perform no
  host effects. Every declared write must
  be assigned exactly once; every declared read/input must be used. A trigger
  that resolves to no button or to multiple same-named buttons is rejected;
  unrelated duplicate button names do not by themselves block compilation.
  Undeclared keys/effects, mismatched labels/result types, dynamic request
  bytes, unsupported syntax, and duplicate interaction names or trigger
  bindings are rejected. `compile_d1_interactions` and
  `compile_d2_interactions` select only their class, while uniqueness is
  validated across all interactions in the scene.
  The D1 and D2 compilers bind triggers to the same depth-first button
  `ActionId`s as scene lowering and emit only verified programs. Their typed
  execution wrappers check authored state/input and resulting write types
  around the VM. D2 exposes read-only declaration, proof, and capability
  metadata for caller-created journals. Raw VM programs are intentionally not
  exposed; future VM/VEE bridges must preserve authored scalar types because
  the VMs verify key sets but do not encode scalar types. Button traversal
  stops at buttons, matching IR lowering. Branches, loops, comparisons, and
  other expression forms are not yet source-language features.
- Totality: parse + layout never panic on any input — 10,000-input
  deterministic mutation fuzzer enforces it.
- Hit testing: `hit_test(&Frame, x_mp, y_mp) -> Option<button-name>` with
  depth-first later-drawn-wins order.
- Demo: `cargo run --bin scene [--file app.gscene] [--live]` builds a
  clickable GUI from a scene file.

Example D1 interaction:

```text
@interaction increment {
  trigger: button.plus.activate;
  class: D1;
  state { read count: int; write count: int; }
  input { amount: int; }
  set count = state.count + input.amount;
}
```

Example static D2 interaction (the caller supplies and records the journal):

```text
@interaction lookup_item {
  trigger: button.search.activate;
  class: D2;
  effects { network; }
  requests {
    lookup: effect network capability "network" request "GET /items/42" result text;
  }
  state { write result: text; }
  input { }
  set result = effect.lookup;
}
```

## Language decision record

The line-oriented DSL (GY-IR/1 parser) remains supported for backward
compatibility and programmatic use. New GUI authoring targets GY-LANG/1.
Unification plan: a GY-IR/2 amendment will define a shared node table so
both frontends lower into one IR; scheduled Wave 2c alongside the EDC
canonical IR work.

## GSSH/1, GSSH/2 and GSSH/3 — Shared semantic handoff (`crates/goya-lang`)

Status: GSSH/1 Candidate (in use by Sash, Emperor, goya-parity and the Aragon
previews); **GSSH/2 Approved** by the project owner, 2026-11-21 (Candidate
from 2026-11-17), including decisions D3 (`@input` `text:` is initial
content; display bindings still open) and D8 (durable record); GSSH/3
Candidate, 2026-11-18 (used by the Aragon scene compiler and Maja), still
awaiting approval. **GSSH/3-live** (`handoff::admit_v3_live`), 2026-11-21: GSSH/3 plus
`text-from` display bindings, approved as decision D9 with ecosystem-wide
wrapping and announcement rules (reference: `goya_lang::display_binding`);
Candidate only because GSSH/3 is. Used by Maja's `maja.web.local.v3`.
Specification: [`docs/gssh/GSSH-2.md`](gssh/GSSH-2.md); decisions:
[`docs/gssh/decisions.md`](gssh/decisions.md).

- `handoff::admit` / `parse_strict` (GSSH/1) is the GY-LANG/1 strict profile
  and rejects GSSH/2 declarations with `GY-E1006`.
- `handoff::admit_v2` / `parse_strict_gssh2` (GSSH/2) adds `@state { key:
  type = initial session|durable; }`, `input { key: type from "<@input id>"; }`
  with mandatory input and state coverage (`GY-E1007`, `GY-E1008`), the
  reference field conversion `handoff::convert_field`, and identity-keyed
  source origins (`handoff::Origin`).
- GY-LANG/1 amendment (all parser modes): `ParseError` and `LexError` carry
  exact UTF-8 byte ranges, and strict `GY-E1000` uses them instead of the
  whole source line. The `focus`/`blur`/`select`/`submit` gesture keys are
  removed. The compatibility parser records GSSH/2 syntax without validating
  it.
- `handoff::admit_v3` / `parse_strict_gssh3` (GSSH/3) is GSSH/2 with every
  length proportional (decision D7): absolute `position`, `extent`,
  `text-size` and `stroke-width` are rejected with `GY-E1009`, and
  `text-size-p` and `stroke-width-p` are admitted. GSSH/1 and GSSH/2 reject
  the latter two with `GY-E1006`. GY-LANG/1 amendment: inside a `scroll`
  element, `_p` fractions are no longer clamped above 1. Migration tool:
  `goya_lang::proportional` (`goya-proportional`).
- Conformance: `crates/goya-lang/tests/strict_corpus.rs` (exact-span reject
  fixtures, accept fixtures, token and byte mutation sweeps under all three
  profiles) and `tests/proportional_migration.rs`.

## GY-INTENT/1-draft — Quitasol intent declarations (`crates/goya-lang` + `crates/quitasol-semantic`)

Status: **draft/experimental — proposed, not ratified**. The shape may change;
nothing is admitted. Design record:
[`docs/quitasol/intents.md`](quitasol/intents.md); phase-0 scope:
[`docs/quitasol/phase0.md`](quitasol/phase0.md).

- Provides: `@intent Name { action: <interaction>; [transport: <class>;]
  voice { utterance: "..."; confirmation: bool; } fallback { remote:
  <target>; keyboard: <target>; } }` authoring syntax, parsed by the
  compatibility parser only — all strict GSSH/1/2/3 profiles reject
  `@intent` with `GY-E1010` ("intent declaration requires intent
  admission"). `IntentDecl` (name, authored_id, action, optional
  `TransportClass` transport, utterances, confirmation,
  fallback_remote/keyboard, range) is carried on `Scene`. The optional
  `transport` key (Q2 draft) declares one of six documented Fire OS Media
  Session transport behaviours (`play_pause`, `skip_next`, `skip_previous`,
  `seek_forward`, `seek_backward`, `restart`); unknown keywords and
  duplicate keys are parse errors. Q4 draft additions: optional
  `category: <keyword>;` (closed set search/playback/navigation/selection/
  settings/confirm/general, default general) and optional typed
  `slots { name: int|text|bool; }` blocks with fail-closed usage symmetry
  (every utterance slot reference must be declared; every declared slot must
  be used — `GY-E1011`); utterance text syntax is unchanged from pinned
  grammar `quitasol.utterance.v0`. `goya_lang::intents` validates intents
  with the pinned utterance grammar `quitasol.utterance.v0`, emitting
  `GY-E1011` ("invalid intent declaration") diagnostics.
  `quitasol-semantic::compile` emits a deterministic versioned intent
  manifest (contract `QUITASOL-INTENT-IR/1-draft`, profile
  `quitasol.intent.v0`, per-intent lowering tier `unresolved`);
  `quitasol-semantic::resolve` resolves device support and fallback
  reachability over Nevada (`QS-E2000`..`QS-E2003`), and
  `quitasol-semantic::fireos` emits the Fire OS integration plan
  (`QUITASOL-FIREOS-PLAN/1-draft`, `QS-E2004` on manifest/scene mismatch). `quitasol-semantic::runtime`
  lowers the plan to deterministic Kotlin MediaSession glue
  (`QUITASOL-MEDIASESSION-RT/1-draft`, `QS-E2005` on callback conflicts;
  never compiled in-tree).
- Invariants: parsing and validation are total and deterministic; validation
  never mutates the scene; every intent requires at least one utterance and a
  required non-voice fallback (remote or keyboard); duplicate utterances and
  duplicate intent names are rejected; unknown or duplicate slot names in
  `{slot}` placeholders are rejected, as are bare braces and empty
  utterances. The manifest serialisation is byte-deterministic for identical
  scenes.
- Stability: draft. GY-IR/1 is untouched — no intent nodes exist in
  `goya-ir`; a future IR home for intents requires a contract revision.

## GY-CHROMATICS-LANG/0.1-draft — Chromatics scene declarations (`crates/goya-lang`)

Status: **draft — proposed, not ratified**. The shape may change; no strict
profile admits it. Directive: GOYA-INT-CHR-001 §12. Engine contract:
GY-CHROMATICS/0.1 (below). Design record:
[`docs/chromatics/CHROMATICS-1.md`](chromatics/CHROMATICS-1.md).

- Provides: `@chromatics <name> { strategy: <keyword>; [fallback:
  <keyword>;] [preserve-slots: bool;] [ordered-slots: bool;]
  [exclude-native-equivalents: bool;] [exclude-duplicates: bool;] [stability
  { min-improvement: int; min-residency: int; max-deferral: int; }] candidate
  <Name> { intent: <interaction>; label: "..."; [entity: "...";] [risk:
  <class>;] [confirm: bool;] [native: "...";] [pin: <slot>;] [rank:
  uint;] [favourite: uint;] [position: uint;] [cost: uint;] [probability:
  uint;] [contextual: int;] [urgency: int;] [preference: int;] [symbol:
  <ident>;] [describe: "...";] } }`
  authoring syntax, parsed by the compatibility parser only — every strict
  profile rejects `@chromatics` with `GY-E1020` ("chromatics declaration
  requires chromatics admission"). At most one block per scene, in the
  ontology body. The schema is closed: unknown keys, repeated keys, unknown
  keywords, missing `strategy`/`intent`/non-empty `label`, an empty
  `describe`, non-canonical or out-of-range integers are parse errors,
  never defaults. Strategy and risk
  keywords are parsed by `goya_chromatics::Strategy::parse` /
  `Risk::parse`, so the scene grammar and the engine share one keyword set
  by construction. `ChromaticsDecl` and `ChromaticCandidate` are carried on
  `Scene::chromatics`.
- `goya_lang::chromatics` validates the statically determinable §12.3
  invariants, emitting `GY-E1021` ("invalid chromatics declaration")
  diagnostics: a candidate's intent must name a declared `@interaction`
  (the admitted unit of behaviour — not the experimental `@intent`),
  duplicate explicit slot pins and duplicate entities are refused,
  sensitive/destructive/privileged candidates must declare `confirm: true`
  (§20: a shortcut may enter a confirmation workflow, never bypass one),
  `probability` stays in 0..=1000 per mille and `cost` is at least 1.
  Capability requirements and scope inheritance are not statically
  representable in this grammar revision and are not checked.
- Canonical lowering: `allocation_policy` and `allocation_candidates` map a
  declaration to the engine's `Policy` and `Candidate` list, so every
  compiler and resolver lowers one declaration the same way. Authored
  numbers pass through unchanged; runtime statistics (`last_used`, `uses`,
  `expires`) are not authored and start neutral.
- Conformance: `crates/goya-lang/tests/chromatics_corpus.rs` — end-to-end
  allocation through the engine (including §10.4 slot preservation across a
  favourite swap and §19 profile isolation), §21 `symbol`/`describe`
  pass-through into the published binding, the six GY-E1021 invariants,
  fourteen closed-schema refusals, and GY-E1020 strict refusal under GSSH/1
  and GSSH/5.
- Stability: draft. No target lowers `@chromatics` yet; Arachnid's
  precomputed and generated-JavaScript lowerings (CHROMATICS-0 §5, decision
  2) are the intended first consumer.

## SASH-WIN-IR/1 — Sash Windows emission IR (`crates/sash-ir`)

Status: Candidate, 2026-11-18. Architecture approval is required before a
target may cite it as a frozen contract. Specification:
[`docs/sash/SASH-WIN-IR-1.md`](sash/SASH-WIN-IR-1.md).

- Consumes: GSSH/1 admission output (`AdmittedWindow`,
  `AdmittedInteractive`). Admission stays the gate for reachable meaning;
  the IR adds, drops, and defaults nothing.
- Provides: `IrDocument` (version, profile, window, controls, optional
  interactive `Transition` and `GesturePolicy`, identity, resources,
  provenance with per-control authored origins), lowering in
  `crates/sash/src/ir_lower.rs`, code generation in
  `crates/sash/src/ir_codegen.rs`.
- Invariants: lowering and codegen are pure functions; two lowerings of one
  admitted scene are equal documents, and two code generations of one
  document produce equal bytes and equal digests. Every evidence source-map
  origin decodes to the authored bytes it names. Emission is byte-identical
  to the adopted direct emitters; fixture digests are pinned in the
  `ir_codegen` tests, so any drift fails the suite.
- Stability: Candidate. Field shape changes bump the major version; profile
  changes never do. The serde form is for tooling and tests only — no build
  artifact carries an IR document yet.
- Not claimed: no new profile, no admission change, no Windows runtime
  qualification. Runtime stays BLOCKED until a Windows worker launches the
  images.

## Correction — Vico Vee reconnaissance (2026-11-04)

The GY-001 reconnaissance table incorrectly identified Vico Vee as the thin
Tauri discovery GUI at `~/vee-gui`. The directive-relevant component is the
**ViCo Execution Environment** at
`~/vico-desktop/src-tauri/vico-vee` (v0.2.20):

- Standalone sandboxed execution service, extracted from vico-desktop, REST
  API on :9987 (submit / status / cancel / artifacts / backups).
- **Exactly Goya's containment layer**: capability-based security with
  Ed25519-signed grants, worker isolation via rlimits + Landlock +
  seccomp-bpf, wall-clock budgets with cancellation, content-addressed
  artifact store, multi-tenant project isolation, Prometheus metrics.
- **Has a lib target** (`src/lib.rs`, 26 `pub mod`s: capability, artifact,
  audit, checkpoint, daemon, …) plus the `vico-vee` binary — no extraction
  work needed, unlike the earlier assessment claimed.
- Tool integration contracts (`docs/vee-tool-integration-contracts.md`)
  already define the governance Goya needs: capabilities declared before
  execution, typed artifacts, budget/cancellation, no credential passthrough.

Consequences for the DAG:
- Wave 3a was reframed as **"VEE adapter: route Goya EDC executable units
  and agent tool calls through VEE's capability/budget/artifact machinery."**
  The HTTP client matches VEE's submit API for caller-supplied source, and
  bounded D0/D1/D2 workers are available. Typed D1/D2 source wrappers
  construct bounded worker requests after validating authored scalar types;
  an opt-in loopback smoke has also submitted both requests and verified the
  returned state. Broader source-language features remain open. On 2026-11-12
  the D0 and authored D1/D2 loopback smokes completed with Landlock enforced
  on this host. The 2026-11-04 runs fell back to rlimits after `EPERM`.
  Production coverage still depends on the deployed kernel and VEE build.
- The Wave-1 evidence table row for Vico Vee is superseded by this section.

---

## Wave 2a/b additions (2026-11-04, afternoon wave)

### GY-WAYLAND/2 keyboard layout support
- The client consumes compositor-provided XKB_V1 keymaps up to 16 MiB and
  updates XKB state from Wayland modifier/layout masks. Evdev key events are
  translated through that state to Unicode `KeyChar` events, including
  Shift/Caps Lock and layouts such as German AltGr. Unsupported or malformed
  keymaps retain the bounded US fallback. Locale-based XKB Compose handles
  dead-key sequences where a compose table is installed. Tests cover US
  Shift/Caps Lock, German AltGr, and fallback.

### GY-WAYLAND/1 amendments — keyboard & resize
- `PlatformEvent::KeyChar(char)`: initial `wl_keyboard` support used an evdev
  US QWERTY subset; it is superseded by the GY-WAYLAND/2 XKB keymap handling
  above.
- `PlatformEvent::Resized(w, h)`: xdg toplevel configure carries new pixel
  dimensions; the fixed-size shm frame is dropped and reallocated at the
  next present, and the caller relayouts against the new viewport.

### GY-AUDIO-LINUX/1 amendment — `PwCatOutput`
Real audible output, achieved without C bindings: the adapter streams a
canonical float32 WAV into the system PipeWire player (`pw-cat`,
falling back to `paplay`, then `aplay`). Contract: spawn failure degrades
to `DeviceState::Unavailable` (application stays operational — the
directive's missing-device criterion); writes to a dead child return
Ok(0) (graceful sample drop, no error storm). **Verified live**: a 0.25 s
880 Hz tap played through the running PipeWire graph (12,000/12,000
samples accepted). Example: `cargo run -p goya-audio-linux --example audio_smoke`.
Demos: `cargo run --bin scene -- --live --sound` plays a tap on every
button activation.

### GY-LANG/1 amendment — `SceneStore` transactional hot-swap
`goya_lang::SceneStore::swap(&str)` replaces the running scene only when
the new source parses; a failed parse leaves scene and generation
untouched (unit-tested both paths). Callers keep interaction state keyed
by element name across swaps. Demo: `cargo run --bin scene -- --live
--watch --file app.gscene` reloads on file change — verified live: a good
edit bumped generation 1→2, a syntactically broken edit was rejected with
the running scene retained. This is the Wave-2 acceptance criterion
"running application accepts a validated external change, updates its UI
and retains state across a failed reload".

### Runtime note
`goya-runtime` hello text now binds the `typed` state key (keyboard input
path demonstrated end-to-end: compositor → KeyChar → `Event::Character`
→ state transaction → relayout → presented frame).

---

# Efficiency Hierarchy — GY-LCPE-001, GY-RARS-001, GY-CSR-001, GY-HCAR-001

Implemented as four new crates (all std-only, Candidate status):

## GY-CHANGE/1 — Localised Change Potential Efficiency (`crates/goya-change`)
- `ChangeEnvelope` per directive §5: object, revision, `PropertyMask`
  persistent/mutable, `EventRule`s, conservative `spatial_bounds`,
  `resolution_tile`.
- `CausalResolver`: input→transition→dirty-region causality. Events are
  **rejected by default** (capability-style); a rendered pixel changing
  never causes a state transition through this path (no feedback loops).
- Resolution-aware invalidation: dirty regions quantise to device-pixel
  tiles, growing **conservatively** (never smaller than the true area).
- Persistence enforcement: transitions may not produce dirty regions for
  properties the envelope marks persistent.
- Cost model (§8): `C_local = resolve + commit + render` vs `C_full`;
  regions union first (one commit); local chosen only when strictly
  cheaper. Full-frame is always permitted.
- Critical qualification enforced: quantisation affects presentation only;
  authoritative state (revision anchor) is independent of pixels.
- 8 tests: causality authorisation, persistence blocking, quantisation,
  cost-model boundaries, resolution hierarchy, semantic preservation.

## GY-RARS/1 — Resolution-Adaptive Resource Scaling (`crates/goya-rars`)
- `FidelityResolver`: deterministic pure function of
  (ProjectionFidelityContext, policy_version). Never touches state.
- Screen-space-error subdivision model (§3): n ≈ √(1/ε_px), clamped [2,64].
- **Spatial and temporal complexity are independent** (§2): a small
  fast-moving object gets reduced raster scale but Continuous updates.
- Accessibility policy: text fidelity protected (glyph_px ≥ 2 when
  protect_text) regardless of projected size; E_interaction = 0.
- `Hysteresis` (§8): fidelity-level changes require N consecutive
  confirmations; first observation seeds residency immediately.
- 7 tests incl. the directive's 1.9%-of-4K model check.

## GY-CSR/1 — Core-Split Rendering scheduler (`crates/goya-csr`)
- Partition the **affected change envelope**, not the frame (§3): serial /
  dual-horizontal / quad-corner policies over a tile grid, worker-budget
  capped.
- Partitioning defines work OWNERSHIP, not draw order; territories are
  provably disjoint and complete (unit-tested).
- Greedy work stealing: idle workers take from the largest queue
  (§5) — unit-tested rebalancing under uneven territories.
- Policy selection avoids parallel overhead for small workloads (§3):
  ≤4 tiles → serial. Worker budget, never raw CPU count (§2).
- 6 tests. `goya-wayland` now consumes `partition`/`schedule` for an
  experimental CPU tile raster path; each tile replays the primitive list in
  original order and writes a disjoint buffer rectangle.

## GY-HCAR/1 — Heterogeneous core classification (`crates/goya-hcar`)
- Per-work-item classification (never per quadrant permanently, §2):
  E-core deadline violation or critical-path ⇒ P-core mandatory;
  otherwise energy-minimising choice among feasible classes (§3:
  min Σ E subject to deadlines).
- Homogeneous CPUs degrade to `General` (§6).
- `Residency` migration hysteresis (§6): class changes require N
  consecutive confirmations; seeded on first observation.
- Affinity modes: OsManaged default / Advisory / Controlled (§7).
- Linux `CpuTopology::detect_linux()` reads online CPU IDs and conservative
  `cpu_capacity`/ACPI `highest_perf` signals. It reports topology metadata,
  but does not pin threads or change the OS-managed renderer.
- Temporal persistence expressed in tests (§4): cheap-but-critical scroll
  tiles → P; relaxed status indicators → E.
- 11 tests, including fixture-driven topology parsing; live sysfs detection is
  opt-in and has been exercised on the current Intel Core 5 120U host.

## Integration (demonstrated in `goya-calculator`)
- LCPE: envelopes compiled from resolved scene geometry per generation;
  presses resolve → quantised dirty regions → cost decision →
  `present_dirty` partial damage to the compositor.
- RARS: glyph fidelity resolved from the projected window vs 4K
  (3.28% → balanced quality, protected-text glyph 4px).
- Pipeline status: LCPE ✓ RARS ✓ CSR scheduler ✓ HCAR classifier ✓; tile
  raster integration is correctness-tested, but remains opt-in because the
  current synthetic benchmark is slower than serial. On an Intel Core 5 120U,
  one manual run pinned to CPU 0 (5.0 GHz advertised max-frequency group)
  measured 180.4 ms serial vs 887.9 ms parallel over five renders; pinned to
  CPU 4 (3.8 GHz advertised max-frequency group), it measured 176.1 ms vs
  1048.6 ms. The OS did not expose a `core_type` file in this environment, so
  the P/E grouping is inferred from frequency topology.
  These are single-run exploratory measurements, not stable performance
  claims. The available path still uses OS-managed threads and HCAR `General`
  classifications. `goya-hcar` now exposes conservative Linux topology
  detection from online-CPU `cpu_capacity` or ACPI `highest_perf` sysfs data,
  but the renderer does not yet consume that metadata or apply core affinity.
  The opt-in live probe (`GOYA_HCAR_LIVE_TOPOLOGY=1 cargo test -p goya-hcar
  live_linux_sysfs_topology_smoke_when_enabled -- --nocapture`) reported
  ACPI `highest_perf`, CPUs 0–3 Performance and 4–11 Efficiency on this Intel
  Core 5 120U host. This validates sysfs classification only.
  Process-level `taskset` measurements do not validate application affinity.
  See `evidence/GY-CSR-RASTER-INTEGRATION.json` for the unpinned baseline.

## Ownership
| Path | Owner |
|---|---|
| crates/goya-change | Efficiency Swarm M1 (LCPE) |
| crates/goya-rars | Efficiency Swarm M2 (RARS) |
| crates/goya-csr | Efficiency Swarm M3 (CSR) |
| crates/goya-hcar | Efficiency Swarm M4 (HCAR) |

---

## GY-FSC/1 — Formatting Similarity Compression (`crates/goya-fsc`)

Status: Candidate. Implements GY-TYP-002 (supersedes the DSC portions of
GY-TYP-001): compression operates on FORMATTING similarity, never glyph
shapes.

- `ProfileStore`/`FormattingProfile`: shared immutable profiles with
  parent inheritance. Resolution walks root→leaf, deeper overrides win;
  cycles and missing parents return errors, never loop or panic.
- **Inheritance-aware identity** (§3, the crucial distinction): profile
  identity = resolved values + LEAF-explicit property set + ancestry
  depth. Two objects that look identical today but override at different
  levels are distinct — a theme/parent change affects them differently.
- `FormattedText` in-object variance (§4): base profile + `Vec` of typed
  range deltas; `effective_at(pos) = F₀ ⊕ Δ(pos)`.
- Stable anchors (§4): insertion inside (or at start of) a delta extends
  it — typing at a styled boundary adopts the style; insertion before
  shifts; deletion intersects/clamps. Content edits are materialised with
  char-boundary snapping; all offsets clamp; never panics.
- Canonicalisation (§7): sort, drop empties, dedupe, merge adjacent
  equal-override ranges.
- Run-based rendering input (§7): `runs()` computes formatting runs from
  the delta boundary set — O(d log d + d·r), not per-character resolution.
- Invalidation table (§6): property → (reshape, relayout, reraster,
  recompose). Colour/opacity/underline = compose-only; family/size/weight
  = full; spacing/line-height/align = relayout+recompose with mask reuse.
  LCPE bridge: `lcpe_property_mask` feeds goya-change PropertyMask bits.
- Acceptance (§10, unit-tested): 100,000 chars, 1,000 variations, one base
  style — global recolour leaves run structure identical (compose-only);
  one-word weight change is local reshape scope; insertion inside a
  formatted range preserves anchors (delta extends 100..120 → 100..123).
- 11 tests incl. a bounded 800-iteration totality fuzz.

## Ownership
| Path | Owner |
|---|---|
| crates/goya-fsc | Typography Swarm T1 (FSC) |

---

## Audit remediation (2026-11-04, external source audit of main @ 56b4d15)

### P0 — Buffer ownership (FIXED)
`goya-wayland` now uses a **triple-buffered** pool and **never writes an
unreleased slot**: presentation picks any released non-front slot; if all
three are compositor-owned the frame is **skipped** (`present_dirty`
returns without committing) and retried after pump() processes
wl_buffer releases. Front slot tracking updated on commit.

### P1 — Incremental rasterisation (IMPLEMENTED)
`present_dirty` copies the front slot's undamaged content into the target
slot (compositor-safe read), clears only the damage rect, rasterises only
primitives intersecting it (clip-scoped fill/glyph paths), and damages
only that rect. Full-frame path unchanged when no damage resolved.
Note: raster clipping is rectangle-level. An opt-in CSR CPU tile path is now
implemented and covered by serial-equivalence tests, but its current measured
regression keeps it disabled by default. HCAR supplies per-tile `General`
classification only. Linux P/E topology detection is available as HCAR
metadata, while renderer integration and application affinity remain deferred.

### P1 — FSC content mutation (VERIFIED ALREADY FIXED)
The audit inspected HEAD 56b4d15 (pre-fix). Current `apply_edit`
materialises content changes with char-boundary snapping; three new
tests prove UTF-8 validity, boundary snapping and atomic
content+anchor mutation.

### P1 — CI (ESTABLISHED)
`.github/workflows/ci.yml`: build, tests, clippy -D warnings, rustfmt
check, uni snapshot on every push/PR. Branch protection is a GitHub
settings action (owner-side).

### Deferred (recorded, not forgotten)
- Typography: cosmic-text shaping/rasterisation, wrapping, per-node size and clipping are implemented in `goya-wayland`. Bitmap fallback wrapping now preserves grapheme clusters. Remaining work is shared shaped-layout geometry (text extent, caret, selection, hit testing), font/fallback policy, accurate IME cursor placement, and glyph-level/live acceptance. The original `evidence/GY-WAVE1-001.json` 5x7 statement is a historical Wave-1 snapshot; it does not describe the current renderer.
- GY-LANG lowering to the shared `goya_ir::Module` and `NodeTable` is implemented and consumed by the SDK, and `SceneProject::compile_interactions` exposes the approved D1/D2 subsets. A lowered module is a complete tree: an `@input` with children is linked through a synthetic column because `Node::Input` has no child list, so `compile_module` can admit it. This does not change the `GY-IR/1` contract identifier. Branches, loops, comparisons, other expression forms, and authored D0 remain out of scope pending a separately versioned language contract and architecture decision.
- HCAR hardware topology/dispatch — needs real P/E hardware profiling.

---

## GY-DDH/1 — Deterministic Dehumanisation (`crates/goya-ddh`)

Status: Candidate. "Every user-operable GOYA object SHALL be discoverable,
locatable and actionable through a deterministic, permission-governed
semantic interface, without requiring visual interpretation of its
rendered projection."

- **DIG** (Deterministic Interaction Graph, directive §7): semantic nodes
  with roles, labels, hit regions, interaction state, permitted actions,
  exposure policy and typed parent edges. Not a DOM — executable,
  spatially aware objects.
- **Mode A**: `resolve_target` → `TargetLease` (centre of hit region +
  revision); leases invalidate on any node revision change — a prediction
  is never a guaranteed click target.
- **Mode B**: `validate_invoke` — authority + exposure + enabled + action
  permit checked before dispatch; the caller performs the transition.
  Order of checks: NoSuchObject → NotExposed → Disabled →
  ActionNotPermitted → AuthorityDenied → Dispatched.
- Information hygiene: `exposed: false` nodes are never exported via
  `find`/`exported` and return NotExposed on invoke.
- `observe_changes(since)`: LCPE-fed incremental semantic updates.
- **Acceptance experiment (directive §7), unit+binary verified**: a
  text-only agent computes 87 × 42 through the calculator's DIG —
  `find` → `invoke` ×6 → observe — verifying 3654 with **512 semantic
  bytes vs 1,088,000 screenshot bytes (2125× smaller)**, zero stale
  leases, and an unauthorised display-press correctly rejected.
  Run: `cargo run --bin calculator -- --ddh`.

## Ownership
| Path | Owner |
|---|---|
| crates/goya-ddh | Agent-Interface Swarm N1 (DDH/DIG) |

---

# Exemplar Applications — GY-APP-001 (calendar, music, clock, files)

Four complete applications authored in GY-LANG/1 scene syntax with thin
deterministic hosts, each following the calculator exemplar contract:
scene declares structure; host supplies state; headless self-test asserts
behaviour; `--live` opens a window with dirty-redraw presentation.

| App | Scene | Host behaviours | Acceptance highlights |
|---|---|---|---|
| Calendar | apps/calendar.gscene | Gregorian math: leap rule incl. century exceptions, monday-first grid, month navigation, today reset | Feb 2024=29/Feb 2023=28, 2000-01-01=Saturday, Nov 2026 day 1 lands col 6 (Sunday), Nov has 30 days at slot 35 |
| Music player | apps/music_player.gscene | 5-track playlist state machine: play/pause, wrapping next/prev, clamped volume, auto-advance at track end, ASCII progress bar | tick advances only while playing; exact 50% bar = `xxxxxxxxxx..........` |
| Clock | apps/clock.gscene | Synthetic clock: h/m/s bumps with day rollover, reset; no wall clock in self-test | 23.59.59 + 1s → day 2; 86400 consecutive seconds = exactly 1 day (no drift) |
| File manager | apps/file_manager.gscene | In-memory VFS: sorted listings with `/` dir markers, open/up navigation, selection status | open docs → intro.doc notes.txt; up at root stays; padding to 8 entries |

All four: live Wayland windows verified (dirty-redraw), glyph-safe labels
(GY-LANG/1 renderable alphabet), host-side data binding overrides scene
text cells, LCPE-style dirty-only redraw.

Known limitation recorded: the glyph set covers a lowercase subset of
Latin — uppercase/other characters render as boxes (Wave 4 typography).

## Fix during integration
Calendar label off-by-one (1-based month indexing a 0-based name array)
and a day-31-in-November test premise error — both caught by the
self-test gate, fixed in the same commit.

## IE-X01/1 — Iberia/España shared contract v0.1 (`crates/ie-contract`)

- Provides: `TypedError` (11 frozen wire codes), `CapabilityDescriptor`,
  `Request`/`Response`, `TensorDesc`/`TensorData` (FP32/INT8, static shapes,
  checked shape product), bounded length-prefixed framing
  (`encode_frame`/`decode_frame`, MAX 16 MiB, oversized rejected before
  allocation), `EndpointState` lifecycle (only `Ready` admits jobs), and the
  FP32 golden oracle (`GoldenFixture`, abs 1e-5 / rel 1e-4 reference
  tolerances) with frozen fixture `fixtures/golden_t01_matmul.golden`.
- Invariants: deadlines are remaining durations, never peer wall-clock;
  protocol version mismatch is VERSION_MISMATCH, never best-effort; shape
  and payload validation precede allocation; conformance seeds T01–T03 run
  against the contract only, independent of any backend.
- Stability: Frozen at G1 (ADR-IE-001). Field-level canonical wire encoding
  of messages is not yet frozen; changes require a new ADR and downstream
  notification per IE-E01.

## IE-X01/2 — Canonical wire codec (`ie-contract::codec`, frozen)

- Provides: `encode/decode_request`, `encode/decode_response`,
  `encode/decode_capability`, `encode/decode_tensor`; all little-endian,
  u32-length-prefixed, count-plausibility-checked before allocation.
  Known-answer vector test pins canonical bytes.
- Invariants: `VersionMismatch` on capability protocol-version mismatch;
  error detail round-trips through `TypedError::from_code` (including
  VersionMismatch `local:remote`); truncated buffers always error, never
  panic.

## IE-CPU/1 — CPU reference oracle (`crates/ie-cpu`, WP-003 seed)

- Provides: `execute(op, inputs) -> Result<TensorData, TypedError>` over the
  golden-op subset (`add`, `matmul`); `SUPPORTED_OPS` allowlist.
- Invariants: deterministic FP32 in fixed iteration order; validation before
  compute; typed errors only. G2 conformance seed reproduces frozen golden
  fixture T01 exactly and deterministically.

## IE-CPU/2 — Resource ledger and cancellation (`ie-cpu::ledger`, WP-004 seed)

- Provides: `Ledger::new(queue_limit, retention_capacity)`,
  `register_model`, `submit`, `run_one(now_elapsed_ms)`, `poll`, `cancel`,
  `queue_depth`.
- Invariants: bounded queue — saturation is `RESOURCE_EXHAUSTED`, slot
  released atomically on cancellation or terminal completion;
  request-ID dedup within retention (no double-queueing); deadlines are
  remaining durations enforced backend-side, expiry is
  `DEADLINE_EXCEEDED`; unregistered model digests are `MODEL_MISSING`;
  consumed terminal results report unknown outcome (`PEER_LOST`) rather
  than replaying — no exactly-once claim. Op dispatch by rank (rank-1 add,
  rank-2 matmul) is a reference-backend seed decision, not a wire rule.
- Stability: Candidate. Conformance seeds T04/T05 embedded as tests.

## IE-ESP/1 — Loopback provider transport (`crates/ie-loopback`, WP-005 seed)

- Provides: `Provider` (Describe/AcquireLease/Submit/Poll/Cancel),
  `Lease { lease_id, epoch, expiry_ms }`, `handle_frame` (framed canonical
  request → framed canonical response), `revoke()` (epoch bump).
- Invariants: no lease, no service — submit without a valid lease is
  `POLICY_DENIED`; stale-epoch leases are `UNTRUSTED_PEER`; expired leases
  are `POLICY_DENIED`; responses carry the provider epoch so callers can
  discard stale completions; `handle_frame` never fabricates success for an
  unconsumable result (PEER_LOST).
- Stability: Candidate. TLS/mTLS transport, pairing and peer trust are
  later S3 packets; this mock is clearly marked non-production per IE-E01.

## IE-GATE/1 — Goya feature declarations and runtime gate (`crates/ie-gate`, WP-007 seed)

- Provides: `FeatureDecl` (required ops, model digest, max input bytes,
  max latency, `Sensitivity` Shareable/LocalOnly, `Fallback`
  Disable/Defer/Substitute/Required), `compile_manifest` (compile-time
  validation; empty op sets are build errors), `evaluate` against an
  authenticated `CapabilityDescriptor`, `Verdict` with
  `remediation()` text.
- Invariants: availability is only ever asserted against an
  authenticated descriptor (PR-04); unavailability is typed
  (Unsupported/ResourceLimited/LatencyUnmet/PolicyBlocked/VersionMismatch)
  with remediation text, never a bare false; LocalOnly features are
  policy-blocked on non-local endpoints (PR-05); a provider publishing no
  latency budget is LatencyUnmet, not assumed fast.
- Stability: Candidate. T11 seed embedded. Wiring to `goya-ir` scene
  compilation and the disconnect/epoch-discard path (P02) are later S4
  packets.

## IE-ESP/2 — Pairing and provider quotas (`ie-loopback::pairing`, WP-006 seed)

- Provides: `SharedProvider` over the loopback `Provider` —
  `offer_pairing(client, code, quota)`, `pair(client, code)`,
  `revoke_client`, `acquire_lease` bound to the paired identity,
  quota-checked `submit`/`cancel`, lease-release on revocation.
- Invariants: no discovery message confers trust — pairing is the only
  admission path; wrong/unknown codes are `UNTRUSTED_PEER`; revoked
  clients get `POLICY_DENIED` on new work with their lease released;
  quotas (max concurrent queued, max deadline) are per-client and breach
  is `RESOURCE_EXHAUSTED`/`POLICY_DENIED` without starving other clients;
  presenting another client's lease is `UNTRUSTED_PEER`; the underlying
  provider is multi-lease per epoch, all invalidated on `revoke()`.
- Stability: Candidate. Out-of-band code equality models the
  user-confirmed pairing step; real key verification and TLS binding are
  later S3 packets.

## IE-ESP/3 — Flagship demo A, loopback edition (`ie-loopback/tests/demo_a.rs`)

- Four legs: (1) gated feature advertises on a paired capable peer and
  produces transport parity with the golden result; (2) peer loss
  mid-request — stale-generation completion rejected by lease epoch, gate
  re-evaluates to typed Unsupported with remediation, fallback Disable
  preserves the baseline; (3) untrusted provider refused at pairing and
  at lease acquisition; (4) reconnect requires explicit re-admission via
  a fresh epoch lease.
- Evidence class: loopback transport, no network or hardware. Per IE-E01
  evidence rules this is a seed, not a gate pass; the real demo A requires
  a physical second host and measured latency.

## IE-UDS/1 — Unix-domain-socket transport (`crates/ie-uds`, WP-005 completion candidate)

- Provides: `Server` (UDS accept loop, framed request → handler → framed
  response; handler errors surface as canonical `Failed` responses) and
  `Client::call` (encode → frame → bounded read → decode). Bounded reads
  validate the length prefix against MAX_MESSAGE_BYTES before reserving
  the body; oversized declarations close the connection with a typed
  error; all sockets carry read/write timeouts; peer-input failures are
  typed errors, never panics.
- Stability: Candidate. Not authenticated TLS — this is the
  same-trust-boundary LAN transport step; mTLS and pairing-over-wire are
  the next S3 packets. Connection lease is server-side (the untrusted
  request `lease_id` field is ignored by dispatch, mirroring
  `handle_frame`).

## IE-CONF/1 — Independent fault-injection harness (`crates/ie-conformance`, WP-009 seed)

- Tests-only crate (no public API): seeded deterministic decoder
  mutation (~4000 mutations/run, never-panic, implausible-length rules),
  ledger fault injection (cancellation/deadline/retention boundaries,
  no-false-exactly-once), pairing trust injection (replay, expiry,
  theft, code enumeration, quota boundaries), and golden-oracle
  cross-check asserting bit-for-bit parity between ie-cpu and the
  ie-contract reference oracle.
- Finding: BUG-IE-LOOPBACK-01 (revoke_client left queued work running
  after consent ended) — found by this harness, fixed in ie-loopback,
  regression test un-ignored and passing.

## GY-GFXC/1 — Shared graphics container (`crates/goya-graphics-container`, GOYA-GFX-001 Phase 0)

- Status: Ratified 2026-11-24 (decisions D1–D5). Normative text:
  `docs/graphics/container.md`. Programme notes: `docs/graphics/README.md`.
- Provides: `encode(Format, Profile, features, &[SectionInput], &Limits) -> Vec<u8>`,
  `Container::open(&[u8], &Expect, &Limits)`, `section(Tag)` (verified on
  fetch), `verify_all()`, `Format` (`GRU\0`/`GVA\0`/`GVI\0`/`SPC\0`), `Tag`,
  `Profile::Baseline`, and `GfxError` (stable `GFX-E1xxx` and `GFX-E4xxx` codes).
- Invariants: 80-byte header and 72-byte directory entries, little-endian. Exactly
  one canonical layout per input (sorted tags, fixed offsets, zero padding),
  so encoding is byte-deterministic and does not depend on input order. The
  envelope digest (SHA-256 of header and directory, digest field zeroed)
  commits to every per-section SHA-256, and so to every byte. Limits and
  checked arithmetic apply before allocation. Unknown mandatory sections and
  required features are rejected. A digest is never treated as authenticity.
- Golden fixture: `fixtures/graphics/container/sample.gva`.

## GY-GIR/0 — Graphical intermediate representation (`crates/goya-graphics-ir`, GOYA-GFX-001 Phase 1)

- Status: Draft. Spec: `docs/graphics/gva.md` §1 and §6.
- Provides:
  - `Path` (canonical segment lists in millipoints) and `Segment`;
  - `Style`, `Fill` and `Stroke`, plus `Affine` (row-vector, Q16.16 linear
    part, millipoint translation);
  - `Item` (an `Arc<Path>`, style, transform and opacity) and `Drawing`;
  - `Quantiser` (round-half-even, with a precision gate);
  - `GfxDiag`, with `codes` for E2xxx, E3xxx, E41xx and E42xx;
  - `reference::render`, the CPU conformance oracle.
- Invariants:
  - Canonicalising a path is idempotent.
  - Affine application is exact integer arithmetic.
  - Rendering uses only IEEE `+ − × ÷ sqrt`, so output is bit-identical
    across hosts. Folding a translation between a path and its transform
    does not change any output bit.
  - Render limits (pixels, edges, curve steps) are enforced.

## GY-GVA/0 — Graphical Vector Array (`crates/goya-gva`, GOYA-GFX-001 Phase 1)

- Consumes: GY-GFXC/1 and GY-GIR/0.
- Status: Draft. Spec: `docs/graphics/gva.md`.
- Provides:
  - `encode(&Drawing, &GvaLimits)`;
  - `Gva::open`, with zero-copy instance arrays (`instances()`), lazy
    `geometry(g)`, `instances_intersecting(rect)`, `to_drawing()` and
    `verify()`.
- Invariants:
  - Geometry and styles are deduplicated by structure and pooled in
    first-use order, with origin folding applied when it is exact.
  - Encoding is deterministic and idempotent: encoding a decoded file
    reproduces it exactly.
  - `open` decodes no geometry. It validates every section, every
    reference and every limit.
  - `verify` rejects any file that is not canonical.
- Golden fixtures: `fixtures/graphics/gva/icon.gva`, plus the render
  digest in `icon.x4.sha256`.

## GY-GVI/0 — Graphical Vector Inventory (`crates/goya-gvi`, GOYA-GFX-001 Phase 3)

- Consumes: GY-GFXC/1, GY-GIR/0 and GY-GVA/0 (shared pool codecs).
- Status: Draft. Spec: `docs/graphics/gvi.md`.
- Provides:
  - `StateModel` (dimensions, closed effect set including `Morph`, product
    or transition reachability), `assess` (KFBM), `compile`,
    `compile_with(…, impl Into<CompileOptions>)` (bake policy and
    differential depth), `cache_key(…, options)`, `morph_path`, and
    `StateModel::binding`;
  - `Inventory::open(bytes, &ParentBinding, &GviLimits)`, plus `state`,
    `resolve`, `permits`, `decode_geometry`, `verify_against`, and
    `Projection` (zero-copy rows that carry source item ids).
- Invariants:
  - Compilation is deterministic, and partial inventories are never
    emitted. Ineligible models fail with E5001–E5004 and a recommended
    alternative.
  - `open` rejects a foreign parent binding (E5007), verifies certificate
    figures, and re-derives completeness independently (product check, or
    closure plus reachability within the index).
  - `resolve` is validation plus binary search. Invalid states (E5005) and
    unreachable states (E5006) are rejected, never substituted.
  - Morphs are integer-exact. Baked rows carry only a translation. Strokes
    bake only under similarity transforms. Baking drift is at most 1
    premultiplied level against the unbaked inventory.
  - Differential geometry (GVI-D) is exact and hash-verified. Records
    reference earlier ids only, and their depth and ancestor counts are
    declared in `COST` and re-derived at open. Records are rebuilt once
    at load, never per lookup. The differential layout is emitted only
    when it is smaller.
  - `PROJ` holds a pool of distinct rows plus a row-id list (omitted when
    it is the identity), all in canonical bit-packed columns (frame of
    reference or dictionary). Every field read is O(1), and every column
    is re-encoded and compared at open.
  - `INDX` stores keys as identity, a rank bitmap or a packed sorted list,
    whichever is smallest. Projection ids are identity or a packed column.
    Lookup is O(1) or O(log n), the section is canonical (re-encoded at
    open), and the coverage digest is independent of the form.
  - `Inventory::set_residency` (`Packed` or `Expanded`) changes speed and
    memory only, never results.
  - GVI-A (`CompileOptions::albit`, opt-in) adds `Lerp` and `BakedLerp`
    records to `DIFF`. The profile is recorded in `COST` byte 24, and
    candidates are provenance-directed. Output is never bigger than the
    same compilation without GVI-A; when GVI-A brings nothing, it is
    byte-identical. See `docs/graphics/gvi-albit.md`.
  - GVI-O (`CompileOptions::operative`, opt-in) stores operative spans
    (`CONST`, `LINE`, `RATIO`, `NUMBERED`, literal) for `STAT` (with
    `REF` morph targets), `DIFF` (columnar) and `PROJ` (column mode 2).
    The parent binding stays over the canonical literal model, and every
    per-shape hash is kept. Never bigger, and byte-identical when nothing
    helps. See `docs/graphics/gvi-operative.md`.
  - GVI-L stores the style pool as `LSTY` when that payload is strictly
    shorter than `STYL`, and sets `CERT` bit 4 exactly then. `open`
    requires the matching section and no other style section, re-encodes
    `LSTY`, and rejects a mismatch. Decoded styles match the `STYL`
    pool. See `docs/graphics/gvi-lineless.md`.
- Golden fixture: `fixtures/graphics/gvi/status_icon.gvi`.

## GY-GRU/0 — Graphical Rendering Unit (`crates/goya-gru`, GOYA-GFX-001 Phase 2)

- Consumes: GY-GFXC/1, GY-GIR/0 and GY-GVA/0 (pool codecs).
- Status: Draft. Spec: `docs/graphics/gru.md`.
- Provides:
  - authoring types: `UnitSource`, `Slot`/`SlotType`/`SlotValue`,
    `Binding`/`Target`/`Map`/`Value`, `Region`/`Role`, and `encode`;
  - loading: `Unit::open` and `Unit::instantiate`;
  - runtime (`UnitState`): `set`, `set_named`, `take_invalidation`,
    `item`, `drawing`, `instance_bounds`, `hit_test`, `focus_order` and
    `focus_next`.
- Invariants:
  - Encoding and loading share one type checker.
  - There is at most one binding per (instance, channel), and every map
    is O(1).
  - `CHNG` and `SPAT` are re-derived and verified at load.
  - Geometry is decoded once at load and never rebuilt.
  - `set` validates the value, touches only the instances `CHNG` lists
    for that slot, and is atomic.
  - Hit-testing and focus use geometry, never pixels.
- Golden fixture: `fixtures/graphics/gru/dashboard.gru`.

---

# GY-FAIR/2 — Foreign Application Intermediate Representation (`crates/goya-fair`)

Status: Candidate (Disparates programme, extraction-only authority). It
supersedes GY-FAIR/1 (owner approval, 26 November 2026). Full contract:
[docs/disparates/contracts/fair-2.md](disparates/contracts/fair-2.md); register:
[reason-codes.md](disparates/contracts/reason-codes.md); previous revision:
[fair-1.md](disparates/contracts/fair-1.md); programme roadmap:
[docs/disparates/roadmap.md](disparates/roadmap.md).

- **Provides**: `FairDocument` (manifest, extractor, `project`, elements,
  `relationships`), `FairElement` (kind, provenance, representability, open
  semantics payload), `FairRelationship` (subject, `Relation`, object, `Basis`,
  evidence, provenance), `FairProject` (entry points, framework detections),
  `Representability` (frozen seven-state evidence progression: extracted /
  representable / translated / verified / remediation_required /
  unrepresentable / undetermined), `REASON_CODES` / `is_registered`, `validate`,
  `upgrade` (FAIR/1 → FAIR/2), `canonical_json`, `coverage_summary`.
  `FAIR_CONTRACT` is `GY-FAIR/2`; `FAIR_1_CONTRACT` is `GY-FAIR/1`.
- **Consumes**: none. Serde-only; no `goya-ir` types. The FAIR→Goya boundary is a
  future contract; reconstruction authorisation is withheld.
- **Consumers**: `crates/goya-disparate-electron` (emits FAIR/2).
- **Invariants**: FAIR preserves ignorance — unresolved constructs are recorded,
  never dropped; every non-`extracted` element carries a reason code, which under
  FAIR/2 must be registered; relationships point at existing elements, and
  derived ones name their evidence; there is no `inferred` basis; canonical
  serialisation is byte-stable (elements and relationships sorted by ID, sorted
  map keys, empty FAIR/2 fields omitted, so FAIR/1 bytes are unchanged);
  extraction is deterministic, offline and static; `verified` is unreachable
  under this revision and compilation alone never implies equivalence.
- **Stability**: the element-kind vocabulary, relation vocabulary and
  reason-code register grow by contract revision only and are never renamed.
  FAIR/1 documents remain readable.

## Ownership
| Path | Owner |
|---|---|
| crates/goya-fair, docs/disparates | Disparates programme |
| crates/goya-disparate-electron, fixtures/disparates | Disparates programme (Electron extractor, DEL-TD-001) |

---

# GY-HTML-TOKENS/1 — HTML tokenizer (`crates/goya-html-tokenizer`)

Status: Candidate (26 November 2026). It was built to replace html5ever in
`goya-disparate-electron`, on the owner's decision.

- **Provides**: `Tokenizer` (`next_token`, `set_state`, `set_last_start_tag`,
  `html_content_model`, `cdata_allowed`, `errors`), `tokenize_document`, `Token`
  (doctype, start tag, end tag, comment, processing instruction, text),
  `Attribute` (with name and value spans), `Span`, `ParseError` (HTML Standard
  code and byte offset).
- **Consumes**: none (no dependencies).
- **Consumers**: `crates/goya-disparate-electron`.
- **Invariants**: follows the HTML Standard §13.2.5 tokenizer, including the
  processing-instruction states; spans are byte offsets into the original
  input; CR LF and CR read as LF; no tree construction. Conformance: every
  html5lib-tests tokenizer case (7,042 runs), for both tokens and error codes;
  4 lone-surrogate cases are skipped.

# GY-CSS-SYNTAX/1 — CSS Syntax Level 3 (`crates/goya-css-syntax`)

Status: Candidate (26 November 2026). It was built to replace cssparser
(MPL-2.0) in `goya-disparate-electron`, on the owner's decision.

- **Provides**: `tokenize`, `parse_component_values`, `parse_stylesheet`,
  `parse_rule_list`, `parse_declaration_list`, `block_declarations`,
  `parse_one_rule`, `parse_one_declaration`, `parse_one_component_value`;
  `Token`, `ComponentValue`, `Block`, `Rule`, `AtRule`, `QualifiedRule`,
  `Declaration`, `DeclarationItem`, `Span`.
- **Consumes**: none (no dependencies).
- **Consumers**: `crates/goya-disparate-electron`.
- **Invariants**: follows the 2021 CR draft of CSS Syntax Level 3; spans are
  byte offsets into the original input; input the specification discards is
  returned as `Invalid` with its span, never silently dropped. Conformance:
  all 136 cases of seven css-parsing-tests files. CSS Nesting block contents
  are not implemented.

| Path | Owner |
|---|---|
| crates/goya-html-tokenizer, crates/goya-css-syntax | Disparates programme (shared Goya libraries) |

# GY-CHROMATICS/0.1 — Chromatics core (`crates/goya-chromatics`)

Status: Draft (26 November 2026), not ratified. Directive GOYA-INT-CHR-001,
Phase 1 and the logic of Phase 2, plus the §12 language surface. See
`docs/chromatics/CHROMATICS-0.md` and `CHROMATICS-1.md`. Unrelated to
GY-CHROMA/1 (colour themes) and goya-cue (colour cueing).

- **Provides**: `Engine::allocate` (eligibility → ordering → Chromatic
  Inertia → placement → verification), `verify`, `Registry` (staged and
  committed generations, `press`/`release` dispatch, `revoke`), `Scopes`
  (nested scopes; a scope may declare that it inherits its parent's
  candidate pool — `compose`, §6), `Candidate`/`Binding` accessibility
  metadata (`symbol`, `description`, §21) with an `Unlabelled` publication
  refusal, and `fixture::run` for the shared conformance scripts.
- **Consumes**: none (no dependencies, no clock, no I/O).
- **Consumers**: `crates/goya-lang` (authoring surface
  GY-CHROMATICS-LANG/0.1-draft above: strategy and risk keywords and the
  canonical `allocation_policy`/`allocation_candidates` lowering).
  Arachnid and the resolver are the intended further consumers, through a
  generated runtime that passes the same fixtures.
- **Invariants**: allocation is a pure function of capability, intents,
  policy, candidates, trigger, tick and the context's previous generation;
  candidate order never matters; ties break by candidate id; all arithmetic
  is integer. A generation that fails verification is not recorded. A press
  executes the intent it had when the key went down, or is refused; it is
  never redirected. A binding without a text label is never published.
  Conformance: `fixtures/chromatics/*.chr` (14 fixtures, including §6
  declared inheritance and §21 identification).
