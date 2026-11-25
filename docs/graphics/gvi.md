# GY-GVI/0 — Graphical Vector Inventory (`.gvi`)

Status: **Draft** (GOYA-GFX-001 Phase 3). Binary spec version 0.0.1, inside
the ratified [GY-GFXC/1 container](container.md) with magic `GVI\0` and
the Baseline profile.
Reference implementation: `crates/goya-gvi`.
Golden fixture: `fixtures/graphics/gvi/status_icon.gvi`.

A GVI is a **parent-scoped, precomputed projection inventory**. It is not
a general vector asset. Its purpose is computational: an animation or
transformation whose state changes are known and bounded is resolved at
build or install time, so the runtime only validates a state and looks it
up:

```text
runtime work = state validation + index lookup + presentation
```

The presentation cost, including rasterisation, remains. GVI removes the
state *evaluation*, not the drawing.

## 1. Inputs

1. **Source graphic.** A GIR drawing, canonicalised through GVA
   ([gva.md](gva.md)). Its item order defines the **item ids** that
   effects and projections refer to.
2. **Parent state model.** For now this is declared through the
   `StateModel` API. Deriving it from `.gscene` is Phase 5 work.
   - **Dimensions,** each with a domain:
     - `Discrete(values)`: eligible for GVI.
     - `Continuous{min,max}`: Known and Bounded, but not Finite. An SPC
       candidate.
     - `Unbounded`.
     - `Opaque`: the compiler cannot see the semantics.
   - **Values,** each with a label and ordered **effects** from a closed
     set:

     | Effect | Meaning |
     |---|---|
     | `Transform(sel, M)` | `item.transform = M ∘ item.transform` (`Affine::then`: exact products rounded half-to-even; pure translations are exact) |
     | `Opacity(sel, f)` | `o' = (o·f + 127) / 255` |
     | `Restyle(sel, style)` | replace the item's style |
     | `Hide(sel)` | leave the item out of the projection |
     | `Morph(sel, target, t)` | interpolate the item's current geometry toward `target` (authored in the source item's own coordinates, with the same segment structure) at `t / 65536`, `0 ≤ t ≤ 65536`: `p + round_half_even((q − p)·t / 65536)` per point. The arithmetic is integer-exact; see `morph_path`. |

     Effects run dimension by dimension in declaration order, and within a
     value in order. A selector is `All` or a strictly ascending list of
     item ids.
   - **Reachability:**
     - `Product{exclusions}`: every combination is reachable except those
       matching an exclusion. Any reachable state may change to any
       other.
     - `Transitions{initial, rules}`: the states reachable from `initial`
       through rules `dim: from|any → to when guards`. Only single rule
       steps are permitted transitions.

Size dimensions must be relative to the extent the parent assigns, never
absolute pixels: for example, scale about the graphic's centre. This keeps
Goya's rule that every length is relative to its parent.

## 2. Eligibility (KFBM, directive §8.2)

`assess` reports each criterion, and `compile` refuses anything that is
not fully eligible. A partial inventory is never emitted.

| Fails | Code | Recommended representation |
|---|---|---|
| Known: an `Opaque` dimension | E5001 | GRU/GVA, evaluated at runtime |
| Bounded: an `Unbounded` dimension or invalid limits | E5003 | GRU/GVA |
| Finite: a `Continuous` dimension | E5002 | SPC if approximation is permitted, otherwise GRU/GVA |
| Manageable: reachable states, enumeration work, projection rows or encoded bytes over `Budget` | E5004 | SPC if permitted, otherwise GRU/GVA |

Default budget: 65,536 states, 2^20 enumeration steps, 2^22 projection
rows, 16 MiB per file and 16 dimensions.

## 3. States and keys

A state is one value index per dimension. Its key is mixed-radix, with
dimension 0 as the most significant digit:
`key = Σ vᵢ · strideᵢ`. The domain (the product of the radices) must fit
in `u64`. Keys are sorted, so the index is ordered lexicographically by
state.

## 4. Sections

