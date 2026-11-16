# Overflow security

Checked addition refuses an overflowing sum or revision and leaves the typed edit in place. `tests::checked_add_overflow_does_not_store` covers that path. This note does not change the gap ledger and does not pass any row.

Reviewed `eval` and `commit` in `crates/sash/src/interactive.rs`, `activation`, `publish_after_text`, and `visible_commit` in `crates/sash/src/interactive_commit.rs`, `KeyController::on_key` and `KeyController::activate` in `crates/goya-parity/src/keys.rs`, `CounterSession::activate` in `crates/goya-parity/src/session.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, `parse_canonical_i64` and `format_i64` in `crates/sash/src/interactive_text.rs`, `press_add` in `crates/emperor-runtime/src/lib.rs`, and the newline arm of `on_char` in `crates/emperor-runtime/src/host.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-11 and EMP-M1-25 as Unmet.

## Not a host sanitizer

Overflow is not a host sanitizer. The edit stays `"1"` or `"-1"`. Leave does not delete text and does not saturate or wrap.

`eval` adds with `checked_add`. `commit` is `let next = eval(...)?; Some((next, revision.checked_add(1)?))`. Either `None` makes `commit` return `None`. `KeyController::activate` builds `visible` with `parse_canonical_i64(&self.session.edit).and_then(|input| sash::interactive::commit(...).map(|(next, _)| next))`, then calls `self.session.activate(false, true, false, visible)`. `CounterSession::activate` parses and commits again, then calls `activation`, `publish_after_text`, and `visible_commit`. A `None` commit is `Activation::Rejected`. `publish_after_text` maps that to `Publish::Hold`. `visible_commit` maps `Hold` to `VisibleCommit::Leave`. The `Leave` arm is empty. Count, revision, and result stay. `i64::MAX` plus canonical `"1"` does not store. `i64::MIN` plus canonical `"-1"` does not store. `revision == u64::MAX` does not store when the integers fit, because `revision.checked_add(1)` is `None` and `commit` returns `None`. The edit stays whatever canonical text was already there.

`i64::MAX` plus canonical `"0"` is not that refusal. From revision 0, `checked_add(0)` keeps the count and `checked_add(1)` stores revision 1. A typed canonical `"-1"` added to count 1 stores count 0. `Store` writes count, revision, and `format_i64(next)` into the result. It does not assign `edit`.

## The host does not add

The host does not call `checked_add`. `press_add` is `controller.on_key(WM_KEYDOWN, VK_RETURN, false)`. The newline arm of `on_char` is `'\n' => press_add(controller)`. A local max-value check would be a second adder. Do not recommend one. `presented.saturating_add(1)` and `pumps.saturating_add(1)` count frames in the window loop. They are not the admitted add.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet. No ledger row moves. EMP-M1 is not certified.

## Enter stays focus-blind

Enter stays focus-blind. `key_action` returns `KeyAction::Activate` for `VK_RETURN` on `WM_KEYDOWN` and does not read focus. The newline arm calls `press_add` with no focus test. Button focus still attempts the same commit. A failed add on the button is still Leave, not a new rule.

## Not the other holds

Busy, release-during-write, and mismatched readback are not simulated by this host. `KeyController::activate` always passes busy false, text written true, and released false. Do not ask the host to fake them. Those three stay inside `activation`, `publish_after_text`, and `visible_commit`.

## Minus one is not a new grammar

A canonical `-1` storing is not a sign to accept `-0` or `00`. Those stay non-canonical. `parse_canonical_i64` returns `None` when `format_i64(value) != text`. `format_i64(-1)` is `"-1"`. `format_i64(0)` is `"0"`, so `"-0"` and `"00"` fail. The decimal `9223372036854775808` in `rejected_edit_does_not_store` fails `parse::<i64>` before `commit`. That is not `checked_add` of two `i64` values.

## Not a pass

`tests::checked_add_overflow_does_not_store` is absent. `overflow_does_not_change_count_or_revision` sets count to `i64::MAX`, edit `"1"`, and calls `CounterSession::activate` with `visible` `None`. It expects `Activation::Rejected` and `VisibleCommit::Leave`. It does not cover `i64::MIN + -1`, `revision == u64::MAX`, `i64::MAX + 0`, a typed `"-1"`, or button focus. Sash `commit_matches_the_canonical_d1_executor` asserts `commit(&app.expr, i64::MAX, 4, 1)` and `commit(&app.expr, 5, u64::MAX, 1)` are `None`. A unit test is not a Wayland seat or a Win32 edit. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.
