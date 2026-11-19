# Quitasol Q3 — MediaSession runtime glue (native media integration)

Status: **proposed, not ratified**. Implementation is experimental. The
emitted Kotlin is a **candidate binding that has never been compiled** —
no Android toolchain is invoked and no on-device behaviour is claimed.
Phase 2 record: [phase2.md](phase2.md); evidence anchor:
[q0-capability-audit.md](q0-capability-audit.md).

## What Q3 delivers

`crates/quitasol-semantic` gains `src/runtime.rs`, lowering the Q2 Fire OS
plan (contract `QUITASOL-FIREOS-PLAN/1-draft`) to deterministic Kotlin
MediaSession glue (contract `QUITASOL-MEDIASESSION-RT/1-draft`):

- **`QuitasolIntentBridge`** — a one-method interface
  (`invoke(intent: String): Boolean`); the application supplies the
  implementation that routes an intent name into its own interaction
  runtime. The glue never reaches into app internals.
- **`QuitasolMediaSessionCallback`** — a `MediaSessionCompat.Callback`
  subclass whose overrides dispatch to the bridge: transport-declaring
  intents bind the documented callbacks (`onPlay`/`onPause`,
  `onSkipToNext`, `onSkipToPrevious`, `onFastForward`, `onRewind`, and a
  `pos == 0L`-guarded `onSeekTo` for restart). The Q2 plan's callback names
  were corrected to these API-faithful forms in the same change.
- **Conflict refusal** — a MediaSession callback method may be bound by at
  most one intent; a second binding (e.g. two `play_pause` intents both
  overriding `onPlay`) is `QS-E2005`, a compile-time refusal, never
  last-wins. Plan/scene intent-set mismatch remains `QS-E2004`.
- **Honesty record** — intents without a documented self-serve lowering
  are listed in the emitted file as "not voice-reachable (VSK closed per
  Q0)"; they are never silently absent.

## Navigation integration: recorded, not emitted

The roadmap's "navigation integration" half of Q3 is **BLOCKED by
evidence**, not by implementation choice: the Q0 audit documents Amazon's
Fire TV voice scrolling & selection as back-end activated per app after
Amazon verifies the app supports the commands — there is no self-serve
navigation API to integrate. The runtime module emits no navigation glue;
when Amazon documents one, it becomes a new lowering recorded in the plan
contract, not a reinterpretation of this one.

## BLOCKED

- **Compilation** — the glue has never been compiled; Kotlin emission is
  text-only with pinned-shape tests. A later phase would compile it inside
  an Android project (Aragon/goya-sdk infrastructure targets phones today;
  TV/APK emission ownership is N0-CODEGEN).
- **On-device behaviour** — no Fire OS device has executed any binding.
- **Bridge runtime** — `QuitasolIntentBridge` implementations require the
  app's D1/D2 interaction runtime; no Quitasol runtime exists.
- **Vega track** — untouched; RNV voice is platform-mediated per Q0 and
  the third-party Vega voice API contract is undocumented.
- **Owners/ratification** — unresolved, per phase 0.

## Verification

```
cargo test -p goya-lang --offline
cargo test -p quitasol-semantic --offline
```

Evidence record: [programme/quitasol-q3.json](programme/quitasol-q3.json).

## VM test (Q3 evidence)

Fire TV OS cannot be installed on a VM — Amazon does not distribute Fire OS
or Vega OS images, and the Vega Virtual Device lacks Alexa voice services
(see the [Q0 audit](q0-capability-audit.md)). The substitute: the harness in
[scripts/quitasol-vmtest.sh](../../scripts/quitasol-vmtest.sh) compiles the
emitted glue into a real signed APK and exercises MediaSession dispatch on a
generic AOSP 34 emulator. The **compilation** gate is now **PASS** — the
first real compile of the glue (kotlinc 2.0.21, zero errors/warnings;
supersedes the BLOCKED gate above). The runtime dispatch verdict on AOSP is
still pending, recorded only from observed logcat. Evidence record:
[programme/quitasol-vmtest.json](programme/quitasol-vmtest.json).
