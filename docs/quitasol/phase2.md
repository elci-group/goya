# Quitasol Q2 — Fire OS integration emission (Media Session track)

Status: **proposed, not ratified**. Implementation is experimental; it does
not admit, ratify or freeze any profile, and produces no APK, no project
and no runtime. Phase 1 record: [phase1.md](phase1.md); evidence anchor:
[q0-capability-audit.md](q0-capability-audit.md).

## What Q2 delivers

`crates/quitasol-semantic` gains `src/fireos.rs`, emitting a deterministic
**Fire OS integration plan** (contract `QUITASOL-FIREOS-PLAN/1-draft`)
from a Q1-resolved manifest:

- **Authoring hook** — optional `transport: <class>;` key on `@intent`
  (goya-lang compatibility parser, draft surface): `play_pause`,
  `skip_next`, `skip_previous`, `seek_forward`, `seek_backward`, `restart`.
  Exactly the six Media Session behaviours Amazon documents; absolute-time
  seek is deliberately absent (Q0: unsupported by the Fire OS voice path).
- **Permission emission** — the plan contains
  `com.amazon.permission.media.session.voicecommandcontrol` exactly when at
  least one intent declares transport. Q0 records the permission "is not
  required for Fire OS 14 and later"; the plan records this as context, and
  no device-version resolution is claimed.
- **Per-intent lowering** — a transport-declaring intent maps to its
  documented MediaSession callback actions (`onPlay`/`onPause`,
  `onSkipToNext`, `onSkipToPrevious`, offset `onSeekTo`, restart-to-zero).
  Every other intent is recorded `Unresolved`: search/discovery would
  require VSK, which the Q0 audit records as closed to new partners.
  Nothing is silently dropped.
- **Launch** — voice launch by app name is platform-provided for every
  installed app (Q0); recorded per plan as platform support, not developer
  work.
- **Fail-closed ordering** — the plan consumes the Q1 `ResolvedManifest`;
  a scene/manifest mismatch is `QS-E2004`, never a partial plan.

The plan-level lowering tier stays `"unresolved"`: a transport mapping is a
candidate binding, not working voice control. No MediaSession runtime,
no Android code generation, and no on-device behaviour is claimed.

## BLOCKED (unchanged by Q2)

- **On-device verification** — no Fire OS device or emulator has executed
  any of this; the permission note (Fire OS 14+) is documentary only.
- **MediaSession runtime** — no code path connects the plan to Android
  callbacks; that is a later phase (and would build on Aragon/goya-sdk
  infrastructure, which targets phones today).
- **APK/AAB emission** — not implemented; Aragon owns phone-track codegen
  and the Nevada ledger (N0-CODEGEN) owns TV emission.
- **Vega track** — untouched; `.vpkg` and RNV emission are separate phases.
- **Owners/ratification** — unresolved, per phase 0.

## Verification

```
cargo test -p goya-lang --offline
cargo test -p quitasol-semantic --offline
```

Evidence record: [programme/quitasol-q2.json](programme/quitasol-q2.json).
