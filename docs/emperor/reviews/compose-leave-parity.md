# Compose leave parity

Keyboard leave cancels a partial XKB compose sequence and then clears the stale modifier booleans. This note compares the `wl_keyboard` `Leave` arm and `clear_keyboard_on_leave` in `crates/goya-wayland/src/lib.rs` with `on_char` in `crates/emperor-runtime/src/host.rs` and with `key_action` in `crates/sash/src/interactive_keys.rs`.

EMP-M1 is not passed. This is not a launched Sash EDIT and not `ToUnicode`. This is not IME. This is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/keyboard-leave-parity.md` or `compose-parity.md`. It does not ask to change `key_chars` or `shortcut_from_key`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Presses that already reset

`handle_key_press` calls `compose.reset()` when `shortcut_from_key` matches, when `key_special` matches, and when an opted-in browser-navigation chord matches. Those resets are key presses. `shortcut_tests::shortcut_cancels_a_partial_compose` presses dead acute (evdev 40), then Ctrl+C (evdev 46), then `e` (evdev 18). After that reset the next `e` is `KeyChar('e')`, not `'é'`, and compose status is `Nothing`. Leave is the focus-loss path.

## Leave

The `wl_keyboard` `Leave` arm calls `leave_keyboard`. That method calls `compose.reset()` when `compose_state` exists, then `clear_keyboard_on_leave`. `clear_keyboard_on_leave` sets `shift`, `ctrl`, and `alt` to false. When an xkb state exists it calls `update_mask(0, 0, 0, 0, 0, self.keyboard_group)`. Leave cancels the partial sequence and the stale ctrl boolean together. A later plain `x` is not Cut.

`shortcut_from_key` returns `Shortcut::Cut` for evdev 45 only when ctrl is true and shift is false. After the clear both are false, so 45 is not Cut. `shortcut_tests::keyboard_leave_drops_stale_ctrl` expects `KeyChar('x')` after `clear_keyboard_on_leave`. That is the existing stale-ctrl rule, and it holds until the next `Modifiers` event sets `ctrl` from `mods_depressed & 0x4`. The same `compose.reset()` leaves the sequence unarmed, so the next `e` is `'e'`, not `'é'`. On `Nothing`, `key_chars_with_compose` still calls `key_chars`.

`shortcut_tests::keyboard_leave_cancels_a_partial_compose` presses dead acute (evdev 40), sets ctrl, calls `leave_keyboard`, then presses `e` (18) and `x` (45). The next `e` is `KeyChar('e')` and the next `x` is `KeyChar('x')`. The enter serial stays 7, the layout group stays 2, and the pointer stays `(3.0, 4.0)`. `keyboard_leave_drops_stale_ctrl` still calls `clear_keyboard_on_leave` and does not press a dead key. Parent re-ran both tests: `COMPOSE_LEAVE_EXERCISED`, 2 passed, 83 filtered.

## What stays

`Enter` stores `keyboard_enter_serial` and does not restore modifiers. Leave does not write that serial. `Modifiers` stores `keyboard_group`. Leave passes that group into `update_mask` and does not switch the layout to group 0. `Keymap` replaces `xkb_state` only when a new map is accepted. Keyboard leave does not. Pointer `Leave` sets `last_pointer` to `None`. Keyboard leave does not. The serial and the layout group stay. That is not GetKeyState.

`edit_proc` samples `GetKeyState(0x11) < 0` and `GetKeyState(0x10) < 0` on the message it is handling. Emperor does not call `GetKeyState`. The bools are the last `Modifiers` event, cleared on leave, not a live key query.

## Host and Sash

Emperor `on_char` is unchanged. A later `KeyChar` appends only while focus is the edit and the character is not a control. `on_event` calls `on_cut` only for `Shortcut::Cut`. After leave, evdev 45 is `KeyChar('x')`, so `on_cut` is not called for it. `ignore_ime` returns `Step::Unchanged`. Keyboard leave is not `zwp_text_input_v3` `Leave`, which calls `feed_leave`.

`key_action` does not know dead keys or a keyboard-leave compose reset. On `WM_KEYDOWN` (`0x0100`), `VK_TAB` (`9`) moves focus and `VK_RETURN` (`13`) is `Activate`. Every other wparam is `Dispatch`. Ctrl is not an argument.

## Not a pass

A unit test is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. This is not a launched EDIT, not `ToUnicode`, and not IME. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`.
