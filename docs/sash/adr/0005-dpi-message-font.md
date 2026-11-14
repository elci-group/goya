# ADR 0005 — DPI message font

Status: accepted as a widening of the static-window profile. Runtime launch remains blocked.

## Decision

`sash.win32.static-window/0.0.3` keeps the cell tree from `0.0.2` and changes only the label font.

`0.0.2` called `GetStockObject(DEFAULT_GUI_FONT)` once. That stock font does not change when the window moves to a monitor with another DPI. No released application depended on that font. Scenes that `0.0.2` accepted still pass. Authored `text-size` stays rejected.

Each `STATIC` receives the system message font from `NONCLIENTMETRICSW.lfMessageFont`. The metrics structure is the Windows 10 shape: `LOGFONTW` is 92 bytes and `NONCLIENTMETRICSW` with `iPaddedBorderWidth` is 504 bytes. `SystemParametersInfoForDpi` is called with that size and the window DPI. `CreateFontIndirectW` creates the font. `WM_DPICHANGED` builds a replacement and deletes the previous object. The initial call is fail-closed: if metrics or font creation fail, the window is not created. A later DPI change that cannot create a font leaves the previous font installed.

This does not ratify a Windows build, a DPI device, or 100/125/150/200 percent scaling. Those remain untested.

## Consequences

The import floor gains `SystemParametersInfoForDpi` on `user32` and `CreateFontIndirectW` plus `DeleteObject` on `gdi32`. No new DLL is required. The previous `0.0.2` images do not contain this font path.
