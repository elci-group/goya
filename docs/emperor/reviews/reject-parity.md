# Reject parity

Emperor appends a non-control character on the focused edit and stores only when Sash accepts that string on activate. This note compares `on_char` and `press_add` in `crates/emperor-runtime` with `key_action` in `crates/sash/src/interactive_keys.rs`, `KeyController::activate` in `crates/goya-parity/src/keys.rs`, and `parse_canonical_i64` in `crates/sash/src/interactive_text.rs`.

EMP-M1 is not passed. This is not a Win32 EDIT, not a Wayland seat, and not IME support. EMP-M1-09 and EMP-M1-25 stay unmet. No ledger row moves. The historical body of `docs/emperor/reviews/interaction-parity.md` is history. This note does not rewrite it.

## Dispatch

`key_action` returns `Activate` only for `WM_KEYDOWN` (`0x0100`) and `VK_RETURN` (`13`). There is no focus check. Ctrl is not an argument. Any other wparam on that message is `Dispatch`, including a digit and a comma. A message that is not `WM_KEYDOWN` is `Dispatch`. Digits and comma are `Dispatch` and do not themselves change the oracle. `KeyController::on_key` follows that decision. Inside `Dispatch` it activates only when focus is the button, the message is `WM_KEYDOWN`, and the wparam is `VK_SPACE`. A digit or a comma does not take that branch.

The Linux shell appends them through `set_edit`. Store happens only on activate.

## Characters

`on_char` keeps space, newline, and backspace on their existing arms. Newline calls `press_add`. `press_add` calls `on_key` with `WM_KEYDOWN`, `VK_RETURN`, and shift false. Space calls `on_key` with `VK_SPACE` and does not insert. Backspace removes one grapheme through `set_edit` only while focus is the edit, and does not change count. The catch-all appends only when `controller.focus == Focus::Edit` and `!ch.is_control()`. It pushes the character onto `session.edit` and calls `CounterSession::set_edit`. `set_edit` replaces the string and leaves count, revision, and result unchanged. Button focus does not append.

A comma is appended because it is not a control character. `edit.push` adds it at the end of the whole string. That is not a Sash selection and not a launched EDIT control. There is no caret.

## Parse

`parse_canonical_i64` lives in Sash. Emperor calls `sash::interactive::parse_canonical_i64`. It must call that function, not copy it. The function returns `None` when the text is empty, when `text.len() > 20`, when any byte is other than `-` or `0-9`, when `parse::<i64>` fails, and when the text is not `format_i64` of the parsed value. `format_i64` is `value.to_string()`, so `format_i64(0)` is `"0"`. `00` and `-0` fail that format check. `9223372036854775808` fails the parse. `"1,"` fails the byte check.

`KeyController::activate` parses that way and commits with `sash::interactive::commit`. It does not implement a second counter. It then calls `CounterSession::activate(false, true, false, visible)`. The session parses and commits again. When the parse is `None`, `visible` is `None`, `activation` returns `Rejected`, `publish_after_text` returns `Hold`, and `visible_commit` returns `Leave`. An empty edit is also outside `activation`'s `1..31` UTF-16 window, so that check rejects it before the parse check. The `Leave` arm writes nothing. Empty, `00`, `-0`, `9223372036854775808`, and `"1,"` stay in the edit and do not store.

## Enter

Enter on the button still activates the current edit string. The newline arm does not read focus, and `key_action` returns `Activate` for `VK_RETURN` with no focus check. After a stored `"1"`, the edit is still `"1"`, and another Enter stores the next count.

The fixture rule is: "One activation of the authored counter, with the edit left at `1`, a successful text write, the window not released, and a readback of the new value, stores count 1 at revision 1. The result text becomes `1`." One Enter from edit `"1"` stores count 1, revision 1, result `"1"`.

## Wording

`docs/emperor/contracts/gy-cross-parity-1.md` says non-control characters other than space append through `set_edit` while the edit is focused, and that a control other than newline and backspace does not. `docs/emperor/GY-EMP-001.md` section 6 says the same for the edit, and that a string `parse_canonical_i64` rejects stays in the edit and does not store. Space still does not insert. The space rule is not a mistake.

## Keymap-free ctrl

`key_chars` with no xkb state does not consult ctrl. Keymap-free ctrl letters stay as they are. This note does not ask to change `key_chars`. With an xkb state, ctrl still suppresses `KeyChar`.

## Not a pass

`tests::rejected_edit_does_not_store` in `crates/emperor-runtime/src/lib.rs` drives that path. Backspace to `""`, then Enter, leaves the edit empty and keeps count 0, revision 0, result `"0"`. Two `0` characters make `"00"`; Enter leaves `"00"` and those three fields. A comma on `"1"` makes `"1,"`; Enter leaves `"1,"` and count 0. `"-"` then `"0"` make `"-0"`; Enter leaves `"-0"` and count 0. Enter on `9223372036854775808` leaves that string and keeps count 0, revision 0, result `"0"`. Enter on `"1"` then stores count 1, revision 1, result `"1"`, and leaves the edit `"1"`. Tab to the button, then `0`, does not append. Enter there stores count 2, revision 2, result `"2"`, and the edit stays `"1"`.

This is not a Win32 EDIT, not a Wayland seat, and not IME support. `ignore_ime` returns `Step::Unchanged`. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met.
