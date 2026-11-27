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
