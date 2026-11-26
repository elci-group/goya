# Goya graphics infrastructure (GOYA-GFX-001)

Directive GOYA-GFX-001 sets out four native graphics formats that share one
envelope and one compiler lowering path:

| Format | Name | Role |
|---|---|---|
| `.gru` | Graphical Rendering Unit | Reusable geometry with typed slots, modulation channels and bounded, externally scheduled operations. Goya's native alternative to SVG. |
| `.gva` | Graphical Vector Array | Deterministic structure-of-arrays vector data: deduplicated geometry pools and instance, modulation and bounds arrays. No executable content. The shared substrate. |
| `.gvi` | Graphical Vector Inventory | A **parent-scoped** inventory of precomputed projections for every reachable state of a graphic. It is allowed only when the state space is Known, Finite, Bounded and Manageable (KFBM), and it carries a completeness certificate. |
| `.spc` | Spatial Probabilistic Compression | The counterpart to GVI for continuous or unbounded state spaces. Anchors, layered tile grids, propagation conditions and residuals reconstruct any valid state within a declared error bound. Under the Non-Exclusion Invariant, low probability never removes a valid state. |

Governing rule: never compute at runtime what can be resolved economically
at build or install time. Never precompute what bounded reconstruction can
represent more cheaply.

## Status (2026-11-25)

| Phase | State |
|---|---|
| 0: Specification ratification | **Done.** [GY-GFXC/1](container.md) was ratified on 2026-11-24 (D1–D5 below). |
| 1: Foundation and GVA | **Exit gate met for GIR-authored drawings.** [GY-GIR/0 and GY-GVA/0](gva.md) are drafted, with the `goya-graphics-ir` and `goya-gva` crates and golden fixtures. Encoding is deterministic and idempotent, and decoded files render bit-identically to their source on the CPU reference renderer. Not done yet: SVG import (W06), entropy coding, and SIMD decode paths (the layout allows them but none is written). |
| 2: GRU | **Core done (2026-11-25).** [GY-GRU/0](gru.md) and the `goya-gru` crate cover typed slots, bounded declarative bindings, `CHNG` invalidation, incremental atomic updates with no geometry rebuilds, and regions for hit-testing and focus. The exit gate is met through the `UnitSource` API. `.gscene` authoring and SVG import are still to do. |
| 3: GVI | **Core done (2026-11-25).** [GY-GVI/0](gvi.md) and the `goya-gvi` crate cover KFBM assessment, reachable-state enumeration (product with exclusions, or transition closure), merging of identical projections, parent binding, certificate verification at open, and runtime lookup that rejects invalid and unreachable states. Baked projections and integer-exact path morphs are included. Differential geometry (GVI-D) with packed projection rows and a packed index bring the baked morph inventory from 119.6 KB to 11.3 KB (−91%) and the TV menu inventory from 47.9 KB to 4.9 KB (−90%). Lookup drops from about 60 ns to 27 ns, and drawing construction is about 9% slower. Expanded residency makes drawing construction 43% faster, at the cost of more heap. GVI-A trajectory records ([gvi-albit.md](gvi-albit.md), opt-in) cut a further 23–72% below the best GVI-D configuration on morph workloads, and rebuild any single shape 4–14× faster. GVI-L ([gvi-lineless.md](gvi-lineless.md)) stores the style pool as a shared colour palette when that section is strictly smaller than `STYL`; the decoded styles are the same pool. GVI-O operative encoding ([gvi-operative.md](gvi-operative.md), opt-in) removes derivable metadata on top of that: −23% to −98.5% against the strongest configuration, with lookup unchanged. Exit gate met with the state model declared through the API. Deriving it from `.gscene` is Phase 5. |
| 4–7 | Not started. |

Colour Cueing is an execution policy, not a new section.
[colour-cue.md](colour-cue.md) and `crates/goya-cue` price full-frame,
front-buffer-copy, and in-place damage, keep a palette index until the
fill, and refuse a device-local GPU placement while Vulkan is blocked.

Measured so far (size only, synthetic; see [gva.md](gva.md) §8): 10,000
repeated circles are 69.5% smaller than SVG text, while a single icon is
about 5× larger because of the envelope. GVI lookup was measured on the B04 TV menu: p99 85 ns against the 100 µs
gate (see [gvi.md](gvi.md) §9). Precomputed morphs make a state change 16× (median) to 190× (p99) faster than morphing at runtime. GRU local updates measured about 500× faster than full reconstruction on the B03 dashboard, against the 5× gate ([gru.md](gru.md) §6). The SPC gate remains a target.

