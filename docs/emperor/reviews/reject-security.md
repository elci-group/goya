# Reject security

Rejected text stays in the edit. Enter does not delete it, does not replace it, and does not change count, revision, or result. `tests::rejected_edit_does_not_store` covers that path. This note does not change the gap ledger and does not pass any row.

Reviewed `on_char` and `on_clipboard_text` in `crates/emperor-runtime/src/host.rs`, `press_add` in `crates/emperor-runtime/src/lib.rs`, `KeyController::on_key` and `KeyController::activate` in `crates/goya-parity/src/keys.rs`, `CounterSession::activate` in `crates/goya-parity/src/session.rs`, `parse_canonical_i64` in `crates/sash/src/interactive_text.rs`, `key_action` in `crates/sash/src/interactive_keys.rs`, and `activation`, `publish_after_text`, and `visible_commit` in `crates/sash/src/interactive_commit.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-11 and EMP-M1-25 as Unmet. `tests::rejected_edit_does_not_store` drives empty, `00`, `1,`, `-0`, overflow, a following canonical Enter, and Enter on the button. The contract sentences name non-control characters and those rejected strings.

## The edit keeps the characters

`on_char` appends only when focus is the edit and `!ch.is_control()`: it clones the edit, `edit.push(ch)`, and calls `set_edit`. Newline is not that arm. The newline arm is `'\n' => press_add(controller)`. It does not insert and does not rewrite the edit. Backspace still drops one grapheme on the edit through `delete_last_grapheme` and does not parse. Nothing in `on_char` deletes a rejected string or writes a sanitized replacement.

`press_add` is `controller.on_key(WM_KEYDOWN, VK_RETURN, false)`. `on_key` matches `KeyAction::Activate` and calls `activate`. `activate` builds `visible` with `sash::interactive::parse_canonical_i64(&self.session.edit).and_then(|input| sash::interactive::commit(...).map(|(next, _)| next))`, then calls `self.session.activate(false, true, false, visible)`. `CounterSession::activate` calls `parse_canonical_i64` again, then `activation`, `publish_after_text`, and `visible_commit`. A failed parse is `Activation::Rejected`. `publish_after_text` maps that to `Publish::Hold`. `visible_commit` maps `Hold` to `VisibleCommit::Leave`. The `Leave` arm is empty. Count, revision, result, and the edit stay. `Store` writes count, revision, and `format_i64(next)` into the result. It does not assign `edit`. `Revert` rewrites the result from the stored count and still does not assign `edit`. Rejected text is not deleted and not replaced with a sanitized string. The edit keeps the user's characters. That is not a filter and not a sandbox.

## No parser in the host

The host does not parse integers itself. `press_add` only sends Return. `on_char` does not call `parse_canonical_i64`. The second call sits in `CounterSession::activate`, which is the shared session, not a host parser. A local parser would be a second adder. Do not add one.

## Canonical decimal is not a boundary

`parse_canonical_i64` returns `None` when `text.is_empty()` or `text.len() > 20`, when any byte fails `matches!(byte, b'-' | b'0'..=b'9')`, when `text.parse::<i64>()` fails, or when `format_i64(value) != text`. `00` and `-0` fail that format check. A comma fails the byte check. The comma is not stripped. `9223372036854775808` fails `parse::<i64>`. Empty, `00`, `-0`, a comma, and that out-of-range decimal are rejected because they are not the canonical decimal. This is not an authorization boundary. It does not install Landlock or seccomp. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. EMP-M1-11 stays unmet.

## Paste is still not filtered

`on_clipboard_text` does `controller.session.set_edit(text)` for `Some(text)` when `ir.capabilities.paste` is true, then returns `Step::Changed`. Any payload replaces the edit when paste is allowed, including controls, a comma, and a non-canonical decimal. Any other case returns `Step::Unchanged` and does not call `set_edit`. Do not strip controls from paste.

## Enter is not focus-gated

`key_action` returns `KeyAction::Activate` for `VK_RETURN` on `WM_KEYDOWN` and does not read focus. `on_char` calls `press_add` for newline with no focus test. Button focus still activates the current edit. That is the existing Sash rule. It is not a bug.

## No caret and no grab

There is no caret and no selection. A character is pushed at the end of the whole edit. `delete_last_grapheme` removes the last grapheme of the whole edit. There is no word delete. `on_modified_special` calls `on_cut` only for `KeySpecial::Delete` when shift is set and ctrl is clear. Other Delete chords return unchanged. `PointerArm` records a button id. `on_pointer_press` stores it and does not activate. `on_pointer_release` takes it and calls `press_add` only when the release hit is that id. That record is not a pointer grab. EMP-M1-25 stays unmet.

## Not a pass

EMP-M1 is not certified. No ledger row moves. A unit test of this path is not a Wayland seat and not a Win32 edit. This note does not claim either. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.
