# Proportional-length migration ledger (GSSH/3)

Started 18 November 2026 with [decision D7](decisions.md#d7--proportional-lengths-gssh3):
every length in Goya is proportional. GSSH/3 rejects absolute lengths, and
targets move to it one at a time. Until a target moves, it stays on its
current profile and is unaffected. GSSH/1 and GSSH/2 reject the new
proportional text and stroke lengths, so no target receives a length it
would ignore.

## Converting a scene

```sh
cargo run --offline -p goya-lang --bin goya-proportional -- \
    scene.gscene <width-pt> <height-pt> converted.gscene
```

The fractions reproduce the reference layout exactly at the given viewport
(choose the size the scene was designed for) and scale with the parent
everywhere else. Only the rewritten properties change. Exit status 3 lists
lengths that no fraction can reproduce. Today these are only text sizes on
elements with zero size; fix them by hand.

## Targets

| Target | Profile today | Status | What it needs |
| --- | --- | --- | --- |
| Aragon (Android, `goya-sdk` scene compiler) | GSSH/3 | **Migrated** 18 Nov 2026 | Emits `text-size-p` (em size) and `stroke-width-p` (inside border); device fixtures re-qualified |
| Aragon previews (static text, root button) | GSSH/1 | Not migrating | Single-element previews with no lengths; kept as recorded APK fixtures |
| Sash (Windows) | GSSH/1 | Not started | Sash admits only absolute geometry and refuses `_p`; it needs proportional geometry, then GSSH/3 admission |
| Emperor | GSSH/1 | Not started | Moves with Sash (shared interactive profile) |
| goya-parity | GSSH/1 | Not started | Parity fixtures follow Sash |
| Wayland renderer (`goya-wayland`) and its apps | compatibility parser | Not started | Read `DrawNode::text_size_mp` and `stroke_width_mp` instead of its own scale for absolute `text-size` |
| Neptune (Swift preview) | GSSH/1 | Not started | Single static text preview; move once it emits geometry |

## Scenes

| Location | Scenes with absolute lengths | Notes |
| --- | --- | --- |
| `apps/` | 10 | All convert exactly at 800×600 and 392×753 points, except 16 text sizes on zero-sized placeholder buttons in `zcode-native.gscene` |
| `crates/sash/fixtures/` | 3 | Convert when Sash moves |
| `crates/goya-lang/tests/corpus/` | 19 | Intentionally GSSH/1 and GSSH/2 fixtures; they stay absolute to test those profiles |

`crates/goya-lang/tests/proportional_migration.rs` converts every scene file
in the repository at both reference viewports and checks that the layout is
unchanged.
