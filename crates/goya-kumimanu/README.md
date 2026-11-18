# goya-kumimanu

Kumimanu is Goya's native X11 backend. It owns the X11 connection, one
window, core events, and a software `PutImage` surface, under the contract
`KUM-X11/0.0.1`. It does not compile ELF files, speak Wayland, or select a
GPU API; text shaping stays out of the crate, and callers present pixels.

The implementation is pure-Rust `x11rb` 0.13 with `default-features = false`
(no libxcb), and the crate is `#![forbid(unsafe_code)]`.

## Minimal usage

The sketch mirrors `crates/emperor-runtime/src/host_kumimanu.rs`:

```rust
use goya_kumimanu::{Event, Session, WindowConfig};

let mut session = Session::connect()?;
let id = session.create_window(WindowConfig::new("title", width, height))?;
session.show_window(id)?;
loop {
    for event in session.poll_events(timeout)? {
        if matches!(event, Event::CloseRequested) {
            // break the loop
        }
    }
    session.present_rgba(id, frame_w, frame_h, &rgba)?;
}
session.close_window(id)?;
```

Copy/cut publish text with `set_clipboard_text`; paste converts the
`CLIPBOARD` selection via `request_clipboard_text` and arrives as an
`Event::ClipboardText(Option<String>)`.

## Phase lifecycle

`Phase` values: `Connecting`, `Connected`, `WindowCreated`, `Mapped`,
`Closing`, `Terminated`, `Failed`. `Failed` is the error sink; `Terminated`
is the only clean end. A session does not return to `Connected` after a
window has been created. Legal edges from `phase::advance`:

```text
Connecting --Connected--> Connected          Connecting --Fault--> Failed
Connected  --Create----> WindowCreated       Connected  --Fault--> Failed
Connected  --Close-----> Terminated
WindowCreated --Map----> Mapped              WindowCreated --Fault--> Failed
WindowCreated --Close---> Closing
Mapped      --Map----> Mapped (repeat)      Mapped      --Fault--> Failed
Mapped      --Close---> Closing
Closing     --Destroyed--> Terminated       Closing     --Fault--> Failed
Failed      --Destroyed--> Terminated
Terminated  --Destroyed--> Terminated
```

Any other pair is refused: `advance` returns `Err(current_phase)` unchanged.

## Capabilities and limitations

From the crate documentation:

- `LOGICAL_SCALE` is 1: pointer positions are X11 window pixels and are the
  Goya logical coordinates. High-DPI, RandR output scale, and fractional
  scale are discovered only as capability bits and are not applied.
- Not implemented: `PRIMARY`, XDND, XIM, XInput2 devices, the Present
  extension, MIT-SHM transport, Render acceleration, Vulkan, and EGL.
  Missing extensions reduce `Capabilities` and do not abort the connection.
  Accessibility is not bridged.
- Clipboard: `CLIPBOARD` text only, valid UTF-8 or `STRING`, never empty.
  INCR works in both directions for text over 256 KiB, capped at
  `MAX_CLIPBOARD_BYTES` = 1 MiB total (an oversized transfer is drained and
  discarded, not buffered). At most four INCR transfers are served at once;
  one idle for five seconds is dropped.
- Hostile clients: another client on the same X server can still observe or
  inject input. Kumimanu does not claim isolation the X11 protocol cannot
  provide.

## Errors

`Error` variants and when each occurs:

| Variant | When it occurs |
| --- | --- |
| `Connect(String)` | the initial connection to the X server failed |
| `Protocol { code, sequence }` | the server reported a protocol error at that sequence number |
| `ProtocolLimit` | too many protocol errors accumulated |
| `UnsupportedVisual { depth, bits_per_pixel }` | the visual is not a supported truecolor format (only 16/32 bpp) |
| `InvalidConfig(&'static str)` | the `WindowConfig` is rejected (bounded lengths, no interior NUL) |
| `StaleWindow` | the window id is not the live window |
| `WrongPhase { current, attempted }` | the operation is illegal in the current `Phase` |
| `SizeMismatch { .. }` | the presented frame dimensions do not match the window |
| `PixelBuffer(&'static str)` | the RGBA buffer is invalid (zero size, wrong length, overflow, above `MAX_PIXELS`) |
| `EventQueueSaturated { capacity }` | the event queue reached its capacity |
| `ServerDisconnect` | the X server disconnected |
| `Io(String)` | a socket error |

Presented frames are tightly packed top-down straight-alpha RGBA8, composited
over a background and packed into the server visual by `frame::encode`, with
a cap of `MAX_PIXELS` = 16 megapixels per present.

## Testing

- `cargo test -p goya-kumimanu` runs the headless (pure) tests; no display
  is needed.
- Tests marked `#[ignore]` open real X11 windows. Run them with
  `cargo test -p goya-kumimanu -- --ignored` while `DISPLAY` is set; they
  assert `DISPLAY` is present rather than silently passing.
- GTK interop tests (`tests/gtk_interop.rs`, with `tests/gtk_peer.py`) need
  `DISPLAY`, `python3`, and PyGObject with GTK 3. They fail, not skip, when
  the peer is absent. They run on a private selection named
  `GOYA_KUMIMANU_TEST_*` (suffixed with the process id), so the desktop
  clipboard is never read or replaced.
- CI runs the headless suites in `unit-and-property-tests` and the
  `#[ignore]` and GTK suites in `kumimanu-display-tests`, which drives them
  with `xvfb-run` in `.github/workflows/emperor.yml`. Xvfb is a real X
  server, so those suites run against live protocol traffic in CI.

## Integration

- The `emperor` CLI accepts `--window kumimanu` (or `wayland`; wayland is
  the default, duplicates are rejected).
- `emperor-runtime` exposes a `kumimanu` cargo feature that enables the
  optional `goya-kumimanu` dependency; `host_kumimanu.rs` then presents the
  lowered scene through this backend.
- `emperor-runtime`'s `open_window` routes via `EMPEROR_WINDOW_BACKEND`
  (read at compile time with `option_env!`). With the `kumimanu` feature
  enabled, the kumimanu backend is used unless the variable is explicitly
  `wayland`; an unset variable or any other value keeps kumimanu. Without
  the feature, only `EMPEROR_WINDOW_BACKEND=kumimanu` selects kumimanu, and
  `open_window` then errors ("linked without it"). `--features kumimanu`
  alone is sufficient.
