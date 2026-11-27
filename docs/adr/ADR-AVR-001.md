# ADR-AVR-001 — Adopt Anticipatory Vector Rendering (AVR) and the Relational Graphical Variation Infrastructure (GVI-R)

Status: Proposed (Phase 0 audit complete) | Date: 2026-11-27 | Owner: Goya Graphics | Directive: GOYA-GFX-AVR-001 | Supersedes: none

## Context

GOYA-GFX-AVR-001 proposes a spatial graphics architecture: Rugid models and environments embedded in `.gscene` elements, heavy observation generation performed once at canonical compilation time (AVR), and target-specific `.gvi` derivation through relational indexing and bounded residency (GVI-R). The directive itself requires Phase 0 to reconcile its proposals with existing Goya contracts before implementation begins. This ADR records that reconciliation.

## Audit findings

### F1 — Rugid exists as a standalone workspace (`~/RUGID`), not a Goya crate

`Rugid` is a separate Rust workspace (`~/RUGID`, crate `rugid 0.1.0`, edition 2024): a vector physics engine and GUI framework with the RDF scene language (`.rdf`, `type:::key[value]` syntax, lexer→parser→AST→converter) and multiple backends (headless SVG, wgpu, TUI, experimental rgpum). Its spatial contract, as audited:

| Directive assumption | Rugid reality | Gap |
| --- | --- | --- |
| Geometry `G` | `geometry3d::Point3D` (f32), `Rotation3D`, transforms; `shapes3d` (2 390 lines) with normals, back-face culling, tessellation | Arbitrary mesh import absent: geometry comes from a fixed `ShapeType` enum (`Cube, Torus, F1Car, MercedesW14, CornellBox, …`) — the §20 vehicle POC is directly served (F1Car, MercedesW14), general meshes are not |
| Cameras `C` / projection `P` | Hard-coded perspective (`focal_length = 400.0`, camera fixed on the −Z axis in `shapes3d`); the closest camera abstraction is `ocrs::ObserverCone` (position, target, fov, distance) | No general camera node; spherical/orbital observation domains need either a camera model or a parametrised `ObserverCone` |
| Materials `M` | `physics::light::OpticalMaterial` (metal/matte/plastic, `TextureType` incl. checkerboard and image) | View-dependent appearance exists only through per-face gradients; reflective sampling (§24) must build on this |
| Lighting `L` | `Scene { faces, lights, ambient_light, indirect_light }` (`physics::optics`) | Deterministic and explicit — good for canonical compilation |
| State `S` | `motion3d::{Translation3DState, Orbit3DState}`, RDF `AnimationSpec`, FSM, OCRS weight lattice | Reachable-state enumeration must factor rotation/translation orbits; OCRS weight-space is a custom observation-domain candidate (§6.2) |
| Environments | `physics::optics::Scene` + `.rdf` document; `cornell_box_physics`, `nested_boxes` demos | No environment composition, no navigation graph, no viewport concept — directive §5.3/§6.2 navigation domains are greenfield in Rugid too |

Two properties matter for AVR. First, Rugid's headless render path **already emits vector output**: `Platform::render(svg: String)`, and `render_shape` produces SVG faces with gradients — the `VEC` representation class (§4.2) is Rugid-native, and rasterisation becomes a downstream step (consistent with GVI's GIR/GVA pipeline). Second, the spatial pipeline is `f32`-based throughout, so byte-identical canonical output is only claimable per-host/per-build: the directive's §7.3 split between reproducible derivation and tolerance-based rendering reproducibility is mandatory, not optional, and `goya-avr`'s integer `ObservationState` coordinates must be mapped from Rugid's f32 state through a declared quantisation rule in the ingestion contract.

