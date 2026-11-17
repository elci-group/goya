# Cluster parity

Backspace removes one grapheme cluster and does not store. U+007F does not append. A zero-width joiner does append. This note compares that rule with `on_char` and `delete_last_grapheme` in `crates/emperor-runtime/src/host.rs`, with `tests::delete_does_not_append_and_backspace_drops_one_cluster` in `crates/emperor-runtime/src/lib.rs`, and with the sentences in `docs/emperor/GY-EMP-001.md` section 6 and `docs/emperor/contracts/gy-cross-parity-1.md`.

EMP-M1 is not passed. These mutations are a Linux shell around the shared session. They are not a launched Sash `EDIT` and they are not IME. There is no caret. EMP-M1-09 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Shared rule

The older sentences stay. Non-control characters other than space append while the edit is focused. Backspace deletes the last Unicode grapheme and does not change the count. A control other than newline and backspace does not change the edit. The added sentence names U+007F, U+200D, and a combining cluster. Both documents say:

> Delete (`U+007F`) does not append. A zero-width joiner appends. Backspace removes one grapheme cluster, including a combining mark or that joiner, and does not store.

In `GY-EMP-001` section 6 that sentence follows "A control other than newline and backspace does not change the edit." In `gy-cross-parity-1.md` it follows "Backspace deletes the last Unicode grapheme of the edit and does not change the count."

`on_char` sends newline to `press_add` and space to `KeyController::on_key` with `WM_KEYDOWN` and `VK_SPACE`. `'\u{8}'` calls `delete_last_grapheme` and `set_edit` only while focus is the edit. Any other character appends only while focus is the edit and `!ch.is_control()`. `set_edit` replaces the edit and does not change count, revision, or result. `delete_last_grapheme` keeps the prefix before the last index from `grapheme_indices(true)`. This note does not ask to change `delete_last_grapheme` or `on_char`.

## What the test locks

`tests::delete_does_not_append_and_backspace_drops_one_cluster` follows `tests::digits_append_only_on_the_edit`. It admits the counter with focus on the edit and the edit `"1"`. U+007F is `Step::Unchanged` and leaves `"1"`. U+200D is `Step::Changed` and the edit becomes `"1\u{200D}"`. Count stays 0, revision stays 0, and the result stays `"0"`. One backspace is `Step::Changed` and the edit becomes `""`, so the joiner is not left as its own stored character. Count, revision, and result stay put.

It then sets the edit to `"e\u{0301}"` and backspaces that cluster to `""`, not to `"e"`. Count, revision, and result stay put. Focus stays the edit. It sets the edit back to `"1"`, moves focus to the button, and feeds U+007F, U+200D, and backspace. Each is `Step::Unchanged`. The edit stays `"1"`. Focus stays the button. Nothing is stored.

`host::tests::backspace_drops_one_grapheme` stays the helper test. It still maps `"12"` to `"1"`, `"e\u{0301}x"` to `"e\u{0301}"`, `"a"` to `""`, and `""` to `""`.

## Frame test

`tests::focused_frame_uses_the_live_session` already sets `"e\u{0301}x"` and backspaces it to `"e\u{0301}"`, then backspaces again on the button and leaves that cluster. It also checks the live frame. This slice does not. It backspaces the remaining cluster `"e\u{0301}"` to `""`, appends U+200D, and refuses U+007F. That backspace is one cluster off the end of the whole string. It is not `KeySpecial::Delete` and not a caret.

## Unlaunched

`crates/sash/src` has no grapheme-cluster helper. The system `EDIT` has not been launched. This shell is not that control. This note does not ask Emperor to switch to UTF-16 code-unit removal to match it. A code-unit pop of `"e\u{0301}"` would leave `"e"`, and a code-unit pop of `"1\u{200D}"` would leave `"1"`. The test locks the cluster results instead.

Ctrl+Backspace stays one `'\u{8}'` because `key_chars` returns that for evdev 14 before the ctrl check. This note does not ask to special-case it. There is no caret. Preedit and commit still go to `ignore_ime`. EMP-M1-09 stays unmet. A unit test is not a Wayland seat and not a Win32 edit. No ledger row moves. This note does not mark a Parity Lab phase met.
