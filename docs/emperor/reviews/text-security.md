# Text security

Non-control characters append only on the edit, and other controls do not. Digits and other non-control characters append only while focus is the edit. They do not change count, revision, or result until Enter. This note does not change the gap ledger and does not pass any row.

Reviewed `on_char` in `crates/emperor-runtime/src/host.rs`, `key_chars` and `handle_key_press` in `crates/goya-wayland/src/lib.rs`, and the sandbox paragraph in `docs/emperor/threat-model.md`.

## Append

The default arm runs for a character that is not newline, space, or backspace. It calls `set_edit` only when focus is the edit and `ch.is_control()` is false. Tab, carriage return, and DEL are controls, so they do not call `set_edit`. Button focus does not append. `set_edit` replaces the edit string. Count, revision, and result stay put. Enter is the later path that can change those three.

## Newline, space, and backspace

Newline still calls `press_add`. That sends `WM_KEYDOWN` and `VK_RETURN` through the controller. It does not insert the newline into the edit.

Space still calls `on_key` with `WM_KEYDOWN` and `VK_SPACE`. It does not call `set_edit`. The edit does not gain a space. Space on the edit does not activate. Space on the button still uses that dispatch, which activates only then. That is not an insert.

Backspace still drops one grapheme on the edit only. The character is `'\u{8}'`. Focus must be the edit. `delete_last_grapheme` cuts the last grapheme and `set_edit` stores the remainder. Button focus does not call `set_edit`. Count, revision, and result stay put.

## Ctrl+Tab and Ctrl+Enter

Ctrl+Tab is focus movement, not activation. Evdev 15 is `KeySpecial::Tab`. With ctrl held, the press is `KeySpecialWithModifiers`. `on_modified_special` calls `on_tab`. That calls `on_key` with `VK_TAB`. It does not call `press_add` and does not call `set_edit`.

Ctrl+Enter still becomes `KeyChar('\n')`. `key_chars` returns newline for evdev 28 and 96 before `key_char_from_state` suppresses a character while ctrl is held. `on_char` then takes the newline arm and calls `press_add`. That is the existing activation path. It is not a new one.

## Keymap-free letters

The keymap-free path still ignores ctrl. `key_chars` with no xkb state does not consult ctrl, so a ctrl letter that is not a shortcut can still emit `KeyChar`. That path is not fixed. With an xkb state, ctrl still suppresses `KeyChar`, apart from the forced newline and backspace returns.

## No caret and no selection

There is no caret and no selection. A digit is pushed at the end of the whole edit. Backspace removes the last grapheme of the whole edit. Nothing in this slice inserts at a caret or publishes a range.

## No sandbox

No sandbox is installed. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set, and it must not report those as enabled. The running `App` is not confined, because no sandbox is attempted. Failure to establish a sandbox cannot be observed because no sandbox is attempted. That is an honest gap, not a pass. EMP-M1-11 stays unmet.

## Not a pass

This note does not mark a ledger row passed. A unit test is not a seat. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet. The keymap-free ctrl letter path is not fixed.
