# ADR 0004 — Nested static cells

Status: accepted as a widening of the static-window profile. Runtime launch remains blocked.

## Decision

`sash.win32.static-window/0.0.2` admits a tree of `@cell` nodes. The previous `0.0.1` profile rejected every child. No released application depended on that rejection. Scenes that `0.0.1` accepted still pass.

The shared `goya_lang::layout` solver is the geometry oracle. The root extent is the window client size. A child with an omitted extent fills its parent. A child with an omitted position sits at the parent origin. Each cell that has text becomes a `STATIC` control at the solved rectangle. A cell with neither text nor children is an error. A cell the solver moves or shrinks, including a child that does not fit its parent, is `SASH-LAYOUT`.

Buttons, inputs, interactions, fill, stroke, text size, anchor, wrap, scroll, proportional geometry, and strong right-to-left text stay rejected. At most 32 cells are admitted.

## Consequences

Overlapping labels keep the solver's depth-first order: a later cell paints above an earlier one. Resizing the window clips those rectangles. It does not reflow them. This is not the editable input, button, or retention slice, and it is not a Windows launch.
