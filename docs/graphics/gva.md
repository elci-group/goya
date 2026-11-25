# GY-GVA/0 — Graphical Vector Array (`.gva`)

Status: **Draft** (GOYA-GFX-001 Phase 1). Binary spec version 0.0.1, inside
the ratified [GY-GFXC/1 container](container.md) with magic `GVA\0` and
the Baseline profile.
Reference implementation: `crates/goya-gva`.
Golden fixtures: `fixtures/graphics/gva/icon.gva`, plus its reference
rendering at 4 px/pt: `icon.x4.pam` and `icon.x4.sha256`.

GVA holds static vector graphics as deduplicated pools plus one array per
instance field. It is a **compile-phase artefact**, the data substrate that
GRU, GVI and SPC build on. It is not a distribution format: a static asset
may still ship as SVG when SVG is smaller (owner decision, 2026-11-25). It contains no executable content. Every interpretation
rule is set by this document.

## 1. Source model (GIR, `GY-GIR/0`, `crates/goya-graphics-ir`)

GIR is the internal compiler contract that GVA encodes. It is not a public
format.

- **Path:** `MoveTo`, `LineTo`, `QuadTo`, `CubicTo` and `Close`, with
  `i32` millipoint points. Paths are canonical:
  - they start with `MoveTo`;
  - a `MoveTo` is never followed by another `MoveTo`, and a path never
    ends in `MoveTo`;
  - a drawing command after `Close` is preceded by an explicit `MoveTo`
    back to the subpath start;
  - `Close` never directly follows `Close`;
  - a path that draws nothing is rejected (E2001).
- **Style:** an optional fill (colour and nonzero or even-odd rule) and an
  optional stroke (colour, width in mp > 0, butt/round/square cap, round
  join). Fill paints before stroke. A style with neither paints nothing
  but stays hit-testable.
- **Instance:** a shared path, a style, an affine transform (Baseline
  §7) and an opacity from 0 to 255. Items paint in order.
- **Drawing:** a logical canvas size in millipoints plus the item list.
- **Quantisation:** floating-point input converts to millipoints with
  round-half-even. Error beyond a declared tolerance is rejected (E2004)
  and never degraded silently.

## 2. Sections

Every section is mandatory except `HASH`. Each one is 16-byte aligned. All
integers are little-endian. A "sub-header" is 16 bytes: `u32 A, u32 B,
8 zero bytes`. Every array inside a section is zero-padded to a multiple
of 16 bytes.

| Tag | Layout |
|---|---|
| `HEAD` | `u32 width_mp, u32 height_mp, u16 unit (0 = millipoint), 6 zero bytes` (16 bytes) |
| `GEOM` | sub-header `(geometry_count, 0)`; `u32 offsets[count + 1]` (padded); then the path blob (padded). `offsets[0] = 0`, the offsets strictly increase, and the last offset is the blob length. |
| `STYL` | sub-header `(style_count, 0)`; then 16-byte records (§4) |
| `INST` | sub-header `(n, 0)`; then eight padded arrays of `n` entries each: `u32 geometry`, `u32 style`, `i32 e`, `i32 f`, `i32 a`, `i32 b`, `i32 c`, `i32 d` |
| `MODS` | sub-header `(n, channels = 1)`; then `u8 opacity[n]` (padded). Bit 0 of `channels` is opacity. Every other value is rejected. |
| `BBOX` | sub-header `(geometry_count, n)`; then `[i32; 4]` local control-point bounds per geometry; then `[i32; 4]` conservative document bounds per instance, stroke included (§5) |
| `HASH` (optional) | sub-header `(geometry_count, algorithm = 1)`; then 16 bytes per geometry: the first 16 bytes of SHA-256 over that geometry's encoded bytes. This is a structural identity for asset-level deduplication across files. It never affects rendering. |

Section lengths are exact. A decoder rejects any mismatch, non-zero
padding, non-zero reserved byte or unknown value (E2006, E2005, E2009).
It also rejects references out of range (E3001, E3002) and counts that
disagree between sections (E3003).

## 3. Path encoding

`varint(segment_count) ‖ one verb byte per segment ‖ coordinates`

