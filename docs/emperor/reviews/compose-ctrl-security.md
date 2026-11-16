# Compose ctrl security

The `Composed` arm of `key_chars_with_compose` reads `ctrl` after `compose.reset()` and returns no characters for any evdev keycode other than 14, 28, and 96. The ctrl check is present. This note does not change the gap ledger and does not pass any row.

Reviewed `key_chars_with_compose`, `key_chars`, `key_char`, `key_char_from_state`, `handle_key_press`, and `shortcut_from_key` in `crates/goya-wayland/src/lib.rs`, `on_event`, `on_char`, `on_clipboard_text`, and `ignore_ime` in `crates/emperor-runtime/src/host.rs`, and `press_add` in `crates/emperor-runtime/src/lib.rs`, against the sandbox paragraph in `docs/emperor/threat-model.md`. `shortcut_tests::ctrl_suppresses_a_composed_character` covers the suppressed completion.

## Composed

`key_chars_with_compose` feeds `key_get_one_sym`, not UTF-32. On `Status::Composed` it collects `compose.utf8()` before `compose.reset()`. Evdev 14, 28, and 96 then return `key_chars`. Any other keycode returns an empty vec when `ctrl` is true, and those collected characters otherwise. `Cancelled`, `Nothing`, a missing compose state, and `FeedResult::Ignored` still call `key_chars`. `Composing` still returns an empty vec. `key_chars` still returns `'\u{8}'` for 14 and `'\n'` for 28 and 96 before `key_char_from_state`. That helper returns `None` when `ctrl` is true.

Dead acute (evdev 40, `us` `intl`) plus `e` (18) stays `'é'`. The same acute plus Ctrl+E emits nothing and leaves compose `Nothing`. The next plain `e` is `'e'`. `shortcut_from_key` does not name evdev 18, so `handle_key_press` does not take the shortcut return. `shortcut_tests::ctrl_suppresses_a_composed_character` presses that sequence through `handle_key_press` and expects no events, status `Nothing`, then `KeyChar('e')`.

## What suppression does not do

Suppressing the composed character does not change the edit by itself. No `KeyChar` means Emperor `on_char` is not called. It does not sanitize paste or strip the edit. `handle_key_press` pushes `PlatformEvent::KeyChar` only for characters the vec still holds. `on_event` calls `on_char` only for `KeyChar`. `on_clipboard_text` still calls `set_edit` for `Some` text when paste is true. The dropped `utf8` vec is not written into the edit.

`handle_key_press` already resets compose when `shortcut_from_key` matches, then pushes `PlatformEvent::Shortcut` and returns. Browser navigation still matches first, pushes `PlatformEvent::BrowserNavigation`, and returns without that reset. This note does not ask to change that. Keymap-free `key_chars` still ignores ctrl: with no xkb state, `key_chars_with_compose` returns `key_chars`, and `key_char` takes keycode and shift only. This note does not ask to change that path or to special-case Ctrl+Space.

## Newline

Ctrl+Enter still activates through the existing newline path. Do not treat that as a bypass to close. For evdev 28 and 96 the `Composed` arm returns `key_chars`, not `compose.utf8()`. `key_chars` returns `'\n'` before the ctrl check. `on_char` takes `'\n'` and calls `press_add`. `press_add` calls `on_key` with `WM_KEYDOWN` and `VK_RETURN`. Ctrl is not an argument. Ctrl+Backspace stays `'\u{8}'` by the same `key_chars` return. The cited test expects `KeyChar('\n')` for evdev 28 and `KeyChar('\u{8}')` for evdev 14 while ctrl is held, after compose is already `Nothing`. It does not press evdev 96. Evdev 96 is the same return.

## Not text input and not a sandbox

Dropping one composed `KeyChar` is not `zwp_text_input_v3` support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. This is not a sandbox and not Landlock or seccomp. EMP-M1-11 stays unmet. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.

No sandbox is installed. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. `open_admitted` rejects other capability strings and does not install a filter.

## Not a seat

A unit test is not a Wayland seat and not a Win32 `ToUnicode` call. `shortcut_tests::ctrl_suppresses_a_composed_character` calls `handle_key_press` on a `GoyaWindowInner` with no compositor. It returns without asserting when the `us` `intl` keymap or the `C.UTF-8` compose table is missing. That skip is not a pass. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.
