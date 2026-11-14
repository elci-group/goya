# ADR 0002 — First proof is one static window

Status: accepted as the first S1 link. The v0.0.1 vertical slice in SASH-ENT-001 is not this slice. Nested `@cell` trees are admitted later by ADR 0004; this record stays the single-cell decision.

## Decision

The first linked profile is `sash.win32.static-window/0.0.1`.

It compiles one top-level Win32 window and one `STATIC` label from one `@cell`. Text, id, and extent are preserved. Interactions, children, buttons, inputs, fill, stroke, and undeclared geometry are errors.

Coordinate rule for this profile only: 250 millipoints equal 1 device-independent pixel, and the millipoint extent must divide evenly. This copies the Wayland host ratio so the numbers are checkable. It does not ratify Windows 11, DPI devices, or other Goya targets.

The default presentation toolkit for this proof is Win32 through handwritten bindings in the generated Rust, not WinUI and not a WebView. COM and WinRT are not used. Gestures stay unadmitted on Windows, so the label is not an edit control.

## Consequences

`App.exe` can prove that admission, emission, and the MSVC linker produce a GUI PE. It cannot prove notifications, taskbar identity, startup, retention, nested layout, or accessibility. Those remain out of the profile until a later ADR admits them.

Physical process boundaries stay inside `App.exe`. No helper executable is generated.
