# Compose ctrl parity

A completed XKB compose emits no character while ctrl is held, except backspace and enter. This note compares `key_chars_with_compose` in `crates/goya-wayland/src/lib.rs` with `on_char` in `crates/emperor-runtime/src/host.rs` and with `key_action` in `crates/sash/src/interactive_keys.rs`.

EMP-M1 is not passed. This is not a launched Sash EDIT and not `ToUnicode`. This is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. This note does not rewrite `docs/emperor/reviews/compose-parity.md`, `text-parity.md`, or `interaction-parity.md`. It does not ask to change `shortcut_from_key` or the keymap-free path. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## The xkb ctrl rule

`key_char_from_state` returns nothing when ctrl is held: `if ctrl { return None; }`. `key_chars` still returns evdev 14 as `'\u{8}'` and evdev 28 and 96 as `'\n'` before that call. `key_chars_with_compose` already passes `ctrl` into `key_chars` when compose is missing, when `feed` is `Ignored`, and on `Cancelled` and `Nothing`.

## Composed

On `Status::Composed`, `key_chars_with_compose` collects `compose.utf8()`, calls `compose.reset()`, and then reads `ctrl`. Evdev 14, 28, and 96 return `key_chars`. Any other keycode returns an empty vec when `ctrl` is true, and the composed characters otherwise. `shortcut_from_key` does not name evdev 18, so `handle_key_press` does not take the shortcut return. `locale_compose_completes_dead_acute_sequence` still completes acute plus `e` without ctrl as `vec!['é']`.

Without ctrl, acute plus `e` stays `'é'`. Acute plus Ctrl+E emits nothing and leaves compose `Nothing`. The next `e` is `'e'`. The suppression is the existing xkb ctrl rule applied to a completed compose, not a second character filter. `shortcut_tests::ctrl_suppresses_a_composed_character` drives that sequence. `shortcut_tests::shortcut_cancels_a_partial_compose` is the other reset: dead acute, then Ctrl+C, then `KeyChar('e')`.

## Ctrl+Tab, Ctrl+Enter, and Space

Ctrl+Tab stays focus movement. Evdev 15 is `KeySpecial::Tab` in `key_special`. `handle_key_press` emits `KeySpecialWithModifiers` before characters. `on_modified_special` calls `on_tab`, which calls `on_key` with `VK_TAB`. `key_action` returns `FocusNext` for `VK_TAB` (`9`) on `WM_KEYDOWN` (`0x0100`). Ctrl is not an argument. `shortcut_tests::ctrl_does_not_swallow_enter_or_tab` still expects that modified Tab.

Ctrl+Enter stays activate. `key_chars` returns `'\n'` for evdev 28 and 96 before the ctrl check. `on_char` takes `'\n' => press_add(controller)`. `press_add` sends `WM_KEYDOWN` and `VK_RETURN` (`13`). `key_action` returns `Activate` for that pair. Ctrl is not an argument.

Space still does not insert. `on_char` for `' '` calls `on_key` with `WM_KEYDOWN` and `VK_SPACE` and does not call `set_edit`. `key_action` returns `Dispatch` for every other wparam, including Space (`0x20`).

## Host and Sash

Emperor `on_char` is unchanged. A `KeyChar` that does not arrive is not appended. `on_event` calls `on_char` only for `PlatformEvent::KeyChar`. The catch-all pushes a character only while focus is the edit and the character is not a control. Button focus does not append. `ignore_ime` returns `Step::Unchanged`.

`key_action` does not know dead keys or ctrl. This is not a launched Sash EDIT and not `ToUnicode`.

## Not a pass

A unit test of compose is not a Wayland seat. EMP-M1-09 stays unmet. No ledger row moves. With no xkb state, `key_chars_with_compose` returns `key_chars`, and `key_char` does not read ctrl. Do not ask to change that path or `shortcut_from_key`.
