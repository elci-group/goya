# Text parity

Digits append only on the focused edit and Ctrl+Tab only moves focus. This note compares that slice with `key_action` in `crates/sash/src/interactive_keys.rs`, `KeyController::on_key` in `crates/goya-parity/src/keys.rs`, and `on_char` and `on_modified_special` in `crates/emperor-runtime/src/host.rs`.

EMP-M1 is not passed. EMP-M1-25 stays unmet. A unit test is not a Wayland seat and not a Win32 edit. No ledger row moves.

## Classification

`key_action` returns `Activate` only for `WM_KEYDOWN` (`0x0100`) and `VK_RETURN` (`13`). Tab (`VK_TAB`, `9`) is `FocusNext`. Shift-Tab is `FocusPrev`. Any other wparam on that message is `Dispatch`, including Space (`VK_SPACE`, `0x20`) and a digit. A message that is not `WM_KEYDOWN` is `Dispatch`. Ctrl is not an argument. The pump reads shift with `GetKeyState(0x10)` and does not pass ctrl into this helper.

`Dispatch` does not activate. Digits are `Dispatch` and do not change the oracle by themselves. `KeyController::on_key` follows that decision. `FocusNext` and `FocusPrev` only swap `Focus::Edit` and `Focus::Button`. `Activate` calls `activate`. Inside `Dispatch`, the controller activates only when focus is the button, the message is `WM_KEYDOWN`, and the wparam is `VK_SPACE`. A digit key does not take that branch. Count stays put.

Sash then translates and dispatches a `Dispatch` key. The numeric field is a system `EDIT`. That control would insert the character. It has not been launched. `key_action` does not write the edit.

## Characters

Emperor appends a non-control character through `set_edit` only while focus is the edit. `on_char` pushes that character onto `session.edit` and calls `CounterSession::set_edit`. `set_edit` replaces the string and leaves count, revision, and result unchanged. Button focus does not append, so a digit on the button does not change the edit or the count. That append is the Linux shell around the session, not a launched Sash `EDIT`. There is no caret. The character is added at the end of the whole string.

Evdev 2 is the character `1`. It is not `VK_1` (`0x31`). `key_action` never sees the evdev code. The seat turns that key into `KeyChar`, and the host appends it only while focus is the edit.

A control character other than newline and backspace does not append. `key_chars` in `crates/goya-wayland/src/lib.rs` keeps newline and backspace and drops every other control. `key_char_from_state` drops every control. Tab is `KeySpecial`, not `KeyChar`, so it never reaches the append arm. Newline (`'\n'`) calls `press_add`. Backspace (`'\u{8}'`) removes one grapheme through `set_edit` only while focus is the edit, and does not change count.

Space on the edit still does not insert. `on_char` for `' '` calls `on_key` with `WM_KEYDOWN`, `VK_SPACE`, and shift false. It does not call `set_edit`. The edit stays as it was. Space on the button activates, which is the `Dispatch` rule above.

## Tab and Return

Tab remains focus movement, including when ctrl is set and when both ctrl and shift are set. Plain Tab is `KeySpecial::Tab` and calls `on_tab` with shift false, which is `FocusNext`. A modified Tab goes through `on_modified_special`, which calls `on_tab` with `modifiers.shift` and does not read ctrl. Ctrl+Tab is still `FocusNext`. Ctrl+Shift+Tab is still `FocusPrev`. Neither call activates. Ctrl does not activate.

Evdev 28 and 96 stay `KeyChar('\n')` when ctrl is held. `key_chars` returns that character before the ctrl suppression on the xkb path. `on_char` still calls `press_add`. `press_add` enters `on_key` with `WM_KEYDOWN` and `VK_RETURN`. Sash activates every `VK_RETURN` on `WM_KEYDOWN`, and ctrl is not an argument, so Return with ctrl held is the same action. This is not a second adder. Count changes only inside `KeyController::activate`.

## Delete and the keymap-free path

Plain Delete still does not remove text. With neither shift nor ctrl, the press is `KeySpecial::Delete`, and `on_event` leaves every plain `KeySpecial` unchanged. `on_modified_special` cuts only for Delete when shift is set and ctrl is clear. There is no caret, so this is not a system-edit delete.

The keymap-free path can still emit a letter for an unrecognized ctrl combo. `shortcut_from_key` returns `None` for a ctrl combo it does not name. On the xkb path, a true ctrl boolean suppresses `KeyChar`. The keymap-free path calls `key_char` and does not read ctrl, so that letter can still arrive. If focus is the edit, `on_char` still appends it. This slice does not change that.

## Not a pass

EMP-M1 is not passed. EMP-M1-25 stays unmet. A unit test is not a Wayland seat and not a Win32 edit. The scripted interaction suite is not a seat event. No ledger row moves.
