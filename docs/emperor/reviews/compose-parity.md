# Compose parity

A shortcut resets a partial XKB compose sequence and still delivers the shortcut. This note compares `handle_key_press` in `crates/goya-wayland/src/lib.rs` with `on_char` and `copy_text` in `crates/emperor-runtime/src/host.rs` and with `key_action` in `crates/sash/src/interactive_keys.rs`.

EMP-M1 is not passed. This is not a launched Sash EDIT and not `ToUnicode`. This is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/interaction-parity.md` or `text-parity.md`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Dead acute

`locale_compose_completes_dead_acute_sequence` already completes dead acute (evdev 40) plus `e` (evdev 18) as `'é'` through `key_chars_with_compose`. The acute call is empty. The `e` call is `vec!['é']`. That test does not press a shortcut. `compose_without_keymap_preserves_keyboard_fallback` passes no compose state. A missing xkb state returns `key_chars` and still yields `'q'` and `'\n'`.

## Resets that exist

`handle_key_press` resets compose when `key_special` matches, then emits `KeySpecial` or `KeySpecialWithModifiers`. The `wl_keyboard` `Leave` arm resets compose, then calls `clear_keyboard_on_leave`. `key_chars_with_compose` resets on `Composed` and on `Cancelled`. `Composing` emits nothing and leaves the sequence armed.

## Shortcut

`shortcut_from_key` runs first. Ctrl without shift on evdev 46 is `Shortcut::Copy`. The arm calls `compose.reset()` when a compose state exists, pushes `PlatformEvent::Shortcut`, and returns. `shortcut_tests::shortcut_cancels_a_partial_compose` presses dead acute, then that Copy chord, then `e`.

After the shortcut, the next `e` is `'e'`, not the composed `'é'`. The shortcut event is still delivered. Compose status is `Nothing`.

## Enter, Tab, Space, and Backspace

Enter, Tab, Space, and Backspace are unchanged. Do not ask to special-case Ctrl+Space. `key_chars` is unchanged: evdev 28 and 96 stay newline before the ctrl check, and evdev 14 stays `'\u{8}'` the same way. On the xkb path, ctrl still suppresses `KeyChar` after those returns. Tab is evdev 15 in `key_special` and keeps that older reset. Space is evdev 57. `shortcut_from_key` does not name it. The keymap-free path still has no compose state and still emits letters for unrecognized ctrl combos, because `key_char` does not read ctrl.

## Host

Emperor `on_char` is unchanged. A later `KeyChar('e')` appends only while the edit is focused and the character is not a control. The catch-all pushes that character and calls `set_edit`. Button focus does not append. `Shortcut::Copy` is still the whole edit only when copy is allowed and focus is the edit: `copy_text` returns `session.edit` only then. There is no caret. `ignore_ime` returns `Step::Unchanged`.

## Sash

`key_action` does not know dead keys. `WM_KEYDOWN` plus `VK_RETURN` (`13`) is `Activate`. `VK_TAB` (`9`) moves focus. Every other wparam, including Space (`0x20`), is `Dispatch`. Ctrl is not an argument. This is not a launched Sash EDIT and not `ToUnicode`.

## Not a pass

A unit test of compose is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/interaction-parity.md` or `text-parity.md`.
