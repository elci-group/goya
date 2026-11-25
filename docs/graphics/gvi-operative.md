# GVI-O — Operative representation in `.gvi`

Directive: GOYA-GFX-GVIO-001.
Status: **Experimental, opt-in** (`CompileOptions::operative`), inside
GY-GVI/0. This is not a new file format, section family or runtime API.
Branch: `graphics/gvi-operative`, in the worktree
`/home/sal/goya-gvi-operative` (§1).
Implementation: `crates/goya-gvi/src/operative.rs` (the span core), plus
`model.rs` (`STAT`), `diff.rs` (`DIFF`) and `packed.rs` (`PROJ`).
Benchmark: `cargo run -p goya-gvi --example gvi_operative --release`; O13
is in `--example gvi_lookup`.

## 1. Phase 0 (2026-11-25)

- **Repository control (§39).** At the owner's direction, GVI-O is
  developed in an external git worktree at `/home/sal/goya-gvi-operative`,
  on branch `graphics/gvi-operative` created from `nebula/v0.1@9ed86e7`.
  The 20 uncommitted graphics files were copied in and verified identical.
  The worktree is outside kaptaind's watch path (`/home/sal/goya`), so
  nothing there is auto-committed or pushed. The owner chose to manage
  kaptaind's configuration themselves. When checked, `origin/nebula/v0.1`
  was still `01cf2c7` (confirmed with `ls-remote`), with 57 local commits
  unpublished.
- **GVI-L integrated (09:3x).** The concurrent GVI-L work (style palette
  `LSTY`, `CERT` bit 4), done in `/home/sal/goya` after the copy, was
  merged into this branch. The strategy was chosen with `hybreed` and
  `emulsify`:
  - `hybreed decide` returned `HUMAN_REVIEW`: no textual merge, because
    the uncommitted shared GVI-A work left a stale merge base.
  - `emulsify` scored the `goya-gvi` crates 0.83 compatible (mix, direct
    bind).

  The merge went file by file. GVI-O-only files came from this branch,
  and GVI-L-only files (`lineless.rs`, `gvi-lineless.md`) from the main
  checkout. In `lib.rs`, `contracts.md`, the README and `gvi.md`, each
  side's independent hunks were combined by hand. Every main-checkout
  line absent from the merged result was checked to be a line GVI-O
  deliberately changed.

  After the merge: 95 graphics tests pass, including GVI-L's own test and
  a test that GVI-L and GVI-O compose. The fixtures are unchanged, and
  every §7 size reproduces exactly.
- **Baseline reproduced** on the same host as GVI-A (Intel Core 5 120U,
  Linux 6.18.7). 84 graphics tests pass. The fixtures match the GVI-A
  baseline hashes, and the GVI-A sizes reproduce to the byte: 6,128,
  8,656, 52,480, 5,584, 9,712 and 29,568.
- **Byte-level accounting** of the 1,024-frame GVI-A inventory (52,480 B),
  read from the file itself:

  | Section, field | Bytes | Per item | Derivable? |
  |---|---|---|---|
  | `STAT` labels `"f0"…"f1023"` | 6,058 | 5.9 per value | Yes: numbered |
  | `STAT` morph fraction | 4,096 | 4 per value | Yes: `floor(k·65536/1023)` |
  | `STAT` target index, effect count, kind and selector | 8,192 | 8 per value | Yes: constant |
  | `STAT` morph target path | 821 | — | Yes: a translated `GEOM` anchor |
  | `DIFF` kind, flags and bases | 4,088 | 4 per record | Yes: constant |
  | `DIFF` sample `t` | 2,810 | 2.75 per record | Yes: a progression |
  | `DIFF` offset table | 4,112 | 4 per record | Yes: uniform records |
  | `DIFF` shape hashes | 16,352 | 16 per record | **No**, and kept by §30 |

## 2. The operative core (`operative.rs`)

A column of values is a list of **spans**. Each value of a span is
evaluated directly from its local index `k`, with no traversal:

| Op | Catalogue | Value `k` |
|---|---|---|
| 0 `LITERAL` | O13 `LITERAL` and O12 `EXCEPT` | stored explicitly |
| 1 `CONST` | O00 | `c` |
| 2 `LINE` | O04 | `a + k·d` |
| 3 `RATIO` | O06, rational endpoints (§9.3) | `a + floor(k·N / D)`, `D ≥ 1` |
| 4 `NUMBERED` | labels: "predictable identifiers" (§18, Phase 2) | `prefix ‖ decimal(a + k·d) ‖ suffix` |

