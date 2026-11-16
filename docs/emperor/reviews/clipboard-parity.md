# Clipboard parity

The working tree now has the edit-focus copy gate and Ctrl+X cut described below. This note was written against commit `17721b3` plus that plan.

Commit `17721b35512ff1506e5d16f2a859e07462a3b838`. This note compares that plan with `operation` and `key_operation` in `crates/sash/src/interactive_clipboard.rs`, the edit subclass in `crates/sash/src/interactive.rs`, and `copy_text`, `cut_edit`, `on_pointer_press`, and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. `shortcut_from_key` in `crates/goya-wayland/src/lib.rs` maps ctrl+c, ctrl+v, and ctrl+q only. Ctrl+x is not a shortcut. `key_action` returns `Dispatch` for these wparams. They are not Enter and they are not Space.

EMP-M1 is not passed. The generated Win32 image has not been launched. A unit test is not a Win32 edit control and not a Wayland seat. The scripted interaction suite is not a seat event. This note does not pass EMP-M1-25 or any other ledger row. It does not replace `interaction-parity.md`.

## Plan

Copy publishes the whole edit only when focus is `Focus::Edit` and `ir.capabilities.copy` is true. Button focus publishes nothing. Ctrl+X (evdev 45, ctrl, no shift) becomes `Shortcut::Cut` and uses `cut_edit`. Ctrl+Shift+X stays inert. Shift+Delete behaviour is unchanged. No selection is added. No caret is added.

Today `Shortcut` is Copy, Paste, Quit, SessionNext, and SessionPrevious. `shortcut_from_key` does not read shift. `copy_text` returns `session.edit` whenever `capabilities.copy` is true, at any focus, including an empty string. It does not read `session.copy_allowed` and does not call `clipboard_blocked`. `cut_edit` runs only from Shift+Delete (`KeySpecial::Delete`, shift, no ctrl). `deny` publishes nothing and leaves the edit. `allow` and `default` publish the whole edit and clear it when focus is the edit and the edit is not empty. Button focus does not cut. Any other `Shortcut` hits the host catch-all and changes nothing. `open_admitted` rejects a `copy_allowed` mismatch with `capabilities.copy`. The counter does not state cut, so cut stays `default`. Paste on that counter is deny.

## WM_COPY / Ctrl+C / Ctrl+Insert

Sash treats `WM_COPY` (`0x0301`) as copy with no modifier test. `WM_CHAR` `0x03` is copy. `WM_KEYDOWN` with `VK_C` (`0x43`) or `VK_INSERT` (`0x2D`), ctrl down, and shift up, is copy. `clipboard_blocked` returns true for copy only when copy is not allowed. The gate is the edit subclass. `key_action` is `Dispatch`, so the pump translates and dispatches to the focused HWND. A button-focused chord does not reach the subclass, and the edit text is not published. When the message is not blocked, the system edit copies its selection. An empty selection does not replace the clipboard. The helper does not read the clipboard. The subclass has not been launched.

Ctrl+C matches the plan on the allow decision and the focus decision. The seat already emits `Shortcut::Copy` for ctrl and evdev 46. The plan publishes only for edit focus with `capabilities.copy` true, and publishes nothing for button focus. That is the same allow/deny bit the admitted scene stores, and the same "not the button" outcome as the subclass. It is not a call to `clipboard_blocked`. Seat ctrl is bit 2 of `mods_depressed`, not `GetKeyState(VK_CONTROL)`.

The payload does not match. The plan publishes the whole edit, not a selection. There is still no `WM_COPY` and no `WM_CHAR` `0x03`. The plan does not require shift to be up for copy, so Ctrl+Shift+C remains `Shortcut::Copy` whenever the seat reports ctrl. Sash does not classify Ctrl+Shift+C as copy.

Ctrl+Insert is not in this plan. Evdev Insert (110) is not a shortcut, not `KeySpecial`, and not `key_char`. It still produces no copy.

## WM_PASTE / Ctrl+V / Shift+Insert

Sash treats `WM_PASTE` (`0x0302`) and `WM_CHAR` `0x16` as paste. `WM_KEYDOWN` `VK_V` (`0x56`) with ctrl and without shift is paste. Shift+Insert without ctrl is paste. The gate blocks paste when paste is not allowed. The admitted counter is paste deny. A button-focused chord does not reach the edit. An allowed paste is the system edit inserting at the caret, or replacing the selection. That control has not been launched.