All sections are 16-byte aligned, all integers are little-endian, and
every variable part is zero-padded to 16 bytes. `PARN`, `SRCE`, `STAT`,
`TRAN`, `INDX`, `PROJ`, `GEOM` and `CERT` are mandatory. `HASH` is
optional. `DIFF` and `COST` are present together exactly when `CERT`
bit 3 is set. Exactly one of `STYL` and `LSTY` is present (§4.3).

| Tag | Layout |
|---|---|
| `PARN` | `binding[32]`, `u32 parent_len`, `u32 compiler_len`, 8 zero bytes, the parent id, then the compiler id |
| `SRCE` | `sha256(canonical source GVA)[32]`, `u32 items`, `u32 geometries`, `u32 width_mp`, `u32 height_mp` (48 bytes) |
| `STAT` | `u32 dims`, `u32 morph_targets`, `u8 encoding` (0 literal as below; 1 GVI-O operative, [gvi-operative.md](gvi-operative.md) §3), 7 zero bytes, then the morph-target path table (each `u32 len` followed by GVA path bytes, deduplicated, in first-use order), then per dimension: `str name`, `u16 n`, and per value: `str label`, `u16 effects`, then each effect as `u8 kind (0 transform, 1 opacity, 2 restyle, 3 hide)`, `u8 selector (0 all, 1 list)`, `[u32 n, u32 ids…]`, payload (`6×i32` / `u8` / 16-byte style record / nothing / for morph (kind 4): `u32 t, u32 target index` into the path table). `open` requires `STAT` and `TRAN` to be byte-for-byte canonical, so a table cannot hold duplicate or unused entries.. Here `str` is `u16 len` followed by UTF-8. |
| `TRAN` | `u8 mode`, 3 zero bytes, `u32 count`. Product mode: `count` exclusions, each `u16 n, n × (u16 dim, u16 value)`. Transitions mode: `dims × u16` initial state, then `count` rules, each `u16 dim, u16 from (0xFFFF = any), u16 to, u16 n, n × guard pair`. |
| `INDX` | `u32 n`, `u32 flags` (bits 0–1: key form, 0 identity, 1 bitmap, 2 packed; bit 2: projection ids are the identity), 8 zero bytes, then the key payload, then a packed `projection[n]` column unless bit 2 is set (§4.2) |
| `PROJ` | `u32 projections`, `u32 rows` (distinct), `u32 refs`, `u32 flags` (bit 0: the row list is the identity; bit 1: columns may use GVI-O operative spans, [gvi-operative.md](gvi-operative.md) §5). Then packed columns (§4.1): `start[projections + 1]` (offsets into the row list), the row list `row_id[refs]` (omitted when bit 0 is set), and one column per row field over the row pool: `item`, `geometry`, `style`, `e`, `f`, `a`, `b`, `c`, `d`, `opacity`. |
| `GEOM`, `HASH`, `STYL` or `LSTY` | The geometry pool and the style pool. `GEOM` starts with the canonical source's geometries in their GVA order. After them come the geometries derived by morphs or baking, in first-use order across ascending keys. The style pool lists the source styles first, then styles introduced by `Restyle` in first-use order across ascending keys. `STYL` is GVA's 16-byte layout and is present when `CERT` bit 4 is clear. `LSTY` is the shared colour palette and is present when bit 4 is set (§4.3, [gvi-lineless.md](gvi-lineless.md)). |
| `DIFF` | Only with `CERT` bit 3. `u32 n`, `u8 encoding` (0 as below; 1 GVI-O columnar, [gvi-operative.md](gvi-operative.md) §4), 11 zero bytes, `u32 offset[n + 1]`, then the record blob (§6). Records are geometry ids `anchors…anchors + n − 1`. |
| `COST` | Only with `CERT` bit 3. `u32 depth_bound`, `u32 max_depth`, `u32 max_ancestors`, `u32 anchors`, `u64 reconstruction_points`, `u8 albit_profile` (0 off, 1 provenance, 2 exhaustive; [gvi-albit.md](gvi-albit.md)), 7 zero bytes (32 bytes) |
| `CERT` | `u8 method (0 product, 1 transition closure)`, 3 zero bytes, `u32 dims`, `u64 domain_size`, `u64 reachable`, `u64 projections`, `u64 merged_states`, `coverage_digest[32]` (SHA-256 of the key array), `u32 assumptions` (bit 0: effects are the closed GY-GVI/0 set; bit 1: unreachable states are rejected; bit 2: transforms are baked, §5; bit 3: differential geometry, §6; bit 4: styles use the GVI-L palette, §4.3. Bits 0–1 are required. Bits 2–4 are optional. Any other bit is rejected.), 4 zero bytes (80 bytes) |

