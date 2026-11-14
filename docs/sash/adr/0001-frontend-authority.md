# ADR 0001 — Frontend authority

Status: accepted for Sash implementation. Not a release ratification.

## Decision

Sash reads one frontend: `goya_lang::handoff::admit`, contract `GSSH/1`, over strict GY-LANG/1 `.gscene`.

`goya-ir` GY-IR/1 stays the legacy flow IR. Sash does not lower windows through it. Viewport `NodeTable` lowering is not a lossless scene interchange and does not change `IR_CONTRACT`.

Quinta is not in this repository. Sash will not recover meaning from Quinta-only metadata. `goya-tov` remains an adapter to an external compiler CLI, not a second scene authority.

## Consequences

Unknown properties and unsupported reachable features fail admission. The static-window profile would rather reject a button, an input, or a nested cell than emit a window that drops them.

Sylyca is not a second compiler. The name in the Gestures documents now refers to Sash. No alias key is accepted.
