# Insert parity

The working tree has the edit-focus copy gate, Ctrl+X cut, and the Insert chords this note describes. It was written against commit `17721b3` plus those edits.

Commit `17721b35512ff1506e5d16f2a859e07462a3b838`. This note compares the plan with `key_operation` in `crates/sash/src/interactive_clipboard.rs`, `shortcut_from_key` in `crates/goya-wayland/src/lib.rs`, and `paste_shortcut_requests_clipboard`, `on_clipboard_text`, and `copy_text` in `crates/emperor-runtime/src/host.rs`. Evdev Insert is 110. `VK_INSERT` is `0x2D`. Those are not the same number. Seat shift is bit 0 of `mods_depressed` and seat ctrl is bit 2. That is not `GetKeyState`.

EMP-M1 is not passed. The generated Win32 image has not been launched. A unit test is not a Win32 edit control and not a Wayland seat. The scripted interaction suite is not a seat event. This note does not pass EMP-M1-25 or any other ledger row. It does not replace `clipboard-parity.md`.

## Plan

Ctrl+Insert (evdev 110, ctrl, no shift) becomes `Shortcut::Copy`. Shift+Insert (shift, no ctrl) becomes `Shortcut::Paste`. Ctrl+Shift+Insert and plain Insert stay `None`. Ctrl+C (evdev 46) and Ctrl+V (evdev 47) require shift up. Ctrl+Shift+C and Ctrl+Shift+V are not those shortcuts. Ctrl+Q (evdev 16) and ctrl+Left/Right (105 and 106) still ignore shift. A paste request requires `capabilities.paste` and `Focus::Edit`. `on_clipboard_text` stays capability-gated only. It does not read focus. Whole-field copy and cut remain. No selection is added. No caret is added. Ctrl+Insert copies the whole edit, not a selection. Shift+Insert, when paste is allowed, replaces the whole edit.

Today `shortcut_from_key` returns `None` when ctrl is up, so Shift+Insert cannot match. Evdev 110 is not a shortcut, not `KeySpecial`, and not `key_char`. Ctrl+C and Ctrl+V match with shift held: `shortcut_from_key(true, true, 46)` is Copy and `shortcut_from_key(true, true, 47)` is Paste. The shift-up rule and the Insert arms are the plan, not the current function. `paste_shortcut_requests_clipboard` returns `ir.capabilities.paste` and does not read focus. `on_clipboard_text` calls `set_edit` for `Some` text when paste is true, at any focus. `None` does not change the edit. An empty `Some` does replace the edit when paste is true. `copy_text` publishes `session.edit`, including an empty string, only when `capabilities.copy` is true and focus is `Focus::Edit`. Button focus publishes nothing. Cut is unchanged.

## Ctrl+Insert

Sash treats `WM_KEYDOWN` `VK_INSERT` with ctrl down and shift up as copy. Ctrl+Shift+Insert is not copy. The plan's Ctrl+Insert chord matches that modifier rule.

The payload does not match. The chord uses `copy_text`. It publishes the whole edit only when copy is allowed and focus is the edit, including when that string is empty. Button focus publishes nothing. That allow and focus outcome is the same "not the button" result as the subclass. It is not a call to `clipboard_blocked`. A Win32 edit copies its selection. An empty selection does not replace the clipboard. There is no selection and no caret. There is still no `WM_COPY` and no `WM_CHAR` `0x03`.

## Shift+Insert

Sash treats `WM_KEYDOWN` `VK_INSERT` with shift down and ctrl up as paste. The plan's Shift+Insert chord matches that modifier rule. Reaching it means `shortcut_from_key` can no longer return `None` for every key while ctrl is up.

The request gate matches the subclass on focus and on the paste bit. The plan asks for text only when `capabilities.paste` is true and focus is `Focus::Edit`. Button focus does not request. The admitted counter is paste deny, so that counter still does not request. It is not a call to `clipboard_blocked`. The apply path does not match. `on_clipboard_text` stays capability-gated only and is not focus-gated. A `Some` payload still calls `set_edit` when paste is true, including when focus is the button. `None` does not change the edit. An empty `Some` still replaces the edit. When paste is allowed and the edit is focused, Shift+Insert still replaces the whole edit. It does not insert at a caret and it does not replace a selection. There is no `WM_PASTE` and no `WM_CHAR` `0x16`.

## Ctrl+Shift+Insert and plain Insert

Sash `key_operation` returns none for Insert when ctrl and shift are both down, and none when both are up. The plan keeps both chords at `None`. That matches the modifier rule. Neither chord copies or pastes, so no clipboard payload is added.

Plain Insert and Ctrl+Shift+Insert are still not `KeySpecial` and not `key_char`. The plan still emits no character. The gate does not block those keys, so a system edit would still see them. That control has not been launched. There is no caret, so there is no insert mode to toggle. A unit test of `shortcut_from_key` is not that key.

## Ctrl+C and Ctrl+V

Sash copies on `VK_C` only when ctrl is down and shift is up. It pastes on `VK_V` only when ctrl is down and shift is up. Ctrl+Shift+C is not copy. Ctrl+Shift+V is not paste. The plan matches those modifier rules. Today the helper does not: shift does not change evdev 46 or 47. Ctrl+Q and ctrl+Left/Right stay shortcuts with shift held. Sash does not classify Q or the arrows as clipboard operations. The plan does not change them.

Ctrl+C with shift up still publishes the whole edit when copy is allowed and focus is the edit, including an empty string, and publishes nothing for the button. That is not a selection. Ctrl+V with shift up uses the same request rule as Shift+Insert: `capabilities.paste` and `Focus::Edit`. The later `ClipboardText` still replaces the whole edit and still ignores focus. Denied paste still changes nothing. There is still no `WM_COPY`, `WM_PASTE`, `WM_CHAR` `0x03`, or `WM_CHAR` `0x16`.

## Letters that are not shortcuts

`handle_key_press` returns before `key_chars` when `shortcut_from_key` matches. After the plan, Ctrl+Shift+C and Ctrl+Shift+V do not match. With an xkb map, ctrl still yields no `KeyChar`. Without a map, `key_chars` ignores ctrl. `key_char` can emit `C` for evdev 46 with shift and `V` for evdev 47 with shift. Sash does not insert those letters. That insert remains open. The same keymap-free path already applies to other ctrl combos that are not shortcuts, including Ctrl+Shift+X. Insert is not in that table, so this gap is not an Insert glyph.

## Messages still absent

`WM_COPY` (`0x0301`), `WM_PASTE` (`0x0302`), `WM_CUT` (`0x0300`), and `WM_CHAR` `0x03`, `0x16`, and `0x18` are still absent. Sash classifies those messages with no modifier test. The plan does not add them. Whole-field copy and cut remain. No selection is added. No caret is added.

## Not a pass

EMP-M1 is not passed. Windows is not launched. A unit test is not a Win32 edit control and not a Wayland seat. The scripted interaction suite is not a seat event. No ledger row moves, including EMP-M1-25.