### Owner decisions on format roles (2026-11-25)

- **GVA is a compile-phase artefact, not a storage or distribution
  format.** Its size on small assets is not a concern for now. The
  small-asset overhead question is closed as option (a), and the
  envelope stays as ratified.
- **SVG keeps static assets where it is the more efficient form.**
  Representation selection (Phase 5) must be allowed to ship plain SVG
  for a static asset when SVG wins on the cost model. This is decided per
  asset, not across the board: repeated geometry, as in B02, still favours
  GVA-derived output.
- **GVI is not affected.** Its value is computational: animations and
  transformations whose state changes are known and bounded are computed
  ahead of time, not at runtime. Storage size is not what GVI is for.
  GVI's projections build on GVA data at compile time.

## Reconciliation with the repository

Where the directive's proposals meet existing Goya conventions, the drafts
make these choices. Each one is part of the ratification decision.

1. **Spec location.** Specs live in `docs/graphics/`, following the
   repository's `docs/<area>/` convention, not in a new top-level
   `specifications/` directory.
2. **Contract naming.** Contracts are `GY-GFXC/1` (ratified), and
   `GY-GIR/0`, `GY-GVA/0`, `GY-GRU/0` and `GY-GVI/0` (drafts). They are
   registered in `docs/contracts.md` like the other `GY-*` contracts.
3. **Crates are created per phase.** So far `goya-graphics-container`,
   `goya-graphics-ir` (GIR and the CPU reference renderer), `goya-gva`,
   `goya-gru` and `goya-gvi` exist. The other crates proposed in
   directive §14 are added when their phase starts, so there are no
   empty scaffolds to drift.
4. **Digest scheme (deviation).** Directive §5.2 asks for a whole-file
   digest computed with the digest field zeroed. The draft hashes
   `header ‖ directory` instead, and the directory carries a SHA-256 for
   each section. This still commits to every byte, because the layout is
   canonical and padding must be zero. It also allows verified partial or
   memory-mapped loading, which the same section of the directive requires
   and which a whole-file hash cannot provide. SHA-256 comes from `sha2
   0.10`, which the workspace already uses.
5. **Directory entry and canonical layout.** The directive did not define
   them. The draft defines 72-byte entries, tags in strictly ascending
   order, and a single legal offset for each section.
6. **Compression.** Only codec 0 (stored) exists in 0.0.x. The entropy
   coder (canonical Huffman or another) is chosen in Phase 1 against
   measured GVA data, as directive §7.5 requires.
7. **Lengths stay parent-relative.** Geometry inside a unit uses local
   millipoints, matching GY-LAYOUT/1. Placement inside a `.gscene` remains
   `position_p`/`extent_p`. The directive's example state
   `size: [16, 24, 32]` is an absolute pixel size and conflicts with this
   rule. GVI size dimensions must be expressed as parent-relative extents,
   or as display-density buckets that the target resolves.
8. **Authoring stays in `.gscene`.** Apps are written only in `.gscene`.
   The illustrative `@gru 0.0.1` text syntax (directive §6.4) and the
   `graphic { … }` block (§10.3) must therefore become goya-lang grammar,
   not a separate authoring language. That grammar review belongs to
   Phase 2 and Phase 5.

## Ratified decisions (2026-11-24)

- D1: Envelope digest plus a SHA-256 per section. Accepted.
- D2: Strict canonical layout, with non-canonical files rejected. Accepted.
- D3: Baseline profile. Accepted.
- D4: Default limits. Accepted.
- D5: 0.x versioning with an exact `major.minor` match. The owner was
  indifferent, so it stays as drafted.

## Ecosystem dependencies named by the directive

These exist in this repository: Iberia/España (`crates/ie-*`), Caprichios
(`crates/goya-caprichios`), Kumimanu (`crates/goya-kumimanu`), and the
target compilers. Tauromaquia and Disparates appear only in Arachnid
documents and crates. Mesut, Adios, Quinta, VVF and Pathologize have no
code or spec here, so integrations that depend on them (directive §11.2,
§15 and §18) cannot be built until they exist.