Consequence: WS-01 keeps its original name (Rugid spatial contract audit) but its output is an **ingestion contract** — which subset of `Runtime`/`Scene`/`ShapeType` state a canonical AVR compiler consumes, the f32→fixed-point quantisation rule, and the camera model gap (either a Rugid camera node or an AVR-side parametrised observer). Phase 1 is gated on that contract, not on building Rugid.

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
4. **Phase 1 gate.** Phase 1 (canonical anticipatory rendering) begins only after the AVR→Rugid ingestion contract exists (F1): the consumed `Runtime`/`Scene`/`ShapeType` state subset, the f32→fixed-point `ObservationState` quantisation rule, and the camera-model decision (Rugid camera node vs. AVR-side parametrised observer). The vehicle POC (§20) can start against `ShapeType::F1Car`/`MercedesW14` immediately, since those already exist in Rugid.
5. **Compatibility gates carried forward:** existing `.gvi` files remain readable (§13.2); GVI colour semantics unchanged (§12.4); all Inquisition classes AVR-C01–C14 apply to every emitted inventory.

## Phase 1 decisions (implemented)

The Phase 1 gate above is discharged by the following decisions, implemented in `goya-avr` and the `goya-avr-rugid` bridge:

- **Ingestion contract (WS-01 output).** A canonical AVR compiler consumes a Rugid model through the `goya_avr::ingestion::SpatialSource` trait: the static model definition (shape/geometry, materials, lighting — `ShapeType`, `OpticalMaterial`, `Scene`) plus one `Observer` per observation. Animation, simulation, FSM and OCRS weight-space state are out of Phase 1 scope. Rugid is consumed as an external workspace via a feature-gated optional path dependency; the rest of Goya never compiles Rugid.
- **Camera model.** AVR-side parametrised `Observer` (azimuth, elevation, distance, fov). For object-centric models an observer is equivalent to an inverse model rotation (Rugid's render path has no camera node), so the bridge maps observer angles onto `Rotation3D`; distance maps to equivalent model size through Rugid's fixed focal length. Environment cameras (`rugid.viewport`) are deferred to Phase 6 and `observer_in_domain` rejects those domains until then.
- **Quantisation rule.** Angles quantise to centi-degrees and distances to 1/256 units, round-half-to-even (the GVI arithmetic convention). Sources render the *snapped* observer (`ingestion::snap`), so a canonical payload is a function of the quantised observation state and exact retrieval reproduces it bit for bit — verified by round-trip test against direct renders.
- **Sampling (WS-03).** Deterministic Fibonacci-sphere mesh (golden-angle increment) for `Spherical`/`Hemispherical`, even ring for `Orbital`, in canonical (elevation, azimuth) order; sample counts above a domain's declared bound are rejected (AVR-C02). Adaptive refinement and weighted acceptance (§6.3–6.4 middle steps) are Phase 2.
- **Canonical compilation (WS-04).** `canonical::compile` renders the mesh through the source under an explicit `CompilationBudget` and emits the inventory plus payload store with a §7.3 identity digest (directive + contract version + source digest + domain + policy + sorted payload digests). Per-observation fidelity is `Exact` (direct canonical render). Budget exhaustion is an explicit `BudgetExhausted`/incomplete-coverage error, never silent truncation (§6.4 step 8).
- **Exact retrieval (WS-08 core).** `retrieval::resolve` validates the domain (AVR-C01/C14), quantises the request, and resolves priority 1 (exact resident) on the quantised state; anything else is an explicit `Unsupported{OutsideDomain, NotCompiled}` — no fallback, no substitution (§4.3). Resolution never invokes the source (enforced by test).

Phase 1 status: canonical inventory generation and exact retrieval are operational against real Rugid shapes (`Cube`, `F1Car`, `MercedesW14`) headlessly, with 48 contract tests in `goya-avr` and 7 bridge round-trip tests in `goya-avr-rugid`, clippy clean. Not yet covered: relational structure (Phase 2), compression/reconstruction (Phase 3), `.escene` emission for AVR decisions (WS-12).

## Phase 2 decisions (implemented)

Relational GVI (directive §22 Phase 2, WS-06/WS-07), implemented in `goya-avr` (`relational.rs`, `target.rs`) and exercised against real Rugid renders:

- **Anchor selection (§8.4).** Greedy farthest-point sampling over quantised observation states, front-seeded, ties broken by lowest id — deterministic (AVR-G04). The full weighted objective `J = αB+βR+γT+δE+εH` is deferred until §18 benchmark evidence exists; fidelity/memory/latency hard constraints are enforced as typed errors, never weighted trade-offs.
- **Coverage contract.** Coverage is defined over neighbourhood *references*: every canonical observation must appear in some anchor's nearest-member neighbourhood. A profile whose `primary_nodes`/`max_neighbourhood_size` cannot cover the canonical set is rejected with `CoverageImpossible` — observational granularity is never silently dropped (§8.1, §9.2).
- **Three separated graphs (§8.3).** Navigation (anchor ring + anchor↔member edges, cycles permitted), reconstruction (empty in Phase 2 — no data dependencies yet, but an independently validated instance) and residency (anchor adjacency; predictive set empty until §11.2 runtime history exists).
- **Encodings in Phase 2.** `ANCHOR`, `COPY` (byte-identical payload detection via digest) and `SHARED` (neighbourhood reference, single keyed payload — no duplication). `DELTA`/`ALBIT`/`REPROJECT`/`COMPOSITE`/`LIVE` are rejected in derivation until Phase 3.
- **Derivation (§9).** `target::derive` consumes only the canonical compilation — the signature takes no spatial source, so eligible derivation cannot re-render (AVR-G03, §4.1). Profiles: Dense (all observations indexed and resident), Hybrid (anchors indexed, all neighbourhoods resident), Relational (anchors indexed and resident; everything else retained cold). Residency budgets are enforced per profile: a dense derivation under a tiny budget fails while relational fits — the budget chooses the profile, not the fidelity. Explicit `allow_pruning` is rejected until the §6.3 weighting machinery exists.
- **Derived resolution (§10.3).** `resolve_derived` implements priorities 1–2 (exact resident / exact non-resident through the neighbourhood) and priority 8 (explicit failure). Tier is reported (`Hot`/`Cold`); locality never restricts valid requests.
- **Measured artifact metrics (§18.3 subset).** Canonical/resident/retained counts, residency reduction in basis points. The §18.4 ≥80 % residency-reduction research target is asserted as a derived-artifact metric (4096 observations, 64 anchors → 98.4 %): a measurement of the artifact, not a benchmarked device result.

Phase 2 status: 62 contract tests in `goya-avr`, 8 bridge tests in `goya-avr-rugid` (relational derivation over a real 256-observation Rugid cube: 16 hot anchors + 240 cold members, all resolving bit-identically to direct renders), clippy clean. Still open: compression/reconstruction (Phase 3), binary format (Phase 2 §13.1 tags remain a reviewed proposal), `.escene` emission (WS-12).

## Phase 3 decisions (implemented)

Compression and reconstruction (directive §22 Phase 3, WS-05 core), implemented in `goya-avr` (`delta.rs`, `engine.rs`, `target.rs`, `retrieval.rs`) and exercised against real Rugid SVG renders:

- **Differential encoding (GVI-D, §12.2).** `delta.rs` implements a line-based LCS delta (`AVR-DELTA1`) that is *self-verifying*: `encode` never returns a delta it has not already reconstructed with `apply` and compared byte-for-byte. That makes the delta format itself an AVR-C09 certification mechanism — a stored delta cannot be wrong by construction. `apply` validates the header, instruction integrity, base line range and expansion limits; malformed deltas are rejected, never guessed at (§17). Phase 3 keeps chains at depth 1 (every base is a raw-stored anchor); multi-level delta chains remain future work.
- **Reconstruction engine contract (GVI-A, §12.3).** `engine.rs` defines the `ReconstructionEngine` trait (`candidates` / `apply`), deliberately absent from the artifact format: an `ALBIT` entry names an operation, and resolution *requires* an engine that provides it — the payload is computed, not stored, so a resolution without the engine is an explicit `ReconstructionUnavailable` failure, never a silent substitution (§4.3, priority 8).
- **Exact-vs-bounded certification.** At derivation time each engine candidate's output is verified against the real canonical payload: byte-identical → exact Albit (`ProvenanceRelation::Albit { exact: true }`); otherwise accepted only if the operation's declared error bound fits the profile's `max_silhouette_error_px` → bounded (`exact: false`, AVR-C10). An operation is **never** certified exact because its output looks similar, and an unverified nominal op is silently dropped, not certified. Resolution propagates this: any approximate step in a chain resolves as `FidelityClass::Bounded`.
- **Base selection is an encoding decision (§6.3).** Derivation evaluates candidate bases across every neighbourhood the observation belongs to (deterministic rank order, capped at 8 bases), not merely the first reference — an observation anchored in an unsuitable neighbourhood can still delta-compress against a better base.
- **Complete-size comparison (§12.5).** An encoding wins only when its total stored size (payload/params/delta bytes plus a declared 8-byte entry overhead) beats the raw fallback; `stored_payload_bytes`/`storage_reduction_bp` metrics measure the encoded result.
- **Derivation validation updated.** `DELTA` and `ALBIT` are accepted in `allowed_encodings` (Phase 2 rejected them); a profile allowing `ALBIT` without an engine derives no albit entries — generative reconstruction degrades to other encodings rather than failing the derivation.
- **Derived resolution (§10.3 priorities 1–5).** `resolve_derived` materialises `COPY`/`DELTA`/`ALBIT` chains depth-bounded by the reconstruction graph's certified maximum (AVR-C05); delta reconstruction and exact albit reconstruction are verified bit-identical against direct renders in tests. Priority 7 (live rendering) and the deferred encodings below remain unimplemented.

Phase 3 status: 75 contract tests in `goya-avr`, 9 bridge tests in `goya-avr-rugid` (a 256-observation real Rugid cube derivation with differential encoding: deltas selected, stored bytes below raw, every delta resolving bit-identically without re-rendering), clippy clean. Deferred to later phases: `DEP`/`REPROJECT` representations (Rugid produces no depth-bearing output — a prerequisite audit belongs to Phase 4), raster/layered representations, `COMPOSITE`, `LIVE`, multi-level delta chains, binary format (§13.1 tags remain a reviewed proposal), `.escene` emission (WS-12).

## Phase 4 decisions (implemented)

Mixed-dimensional composition (directive §22 Phase 4, WS-02 slice 1), implemented in `goya-avr` (`composition.rs`), `goya-lang` (`scene.rs`, `parser.rs`, `lib.rs`) and `goya-avr-rugid` (`tests/spatial_scene.rs`):

- **Grammar (§5.2/§5.3).** `.gscene` gains `@spatial.model` and `@spatial.viewport` blocks on elements, carrying the authored intent of §5.2/§5.3: source/environment, camera, `observation { domain, interaction|movement, zoom }`, `rendering { strategy, fidelity|anticipation, fallback }` and — required for viewports — `lifecycle { simulation, rendering }`. Unknown keys, unknown values, wrong-kind keys (`movement` in a model, `interaction` in a viewport), duplicate blocks and missing required declarations are refused at parse time at the narrowest range; a scene that misstates its spatial intent never parses into a silently different one (§4.3).
- **Admission gating.** `@spatial` is GSSH/4 surface, refused by GSSH/1–3 strict profiles with the new code **GY-E1019** ("invalid spatial declaration"), on the same footing as `@platform`/`@persistence`/`@iden` (GY-E1015). Earlier consumers never receive spatial content they would silently flatten (decision D11's refusal rule). The ontology-root collapse rule now refuses to collapse a wrapper that itself declares spatial content.
- **Composition contract (§5.1, AVR-C13).** `goya-avr::composition` owns the four composition classes, the recursive-composition budget (`MAX_SPATIAL_NESTING_DEPTH = 16`, spatial edges only), parent-chain cycle detection and per-element correctness: authored identity required (camera/input/lifecycle state must be addressable), asset required, model/viewport domain-kind agreement (object-centric vs environmental domains), viewport camera and lifecycle required. `LifecyclePolicy` pairs simulation and rendering policies explicitly so the §5.3 invariant — suspending rendering must not inherently suspend simulation — cannot be violated by construction: there is no shared "active" flag to couple them through.
- **The embedded-content loop, end to end.** The bridge integration test parses a real `.gscene` with strict GSSH/4 admission, validates the spatial element through the composition contract, compiles the referenced Rugid asset once into a canonical inventory, derives a relational target inventory (no re-render; §4.1) and resolves an embedded observation bit-identically to a direct render.
- **`.escene` participation (§16, WS-12 slice).** The bridge emits §11.2 compiler events (`SourceAccepted`, `DependencyResolved`, `ConstructLowered` with the AVR decision and canonical digest as `implementation_ref`, `OutputFinalised` with the derivation digest) through `goya-escene`'s bounded `Collector` — machine-readable, budget-bounded, with no invented evidence.

Phase 4 status: `goya-avr` 84 tests, bridge 9 + 1 end-to-end, `goya-lang` 111 lib + 7 spatial-corpus tests, clippy clean. Deferred to later phases: environmental viewports actually binding Rugid *environments* (Rugid has no environment/camera-node substrate — Phase 6 per the ADR audit), spatial input translation and focus (§5.4 runtime shell), navigation-graph content (Phase 6), live fallback execution (priority 7), and AVR emoji assignments in rendered `.escene` projections (follow the canonical registry, which currently carries no AVR entries).

## Deferred proposals (recorded, not adopted)

- **Differential break ruling (BR-D)** ([gvi-brd.md](../graphics/gvi-brd.md), proposed 2026-11-27): hierarchical differential curves over exceptional pixels — an exceptional luminance curve stored as a differential of the global curve, individual break diffs as differentials of that, recursively. This is the GVI-D record shape hosted in the AVR reconstruction graph (empty in Phase 2), not new machinery; the only new contract is deterministic greedy grouping. Adoption is gated on Phase 3 existing, the grouping contract being specified, and §18 benchmark evidence over the candidate scene classes (glitter, fireworks, rain, ocean reflections, cooling sparks). Any future adoption must accept the note's two conditions: exact reconstruction stores the correction root rather than deriving it (approximate derivation loses the exact bit per AVR-C09), and `max_break_diff_depth` is a non-default profile parameter (Decision 2).

## Consequences

- `crates/goya-avr` is added as the contract home for AVR/GVI-R (Phase 0 scope only: domains, representations, encoding classes, logical data model, graph separation, tags, verification classes, derivation profile).
- WS-01 produces the AVR→Rugid ingestion contract against the real `~/RUGID` workspace (F1): state subset, f32→fixed-point quantisation, camera-model decision. Rugid's vector-native headless output makes `VEC` observations the natural Phase 1 target, and its existing vehicle shapes serve the §20 POC.
- Binary format work (§13.1 sections) is deferred until Phase 2 and gated on the tag review test plus `.gvi` version negotiation.
- Benchmarking baselines (§18) must include the existing flat-GVI path; the F2 cost model (`validation + lookup + presentation`) is the internal baseline for lookup latency.

## Risks (from directive §24, adjusted by audit)

| Risk | Severity | Audit adjustment |
| --- | --- | --- |
| Rugid contract gaps (no camera model, no mesh import, no navigation graphs) | High | F1 — ingestion contract first; navigation/environment domains (§5.3, §6.2) are greenfield in Rugid as well |
| `f32` spatial pipeline blocks byte-identical canonical output | High | F1 — §7.3 reproducibility split is mandatory; quantisation rule fixed in the ingestion contract |
| Observation state explosion | Critical | Directive §24 mitigation; discretisation per F2 |
| Unauthorised substitution of look-alike reconstructions | High | F4 — exact bit + AVR-C09 enforcement in Phase 3 |
| Device-specific renderer divergence | High | Directive §24; F5/F6 give per-device Caprichio provenance through `.escene` |
