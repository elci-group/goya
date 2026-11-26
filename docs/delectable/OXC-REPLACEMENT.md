# Oxc Replacement Plan

**Status**: Planned (high priority per roadmap, owner decision 26 Nov 2026)
**Directive**: GY-ENT-001 / DEL-TD-001
**Target**: Replace Oxc 0.146 with Goya-owned JavaScript/JSX/TypeScript parser in `goya-disparate-electron`

---

## 1. Current Oxc Usage (Scope)

### Direct Dependencies (6 crates, pinned =0.146.0)
| Crate | Purpose |
|-------|---------|
| `oxc_allocator` | AST node allocation |
| `oxc_ast` | AST node definitions |
| `oxc_ast_visit` | Visitor pattern for AST traversal |
| `oxc_parser` | Parser entry point |
| `oxc_span` | Source spans, SourceType |
| `oxc_syntax` | Scope analysis, module resolution |

### What the Extractor Uses
From `crates/goya-disparate-electron/src/js.rs`:

1. **Parsing**: `Parser::new(...).parse()` with `SourceType` per file extension
2. **AST Visitor**: `Visit` trait impl (`Walker`) traverses entire program
3. **Span Tracking**: `oxc_span::Span` → exact byte offsets for provenance
4. **Scope Flags**: `ScopeFlags` for function depth tracking
5. **Node Types**: Full `oxc_ast::ast::*` — expressions, statements, declarations, patterns
6. **Diagnostics**: Parse errors → `PARSE_FAILED` elements with error message
7. **SourceType Detection**: `.js/.cjs/.mjs/.jsx/.ts/.cts/.mts/.tsx` → correct parser mode

### Key Extraction Logic Built on Oxc
| Feature | Oxc Dependency |
|---------|----------------|
| Import/require resolution | `Visit` on `ImportDeclaration`, `CallExpression` (require) |
| Binding tracking | `VariableDeclarator` visitor + `BindingPattern` |
| DOM API detection | `CallExpression` on `document.getElementById`, `querySelector` |
| IPC detection | `CallExpression` chain resolution (`electron.ipcMain.handle`) |
| Window lifecycle | `NewExpression` for `new BrowserWindow` |
| Event handlers | `CallExpression` for `addEventListener`, assignment to `on*` |
| Preload bridge | `CallExpression` for `contextBridge.exposeInMainWorld` |
| State stores | Module-scope `let`/`var` assigned inside functions |
| Logic units | Top-level `FunctionDeclaration` |
| Dynamic imports | `ImportExpression` visitor |
| Bridge calls | `window.<key>.<member>()` pattern at handler_depth==0 |

---

## 2. Language Scope (What Must Be Supported)

### Required (Current Coverage)
| Language Feature | Oxc Support | Notes |
|------------------|-------------|-------|
| ES2022 syntax | ✅ | Parser target |
| CommonJS `require` | ✅ | Including destructured |
| ESM `import`/`export` | ✅ | Static and dynamic |
| JSX | ✅ | `.jsx`/`.tsx` files |
| TypeScript | ✅ | Types stripped, syntax parsed |
| Decorators | ⚠️ | Stage 3, limited use in fixtures |
| Class fields | ✅ | |
| Optional chaining | ✅ | `?.` |
| Nullish coalescing | ✅ | `??` |
| Private fields | ✅ | `#field` |
| Top-level await | ✅ | |

### Fixture Conformance Corpus
Must parse all 5 fixtures without `PARSE_FAILED`:
1. `electron-counter` — vanilla JS (CommonJS)
2. `electron-react-counter` — React + TSX + hooks
3. `electron-file-explorer` — React + TSX + fs/dialog
4. `electron-text-editor` — React + TSX + large text
5. `electron-task-manager` — React + TSX + array `.map()`
6. `electron-markdown-preview` — React + TSX + `marked` import

### Explicitly NOT Required (Out of Scope)
- Flow types
- Experimental decorators (stage 1/2)
- JSDoc type annotations (beyond parsing)
- Non-standard syntax (e.g., Svelte, Vue SFC)

---

## 3. Architecture: Goya-Owned Parser Crate

### Proposed Crate Structure
```
crates/goya-javascript/
├── Cargo.toml
├── src/
│   ├── lexer/          # Tokenizer (may reuse goya-html-tokenizer patterns)
│   ├── parser/         # Recursive descent / Pratt parser
│   ├── ast/            # AST node definitions (Goya-owned, FAIR-aligned)
│   ├── visit/          # Visitor trait (like oxc_ast_visit)
│   ├── span/           # Source spans, SourceType (like oxc_span)
│   ├── scope/          # Scope analysis (like oxc_syntax)
│   ├── diagnostics/    # Parse error reporting
│   └── mod.rs          # Public API: parse(text, SourceType) -> (Program, Diagnostics)
└── tests/
    ├── conformance/    # Test262 subset + fixture round-trips
    └── fair_parity/    # Compare FAIR output vs Oxc
```

### Design Principles
1. **FAIR-aligned AST** — Node kinds map directly to extraction needs (`IpChannel`, `PreloadBridge`, etc.)
2. **Exact spans** — Every node carries `Span { start, end }` for provenance
3. **Deterministic** — No arena allocation randomness; same input → same AST bytes
4. **Error recovery** — Continue parsing after errors; emit `PARSE_FAILED` elements
5. **No scope analysis in parser** — Separate pass (like `oxc_syntax`), extraction does binding resolution

---

## 4. Staged Integration Plan

