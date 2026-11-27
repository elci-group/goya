# ADR-AVC-001 — Adopt Ahead-of-Time Video Compilation (AOT-VC) as a temporal derivation family over GVI-R

Status: Proposed (Phase 0 audit complete) | Date: 2026-11-27 | Owner: Goya Graphics | Directive: GOYA-GFX-AVC-001 | Supersedes: none

## Context

GOYA-GFX-AVC-001 proposes distributing video as a **precomputed timeline of addressable visual states** synchronised against a media clock, rather than as a compressed sample sequence: source video is decomposed at canonical compilation time into a deduplicated colour inventory, a pixel-state inventory (fill colour, 0–6 trim colours, independent light), a row/region inventory, and a temporal program of state-mutation instructions that a lightweight runtime resolves, dispatches and presents. The proposal positions this as Ahead-of-Time Video Compilation (AOT-VC) — video as a first-class participant in Goya's graphics infrastructure rather than an opaque multimedia surface. The directive requires Phase 0 to reconcile its claims with existing Goya contracts before implementation. This ADR records that reconciliation.

## Audit findings

### F1 — Most of the architecture already exists in GVI; the temporal dimension is the only genuinely new surface

| Proposal element | Existing Goya artefact |
| --- | --- |
| Persistent, addressable visual states | `crates/goya-gvi` contract `GY-GVI/0` (`model.rs`, `index.rs`): projection inventory over a discrete parent state model, runtime cost `validation + lookup + presentation` |
| `APPLY_DELTA` / differential events | `goya-gvi/src/diff.rs`: differential chains with bounded reconstruction depth (`max_depth`, `max_ancestors`) and the `DIFF` container tag |
| `LOAD_STATE` checkpoints | GVI certification (`CERT`), content hashing (`HASH`), `COMPILER_ID` cache keys |
| Exact vs approximate reconstruction | GVI-A (`operative.rs`, Albit/Operative profiles) — but the explicit `exact` provenance bit **does not exist yet** (AVR audit F4 deferred it to Phase 3) |
| `SET_RUN` / row and region residency | `goya-gvi/src/packed.rs` and GVI-L (`lineless.rs`) |
| Target compilation profiles (embedded → archival → interactive) | Diablos pipeline (`goya-diablos/src/graphics`) + `goya-caprichios` device/strategy taxonomy |
| Provenance for per-target binaries | `.escene` producer contract (`goya-escene::producer`) |
| Geometry/style/colour deduplication | GVA geometry pool and the `STYL`/`LSTY` shared palette |

Consequence: AOT-VC is not a new subsystem adjacent to GVI. It is a **temporal derivation family over GVI-R**, in the same position AVR occupies as a spatial one. The canonical inventory model, budgets, certification and hashing machinery are reused, not reinvented.

### F2 — State explosion is the Critical risk, and the proposal's own bandwidth model is the evidence

The proposal's §5 payload model (up to 99.53 MB/s uncompressed at 10% pixel updates, 1080p/30) is the finite-state problem restated: per-pixel video colour is a **continuous domain**, and GVI's state model is finite and enumerable. The AVR audit already classified observation-state explosion as Critical and resolved it with the bounded-computation rule: continuous domains are discretised, approximated under declared tolerance, hybrid-resolved, or **rejected with an actionable diagnostic**. AOT-VC adopts that rule verbatim as the **colour-inventory closure rule**: for a given source, the compiler must either close the colour inventory under an explicitly bounded palette and declared tolerance, or fail closed — a photographic source that cannot close is routed to a conventional codec, never silently palette-crushed. The proposal's §10 target taxonomy is thereby made contractual: screen recordings, UI capture, pixel art and motion graphics are the sources for which closure is expected; photorealistic cinema is the source for which rejection is the expected deterministic outcome.

### F3 — "Zero unexplained pixel differences in lossless mode" conflicts with trim-assisted reconstruction unless exactness is per-record from Phase 0

