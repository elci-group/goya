# Min parity

Emperor stores only through `KeyController::activate`, which calls Sash `parse_canonical_i64` and `sash::interactive::commit`. This note compares that call with `parse_canonical_i64` in `crates/sash/src/interactive_text.rs`, `activation` in `crates/sash/src/interactive_commit.rs`, `eval` and `commit` in `crates/sash/src/interactive.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, and `on_char` plus `press_add` in `crates/emperor-runtime`.

EMP-M1 is not passed. This is not a launched EDIT, not `ToUnicode`, and not a Windows launch. Sash has not been launched. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/overflow-parity.md` or `docs/emperor/reviews/reject-parity.md`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Canonical decimal

`parse_canonical_i64` returns `None` when the text is empty, when `text.len() > 20`, when any byte is other than `-` or `0-9`, when `parse::<i64>` fails, and when the text is not `format_i64` of the parsed value. `format_i64` is `value.to_string()`. `i64::MIN.to_string()` is `"-9223372036854775808"`. That text has length 20. It is not empty, it is not longer than 20, every byte is `-` or a digit, `parse::<i64>` yields `i64::MIN`, and `format_i64` of that value is the same text, so the function returns `Some(i64::MIN)`. `9223372036854775808` still fails `parse::<i64>`. `00` and `-0` still fail because `format_i64(0)` is `"0"`.

## Length window

`activation` rejects a UTF-16 length outside `1..31` before it looks at the parse. `edit_len` counts `encode_utf16`. The MIN text is ASCII, so that count is 20. Twenty code units sit inside `1..31`. The length check does not reject it. Sash has not been launched.

## Checked add

The fixture line is `set count = state.count + input.amount`. `lower_expr` builds `IntExpr::Add`. `eval` uses `checked_add` on the two sides. `commit` is `let next = eval(expr, state_value, input_value)?;` then `Some((next, revision.checked_add(1)?))`. `0 + i64::MIN` is `Some(i64::MIN)`. From revision 0, `commit` is `Some((i64::MIN, 1))`. `i64::MIN + -1` is `None` because `i64::MIN.checked_add(-1)` fails inside `eval`. There is no second adder. `Store` assigns count, revision, and `format_i64(next)`. It does not assign the edit. `Leave` writes nothing.

## Enter

`key_action` returns `Activate` only for `WM_KEYDOWN` (`0x0100`) plus `VK_RETURN` (`13`). Focus and ctrl are not arguments. `press_add` is `controller.on_key(WM_KEYDOWN, VK_RETURN, false)`. `on_char` takes `'\n' => press_add(controller)` and does not read focus. Emperor Enter is not focus-gated. `CounterSession::set_edit` replaces the edit string and leaves count, revision, and result unchanged. That call does not move focus.

## Wording

`docs/emperor/GY-EMP-001.md` section 6 says a canonical negative stores through the same Enter path, and that the canonical text of `i64::MIN` stores that count through the same path. The edit stays. `docs/emperor/contracts/gy-cross-parity-1.md` says a canonical `-1` stores through the same Enter path, names `-9223372036854775808` as the text that stores `i64::MIN`, and says `i64::MIN` plus `-1` does not store. The older canonical-negative sentence stays.

## Test

`tests::canonical_i64_min_stores` replaces the edit with `i64::MIN.to_string()` and sends newline. Count becomes `i64::MIN`, revision becomes 1, and the result and the edit stay `"-9223372036854775808"`. A following edit of `"-1"` and newline do not store. Focus stays Edit. Parent re-ran it: 1 passed, 28 filtered. `checked_add_overflow_does_not_store` still sets count to `i64::MIN` and edit `"-1"` without first storing the MIN text from count 0. Parent re-ran that test: 1 passed, 28 filtered. `rejected_edit_does_not_store` still rejects `9223372036854775808`. That failure is `parse::<i64>`, not `checked_add`. A unit test is not a launched EDIT, not `ToUnicode`, and not a Windows launch. EMP-M1-09 stays unmet. No ledger row moves.
