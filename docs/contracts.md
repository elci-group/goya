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
GY-WAYLAND-001 (open): Wayland surface adapter, consumes GY-RUNTIME/1
GY-TEST-001 (open): conformance suite + fuzzed parser (assurance swarm)
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
Deterministic decaying-sine `Tap`, priority-evicting `Mixer` (lower numeric = more urgent; graceful overload), hard + smoothed `Limiter`. Linux layer is the device lifecycle contract with `NullOutput` (missing device keeps the app operational); PipeWire C binding is a future adapter.

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
| PipeWire audio adapter (`goya-audio-linux` backend) | 2a | real ARSEM output |
| Hot-swap supervisor + Tov adapter | 2b | release 0.2 |
| EDC: canonical IR lowering + bytecode backend | 2c | compiled execution units |
| Vico Vee library extraction | 3a | containment contracts |
| RUGID / 3form / Mesut / Theosis adapters | 3b | release 0.3–0.4 |
| C2PA manifest compatibility (provenance signing) | 3c | artifact provenance |
| HRTF / multichannel elevation cues | 3d | beyond-stereo ARSEM |
| RPI Level-2 learned anticipation (opt-in) | 3e | anticipatory runtime |
| Accessibility conformance suite (WCAG, screen readers) | 4 | release 0.6 |
| Packaging, SDK, Studio, distribution | 4 | release 0.6–1.0 |
| Adversarial robustness corpus for watermark detection | 4 | no robustness claim without corpus |

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
  `stroke-width`, `text`, `text-size`, `text-anchor: start|middle|end`.
- Forward compatibility: `@template` and `@interaction` blocks are parsed
  and skipped (visual state work is Wave 3); unknown properties are
  consumed and ignored, never fatal.
- Totality: parse + layout never panic on any input — 10,000-input
  deterministic mutation fuzzer enforces it.
- Hit testing: `hit_test(&Frame, x_mp, y_mp) -> Option<button-name>` with
  depth-first later-drawn-wins order.
- Demo: `cargo run --bin scene [--file app.gscene] [--live]` builds a
  clickable GUI from a scene file.

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
- Wave 3a changes from "Vico Vee library extraction" to **"VEE adapter:
  route Goya EDC executable units and agent tool calls through VEE's
  capability/budget/artifact machinery"** — a much smaller, better-fitting
  package. VEE is a strong candidate to *be* the 0.3 "Contained Modules"
  enforcement point rather than a peer to integrate.
- The Wave-1 evidence table row for Vico Vee is superseded by this section.

---

## Wave 2a/b additions (2026-11-04, afternoon wave)

### GY-WAYLAND/1 amendments — keyboard & resize
- `PlatformEvent::KeyChar(char)`: `wl_keyboard` wired with an evdev US
  QWERTY subset table (letters, digits + shifted symbols, space, enter,
  backspace, punctuation). The xkb keymap fd is accepted and closed — the
  built-in table is the declared scope; full keymap/IME support is Wave 4.
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
- 6 tests. (Tile-parallel raster execution lands with the GPU/tile
  renderer backends; this crate is the deterministic scheduler contract.)

## GY-HCAR/1 — Heterogeneous core classification (`crates/goya-hcar`)
- Per-work-item classification (never per quadrant permanently, §2):
  E-core deadline violation or critical-path ⇒ P-core mandatory;
  otherwise energy-minimising choice among feasible classes (§3:
  min Σ E subject to deadlines).
- Homogeneous CPUs degrade to `General` (§6).
- `Residency` migration hysteresis (§6): class changes require N
  consecutive confirmations; seeded on first observation.
- Affinity modes: OsManaged default / Advisory / Controlled (§7).
- Temporal persistence expressed in tests (§4): cheap-but-critical scroll
  tiles → P; relaxed status indicators → E.
- 8 tests.

## Integration (demonstrated in `goya-calculator`)
- LCPE: envelopes compiled from resolved scene geometry per generation;
  presses resolve → quantised dirty regions → cost decision →
  `present_dirty` partial damage to the compositor.
- RARS: glyph fidelity resolved from the projected window vs 4K
  (3.28% → balanced quality, protected-text glyph 4px).
- Pipeline status: LCPE ✓ RARS ✓ CSR scheduler ✓ HCAR classifier ✓ —
  the remaining integration (tile-parallel raster with HCAR affinity)
  is Wave-2c work alongside the GPU backend decision.

## Ownership
| Path | Owner |
|---|---|
| crates/goya-change | Efficiency Swarm M1 (LCPE) |
| crates/goya-rars | Efficiency Swarm M2 (RARS) |
| crates/goya-csr | Efficiency Swarm M3 (CSR) |
| crates/goya-hcar | Efficiency Swarm M4 (HCAR) |