Trim colours are derived interpolation anchors (the proposal's own §7 concedes they contain no independent source information). Every pixel reconstructed through trim-assisted interpolation is therefore **approximate by construction**. The proposal's acceptance target of zero unexplained pixel differences in lossless mode can only hold for fill-only records. Consequence: the GVI-A `exact` bit (AVR F4) lands in AOT-VC **Phase 0**, not Phase 3. Every pixel-state, row, region and checkpoint record carries an explicit exactness class; trim-assisted records are never certified exact; the `PRESENT` compositor reports the exactness coverage of each presented frame; and no acceptance claim of losslessness may be made per-frame without exact coverage = 100%.

### F4 — Audio is absent from the proposal while its acceptance table demands ±20 ms synchronisation

The proposed instruction set contains `SYNC`/`PRESENT` but no audio pipeline. Goya's audio stack (`goya-sonic`, `goya-spatial-audio`, `goya-audio-dsp`) is out of scope for visual compilation. Decision: **audio stays as an opaque, conventionally-coded sidecar**; the media clock is the only contract between the visual timeline and the audio track; AOT-VC never decodes, transforms or emits audio samples. A container descriptor section announces the sidecar's codec, duration and initial PTS offset so a player can honour ±20 ms without the visual contract knowing anything else about audio.

### F5 — Timing must be fixed-point, not floating-point (the AVR F1 lesson applied to time)

AVR found Rugid's `f32` spatial pipeline forces a reproducibility split between derivation and rendering. AOT-VC's media clock is created new, so the lesson is applied at design time: all timeline timestamps are **integer ticks over a rational timescale** (`u64` ticks + `u32` ticks-per-second, e.g. 30_000/1001 style NTSC rates representable exactly). No `f32`/`f64` timestamps enter the contract. This makes byte-identical compilation, checkpoint digests and `SYNC` ordering deterministic on every host.

### F6 — Binary section tags: collision review (mirrors AVR F3)

Existing container tags at audit time: GVI's `CERT, COST, DIFF, GEOM, HASH, INDX, LSTY, PARN, PROJ, SRCE, STAT, STYL, TRAN` and AVR's proposed `OBSV, ODOM, OREP, ANCH, NBRH, NAVG, RDEP, RSDY, FIDY, PROV`. Proposed AOT-VC sections `TIML, EVNT, CHKP, PALT, PIXS, ROWS, REGN, TRIM, EXCT, AUDS`: **no collision** with any existing or proposed tag. All are `A–Z`/`0–9` four-byte tags per GY-GFXC/1 E1014. The review is encoded as an executable test in `crates/goya-avc` (`tags.rs`) that checks against the union of the GVI and AVR tag sets so it cannot rot. Final adoption is gated on Phase 2 and `.gvi` format version negotiation (§13.2 of GOYA-GFX-AVR-001), unchanged.

### F7 — Namespace separation now spans three state spaces

Per AVR F2, GVI variation states and AVR observation states must never share a namespace. AOT-VC adds a third: **temporal states** (frame-time-indexed visual checkpoints). The §11 ambition of a unified visual architecture is satisfied at the *inventory/container* level — one provenance model, one derivation pipeline, one `.escene` channel — not at the identifier level. `TemporalStateId`s never collide with GVI state ids or AVR observation ids, and each family carries its own directive tag in provenance records.

### F8 — Trim topology requires three rules the proposal states but no contract yet enforces

The proposal's §7 rules are adopted as normative contract types, not conventions: (1) **shared boundaries** — adjacent logical pixels reference the same trim definition for a shared edge, enforced structurally at inventory construction; (2) **linear-light averaging** — trim values are averages computed in a declared linear working space; no existing GVI colour document fixes a working space (audited: `gvi-lineless.md`, `colour-cue.md` are silent), so the AOT-VC working space is a new, single decision recorded in this ADR's decision section and changed only by ADR supersession; (3) **optional residency** — trim storage is permitted only where a boundary is non-uniform; solid regions store no trims, enforced by the row compiler contract.

### F9 — Compilation profiles never carry default budgets

The proposal's §9 profile table (embedded/mobile/TV/desktop/archival/interactive) maps onto Caprichios (AVR F6) and Diablos derivation (AVR F7). As with AVR decision 2, numeric budgets are illustrative until benchmarked: the AOT-VC compilation profile type has **no `Default` implementation**; every profile states its colour-inventory bound, checkpoint interval, reconstruction depth and residency budgets explicitly.

## Decision

1. **Adopt** AOT-VC as a temporal derivation family over GVI-R, per the reconciliations above. The proposal's logical model — media clock, timeline of checkpoints and events, pixel-state record (fill, 0–6 trims, light), hierarchical inventory, instruction classes — is accepted as the Phase-0 logical contract, implemented in a new `crates/goya-avc` (workspace member, `#![forbid(unsafe_code)]`, no runtime deps).
2. **The bounded-computation rule governs closure.** Colour-inventory closure (F2) is a Phase-1 gate: the compiler either closes the inventory under declared bounds and tolerance or fails with an actionable diagnostic naming the un-closable region. No implicit degradation path exists.
3. **Exactness is per-record from Phase 0** (F3): the `Exactness` class exists in the logical contract before any binary format does, and trim-assisted reconstruction is classified approximate by construction.
4. **Audio is a sidecar** (F4): the media clock is the only visual↔audio contract; `AUDS` is a descriptor, not a sample container.
5. **The media clock is fixed-point** (F5): `u64` ticks over a `u32` ticks-per-second rational; floating-point timestamps are rejected at the contract boundary.
6. **The trim working space is scene-linear sRGB-derived (linear Rec. 709 primaries)**, with trim values stored as fixed-point linear-light quantities; averaging in any other space is a contract violation (F8).
7. **Compatibility gates carried forward** from AVR decision 5: existing `.gvi` files remain readable; GVI colour semantics unchanged; Inquisition classes apply to every emitted inventory; binary format work deferred to Phase 2 behind the tag review test and version negotiation.
8. **Phase 1 gate.** Canonical video compilation (frame decomposition, row compilation, temporal optimisation) begins only after the source-ingestion contract exists: which decoded frame formats are accepted, the decode→working-space conversion rule, and the closure parameters for the colour inventory. The first proof-of-concept target is a **screen-recording source**, where state closure is trivially achievable and the persistence model shows its largest win.

## Consequences

- `crates/goya-avc` is added as the contract home for AOT-VC (Phase 0 scope only: media clock, temporal model, pixel-state records, colour-inventory closure, instruction classes, trim topology, exactness classes, compilation profiles, tags, verification classes).
- The GVI-A `exact` bit (AVR F4) is now scheduled for the AOT-VC contract first; when it lands in `goya-gvi`, the two crates share one provenance semantics.
- `.escene` gains `C.VID_*` / `I.VID_*` / `S.VID_*` registry entries at Phase 2, following the AVR F5 pattern (annotations register through the existing producer contract, never a parallel channel).
- Benchmarking baselines (mirroring AVR §18) must include: the existing flat-GVI lookup path, H.264/AV1 software decode, and — on targets that have it — hardware decode. Energy claims are made only from measurements on physical targets (the proposal's own §9 caveat, made contractual).
- A video runtime that executes `TIML`/`EVNT`/`CHKP` programs does not exist and is **out of Phase-0 scope**; the contract types are written so a runtime can be built against them without renegotiation.

## Risks

| Risk | Severity | Audit adjustment |
| --- | --- | --- |
| State explosion on photographic sources | Critical | F2 — closure rule fails closed; rejection is the deterministic outcome for un-closable sources |
| Approximate reconstruction silently certified exact | High | F3 — per-record exactness from Phase 0; `PRESENT` reports coverage |
| Storage competitiveness vs H.264/AV1 unproven | High | Accepted: AOT-VC competes on runtime cost, inspectability and addressability, not bitrate; §8 of the proposal concedes this and Phase 2 benchmarks quantify it |
| Trim interpolation darkening boundaries (gamma-space averaging) | Medium | F8 — linear working space fixed by decision 6 |
| Timeline/audio drift on non-integer frame rates | Medium | F5 — rational timescale represents NTSC rates exactly |
| New runtime surface (schedule/resolve/dispatch/present) duplicating GVI presentation | Medium | F1 — runtime consumes the shared inventory contract; presentation cost model is GVI's internal baseline |

## Amendment 1 — source-ingestion contract exists; Phase-1 gate satisfied (contract level)

`crates/goya-avc/src/ingestion.rs` implements decision 8's gate as executable contract, following the AVR `SpatialSource` pattern (ADR-AVR-001 F1):

- **Accepted formats.** Byte-packed RGB-family frames (`Rgb8`, `Rgba8`, `Bgra8`). YUV-family decode output is rejected at the contract boundary with an actionable hint naming the required conversion: chroma resampling and matrix decisions belong to a decoder adapter, not to the visual-state contract. Malformed frame payloads (length vs declared geometry) are rejected structurally.
- **Conversion rule.** sRGB EOTF decoded to scene-linear Rec. 709 (decision 6's working space) and quantised to Q24.8 with round-half-to-even (the GVI arithmetic convention). This is the contract's one floating-point stage, declared like AVR's f32 lesson: byte-identical output is claimable per host build, cross-host is tolerance-class.
- **Closure parameters.** `IngestionParameters` (working space + media-clock timescale) binds explicitly; no `Default` (AVC-C09).

`crates/goya-avc/src/compile.rs` implements the reference compilation sequence as executable contract: decode → working-space conversion → colour closure (fail-closed with the F2 diagnostic) → row decomposition and first-use-order deduplication → temporal optimisation (differential `SET_ROW` events over persistent state, per-frame positions at exact tick multiples) → interval-bounded `LOAD_STATE` checkpoints → `PRESENT` → budget validation. It also records the measurement the §5 bandwidth model needs: `CompileStats` carries naive vs persistent pixel-write counts for benchmark baselines.

Proof-of-concept evidence (deterministic synthetic screen recording, 8×4, 3 frames, 3 colours): the persistent-state model issues 56 pixel writes against a naive 96 (the §5 baseline), the timeline compiles deterministically (byte-identical repeat builds), and an over-bound source fails closed with the remediation diagnostic. One quantisation finding: the sRGB EOTF maps near-black levels onto the same Q24.8 value (byte 1 → 0), so closure operates post-quantisation by design — palette counts are canonical-entry counts, not source-byte counts.

Gate status: the Phase-1 gate is satisfied **at the contract level**. Canonical compilation of real sources additionally requires a decoder adapter for the MP4/MKV/WebM/MOV container families (process boundary, mirroring `goya-tov`'s adapter pattern); that adapter is Phase-1 implementation work, not a contract renegotiation.

## Amendment 2 — decoder adapter exists (`goya-avc-decode`); audio sidecar descriptor implemented

`crates/goya-avc-decode` implements the decoder adapter as a **process boundary around the ffmpeg/ffprobe CLI** (the `goya-tov` pattern, contract `avc-decode/0.1.0`): every path and filter expression is passed as its own OS argument, never through a shell; the tools are external and never vendored; absence is `ToolUnavailable` with the attempted path. The adapter's sole conversion responsibility — chroma and format normalisation — happens *inside the decoder process*: frames are always delivered as rawvideo RGB8, the family the ingestion contract accepts, so the visual-state contract never sees YUV or container semantics (decision 4, Amendment 1).

Two contract completions accompany it:

- **Audio sidecar descriptor (decision 4).** `AudioSidecar { codec, duration_ts, time_base, start_pts }` is the `AUDS` section's logical form: opaque codec name, integer duration and start timestamp in the stream's own time base — no floating-point timestamps enter even the sidecar descriptor (F5's lesson applied fully). A silent container yields no descriptor.
- **Frame-count fallback.** Containers without `nb_frames` (many MKV/WebM) are probed with `-count_packets` and fall back to `nb_read_packets`; sources with neither are rejected as `UnsupportedSource` with the reason attached.

The full pipeline is exercised end-to-end (`tests/container_round_trip.rs`): a deterministic rawvideo fixture is encoded to lossless FFV1 MKV, probed, decoded frame-by-frame through the adapter, and compiled through the reference pipeline — closure sees exactly the fixture's three colours, the persistent-state write counts hold, and repeat decode+compile runs are identical. The test self-skips on hosts without ffmpeg (it is never vendored), so the real-container round-trip must be re-executed on a host with the tools present; the hermetic surface (probe parsing, rational rates, filtergraph argument construction, tool-absence handling) is verified everywhere.

Adapter scope remains deliberately narrow: per-frame `select=eq(n,i)` decoding is correct-but-slow and adequate for canonical compilation, which is an offline, compile-time cost. Batched or seeking decode strategies are runtime performance work, gated on the §18 benchmarking programme, and change nothing in this contract.

## Amendment 3 — temporal optimisation quality: run packing, content-keyed deltas, light deferral

The reference compiler's temporal pass is upgraded from whole-row declarations to three deterministic rules, all within the existing instruction contract:

1. **Run packing (`SET_RUN`).** Within-row changes are grouped into maximal contiguous same-colour runs; each run is one event. Only a whole-row change, or a row fragmenting into **three or more runs**, is materialised as one whole-row `SET_ROW` — the fragment threshold is a fixed, deterministic rule, not a heuristic knob.
2. **Content-keyed deltas (`APPLY_DELTA`, `delta.rs`).** A row transition is identified by its *content* pair (`ContentId` → `ContentId`), not by row position or frame index. The first occurrence materialises and registers a `RowDelta` (ascending, deduplicated pixel changes; precondition = current content; empty deltas never registered); every later occurrence of the same content transition — on **any row** — applies the delta by id. Delta reuse is therefore position-independent, and preconditions are checkable by a runtime that tracks row content: applying against unmatched content is a contract violation (AVC-C13), never a best-effort patch. Reference deltas are **depth-1 only** (precondition is concrete content, never another delta), so reconstruction cannot chain; production chaining is bounded by `MAX_DELTA_DEPTH` and remains Phase-2 work. `CompileStats` now breaks events down by kind (`row_events`/`run_events`/`delta_events`) with pixel-write accounting for each.
3. **Light separation is deferred, deliberately.** Representing a pixel as (chroma index, light value) would change `ColourInventory` semantics — entries would become unit-luminance chroma, and the closure rule would operate on chroma, not colour. It also interacts with the near-black quantisation finding of Amendment 1: exact light-factor matching against byte-quantised source colours is fragile (sRGB byte 188 does not land exactly on half of byte 255 in Q24.8). Light-only optimisation therefore requires its own ADR amendment defining the chroma/light decomposition, the exactness consequences (which round-trips are `Exact`), and closure-on-chroma. `SET_L` stays contract-present but unexercised by the reference compiler.

Evidence: the screen-recording POC now compiles its three frames as 4 `SET_ROW` + 2 `SET_RUN` + 1 `APPLY_DELTA` (35 persistent pixel writes against a naive 96), with the enter-transition delta registered once and applied on a *different row* in the final frame — demonstrating position-independent delta reuse. Scattered-change and equal-content paths are unit-tested, and compilation remains byte-identical on repeat runs.

## Amendment 4 — reference playback runtime; per-frame `PRESENT`; the runtime found a contract bug

`crates/goya-avc/src/runtime.rs` implements the reference interpreter: **resolve** (inventory references), **dispatch** (persistent-state mutations), **restore** (checkpoints), **present** (atomic snapshots with per-pixel exactness). Its declared subset is the fill-only path (`SET_ROW`, `SET_RUN`, `APPLY_DELTA`, `LOAD_STATE`, `SYNC`, `PRESENT`); trim and light instructions are rejected *explicitly*, never silently ignored — a runtime that pretended to apply them would break determinism. Delta application checks the target row's current content against the delta precondition by value; mismatch is `DeltaPrecondition` (AVC-C13).

Three contract-level findings came out of making the timeline executable:

1. **`PRESENT` is per-frame.** The compiler previously emitted a single terminal `PRESENT`; the directive's `PRESENT` ("atomically present the completed visual state") is a per-frame event — each frame's state is presented, which is what makes the timeline a playback program rather than a final-state dump. Fixed in the compiler; every frame now presents at its exact tick.
2. **`APPLY_DELTA` needed an explicit row target.** The instruction originally carried only `delta_id`; with content-keyed deltas, several rows can legitimately hold the same content, so the runtime cannot infer the target — inferring it would corrupt sibling rows. `ApplyDelta { row, delta_id }` now targets one row, and the precondition check is per-row. This is exactly the class of bug the runtime exists to catch: the contract was underspecified until something executed it.
3. **Checkpoint payloads are runtime-loadable.** `Checkpoint` now retains the payload its digest was computed over (`payload_bytes`, `from_parts`, `digest_is_intact` for the Phase-2 deserialisation path), and the payload encoding (row-content ids, LE, ascending rows) lives in the runtime module and is *imported by the compiler*, so the format cannot drift.

Verification achieved: **the playback-correctness acceptance target is now demonstrated at POC scale** — the runtime reconstructs every presented frame of the screen-recording POC and each pixel is diffed against the source: zero unexplained differences, every pixel `Exact` (lossless mode). Replay from a checkpoint reproduces the full replay's final state, playback is deterministic (identical `PresentedFrame` vectors on repeat runs), precondition violations and unsupported instructions are rejected explicitly, and present-with-undeclared-rows is an error rather than a partial frame.
