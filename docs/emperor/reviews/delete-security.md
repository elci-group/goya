# Delete security

The working tree already cuts on Shift+Delete, and on Ctrl+X without shift. This note records the tests that lock the other Delete chords as non-cuts. The tests were the slice. They do not add a clipboard policy. This note does not change the gap ledger and does not pass any row. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.

Reviewed `key_operation` in `crates/sash/src/interactive_clipboard.rs`, `on_modified_special` and `on_cut` in `crates/emperor-runtime/src/host.rs`, and `shortcut_from_key`, `key_special`, and `handle_key_press` in `crates/goya-wayland/src/lib.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-11 as Unmet.

## Cut that stays

Shift+Delete, shift held and ctrl up, calls `on_cut`. Cut `"deny"`, focus other than Edit, or an empty edit publishes nothing and does not call `set_edit`. Cut `"allow"` and `"default"` publish the whole edit and clear it. That publish is not a selection. Button focus does not cut and does not call `set_edit`. Ctrl+X still uses `on_cut` with those same rules. It is not a Delete chord and it is not a selection.

## Delete chords that must not cut

Ctrl+Shift+Delete and Ctrl+Delete must not publish. `on_modified_special` calls `on_cut` only when the key is `KeySpecial::Delete`, shift is set, and ctrl is clear. The other modified Delete chords return unchanged with no clipboard text. They do not call `on_cut`. The edit stays. Nothing is copied.

Plain Delete must not publish and must not remove text. With neither shift nor ctrl, `handle_key_press` emits `PlatformEvent::KeySpecial`, and `on_event` leaves every plain `KeySpecial` unchanged. Plain Delete must not be implemented as Backspace and must not delete a character. There is no caret. `on_char` still removes one grapheme only for `'\u{8}'` while focus is Edit. That path is not Delete.

Evdev 111 is `KeySpecial::Delete`. `shortcut_from_key` does not return `Shortcut::Cut` for it, with or without ctrl or shift. `handle_key_press` resolves `key_special` before characters, so 111 is never `KeyChar`. `VK_DELETE` (`0x2E`) in `key_operation` is not evdev 111. Sash classifies Shift+Delete without ctrl as cut and does not classify plain Delete, Ctrl+Delete, or Ctrl+Shift+Delete. That helper does not read a clipboard. The Emperor host does not call `clipboard_blocked`.

## Paste and copy that stay

A paste request stays `capabilities.paste && focus == Edit`. Button focus does not request. `on_clipboard_text` stays capability-gated only. The tests must not add a focus check on the payload path. A later `Some` payload still calls `set_edit` when paste is true. `None` does not.

Copy stays whole-field and edit-focused. `copy_text` returns the edit, including an empty edit, only when copy is true and focus is Edit. Button focus returns nothing and does not change the edit.

Keymap-free `key_chars` still ignores ctrl. That GY-ZCODE-004 asymmetry must not be described as fixed. With an xkb map, ctrl still suppresses `KeyChar`. Evdev 111 does not reach that fallback, because it is already `KeySpecial::Delete`.

## Sandbox

`open_admitted` rejects a filesystem value other than `none` and rejects `network`, `subprocess`, or `web_content` when true. That check does not call Landlock, seccomp, a namespace, or a privilege drop. The threat model states that this cycle does not call Landlock, seccomp, or a capability-bounding set, and that the running `App` is not confined because no sandbox is attempted. Describing filesystem, network, subprocess, or web_content does not install a sandbox. EMP-M1-11 stays unmet.

## Not a pass

This note does not mark a ledger row passed. A unit test of `on_modified_special`, `on_cut`, `shortcut_from_key`, or `key_special` is not a launched clipboard, not a data-device offer, not a Wayland seat, and not EMP-M1-25. EMP-M1-25 stays unmet.