### 4.1 Packed columns

Every `PROJ` column is stored in whichever of two forms is smaller, with
ties going to frame of reference:

- **Frame of reference:** `value = base + packed`. `packed` uses the fewest
  bits that hold `max − min`, so a constant column takes only its 16-byte
  header.
- **Dictionary:** an ascending table of the distinct values, plus each
  row's index in the fewest bits that hold `entries − 1`.

**Column layout:** `u8 mode (0 frame of reference, 1 dictionary)`,
`u8 width (0–32)`, `u16 entries`, 4 zero bytes, `i64 base`, then
`entries × i64`, then `ceil(len · width / 8)` bytes of LSB-first packed
values, with zero padding bits. Columns follow one another unaligned, and
the section is padded to 16 bytes at the end.

**The row pool** holds each distinct row once, in first-use order. A
projection is a range of the row list. When every reference is a new row
in order, the list is omitted (flag bit 0).

**Validation at open:**

- Every column must re-encode to its exact bytes.
- Row ids must appear in first-use order, every row must be referenced,
  and no two rows may be equal.
- The identity form must be used exactly when it applies.
- Each value must lie in its field's range.

Any failure is E2007 or E2006.

**Reading** one field costs an 8-byte load, a shift and a mask, plus a
table index for dictionary columns. A row's id is resolved once per row
when building a drawing.

### 4.2 Packed index

The index stores the reachable keys and each one's projection id in the
smallest exact form:

| Key form | Payload | Lookup |
|---|---|---|
| identity (keys are exactly `0..n`) | none | `key < n` gives position `key` |
| bitmap | a packed column of rank samples (keys set before each 512-bit block), then `ceil(domain / 64)` little-endian `u64` words | bit test, then the block's sample plus at most 8 popcounts |
| packed | the sorted keys as a packed column (§4.1) | binary search |

**Choosing a form:** identity is used whenever it applies. Otherwise the
smaller of bitmap and packed is used, with ties going to packed. A bitmap
is considered only for domains up to 2^32.

**Projection ids** are omitted when state `i` uses projection `i`, and are
otherwise a packed column.

**Validation at open:** the keys and ids are decoded, keys must be
strictly ascending and inside the domain, and the section must re-encode
to its exact bytes. Any failure is E2007, E2006 or E5009.

**Coverage digest:** `CERT`'s digest stays defined over the plain sorted
`u64` key list, whatever form stores it.

**Projections** are deduplicated by the exact content of their rows. States
that look identical share one projection, and `merged_states` counts them.
Each row keeps the **source item id**, so hit-testing and interaction stay
mapped to the parent's semantic items exactly.

### 4.3 Style palette (GVI-L)

The style pool is either `STYL`, in GVA's 16-byte records, or `LSTY`,
a shared colour palette ([gvi-lineless.md](gvi-lineless.md)). The
compiler emits `LSTY` only when that payload is strictly shorter than
`STYL`, and then sets `CERT` bit 4. Otherwise the file carries `STYL`
and bit 4 is clear. `open` requires exactly one of the two sections,
matching the bit, and rejects any other combination (E5009).

The decoded styles are the same pool either way. Source styles come
first, then styles introduced by `Restyle`, in first-use order across
ascending keys. Style ids in `PROJ` do not change.

## 5. Baked projections and derived geometry

A projection row normally keeps its full transform, and presentation
applies it. Some per-state work goes further than choosing a transform.
That work moves to build time in two ways:

- **Morphs** always produce new geometry. Each distinct interpolated path
  is stored once and shared by every state that reaches it.
