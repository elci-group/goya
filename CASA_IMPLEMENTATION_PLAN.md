# CASA — CSS-to-.gscene Semantic Transformation Infrastructure
## Implementation Plan (based on Directive GOYA-DIS-CASA-001)

---

## 1. Executive Summary

This plan defines the implementation of **CASA** (CSS-to-.gscene semantic transformation infrastructure) as specified in directive GOYA-DIS-CASA-001. CASA will be implemented as a Rust crate (`goya-casa`) within the Goya workspace, leveraging existing foundational crates.

**Current State**: Many supporting crates are already implemented (CSS parser, HTML tokenizer, FAIR, Chromatics, Cue, Scene Graphics, GVI family). The main `goya-casa` crate does not exist yet.

**Goal**: Create a production-ready CSS-to-.gscene transformation pipeline with four operational modes (Standalone, Contextual, Adaptive, Integrated) that integrates with the Disparates ecosystem.

---

## 2. Architecture Overview

### 2.1 Pipeline Stages (per Directive §4)

```
CSS → Ingestion → Cascade & Context Resolution → CasaIR
    → Spatial Transformation → Appearance Transformation
    → Behavioural Transformation → Adaptive Transformation
    → Native Optimisation (GVI) → Inquisition Validation → .gscene
```

### 2.2 Operational Modes (per Directive §5)

| Mode | Input | Output |
|------|-------|--------|
| **Standalone** | CSS | Reusable scene style definitions |
| **Contextual** | CSS + Structural tree | Context-resolved .gscene |
| **Adaptive** | CSS + Structural tree + Environmental constraints | Responsive .gscene |
| **Integrated** | CSS + Upstream application decomposition model | Complete presentation-layer transformation |

---

## 3. Crate Structure

