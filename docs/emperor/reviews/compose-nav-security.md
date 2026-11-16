# Compose navigation security

`handle_key_press` resets compose on an opted-in browser-navigation chord, then pushes `PlatformEvent::BrowserNavigation`. The shortcut arm and the `key_special` arm use the same reset. This note does not change the gap ledger and does not pass any row.

Reviewed `handle_key_press`, `shortcut_from_key`, `key_special`, `set_browser_navigation_keys`, and `GoyaWindowInner::new` in `crates/goya-wayland/src/lib.rs`, and `run_window`, `on_event`, and `ignore_ime` in `crates/emperor-runtime/src/host.rs`, against the sandbox paragraph in `docs/emperor/threat-model.md`. `shortcut_tests::browser_navigation_cancels_a_partial_compose` covers that reset. `browser_navigation_compatibility::navigation_is_opt_in_and_preserves_session_shortcuts` presses the chords and does not read compose.

## The return

When `browser_navigation_keys` is true, `handle_key_press` matches evdev 105 with alt and not ctrl as `BrowserNavigation::Back`, 106 with alt and not ctrl as `Forward`, 19 with ctrl and not alt as `Reload`, and 63 with neither ctrl nor alt as `Reload`. A match calls `compose.reset()` when a compose state exists, pushes `PlatformEvent::BrowserNavigation`, and returns. On `shortcut_from_key` returning `Some`, the shortcut arm resets the same way, pushes `PlatformEvent::Shortcut`, and returns. The `key_special` arm resets, then pushes `KeySpecial` or `KeySpecialWithModifiers`.

`shortcut_from_key` maps ctrl plus evdev 106 to `Shortcut::SessionNext` and ctrl plus 105 to `Shortcut::SessionPrevious`. The navigation guards for 105 and 106 require alt and not ctrl, so Ctrl+Right stays `SessionNext` with the flag set or clear. `navigation_is_opt_in_and_preserves_session_shortcuts` expects that shortcut for 106 with ctrl, then `Back`, `Forward`, and `Reload` once alt or ctrl matches. It does not press F5.

## What the reset does not do

The reset does not perform navigation. It does not change the Emperor edit, count, or clipboard. `BrowserNavigation` stays ignored by the counter. `on_event` matches `PlatformEvent::BrowserNavigation(_)` in the catch-all and returns `Step::Unchanged`. `run_window` does not mark the frame dirty and does not touch the controller on `Step::Unchanged`. That arm does not call `on_char`, `copy_text`, `on_cut`, or `on_clipboard_text`. The counter does not go back, forward, or reload.

Opt-in stays off unless `set_browser_navigation_keys(true)` was called. `GoyaWindowInner::new` sets `browser_navigation_keys` to false. `set_browser_navigation_keys` writes that flag and nothing else. `run_window` calls `GoyaWindow::create` and does not call `set_browser_navigation_keys`. The Emperor window does not call it. With the flag false, Alt+Left and Alt+Right are not `BrowserNavigation`. `shortcut_from_key` runs first, and `key_special` runs when it returns `None`. Both arms already call `compose.reset()`.

## Not a sandbox

This is not a sandbox, not a site isolation boundary, and not a claim that the browser honoured Back or Reload. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. Cancelling XKB compose is not `zwp_text_input_v3` support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-11 stays unmet. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.

## Not a seat

A unit test is not a Wayland seat. `browser_navigation_compatibility` and `shortcut_tests` call `handle_key_press` on a `GoyaWindowInner` with no compositor. `shortcut_tests::browser_navigation_cancels_a_partial_compose` calls `handle_key_press` on that inner and does not open a seat. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.