- **Baking** (`compile_with(…, Bake::Transforms)`, recorded as `CERT`
  bit 2) works as follows for each visible row whose transform has a
  non-identity linear part:
  1. Every point maps to `round_half_even(M·p / 2^16)`, exactly, in
     document millipoints.
  2. The result is translated so its first point is the origin, and the
     row's transform becomes that translation. Equal baked shapes at
     different places still share geometry.
  3. A stroke bakes only under a **similarity** (`a = d, b = −c`, or
     `a = −d, b = c`). Its width becomes `round_half_even(w·√(a² + b²) /
     2^16)` and must be at least 1 mp. A stroked row under any other
     linear part (shear or non-uniform scale) keeps its transform,
     because its stroke cannot be baked faithfully. Fills bake under any
     affine.

Baking rounds each point to the nearest millipoint (≤ 0.0005 pt). Against
the unbaked inventory, the reference renderer then differs by at most
**1 premultiplied 8-bit level**, at edge pixels only. Tests enforce this
at 4 px/pt, and `examples/gvi_morph` measures it. The metric is
`Image::max_premultiplied_diff`. Straight-alpha RGB is unsuitable,
because it jumps arbitrarily where alpha is near zero.

Baking trades size for presentation work. Each (shape, linear transform)
pair becomes its own geometry, so baked inventories are larger (§10). A
backend that presents baked rows needs only a translation per row, with
no matrix.

## 6. Differential geometry (GVI-D)

GVI's object is the least information that reproduces every reachable
state exactly, within a declared reconstruction budget. With
`CompileOptions { max_diff_depth: D > 0, .. }`, each geometry created by
a morph or by baking may be stored as a **record** instead of a complete
path. A record is a deterministic predictor built from lower-id
geometries, plus an exact integer residual:

| Predictor | Prediction |
|---|---|
| `Copy(b)` | `b` |
| `Extrapolate(a, b)` | `2·b − a` per point. Morph frames are linear in `t`, so the residual is rounding noise. |
| `Affine(b, M)` | `b` baked under the linear part `M`, exactly as §5 (the unbaked geometry is placed before its baked forms) |
| `Lerp(a, b, t)` (kind 3, GVI-A) | `morph_path(a, b, t)`: a sample of the trajectory from `a` to `b`, from compiler provenance ([gvi-albit.md](gvi-albit.md)) |
| `BakedLerp(a, b, t, M)` (kind 4, GVI-A) | that sample baked under `M` |

**Record layout:** `u8 kind (0 copy, 1 extrapolate, 2 affine)`, `u8 flags`,
varint base ids, then for `Affine` six zigzag varints, then
`hash[16]` (the first 16 bytes of SHA-256 over the reconstructed path's
GVA encoding), then the residual.

**Residual encoding,** chosen by flags:

- bit 0 alone: one zigzag varint per coordinate;
- bit 0 plus bit 1: every residual is −1, 0 or +1, packed 2 bits each;
- bit 0 plus bit 2: every residual is in −7..7, packed as 4-bit nibbles;
- no flags: the residual is zero.

Padding bits must be zero, and invalid codes are rejected.

**The encoder** works through derived geometries in id order. Source
geometries always stay complete. For each derived geometry:

1. It tries `Affine` when the bake origin is known, then `Copy` and
   `Extrapolate` over the last 16 geometries with the same segment
   structure (all pairs).
2. It keeps the cheapest predictor whose depth, `1 + max(depth of bases)`,
   is at most `D`.
3. **Adaptive anchors:** if the best record would cost more than half a
   complete path, the geometry is stored complete instead. Depth resets
   there, which keeps later chains cheap.
4. Ids are renumbered so anchors come first, and each record references
   lower ids only.

The whole differential layout is used only if the file comes out
smaller. Otherwise the output is byte-identical to `max_diff_depth: 0`.

**Bounded Differential Traversal:** `COST` declares the depth bound, the
actual maximum depth, the largest ancestor set and the total points
rebuilt.