```
/home/sal/goya/crates/goya-casa/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # Public API, re-exports
│   ├── ingestion/
│   │   ├── mod.rs
│   │   ├── parser.rs               # CSS parsing via goya-css-syntax
│   │   ├── resources.rs            # Font/asset ingestion
│   │   ├── imports.rs              # @import dependency resolution
│   │   └── shadow_dom.rs           # Shadow DOM encapsulation handling
│   ├── cascade/
│   │   ├── mod.rs
│   │   ├── engine.rs               # Cascade resolution (origin, specificity, layers)
│   │   ├── inheritance.rs          # Property inheritance resolution
│   │   ├── custom_properties.rs    # CSS custom property substitution
│   │   └── conditional.rs          # Media/container/supports condition evaluation
│   ├── ir/
│   │   ├── mod.rs
│   │   ├── casa_ir.rs              # CasaIR: canonical semantic model
│   │   ├── value_categories.rs     # Declared/Cascaded/Specified/Computed/Used/Actual/Conditional/Native
│   │   ├── dependency_graph.rs     # Property-level dependency graphs
│   │   └── serialization.rs        # Deterministic IR serialization
│   ├── context/
│   │   ├── mod.rs
│   │   ├── structural.rs           # DOM/scene tree binding
│   │   ├── selector_matching.rs    # Selector resolution against structure
│   │   └── computed_style.rs       # Per-element computed style resolution
│   ├── geometry/
│   │   ├── mod.rs
│   │   ├── proportional.rs         # Proportional geometry conversion (per §9)
│   │   ├── layout_systems/
│   │   │   ├── mod.rs
│   │   │   ├── block.rs            # Block layout
│   │   │   ├── flexbox.rs          # Flexbox
│   │   │   ├── grid.rs             # CSS Grid
│   │   │   ├── absolute.rs         # Absolute positioning
│   │   │   ├── fixed.rs            # Fixed positioning
│   │   │   ├── sticky.rs           # Sticky positioning
│   │   │   ├── table.rs            # Table layout
│   │   │   ├── float.rs            # Float layout
│   │   │   └── multicol.rs         # Multi-column layout
│   │   ├── dimensional.rs          # CSS units → proportional constraints
│   │   └── calc.rs                 # calc()/min()/max()/clamp() symbolic handling
│   ├── appearance/
│   │   ├── mod.rs
│   │   ├── colors.rs               # Color parsing and native mapping
│   │   ├── gradients.rs            # Linear/radial/conic gradient mapping
│   │   ├── borders.rs              # Border and border-radius mapping
│   │   ├── shadows.rs              # Box/text shadow mapping
│   │   ├── transforms.rs           # Transform mapping
│   │   ├── filters.rs              # Filter/blend mode mapping
│   │   ├── masks.rs                # Clip-path/mask mapping
│   │   ├── backgrounds.rs          # Background image/color mapping
│   │   └── deduplication.rs        # Style pooling and appearance deduplication
│   ├── typography/
│   │   ├── mod.rs
│   │   ├── fonts.rs                # Font family/fallback/variable font handling
│   │   ├── text_layout.rs          # Text flow, line breaking, bidi
│   │   ├── shaping.rs              # Glyph shaping integration
│   │   └── metrics.rs              # Intrinsic text dimensions
│   ├── animation/
│   │   ├── mod.rs
│   │   ├── keyframes.rs            # Keyframe parsing and classification
│   │   ├── easing.rs               # Easing function mapping
│   │   ├── interpolation.rs        # Interpolation semantics
│   │   ├── trajectory.rs           # GVI-A trajectory encoding
│   │   └── classification.rs       # Precomputation vs runtime classification
│   ├── adaptive/
│   │   ├── mod.rs
│   │   ├── media_queries.rs        # Media query evaluation and native mapping
│   │   ├── container_queries.rs    # Container query handling
│   │   ├── env_registry.rs         # Environmental condition registry (§13.1)
│   │   ├── breakpoint.rs           # Breakpoint economy (§13.2)
│   │   └── interaction_adaptation.rs # Platform-native interaction adaptation (§12.1)
│   ├── behavioural/
│   │   ├── mod.rs
│   │   ├── state_extraction.rs     # Interaction state extraction (hover, focus, etc.)
│   │   ├── state_space.rs          # State-space extraction (§11.1)
│   │   ├── explosion_prevention.rs # State explosion prevention (§11.2)
│   │   └── transitions.rs          # CSS transition handling
│   ├── resources/
│   │   ├── mod.rs
│   │   ├── vectorisation.rs        # CSS-generated graphics → native vector (§15)
│   │   ├── deduplication.rs        # Content-addressed resource deduplication (§15.1)
│   │   ├── containment.rs          # Resource containment (§15.2)
│   │   └── fonts.rs                # Font resource handling and licensing
│   ├── optimisation/
│   │   ├── mod.rs
│   │   ├── cost_model.rs           # Transformation cost model (§16)
│   │   ├── profiles.rs             # Optimisation profiles (Fidelity/Compact/Performance/Constrained/Balanced)
│   │   ├── resolution_model.rs     # Static vs adaptive resolution decision model
│   │   └── gvi_integration.rs      # GVI family integration (§11)
│   ├── emission/
│   │   ├── mod.rs
│   │   ├── gscene.rs               # .gscene emission
│   │   ├── manifest.rs             # Transformation manifest
│   │   ├── provenance.rs           # Source provenance tracking
│   │   └── compatibility.rs        # Compatibility report (C0-C4 classifications)
│   ├── validation/
│   │   ├── mod.rs
│   │   ├── inquisition.rs          # Inquisition integration (§22)
│   │   ├── correctness.rs          # Structural/semantic/geometric/appearance validation
│   │   └── certificates.rs         # CASA-S/G/A/B/R/D/X/P certificates
│   ├── diagnostics/
│   │   ├── mod.rs
│   │   ├── codes.rs                # Diagnostic codes (CASA-GEO-004 etc.)
│   │   ├── reporter.rs             # Machine-readable diagnostic output
│   │   └── explain.rs              # Conversion decision explanation
│   ├── modes/
│   │   ├── mod.rs
│   │   ├── standalone.rs           # Standalone mode implementation
│   │   ├── contextual.rs           # Contextual mode implementation
│   │   ├── adaptive.rs             # Adaptive mode implementation
│   │   └── integrated.rs           # Integrated mode (Delectable embedding)
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── commands.rs             # casa analyse/convert/validate/inspect/compare/benchmark/explain/capabilities
│   │   └── args.rs                 # CLI argument parsing
│   └── integration/
│       ├── mod.rs
│       ├── delectable.rs           # Delectable integration (§17.1)
│       └── resolver.rs             # Resolver integration (§17.2)
├── tests/
│   ├── unit/
│   ├── conformance/
│   ├── integration/
│   ├── differential/
│   ├── regression/
│   ├── security/
│   └── fixtures/
├── benches/
└── fixtures/
```

