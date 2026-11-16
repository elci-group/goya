# Interaction parity

Commit `594bac7bdda15b36dd296aaf2dfc1c0dbbacd1b4`. Compared `crates/emperor-runtime/src/host.rs` with `sash::interactive::key_action` (`crates/sash/src/interactive_keys.rs`, pumped from `crates/sash/src/interactive.rs`) and with `goya_parity::keys::KeyController` and `goya_parity::pointer::hit_map`. EMP-M1 is not passed. The generated Win32 image has not been launched. `oracle` and `activate_add` set `wayland` to `not-requested` and `presented` to 0. That headless path is not a Wayland seat event.

## Enter and `KeyAction::Activate`

`key_action` returns `Activate` only for `WM_KEYDOWN` (`0x0100`) and `VK_RETURN` (`13`). Focus is not read. Any other message is `Dispatch`. The pump then sends `BM_CLICK` to the button.

`KeyController::on_key` uses that function. `Activate` calls `activate`, so Enter stores while focus is still the edit.

The seat does not emit `VK_RETURN`. Evdev 28 becomes `PlatformEvent::KeyChar('\n')` (`key_chars` in `crates/goya-wayland/src/lib.rs`). The host maps that character to `press_add`, which synthesizes `WM_KEYDOWN` / `VK_RETURN` / `shift_down` false. Shift does not change the Return action, so this chord matches.

Mismatches: keypad Enter (evdev 96) is not forced to `'\n'`. With an XKB map, control characters are dropped, so `KeyController` never runs. Sash has no second code; every `WM_KEYDOWN` with wparam 13 is `Activate`. Any `KeyChar('\n')`, not only evdev 28, also activates. The host discards the `KeyAction` value.

## Tab and Shift-Tab

Tab is `FocusNext`. Shift-Tab is `FocusPrev`. Ctrl is ignored. The pump does not dispatch the key. `focus_cycle` walks the edit, then the button. If `message.hwnd` is neither, both directions focus the edit. Setup focuses the edit.

`KeyController` only toggles `Focus::Edit` and `Focus::Button`. It cannot snap an unknown target to the edit.

The host maps `KeySpecial::Tab` to `on_key(WM_KEYDOWN, VK_TAB, false)` and a modified Tab through `modifiers.shift` only. On that two-stop cycle, including Ctrl-Tab when the seat still delivers Tab, it matches `KeyController`.

Mismatch: a Sash key whose hwnd is not the edit or the button focuses the edit. The host cannot. A press on `window.root` or `state.count` leaves the enum unchanged.

## Space

`key_action` returns `Dispatch` for `VK_SPACE` (`0x20`), never `Activate`. The controls disposition says Space is delivered to the focused control and activates the button only while that button has focus. The generated source has no button procedure. That click would be the system button. It has not been launched.

`KeyController`, inside `Dispatch`, activates only when focus is the button, the message is `WM_KEYDOWN`, and `wparam` is `0x20`. Space on the edit does not activate and does not change the edit. The unit test leaves the edit at `1`.

The host calls that `on_key` for `KeyChar(' ')` only when focus is the button. Otherwise it appends `' '` with `CounterSession::set_edit` and does not call `on_key`. Count, revision, and result stay put. That insertion drifts from `KeyController`. It is not a launched Sash `EDIT`.

With an XKB map, Ctrl-Space emits neither `KeyChar` nor `Shortcut`, so the host does nothing. `KeyController` would still activate a button-focused `WM_KEYDOWN` / `VK_SPACE`, because it ignores Ctrl. With no keymap, `key_char` ignores Ctrl and emits `' '`.

## Pointer

`hit_map` walks last control to first. Containment is `x >= left`, `y >= top`, `x < left + width`, `y < top + height`. An interior point of `window.add` is not the root. An interior point of `window.amount` is the edit. Outside the client is `None`.

The host handles button `0x110` `PointerPress` only. It truncates the surface `f64` toward zero. It does not multiply by buffer scale or by 250 millipoints. Id `window.add` or role `button` sets `Focus::Button` and calls `press_add`. Id `window.amount` or role `edit` sets `Focus::Edit` and does not activate. `state.count` and `window.root` do nothing. `PointerRelease` is ignored. Non-finite input becomes 0.

