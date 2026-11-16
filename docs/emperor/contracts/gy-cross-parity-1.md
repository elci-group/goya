# GY-CROSS-PARITY/1

Contract identifier: `GY-CROSS-PARITY/1`. Programme GY-EMP-001, revision 1.0.0. Date 16 November 2026.

Emperor and Sash SHALL share one functional oracle. Emperor SHALL NOT reimplement the counter's addition. Platform painting and window handles may differ. Stored integers may not.

Repository status: the oracle below already exists in `crates/goya-parity` and is fed by Sash admission. Wiring it into `crates/emperor-runtime` is specified for this cycle. A Linux headless result does not satisfy a Wayland launch gate. A Sash PE does not satisfy a Windows launch gate. Parity Lab phases 1 through 6 stay unmet. Windows execution stays BLOCKED.

## Shared oracle

The oracle is `goya_parity::session::CounterSession` and `goya_parity::keys::KeyController`.

`CounterSession::open` admits the scene with `sash::admit::open`. A static scene is diagnostic `SASH-CAP`. The session copies `initial_state`, `initial_input`, the profile, and the copy and paste flags. It does not invent a missing integer and it does not open a window.

`KeyController::open` opens that session and sets focus to the edit. Keys go through `sash::interactive::key_action`. Activation calls `CounterSession::activate`. The stored count changes only on `VisibleCommit::Store`.

On the fixture the session starts at count 0, revision 0, edit `1`, result `0`, copy allowed, paste denied.

One activation of the authored counter, with the edit left at `1`, a successful text write, the window not released, and a readback of the new value, stores count 1 at revision 1. The result text becomes `1`.

A second activation of the same kind stores count 2 at revision 2. The result text becomes `2`.

`emperor_runtime::oracle` performs the first activation by the same Enter path `KeyController` uses: message `WM_KEYDOWN` (`0x0100`), wparam `VK_RETURN` (`13`). `activate_add` repeats that path. Two successful presses return count 2 at revision 2. `key_action` maps Return to `Activate` without looking at focus, so Enter activates even when focus is the edit.

The oracle also checks paste denial. With `capabilities.paste` false, a paste attempt leaves the edit at `1` and does not change count or revision. If paste were allowed, the runtime would not special-case a deny. The fixture is not that scene.

These cases leave the stored count unchanged: a rejected edit such as `00`, overflow, a busy activation, a release during the write, and a mismatched readback. The mismatched readback restores the previous result text and does not store. Emperor calls the session for those decisions. It does not code a second adder.

Copy is allowed. Paste is denied. Cut is `default` because the scene does not author cut. `default` is not deny. A scene that sets cut to deny drops the cut before the edit changes. Delete without Shift is not a cut.

## What does not satisfy a gate

A Linux headless result, including `CounterSession` in process, `parity-counter`, and `App --oracle`, does not satisfy a Wayland launch gate. The window gate needs a real `GoyaWindow` on a display, recorded as smoke status `passed`.

A Sash PE, including a `sash verify` result whose runtime qualification is BLOCKED, does not satisfy a Windows launch gate. The image has to be executed on Windows. Wine is not that launch.

An Emperor ELF that links and passes the oracle while `WAYLAND_DISPLAY` is empty is a blocked qualification, exit 3. It is not cross-platform parity.

## Declared platform differences

These differences are declared. They are not permission to change the integers.

Win32 uses an HWND tree: a heading `STATIC`, an `EDIT`, a default button, and a result `STATIC`. Wayland uses one `xdg_toplevel` and software primitives. The toplevel is not an HWND. Emperor does not pretend the handles are the same object.

Resize clips and does not reflow on both designs. Sash keeps each control at its authored DIP origin and clips the size to the client. Emperor keeps the same DIP rectangles for hit testing and does not solve layout again. Neither side stretches the four rectangles to a new aspect.

Enter, Tab, and Space follow Sash `key_action` and the `KeyController` rule built on it.

- Enter (`VK_RETURN`) is `Activate`.
- Tab is `FocusNext`. Shift-Tab is `FocusPrev`. Focus moves only between the edit and the button. It starts on the edit.
- Space is `Dispatch` from `key_action`. `KeyController` activates on Space only when focus is the button (`VK_SPACE` `0x20` on `WM_KEYDOWN`). Space on the edit does not activate.

On Linux the host reports Enter as `KeyChar` newline, Tab as `KeySpecial::Tab`, and Shift-Tab as a modified special. The runtime maps those onto the Win32 messages above. It does not invent a different focus policy. Space while the edit is focused may be inserted into the edit string. That insertion uses `CounterSession::set_edit` and does not change count, revision, or result. Backspace deletes the last Unicode grapheme of the edit and does not change the count. Those edit mutations are a Linux shell around the shared session. They are not a claim that the Sash `EDIT` control was launched, and they are not IME support.

Pointer hits use `goya_parity::pointer::hit_map` on the scene. Rectangles are the admitted DIP values. The walk is front to back, last control to first. A point is inside when `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, using 64-bit arithmetic. A point outside the client is a miss. A hit on `window.add` focuses the button and activates. A hit on `window.amount` focuses the edit and does not activate. Pixel coordinates truncate toward zero into DIP. At 250 millipoints per pixel and buffer scale 1, that truncation does not scale. Fractional outputs are not qualified.

Copy on Linux, when allowed, offers the current edit through `GoyaWindow::set_clipboard_text`. Paste, when denied, ignores `Shortcut::Paste` and ignores a later clipboard text event. The Sash image enforces the same allow and deny decision in the edit subclass and has not been launched. Matching policy in process is not a launched clipboard on either platform.

Close on Windows is specified as font release, semantic-id removal, subclass restoration, and class unregister, and that sequence has not been launched. Close on Linux sets the host's close request and ends the live loop. Those are not the same calls. Both are unqualified until observed on their own systems.

## Not in the oracle yet

Undo, redo, document save and restore, theme changes, menus, and a second window are absent from this scene. The parity application described in the programme is larger than the counter. The counter is not that application. Visual tolerance is not declared. No pixel baseline exists.
