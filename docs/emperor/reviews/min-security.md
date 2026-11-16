# Min security

`tests::canonical_i64_min_stores` admits the interactive counter and, while focus is Edit, `set_edit`s the canonical text of `i64::MIN` (`"-9223372036854775808"`, 20 bytes, `i64::MIN.to_string()`). Newline through `host::on_char` calls the existing `press_add` path. Count becomes `i64::MIN`, revision becomes 1, result becomes that text, and the edit stays that text. Then `set_edit("-1")` and newline do not store: count stays `i64::MIN`, revision stays 1, result stays the MIN text, and the edit stays `"-1"`. Storing canonical MIN is not an overflow bypass. The following `-1` still does not store. Parent re-ran the test: 1 passed, 28 filtered. This note does not change the gap ledger and does not pass any row.

Reviewed `eval` and `commit` in `crates/sash/src/interactive.rs`, `activation`, `publish_after_text`, and `visible_commit` in `crates/sash/src/interactive_commit.rs`, `edit_len` and `CounterSession::activate` in `crates/goya-parity/src/session.rs`, `KeyController::on_key` and `KeyController::activate` in `crates/goya-parity/src/keys.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, `parse_canonical_i64` and `format_i64` in `crates/sash/src/interactive_text.rs`, `press_add` in `crates/emperor-runtime/src/lib.rs`, and the newline arm of `on_char` in `crates/emperor-runtime/src/host.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet.

## Not an overflow bypass

Storing the canonical MIN text is `checked_add` of the admitted count `0` and `i64::MIN`. That sum is `Some(i64::MIN)`. From revision 0, `revision.checked_add(1)` is `Some(1)`. It is not a wrap, not a saturation, and not a branch that skips the check.

`parse_canonical_i64` returns `None` when the text is empty, when `text.len() > 20`, when any byte is other than `-` or `0-9`, when `parse::<i64>` fails, or when `format_i64(value) != text`. `format_i64` is `value.to_string()`. The MIN text is 20 bytes, so the length check does not reject it, and the function returns `Some(i64::MIN)`. `edit_len` counts UTF-16 units. The text is ASCII, so the count is 20. `activation` rejects a length outside `1..31` before it looks at the parse. Twenty is inside that window. No parser change.

`KeyController::activate` builds `visible` with `parse_canonical_i64(&self.session.edit).and_then(|input| sash::interactive::commit(...).map(|(next, _)| next))`, then calls `self.session.activate(false, true, false, visible)`. `CounterSession::activate` parses and commits again, then calls `activation`, `publish_after_text`, and `visible_commit`. The commit is `Some((i64::MIN, 1))`, so the step is `Activation::Publish`. Text was written, the release flag is false, and `visible` matches `next`, so the outcome is `VisibleCommit::Store`. `Store` writes count, revision, and `format_i64(next)` into the result. It does not assign `edit`. The edit stays the MIN text. The result is that same text.

The following `"-1"` is the overflow refusal. `parse_canonical_i64("-1")` is `Some(-1)`. `i64::MIN.checked_add(-1)` is `None`, so `commit` returns `None`. A `None` commit is `Activation::Rejected`. `publish_after_text` maps that to `Publish::Hold`. `visible_commit` maps `Hold` to `VisibleCommit::Leave`. The `Leave` arm is empty. Count stays `i64::MIN`, revision stays 1, and result stays the MIN text. The edit stays `"-1"` because `set_edit` replaced it and `Leave` does not write it back. That is not a wrap to `i64::MAX`.

`tests::checked_add_overflow_does_not_store` assigns `count = i64::MIN` and newlines on `"-1"`. Count stays `i64::MIN`, revision stays 0, and result stays `"0"`. That refusal never stored the MIN text from count 0. It is not this path.

## No second adder

The host does not call `checked_add`. `press_add` is `controller.on_key(WM_KEYDOWN, VK_RETURN, false)`. The newline arm of `on_char` is `'\n' => press_add(controller)`. It has no `i64::MIN` branch. Do not add one. A local min-value check would be a second adder. Do not recommend one. Do not change `parse_canonical_i64`. `presented.saturating_add(1)` and `pumps.saturating_add(1)` count frames in the window loop. They are not the admitted add.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. Describing filesystem, network, subprocess, or web_content does not install Landlock or seccomp. EMP-M1-11 stays unmet. No ledger row moves. EMP-M1 is not certified. This note does not claim a signed release, sandbox enforcement, or a state-of-the-art result.

## Enter stays focus-blind

Enter stays focus-blind. `key_action` returns `KeyAction::Activate` for `VK_RETURN` on `WM_KEYDOWN` and does not read focus. The newline arm calls `press_add` with no focus test. Do not gate Enter on focus. Do not change `on_char`. Do not filter the edit. `set_edit` replaces the edit string and leaves count, revision, and result unchanged. It does not move focus. The specified path uses that call, then newline. A host filter on the characters would be a second parser.

## Not the other holds

Busy, release-during-write, and mismatched readback stay session decisions. The host does not simulate them. `KeyController::activate` always passes busy false, text written true, and released false. Do not ask the host to fake them. Those three stay inside `activation`, `publish_after_text`, and `visible_commit`.

## Not IME

This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. Ignoring those events is not support. EMP-M1-09 stays unmet.

## Not a pass

`tests::canonical_i64_min_stores` calls `on_char` on an admitted controller. It does not open a Wayland seat or a Win32 edit. A unit test of this path is not a Wayland seat or a Win32 edit. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
