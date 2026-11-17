# Paste raw parity

Paste allow is one decision. `on_clipboard_text` replaces the whole edit through `set_edit` when paste is true. It does not parse and does not call `press_add`. Sash paste is `WM_PASTE`, `WM_CHAR` `0x16`, Ctrl+V, or Shift+Insert, gated by `paste_allowed` in the edit subclass. That control has not been launched. Emperor does not fake those messages. This note compares `tests::paste_payload_is_not_rewritten` in `crates/emperor-runtime/src/lib.rs` with `on_clipboard_text` in `crates/emperor-runtime/src/host.rs`, `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`, `parse_canonical_i64` and `format_i64` in `crates/sash/src/interactive_text.rs`, `on_char` in `crates/emperor-runtime/src/host.rs`, and `clipboard_blocked` plus `edit_proc` in `crates/sash/src/interactive_clipboard.rs` and `crates/sash/src/interactive.rs`.

EMP-M1 is not passed. This is not a launched clipboard, not `ToUnicode`, and not a Windows launch. Sash has not been launched. EMP-M1-09 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Allow

`paste_shortcut_requests_clipboard` is true only when `capabilities.paste` is true and focus is `Focus::Edit`. That helper is the request. It does not apply text. `on_clipboard_text` does not read focus. `Some` text calls `set_edit` when paste is true and returns `Step::Changed`. Any other payload, including when paste is false, returns `Step::Unchanged` and does not call `set_edit`. `clipboard_blocked` blocks a paste operation only when `paste_allowed` is false. `edit_proc` drops that message and otherwise calls `CallWindowProcW`. The subclass does not replace the edit. The allow and deny decision is the same. The image has not been launched. Do not add a focus check. Do not filter the payload.

## Replace

`set_edit` assigns the string. It does not change count, revision, or result. `"00"` is not canonical. `parse_canonical_i64("00")` is `None` because `format_i64(0)` is `"0"`. Paste must not run that parser. The edit becomes `"00"` and the result stays `"0"`. A newline payload is not Enter. Enter is `on_char('\n')`, which calls `press_add`. Paste must not take that path. The edit becomes `"\n"` and the count stays 0. `"1\u{200D}"` replaces the edit as that text. It is not an `on_char` append and it does not store.

## Late paste

`tests::paste_payload_applies_after_focus_moves` already applies `"9"` and `""` after focus moves to the button. This slice stays on the edit. It does not move focus and it does not call the request helper. It applies `"00"`, `"\n"`, and `"1\u{200D}"`.

## Wording

`docs/emperor/GY-EMP-001.md` section 6, after "The request is the focus gate.", says: "An allowed payload is not filtered. `00`, a newline, and a zero-width joiner replace the edit as that text and do not store."

`docs/emperor/contracts/gy-cross-parity-1.md`, after "`on_clipboard_text` does not read focus.", says: "The payload is not filtered and is not parsed. `00`, a newline, and a zero-width joiner replace the edit as that text and do not store."

## Test

`tests::paste_payload_is_not_rewritten` admits the counter and sets paste true. Focus stays `Focus::Edit`. The edit starts as `"1"`. Count, revision, and result are 0, 0, `"0"`. `on_clipboard_text(Some("00"))` returns `Step::Changed`. The edit is `"00"`, which is not the result `"0"`. Focus stays Edit. Count, revision, and result stay 0, 0, `"0"`. `Some("\n")` returns `Step::Changed`, the edit is `"\n"`, and the count stays 0. `Some("1\u{200D}")` returns `Step::Changed` and the edit is `"1\u{200D}"`. Count, revision, and result stay 0, 0, `"0"`. A fresh admit with paste still false and focus still Edit ignores `Some("00")` and `Some("\n")`. The edit stays `"1"`. Count, revision, and result stay 0, 0, `"0"`.

The edit subclass has not been launched. Emperor does not send `WM_PASTE`, `WM_CHAR` `0x16`, Ctrl+V, or Shift+Insert. A unit test is not a launched clipboard. EMP-M1 stays uncertified. No ledger row moves.
