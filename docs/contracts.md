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
- Protocol: `begin()` → `set()`* → `commit(invariant)`; drop without commit =
  rollback. Single-writer enforced (`begin` returns `None` if a transaction
  is open). Committed snapshot is never mutated in place; commit swaps the
  staged copy atomically and bumps `revision()` only on success.
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
- Invariants: events are processed synchronously; layout is recomputed only
  when the state revision advanced; unknown actions are inert.

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
| EDC canonical lowering + D0 bytecode/reference VM + D1/D2 journals | 2c (implemented, partial) | Authored Goya source lowers through bounded typed single-button D1 and static-journal D2 subsets; a loopback live smoke submitted both typed source-compiled worker requests and verified their results. Broader language features remain open. D1/D2 canonical decoding and VEE workers are available |
| VEE submission API + bounded D0 data bridge | 3a (implemented, partial) | D0/D1/D2 workers and authored typed D1/D2 requests passed local loopback submit/status/result E2E; VEE's D0 stdout artifact may prefix sandbox diagnostics; local Landlock was unavailable, so live smokes used rlimit isolation only |
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
partial state. Callers can stage the returned snapshot in their state
transaction and activate the replacement only after that transaction succeeds.

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
  `scroll: true` makes that node's text viewport vertically and horizontally
  scrollable. `wrap` remains an explicit deterministic column-wrap override.
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
  returned state. Broader source-language features remain open. Live sandbox
  evidence is limited to this environment's rlimit fallback because Landlock
  setup returned `Operation not permitted`.
- The Wave-1 evidence table row for Vico Vee is superseded by this section.

---

## Wave 2a/b additions (2026-11-04, afternoon wave)

### GY-WAYLAND/2 keyboard layout support
- The client consumes compositor-provided XKB_V1 keymaps up to 16 MiB and
  updates XKB state from Wayland modifier/layout masks. Evdev key events are
  translated through that state to Unicode `KeyChar` events, including
  Shift/Caps Lock and layouts such as German AltGr. Unsupported or malformed
  keymaps retain the bounded US fallback. Dead-key compose sequences are not
  implemented. Tests cover US Shift/Caps Lock, German AltGr, and fallback.

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
- P1 typography stack (shaping/rasterisation) — Wave 4-scale dependency decision.
- GY-IR/2 model unification — Wave 2c.
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