- **Numerical contract** (§33):
  - integers are `i64`, with checked `i128` intermediates;
  - `RATIO` uses floor division (`div_euclid`);
  - `NUMBERED` uses Rust's canonical `i64` decimal, so a label like
    `f007` stays literal;
  - overflow is an error, never wrapping or saturation;
  - there are no floats.
- **Span selection:**
  - A greedy, linear scan considers, at each position, the longest
    `CONST` run, the longest `LINE` run, and a `RATIO` fitted to the
    longest run whose steps differ by at most 1.
  - It keeps the span with the most bytes saved, provided that is
    positive; otherwise the value goes into a literal run.
  - Each scan is capped at 65,536 values (the compile work bound).
- **Canonical form:** the encoder is deterministic, and every reader
  re-encodes and compares, so each column has exactly one valid byte form.
- **Security** (§32):
  - span lengths must be non-zero and cannot overrun the declared count;
  - zero denominators and unknown ops are rejected;
  - minimal varints are required;
  - `NUMBERED` prefixes and suffixes are limited to 255 bytes;
  - labels are charged against a 16 MiB budget *before* they are
    materialised, and effect fields against a budget of 2^24.

## 3. Operative `STAT` (Phase 2)

- **Marker:** `STAT` byte 8 holds the encoding (0 literal, 1 operative).
  Literal `STAT` is byte-identical to before.
- **Morph targets:** each target is either a literal path or **`REF`**
  (O01): `(GEOM anchor id, dx, dy)` with `target = anchor.translated(dx,
  dy)`, using the lowest matching anchor id. Under GVI-A the morph target
  is already a trajectory endpoint in `GEOM`, so storing it twice was pure
  redundancy.
- **Per dimension:** the name, the value count, and a label column. If
  every value has the same effect signature (kinds, selector kinds and
  selector lengths), the signature is stored once and each field becomes
  an integer column: selector ids, then payload (6 for a transform, 1 for
  opacity, 16 for a restyle record, `t` and target index for a morph).
  Otherwise each value's effects are stored literally.
- **Binding unchanged (GO015):** the parent binding is always the SHA-256
  of the **canonical literal** model. `open` reconstructs the model from
  whichever form is stored, re-encodes the literal form, and checks the
  binding against that. It then checks that the stored bytes are exactly
  the encoder's output for their form. A forged operative model that
  decodes to a different model therefore fails the binding.

## 4. Columnar operative `DIFF` (Phase 3)

`DIFF` byte 4 holds the encoding (0 record-per-entry, 1 columnar). The
columnar form stores twelve integer columns: kind, flags, base0, base1,
`t`, the six affine fields, and the residual length. Fields a record kind
does not use must be 0. Then come `n × hash[16]` (**every per-shape hash
is kept**, as §30 requires), the concatenated residuals, and zero
padding. There is no offset table. Records go through the same field
checks as the record-per-entry form (`record_from_columns`), and the
section must re-encode exactly.

## 5. Operative `PROJ` columns (§21)

When the compilation opts in, `PROJ` flag bit 1 is set, and each column
may use mode 2: operative spans, if strictly smaller than both
frame-of-reference and dictionary packing. The packed codec stays
authoritative wherever it is smaller.

Operative columns are decoded once at load. Because they have no packed
form, an inventory holding them always uses **expanded residency**, and
`set_residency(Packed)` leaves it expanded. Expansion therefore never
touches the hot packed path. That path was measured unchanged: 641 ns,
against 640–655 ns before GVI-O. Mode 2 in a `PROJ` without bit 1 is
rejected.

## 6. Never bigger (§24)

- **Per section:** each section takes its operative form only if smaller.
- **Whole file:** the file is compared with the same compilation using
  `operative: Off`, and that one is returned unless GVI-O is strictly
  smaller. If nothing helps, the output is byte-identical (GO021).
- **`verify_against`:** recovers the profile from the stored markers.

## 7. Results (§41 workloads, this machine)

The baseline is the strongest existing configuration (GVI-A provenance
depth 8 in every case below; it beat GVI-D depth 32 throughout). Build
figures are medians of 11 runs, or 3 for O02 and O03. Load is `open` plus
`decode_geometry`, median. Resolve is the median of 100,000 warm lookups.
Every GVI-O inventory was checked against the baseline for every state.

