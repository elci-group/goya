# Delete parity

The working tree already cuts only on Shift+Delete and on Ctrl+X without shift. This note compares the other Delete chords with Sash.

It compares `key_operation` in `crates/sash/src/interactive_clipboard.rs` with `on_modified_special` and `on_cut` in `crates/emperor-runtime/src/host.rs`, and with `shortcut_from_key`, `key_special`, and `handle_key_press` in `crates/goya-wayland/src/lib.rs`. The working tree is commit `17721b35512ff1506e5d16f2a859e07462a3b838` plus the uncommitted clipboard and Insert edits. Evdev Delete is 111. `VK_DELETE` is `0x2E`. Those are not the same number. Evdev Insert is 110. Delete is not Insert. Seat shift is bit 0 of `mods_depressed` and seat ctrl is bit 2. That is not `GetKeyState`.

EMP-M1 is not passed. The system edit has not been launched. There is no caret. A unit test is not a Win32 edit and not a Wayland seat. The scripted interaction suite is not a seat event. This note does not pass EMP-M1-25 or any other ledger row.

## Chords

`key_operation` returns a cut on `WM_KEYDOWN` for `VK_X` (`0x58`) when ctrl is down and shift is up, and for `VK_DELETE` (`0x2E`) when shift is down and ctrl is up. Delete alone returns none. Ctrl+Delete returns none. Ctrl+Shift+Delete returns none. Ctrl+Shift+X returns none. The comment says Delete alone removes text without copying it. `clipboard_blocked` blocks a cut only when `cut_mode` is `CUT_DENY` (`2`). `CUT_DEFAULT` is `0` and `CUT_ALLOW` is `1`. The helper does not perform the cut.

Evdev 111 is `KeySpecial::Delete` in `key_special`. `shortcut_from_key` returns `None` for 111 whether or not ctrl or shift is held, so Delete is never a shortcut. `handle_key_press` checks that helper first. For 111 the helper returns nothing, and the press becomes `KeySpecial` when neither modifier is held, or `KeySpecialWithModifiers` when shift or ctrl is held. It does not become `KeyChar`.

Shift+Delete is that modified Delete event with shift and without ctrl. `on_modified_special` calls `on_cut`. Ctrl+X without shift is `Shortcut::Cut` for evdev 45, and `on_event` calls `on_cut`. Ctrl+Shift+X stays `None`. Plain Delete, Ctrl+Delete, and Ctrl+Shift+Delete do not call `on_cut`. Shift+Delete and those three non-cuts match `key_operation` on whether a clipboard operation exists. Ctrl+X without shift already matches.

The seat delivers plain Delete as `KeySpecial`, so that press never reaches `on_modified_special`. Ctrl+Delete and Ctrl+Shift+Delete do arrive as `KeySpecialWithModifiers`. The cut condition is shift and not ctrl, so neither call reaches `on_cut`. `delete_chords_that_are_not_cuts` feeds those three, including plain Delete, to `on_modified_special` and records no clipboard and an unchanged edit. That only locks that they do not call `on_cut`. It does not add forward-delete. `delete_is_never_a_shortcut` only locks the four presses of evdev 111 as Delete, never as a shortcut.

## Effect

`on_cut` publishes and clears only when cut is not `deny`, focus is Edit, and the edit is non-empty. `allow` and `default` both publish the whole field. `deny`, any other focus, and an empty edit publish nothing and leave the text. The host compares the string `deny`. It does not call `clipboard_blocked`. A non-deny cut publishes the whole edit, not a selection. Whole-field publish-and-clear is not a selection cut. Skipping an empty string is not an empty-selection test. There is no caret.

## Delete alone

Sash's comment describes a system edit removing text. The gate does not block Delete alone, including when cut is deny, because `key_operation` returns none. That control has not been launched. Emperor leaves the text. The host catch-all for `KeySpecial` does not change the edit, and the unit test that passes both modifiers clear into `on_modified_special` also leaves the text. That is not Sash delete, because there is no caret. This slice must not be described as implementing that delete.

## Messages still absent

`WM_CUT` (`0x0300`) and `WM_CHAR` `0x18` are still absent. Sash `operation` classifies both as cut and does not test modifiers. This note does not ask the slice to fake them.

## Not a pass

EMP-M1 is not passed. EMP-M1-25 stays unmet. A unit test is not a Win32 edit and not a Wayland seat. The scripted interaction suite is not a seat event. No ledger row moves.