The plan does not change paste. Denied paste still matches the counter: `Shortcut::Paste` requests text only when `capabilities.paste` is true, and `ClipboardText` calls `set_edit` only for `Some` text when paste is true. `None` does not change the edit. An empty `Some` does replace the edit when paste is true. The plan adds no focus check, so button focus is still not the subclass rule. An allowed payload still replaces the whole edit. There is no caret. There is no `WM_PASTE` and no `WM_CHAR` `0x16`. Ctrl+Shift+V remains `Shortcut::Paste`. Sash does not classify that chord as paste.

Shift+Insert is not in this plan. Evdev 110 with shift still yields no paste.

## WM_CUT / Ctrl+X / Shift+Delete

Sash treats `WM_CUT` (`0x0300`) and `WM_CHAR` `0x18` as cut. `WM_KEYDOWN` `VK_X` (`0x58`) with ctrl and without shift is cut. Shift+Delete without ctrl (`VK_DELETE` `0x2E`) is cut. The gate blocks a cut only when the mode is `CUT_DENY`. `CUT_ALLOW` and `CUT_DEFAULT` pass the message through. The counter leaves cut at `CUT_DEFAULT`. The system edit then cuts its selection. Button focus does not reach the subclass. Ctrl+Shift+X is not a cut. Ctrl+Shift+Delete is not a cut. The subclass has not been launched.

Ctrl+X matches the plan on the chord and the policy. Evdev 45 with ctrl and without shift becomes `Shortcut::Cut` and uses `cut_edit`. `deny` publishes nothing and leaves the edit. `allow` and `default` publish and clear when focus is the edit and the text is not empty. Button focus does not cut. Ctrl+Shift+X stays off that path, which matches the gate. The effect does not match. The plan still cuts the whole edit, not a selection. There is no `WM_CUT` and no `WM_CHAR` `0x18`.

Before the plan, evdev 45 is not a shortcut. With an xkb map, ctrl produces no `KeyChar`, so Ctrl+X changes nothing. Without a map, `key_chars` ignores ctrl and can emit `x`. `handle_key_press` returns before that path when `shortcut_from_key` matches, which is how Copy and Paste already work. The plan stops the `x` insert only for ctrl and no shift. Ctrl+Shift+X is specified as inert, meaning not a cut. On the keymap-free path that chord can still become `KeyChar('X')`, because `key_chars` does not see ctrl. Sash does not insert `X`. That insert remains open. `shortcut_from_key` does not take shift today. The inert claim is the plan, not the current function.

Shift+Delete is unchanged, so the current host behaviour stands. It matches the gate on deny versus `allow` and `default`, on edit focus, and on refusing Ctrl+Shift+Delete. It does not match the system edit. The host publishes and clears the whole edit even when a selection would have been empty. Skipping an empty string is not a selection test. A unit test of `on_modified_special` is not a seat Delete and not `VK_DELETE`.

## Delete alone

Sash does not treat Delete without shift as a clipboard operation. `key_operation` returns none. The comment says Delete alone removes text without copying it. The gate does not block it, including when cut is deny. Ctrl+Delete is not a cut either. The system edit deletes the selection, or the character after the caret when there is no selection, and does not copy. That control has not been launched.

The plan does not change Delete alone. Plain `KeySpecial::Delete` stays off `cut_edit`. The host ignores that event. The edit does not change and nothing is copied. That is not Sash's delete. There is no caret, so there is no character to remove. Delete alone must not be recorded as a cut.

## Whole edit, not a selection

Emperor still copies or cuts the whole edit rather than a selection. The plan does not add a selection or a caret. Copy, when it publishes, publishes `session.edit`, including when that string is empty. Cut, when it runs, publishes that same string and clears the field. A Win32 edit copies or cuts the selection only. Ctrl+Insert and Shift+Insert are not in this plan.

The focus the plan reads is `KeyController`'s enum. Tab toggles it through `key_action`. `on_pointer_press` sets `Focus::Button` for the button and `Focus::Edit` for the edit, and does not activate. `on_pointer_release` calls `press_add` only when the release hit is the armed button id. A press outside the button clears the arm. Neither call copies or cuts. That arm is not a seat grab and it is not `SetFocus`. EMP-M1-25 stays unmet.

## Not a pass

EMP-M1 is not passed. Windows is not launched. A unit test is not a Win32 edit control and not a Wayland seat. The scripted interaction suite is not a seat event. No ledger row moves, including EMP-M1-25.
