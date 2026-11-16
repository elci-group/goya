# MAJA-ADR-001: Frampton boundary

Status: superseded for standalone web output by [ADR-002](adr-002-browser-native.md), 2026-11-15. The evidence below remains relevant to optional Frampton integration; it does not approve a browser interface or remote integration contract.

The local checkout `/home/sal/frampton`, origin `git@github.com:elci-group/frampton.git`, was inspected at HEAD `fcce0274fd7aa3f320fff0d9c3da917144b2fb93` (workspace version 0.1.8). The committed README and HIR were checked with `git show HEAD:...`; unrelated dirty files do not underpin those observations. The README describes an experimental server application language. `crates/frampton-hir/src/lib.rs` models state, functions, jobs and HTTP routes; `examples/counter.frampton` is server-oriented; `crates/frampton-runtime/README.md` describes a Rust-hosted effects runtime. Searches of inspected source and docs found no relevant browser, WebAssembly, JavaScript, HTML or CSS target interface. Goya's workspace and lockfile contain no Frampton dependency.

| Capability | Observed disposition |
|---|---|
| Presentation nodes, properties and layout | No browser mapping API identified |
| Browser events and lifecycle | No interface identified |
| Browser packaging and execution | No documented target identified |
| Effects | Rust-hosted runtime; browser capability mapping unverified |
| Equivalence and dependency integrity | Not tested; commit identified but no Maja dependency lock approved |

Keep a typed, versioned adapter boundary between admitted presentation and a future verified Frampton interface. Do not invent mappings or claim integration. A build requiring Frampton must fail explicitly until its authoritative interface, immutable version, browser execution and packaging model, compatibility and deterministic behavior are verified. Independent admission, fixture design and contracts may proceed. M1 and M2 remain blocked. This evidence does not prove that every possible integration is impossible. If no suitable interface exists, the integration authority must approve an architectural change; no silent substitution is permitted.
