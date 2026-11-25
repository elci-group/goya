# GVI-A — Albit trajectory records in `.gvi`

Directive: GOYA-GFX-GVIA-001.
Status: **Experimental, opt-in**, inside GY-GVI/0. This is not a new file
format, section family or runtime API.
Implementation: `crates/goya-gvi`, in `diff.rs` (records) and `lib.rs`
(provenance, options, never-bigger).
Benchmark: `cargo run -p goya-gvi --example gvi_albit --release`.

## 1. Phase 0 — baseline and reconciliation (2026-11-25)

**Host:** Intel Core 5 120U (12 threads), Linux 6.18.7, x86_64, Rust 1.95.0.

**Tests and fixtures:** 80 graphics tests passed before GVI-A, and 84
pass after. Every fixture is byte-identical to the baseline:

| Fixture | SHA-256 |
|---|---|
| `container/sample.gva` | `e625efcb…5abf45` |
| `gru/dashboard.gru` | `860e1caa…210465b` |
| `gva/icon.gva` | `c89e2b2d…995539` |
| `gvi/status_icon.gvi` | `a58a3aa4…02d077` |

**The directive's baseline is out of date.** §2 cites 18.4 KB unbaked and
20.5 KB baked at depth 32. Packed `PROJ` (gvi.md §4.1) and packed `INDX`
(§4.2) have since landed, and the real GVI-D baseline on A01 is:

| Representation | Directive's baseline | Actual baseline (GVI-D, depth 32) |
|---|---|---|
| Unbaked | 18.4 KB | **8,976 B** |
| Baked | 20.5 KB | **11,280 B** |

GVI-A is measured against the actual figures. The directive's
"conventional columnar control" (§18) also already exists as packed
`PROJ`.

**Branch isolation (WA00) was not done.** The repository's `kaptaind.toml`
runs in `mode = "actuate"` with `[push] enabled = true` and
`network_push = true`. During this work the daemon was observed pushing
`nebula/v0.1` to `origin`. Switching branches under a daemon that commits
and pushes would change what it commits and pushes, so creating
`graphics/gvi-albit` is left to the owner.

## 2. What was built

The directive's software model maps onto existing machinery as follows:

| Albit term | Implementation |
|---|---|
| Albit point `(x, y, τ)` | A coordinate of a geometry at morph fraction `τ = t / 65536` |
| Albit trajectory | The endpoint pair `(from, to)`. Every sample is `morph_path(from, to, t)`, which is integer-exact and identical to canonical GVI. |
| Albit field | All coordinates of a path, sharing one endpoint pair |
| Albit domain | The fractions `t` used by the parent model's morph values. They may be nonuniform: each record stores its own `t`, so a frame index never stands in for a fraction. |
| Transform-derived family (A05) | `bake(morph_path(from, to, t), M)`, exactly as canonical baking (gvi.md §5) |

**Two record kinds were added to `DIFF`** (gvi.md §6). Both reuse its
offsets, base rules, residual codecs and 16-byte hashes:

| Kind | Name | Payload after `kind, flags` | Prediction |
|---|---|---|---|
| 3 | `Lerp` (A02 rational-affine) | varint `from`, varint `to`, varint `t ≤ 65536`, `hash[16]`, residual | `morph_path(from, to, t)` |
| 4 | `BakedLerp` (A05) | as kind 3, then 6 zigzag varints `M` | `bake_geometry(morph_path(from, to, t), M)` |

- **`COST` byte 24** records the GVI-A profile (0 off, 1 provenance,
  2 exhaustive). Bytes 25–31 must be zero.
- **`open`** rejects an unknown profile (E2006), and a trajectory record
  under profile 0 (E5009).
- **No new sections were needed** (`ALBT`, `ADOM`, `ARES`, `AMAP` and
  `ACST` in the directive). The record's position is its geometry id,
  which is the mapping. `COST` carries the bounds, and `DIFF` carries the
  parameters and residuals.

**Discovery is provenance-directed** (§15.1):

- While resolving states, the compiler remembers the last morph that
  produced each row's path: the source path, the target in the canonical
  frame, and `t`.
- With GVI-A on, the endpoints are placed in the geometry pool before the
  samples, so sample records can reference them.
- Each new derived geometry gets one trajectory candidate, costing O(V).

**Profiles** (`CompileOptions::albit`, default `Off`):

- `Provenance`: geometries with a trajectory origin skip the GVI-D
  all-pairs search. Others still use it.
- `Exhaustive`: trajectory candidates plus the full GVI-D search, keeping
  the cheapest exact record.
