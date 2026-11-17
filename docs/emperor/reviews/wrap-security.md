# Wrap security

`git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. The wrap was already in `on_tab` and `on_modified_special`. This slice adds `tests::shift_tab_on_the_edit_wraps_to_the_button` and the sentences quoted below. This note does not change the gap ledger and does not pass any row. EMP-M1-10 stays unmet.

Reviewed `on_tab`, `on_modified_special`, and the `KeySpecialWithModifiers` arm of `on_event` in `crates/emperor-runtime/src/host.rs`, and `KeyController::on_key` in `crates/goya-parity/src/keys.rs`, against `key_action` in `crates/sash/src/interactive_keys.rs` and the sandbox paragraph in `docs/emperor/threat-model.md`.

## What the wrap does

Shift-Tab is `KeySpecial::Tab` with shift true. `on_modified_special` calls `on_tab(controller, modifiers.shift)` and returns clipboard `None`. That arm does not call `on_cut`. `on_cut` runs only when the key is `KeySpecial::Delete`, shift is set, and ctrl is clear. The Tab arm does not read ctrl.

`on_tab` calls `KeyController::on_key` with `WM_KEYDOWN` (`0x0100`), `VK_TAB` (`9`), and that shift flag. It does not call `press_add`. `key_action` returns `FocusPrev` when shift is set and `FocusNext` otherwise. `FocusPrev` on `Edit` sets `Button`. `FocusPrev` on `Button` sets `Edit`. `FocusNext` wraps the other way. `Focus` has only those two values.

Neither match calls `activate`. `CounterSession::activate` is the store. The edit string, count, revision, and result stay put. Wrapping is not activation. `Step::Changed` from a focus swap only sets the frame dirty in the host loop. It does not write the count.

`on_event` calls `set_clipboard_text` for `KeySpecialWithModifiers` only when `effect.clipboard` is `Some`. The Tab effect is `None`, so the chord does not publish. Plain Tab is `PlatformEvent::KeySpecial(KeySpecial::Tab)`, which calls `on_tab(controller, false)`. That arm does not call `set_clipboard_text`.

Section 6 of `docs/emperor/GY-EMP-001.md` says: "Shift-Tab on the edit moves focus to the button and does not store. A second Shift-Tab returns to the edit. Tab from the button returns to the edit."

The Tab bullet in `docs/emperor/contracts/gy-cross-parity-1.md` says the same: "Shift-Tab on the edit moves focus to the button and does not store. A second Shift-Tab returns to the edit. Tab from the button returns to the edit."

## What the test locks

`shift_tab_on_the_edit_wraps_to_the_button` follows `delete_does_not_append_and_backspace_drops_one_cluster`. It admits the counter on the edit with edit `"1"`, count `0`, revision `0`, and result `"0"`.

Shift-Tab through `on_modified_special`, with shift true and ctrl false, returns `Step::Changed` and clipboard `None`, and sets focus to the button. A second Shift-Tab returns to the edit, again with clipboard `None`. Tab through `on_tab(controller, false)` sets the button, and a second Tab returns to the edit. Edit stays `"1"` and count stays `0` after every call. Revision stays `0` and result stays `"0"` across both Shift-Tabs and the return to the edit.

## What it does not prove

This is not a pointer grab. The test does not press, does not record a pointer id, and does not capture the pointer. This is not AT-SPI. Swapping `Focus` does not publish a name, a role, a value, or focus to a screen reader. This is not EMP-M1-10. EMP-M1-10 stays unmet.

A unit test is not a launched window and not `SetFocus`. The test calls `on_modified_special` and `on_tab` on an admitted `KeyController`. It does not open a window. `focus_cycle` in `crates/sash/src/interactive.rs` is the path that calls `SetFocus`. This test does not call that pump.

No sandbox is installed. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. EMP-M1-11 stays unmet.

This note does not mark a ledger row passed. No gap-ledger row moves.
