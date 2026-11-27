# ADR-ESCENE-001 — Define `.escene` as Goya's intent realisation artefact

Status: Accepted (foundational definition) | Date: 2026-11-27 | Owner: Goya Ecosystem | Supersedes: none

## Context

`.gscene` gives the ecosystem a common language for expressing developer intent, and the platform compilers, resolvers, Caprichios and profilers give Goya the machinery to realise that intent on many targets. What the ecosystem lacks is a durable, machine-readable account of *what Goya actually did*: which code it generated, what difficulties it met, and which Caprichios it substituted in. Diagnostics and profiler output are ephemeral and not source-preserving, so no agent can later reason about implementation provenance or drive toolchain improvement from artefacts alone.

## Decision

`.escene` is Goya's **deterministic, source-preserving, target-specific intent realisation artefact**, documenting the functional implementation of developer intent, the computational experiences encountered during its realisation, and the Caprichios employed in its implementation.

It is not a diagnostic format or an annotated profiler output. It preserves source locality (annotations attach to the originating declarations) and carries structured evidence, so the artefact answers: *how did Goya realise this intent on this target, and why does the result have its shape?*

### The three annotation classes

The three classes have distinct semantic responsibilities and do not overlap:

| Class | Responsibility | Question answered |
| ----- | -------------- | ----------------- |
| Compilation | Intent realisation | How did Goya translate this intent into functional code? |
| Interpretation | Experienced behaviour | What issues and computational characteristics did Goya encounter? |
| Substitution | Implementation provenance | Which Caprichios did Goya employ to realise this intent on the named platform? |

**Compilation — intent realisation.** Explains the relationship between the abstract intent in `.gscene` and its concrete implementation on the target, so an agent can understand the implementation without reverse-engineering the executable: native implementation generated, native platform facility used, synthesised implementation required, computation resolved at compile time, redundant implementation eliminated, external dependency introduced, implementation packaged, semantic equivalence verified, intent decomposed into multiple implementations.

**Interpretation — experienced behaviour.** Communicates what Goya actually encountered during analysis and execution: resource sensitivity, call redundancy, pre-exposure preparation opportunity, thermal sensitivity, dependency issue, execution latency, thread contention, cache inefficiency, energy inefficiency, computational bottleneck, resource efficiency, execution efficiency, and insufficient observational evidence. Observations carry measurable evidence, environmental context, and attribution confidence.

**Substitution — Caprichio provenance.** Records which Caprichios were selected and applied, per target: graphical representation, compression strategy, computational reuse, precomputation, native platform implementation, parallelisation, neural execution, distributed execution, data representation, resource residency, and fallback. Every applied substitution references the exact Caprichio, its version, its configuration, and the implementation it replaced. **An identified but unapplied Caprichio must never be reported as an applied substitution.**

### Architectural position

```
.gscene  ── (Goya: compilers · resolvers · Caprichios · profilers) ──▶  .escene
what the developer intends          │          how Goya realised the intent
                                    │     exact source + deterministic annotations
                                    │            + structured evidence
                                    ▼
                          engineering agent: diagnosis · comparative analysis
                                  · source improvement · toolchain improvement
```

`.gscene` is a common language for developing applications; `.escene` is a common language for understanding and improving their implementations. Cross-target comparison of `.escene` artefacts from the same source revision lets an agent attribute a defect to the developer's expression of intent, the target compiler's implementation, the resolver's strategy, a Caprichio's suitability, or the target environment — rather than rewriting `.gscene` to compensate for a deficiency in one target. This supports two independent improvement cycles: application development and compiler development. An agent consuming many `.escene` artefacts across applications and targets can identify recurring implementation deficiencies, ineffective Caprichios, and opportunities for new Caprichios.

### Emojis as a semantic index

Emoji markers act as compact, deterministic diagnostic identifiers preserving source locality; they are an index into structured evidence, not the entirety of the information. An agent scans the annotated source, identifies diagnostic classes, and retrieves detailed evidence only where necessary. Emoji tokens map to stable machine-readable identifiers (emoji↔ID mapping is separate from this ADR). Their token efficiency must be **measured, not assumed** — different model tokenizers encode emoji differently.

Illustrative `.escene` fragment:

```
⚙️ 🐘 🎨 block render_world {
    🧮 ♻️ resolve_geometry()
    🏠 🐘 resolve_textures()
    ⚙️ ⏳ render_environment()
}
```

An agent inspecting `resolve_geometry()` retrieves its annotation records and learns the original intent, the implementation, the measured performance (e.g. original 18.4 µs vs observed 0.42 µs), and the mechanism responsible (a reuse substitution backed by a named Caprichio, with status *Applied*).

## Consequences

- `.escene` becomes the shared evidence artefact for agent-driven diagnosis, cross-target comparison, and closed-loop toolchain improvement.
- The proposed emoji assignments, evidence schemas, and agent retrieval mechanisms **remain open design decisions**, not established implementations. A follow-up ADR must fix the stable machine-readable identifiers, the evidence record schema (measurements, environmental context, attribution confidence), the applied-vs-identified substitution distinction, and a tokenizer benchmark before any compiler emits `.escene`.
- Platform compilers remain the natural producers; no existing crate emits `.escene` today, and this ADR creates no dependency changes.