- **`open`** re-derives depth and ancestor sets from `DIFF` and rejects any
  mismatch (E5009). It also rejects bases that are not earlier, depth over
  the bound, and anything over `GviLimits::{max_diff_depth,
  max_diff_ancestors}`.
- **`decode_geometry`** rebuilds every record once, in id order, at load.
  Each record's hash proves its reconstruction exact (E5009 otherwise),
  and the total points must match `COST`.
- **`geometry(g)`** rebuilds a single geometry lazily from at most
  `max_ancestors` others.
- **Lookup is unchanged,** because projections reference geometry ids
  whatever the storage.

Reconstruction happens at **load time**, once, and never per lookup.
That is why there are no inverse patches or bidirectional traversal:
nothing is traversed at runtime. Approximate reconstruction belongs to
SPC, not here.

## 7. Parent binding and identity (directive §8.6)

`binding = SHA-256("GY-GVI/0 parent binding\0" ‖ len‖parent ‖ len‖STAT ‖
len‖TRAN)`. The lengths are `u64`.

`Inventory::open` requires the consumer's expected binding and rejects any
other (E5007). An inventory built for one parent is therefore never used by
another parent that merely shares the same SVG. It is also rejected after
the same parent's state semantics change. `open` also recomputes the
binding from the stored `STAT` and `TRAN` (E5009).

`cache_key = SHA-256(compiler id ‖ profile ‖ SHA-256(source GVA) ‖
binding)` tells a build system when it must regenerate.

## 8. Certificate verification at open (directive §8.7)

Before an inventory can be used, `open` checks:

- **Structure:** keys are strictly ascending and inside the domain; every
  projection id is valid and every projection is referenced; the row
  ranges cover the row arrays exactly; every row's item, geometry and
  style reference exists.
- **Figures:** method, dimension count, domain size, reachable count,
  projection count, merged count and coverage digest all match the index.
- **Completeness,** re-derived from `STAT` and `TRAN` by an argument
  different from the compiler's enumeration:
  - **Product:** every key in the domain is indexed if and only if no
    exclusion matches it.
  - **Transitions:** the initial state is indexed; the index is closed
    under every rule (each successor is present); and every indexed state
    is reached by walking rules from the initial state within the index.
    Together these make the index exactly the reachable set.

The certificate proves completeness relative to the stored model only. It
says nothing about future application behaviour beyond that model.

`verify_against(source, model)` also recompiles and compares bytes. It
reports a different source as E5008, a different model as E5007, and a
non-canonical file as E2007.

## 9. Runtime semantics (directive §8.8–§8.9)

`resolve(state)`:

1. Validate the arity and each value's range. Failure is **E5005**
   (invalid state).
2. Binary-search the key. A state that is valid in the domain but not
   reachable is **E5006**.
3. Return a zero-copy view of the projection's rows.

The runtime never substitutes another projection. Any fallback belongs to
the parent: it must be declared explicitly and reported, and it voids the
coverage claim for that state. `permits(from, to)` exposes the transition
relation. `state(&[(dim, label)])` builds a state by labels.

### 9.1 Residency

`Inventory::set_residency(Residency::Expanded)` unpacks the `PROJ` and
`INDX` tables into native arrays:

- `u32` ids, a contiguous `Affine` per row, `u8` opacity, and `u64` keys
  when keys are a packed list.

`Residency::Packed`, the default, reads from the file bytes. Results are
identical either way, and a test checks every state under both.
`table_residency()` reports the bytes borrowed from the file and the heap
in use.

Measured on B04 (`gvi_lookup`), in identical isolated loops run packed,
then expanded, then packed again:

| Residency | Lookup + drawing, median | Heap for tables |
|---|---|---|
| Packed | 640–655 ns (the two packed runs agree) | 328 B, plus 1,730 B borrowed |
| Expanded | 364–368 ns (−43%) | +11,212 B |

Expanded is also faster than the original fixed-width layout (620 ns). It
checks residency once per drawing, and reads each transform as one
contiguous struct instead of six columns.

## 10. Measured (B04 TV menu, this machine, `--example gvi_lookup --release`)