---

## 4. Dependencies

### 4.1 Workspace Dependencies (Already Available)

```toml
[dependencies]
goya-css-syntax = { path = "../goya-css-syntax" }
goya-html-tokenizer = { path = "../goya-html-tokenizer" }
goya-fair = { path = "../goya-fair" }
goya-disparate-electron = { path = "../goya-disparate-electron" }
goya-lang = { path = "../goya-lang" }
goya-scene-graphics = { path = "../goya-scene-graphics" }
goya-gvi = { path = "../goya-gvi" }
goya-gva = { path = "../goya-gva" }
goya-gru = { path = "../goya-gru" }
goya-graphics-ir = { path = "../goya-graphics-ir" }
goya-caprichios = { path = "../goya-caprichios" }
goya-chromatics = { path = "../goya-chromatics" }
goya-cue = { path = "../goya-cue" }
goya-ir = { path = "../goya-ir" }
goya-state = { path = "../goya-state" }
goya-runtime = { path = "../goya-runtime" }
goya-layout = { path = "../goya-layout" }
goya-romantico = { path = "../goya-romantico" }
goya-provenance = { path = "../goya-provenance" }
goya-change = { path = "../goya-change" }
goya-artifact = { path = "../goya-artifact" }
```

### 4.2 External Dependencies

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
thiserror = "1"
clap = { version = "4", features = ["derive"] }
cssparser = "0.29"        # For advanced CSS parsing (selectors, values)
selectors = "0.24"        # For selector matching
# Note: goya-css-syntax provides base tokenization; cssparser/selectors for higher-level
```

---

## 5. Phased Implementation

### Phase 0: Architectural Reconnaissance & Scaffolding (Week 1-2)
**Goal**: Establish crate structure, integrate with workspace, verify existing dependencies

- [ ] Create `goya-casa` crate with Cargo.toml
- [ ] Add to workspace members in root Cargo.toml
- [ ] Create module skeleton (all directories and mod.rs files)
- [ ] Verify all workspace dependencies build correctly
- [ ] Establish CI/test infrastructure
- [ ] Document integration points with existing crates

**Exit Criteria**: `cargo build -p goya-casa` succeeds; all modules compile

---

### Phase 1: Foundational Ingestion & CasaIR (Week 2-4)
**Goal**: CSS parsing, tokenization, declaration representation, CasaIR, diagnostics

- [ ] Implement `ingestion/parser.rs` - wrap goya-css-syntax for stylesheet parsing
- [ ] Implement `ingestion/resources.rs` - font/asset discovery and ingestion
- [ ] Implement `ingestion/imports.rs` - @import dependency resolution with cycle detection
- [ ] Implement `ingestion/shadow_dom.rs` - Shadow DOM encapsulation handling
- [ ] Define `ir/casa_ir.rs` - Canonical semantic model with all value categories
- [ ] Implement `ir/value_categories.rs` - Declared/Cascaded/Specified/Computed/Used/Actual/Conditional/Native
- [ ] Implement `ir/dependency_graph.rs` - Property-level dependency graphs
- [ ] Implement `ir/serialization.rs` - Deterministic IR serialization with schema versioning
- [ ] Implement `diagnostics/codes.rs` - Diagnostic code registry
- [ ] Implement `diagnostics/reporter.rs` - Machine-readable diagnostic output
- [ ] Unit tests for parsing, IR round-tripping

**Exit Criteria**: CSS → CasaIR round-trip with deterministic serialization; all parser conformance tests pass

---

### Phase 2: Cascade & Structural Resolution (Week 4-6)
**Goal**: Selectors, specificity, inheritance, custom properties, context binding

- [ ] Implement `cascade/engine.rs` - Full cascade resolution (origin, importance, specificity, layers, scoping, source order)
- [ ] Implement `cascade/inheritance.rs` - Property inheritance and defaulting
- [ ] Implement `cascade/custom_properties.rs` - Custom property substitution with cycle detection
- [ ] Implement `cascade/conditional.rs` - Media/container/supports condition evaluation
- [ ] Implement `context/structural.rs` - DOM/scene tree representation and binding
- [ ] Implement `context/selector_matching.rs` - Selector matching against structure (using `selectors` crate)
- [ ] Implement `context/computed_style.rs` - Per-element computed style resolution
- [ ] Integrate with `goya-fair` for structural context from Delectable
- [ ] Cascade conformance tests (specificity, precedence, inheritance)

**Exit Criteria**: Full cascade resolution with structural context; selector matching validated against test suite

---

### Phase 3: Native Geometry & Appearance (Week 6-10)
**Goal**: First usable CSS-to-.gscene pipeline with core layout, proportional conversion, appearance

- [ ] Implement `geometry/proportional.rs` - Proportional geometry conversion (§9.1)
- [ ] Implement `geometry/dimensional.rs` - CSS units → proportional constraints
- [ ] Implement `geometry/calc.rs` - calc()/min()/max()/clamp() symbolic handling
- [ ] Implement layout systems (progressive):
  - [ ] `layout_systems/block.rs` - Block layout (flow, margins, containment)
  - [ ] `layout_systems/flexbox.rs` - Flexbox (axis alignment, wrapping, growth/shrinkage)
  - [ ] `layout_systems/grid.rs` - Grid (tracks, placement, spanning, fractional allocation)
  - [ ] `layout_systems/absolute.rs` - Absolute positioning
  - [ ] `layout_systems/fixed.rs` - Fixed positioning
  - [ ] `layout_systems/sticky.rs` - Sticky positioning
  - [ ] `layout_systems/table.rs` - Table layout
  - [ ] `layout_systems/float.rs` - Float layout
  - [ ] `layout_systems/multicol.rs` - Multi-column layout
- [ ] Implement `appearance/colors.rs` - Color parsing and native mapping
- [ ] Implement `appearance/gradients.rs` - Linear/radial/conic gradient mapping
- [ ] Implement `appearance/borders.rs` - Border and border-radius mapping
- [ ] Implement `appearance/shadows.rs` - Box/text shadow mapping
- [ ] Implement `appearance/transforms.rs` - Transform mapping
- [ ] Implement `appearance/filters.rs` - Filter/blend mode mapping
- [ ] Implement `appearance/masks.rs` - Clip-path/mask mapping
- [ ] Implement `appearance/backgrounds.rs` - Background image/color mapping
- [ ] Implement `appearance/deduplication.rs` - Style pooling and appearance deduplication
- [ ] Implement `emission/gscene.rs` - .gscene emission
- [ ] Implement `emission/manifest.rs` - Transformation manifest
- [ ] Implement `emission/provenance.rs` - Source provenance tracking
- [ ] Layout conformance tests

**Exit Criteria**: Basic CSS → .gscene conversion working for core layout and appearance properties

---

### Phase 4: Adaptive Presentation (Week 10-13)
**Goal**: Responsive and conditional semantics, media queries, container queries, interaction states

- [ ] Implement `adaptive/media_queries.rs` - Media query evaluation and native mapping
- [ ] Implement `adaptive/container_queries.rs` - Container query handling
- [ ] Implement `adaptive/env_registry.rs` - Environmental condition registry (§13.1)
- [ ] Implement `adaptive/breakpoint.rs` - Breakpoint economy (§13.2)
- [ ] Implement `adaptive/interaction_adaptation.rs` - Platform-native interaction adaptation (§12.1)
- [ ] Implement `behavioural/state_extraction.rs` - Interaction state extraction (hover, focus, active, etc.)
- [ ] Implement `behavioural/state_space.rs` - State-space extraction (§11.1)
- [ ] Implement `behavioural/explosion_prevention.rs` - State explosion prevention (§11.2)
- [ ] Implement `behavioural/transitions.rs` - CSS transition handling
- [ ] Implement `animation/keyframes.rs` - Keyframe parsing and classification
- [ ] Implement `animation/easing.rs` - Easing function mapping
- [ ] Implement `animation/interpolation.rs` - Interpolation semantics
- [ ] Implement `animation/trajectory.rs` - GVI-A trajectory encoding
- [ ] Implement `animation/classification.rs` - Precomputation vs runtime classification

**Exit Criteria**: Responsive CSS converts to adaptive .gscene with conditional constraints; interaction states preserved

---

### Phase 5: Graphics Optimisation & GVI Integration (Week 13-16)
**Goal**: GVI family integration, finite-state discovery, style pooling, trajectory extraction

- [ ] Implement `optimisation/cost_model.rs` - Transformation cost model (§16)
- [ ] Implement `optimisation/profiles.rs` - Optimisation profiles (Fidelity/Compact/Performance/Constrained/Balanced)
- [ ] Implement `optimisation/resolution_model.rs` - Static vs adaptive resolution decision model
- [ ] Implement `optimisation/gvi_integration.rs` - GVI family integration (§11)
- [ ] Implement `resources/vectorisation.rs` - CSS-generated graphics → native vector (§15)
- [ ] Implement `resources/deduplication.rs` - Content-addressed resource deduplication (§15.1)
- [ ] Implement `resources/containment.rs` - Resource containment (§15.2)
- [ ] Implement `resources/fonts.rs` - Font resource handling and licensing
- [ ] Implement `typography/fonts.rs` - Font family/fallback/variable font handling
- [ ] Implement `typography/text_layout.rs` - Text flow, line breaking, bidi
- [ ] Implement `typography/shaping.rs` - Glyph shaping integration
- [ ] Implement `typography/metrics.rs` - Intrinsic text dimensions

**Exit Criteria**: GVI integration demonstrates semantic equivalence and measurable advantages on eligible workloads

---

### Phase 6: Ecosystem Integration (Week 16-19)
**Goal**: Disparates and Delectable integration, embeddable interfaces

- [ ] Implement `modes/standalone.rs` - Standalone mode
- [ ] Implement `modes/contextual.rs` - Contextual mode
- [ ] Implement `modes/adaptive.rs` - Adaptive mode
- [ ] Implement `modes/integrated.rs` - Integrated mode (Delectable embedding)
- [ ] Implement `integration/delectable.rs` - Delectable integration (§17.1)
- [ ] Implement `integration/resolver.rs` - Resolver integration (§17.2)
- [ ] Implement `emission/compatibility.rs` - Compatibility report (C0-C4 classifications)
- [ ] Implement `validation/inquisition.rs` - Inquisition integration (§22)
- [ ] Implement `validation/correctness.rs` - Structural/semantic/geometric/appearance validation
- [ ] Implement `validation/certificates.rs` - CASA-S/G/A/B/R/D/X/P certificates
- [ ] Implement `diagnostics/explain.rs` - Conversion decision explanation

**Exit Criteria**: Delectable can delegate CSS transformation; converted scenes execute on Goya targets

---

### Phase 7: CLI, Security & Enterprise Qualification (Week 19-22)
**Goal**: Command-line interface, security hardening, conformance, performance

- [ ] Implement `cli/commands.rs` - All CLI commands (analyse/convert/validate/inspect/compare/benchmark/explain/capabilities)
- [ ] Implement `cli/args.rs` - CLI argument parsing
- [ ] Security hardening:
  - [ ] Bounded, validated parsing
  - [ ] Cycle detection and depth restrictions for imports
  - [ ] Configurable evaluation budgets for selector complexity
  - [ ] Allocation and resource limits
  - [ ] Sandboxed resource resolution
  - [ ] Deny-by-default network policy
  - [ ] Integrity verification and dependency pinning
- [ ] Conformance testing:
  - [ ] Web Platform Tests integration
  - [ ] Differential testing against browser references
  - [ ] Property-based testing
  - [ ] Fuzz testing
  - [ ] Metamorphic testing
- [ ] Performance benchmarking:
  - [ ] Benchmark corpus implementation (§23.2)
  - [ ] Cold/warm transformation time
  - [ ] Memory residency, binary size
  - [ ] Frame time, interaction latency

**Exit Criteria**: All mandatory release gates (CASA-G01 through CASA-G12) satisfied

---

## 6. Release Gates (per Directive §26)

| Gate | Description | Phase |
|------|-------------|-------|
| CASA-G01 | Repository integration | 0 |
| CASA-G02 | Parser conformance | 1 |
| CASA-G03 | Cascade correctness | 2 |
| CASA-G04 | Proportional compliance | 3 |
| CASA-G05 | Semantic fidelity | 3 |
| CASA-G06 | Determinism | 1 |
| CASA-G07 | Graphics correctness | 5 |
| CASA-G08 | Security qualification | 7 |
| CASA-G09 | Cross-platform execution | 6 |
| CASA-G10 | Delectable integration | 6 |
| CASA-G11 | Performance qualification | 7 |
| CASA-G12 | Documentation | 7 |

---

## 7. Testing Strategy

### 7.1 Test Families (per Directive §23.1)

| Test Family | Implementation |
|-------------|----------------|
| Unit testing | Per-module unit tests in `tests/unit/` |
| Parser conformance | css-parsing-tests suite in `tests/conformance/parser/` |
| Cascade conformance | Custom cascade test suite in `tests/conformance/cascade/` |
| Layout conformance | Layout test suite in `tests/conformance/layout/` |
| Differential testing | Compare native output with browser references in `tests/differential/` |
| Property-based testing | Proptest for semantic combinations in `tests/property/` |
| Fuzz testing | cargo-fuzz for malformed input in `tests/fuzz/` |
| Metamorphic testing | Equivalent source transformations in `tests/metamorphic/` |
| Golden fixtures | Visual/binary regression in `tests/regression/` |
| Cross-platform testing | Target consistency in `tests/cross_platform/` |
| Performance testing | Benchmarks in `benches/` |
| Security testing | Resource/execution containment in `tests/security/` |

### 7.2 Benchmark Corpus (per Directive §23.2)

- Static component library
- Responsive dashboard
- Animated interface
- Complex CSS Grid application
- Text-intensive document
- CSS-heavy Electron application (Delectable integration)
- Vector-rich CSS illustration
- Television interface (remote-oriented)
- Constrained mobile interface
- Dynamically styled application

---

## 8. Integration Points with Existing Crates

| CASA Component | Existing Crate | Integration |
|----------------|----------------|-------------|
| CSS parsing | `goya-css-syntax` | Direct dependency |
| HTML parsing | `goya-html-tokenizer` | For inline styles |
| Structural context | `goya-fair` | FAIR/2 consumption |
| Electron extraction | `goya-disparate-electron` | Upstream provider |
| .gscene emission | `goya-scene-graphics` | Graphic lowering |
| GVI/GVI-D/GVI-A/GVI-L | `goya-gvi`, `goya-gva`, `goya-gru` | Graphics optimization |
| Caprichio negotiation | `goya-caprichios` | Target capability resolution |
| Chromatics | `goya-chromatics` | Adaptive input allocation |
| Colour Cueing | `goya-cue` | Indexed paint, damage accounting |
| Proportional geometry | `goya-lang::proportional` | Geometry doctrine |
| Scene representation | `goya-lang::scene` | .gscene AST |
| IR lowering | `goya-ir` | Intermediate representation |
| State management | `goya-state` | Transactional state |
| Layout | `goya-layout` | Layout primitives |
| Continuity | `goya-romantico` | Replica/snapshot/grant |
| Provenance | `goya-provenance` | Artefact provenance |
| Graphics IR | `goya-graphics-ir` | Graphic lowering |

---

## 9. Cargo.toml for goya-casa

```toml
[package]
name = "goya-casa"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false
description = "CASA — CSS-to-.gscene semantic transformation infrastructure (GOYA-DIS-CASA-001)"

