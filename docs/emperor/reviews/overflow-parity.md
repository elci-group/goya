# Overflow parity

Emperor stores only through `KeyController::activate`, which calls Sash `parse_canonical_i64` and `sash::interactive::commit`. This note compares that call with `eval` in `crates/sash/src/interactive.rs`, `activation` and `visible_commit` in `crates/sash/src/interactive_commit.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, and `on_char` plus `press_add` in `crates/emperor-runtime`.

EMP-M1 is not passed. This is not a Win32 EDIT, not a Wayland seat, and not IME. EMP-M1-09 and EMP-M1-25 stay unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/interaction-parity.md`. It does not ask to change `key_chars`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Return

Sash `key_action` activates on every `WM_KEYDOWN` (`0x0100`) plus `VK_RETURN` (`13`). The arm is `VK_RETURN => KeyAction::Activate`. There is no focus check. `press_add` is `controller.on_key(WM_KEYDOWN, VK_RETURN, false)`. `on_char` takes `'\n' => press_add(controller)` and does not add. The Linux shell does not reimplement addition. There is no second adder. Tab only moves `Focus`. Enter on the button still activates the current edit.

## Checked add

The fixture line is `set count = state.count + input.amount`. `lower_expr` builds `IntExpr::Add`. `eval` uses `checked_add` on the two sides. `commit` is `let next = eval(expr, state_value, input_value)?;` then `Some((next, revision.checked_add(1)?))`. `commit_matches_the_canonical_d1_executor` asserts `commit(&app.expr, 1, 0, 2) == Some((3, 1))`, `commit(&app.expr, i64::MAX, 4, 1) == None`, and `commit(&app.expr, 5, u64::MAX, 1) == None`. `commit(expr, i64::MIN, _, -1)` is `None` because `i64::MIN.checked_add(-1)` fails inside `eval`. That `i64::MIN` call is not in the fixture test. `commit(expr, _, u64::MAX, _)` is `None` because the revision cannot increase.

`activation` maps that `None` with `None => Activation::Rejected`. `publish_after_text` maps `Rejected` to `Publish::Hold`. `visible_commit` maps `Hold` to `VisibleCommit::Leave`. `CounterSession::activate` leaves that arm empty: `VisibleCommit::Leave => {}`. Count, revision, result, and the edit stay. The edit is left as it is.

## Canonical minus

`parse_canonical_i64("-1")` is `Some(-1)`. The bytes match `b'-' | b'0'..=b'9'`, `parse::<i64>()` yields `-1`, and `format_i64` is `value.to_string()`, which equals `"-1"`. `non_canonical_integers_do_not_commit` already has `parse_canonical_i64("-3") == Some(-3)`, `parse_canonical_i64("00") == None`, and `parse_canonical_i64("-0") == None`. `"-0"` and `"00"` stay `None` because `format_i64(0)` is `"0"`. Canonical negative is not a waiver of the format check.

## Max is not a reject

`i64::MAX` with edit `"0"` is `checked_add` of zero, so the sum is still `i64::MAX`. From revision 0, `commit` is `Some((i64::MAX, 1))` and `format_i64` is `"9223372036854775807"`. Storing the same count at revision 1 proves overflow is checked addition, not a rejection of the max count. `i64::MAX` with edit `"1"` does not store, including after Tab to the button: `key_action` does not read focus, and newline still calls `press_add`. `i64::MIN` with edit `"-1"` does not store. Revision `u64::MAX` with edit `"1"` does not store.

After restoring count 0, revision 0, and edit `"1"`, Enter stores count 1, revision 1, result `"1"`. Backspace, `-`, and `1` make `"-1"` through `set_edit` and do not change the count. Enter stores count 0, revision 2, result `"0"`. The `Store` arm assigns count, revision, and `format_i64(next)`. It does not assign the edit, so the edit stays `"-1"`.

## What the host does not drive

Busy, release, and mismatched readback remain in the session. `busy_activation_leaves_the_count` passes `busy: true` and gets `Activation::Busy` and `Leave`. `release_during_publish_leaves_the_count` passes `released: true` and gets `Leave` after `Publish`. `mismatched_readback_reverts_without_storing` gets `VisibleCommit::Revert` and restores the previous result text. The host always calls `activate(false, true, false, visible)`. `KeyController::activate` builds `visible` with `parse_canonical_i64(&self.session.edit).and_then(|input| sash::interactive::commit(...).map(|(next, _)| next))`, then `self.session.activate(false, true, false, visible)`. Matching `visible` to the commit is the in-process readback. It is not a launched text write. Generated `finish_activation` writes with `SetWindowTextW` and reads back with `GetWindowTextW`. Emperor does not call those.

## Wording

`docs/emperor/GY-EMP-001.md` section 6 says checked addition overflow and a revision that cannot increase do not store, the edit stays, and a canonical negative stores through the same Enter path. `docs/emperor/contracts/gy-cross-parity-1.md` says `i64::MAX` plus `1`, `i64::MIN` plus `-1`, and a revision of `u64::MAX` do not store, and that `i64::MAX` plus `0` stores the same count. The code path is `commit`.

## Test

`tests::checked_add_overflow_does_not_store` drives those cases through `on_char` and `on_tab`. `i64::MAX` with edit `"1"` stays unchanged, including after Tab to the button. `i64::MIN` with `"-1"` stays unchanged. Revision `u64::MAX` with `"1"` stays unchanged. `i64::MAX` with `"0"` stores that count at revision 1 and result `"9223372036854775807"`. A later Enter on `"1"` stores count 1. Typed `"-1"` then stores count 0, revision 2, result `"0"`, and leaves the edit `"-1"`. `rejected_edit_does_not_store` still rejects the non-canonical decimal `9223372036854775808`. That failure is `parse::<i64>`, not `checked_add`.

`ignore_ime` returns `Step::Unchanged`. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.
