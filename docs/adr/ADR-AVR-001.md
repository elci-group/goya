# ADR-AVR-001 — Adopt Anticipatory Vector Rendering (AVR) and the Relational Graphical Variation Infrastructure (GVI-R)

Status: Proposed (Phase 0 audit complete) | Date: 2026-11-27 | Owner: Goya Graphics | Directive: GOYA-GFX-AVR-001 | Supersedes: none

## Context

GOYA-GFX-AVR-001 proposes a spatial graphics architecture: Rugid models and environments embedded in `.gscene` elements, heavy observation generation performed once at canonical compilation time (AVR), and target-specific `.gvi` derivation through relational indexing and bounded residency (GVI-R). The directive itself requires Phase 0 to reconcile its proposals with existing Goya contracts before implementation begins. This ADR records that reconciliation.

## Audit findings

### F1 — Rugid does not exist

`Rugid` appears nowhere in the codebase (`crates/`, `docs/`). The directive's §3 positions Rugid as existing spatial infrastructure (models, environments, geometry, materials, cameras); it is in fact a proposed system. WS-01 ("Rugid spatial contract audit") therefore has no present subject: it becomes **define** the Rugid spatial contract, not audit one. All phases that depend on it (WS-01–WS-14) inherit this prerequisite. Phase 1 cannot begin until a minimal Rugid model format (geometry, materials, cameras, environments) exists or is imported.

### F2 — GVI (`GY-GVI/0`) exists and is compatible in spirit, different in scope

`crates/goya-gvi` implements contract `GY-GVI/0` (normative text: `docs/graphics/gvi.md`): a parent-scoped, precomputed **projection** inventory over a discrete parent state model, with runtime cost `state validation + index lookup + presentation`. Its reachability model (`Product{exclusions}` / `Transitions{rules}`), budget machinery, certification (`CERT`), content hashing (`HASH`) and cache key (`COMPILER_ID` folded into cache keys) are directly reusable for AVR.

Two semantic differences must be preserved:

- **Observation states ≠ element variation states.** GVI states are combinations of parent dimension values; AVR observations are points in a camera/material/environmental state space `F(G,C,L,M,S,P)`. The canonical inventory must keep them distinguishable (directive §12.1). Concretely: AVR observation ids never share a namespace with GVI state ids, and an observation's `state` component (§13 `ObservationRecord`) carries spatial coordinates, not parent-state bindings.
- **Continuous domains.** GVI's `Dimension::Continuous` is already classified "Known and Bounded, but not Finite — an SPC candidate" and is not enumerable. AVR's bounded-computation rule (§4.4) matches this exactly: continuous observation domains are discretised, approximated under declared tolerance, hybrid-resolved, or rejected with an actionable diagnostic. No new contract exception is created.

### F3 — Binary section tags: collision review (directive §13.1)

Existing `goya-gvi` container tags (4 bytes, `A–Z`/`0–9` per GY-GFXC/1 E1014): `CERT, COST, DIFF, GEOM, HASH, INDX, LSTY, PARN, PROJ, SRCE, STAT, STYL, TRAN`.

Proposed AVR sections `OBSV, ODOM, OREP, ANCH, NBRH, NAVG, RDEP, RSDY, FIDY, PROV`: **no collision** with any existing tag. `PROV` is the only near-homophone (`PROJ` exists, `PROV` does not) and is retained. This review is encoded as an executable test in `crates/goya-avr` (`tags.rs`) so the collision check cannot rot. Final adoption still requires the `.gvi` format version bump and feature negotiation of §13.2 (old readers reject unsupported mandatory features).

### F4 — GVI-D / GVI-A / GVI-L extension points exist

- **GVI-D** (`goya-gvi/src/diff.rs`, `DIFF` tag): differential chains with bounded reconstruction depth already exist; extension to projected-geometry, depth, material and observation-local differences (§12.2) is additive.
- **GVI-A** (`goya-gvi/src/operative.rs`, Albit/Operative profiles already surfaced through `goya-diablos::graphics`): the exact-vs-approximate distinction required by §12.3 has no current flag; Phase 3 must add an explicit `exact` bit to provenance records, and Inquisition class AVR-C09 must enforce that certification is never inferred from visual similarity.
- **GVI-L** (`goya-gvi/src/lineless.rs`): appearance optimisation is line/colour-scoped; shared material identifiers for spatial observation are new surface area, backward-compatible.

### F5 — `.escene` producer contract exists; AVR annotations must register through it

