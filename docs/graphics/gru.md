# GY-GRU/0 — Graphical Rendering Unit (`.gru`)

Status: **Draft** (GOYA-GFX-001 Phase 2). Binary spec version 0.0.1, inside
the ratified [GY-GFXC/1 container](container.md) with magic `GRU\0` and
the Baseline profile.
Reference implementation: `crates/goya-gru`.
Golden fixture: `fixtures/graphics/gru/dashboard.gru`.

A GRU is Goya's general-purpose native graphic: reusable geometry plus
typed parameters that drive it, without an embedded program. The parent
owns execution. It sets slots, receives an invalidation, and schedules
presentation. Nothing in a unit runs by itself (§6.2.7).

## 1. Model

- **Instances:** a GIR drawing, canonicalised through GVA ([gva.md](gva.md)).
  Item order defines instance ids. Identical geometry is stored once.
- **Slots** (`TYPE`) are named and typed parameters with a default and a
  domain:

  | Type | Value | Domain |
  |---|---|---|
  | `Scalar{min,max}` | Q16.16 `i32` (Baseline stores no floats) | inclusive range |
  | `Int{min,max}` | `i32` | inclusive range |
  | `Bool` | `bool` | — |
  | `Enum(labels)` | label index | the label list |
  | `Colour` | RGBA8 | — |

- **Bindings** (`SLOT`) connect one slot to one **channel** of one
  instance. At most one binding drives a given (instance, channel) pair
  (E3005), so evaluation order never matters.

  | Target | Channel bit | Value |
  |---|---|---|
  | `Opacity(i)` | 1 | 0–255 |
  | `FillColour(i)` | 2 | RGBA8; the base style must have a fill |
  | `StrokeColour(i)` | 4 | RGBA8; the base style must have a stroke |
  | `TranslateX(i)`, `TranslateY(i)` | 8, 16 | mp, applied after the base transform |
  | `Scale{i, pivot}` | 32 | Q16.16 ≥ 0, uniform, about `pivot` in the instance's own coordinates, applied before the base transform |
  | `Visible(i)` | 64 | bool (table only) |
  | `Geometry(i)` | 128 | a geometry variant (table only), stored in the pool and decoded at load |

  The resolved transform is `scale_about(pivot) → base → translate(tx, ty)`,
  composed with `Affine::then`.
- **Maps:** every map is O(1). There are no loops, recursion, calls,
  allocation that depends on values, or ambient inputs.

  | Map | Rule | Slot types | Targets |
  |---|---|---|---|
  | `Linear{mul, add}` | `round_half_even(x · mul / 2^16) + add`, with `x` the raw slot value | Scalar, Int | numeric |
  | `Mix(from, to)` | per channel `from + round_half_even((to − from) · t / 2^16)` | Scalar ⊆ [0, 1] | colour |
  | `Table(entries)` | index by `false/true`, the enum index, or `int − min`; exactly one entry per domain value | Bool, Enum, Int | any |

  Linear outputs are checked at both ends of the slot's domain, and tables
  entry by entry, so no runtime evaluation can leave the target's range.
  Encoding and loading run the **same** type checker (E3004, E3008,
  E4105–E4107).
- **Regions** (`SPAT`): `{id, role (Button, Toggle, Slider, Indicator,
  Image), instance, focus, precise}`. Ids and focus positions are unique.
  A precise region must have a filled shape.

## 2. Sections

All sections are 16-byte aligned and all integers are little-endian. A
sub-header is `u32 a, u32 b, 8 zero bytes`. A `str` is `u16 len` followed
by UTF-8. Every variable part is zero-padded to 16 bytes. All sections
are mandatory except `HASH` and `META`.