Mismatches: Sash mouse activation is `WM_COMMAND` with the button id and note 0 (`BN_CLICKED`). The system button posts that note on release inside the control. The host activates on press, ignores release, and has no capture, so a press that leaves the button still counts. The press is aliased to `VK_RETURN`. `KeyController` has no pointer entry. A `hit_map` test is not seat button `0x110`. The predicate is wider than the button id. On the admitted counter the button id and role coincide, and the edit id and role coincide. Fractional scale is not qualified.

## Copy allow and paste deny

The fixture edit is `copy: allow` and `paste: deny`. The session stores those flags. `open_admitted` requires them to equal `ir.capabilities`. The host does not write them again.

`clipboard_blocked` is the edit subclass. Allowed copy does not block `WM_COPY`, Ctrl-C, or Ctrl-Insert. Denied paste blocks `WM_PASTE`, Ctrl-V, and Shift-Insert. Cut is blocked only when the mode is deny. The helper does not read the clipboard. The subclass has not been launched.

`Shortcut::Copy` calls `set_clipboard_text` with the whole edit when `ir.capabilities.copy` is true, at any focus. It does not read `session.copy_allowed` and does not call `clipboard_blocked`. `Shortcut::Paste` requests text only when paste is true. `ClipboardText` calls `set_edit` only for `Some` text when paste is allowed. A denied payload, including empty, is ignored and does not change count.

Mismatches: allowed copy offers the entire edit, including while the button is focused. The Sash gate is the edit subclass, so a button-focused chord never reaches it, and that control copies its selection rather than forcing the whole field. The crate says the keymap-free path never emits `Shortcut`. `handle_key_press` still calls `shortcut_from_key` with `ctrl` taken from bit 2 of `mods_depressed`, keymap or not. A seat that leaves that bit clear delivers no copy. That is not `GetKeyState(VK_CONTROL)`. Shift-Delete is `Unchanged` for `cut == "deny"` and for every other mode, including this fixture's `default`. Sash lets a non-deny cut through. `prove_paste_denied` and `paste_is_ignored` only skip `set_edit("999")` when the flag is false. They emit neither `Shortcut::Paste` nor `ClipboardText`.

## Drift from `KeyController`

The host can leave the controller without a Win32 key:

- Letters, Space on the edit, and Backspace call `set_edit`. `on_key` is not used. The count stays put. `KeyController` has no insert path.
- An edit hit assigns `focus` directly. Only Tab messages move focus inside `on_key`.
- A button hit does re-enter `on_key` through `press_add`. The integers stay on that activate path. The gesture is still not a key.
- Clipboard and close use `ir.capabilities` and `GoyaWindow`. Plain Delete is ignored, so the host neither deletes nor cuts.
- Preedit and commit are ignored and never reach the controller. That is not IME support.
- Seat translation can drop keypad Enter and mapped Ctrl-Space, or activate on a newline that was not `VK_RETURN`.

`RunMode::Oracle` calls `press_add` in process. A smoke that presents one frame without a seat key does not cover this list. EMP-M1 is not passed. Windows launch is not claimed.

## Working tree

The sections above describe commit `594bac7`. The working tree has since moved four host paths closer to the shared actions. None of these is a seat event, a Windows launch, or an EMP-M1 pass.

- Space on the edit calls `KeyController::on_key` with `VK_SPACE` and does not insert. Button focus still activates.
- Evdev 28 and 96 both become `KeyChar('\n')` before the XKB path, and that character still calls `press_add`.
- Shift+Delete with cut `allow` or `default` publishes the whole focused edit and clears it. `deny` does not. This is not the Sash selection cut.
- A button press records the control id and does not activate. Release on that same id calls `press_add`. Release elsewhere, or a release with no press, does not. A press outside the button clears the arm. `--exercise` presents that scripted frame (count 3, revision 2) on a real Wayland window. The compositor is not identified. EMP-M1-25 stays unmet.
- Copy publishes the whole edit only while focus is the edit and copy is allowed. Button focus publishes nothing. Ctrl+X is `Shortcut::Cut` and uses the same whole-field cut as Shift+Delete. There is still no selection. See `clipboard-parity.md`.
- Plain Delete, Ctrl+Delete, and Ctrl+Shift+Delete do not cut and do not remove text. Cut `allow` follows the same whole-field rule as `default`. There is no caret. See `delete-parity.md`.
