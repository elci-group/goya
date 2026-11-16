# Empty copy parity

Emperor copy of an empty edit offers that empty string. Cut of an empty edit publishes nothing. This note looks for `tests::empty_edit_copy_offers_an_empty_string` in `crates/emperor-runtime/src/lib.rs` and compares `copy_text` and `on_cut` in `crates/emperor-runtime/src/host.rs`, `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, and `edit_proc` in `crates/sash/src/interactive.rs`.

`tests::empty_edit_copy_offers_an_empty_string` covers that offer. GY-EMP-001 section 6 says an empty edit still copies as an empty string, and that cut of an empty edit publishes nothing.

EMP-M1 is not passed. This is not a launched EDIT, not `ToUnicode`, and not a Windows launch. Sash has not been launched. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/clipboard-parity.md` or `docs/emperor/reviews/raw-edit-parity.md`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Copy

`copy_text` returns `Some(session.edit)` when `capabilities.copy` is true and focus is Edit. An empty edit is `Some("")`. Button focus and copy false return `None` and do not change the edit. It does not read `session.result`. `set_edit("")` replaces the edit and leaves count, revision, and result unchanged. Do not change the empty offer to `None`.

## Cut

`on_cut` publishes the whole edit only when `capabilities.cut` is not `"deny"`, focus is Edit, and the edit is non-empty. `allow` and `default` both take that path. An empty edit returns `Step::Unchanged` and clipboard `None` and does not call `set_edit`. Deny and any other focus do the same and leave the text. There is no caret and no selection. The check does not read `capabilities.copy`.

## Sash

Sash `edit_proc` calls `clipboard_blocked` and, when the message is not blocked, `CallWindowProcW`. The system edit copies its selection. An empty selection does not replace the clipboard. The helper does not read the clipboard. That control has not been launched. Emperor whole-field empty copy returns `Some("")`. That is the difference. Do not ask to change Emperor to `None`.

`key_action` returns `Activate` only for `WM_KEYDOWN` (`0x0100`) plus `VK_RETURN` (`13`). It does not know clipboard text. `PlatformEvent::Shortcut(Shortcut::Copy)` calls `copy_text` and, when that returns `Some`, `set_clipboard_text`. It does not read `session.result`. An empty `Some("")` is still `Some`, so that arm publishes it. The step stays `Unchanged`.

## Wording

`docs/emperor/contracts/gy-cross-parity-1.md` already says copy offers the whole edit when copy is allowed and focus is the edit, including when that string is empty. Keep that sentence. The same paragraph says a non-canonical edit such as `00` is offered as that text, not as the result `0`, and that cut publishes that same text, clears the edit, and does not store. Button focus offers nothing and does not change the edit. The offer is not a selection.

`docs/emperor/GY-EMP-001.md` section 6 says a string `parse_canonical_i64` rejects, including empty, `00`, `-0`, and overflow, stays in the edit and does not store. It also says copy and cut of a rejected non-empty edit offer that text, not the result, and that cut clears the edit and does not store. It also says an empty edit still copies as an empty string when copy is allowed and focus is the edit, and that cut of an empty edit publishes nothing.

## Test

`tests::empty_edit_copy_offers_an_empty_string` sets the edit to `""`. `copy_text` returns `Some("")` and leaves the edit empty. Count, revision, and result stay 0, 0, `"0"`. `on_cut` is `Unchanged` with clipboard `None`. Button focus offers nothing. Copy denied offers nothing. Parent re-ran it, `copy_text_follows_the_capability`, and `on_cut_clears_a_focused_edit_unless_blocked`: 1 passed each, 32 filtered. `copy_text_follows_the_capability` still copies the admitted edit `"1"`. `on_cut_clears_a_focused_edit_unless_blocked` still cuts a non-empty edit and records an empty cut as `Unchanged`. A unit test is not a launched EDIT, not `ToUnicode`, and not a Windows launch. EMP-M1-09 stays unmet. No ledger row moves.