`goya-escene` (ADR-ESCENE-001, ESC-2) defines the C/I/S annotation classes, `escene-registry/1.0.0` canonical identifiers, emoji-as-presentation-only mapping, and a producer contract already integrated into the Diablos pipeline (`goya-diablos/src/escene.rs` emits partial `.escene` per invocation). AVR's compilation/interpretation/substitution annotations (directive §16) MUST be added as new registry entries (`C.SPA_*`, `I.SPA_*`, `S.SPA_*` families), not as a parallel annotation channel. Emoji assignment follows the canonical schema; the directive's illustrative emoji are non-normative.

### F6 — Caprichios cover the §14 device families

`goya-caprichios` carries per-device Caprichios (AMD desktop/APU/EPYC/Threadripper, Intel desktop/mobile/Xeon, Apple, Snapdragon, Kirin, MediaTek, Exynos, NVIDIA, …) plus strategy Caprichios (compression, memory, parallelism, batching). The §14 candidate-strategy table maps onto these; no new device taxonomy is required. New spatial Caprichios (canonical-compilation placement, derivation profiling, residency strategy) will be registered through the existing catalog.

### F7 — Target-derivation host is Diablos

`goya-diablos` already owns the shared BSD compilation pipeline, capability registry, security synthesis, packaging, certification and `.escene` emission, plus a `graphics` module with backend selection (software, DRM, Wayland, X11) and GVI integration. AVR canonical compilation (Phase 1) and target `.gvi` derivation (Phase 2) belong behind this existing integration surface; a separate `goya-avr` crate holds the format/contract types so Diablos, Caprichios and Inquisition depend on the contract, not on each other.

### F8 — `.gscene` spatial grammar does not exist

The scene grammar in `goya-lang` has no `element`/`rugid.*` composition blocks. WS-02 is greenfield grammar work subject to the composition classes of §5.1, recursive-composition budgets, cycle detection and maximum nesting depth.

## Decision

1. **Adopt** AVR and GVI-R as specified in GOYA-GFX-AVR-001, with the reconciliations above. The directive's schemas (§13), encoding classes (§8.5), verification classes (§15.1) and profile contract (§9.3) are accepted as the Phase-0 logical contract, implemented in a new `crates/goya-avr` (workspace member, `#![forbid(unsafe_code)]`, no runtime deps).
2. **Never-default derivation budgets.** §9.3's numeric values are illustrative. `DerivationProfile` is deliberately constructed without `Default`; every derivation must state its budgets explicitly, so no value can silently become a universal default before the §18 benchmarking programme produces evidence.
3. **Reconstruction graphs are acyclic and checked.** The contract type enforces maximum dependency depth and rejects cycles at construction, matching AVR-C04/C05 rather than deferring them to Inquisition alone.
4. **Phase 1 gate.** Phase 1 (canonical anticipatory rendering) begins only after a minimal Rugid spatial contract exists (F1) and `goya-avr`'s observation-domain machinery (WS-03) is ready to consume it. The vehicle POC (§20) proceeds against a synthetic minimal Rugid model if the full format is not yet settled.
5. **Compatibility gates carried forward:** existing `.gvi` files remain readable (§13.2); GVI colour semantics unchanged (§12.4); all Inquisition classes AVR-C01–C14 apply to every emitted inventory.

## Consequences

- `crates/goya-avr` is added as the contract home for AVR/GVI-R (Phase 0 scope only: domains, representations, encoding classes, logical data model, graph separation, tags, verification classes, derivation profile).
- WS-01 is re-scoped from "audit Rugid" to "define the Rugid spatial contract" — the single largest prerequisite and the directive's principal unmitigated risk.
- Binary format work (§13.1 sections) is deferred until Phase 2 and gated on the tag review test plus `.gvi` version negotiation.
- Benchmarking baselines (§18) must include the existing flat-GVI path; the F2 cost model (`validation + lookup + presentation`) is the internal baseline for lookup latency.

## Risks (from directive §24, adjusted by audit)

| Risk | Severity | Audit adjustment |
| --- | --- | --- |
| Rugid absent (new) | Critical | F1 — blocks Phases 1–7; define contract before any rendering work |
| Observation state explosion | Critical | Directive §24 mitigation; discretisation per F2 |
| Unauthorised substitution of look-alike reconstructions | High | F4 — exact bit + AVR-C09 enforcement in Phase 3 |
| Device-specific renderer divergence | High | Directive §24; F5/F6 give per-device Caprichio provenance through `.escene` |
