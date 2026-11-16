# Clipboard security

The working tree now has the edit-focus copy gate and `Shortcut::Cut` for evdev 45 with ctrl and without shift. This note was written against `17721b3` plus that plan. The gates below are the ones the code keeps.

Reviewed the clipboard sources at `17721b3` and the planned Ctrl+X change. The files were `crates/sash/src/interactive_clipboard.rs`, `crates/emperor-runtime/src/host.rs`, `crates/goya-wayland/src/lib.rs`, `docs/emperor/threat-model.md`, and `docs/emperor/reviews/interaction-security.md`. This note does not change the gap ledger and does not pass any row. EMP-M1-11 stays unmet.

## Current source

`Shortcut` is Copy, Paste, Quit, SessionNext, and SessionPrevious. `shortcut_from_key` maps a key only while Control is held. Evdev 46 is Copy, 47 is Paste, 16 is Quit, and 105 and 106 are the session chords. Evdev 45, KEY_X, is in the inert set. The helper does not take Shift, so an arm that only matches 45 would also fire for Ctrl+Shift+X. `handle_key_press` returns as soon as that helper yields a shortcut. While Control is held and the helper yields nothing, the key produces no character.

`copy_text` returns the whole `session.edit` when `capabilities.copy` is true and returns `None` otherwise. It does not read focus. `on_event` calls `set_clipboard_text` for `Shortcut::Copy` only when that function returns text. The planned change narrows the return: the whole edit only when copy is true and focus is Edit. Button focus returns `None`.

`Shortcut::Paste` calls `request_clipboard_text` only when `capabilities.paste` is true. `on_clipboard_text` calls `set_edit` only for `Some(text)` when paste is true. A denied paste does not call `set_edit` for a payload, for an empty string, or for `None`.

`cut_edit` is the only cut. It returns unchanged and publishes nothing, and it does not call `set_edit`, when cut is `deny`, when focus is not Edit, or when the edit is empty. `allow` and `default` on a non-empty focused edit publish the whole edit and clear it with `set_edit("")`. The only caller is Shift+Delete with Control up. Ctrl+Shift+Delete is not that chord. There is no selection. The published string is the whole edit buffer.

`open_admitted` rejects a filesystem value other than `none` and rejects `network`, `subprocess`, or `web_content` when true. That check does not call Landlock, seccomp, a namespace, or a privilege drop. `crates/sash/src/interactive_clipboard.rs` classifies Win32 messages and does not read a clipboard. The Emperor host does not call `clipboard_blocked`. A Win32 Ctrl+X classification is not this Wayland path.

## What must stay absent

A denied paste must not call `set_edit`. Copy must not publish when `capabilities.copy` is false. Copy must not publish when focus is not Edit, including Button focus. A deny cut must not publish and must not clear the edit. Describing `filesystem`, `network`, `subprocess`, or `web_content` does not install a sandbox. The plan still has no Landlock or seccomp. EMP-M1-11 stays unmet.

## Ctrl+X

`Shortcut::Cut` may be added for evdev 45 with Control and without Shift. Emperor must handle that shortcut with the existing `cut_edit` rules. Deny, focus that is not Edit, or an empty edit publishes nothing and does not call `set_edit`. `allow` and `default` on a non-empty focused edit publish the whole edit and clear it. The new path must not bypass cut deny. It must not cut from the button. It must not treat Ctrl+Shift+X as cut. That chord stays inert. It must not be described as a selection cut. There is still no selection.

The publish, when one is allowed, is still `GoyaWindow::set_clipboard_text`: an in-memory `text/plain` offer. Missing a data device makes that call a no-op. A unit test of the helper is not that offer leaving the process.

## Review result

This review covers the current source plus the plan above. It does not mark a ledger row passed. A passing unit test is not a launched clipboard or EMP-M1-11.