[lib]
path = "src/lib.rs"

[[bin]]
name = "casa"
path = "src/cli/main.rs"

[dependencies]
# Workspace dependencies
goya-css-syntax = { path = "../goya-css-syntax" }
goya-html-tokenizer = { path = "../goya-html-tokenizer" }
goya-fair = { path = "../goya-fair" }
goya-disparate-electron = { path = "../goya-disparate-electron" }
goya-lang = { path = "../goya-lang" }
goya-scene-graphics = { path = "../goya-scene-graphics" }
goya-gvi = { path = "../goya-gvi" }
goya-gva = { path = "../goya-gva" }
goya-gru = { path = "../goya-gru" }
goya-graphics-ir = { path = "../goya-graphics-ir" }
goya-caprichios = { path = "../goya-caprichios" }
goya-chromatics = { path = "../goya-chromatics" }
goya-cue = { path = "../goya-cue" }
goya-ir = { path = "../goya-ir" }
goya-state = { path = "../goya-state" }
goya-runtime = { path = "../goya-runtime" }
goya-layout = { path = "../goya-layout" }
goya-romantico = { path = "../goya-romantico" }
goya-provenance = { path = "../goya-provenance" }
goya-change = { path = "../goya-change" }
goya-artifact = { path = "../goya-artifact" }

