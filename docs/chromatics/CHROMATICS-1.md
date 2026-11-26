# Chromatics: `.gscene` language surface (Phase 3 core)

Status: **Phase 3 language core built**, 26 November 2026, branch
`Lantern`, uncommitted. There is still **no Arachnid integration, no
presentation and no device run**. No release gate is claimed (§6).
Continued by `CHROMATICS-2.md`, which closes the two §14 coverage gaps
this record inherited from CHROMATICS-0 (scope inheritance and the
accessibility floor).

This record continues `CHROMATICS-0.md`. That file ended with seven owner
decisions (§5). This phase executes the recommendations that live in the
shared tree — decision 3 (a), decision 4 and decision 5 — as one coherent
language increment. Decisions 1, 2, 6 and 7 are Arachnid-side and remain
open; nothing here changes them.

## 1. What was built

`@chromatics` is a scene-level declaration, parsed by the compatibility
parser and carried on `Scene::chromatics`:

    @chromatics favourites {
        strategy: preferential;
        fallback: ranked;

        candidate severance {
            intent: resume_severance;
            label: "Resume Severance";
            entity: "show.severance";
            favourite: 0;
            cost: 8;
        }
    }

| Piece | Where | Notes |
| --- | --- | --- |
| Grammar | `goya-lang` `parse_chromatics`, `parse_chromatic_candidate` | Ontology body only, at most once per scene. The schema is closed: unknown keys, repeated keys, unknown keywords, missing `strategy`/`intent`, empty `label`, non-canonical or out-of-range integers are parse errors, never defaults. |
| AST | `ChromaticsDecl`, `ChromaticCandidate` | Strategy and risk are `goya_chromatics::Strategy` / `Risk` directly, so the grammar and the engine share one keyword set by construction (directive §12: one declaration, every target lowers it the same way). |
| Static validation | `goya_lang::chromatics`, `GY-E1021` | The statically determinable §12.3 checks: intent names a declared `@interaction`; duplicate slot pins refused; duplicate entities refused; sensitive/destructive/privileged candidates must declare `confirm: true` (§20); `probability` in 0..=1000; `cost` ≥ 1. |
| Strict admission | `GY-E1020` | Draft surface: every strict profile refuses the block, exactly as every profile refuses `@intent` with `GY-E1010`. Ratification flips admission; nothing is silently dropped. |
| Canonical lowering | `allocation_policy`, `allocation_candidates` | Declaration → engine `Policy` + `Candidate` list. Authored numbers pass through unchanged; `last_used`, `uses` and `expires` are not authored (runtime state, not scene) and start neutral. |
| Conformance | `crates/goya-lang/tests/chromatics_corpus.rs` | 7 tests: end-to-end allocation through the engine, §10.4 slot preservation across a favourite swap, §19 profile isolation via context keys and `forget`, the six GY-E1021 invariants with in-bounds spans, thirteen closed-schema refusals, GY-E1020 refusal under GSSH/1 and GSSH/5. |

Integer authoring is canonical (no leading zeros, no signs where unsigned)
and every number the engine consumes as an integer is parsed as one — the
scene cannot hand the engine a fraction (§9 keeps allocation exact and
portable to generated JavaScript).

## 2. Decisions taken, against CHROMATICS-0 §5

* **Decision 3 (a): declared candidates.** A candidate is written in the
  scene and names an `@interaction`. No collection construct was added;
  `source:` over runtime collections stays future work, as recommended.
  The `…-from` state binding of candidate fields (`rank-from`,
  `available-from`) is **not** in this increment — it needs live-profile
  admission and target wiring, which belong with decision 2.
* **Decision 4: intent vocabulary.** A candidate's intent must name a
  declared `@interaction` (`GY-E1021` otherwise). Naming an `@apps`
  request or an experimental `@intent` is refused in the core grammar; a
  target that wants `@apps` launches extends admission, as with other
  target-specific surface.
* **Decision 5: Inquisition.** The §12.3 static checks are goya-lang
  diagnostics (`GY-E1021`), not a new tool. The runtime invariants remain
  `goya_chromatics::generation::verify`.
* **Grammar shape.** The directive's §12 illustration uses `view`,
  `capacity: hardware.available_chromatics` and named sub-blocks
  (`eligibility`, `activation`, `presentation`). This tree has no view
  construct, capacity is negotiated by the target (§13.1), and
  presentation is Phase 4. The declaration therefore carries what a scene
  can mean today: strategy, eligibility flags, stability thresholds and
  candidates. Keys follow the tree's kebab-case property style.

## 3. What remains open

* **Decisions 1, 2, 6, 7** (branch placement, webOS lowering,
  `remote-colour` retirement, focus-model cost) — Arachnid-side, unchanged.
* **Profile admission.** No strict profile admits `@chromatics` while
  GY-CHROMATICS/0.1 is unratified; the first target flips this together
  with its lowering.
* **Utility authoring.** `contextual`, `urgency` and `preference` are
  authored integers in milli-inputs. Authored `priority:` keywords (§12.1)
  are not modelled; add a keyword set only with evidence it is needed.
* **§12.3 checks not statically representable:** capability requirements
  (targets negotiate capacity) and scope inheritance (no view constructs).
* **Resolver conformance (G10).** The fixtures are shared text, and the
  canonical lowering is available to a resolver, but only the Rust engine
  runs them. A second implementation is still required.
* **Presentation (Phase 4).** `ChromaticCandidate` carries a `label` (the
  accessibility floor, §21) and nothing visual yet.

## 4. Verification

* `cargo test -p goya-lang`: 126 unit + 30 integration tests pass,
  including the new corpus (7 tests).
* `cargo test -p goya-chromatics`: 13 tests pass, unchanged.
* `cargo clippy -p goya-lang -p goya-chromatics --all-targets --
  -D warnings`: clean.
* `cargo check --workspace`: clean (the additive `Scene` field breaks no
  consumer; nothing outside the parser constructs `Scene` literally).

**END OF RECORD**
