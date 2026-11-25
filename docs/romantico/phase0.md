# romántico Phase 0 — repository baseline

Status: **in progress**. Date: 25 November 2026. Contract under test: `GY-ROMANTICO/0.1`.

This note reconciles the romántico directive with the Goya tree on `nebula/v0.1`. It is not a claim that a game has moved between a television and a phone.

## What the tree actually contains

| Directive assumption | Repository fact |
| --- | --- |
| Goya Pairs is the reference application | No Pairs scene or host exists. `apps/` holds calculator, calendar, chess, clock, file manager, mail, messenger, music player, and ZCode fixtures. |
| `romantico { ... }` and `state GameSession` are language | `.gscene` has no continuity block and no state-class annotations. `goya-lang` still admits layout, gestures, and bounded D1/D2. Those keywords were not added. |
| `GY-STATE/1` is the portable store | `goya-state` is an in-process `HashMap` of `Int` / `Text` / `Bool`. One writer, one revision counter, no canonical bytes, no journal of semantic operations. The resolver deliberately does not seed it, because `Store::commit` would publish a different revision from `GY-D1-APP/1`. |
| webOS and Android 9 run the same source | The root workspace has no webOS host. `@platform webos` is a scene-token exception. `goya-resolver` rejects target `webos`. Aragon's locked Android project uses `minSdk` 23 and was qualified on API 23 and API 35 emulators, not on a named Android 9 device. |
| Resolver joins a compiled session | `GY-RESOLVER/0.1` loads one admitted scene per instance. Two instances from one application do not share state. |
| A secure WebSocket host is available | No WebSocket client is part of this crate. `Cargo.lock` has `sha2` and does not have an AEAD or CBOR crate. |

The nested `arachnid/` tree is not a member of the root workspace and was not treated as evidence of a webOS package.

## Decision that follows

The first implementation is a std-and-`sha2` crate, `goya-romantico`, that proves semantic continuity in one process. It does not pretend to be the device demonstration.

Portable state is a separate record from `GY-STATE/1`. Linking the crate is the opt-in. `goya-hello` does not depend on it, so a continuity-disabled host does not take the codec, the journal, or the pairing checks.

## What `GY-ROMANTICO/0.1` does

- Canonical RCB/1 encoding, SHA-256 fingerprints, and schema manifests.
- Atomic mutations, duplicate rejection, gap rejection, and a bounded journal.
- Transferable single-writer handoff with epoch fencing. A checkpoint that disagrees with its authority record admits no writer.
- Goya Pairs as a pure reducer: shared deck, authoritative score, resumable reveal, local viewport, private secret. Derived completion is computed, not stored.
- Pairing consent, attempt limit, and expiry on a caller-supplied tick.
- Surface roles. A controller may submit an intent. It cannot commit.
- Transport admission that refuses any adapter which does not report authenticated encryption. No adapter in this slice makes that report.

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 35 tests on this baseline. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed after formatting. Library code does not reference `std::net` or `std::fs`. Phase 1 is recorded in `phase1.md`.

## What this slice does not do

- No `.gscene` syntax, so RMT-001 is not met.
- No webOS package, no Android APK, no shared source hash across devices.
- No authenticated encrypted transport, so RMT-005 is not met. Certificate verification is not disabled; there is no TLS client.
- No CBOR comparison and no latency, RAM, or size measurement. The budgets in the directive stay targets.
- No compiled-to-interpreted session. RMT-T016 was not run.
- No Quinta inspector.

The P0 acceptance checklist in the directive remains entirely open. Success is still: the same Goya Pairs source starts on webOS, continues on Android 9, drops the originating process, and comes back without a semantic gap. This slice is the state contract that demonstration has to sit on.

## Next slice

Phase 1, the portable-state gate, is recorded in `phase1.md`. Stable field identity and snapshot bytes were checked there. The items below are still ahead of any device demonstration.

1. The in-process AEAD channel is recorded in `phase2.md`. The webOS–Android half of that exit gate is still open.
2. Measure the actual webOS execution environment before choosing a session host. Until then, `select_discovery` keeps the manual endpoint even if a caller claims multicast or a rendezvous service.
3. Revise the scene-language contract before adding a `romantico` keyword. The resolver must consume the same schema bytes as this crate, not a second store.