- **Verbs:** `0` MoveTo, `1` LineTo, `2` QuadTo, `3` CubicTo, `4` Close.
- **Coordinates:** every stored point in order (control points first),
  written as `zigzag(Δx), zigzag(Δy)` LEB128 varints. Each Δ is taken
  from the previous point, and the first point's Δ is from the origin.
- **Varints:** must be minimal (E2002). A decoded coordinate must fit in
  `i32` (E2003).
- **Canonical form:** the decoded path must already be canonical under §1
  (E2007).

## 4. Style record (16 bytes)

| Byte | Field |
|---|---|
| 0 | flags: bit 0 = fill, bit 1 = stroke. Other bits must be 0. |
| 1 | fill rule: 0 = nonzero, 1 = even-odd |
| 2 | cap: 0 = butt, 1 = round, 2 = square |
| 3 | join: 0 = round |
| 4–7 | fill RGBA |
| 8–11 | stroke RGBA |
| 12–15 | stroke width, `u32` mp, > 0 |

Fields belonging to an absent fill or stroke must be zero.

## 5. Canonical encoding

An encoder must produce exactly the bytes the reference encoder produces,
and `Gva::verify` checks this by re-encoding.

1. **Origin folding.** For each item, if the path's first point `o` is not
   the origin, and `(a·o.x + c·o.y)` and `(b·o.x + d·o.y)` are both exact
   multiples of 2^16, then the path is translated by `−o` and `(e, f)`
   grows by those quotients. Otherwise the item is kept as is. This makes
   copies of a shape drawn at different places share one geometry without
   changing any rendered pixel (see §6).
2. **Pools.** Geometry and styles are deduplicated by structural equality
   and ordered by first use in paint order.
3. **Instance bounds.** Take the floor or ceiling, in mp, of the exact
   transformed corners of the geometry bounds. If the item has a stroke,
   widen this by `ceil(width/2 · √2 · max_column_norm)`.
4. **Section order.** Sections follow the container's ascending-tag
   order, and `HASH` is always emitted.

Entropy coding (directive §7.5 step 6) is not applied yet, because the
container defines only the stored codec.

## 6. Rendering contract

`goya_graphics_ir::reference::render` is the conformance oracle. It
flattens curves to within 0.05 device px and samples 16 sub-scanlines per
pixel row with exact horizontal coverage. It expands strokes into a union
of oriented quads and disks, and composites source-over in sRGB-encoded
space.

Each path is mapped to device space through coordinates relative to its
first point, plus the exact integer constant `a·o.x + c·o.y + e·2^16`.
Origin folding (§5.1) keeps that constant identical, so a decoded file
renders **bit-identically** to its source drawing. The tests check this
at 1, 2.5 and 4 px/pt.

Native backends need not be pixel-identical. Their tolerance against the
oracle is measured per backend (directive §5.3).

## 7. Access model

- `Gva::open` does several things without decoding any path:
  - verifies the container and every section digest;
  - validates every section's structure, all references and all styles;
  - checks limits before any allocation that depends on counts (instances,
    geometries, styles, segments per path, total expanded segments).
- `instances()` returns zero-copy views of each array, with no object
  graph built.
- `geometry(g)` decodes one path on demand.
- `instances_intersecting(rect)` answers a spatial query from `BBOX`
  alone, without rasterising.
- `verify()` does everything `open` does, decodes every path, checks
  `HASH`, and confirms canonical form.

## 8. Measured storage (synthetic, size only)

`cargo run -p goya-gva --example gva_storage --release` compares GVA with
plain SVG text that writes every shape with absolute coordinates:

| Workload | SVG text | GVA | Change |
|---|---|---|---|
| B01: one circle icon | 197 B | 992 B | **+404% (larger)** |
| B02: 10,000 repeated circles | 1,611,473 B | 490,832 B | −69.5% |

B02 meets the directive's 50% target for repeated geometry: 10,000 copies
collapse to one stored path. B01 shows a real weakness. The container
envelope is an 80-byte header plus 72 bytes per directory entry, and the
16-byte alignment adds padding. Together they cost more than a single tiny
icon. Because GVA is a compile-phase artefact, the owner has accepted this
overhead for now (see [README.md](README.md)). None of these numbers
measure decode or render speed.