### Stage 0: Design & Spec (Week 1-2)
- [ ] Define AST node kinds (minimal, extraction-focused)
- [ ] Define `SourceType` enum matching Oxc
- [ ] Define parser API signature
- [ ] Write conformance test harness

### Stage 1: Lexer + Parser Core (Week 3-6)
- [ ] Tokenizer for JS/TS/JSX
- [ ] Pratt parser for expressions
- [ ] Statement/declaration parsing
- [ ] TypeScript type annotation skipping
- [ ] JSX element parsing
- [ ] Parse error recovery

### Stage 2: AST + Visitor (Week 7-8)
- [ ] AST node definitions with spans
- [ ] Visitor trait + walk functions
- [ ] SourceType-driven parser entry point

### Stage 3: Scope Analysis (Week 9-10)
- [ ] Binding resolution pass
- [ ] Scope flags for function depth
- [ ] Module import/export tracking

### Stage 4: Diagnostics (Week 11)
- [ ] Structured parse errors with spans
- [ ] `PARSE_FAILED` element generation

### Stage 5: Integration & Parity (Week 12-14)
- [ ] Swap Oxc → goya-javascript in `goya-disparate-electron`
- [ ] Run all 6 fixtures, compare FAIR output
- [ ] Document every difference (must be accounted for)
- [ ] Zero `PARSE_FAILED` on fixtures
- [ ] Performance parity (within 2x Oxc)

### Stage 6: Cleanup (Week 15)
- [ ] Remove Oxc dependencies from workspace
- [ ] Update Cargo.lock
- [ ] Update README/roadmap

---

## 5. Conformance Corpus

### Primary: Fixture Round-Trips
| Fixture | Files | Must Parse | FAIR Parity |
|---------|-------|------------|-------------|
| electron-counter | 3 JS | ✅ | ✅ |
| electron-react-counter | 4 TSX + 1 JS | ✅ | ✅ |
| electron-file-explorer | 4 TSX + 1 JS | ✅ | ✅ |
| electron-text-editor | 4 TSX + 1 JS | ✅ | ✅ |
| electron-task-manager | 4 TSX + 1 JS | ✅ | ✅ |
| electron-markdown-preview | 4 TSX + 1 JS | ✅ | ✅ |

### Secondary: Test262 Subset
- ES2022 core syntax
- JSX transform output
- TypeScript compiler test cases (syntax only)

### Tertiary: Property-Based
- Random valid JS programs → parse → serialize → parse = identical AST

---

## 6. Validation Gates (Must Pass Before Switch)

| Gate | Criteria |
|------|----------|
| **G1: Parse All Fixtures** | 0 `PARSE_FAILED` on 6 fixtures |
| **G2: FAIR Parity** | Canonical FAIR JSON identical to Oxc (or documented differences) |
| **G3: Element Count Parity** | ±5% element count per kind per file |
| **G4: Relationship Parity** | Identical relationship graph (subject/relation/object/basis) |
| **G5: Performance** | Parse time ≤ 2× Oxc on largest bundle (180 KB) |
| **G6: Diagnostics** | Error messages at least as precise (line/column) |

### Difference Accounting
Every FAIR difference must be categorised:
| Category | Action |
|----------|--------|
| Benign (whitespace, ordering) | Accept |
| More precise (better span, extra node) | Accept, document |
| Less precise (missing node, wider span) | Fix before switch |
| Semantic (different kind/basis) | Fix before switch |

---

## 7. Dependencies & Risks

### Blockers
- None — independent workstream

### Risks
| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Parser bugs on complex TSX | Medium | Extensive fixture testing, fuzzing |
| Scope analysis differences | Medium | Direct port Oxc's algorithm initially |
| Performance regression | Low | Profile early, optimise hot paths |
| Maintenance burden | Medium | Keep AST minimal, extraction-focused |

### What Stays
- `goya-html-tokenizer` (HTML)
- `goya-css-syntax` (CSS)
- `goya-fair` (FAIR contract)
- `goya-disparate-electron` logic (relate, emit, topology)

---

## 8. Decision Points

| Decision | When | Options |
|----------|------|---------|
| Parser algorithm | Stage 1 | Pratt / Recursive descent / PEG |
| AST granularity | Stage 2 | Minimal (extraction-only) vs Full (spec-compliant) |
| TypeScript handling | Stage 1 | Strip types / Parse types as comments / Full type AST |
| Error recovery strategy | Stage 1 | Panic mode / Synchronisation points / Best effort |

---

## 9. Timeline Estimate

| Phase | Duration | Cumulative |
|-------|----------|------------|
| Design & Spec | 2 weeks | 2 weeks |
| Lexer + Parser Core | 4 weeks | 6 weeks |
| AST + Visitor | 2 weeks | 8 weeks |
| Scope Analysis | 2 weeks | 10 weeks |
| Diagnostics | 1 week | 11 weeks |
| Integration & Parity | 3 weeks | 14 weeks |
| Cleanup | 1 week | **15 weeks** |

---

## 10. Success Criteria

**Phase 4 unblocked?** No — this is independent.

**Done when:**
1. ✅ `goya-disparate-electron` builds with `goya-javascript` only
2. ✅ All 6 fixtures parse with 0 `PARSE_FAILED`
3. ✅ FAIR output matches Oxc on all fixtures (documented differences only)
4. ✅ Oxc dependencies removed from workspace `Cargo.lock`
5. ✅ Roadmap updated: "Oxc replacement complete"

---

**Owner Approval Required**: Before Stage 1 implementation begins — confirm language scope, parser algorithm, and AST granularity decisions.