# External dependencies
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
thiserror = "1"
clap = { version = "4", features = ["derive"] }
cssparser = "0.29"
selectors = "0.24"
# For security: resource limits
arc-swap = "1"
parking_lot = "0.12"

[dev-dependencies]
# Testing
proptest = "1"
criterion = "0.5"
# CSS conformance
css-parsing-tests = { git = "https://github.com/w3c/css-parsing-tests" }

[profile.release]
debug = true
```

---

## 10. Next Steps

1. **Review this plan** - Confirm approach, priorities, and timeline
2. **Approve Phase 0** - Begin crate scaffolding and workspace integration
3. **Establish CI** - Configure build/test pipeline for new crate
4. **Begin Phase 1** - Foundational ingestion and CasaIR implementation

---

## 11. Open Questions

1. **Selector matching**: ✅ **Resolved** — use servo's `selectors` crate (already declared in `goya-casa/Cargo.toml`, version 0.24 pinned in `Cargo.lock`). Rationale: spec-correct specificity/pseudo-class handling without reimplementing CSS matching; supply-chain cost already accepted in `deny.toml` (fxhash advisory waived for `selectors (goya-casa)`). In-house matching is deferred unless profile-size constraints emerge.
2. **CSS values parsing**: ✅ **Resolved** — use `cssparser` (0.29, already declared) for the primitive token/value layer required by `selectors`; keep `goya-css-syntax` (zero-dep, in-house) as the structural parse layer and extend it for high-level value types. Casa consumes both: cssparser at the selector/value boundary, goya-css-syntax everywhere else.
3. **Font shaping integration**: Which Goya typography crate handles shaping? Need to identify.
4. **Inquisition integration**: What's the current Inquisition API for validation?
5. **Delectable API**: What's the exact interface Delectable expects for CSS transformation delegation?
6. **Performance profiles**: Should optimisation profiles be compile-time or runtime configuration?

---

*Plan version: 1.0*  
*Based on directive: GOYA-DIS-CASA-001 v1.0*  
*Date: 2026-11-26*