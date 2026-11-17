# Paste raw security

`git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. `tests::paste_payload_is_not_rewritten` sits immediately after `tests::paste_payload_applies_after_focus_moves`. It keeps focus on the edit. The late-paste test is the one that moves focus. This note does not change the gap ledger and does not pass any row.

Reviewed `on_clipboard_text` and the `ClipboardText` arm of `on_event` in `crates/emperor-runtime/src/host.rs`, and `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`.

## What apply does

`on_clipboard_text` calls `set_edit` for any `Some` text when `capabilities.paste` is true, including `"00"`, `"\n"`, and `"1\u{200D}"`, and returns `Step::Changed`. It does not read focus. It does not call `parse_canonical_i64`. It does not call `press_add`. It does not call `on_char`.

`set_edit` assigns `self.edit` and leaves count, revision, and result unchanged. `"00"` stays `"00"`. It is not rewritten to the result `"0"`. A newline payload replaces the edit. It is not Enter. `on_char('\n')` is the path that calls `press_add`. The clipboard arm does not. A zero-width joiner stays in the string. The payload is not filtered and is not parsed.

`PlatformEvent::ClipboardText` calls only `on_clipboard_text`. `Shortcut::Paste` requests clipboard text only when paste is true and focus is the edit, then returns `Step::Unchanged`. That arm does not write the edit.

Denied paste is the other match arm. It returns `Step::Unchanged` and does not call `set_edit`. There is no second write for a denied payload.

Section 6 of `docs/emperor/GY-EMP-001.md` says: "An allowed payload is not filtered. `00`, a newline, and a zero-width joiner replace the edit as that text and do not store."

`docs/emperor/contracts/gy-cross-parity-1.md` says: "The payload is not filtered and is not parsed. `00`, a newline, and a zero-width joiner replace the edit as that text and do not store."

Do not filter controls. Do not strip the joiner. Do not canonicalize the text. Do not treat a newline payload as Enter.

## What the test locks

With paste set true on the local ir, focus stays Edit and the edit starts at `"1"`. `Some("00")` returns `Changed`, the edit is `"00"`, and that string is not the result `"0"`. Count, revision, and result stay 0, 0, `"0"`. `Some("\n")` returns `Changed`, the edit is `"\n"`, focus stays Edit, and the count stays 0. `Some("1\u{200D}")` returns `Changed`, the edit is that text, and the count stays 0.

A fresh admit leaves paste false and focus Edit. `Some("00")` and `Some("\n")` return `Unchanged`. The edit stays `"1"`. Count, revision, and result stay 0, 0, `"0"`.

## What it does not prove

The test calls `on_clipboard_text` on an admitted controller. It does not enter `on_event`, so it does not show a `ClipboardText` event, a clipboard request, or `request_clipboard_text`. It does not move focus. It does not pass the joiner on the deny path. The deny arm is still the same non-writing arm for every payload. A unit test is not a launched clipboard, not `WM_PASTE`, and not a Win32 edit. `host.rs` is unchanged. This is not a sandbox. No ledger row moves. This note does not pass any row.
