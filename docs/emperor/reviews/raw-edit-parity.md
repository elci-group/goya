# Raw edit parity

Emperor copy and cut publish the edit string. They do not publish the result and they do not parse it. This note compares `tests::noncanonical_edit_is_the_clipboard_text` in `crates/emperor-runtime/src/lib.rs` with `copy_text` and `on_cut` in `crates/emperor-runtime/src/host.rs`, `parse_canonical_i64` and `format_i64` in `crates/sash/src/interactive_text.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, and `edit_proc` in `crates/sash/src/interactive.rs`.

EMP-M1 is not passed. This is not a launched EDIT, not `ToUnicode`, and not a Windows launch. Sash has not been launched. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/clipboard-parity.md` or `docs/emperor/reviews/reject-parity.md`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Parse

`parse_canonical_i64` returns `None` when the text is not `format_i64` of the parsed value. `format_i64` is `value.to_string()`, so `format_i64(0)` is `"0"`. `parse_canonical_i64("00")` and `parse_canonical_i64("-0")` are `None`. `non_canonical_integers_do_not_commit` already asserts both. `copy_text` and `on_cut` do not call the parser. `CounterSession::set_edit` replaces the edit and leaves count, revision, and result unchanged. A rejected edit stays in the field until cut clears it.

## Copy

`copy_text` returns `Some(session.edit)` when `capabilities.copy` is true and focus is Edit, including an empty `Some("")`. Button focus and copy false return `None` and do not change the edit. It does not read `session.result`. This slice does not reverse the empty offer. The named test does not pass `""`.

Sash `edit_proc` calls `clipboard_blocked` and, when the message is not blocked, `CallWindowProcW`. The system edit copies its selection. An empty selection does not replace the clipboard. The helper does not read the clipboard. The subclass has not been launched. Emperor is not that control.

`key_action` returns `Activate` only for `WM_KEYDOWN` (`0x0100`) plus `VK_RETURN` (`13`). It does not know clipboard text. `PlatformEvent::Shortcut(Shortcut::Copy)` calls `copy_text` and, when that returns text, `set_clipboard_text`. It does not read `session.result`.

## Cut

`on_cut` publishes the whole edit when `capabilities.cut` is not `"deny"`, focus is Edit, and the edit is non-empty. `allow` and `default` both take that path. The check does not read `capabilities.copy`. Deny, any other focus, and an empty edit publish nothing and leave the text. There is no selection. `set_edit("")` clears the edit and does not change count, revision, or result. The named test uses the counter's `default`. It does not call `on_cut` after setting copy false. `on_cut_clears_a_focused_edit_unless_blocked` still denies cut on edit `"1"`, not on `"00"`.

Sash cut is `edit_proc`. `clipboard_blocked` blocks a cut only when `cut_mode` is `CUT_DENY`. The subclass then forwards the message. It does not perform the cut. That control has not been launched.

## Wording

`docs/emperor/GY-EMP-001.md` section 6 says a string `parse_canonical_i64` rejects, including empty, `00`, `-0`, and overflow, stays in the edit and does not store. It also says copy and cut of a rejected non-empty edit offer that text, not the result, and that cut clears the edit and does not store.

`docs/emperor/contracts/gy-cross-parity-1.md` says a rejected edit such as `00` leaves the stored count unchanged, and that empty text, `00`, `-0`, and overflow stay in the edit. The older whole-edit sentence stays: copy offers the whole edit when copy is allowed and focus is the edit, including when that string is empty. The same paragraph says a non-canonical edit such as `00` is offered as that text, not as the result `0`, and that cut publishes that same text, clears the edit, and does not store.

## Test

`tests::noncanonical_edit_is_the_clipboard_text` admits the counter and calls `set_edit("00")`. Count stays 0, revision stays 0, and the result stays `"0"`. `host::copy_text` returns `Some("00")` and leaves the edit. `host::on_cut` publishes `Some("00")`, clears the edit, and does not change count, revision, or result. `set_edit("-0")` is offered as `Some("-0")`, and cut publishes `Some("-0")` and clears the edit. The result stays `"0"`. Button focus makes `copy_text` return `None` and makes `on_cut` publish nothing. The edit stays `"00"`. Copy denied returns `None` and leaves `"00"`. Count, revision, and result stay put.

Parent re-ran `tests::noncanonical_edit_is_the_clipboard_text`, `copy_text_follows_the_capability`, and `on_cut_clears_a_focused_edit_unless_blocked`: 1 passed each, 29 filtered. A unit test is not a launched EDIT, not `ToUnicode`, and not a Windows launch. EMP-M1-09 stays unmet. No ledger row moves.
