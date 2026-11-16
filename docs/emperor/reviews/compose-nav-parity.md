# Compose navigation parity

An opted-in browser-navigation chord resets a partial XKB compose and still delivers the navigation event. This note compares that return in `crates/goya-wayland/src/lib.rs` with `on_event` in `crates/emperor-runtime/src/host.rs` and with `key_action` in `crates/sash/src/interactive_keys.rs`.

EMP-M1 is not passed. This is not a launched browser and not IME. This is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/compose-parity.md` or `compose-ctrl-parity.md`. It does not ask to change `key_chars` or `shortcut_from_key`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Resets already present

`handle_key_press` calls `compose.reset()` when `shortcut_from_key` returns a shortcut, pushes `PlatformEvent::Shortcut`, and returns. The `key_special` arm calls `compose.reset()` the same way, then emits `KeySpecial` or `KeySpecialWithModifiers`. `shortcut_tests::shortcut_cancels_a_partial_compose` presses dead acute (evdev 40), then Ctrl+C (46), then `e` (18), and expects `KeyChar('e')`. On `Status::Composed`, `key_chars_with_compose` still emits nothing while ctrl is held, except backspace and enter (evdev 14, 28, and 96).

## The navigation return

The browser-navigation return runs only when `browser_navigation_keys` is true. Alt+Left is Back: evdev 105 with alt and not ctrl. Alt+Right is Forward: evdev 106 with alt and not ctrl. Ctrl+R and F5 are Reload: evdev 19 with ctrl and not alt, and evdev 63 with neither ctrl nor alt. Alt comes from `mod_name_is_active` in the `Modifiers` arm, not a depressed-bit guess. The match calls `compose.reset()`, pushes `PlatformEvent::BrowserNavigation`, and returns.

After Back, Reload, or F5, the next `e` is `'e'`, not `'é'`, and the navigation event was still delivered. `shortcut_tests::browser_navigation_cancels_a_partial_compose` drives those three chords and then Ctrl+Right. `locale_compose_completes_dead_acute_sequence` still completes acute plus `e` with no chord in between as `vec!['é']`.

## Ctrl+Right

Ctrl+Right is still a session shortcut, not Forward. Forward requires alt and not ctrl. `shortcut_from_key` maps ctrl plus evdev 106 to `Shortcut::SessionNext` because the ctrl chord is not the alt chord. `navigation_is_opt_in_and_preserves_session_shortcuts` pops that shortcut for 106 with ctrl, both before the flag and after `browser_navigation_keys = true`. The shortcut arm already resets compose. This slice does not change `shortcut_from_key`.

## Emperor and Sash

Emperor ignores `BrowserNavigation`. `on_event` matches it with the unchanged arms and returns `Step::Unchanged`. It does not call `on_char` or `press_add`. The counter's focus, edit, and count do not change. `ignore_ime` returns `Step::Unchanged`. Emperor does not call `set_browser_navigation_keys`. That call is in `goya-browser`, which this note does not launch.

`key_action` has no browser-navigation action and no dead keys. On `WM_KEYDOWN` (`0x0100`), `VK_TAB` (`9`) moves focus and `VK_RETURN` (`13`) is `Activate`. Every other wparam is `Dispatch`. Ctrl is not an argument.

## Not a pass

A unit test of `handle_key_press` is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. This is not a launched browser and not IME. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`.
