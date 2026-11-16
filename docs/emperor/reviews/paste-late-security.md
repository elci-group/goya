# Paste late security

`tests::paste_payload_applies_after_focus_moves` admits the counter and sets `capabilities.paste` true on that local ir. Focus starts Edit, so `paste_shortcut_requests_clipboard` is true. Focus is then Button, so the request helper is false and the edit is still `"1"`. `on_clipboard_text` with `Some("9")` returns `Changed`, the edit becomes `"9"`, focus stays Button, and count, revision, and result stay 0, 0, `"0"`. `Some("")` then clears the edit. `None` leaves `"1"` after `set_edit("1")`. A fresh admit with paste still false and focus Button ignores `Some("9")`. This note does not change the gap ledger and does not pass any row.

Reviewed `paste_shortcut_requests_clipboard` and `on_clipboard_text` in `crates/emperor-runtime/src/host.rs`, and `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet.

## Not a store

Applying `"9"` after focus moves is not a store and not a parser bypass. `on_clipboard_text` does not call `parse_canonical_i64` and does not call `press_add`. `set_edit` assigns `self.edit` and does not change count, revision, or result. The result stays `"0"`. The payload is the edit string, not the stored count.

## No second paste policy

The request is the focus gate because the payload can arrive after focus moves. `paste_shortcut_requests_clipboard` is `capabilities.paste && focus == Edit`. Button focus does not request. Denied paste does not request. `Shortcut::Paste` calls `request_clipboard_text` only when that helper is true, and the step stays `Unchanged`. `on_clipboard_text` calls `set_edit` for `Some` text when paste is true. It does not read focus. Do not add a focus check to `on_clipboard_text`. Do not filter the payload. Empty `Some("")` still replaces the edit. Denied paste ignores every payload, including after focus moves. No second paste policy.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. Describing filesystem, network, subprocess, or web_content does not install Landlock or seccomp. EMP-M1-11 stays unmet. No ledger row moves. This note does not claim a signed release, sandbox enforcement, or SOTA.

## Not IME

This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. Ignoring those events is not support. EMP-M1-09 stays unmet.

## Not a pass

`paste_shortcut_requests_clipboard` and `on_clipboard_text` do not open a window. `tests::paste_payload_applies_after_focus_moves` calls those helpers on an admitted controller. Parent re-ran it and `denied_paste_ignores_shortcut_empty_and_payload`: 1 passed each, 31 filtered. A unit test is not a launched clipboard, a Wayland seat, or a Win32 edit. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
