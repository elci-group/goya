# Raw edit security

`tests::noncanonical_edit_is_the_clipboard_text` admits the counter. It starts at count 0, revision 0, result `"0"`, and focus Edit, with cut `default` and copy allowed. `CounterSession::set_edit` replaces the edit and leaves count, revision, and result unchanged, so `set_edit("00")` leaves count 0, revision 0, and result `"0"`. `copy_text` then returns `Some("00")` and does not clear the edit. `on_cut` with cut `default` returns `Step::Changed` and clipboard `Some("00")`, then the edit is `""` and the count, revision, and result stay put. The same holds for `"-0"`: the clipboard text is `"-0"`, not `"0"`. Button focus returns `None` from `copy_text` and `on_cut` does not publish. `capabilities.copy = false` makes `copy_text` return `None` while the edit stays `"00"`. Parent re-ran the test: 1 passed, 29 filtered. This note does not change the gap ledger and does not pass any row.

Reviewed `copy_text`, `on_cut`, and the `Shortcut::Copy` and `Shortcut::Cut` arms of `on_event` in `crates/emperor-runtime/src/host.rs`, `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`, and `parse_canonical_i64` in `crates/sash/src/interactive_text.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet.

## Not a parser bypass

Offering `"00"` is not a parser bypass and not a store. `parse_canonical_i64` returns `None` when the text is empty, when `text.len() > 20`, when any byte is other than `-` or `0-9`, when `parse::<i64>` fails, or when `format_i64(value) != text`. `format_i64` is `value.to_string()`. `parse::<i64>()` of `"00"` is `0`, and `format_i64(0)` is `"0"`, so `parse_canonical_i64("00")` stays `None`. `"-0"` fails that same format check. `copy_text` and `on_cut` do not call `parse_canonical_i64` and do not call `press_add`. Newline is not sent. The result stays `"0"`. The offer is the edit string, not the result.

## No second clipboard policy

`copy_text` returns `controller.session.edit` only when `capabilities.copy` and focus is Edit. It does not read `session.result`. Button focus returns `None`. `capabilities.copy = false` returns `None` and does not write the edit. Empty allowed copy still returns `Some("")`. This slice does not reverse that.

`on_cut` clones the edit, then `set_edit("")`, unless cut is `"deny"`, focus is not Edit, or the edit is empty. Cut is not gated by the copy flag. There is no selection. Button focus returns `Step::Unchanged` and clipboard `None`, and does not call `set_edit`. The published string is that clone, so `"-0"` stays `"-0"`.

`Shortcut::Copy` calls `set_clipboard_text` only when `copy_text` returns `Some`, and the step stays `Unchanged`. `Shortcut::Cut` calls `on_cut` and calls `set_clipboard_text` only when that effect carries clipboard text. Neither arm filters the string. No second clipboard policy and no filter of the edit. A host rewrite of `"00"` or `"-0"` into `"0"` would be a second parser. Do not add one.

## Paste stays where it is

Do not ask to gate paste on focus, to filter payloads, or to make empty allowed copy return `None`. `paste_shortcut_requests_clipboard` is still `capabilities.paste && focus == Edit`. `on_clipboard_text` still calls `set_edit` for `Some` text when paste is true and does not read focus. This slice does not change that path.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. Describing filesystem, network, subprocess, or web_content does not install Landlock or seccomp. EMP-M1-11 stays unmet. No ledger row moves. This note does not claim a signed release, sandbox enforcement, or SOTA.

## Not IME

This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. Ignoring those events is not support. EMP-M1-09 stays unmet.

## Not a pass

`copy_text` and `on_cut` do not open a window. `tests::noncanonical_edit_is_the_clipboard_text` calls those helpers on an admitted controller. Parent re-ran it, `copy_text_follows_the_capability`, and `on_cut_clears_a_focused_edit_unless_blocked`: 1 passed each, 29 filtered. A unit test is not a launched clipboard, a Wayland seat, or a Win32 edit. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