The workload is 36 items (12 tiles), with `focus × theme × overlay` giving
48 states and 48 distinct projections. Results are over 200,000 warm
resolutions; precompute is the median of 31 builds.

| Path | Fixed-width `PROJ` and `INDX` (47,872 B) | Packed `PROJ` (5,520 B) | Packed `PROJ` and `INDX` (4,944 B) |
|---|---|---|---|
| GVI lookup (validate + index), median / p99 | 59 ns / 85 ns | 60 ns / 131 ns | 27 ns / 68 ns |
| GVI lookup + build drawing, median / p99 | 620 ns / 809 ns | 690 ns / 1.34 µs | 675 ns / 1.21 µs |
| Dynamic evaluation + build drawing, median / p99 | 986 ns / 1.15 µs | 991 ns / 2.27 µs | 1.07 µs / 2.19 µs |
| Precompute (compile) | 441 µs | 464 µs | 475 µs |

**Packed `INDX` (§4.2):** this menu is a full product, so its keys and
projection ids are both identity. That removes the binary search, and
lookup drops from about 60 ns to 27 ns.

**Packed `PROJ` (§4.1):**

- **Size:** the inventory shrinks by 88%, because the unchanged tiles'
  rows are pooled.
- **Speed:** building the drawing costs about 11% more.
- **A regression, fixed before measuring:** the first version re-read the
  row id for every field and assembled bits one byte at a time. That made
  lookup + drawing 1.62 µs, slower than dynamic evaluation. Resolving the
  row id once per row and using one 8-byte load fixed it.
- **Timing noise:** the dynamic path's p99 also moved between the two
  runs, so treat the tails as noisy.

- **Latency gate:** p99 lookup is far below the directive's 100 µs target.
- **Saving:** lookup is about 17× cheaper than evaluating the state.
  Measured end to end, when both paths materialise a GIR drawing, the
  saving is only about 1.6×. This menu's effects are cheap, and building
  the drawing costs the same on both paths.
- **Where the gain grows:** with the amount of work a state change
  triggers, and when a native backend consumes the projection arrays
  directly instead of building a drawing.
- **Break-even (§13.5):** about 1,200 resolutions per single-device
  build with fixed-width `PROJ`, about 1,500–1,700 with packed `PROJ`, and
  about 1,200 with packed `PROJ` and `INDX`. An ahead-of-time build amortises this across every installation.
- **Morph workload,** from `--example gvi_morph --release`: a 64-cubic star
  morphing into a circle over 32 frames, × 2 themes × 3 relative sizes,
  for 192 states. Resolutions are 100,000 warm lookups; precompute is the
  median of 31 builds.

  | Path | median | p99 | Inventory | Precompute |
  |---|---|---|---|---|
  | Dynamic: morph + transform + drawing | 1.40 µs | 23.6 µs | — | — |
  | GVI lookup + drawing | 85 ns | 122 ns | 65,408 B | 1.5 ms |
  | GVI baked lookup + drawing | 85 ns | 122 ns | 119,600 B | 2.2 ms |

  - **Morph speed-up:** precomputing the morph makes a state change about
    16× faster at the median and about 190× faster at p99. The dynamic
    path allocates a new path on every change, which causes its tail.
  - **What baking buys:** it does not change lookup cost here. It removes
    the per-row matrix from presentation, at about 1.8× the inventory size.
  - **Baking drift:** at most 1 premultiplied level, on 1,840 edge pixels
    across all 192 states at 1 px/pt.
