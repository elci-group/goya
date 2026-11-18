# Quitasol Phase 0 — Intent primitive

Status: **proposed, not ratified**. Implementation is experimental; it does
not admit, ratify or freeze the intent profile. Design record:
[intents.md](intents.md); contract: [../contracts.md](../contracts.md)
(GY-INTENT/1-draft).

## What phase 0 delivers

- `@intent` parser arm in `crates/goya-lang` (compatibility parser only);
  strict GSSH/1/2/3 reject `@intent` with `GY-E1010`.
- `IntentDecl` side-collection on the goya-lang `Scene`; `goya-ir` untouched
  (GY-IR/1 frozen).
- `goya_lang::intents` validation with pinned grammar
  `quitasol.utterance.v0`; diagnostics `GY-E1011`.
- `quitasol-semantic::compile` emits a deterministic versioned intent
  manifest (contract QUITASOL-INTENT-IR/1-draft, profile
  `quitasol.intent.v0`, per-intent lowering tier `unresolved`).
- Corpus/tests: goya-lang intent corpus with mutation sweep; strict-reject
  fixture for all three GSSH profiles.

## Pinned identifiers

| Identifier | Value |
|---|---|
| Utterance grammar | `quitasol.utterance.v0` |
| Manifest contract | `QUITASOL-INTENT-IR/1-draft` |
| Manifest profile | `quitasol.intent.v0` |
| Strict rejection | `GY-E1010` |
| Validation diagnostics | `GY-E1011` |

## Q0 follow-up

The roadmap Q0 capability audit is recorded in
[q0-capability-audit.md](q0-capability-audit.md) with its evidence record at
[programme/quitasol-q0.json](programme/quitasol-q0.json). It re-anchors the
[intents.md](intents.md) §4 lowering map: Media Session transport and app
launch are the only self-serve Fire OS voice tiers; VSK is closed to new
partners; in-app speech recognition and raw microphone access are
unavailable to third-party apps; the Vega third-party voice API contract is
undocumented.

## BLOCKED (no evidence, not claimed)

| Item | Status |
|---|---|
| Alexa / VSK lowering | no evidence, not claimed |
| Vega OS backend | no evidence, not claimed |
| Runtime confirmation (Confirmation Engine) | no evidence, not claimed |
| OEM capability resolver | no evidence, not claimed |
| Owners / ratification | no evidence, not claimed |
| Appstore policy audit | no evidence, not claimed |

## Verification

```
cargo test -p goya-lang --offline
cargo test -p quitasol-semantic --offline
cargo test -p nevada --offline
```

Q1 extends `quitasol-semantic` with capability resolution over Nevada; see
[phase1.md](phase1.md).
