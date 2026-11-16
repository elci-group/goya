# Compose security

The shortcut arm of `handle_key_press` resets compose, then pushes `PlatformEvent::Shortcut` and returns. The `key_special` arm uses the same reset. This note does not change the gap ledger and does not pass any row.

Reviewed `handle_key_press`, `shortcut_from_key`, `key_chars`, `key_chars_with_compose`, `key_char_from_state`, and the `wl_keyboard` `Leave` arm in `crates/goya-wayland/src/lib.rs`, and `on_event`, `copy_text`, `on_cut`, `on_clipboard_text`, and `ignore_ime` in `crates/emperor-runtime/src/host.rs`, against the sandbox paragraph in `docs/emperor/threat-model.md`. `shortcut_tests::shortcut_cancels_a_partial_compose` covers the shortcut reset.

## Shortcut reset

`handle_key_press` calls `shortcut_from_key(self.ctrl, self.shift, key)`. On `Some` it calls `compose.reset()` when a compose state exists, pushes `PlatformEvent::Shortcut(shortcut)`, and returns. The `key_special` arm resets the same way, then pushes `KeySpecial` or `KeySpecialWithModifiers`. Browser navigation still returns before either arm and does not reset compose.

`shortcut_from_key(true, false, 46)` is `Shortcut::Copy`. Evdev 46 is Ctrl+C when ctrl is set and shift is clear. A partial dead acute is evdev 40 on `us` `intl`. `locale_compose_completes_dead_acute_sequence` feeds 40, gets no character, then feeds evdev 18 and gets `'é'`. `shortcut_cancels_a_partial_compose` presses 40, then 46, then 18: Copy, compose status `Nothing`, then `KeyChar('e')`. `shortcut_from_key`, `key_chars`, and the Emperor host are not this slice.

## What that reset does not do

The reset does not drop the shortcut and does not change the edit, the count, or the clipboard by itself. Copy still depends on the Emperor host gates. `on_event` handles `Shortcut::Copy` by calling `copy_text` and publishing only when that returns text. `copy_text` returns the edit only when `capabilities.copy` is true and focus is `Focus::Edit`. That arm returns `Step::Unchanged`. Cut still uses `on_cut`. Paste still uses the paste request and `on_clipboard_text`.

This is not a filter on the next character beyond the existing `key_chars` rules. It does not sanitize paste. After status `Nothing`, `key_chars_with_compose` still calls `key_chars`. `key_char_from_state` still returns nothing while ctrl is held. `on_clipboard_text` still calls `set_edit` for `Some` text when paste is true.

Keymap-free `key_chars` still ignores ctrl. With no xkb state it calls `key_char` and does not read ctrl. Do not ask to change it.

## Not text input

Cancelling XKB compose is not `zwp_text_input_v3` support. Preedit and commit stay ignored by the Emperor host. `on_event` sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.

`Leave` already calls `compose.reset()` when a compose state exists, then `clear_keyboard_on_leave`. That is not the shortcut arm.

## Browser navigation

Browser navigation is unchanged and still does not reset compose. When `browser_navigation_keys` is true, a match pushes `PlatformEvent::BrowserNavigation` and returns. The chords are Alt+Left (evdev 105), Alt+Right (106), Ctrl+R (19), and F5 (63). Do not call that a sandbox or a pass.

## No sandbox

No sandbox is installed. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. EMP-M1-11 stays unmet.

## Not a seat

A unit test is not a Wayland seat and not a Win32 dead-key call. `shortcut_tests` calls `handle_key_press` on a `GoyaWindowInner` with no compositor. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.
