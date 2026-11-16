# Compose leave security

The `wl_keyboard` `Leave` arm calls `GoyaWindowInner::leave_keyboard`. That method resets compose, then calls `clear_keyboard_on_leave`. Parent re-ran `shortcut_tests::keyboard_leave_cancels_a_partial_compose` (`COMPOSE_LEAVE_EXERCISED`) and `shortcut_tests::keyboard_leave_drops_stale_ctrl`. This note does not change the gap ledger and does not pass any row.

Reviewed the `Leave` arm, `leave_keyboard`, `clear_keyboard_on_leave`, `handle_key_press`, `shortcut_from_key`, `key_special`, `key_chars_with_compose`, and `set_clipboard_text` in `crates/goya-wayland/src/lib.rs`, and `on_event`, `on_char`, `on_cut`, `copy_text`, and `ignore_ime` in `crates/emperor-runtime/src/host.rs`, against the sandbox paragraph in `docs/emperor/threat-model.md`. `shortcut_tests::keyboard_leave_drops_stale_ctrl` calls `clear_keyboard_on_leave` and does not feed a dead key. `shortcut_tests::keyboard_leave_cancels_a_partial_compose` presses dead acute, calls `leave_keyboard`, then expects `KeyChar('e')` and `KeyChar('x')`.

## The leave pair

`Leave` calls `leave_keyboard`. That method calls `compose.reset()` when `compose_state` is present, then `clear_keyboard_on_leave`. That helper sets `shift`, `ctrl`, and `alt` to false. When `xkb_state` is present it calls `update_mask(0, 0, 0, 0, 0, self.keyboard_group)`. It does not reset compose. Both steps stay on the `Leave` path. Dropping the reset leaves a partial compose armed. Dropping the clear leaves a stale ctrl boolean. `leave_keyboard` is this pair, called from `Leave`.

A partial dead acute is evdev 40. `key_chars_with_compose` emits nothing while status is `Composing`. `shortcut_tests::ctrl_suppresses_a_composed_character` then feeds evdev 18 and gets `KeyChar('é')`. After `compose.reset()`, that 18 is `KeyChar('e')`, not `'é'`. `shortcut_from_key` returns `Shortcut::Cut` for evdev 45 only when ctrl is set and shift is clear. `key_special` does not claim 45. After the clear, 45 is `KeyChar('x')`, not `Shortcut::Cut`.

## What leave does not clear

`Leave` pushes no `PlatformEvent` and does not call `on_event`. It does not clear the Emperor edit, count, or clipboard. `on_char` appends through `set_edit` only when a `KeyChar` arrives and focus is the edit. `on_cut` publishes and clears the edit only for `Shortcut::Cut` when cut is allowed, focus is the edit, and the edit is non-empty. `copy_text` only reads the edit. Leave calls none of them and does not call `set_clipboard_text` or `request_clipboard_text`.

It does not wipe the data-device serial. `Enter` stores `keyboard_enter_serial`. `set_clipboard_text` passes that value to `set_selection`. `Leave` does not write the serial, the keymap, or `last_pointer`. `clear_keyboard_on_leave` keeps `keyboard_group`. Pointer `Leave` is what sets `last_pointer` to `None`.

Keeping the serial is not a credential cache and not a new grab. Leave does not call `set_selection` and does not take a keyboard or pointer grab. EMP-M1-11 stays unmet. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.

## Not GetKeyState

This is not GetKeyState. It does not read a live key bitmap. Latched and locked modifiers are cleared in the xkb mask and are not folded into the booleans. `Modifiers` sets `shift` from `mods_depressed & 0x1` and `ctrl` from `mods_depressed & 0x4`. Leave assigns the three booleans false and passes zero for depressed, latched, and locked. Do not ask to change that.

## Not IME and not a sandbox

`compose.reset()` is not `zwp_text_input_v3` support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. EMP-M1-11 stays unmet. EMP-M1-09 stays unmet.

## Not a seat

A unit test is not a Wayland seat. `shortcut_tests` builds a `GoyaWindowInner` with no compositor. `keyboard_leave_drops_stale_ctrl` does not press evdev 40 and does not dispatch `Leave`. `keyboard_leave_cancels_a_partial_compose` calls `leave_keyboard` on that inner and does not dispatch a seat `Leave`. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.
