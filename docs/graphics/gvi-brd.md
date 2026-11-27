# GVI-BR — Differential break ruling (BR-D)

Status: **Proposal — not implemented.** A continuous-media extension, recorded
here so the idea does not have to survive in conversation. It would land as a
new record class in the AVR reconstruction graph (`crates/goya-avr/src/graph.rs`,
GOYA-GFX-AVR-001 Phase 3+). Nothing in `GY-GVI/0` or the Phase 2 contract
changes until the benchmark gate in §7 passes.

Related: [gvi.md](gvi.md) §6 (GVI-D), [gvi-albit.md](gvi-albit.md),
[ADR-AVR-001](../adr/ADR-AVR-001.md).

## 1. What it is

Exceptional pixels ("breaks" against the global luminance curve) are currently
conceived as independent departures. BR-D instead stores a secondary curve
governing the *average* behaviour of exceptions, and individual breaks as
differentials of that secondary curve:

\\[ L_{\text{exception}}(t) = L_{\text{global}}(t) + D_E(t) \\]
\\[ L_{\text{break},i}(t) = L_{\text{exception}}(t) + D_i(t) \\]

A break pixel then inherits global illumination changes, collective
exceptional illumination changes, and only its own residual reflective
behaviour. There is no architectural limit at two levels: material families,
collective motion, and pixel residuals can nest, so

\\[ \text{Reconstruction} = \text{Inheritance} + \text{Differentiation} + \text{Exception} \\]

The relationships need not be physically understood by the compiler; they only
need to be mathematically identifiable and economically reproducible.

## 2. Where it lands, and what is reused

BR-D is **not** new machinery. It is the GVI-D contract
(`crates/goya-gvi/src/diff.rs`) applied to a new domain, hosted in the AVR
reconstruction graph, which is empty in Phase 2 by design. Specifically:

- Records are predictor + exact residual, hash-verified, referencing lower ids
  only (acyclic by construction), with depth and ancestor counts declared in
  `COST` and re-derived at open.
- The per-record escape hatch carries over: a member whose differential
  encoding does not win is stored complete instead, as `diff.rs` already does
  for geometry.
- Graph-level cycle rejection and maximum dependency depth are enforced at
  construction by `graph.rs` (`MAX_DEPENDENCY_DEPTH = 8`), per ADR-AVR-001
  Decision 3.

The genuinely new machinery is only one thing: **grouping** (§4) — deciding
which exceptions share a parent curve. Everything else is a new record class
under an existing contract.

## 3. Root model: two curves plus one stored residual

The four-root table (observed, persistent, exceptional, correction) reduces to
two estimated curves plus a bookkeeping identity:

| Element | Status |
| --- | --- |
| Observed root | Derived: average of all source pixels |
| Persistent root | Estimated, excluding identified exceptions |
| Exceptional root | Estimated, over identified exceptions |
| Correction | **Determined** by the other three (weighted identity), not a fourth curve |

Two consequences:

1. **If reconstruction is exact, the correction is stored, never re-derived.**
   Re-deriving it at decode time from trimmed means or robust regression is an
   approximation path and must be flagged through the existing exact bit
   (AVR-C09). Storing the correction costs one curve; claiming exactness while
   deriving it is not permitted.
2. **Exclusion must be deterministic.** Exceptions are identified from the same
   frame data used to estimate the baseline they are excluded from — an
   in-sample loop. Iterative exclusion needs a declared iteration bound and
   deterministic tie-breaks (AVR-G04) or it is nondeterministic under float
   reordering. A single-pass rule over the quantised domain (declared
   threshold) is preferred over iterative robust regression.

The purpose stands: the persistent root represents the persistent environment
and must not react to transient events.

## 4. Grouping is the new risk

"The compiler constructs a hierarchy based on observed correlations" is a
search over clusterings — the one part of BR-D with no existing contract. For
output to remain reproducible:

- Grouping is **greedy** with a declared candidate window and deterministic
  tie-breaking (lowest id, as in anchor selection), never free-form clustering.
- The never-larger policy is evaluated **per group and per member**, not per
  frame. A differential chain can win in aggregate and lose for one particle;
  the member-level fallback (store complete) is the escape.
- An intermediate parent node exists in the output only if its subtree
  outperforms the alternatives under the declared storage, fidelity and
  decoding budgets.

## 5. Adaptive depth

`max_break_diff_depth` is a **profile parameter, never a default**
(ADR-AVR-001 Decision 2: never-default derivation budgets). It is bounded by
`MAX_DEPENDENCY_DEPTH`, and it is simultaneously a decoder-latency contract:
the resolved depth is declared in `COST`, parent curves are resolved once per
frame and cached, and strategy Caprichios (ADR-AVR-001 F6) choose the depth per
device family — compute-constrained devices flatten, storage-constrained
devices deepen.

| Depth | Representation |
| ----- | -------------- |
| 0 | Independent break curves |
| 1 | Global-relative break curves |
| 2 | Collective exceptional curves |
| 3 | Nested exceptional families |
| 4+ | Recursively differentiated exceptions |

## 6. Exactness

All curve arithmetic stays in the quantised integer domain (round-half-to-even,
the GVI arithmetic convention), as in the worked glitter example
(23 + 110 + {−20, 0, +15} = 113, 133, 148, exact). If any level of the
hierarchy introduces floating-point derivation, the `Exact` bit is withdrawn
for everything downstream of that level.

## 7. Benchmark gate — what is not built, and why

Every saving below is theoretical until measured (§18 programme, GOYA-GFX-AVR-001):

| Mechanism | Claimed saving |
| --- | --- |
| Shared exceptional curves | Fewer independently stored coefficients |
| Differential inheritance | Smaller residual magnitudes where correlations exist |
| Recursive grouping | Exploitation of collective particle behaviour |
| Parent evaluation caching | Reduced repeated decoder computation |
| Temporal bounding | No evaluation of inactive exceptions |

Implementation is gated on: (a) Phase 3 reconstruction work existing at all;
(b) a benchmark over at least the candidate scene classes — glitter of mixed
materials, colour-family fireworks, rain through a shared light source, ocean
reflections, cooling sparks — showing a strict win for the differential
encoding against flat break curves under the never-larger policy; and (c) the
deterministic-grouping contract of §4 being specified first.
