# Insert security

The working tree has the edit-focus copy gate, `Shortcut::Cut`, and the Insert chords: Ctrl+Insert is Copy, Shift+Insert is Paste, and Ctrl+C and Ctrl+V require shift up. This note was written against `17721b35512f` plus that plan. The "current source" section describes the tree before the Insert edit. The gates below are the ones the code keeps. This note does not change the gap ledger and does not pass any row. EMP-M1-11 stays unmet.

Reviewed `crates/sash/src/interactive_clipboard.rs`, `crates/emperor-runtime/src/host.rs`, `crates/goya-wayland/src/lib.rs`, `docs/emperor/threat-model.md`, `docs/emperor/gap-ledger.md`, and `docs/emperor/reviews/clipboard-security.md`.

## Current source

`shortcut_from_key` returns `None` unless Control is held. Evdev 46 is Copy and evdev 47 is Paste with or without Shift. Evdev 45 is Cut only without Shift. Evdev 16 is Quit, and 105 and 106 are the session chords; those three ignore Shift. Evdev 110 is not a shortcut, not `KeySpecial`, and not in the US subset table, so Insert produces no character on the keymap-free path. `handle_key_press` returns as soon as the helper yields a shortcut.

With an xkb map, `key_char_from_state` emits no `KeyChar` while Control is held. Without a map, `key_chars` ignores its ctrl argument and calls `key_char`. An unrecognized ctrl combo can still emit a letter. That asymmetry is in the source now. The plan does not remove it, and it must not be described as fixed.

`copy_text` returns the whole edit, including an empty edit, only when `capabilities.copy` is true and focus is Edit. Button focus returns `None`. `on_event` calls `set_clipboard_text` for `Shortcut::Copy` only when that function returns text. Cut still uses `on_cut`. Deny, focus that is not Edit, or an empty edit publishes nothing and does not call `set_edit`.

`paste_shortcut_requests_clipboard` takes the IR only. It returns `capabilities.paste` and does not read focus. `Shortcut::Paste` calls `request_clipboard_text` only when that function is true. `on_clipboard_text` is not focus-gated. It calls `set_edit` only for `Some(text)` when paste is true. A denied paste does not call `set_edit` for a payload, for an empty string, or for `None`.

`open_admitted` rejects a filesystem value other than `none` and rejects `network`, `subprocess`, or `web_content` when true. That check does not call Landlock, seccomp, a namespace, or a privilege drop. `crates/sash/src/interactive_clipboard.rs` classifies Win32 messages, including `VK_INSERT` (`0x2D`), and does not read a clipboard. The Emperor host does not call `clipboard_blocked`. A Win32 Insert classification is not this Wayland path. Evdev 110 is not `VK_INSERT`.

## What must stay absent

A denied paste must not call `set_edit`, including when the data event arrives after focus has moved. Paste false must not call `request_clipboard_text`. Button focus must not call `request_clipboard_text`. `on_clipboard_text` must not grow a focus check. The request is the focus gate, because the data event can arrive after focus has moved. A later `Some` payload still calls `set_edit` when paste is true. Copy, including the planned Ctrl+Insert, must not publish when `capabilities.copy` is false or when focus is not Edit. Ctrl+Shift+C must not be Copy. Ctrl+Shift+V must not be Paste. Ctrl+Shift+Insert and plain Insert must not be Copy or Paste. Shift+Insert with Control held must not be Paste. No non-Control chord other than Shift+Insert may become a shortcut. There is no selection. Describing `filesystem`, `network`, `subprocess`, or `web_content` does not install a sandbox. The plan still has no Landlock or seccomp. EMP-M1-11 stays unmet. The keymap-free path ignoring ctrl must not be described as fixed.

## Insert

Evdev 110 may become a shortcut on two chords only. Control without Shift is `Shortcut::Copy`. Shift without Control is `Shortcut::Paste`. Control+Shift and plain Insert stay `None`. The helper now returns `None` whenever Control is up, so Shift+Insert is the one non-Control chord the plan admits. Every other key without Control stays `None`.

Ctrl+C (evdev 46) is Copy only without Shift. Ctrl+Shift+C is not Copy. Ctrl+V (evdev 47) is Paste only without Shift. Ctrl+Shift+V is not Paste. Ctrl+Q and the session chords still ignore Shift. They are not clipboard operations. `Shortcut::Quit` still quits. The session shortcuts still hit the host catch-all and do not publish or request.

Ctrl+Insert uses the existing `copy_text` rules. The publish, when one is allowed, is still `GoyaWindow::set_clipboard_text`: an in-memory `text/plain` offer of the whole edit, including an empty edit. Missing a data device makes that call a no-op. Button focus publishes nothing. There is no selection.

Shift+Insert, and Ctrl+V without Shift, use the narrowed paste request. `paste_shortcut_requests_clipboard` returns true only when `capabilities.paste` is true and focus is Edit. Button focus does not call `request_clipboard_text`. The payload is still applied only in `on_clipboard_text`, and only for `Some` text when paste is true. `None` does not change the edit. An empty `Some` still replaces the edit when paste is true. That replacement is the whole edit, not a caret insert. If focus leaves the edit after an allowed request, the later payload still writes. That is the specified gate, not a second one.

Once Ctrl+Shift+C and Ctrl+Shift+V are not shortcuts, `handle_key_press` falls through. With an xkb map, Control still suppresses `KeyChar`, so those chords emit nothing. Without a map, `key_chars` still ignores ctrl, so the same chords can emit `C` or `V`. Insert itself is still absent from the US subset table, so a `None` from the helper still emits no letter for evdev 110 on that path. The asymmetry stays. It is not a fix.

Sash `key_operation` already classifies Ctrl+Insert without Shift as copy and Shift+Insert without Control as paste. It does not classify Ctrl+Shift+C, Ctrl+Shift+V, Ctrl+Shift+Insert, or plain Insert. The helper does not read a clipboard. The subclass has not been launched. Seat Control is bit 2 of `mods_depressed`, and Shift is bit 1. That is not `GetKeyState`.

## Review result

This review covers the current source plus the plan above. It does not mark a ledger row passed. A passing unit test of `shortcut_from_key`, `copy_text`, or `paste_shortcut_requests_clipboard` is not a launched clipboard, not a data-device offer, and not EMP-M1-11.
