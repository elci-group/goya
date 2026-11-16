# Empty copy security

`tests::empty_edit_copy_offers_an_empty_string` admits the counter. Copy is true. Cut is `"default"`. Focus starts Edit, the edit is `"1"`, count is 0, revision is 0, and the result is `"0"`. `CounterSession::set_edit` replaces the edit and leaves count, revision, and result unchanged, so `set_edit("")` leaves count 0, revision 0, and result `"0"`. `copy_text` returns `Some("")` and does not clear or fill the edit. `on_cut` returns `Step::Unchanged` and clipboard `None`. The edit stays `""`. Count, revision, and result stay put. Button focus makes both helpers publish nothing. The edit stays `""` and the count stays 0. `capabilities.copy = false` makes `copy_text` return `None` while the edit stays `""`. This note does not change the gap ledger and does not pass any row.

Reviewed `copy_text`, `on_cut`, and the `Shortcut::Copy` and `Shortcut::Cut` arms of `on_event` in `crates/emperor-runtime/src/host.rs`, `CounterSession::set_edit` in `crates/goya-parity/src/session.rs`, and `parse_canonical_i64` in `crates/sash/src/interactive_text.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet.

## Not a store

Offering `""` is not a store and not a credential. `parse_canonical_i64` returns `None` when the text is empty. `copy_text` and `on_cut` do not call `parse_canonical_i64` and do not call `press_add`. Newline is not sent. The result stays `"0"`. The offer is the edit string, not the result.

## No second clipboard policy

`copy_text` returns `controller.session.edit`, including `""`, only when `capabilities.copy` and focus is Edit. It does not read `session.result`. Button focus returns `None`. `capabilities.copy = false` returns `None` and does not write the edit.

`on_cut` returns `Step::Unchanged` and clipboard `None` when cut is `"deny"`, focus is not Edit, or the edit is empty, and does not call `set_edit`. Cut is not gated by the copy flag. There is no selection. The named test uses cut `"default"`. It does not call `on_cut` after setting copy false. Button focus returns `Step::Unchanged` and clipboard `None`.

`Shortcut::Copy` calls `set_clipboard_text` only when `copy_text` returns `Some`, including `Some("")`, and the step stays `Unchanged`. `Shortcut::Cut` calls `on_cut` and calls `set_clipboard_text` only when that effect carries clipboard text. An empty edit does not. Neither arm filters the string. No second clipboard policy and no filter.

Sash `edit_proc` calls `clipboard_blocked` and, when the message is not blocked, `CallWindowProcW`. The system edit copies its selection. An empty selection does not replace the clipboard. Emperor is not that control. Reversing `Some("")` would be a second policy. Do not ask to change the empty copy to `None`.

## Paste stays where it is

Do not ask to gate paste on focus, to filter payloads, or to make empty allowed copy return `None`. `paste_shortcut_requests_clipboard` is still `capabilities.paste && focus == Edit`. `on_clipboard_text` still calls `set_edit` for `Some` text when paste is true and does not read focus. This slice does not change that path.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. Describing filesystem, network, subprocess, or web_content does not install Landlock or seccomp. EMP-M1-11 stays unmet. No ledger row moves. This note does not claim a signed release, sandbox enforcement, or SOTA.

## Not IME

This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. Ignoring those events is not support. EMP-M1-09 stays unmet.

## Not a pass

`copy_text` and `on_cut` do not open a window. `tests::empty_edit_copy_offers_an_empty_string` calls those helpers on an admitted controller. Parent re-ran it, `copy_text_follows_the_capability`, and `on_cut_clears_a_focused_edit_unless_blocked`: 1 passed each, 32 filtered. A unit test is not a launched clipboard, a Wayland seat, or a Win32 edit. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