| Tag | Layout |
|---|---|
| `HEAD` | `u32 width_mp, u32 height_mp, u16 unit (0), u16 profile (0 = bounded v0), 4 zero bytes` |
| `GEOM`, `HASH`, `STYL` | GVA pools (gva.md §2–§4). `GEOM` holds the canonical base geometry, then table variants in first-use order. |
| `INST` | sub-header `(n, 0)`; eight arrays: `u32 geometry`, `u32 style`, `i32 e, f, a, b, c, d` |
| `MODS` | sub-header `(n, channels = 1)`; `u8 opacity[n]` (base modulation) |
| `TYPE` | sub-header `(slots, 0)`; then per slot: `str name`, `u8 kind (0 scalar, 1 int, 2 bool, 3 enum, 4 colour)`, then `i32 min, i32 max` for scalar and int, or `u16 n, n × str` for enum; then the default value (`i32`, `u8`, `u16` or RGBA8) |
| `SLOT` | sub-header `(bindings, 0)`; then per binding: `u16 slot`, `u8 target kind (0–7, as in the target table)`, `u8 map (0 linear, 1 mix, 2 table)`, `u32 instance`, `[i32 px, i32 py]` for scale, then `i32 mul, i32 add`, or 8 bytes of colours, or `u16 n` followed by n 4-byte entries |
| `CHNG` | sub-header `(slots, 0)`; then per slot: `u32 channel mask`, `u32 k`, `k × u32` sorted instances |
| `SPAT` | sub-header `(n, regions)`; `[i32; 4]` local bounds per instance (covering every geometry variant it can show); then per region: `str id`, `u8 role`, `u8 precise`, `u16 focus`, `u32 instance` |
| `META` | optional, non-executable provenance: `u32 len`, 12 zero bytes, UTF-8 text |

On load, `CHNG` must equal what `SLOT` implies, and `SPAT` bounds must
cover every variant (E3006). A backend may therefore trust both without
re-deriving them.

## 3. Execution (directive §6.5)

`Unit::open` validates every section, decodes **all geometry once**, and
type-checks the unit. `Unit::instantiate` creates a state from the slot
defaults. Then:

`UnitState::set(slot, value)`:

1. Check the value's type (E2011) and domain (E2010). Unknown slots are
   E3008. Rejection is deterministic and changes nothing.
2. If the value is unchanged, return without doing anything.
3. Evaluate only that slot's bindings, on copies of only the instances
   `CHNG` lists. Recompute their resolved transform and style. If any
   transform overflows, abort with E2003 and leave the state unchanged.
4. Commit. Record the affected instances, the channel mask, and a dirty
   document rectangle covering each instance's old and new bounds.

`take_invalidation()` hands the parent `{instances, channels, region}`.
`modulation_only()` is true when only opacity, colour or visibility
changed. Geometry is never rebuilt after load: a geometry channel swaps
an `Arc` to a path decoded at load.

## 4. Visionless interaction (directive §6.6)

Interaction never looks at pixels:

- **`hit_test(point)`** returns the topmost region whose instance is
  visible and whose current document bounds contain the point. For
  precise regions, the point must also lie inside the filled shape under
  its fill rule, with curves subdivided into 16 chords.
- **Focus:** `focus_order()` lists visible regions by focus position, and
  `focus_next(id)` walks them with wrap-around. This serves TV remotes and
  assistive technology.
- Regions follow their instance's current transform and visibility.

## 5. Security bounds (directive §6.7)

The only execution profile is `bounded v0`, and an unknown profile is
rejected. Each binding costs O(1). Every count has a load-time limit in
`GruLimits`: slots, bindings, bindings per slot, table length and
regions. There are no system calls, no executables, no recursion and no
network access. Allocation after load is bounded by the instance count.

## 6. Measured (B03 dashboard, this machine, `--example gru_dashboard --release`)

The workload has 64 gauges: 256 instances, 128 slots, and 4 stored
geometries. There were 2,000 random single-slot updates, split between
level (a transform) and status (a colour).

| Strategy | median | p99 |
|---|---|---|
| Full reconstruction: reload the unit, re-evaluate, rebuild the scene | 111.8 µs | 249.7 µs |
| Full re-evaluation: unit loaded, re-evaluate everything, rebuild the scene | 29.7 µs | 80.2 µs |
| Incremental GRU: `set`, take the invalidation, rebuild dirty instances | 0.22 µs | 0.78 µs |

- **Gate:** incremental updates are about 500× faster than full
  reconstruction, against the directive's 5× gate, and 134× faster than
  full re-evaluation.
- **Correctness:** the run asserts that the incremental state equals a
  from-scratch evaluation at the end.
- **Caveat:** about one update in six sets a slot to its current value and
  costs almost nothing; the mean was 0.82 dirty instances per update.
  Rasterisation is excluded.

## 7. Not yet done

- **`.gscene` authoring:** units are currently built through the
  `UnitSource` API. The directive's illustrative `@gru` syntax should
  become goya-lang grammar, so that apps stay pure `.gscene`.
- **SVG import** (W06) and its "imported SVG preserves semantics" check.
- **Rotation channels.** These need deterministic trigonometry or
  precomputed tables.
- **Chained or derived bindings.** The current depth is 1 by design.
- **Wiring GRU slots into GVI** as a state source.
