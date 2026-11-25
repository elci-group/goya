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

## Status (2026-11-24)

| Phase | State |
|---|---|
| 0: Specification ratification | **In progress.** The container contract is drafted ([container.md](container.md)) with a reference crate and golden fixture. GIR and the four format specs are not drafted yet. |
| 1–7 | Not started. Per the directive, no format implementation begins before the container is ratified. |

Nothing here has been benchmarked. The directive's performance gates (GRU
5×, GVA 50%, GVI p99 100 µs, SPC 90%) are targets, not results.

## Reconciliation with the repository

Where the directive's proposals meet existing Goya conventions, the drafts
make these choices. Each one is part of the ratification decision.

1. **Spec location.** Specs live in `docs/graphics/`, following the
   repository's `docs/<area>/` convention, not in a new top-level
   `specifications/` directory.
2. **Contract naming.** The container is `GY-GFXC/0` and is registered in
   `docs/contracts.md`, like other `GY-*` contracts. It moves to `/1` at
   ratification.
3. **Crates are created per phase.** Only `goya-graphics-container`
   exists. The other twelve crates proposed in directive §14 are added when
   their phase starts, so there are no empty scaffolds to drift.
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

## Open decisions for ratification

- D1: Accept the envelope-plus-section digest scheme (item 4)?
- D2: Accept the strict canonical layout? Any non-canonical file is rejected,
  not normalised.
- D3: Accept the Baseline profile ([container.md](container.md) §7): integer
  millipoints with round-half-even, 8-bit sRGB, straight alpha at rest,
  and no floats.
- D4: Accept default limits of 256 MiB per file, 64 sections and 64 MiB per
  section? Targets can override them.
- D5: Accept 0.x versioning, where `major.minor` must match exactly?

## Ecosystem dependencies named by the directive

These exist in this repository: Iberia/España (`crates/ie-*`), Caprichios
(`crates/goya-caprichios`), Kumimanu (`crates/goya-kumimanu`), and the
target compilers. Tauromaquia and Disparates appear only in Arachnid
documents and crates. Mesut, Adios, Quinta, VVF and Pathologize have no
code or spec here, so integrations that depend on them (directive §11.2,
§15 and §18) cannot be built until they exist.