- GVI-A records live in `DIFF`, so `max_diff_depth = 0` disables them.

**Never bigger** (§17.2): a GVI-A compilation is compared with the same
compilation using `albit: Off`, and that one is returned unless GVI-A is
strictly smaller. When GVI-A brings nothing, the output is byte-identical
to GVI-D; A09 and the tests confirm this.

**Exactness:** every record is reconstructed and checked against its
hash. The benchmark also checks every state of every configuration
against the plain inventory before reporting.

## 3. Results (§37 workloads, this machine)

Medians. "build" counts 31 runs for A01 and A03, 11 for A07 and A09, and
5 for A02. "load" is eager `decode_geometry`. "lazy" is one
`geometry(g)` for 2,000 random `g` (A11). "anc" is the largest set of
geometries a lazy rebuild touches.

| Workload | Config | Bytes | Build | Load | Lazy | Trajectory records | anc |
|---|---|---|---|---|---|---|---|
| A01 unbaked | GVI-D d8 | 10,464 | 9.5 ms | 176 µs | 42.5 µs | 0 | 9 |
| | GVI-D d32 | 8,976 | 20.2 ms | 186 µs | 87.9 µs | 0 | 31 |
| | **GVI-A provenance d8** | **6,128** | 13.7 ms | 133 µs | **7.7 µs** | 30 | 2 |
| | GVI-A exhaustive d8 | 6,128 | 30.8 ms | 132 µs | 7.7 µs | 30 | 2 |
| A01 baked | GVI-D d8 | 18,528 | 60.8 ms | 452 µs | 26.0 µs | 0 | 10 |
| | GVI-D d32 | 11,280 | 78.4 ms | 496 µs | 94.9 µs | 0 | 32 |
| | **GVI-A provenance d8** | **8,656** | 67.4 ms | 622 µs | **10.5 µs** | 90 | 2 |
| A02 1,024 frames | GVI-D d8 | 253,680 | 216 ms | 6.3 ms | 46.4 µs | 0 | 9 |
| | GVI-D d32 | 189,168 | 555 ms | 6.0 ms | 126.8 µs | 0 | 34 |
| | **GVI-A provenance d8** | **52,480** | 262 ms | 5.7 ms | **9.3 µs** | 1,022 | 2 |
| A03 eased fractions | GVI-D d32 | 15,216 | 18.9 ms | 143 µs | 67.0 µs | 0 | 30 |
| | **GVI-A provenance d8** | **5,584** | 13.0 ms | 136 µs | **7.9 µs** | 30 | 2 |
| A07 baked rotations | GVI-D d32 | 11,056 | 106 ms | 582 µs | 48.5 µs | 0 | 16 |
| | **GVI-A provenance d8** | **9,712** | 103 ms | 582 µs | **11.5 µs** | 14 | 3 |
| A09 unpredictable | all four | 29,568 (identical bytes) | 5.9 / 11.9 ms | 30 µs | 1.7 µs | 0 | 0 |

### Against the directive's gates (§39)

| Gate | Result |
|---|---|
| ≥ 20% smaller than GVI-D on trajectory-rich workloads | **Met.** Against GVI-D d32: A01 unbaked −32%, A01 baked −23%, A02 −72%, A03 −63%. A07 is −12%, because the exact `Affine` re-bake predictor is cheaper than `BakedLerp` for the rotated frames, so only the unbaked frames use trajectories. |
| Lookup regression ≤ 5% | **Met.** Lookup is unchanged: GVI-A changes only how geometry is stored, and geometry is rebuilt once at load. |
| 100% exact reconstruction | **Met.** Checked for every state of every configuration. |
| 0% inflation | **Met.** A09 is byte-identical, and so is depth 0. |
| Direct addressing, no preceding-sample traversal | **Met.** Provenance samples need at most 2–3 ancestors, against 9–34 for GVI-D. Lazy single-shape rebuild is 4–14× faster. |
| Reduced compile expenditure (Opportunity B) | **Not met end to end.** Provenance planning is cheap: about 4 ms on A01 and 46 ms on A02, against 216 ms for the GVI-D search. But the never-bigger guarantee compiles the GVI-D candidate too, so total build time is GVI-D's plus the trajectory pass. A cheaper guarantee (for example, skipping the comparison when every derived geometry has a zero-residual trajectory) would need its own justification. |
| Eager load cost | Mixed: lower than GVI-D on A01 unbaked, A02 and A03, but higher on A01 baked (622 µs against 452–496 µs), because each `BakedLerp` runs a morph and then a bake. |

