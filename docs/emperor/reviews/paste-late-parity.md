# Paste late parity

An allowed clipboard payload replaces the edit after focus has left it. The request is the focus gate. This note compares `tests::paste_payload_applies_after_focus_moves` in `crates/emperor-runtime/src/lib.rs` with `paste_shortcut_requests_clipboard` and `on_clipboard_text` in `crates/emperor-runtime/src/host.rs`, `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, and `clipboard_blocked` plus `edit_proc` in `crates/sash/src/interactive_clipboard.rs` and `crates/sash/src/interactive.rs`.

EMP-M1 is not passed. This is not a launched clipboard, not `ToUnicode`, and not a Windows launch. Sash has not been launched. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/clipboard-parity.md` or `docs/emperor/reviews/insert-parity.md`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Request

`paste_shortcut_requests_clipboard` is true only when `capabilities.paste` is true and focus is `Focus::Edit`. Button focus returns false. `Shortcut::Paste` calls `request_clipboard_text` only when that helper is true, then returns `Step::Unchanged`. That is the request gate. The helper does not apply text.

## Apply

`on_clipboard_text` calls `set_edit` for `Some` text when paste is true and returns `Step::Changed`. It does not read focus. `None` returns `Step::Unchanged` and does not call `set_edit`. An empty `Some` still replaces the edit when paste is true. Paste false ignores `Some("9")`, an empty `Some`, and `None`. Do not ask to gate apply on focus.

`set_edit` replaces the edit and does not change count, revision, or result. `"9"` is not an activation. `key_action` returns `Activate` only for `WM_KEYDOWN` (`0x0100`) plus `VK_RETURN` (`13`). It does not know clipboard text.

## Sash

`clipboard_blocked` blocks a paste operation only when paste is not allowed. `edit_proc` drops that message and otherwise calls `CallWindowProcW`. The gate is the edit subclass, so a button-focused chord does not reach it. The helper does not read the clipboard and does not replace the edit. That image has not been launched. Emperor is not that control.

## Wording

`docs/emperor/contracts/gy-cross-parity-1.md` says paste, when denied, does not request clipboard text and ignores a later clipboard text event. When paste is allowed, a request is made only while focus is the edit. A later `Some` payload still replaces the edit when paste is true, even if focus has moved. `on_clipboard_text` does not read focus. The Sash image enforces the same allow and deny decision in the edit subclass and has not been launched. Keep that sentence.

`docs/emperor/GY-EMP-001.md` section 6 says an allowed paste payload still replaces the edit after focus has moved, and that the request is the focus gate. Section 4.2 still says copy allowed offers the edit text and paste denied changes nothing. The programme sentence does not name `on_clipboard_text`. The contract sentence does.

## Test

`tests::paste_payload_applies_after_focus_moves` admits the counter, sets paste true while focus is Edit, and sees the request helper return true. It then sets focus to Button. The helper returns false and the edit stays `"1"`. `on_clipboard_text(Some("9"))` returns `Step::Changed` and replaces the edit with `"9"`. Focus stays Button. Count, revision, and result stay 0, 0, `"0"`. `Some("")` clears the edit and leaves the count at 0. After `set_edit("1")`, `None` leaves the edit at `"1"` and the count at 0. With paste false and focus Button, the helper is false and `Some("9")` leaves the edit at `"1"`. Count, revision, and result stay 0, 0, `"0"`.

Parent re-ran `tests::paste_payload_applies_after_focus_moves` and `denied_paste_ignores_shortcut_empty_and_payload`: 1 passed each, 31 filtered. A unit test is not a launched clipboard, not `ToUnicode`, and not a Windows launch. EMP-M1-09 stays unmet. No ledger row moves.
