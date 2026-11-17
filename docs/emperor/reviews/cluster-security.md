# Cluster security

`on_char` already classifies newline, space, backspace, and every other character. This slice does not change that function. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. Do not change `host.rs`. The test is `tests::delete_does_not_append_and_backspace_drops_one_cluster` in `crates/emperor-runtime/src/lib.rs`, immediately after `tests::digits_append_only_on_the_edit`. This note does not change the gap ledger and does not pass any row.

Reviewed `on_char` and `delete_last_grapheme` in `crates/emperor-runtime/src/host.rs` and `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`. The threat model is `docs/emperor/threat-model.md`. The later read of section 6 in `docs/emperor/GY-EMP-001.md` and of `docs/emperor/contracts/gy-cross-parity-1.md` includes: "Delete (`U+007F`) does not append. A zero-width joiner appends. Backspace removes one grapheme cluster, including a combining mark or that joiner, and does not store."

## What on_char does

`'\n'` calls `press_add`. `' '` calls `on_key` with `WM_KEYDOWN` and `VK_SPACE`. `'\u{8}'` deletes one grapheme through `delete_last_grapheme` and `set_edit` only while focus is Edit. Any other character appends only when focus is Edit and `!ch.is_control()`. `set_edit` replaces the edit string. Count, revision, and result stay put.

`is_control` is Unicode general category Cc. U+007F is a control, so it does not append and it does not enter the backspace arm. A control must not be turned into a deletion. U+200D is a format character (Cf), not a control, so it appends. A format character must not be dropped just because it is invisible. Do not filter the joiner.

`delete_last_grapheme` walks `grapheme_indices(true)` and keeps the prefix before the last extended grapheme cluster. One backspace removes one cluster, not a word. `"e\u{0301}x"` already loses only `x` in `backspace_drops_one_grapheme`. An extended grapheme cluster does not break before ZWJ or before a combining mark, so `"1\u{200D}"` is one cluster and `"e\u{0301}"` is one cluster. One backspace clears that cluster. That is still one cluster. Button focus does not append and does not delete. There is no caret. Do not add one. Do not treat Delete as backspace. Do not delete a word.

## What the test locks

The admitted counter starts on the edit, with edit `"1"`, count 0, revision 0, and result `"0"`.

On the edit, `'\u{7f}'` returns `Step::Unchanged` and the edit stays `"1"`. `'\u{200D}'` returns `Step::Changed` and the edit is `"1\u{200D}"`. `'\u{8}'` returns `Step::Changed` and the edit is `""`. Count, revision, and result stay 0, 0, and `"0"`. `set_edit("e\u{0301}")` then `'\u{8}'` returns `Step::Changed` and the edit is `""`. Focus stays Edit. Count, revision, and result stay put.

`set_edit("1")` and `on_tab` move focus to the button. The same three characters, `'\u{7f}'`, `'\u{200D}'`, and `'\u{8}'`, each return `Step::Unchanged`. The edit stays `"1"`. Count, revision, and result stay put. Focus stays Button.

## What it does not prove

This is not IME, not `zwp_text_input_v3`, and not EMP-M1-09. `ignore_ime` still returns `Step::Unchanged`. The test calls `on_char`. It does not deliver preedit or commit. A combining cluster planted with `set_edit`, and a joiner appended as a `char`, are not a real input method. Do not move that row.

The test does not call `scene_frame` or `present_dirty`. A unit test is not a presented buffer. EMP-M1-25 stays unmet.

It does not send `'\n'` or `' '`. Those arms stay as they are. It does not drive U+0301 through `on_char`. The combining cluster arrives through `set_edit`. It locks U+200D, not every format character. It does not see `KeySpecial::Delete`. U+007F on this path is not the Delete key, and that key must not be implemented as backspace.

No sandbox is installed. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. EMP-M1-11 stays unmet.

## Not a pass

This note does not mark a ledger row passed. No ledger row moves. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.
