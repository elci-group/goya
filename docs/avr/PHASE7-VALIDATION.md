# AVR Phase 7 — Enterprise and SOTA Validation Report

| Document control | Specification |
| ---------------- | ------------- |
| Directive | GOYA-GFX-AVR-001 |
| Phase | 7 (§22) — Enterprise and SOTA validation |
| Status | Phase complete; **enterprise readiness NOT declared** (§23) |
| Host | Intel Core 5 120U, x86_64, Linux — release build (`cargo test --release`) |
| Evidence | `crates/goya-avr/tests/{security,format}.rs`, `crates/goya-avr-rugid/tests/benchmarks.rs` |

## 1. §18.4 acceptance objectives vs measured results

| Metric | §18.4 target | Measured (AVR-B02 F1Car, 144 obs) | Measured (AVR-B12 MercedesW14, 64 obs) | Verdict |
| ------ | ------------ | --------------------------------- | -------------------------------------- | ------- |
| Target derivation vs canonical compilation | ≥ 10× | 6× (canonical 54 ms, derive 8.9 ms) | 7× (canonical 22 ms, derive 3.2 ms) | **BELOW** on these assets |
| Relational asset residency reduction | ≥ 80 % | 87.5 % (bp 8750) | 87.5 % (bp 8750) | **MEETS** |
| Resident index lookup | sub-µs median | 2.37 µs median, 6.4 µs p99 | 2.30 µs median, 5.3 µs p99 | **BELOW** on this host/build |
| Observation reconstruction | within frame budget | DELTA resolution ~µs-scale, no rendering | same | **MEETS** (µs ≪ frame budget) |
| Canonical reuse | no re-render for eligible derivations | asserted byte-identical; render calls only in canonical pass | same | **MEETS** |
| Computation avoidance (§18.2 live baseline) | derived ≪ live | live render 248 µs vs resolve 2.4 µs — **104×** | live render 293 µs vs resolve 2.3 µs — **127×** | **MEETS** (internal baseline) |
| Invalid state acceptance | 0 | AVR-C14 refusals verified (§17 suite) | same | **MEETS** |
| Reconstruction dependency violations | 0 | AVR-C04/C05 construction-enforced | same | **MEETS** |
| Unauthorised substitutions | 0 | AVR-C12 policy enforcement verified | same | **MEETS** |
| Backward compatibility regressions | 0 | tag collision review + charset gate pass | same | **MEETS** (contract level) |

**Honest interpretation (§24 "unsupported SOTA claims" mitigation).** The 6–7×
derivation speedup and the 2.3 µs lookup median are *this host, this build,
these assets*. The §18.4 targets are explicitly "proposed research targets,
not existing measured achievements". Two structural factors explain the
shortfalls without invalidating the architecture: (a) the Rugid SVG renders
(~250 µs/observation) are cheap relative to derivation bookkeeping
(~76 µs/observation); genuinely expensive assets (ms-scale renders) raise the
ratio proportionally; (b) the resolver is an unoptimised contract
implementation; the sub-µs target is a reference-hardware statement pending
the profiling/binary-format work. No SOTA claim is made.

Cache-hit rate (56 %) is structural for the measured pattern: one cold sweep
(16 anchors resident of 144) plus one fully-resident revisit sweep.

## 2. §17 security matrix — verification status

| Threat | Mitigation | Evidence |
| ------ | ---------- | -------- |
| Malformed inventory | Structural validation | `cyclic_reconstruction_graphs_are_rejected`, `dangling_anchors_are_rejected` |
| Cyclic reconstruction | Dependency cycle rejection (AVR-C04) | same |
| Decompression bomb | Expansion limits | `hostile_delta_lengths_are_not_trusted_for_allocation` |
| Excessive neighbourhood size | Resource-bound validation (AVR-C07) | `oversized_neighbourhoods_are_rejected` |
| Invalid graph references | Referential integrity | `dangling_anchors_are_rejected` |
| Malicious asset dimensions | Allocation limits, checked arithmetic | `hostile_navigation_specs_and_request_storms_are_bounded` |
| Excessive recursion | Nesting-depth enforcement | composition `MAX_SPATIAL_NESTING_DEPTH` (Phase 4 suite) |
| Corrupted payload | Digest verification | `corrupted_delta_streams_are_rejected` |
| Unauthorised rendering fallback | Capability and policy enforcement (AVR-C12) | `live_fallback_requires_authorisation` |
| Resource exhaustion | Budget enforcement and cancellation | `request_bursts_never_exceed_residency_budgets` (2 000-request storm under budgets) |
| Cache poisoning | Canonical identity and provenance verification (AVR-C11) | `poisoned_provenance_is_refused_at_derivation` |

Additional §15 gate: `uncertified_exact_provenance_is_rejected` (AVR-C09).
Corpus-based fuzzing (cargo-fuzz or equivalent) remains tooling work — not run.

## 3. §23 enterprise release gates

| Gate | Requirement | Status |
| ---- | ----------- | ------ |
| AVR-G01 | Existing GVI compatibility preserved | **PASS** — existing-gvi tag namespace untouched, charset gate green |
| AVR-G02 | Canonical inventory generation operational | **PASS** — Phases 1 & 6 (model + environment) |
| AVR-G03 | Multiple target derivations from one canonical compilation | **PASS** — 3 profiles per asset, no re-render, asserted |
| AVR-G04 | Deterministic relational graph construction | **PASS** — determinism asserted per phase |
| AVR-G05 | Bounded reconstruction and residency | **PASS** — depth + byte budgets construction- and runtime-enforced |
| AVR-G06 | Certified observation fidelity | **PASS** — Exact/Bounded/Unsupported classes with AVR-C09 enforcement |
| AVR-G07 | Mixed-dimensional composition operational | **PASS** — Phase 4 + nested viewports (Phase 6) |
| AVR-G08 | Physical cross-platform execution demonstrated | **UNRESOLVED** — §19 hardware validation deferred; host-side only |
| AVR-G09 | Security and malformed-input testing passed | **PASS** (property classes) — corpus fuzzing outstanding |
| AVR-G10 | `.escene` diagnostic completeness | **PASS** (WS-12 slice, Phase 4 bridge) |
| AVR-G11 | Comparative performance advantage demonstrated | **PARTIAL** — internal baselines measured (104–127× avoidance, 87.5 % residency); external baselines (Godot, Qt Quick 3D, sprite atlas, meshoptimizer, impostors) not runnable in this environment |
| AVR-G12 | Stable specification and developer documentation | **PARTIAL** — contract versioned, ADR complete; binary format still proposed (§13.1/13.2), developer guide not written |

## 4. Declaration

Per §23: **no enterprise-readiness declaration is made.** Gates AVR-G08,
AVR-G09 (corpus fuzzing), AVR-G11 (external comparative baselines) and
AVR-G12 (binary format stabilisation, developer documentation) remain open.
Everything host-side that the directive requires of Phases 0–7 is
implemented, measured and certified; the §18.4 shortfall entries are recorded
as measurements, not hidden.
