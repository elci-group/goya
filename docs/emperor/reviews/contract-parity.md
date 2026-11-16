# Contract parity

The documents are being corrected to the working tree. The host is not changing. This note compares that wording with `sash::interactive::key_action` in `crates/sash/src/interactive_keys.rs`, `goya_parity::keys::KeyController` in `crates/goya-parity/src/keys.rs`, and `crates/emperor-runtime/src/host.rs`. Cut chords are also checked against `key_operation` in `crates/sash/src/interactive_clipboard.rs`. Button activation is checked against `command_activates` in `crates/sash/src/interactive_commit.rs`.

EMP-M1 is not passed. No ledger row moves. A wording change is not a Windows launch and not a Wayland seat.

## Space

`key_action` returns `Dispatch` for `VK_SPACE` (`0x20`). It returns `Activate` only for `WM_KEYDOWN` (`0x0100`) and `VK_RETURN`. `KeyController::on_key`, inside `Dispatch`, activates only when focus is the button, the message is `WM_KEYDOWN`, and the wparam is `VK_SPACE`. Space on the edit returns `Dispatch` and leaves the edit at `1`.

The host matches that controller. `on_char` for `' '` calls `on_key` with `WM_KEYDOWN`, `VK_SPACE`, and shift false. It does not call `set_edit`. Space on the edit does not change the edit. Space on the button activates. That does not match the old sentence that space may be inserted through `CounterSession::set_edit`.

## Press and release

A press on `window.add`, or on role `button`, sets `Focus::Button` and records that id on `PointerArm`. It does not call `press_add`. A release whose hit is that same id calls `press_add`. A release on any other id does not. A press on `window.amount`, or on role `edit`, sets `Focus::Edit`, clears the recorded id, and does not activate. The record is not a pointer grab. It does not call a pointer-capture API.

`command_activates` is true only when the control id is the button and the note is 0. The generated window procedure runs that test on `WM_COMMAND`. Sash activation is still `WM_COMMAND` note 0 on release inside the control. The host is that rule on the hit map. `press_add` still enters `on_key` with `VK_RETURN`, so the integers stay on `KeyController`. The gesture is not a key. It is not a seat event. EMP-M1-25 stays unmet.

## Copy

`copy_text` returns the whole edit only when `capabilities.copy` is true and focus is `Focus::Edit`, including an empty string. Button focus returns nothing and does not change the edit. The host does not call `clipboard_blocked`.

A Win32 edit copies a selection. An empty selection does not replace the clipboard. Emperor still has no selection, so the host publishes the whole field, including an empty string. That is the mismatch. This note does not ask for a caret.

## Paste

A paste request requires paste and edit focus. `paste_shortcut_requests_clipboard` is true only when `capabilities.paste` is true and focus is `Focus::Edit`. Button focus does not request. Apply stays capability-only. `on_clipboard_text` does not read focus. `Some` text calls `set_edit` when paste is true. `None` does not change the edit. An empty `Some` still replaces the edit. Whole-field replacement remains.

## Cut and Delete

`key_operation` is a cut for `VK_X` (`0x58`) when ctrl is down and shift is up, and for `VK_DELETE` (`0x2E`) when shift is down and ctrl is up. Plain Delete, Ctrl+Delete, and Ctrl+Shift+Delete are not cuts. Ctrl+Shift+X is not a cut. The comment says Delete alone removes text without copying it.

The host matches Sash on which of those chords are clipboard operations. Shift+Delete without ctrl calls `on_cut`. Ctrl+X without shift arrives as `Shortcut::Cut` and calls `on_cut`. Plain Delete, Ctrl+Delete, and Ctrl+Shift+Delete do not. Emperor does not remove text on plain Delete because there is no caret. That is not Sash's system-edit delete.

## Messages still absent

`WM_COPY` (`0x0301`), `WM_PASTE` (`0x0302`), `WM_CUT` (`0x0300`), and `WM_CHAR` `0x03`, `0x16`, and `0x18` stay absent from the host. Sash `operation` classifies those messages with no modifier test. The host does not send them.

## Not a pass

EMP-M1 is not passed. The host is not changing. Nothing here asks for that host to be reverted. No ledger row moves. A wording change is not a Windows launch and not a Wayland seat. EMP-M1-25 stays unmet.
