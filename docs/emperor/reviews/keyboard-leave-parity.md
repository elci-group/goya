# Keyboard leave parity

Keyboard leave drops stale Ctrl so the next X is not a cut. This note compares that slice with `key_operation` in `crates/sash/src/interactive_clipboard.rs` and with `shortcut_from_key` and the `wl_keyboard` `Leave` arm in `crates/goya-wayland/src/lib.rs`. The host rules are `on_cut` and `on_modified_special` in `crates/emperor-runtime/src/host.rs`.

EMP-M1 is not passed. This is not a Win32 keyboard and not a Wayland seat. EMP-M1-25 stays unmet. No ledger row moves.

## Sash

`key_operation` returns a cut on `WM_KEYDOWN` for `VK_X` (`0x58`) when ctrl is down and shift is up, and for `VK_DELETE` (`0x2E`) when shift is down and ctrl is up. Delete alone, Ctrl+Delete, Ctrl+Shift+Delete, and Ctrl+Shift+X return none. The edit procedure samples the booleans with `GetKeyState(0x11) < 0` and `GetKeyState(0x10) < 0` on the message it is handling. Those messages are not delivered after the edit loses the keyboard. The helper does not keep a modifier cache and does not perform the cut. `clipboard_blocked` blocks a cut only when `cut_mode` is `CUT_DENY` (`2`).

## Seat bits

Emperor does not call `GetKeyState`. `wl_keyboard` `Modifiers` sets `shift` from bit 0 of `mods_depressed` and `ctrl` from bit 2. `alt` is the xkb effective Alt name when a keymap state exists. Those booleans are the last event, not a live key query. Enter stores `keyboard_enter_serial` and does not restore them. Evdev 45 is not `VK_X`. Evdev 46 is not `VK_C` (`0x43`). Evdev 111 is not `VK_DELETE`.

## Leave

The slice clears `shift`, `ctrl`, and `alt` on `wl_keyboard` `Leave`. When an xkb state exists, it clears the depressed, latched, and locked modifier masks and keeps the layout group. It does not switch the layout to group 0. The next `Modifiers` event writes the bits and the masks again.

Until that event, `shortcut_from_key` sees ctrl and shift clear. Evdev 45 is not `Shortcut::Cut`. Evdev 46 is not `Shortcut::Copy`. `handle_key_press` does not push those shortcuts, so `on_event` does not call `on_cut` or `copy_text` for them. Shift is clear as well, so evdev 111 is plain `KeySpecial::Delete`, not the Shift+Delete cut. Sash would not deliver the key at all. The slice does not drop a later press. It only drops the cached modifiers.

After the next `Modifiers` event, ctrl without shift is still `Shortcut::Cut` for evdev 45, and shift without ctrl is still the Delete cut. Ctrl+Shift+X stays `None`.

## Host

The host `on_cut` rules do not change. `deny`, a focus other than the edit, and an empty edit publish nothing and leave the text. `allow` and `default` publish the whole edit and clear it. The host compares the string `deny`. It does not call `clipboard_blocked`. There is no selection. There is no caret. A non-deny cut is the whole field, not a selection cut.

Plain Delete still does not remove text. The `KeySpecial` catch-all leaves the edit unchanged. Sash's comment says Delete alone removes text. That control has not been launched. This slice must not be described as that delete.

## Keymap-free letters

`shortcut_from_key` still returns `None` for a ctrl combo it does not name. On the xkb path, a true `ctrl` boolean suppresses `KeyChar`. The keymap-free path calls `key_char` and does not read ctrl, so it can still emit a letter for an unrecognized ctrl combo while ctrl is actually down. Leave does not change that.

## Not a pass

A unit test of the leave arm is not a Win32 keyboard and not a Wayland seat. The scripted interaction suite is not a seat event. EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves.