### Where the bytes are now

| Workload | Records (`DIFF`) | Anchors (`GEOM`) | State model (`STAT`) | Projections (`PROJ`) |
|---|---|---|---|---|
| A01 unbaked, 6,128 B | 832 B | 1,696 B | 1,520 B | 752 B |
| A02, 52,480 B | 27,376 B | 1,696 B | 19,200 B | 2,896 B |

- **Hash cost:** a trajectory record is about 27 bytes, and 16 of them are
  its integrity hash. On A02 the hashes are about 16 KB, or 31% of the
  file. §13.4 forbids removing per-shape coverage just to save size.
  Grouped verification (one digest over every reconstructed geometry)
  would still detect any corruption, but a single lazily rebuilt shape
  could no longer be verified on its own. **This is an owner decision.**
- **`STAT`:** the parent model (labels, effects, fraction per value) costs
  about 19 B per frame value. A compact `STAT` encoding is the next
  storage lever on long timelines.
- **`PROJ`:** 5–12% of these files. That caps any gain from the
  directive's projection-trajectory codec (§18), so it was not built.

## 4. Invariants (§3)

| ID | Status |
|---|---|
| INV-01 every reachable state exact | Holds, by test and benchmark |
| INV-02 semantics unchanged | Holds: `morph_path` and baking are reused, never reimplemented |
| INV-03 no trajectory evaluation in steady-state lookup | Holds: records are rebuilt at load, or lazily on request |
| INV-04 deterministic | Integer-only predictors, with no floats in reconstruction |
| INV-05 bounded, independently validated | Depth and ancestor counts are re-derived from `DIFF` at open, `t ≤ 65536` is checked, and arithmetic is checked |
| INV-06 existing artifacts compatible | All fixtures are byte-identical; files without `DIFF` are unaffected |
| INV-07 never replaces a smaller representation | Holds (never-bigger) |
| INV-08 no categorical values treated as continuous | Only morph fractions are trajectories. Themes and sizes stay categorical. |
| INV-09 reachability independent of compression | The record graph is unrelated to `TRAN` and `INDX` |
| INV-10 item identities intact | Projection rows are untouched |
| INV-11 no special hardware | Holds |
| INV-12 improvements measured | See §3 |

## 5. Diagnostics (§31)

The directive's categories map onto the existing registry. No new
numeric codes were needed, and 5001–5010 are unchanged.

| Directive category | Raised as |
|---|---|
| `ALBIT_INVALID_DOMAIN` (`t > 65536`) | E5009 at open |
| `ALBIT_INVALID_TOPOLOGY` | E5010 at compile; E5009 when a predictor cannot be evaluated at reconstruction |
| `ALBIT_UNKNOWN_EVALUATOR` | E5009 (unknown record kind) |
| `ALBIT_ARITHMETIC_OVERFLOW` | E5009 (checked arithmetic, predictor fails) |
| `ALBIT_INVALID_RESIDUAL` | E5009 or E2002 |
| `ALBIT_HASH_MISMATCH` | E5009 |
| `ALBIT_DEPENDENCY_CYCLE` | E5009. Bases must have lower ids, so cycles cannot be encoded. |
| `ALBIT_BUDGET_EXCEEDED` | E5009 (`COST` or open limits) and E5004 (compile budget) |
| `ALBIT_DUPLICATE_MAPPING` | Cannot occur: a record's position is its geometry id |
| `ALBIT_INCOMPLETE_MAPPING` | E3001 (a projection row references a missing geometry) |
| `ALBIT_UNSUPPORTED_PROFILE` | E2006 (unknown `COST` profile) |

## 6. Not built, and why

- **A03 piecewise, A04 shared displacement, A06 multidimensional, A07
  polynomial:** not needed on these workloads. Exact rational trajectories
  plus residuals already reach −23% to −72%. Without a workload that needs
  them, they would be unmeasured complexity (§39, INV-12).
- **Projection trajectories (§18):** `PROJ` is 5–12% of the file.
- **A-Native:** experimental by the directive's own terms.
- **A separate A-Lazy profile:** `Inventory::geometry(g)` already
  rebuilds lazily, touching ≤ 3 geometries.
- **Mesut, Adios, Tauromaquia and Quinta integration:** those components
  have no code in this repository.
- **The compilation report (§30):** `Inventory::geometry_layout()` exposes
  the profile, the trajectory record count and the bounds. A JSON report
  is not built.
- **Workloads A04–A06, A08 and A10–A14:** not built. A11 is covered by
  the lazy column, and A13 by the tamper tests.