| Workload | Baseline | GVI-O | Total | `STAT` | `DIFF` | Build | Load | Resolve |
|---|---|---|---|---|---|---|---|---|
| O01 192-state morph | 6,128 | **4,304** | **−29.8%** | 1,520 → 176 | 832 → 560 | 13 → 39 ms | 227 → 329 µs | 32 → 30 ns |
| O01 baked | 8,656 | **6,208** | **−28.3%** | 1,520 → 176 | 3,168 → 2,272 | 65 → 195 ms | 0.73 → 1.80 ms | 32 → 30 ns |
| O02 1,024 frames | 52,480 | **20,160** | **−61.6%** | 19,200 → 64 | 27,376 → 16,864 | 254 → 748 ms | 6.4 → 7.2 ms | 34 → 32 ns |
| O03 65,536 states | 293,024 | **4,496** | **−98.5%** | 17,216 → 128 | — | 224 → 659 ms | 37.8 → 29.3 ms | 67 → 67 ns |
| O04 eased fractions | 5,584 | **4,096** | **−26.6%** | 1,424 → 192 | 832 → 624 | 13 → 37 ms | 189 → 243 µs | 32 → 30 ns |
| O11 random labels and fractions | 5,952 | **4,576** | **−23.1%** | 1,808 → 688 | 896 → 672 | 20 → 60 ms | 191 → 240 µs | 32 → 29 ns |
| O13 TV menu | 4,944 | **3,808** | **−23.0%** | — | — | — | — | lookup + drawing 641 → 365 ns (expanded) |

### Against the directive's gates (§42)

| Gate | Result |
|---|---|
| ≥ 80% state-model reduction on regular workloads | **Met:** O01 −88%, O02 −99.7%, O03 −99.3%, O04 −87% |
| ≥ 20% total reduction on operative-rich workloads | **Met** on every workload measured: −23% to −98.5% |
| 100% exact reconstruction | **Met:** every state of every workload, and the stored model equals the compiled model (test) |
| 0% inflation | **Met:** per-section and whole-file never-bigger; byte-identical when nothing helps (test) |
| No lookup regression (≤ 5%) | **Met:** resolve is unchanged or faster. The hot packed path is unchanged (641 ns). Inventories with operative `PROJ` are expanded, which is faster but uses more heap. |
| Integrity coverage unchanged | **Met:** every 16-byte per-shape hash is kept |
| Existing fixtures compatible | **Met:** all four fixtures are byte-identical |
| Compilation search bounded | **Met:** linear greedy scan, each scan capped at 65,536 values |
| Clippy clean, uni ≥ 93 | **Met** (93) |

### Costs, stated plainly

- **Build time** roughly triples. Never-bigger compiles the non-GVI-O
  candidate (and, under GVI-A, its own comparison as well).
- **Load time** rises on morph workloads, from 20% (O02) to 2.5× (O01
  baked, 0.73 → 1.80 ms). `REF` decodes the `GEOM` anchors at open, and
  operative columns are decoded and re-encoded for the canonical check.
  O03's load *falls*, because there is far less `PROJ` to parse.
- **Heap:** an inventory with operative `PROJ` columns holds expanded
  tables (about 11 KB extra on O13).

## 8. Not built, and why

- **A strided or periodic operative (O07).** It is the next measured
  lever: with several items the `PROJ` row pool interleaves them, which
  plain spans cannot express. On a four-item 32 × 32 grid, operative
  columns saved only 12% of `PROJ` (13,936 → 12,272 B). With one item,
  O03 drops 98.5%.
- **Relationship deduplication and shared templates (§7, §13; O06–O08).**
  No measured workload showed repeated relationships across columns that
  spans did not already capture.
- **Operative algebra and fusion (§11–§12).** Transform effects are
  already composed exactly at compile time (`Affine::then`), and no
  stored relationship chains exist to fuse.
- **`REPEAT`, `CURVE`, `COMPOSE` and `BLOCK_REF`.** Not required by the
  benchmark corpus (§8.1).
- **Mesut (§29).** It is not in this repository. Load-time work is 0.3–29
  ms per inventory, and the lookup path is nanoseconds; neither is a
  scheduling target. Deferred until there is a multi-inventory workload
  (O15).
- **O-Selective and O-Adaptive residency.** Experimental by the
  directive's own terms.
- **The JSON optimisation report (§36).** The section sizes in §7 come
  from the benchmark, and `Inventory::geometry_layout()` exposes the
  geometry side. A machine-readable report is not built.
- **New sections** (`OPER`, `BIND`, `PARM`, `EXCP`, `ODOM`). Not needed:
  encoding markers in `STAT`, `DIFF` and `PROJ` carry everything.
