# Nevada threat model — phase-0 subsystem slice

Status: scope note for phase 0 (NEVADA-1). Not a completed security
review. Nevada is a proposed Android TV compiler target; this document
covers the four library modules that exist today
(`crates/nevada`: dcr.rs, profiles.rs, translate.rs, focus.rs) and names
the threats the later phases must address before any claim of TV
conformance.

## Assets

The authored scene (future), the capability census and OEM profiles, the
translation table and conformance reports, the focus graph, and — in
later phases — the generated leanback artefact and its capability claims.
The compiler's diagnostic output can quote authored identifiers and must
not become an injection surface.

## Adversaries considered

A dishonest or misconfigured device census (the caller-supplied
`DeviceProfile`), a spoofed OEM profile id, an OEM caprichio that lies
about hardware, a malicious scene that hides intent loss behind
translation, and accidental drift between the mirrored capability models.
Network attackers, the Android TV platform itself, OTA update channels
and voice-service backends are outside this slice because Nevada compiles
none of them yet.

## Threats and controls

### Device profile spoofing

The DCR trusts its census entirely: `dcr::resolve` never validates a
census against reality, and `profiles::lookup` validates only that an id
matches a registered static profile, not that the profile's contents are
true of any device. A caller (or, later, a compromised census source) can
claim a D-pad, a microphone, or native pointer events the device lacks.
Mitigations in this slice: lookup is exact-id only, fail-closed, with no
partial matches and no implicit defaults (`LookupError::UnknownProfile`),
so a typo cannot silently select a neighbouring device class. Required
later mitigations: attestation of census provenance (who authored the OEM
profile and on what evidence), and the runtime capability re-resolution
contract (gap ledger N0-RERESOLVE) so a compile-time profile is never
reported as a device guarantee.

### Capability detection dishonesty — presence ≠ usability

This is the spec's key distinction and the model's central honesty rule:
an accelerometer present in the census is not a usable pointer.
`MotionOutput::None` (the default) means the sensor emits nothing the
application can consume, and `resolve` enables neither `motion_gestures`
nor `spatial_pointer` when the output path is absent — tests pin this
(`motion_sensor_without_output_is_not_usable`). The residual risk is at
the census boundary: a census that dishonestly declares
`NativePointerEvents` for a remote whose "pointer" is a laggy, unusable
touchpad strip defeats the model, because the DCR cannot second-guess a
declared output path. The same applies one level up: `Microphone` present
says nothing about voice-recognition service quality; `Touchpad` present
says nothing about pointer usability. Controls: `Requirement::Required`
is hard-coded to D-pad only, so no other capability is ever load-bearing;
the translator prefers the connected keyboard over voice for text entry,
so voice is additive, never sole-channel (except on a
voice-and-screen-keyboard device, where the TV keyboard remains the
non-voice fallback). Required later: per-capability usability evidence in
OEM profiles, not just presence bits.

### Focus-graph ambiguity

The Spatial Focus Engine is fully deterministic — documented tie-breaks
(primary distance, perpendicular centre distance, lower id), `i128` edge
arithmetic that cannot overflow or panic, initial focus
topmost-then-leftmost — and the precomputed graph is derived by the same
rule as per-query navigation, so the two cannot disagree. The ambiguity
threat is therefore not nondeterminism but *model mismatch*: the engine
consumes absolute rectangles with authored `focusable` and `essential`
flags that no scene lowering produces yet (gap ledger N0-FOCUSGRAPH). A
wrong lowering produces a deterministically wrong graph, and a wrong
`essential` flag silently weakens the reachability check. Half-open
perpendicular overlap is deliberately strict so off-axis elements are
reported unreachable rather than silently reached. Required later: a
scene→element lowering with tests, and viewport policy that matches the
generated runtime's scroll containers.

### Silent functional loss during translation

The translator must never drop intent. Controls in this slice: every
interaction maps to exactly one outcome; untranslatable intent is
`Refused` with an explicit `IncompatibilityReason` (`NoTextInputMethodAvailable`)
— there is no `Ok(loss)` path; the `conformance_report` splits
translated and refused entries in stable order so a compile report can
enumerate every refusal; `translate_all` and the full-table report are
deterministic, so the same device always yields the same evidence.
Residual risks: (1) the capability summary (`DeviceCapabilities`) is
mirrored locally rather than derived from the DCR (gap ledger
N0-TRANSLATECTX) — drift between the two models could make a compile
report dishonest relative to the DCR's own resolution; (2) some rows are
*degradations*, not losses — pinch-zoom becomes incremental zoom and
drag-and-drop becomes select-reposition-confirm, and the table asserts
this is acceptable parity, but the criteria for that judgement live in
spec prose, not in a gate; the conformance gate that would make a
degradation evidence-backed does not exist yet (gap ledger N0-GATE).
Required later: single-source capability derivation, and a gate that
classifies each row as parity, degradation (with evidence), or refusal.

### OEM caprichio supply chain

Nevada routes OEM-specific hardware through goya-caprichios-style
Caprichios (phase 3; none exist). The inherited threat is the one the
Blue Moon programme already documents: a caprichio is trusted code that
asserts device facts, so a malicious or careless caprichio can grant
capabilities no device has, or lower a declaration permissively. Controls
the pattern already provides and Nevada must keep: typed declarations,
canonical IR, fail-closed lowering, generated factories, exhaustive
conformance tests per caprichio; the profile registry's exact-id,
no-default lookup; and the D-pad-only `Required` baseline, which bounds
the damage of any single false optional capability. Required later:
provenance for third-party caprichios, a review gate for built-in census
changes (gap ledger N0-CENSUS — a wrong built-in census changes every
downstream translation), and refusal — never degradation — when a
caprichio's declared capabilities contradict the runtime census.

## Limits

The phase-0 modules are std-only pure functions: no I/O, no network, no
process spawning, no filesystem access, so the slice has no execution
surface. The census is an in-memory argument; whoever supplies it is
outside this threat model's control boundary. Tests cover the cases
encoded in `crates/nevada` (40 unit tests); there is no fuzz corpus.
Nothing in this slice runs on Android TV hardware, so no runtime claim in
this document is evidence-backed. Once phase 3 emits leanback artefacts,
this model must be extended with the Android-side threats (generated
Kotlin injection, manifest permissions, signing, voice-service trust) —
they are out of scope until admitted.

## Out of scope until admitted

Scene admission and its parser surface, artefact generation, the leanback
runtime, voice recognition quality, remote-control pairing security, OEM
app stores and sideloading, and the runtime re-resolution of capability
mismatches. Those need the corresponding phases to exist before any PASS
can be recorded.
