# romántico threats — GY-ROMANTICO/0.1

Status: a note on the in-process crate. This is not a penetration test and not a release-candidate sign-off. Date: 25 November 2026.

## Assets

The portable snapshot, the mutation journal, the pairing secret, and the identity of the single writer.

## In scope

Malformed decoder input. Replay, and a gap in the journal. Pairing consent and lockout. Authority checks on commit, grant, and activate. The four capability booleans. Execution mode staying out of session identity.

## Out of scope

A hostile host process. A network attacker, because the crate opens no socket. A pairing code stolen after consent. Device theft. TLS, because there is none. Quinta. Physical fault injection. The 15 P0 device gates.

## Controls that exist

ChaCha20-Poly1305 over HKDF-SHA256, keyed from the accepted pairing code. Five mismatches lock the offer. Grant clears the origin's mutate right before the successor activates. A lost post-grant snapshot leaves nobody able to commit. Canonical decoders reject unknown structure, a gap, and a non-canonical encoding. Buffers stop at the published limits. A diagnostic trace records outcomes and is not portable state.

## What this note does not claim

No coverage-guided fuzz campaign was run. The probe in the test suite flips a bounded window of real documents and checks that decoders return `Ok` or an `RMT-E` code. Latency and RAM were not measured. No certificate was verified, and none was ignored. An unauthorised device on a physical network was not rejected, because no network was opened.
