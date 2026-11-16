# Frame parity

The working tree already paints this frame. This note compares that paint with GY-EMP-RENDER/1 and with the shared session. The tree is commit `17721b35512ff1506e5d16f2a859e07462a3b838` plus the uncommitted clipboard and Delete edits. It compares `scene_frame` and `on_char` in `crates/emperor-runtime/src/host.rs` with `CounterSession` in `crates/goya-parity/src/session.rs`.

## Paint

`scene_frame` builds five primitives, in order. First is a client `Panel`. Second is client `Text` whose string is the authored client text, `Count` on the fixture. Third is `Text` of the live edit string. Fourth is a `Button` whose string is the authored button text, `Add`. Fifth is `Text` of the live result string. Each rectangle is the admitted DIP edges times `goya_wayland::MP_PER_PX`. That constant is 250, the same scale as `sash::admit::MP_PER_DIP`.

Every foreground entry is `None`. `present_dirty` receives foreground `[20, 20, 28]`, background `[235, 235, 240]`, `glyph_px` 8, and damage `None`.

`KeyController::open` starts focus on the edit. That frame fills only the edit text with `Some(0xFFD6E4F5)`. Tab is `FocusNext` through `on_key`, and the next frame fills only the button. The client panel, the client text, and the result text are always `None`. The focus value is only the edit or the button, so the client and the result are not focusable. Exactly one control is filled.

## Result string

The result primitive copies `controller.session.result`. `CounterSession::activate` writes that string only on `VisibleCommit::Store`, using `format_i64` of the stored next value. Emperor does not format a parallel count. One newline in `on_char` calls `press_add`, which sends `WM_KEYDOWN` and `VK_RETURN`. That action is `Activate`, so the controller calls `CounterSession::activate`. With the fixture edit left at `1`, count becomes 1 and the result text becomes `1`. There is no second adder.

## Edit shell

Backspace on the edit, character `'\u{8}'`, drops one grapheme through `delete_last_grapheme` and `set_edit`. A letter on the edit appends that character and calls `set_edit`. `set_edit` replaces the edit and does not change count, revision, or result. Those mutations are the Linux shell around the shared session. They are not a launched Sash `EDIT` and they are not IME. Preedit, commit, and surrounding-delete still go to `ignore_ime` and change nothing. EMP-M1-09 stays unmet. There is no caret.

## Contract sentences

The normative sentences now match the working tree. Space on the edit calls on_key and does not insert. A press records the button id and does not activate. Release inside that id calls press_add. The record is not a pointer-capture API. EMP-M1-25 stays unmet.

## Not a pass

No pixel baseline exists. A unit test of `scene_frame` is not a presented buffer, not a Wayland seat, and not a Windows `STATIC`. EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. Vulkan stays blocked. Fractional scale stays blocked.
