# Contract security

This slice corrects the documents and does not change the host. `on_char`, `on_pointer_press`, `on_pointer_release`, `copy_text`, `on_cut`, `paste_shortcut_requests_clipboard`, and `on_clipboard_text` in `crates/emperor-runtime/src/host.rs` stay as they are. The sandbox gap is the one in `docs/emperor/threat-model.md`. This note does not change the gap ledger and does not pass any row. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.

## Space

Space does not insert. The host calls KeyController for VK_SPACE. In `on_char`, the character `' '` calls `on_key` with `WM_KEYDOWN` and `VK_SPACE`. The edit stays unchanged. That path does not call `set_edit`. The button activates when focus is the button, because `KeyController` activates on that dispatch only then. Space on the edit does not activate and does not append a character.

## Pointer

A press on the button records the id and does not activate. `on_pointer_press` sets focus to the button and stores that id on `PointerArm`. It does not call `press_add`. Release on that same id activates. `on_pointer_release` calls `press_add` only when the release hit is the recorded id. Release elsewhere does not. A miss, or a press on the edit, clears the record and does not activate. The record is not a pointer-capture API. EMP-M1-25 stays unmet.

## Copy

Copy publishes the whole edit only when copy is allowed and focus is the edit, including an empty string. `copy_text` returns that string only in that case. Button focus publishes nothing. Not a selection. `Shortcut::Copy` calls `set_clipboard_text` only when `copy_text` returns text.

## Paste

Paste requests text only when paste is allowed and focus is the edit. `paste_shortcut_requests_clipboard` is that check. Button focus does not request. `on_clipboard_text` stays capability-gated only and is not focus-gated. A later `Some` payload still calls `set_edit` when paste is true, including after focus has moved. `None`, and any payload while paste is false, leave the edit unchanged.

## Cut

Shift+Delete without ctrl, and Ctrl+X without shift, cut the whole focused non-empty edit when cut is allow or default. `on_modified_special` calls `on_cut` only for Delete with shift set and ctrl clear. `Shortcut::Cut` calls `on_cut` with the same rules. Deny, focus other than the edit, and an empty edit publish nothing and do not call `set_edit`. Plain Delete, Ctrl+Delete, and Ctrl+Shift+Delete do not cut and do not remove text. Those chords do not call `on_cut`. No caret.

## Sandbox

`docs/emperor/threat-model.md` says describing capabilities is not a sandbox. A manifest that says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. Under privileged Linux APIs, the compiler does not request a sandbox and then continue. It does not call Landlock, seccomp, or a capability-bounding set. The control that does not exist is any confinement of the running `App`. The process can do what the user who launched it can do. Failure to establish a sandbox cannot be observed because no sandbox is attempted. That is an honest gap, not a pass. Describing filesystem none does not install Landlock or seccomp. EMP-M1-11 stays unmet.

## Not a pass

This note does not mark a ledger row passed. Correcting the sentences does not install a filter and does not offer clipboard text. A document edit is not a launched clipboard and not a Wayland seat. EMP-M1-25 stays unmet.