- **Inventory size by encoding,** for the same morph workload (`gvi_morph`).
  These figures include the morph-target deduplication in `STAT`, which
  alone took the unbaked inventory from 65,408 B to 40,080 B. Sizes are
  deterministic; times are medians of 31 runs and are for the final
  column. The byte columns show each step in turn: fixed-width rows,
  packed `PROJ` (§4.1), then packed `INDX` as well (§4.2).

  | Bake | `D` | Bytes (fixed) | + packed `PROJ` | + packed `INDX` | vs original | Precompute | Load-time decode | Anchors + records |
  |---|---|---|---|---|---|---|---|---|
  | none | 0 | 40,080 | 32,928 | 30,624 | −53% | 1.1 ms | 45 µs | 32 + 0 |
  | none | 2 | 22,816 | 15,664 | 13,360 | −80% | 7.1 ms | 160 µs | 8 + 24 |
  | none | 8 | 19,920 | 12,768 | 10,464 | −84% | 9.5 ms | 176 µs | 4 + 28 |
  | none | 32 | 18,432 | 11,280 | 8,976 | −86% | 20.3 ms | 184 µs | 2 + 30 |
  | baked | 0 | 94,272 | 87,312 | 85,008 | −29% | 2.3 ms | 171 µs | 96 + 0 |
  | baked | 2 | 41,792 | 34,832 | 32,528 | −73% | 34.6 ms | 365 µs | 27 + 69 |
  | baked | 8 | 27,792 | 20,832 | 18,528 | −85% | 62.6 ms | 446 µs | 10 + 86 |
  | baked | 32 | 20,544 | 13,584 | 11,280 | −91% | 79.5 ms | 495 µs | 2 + 94 |

  "vs original" compares against the first GVI inventories: 65,408 B
  unbaked and 119,600 B baked.

  - **`PROJ` itself:** 7,904 B fixed against 752 B packed (−90%). Each
    projection here has one unique row, so the identity list applies and
    the savings come from the columns alone: `item` is constant, `style`
    takes 1 bit, and `a`–`d` take a few bits each.
  - **`INDX` itself:** 2,320 B against 16 B. Keys and ids are both
    identity, so only the header remains.
  - **Baking penalty:** the baked inventory was 2.35× the unbaked one at
    `D = 0`, and is 1.77× at `D = 8` with everything packed.
  - **Lookup + drawing:** about 110–120 ns with fixed-width `INDX`, and 76
    ns with packed `INDX`, because identity keys skip the binary search.
    Geometry reconstruction happens once at load either way.
  - **What it costs:**
    - load-time decode rises by 20–320 µs, mostly hashing each record;
    - precompute rises by up to 36× (78.9 ms against 2.2 ms), from the
      all-pairs predictor search plus the complete-layout comparison.
      This is build-time cost.
  - **What dominates now** (unbaked, `D = 8`, 10,464 B): geometry
    (`GEOM` + `DIFF`, 65%) and `STAT` (1,520 B, 15%). `PROJ` is 7%, and
    `INDX` is 16 B.
- **Index forms,** from `--example gvi_index --release`: three models over
  a 40 × 40 × 40 domain (64,000 keys), each with trivial graphics. The
  baseline is the earlier fixed-width layout (`u64` keys plus `u32` ids).

  | Form | States | `INDX` | Fixed-width | `resolve` median | Fixed: pack + binary search only |
  |---|---|---|---|---|---|
  | identity (full product) | 64,000 | 32 B | 768,016 B | 61 ns | 109 ns |
  | bitmap (product excluding `a = b`) | 62,400 | 8,304 B | 748,816 B | 74 ns | 111 ns |
  | packed (sparse transition staircase) | 118 | 288 B | 1,440 B | 72 ns | 32 ns |

  - **Not like for like:** the last column is only the fixed layout's
    search, without state validation or the projection lookup.
  - **Large domains:** for the identity and bitmap forms, full `resolve`
    is still faster than the old search alone.
  - **Small sparse sets:** a 118-key binary search is cheap in either
    layout, so the packed form's gain is size, not speed.

## 11. Not yet done

- Deriving the state model from `.gscene` (Phase 5), and generation at
  install time.
- The `RSRC` (external resource manifest) and `META` sections from the
  directive. Geometry is embedded as GVA-format `GEOM`. The style pool is `STYL` or `LSTY` (§4.3).
- A declared, telemetry-reporting fallback hook.
- Opacity baking into style alpha, and miter or bevel joins for baked
  strokes, which GIR does not have yet.
- **Factorised projections (GVI-F).** No longer looks worthwhile: packed
  `PROJ` is 6% of the morph inventory, and factorisation would add
  runtime composition.
- A cheaper planner: the all-pairs search is O(window²) per geometry.
