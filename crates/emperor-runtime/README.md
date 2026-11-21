# emperor-runtime

Shared Linux runtime for one admitted Goya counter. Native IR uses schema `GY-EMP-NIR/1` and profile `emperor.linux.interactive/0.0.1`. Admission, focus, and addition go through the shared counter session. This crate does not reimplement that arithmetic.

`emperor-app` reads `--nir PATH` when that flag is set. A path with a `..` component is rejected. Otherwise it runs a `CompiledScene` copied in at build time by `EMPEROR_EMBED`, if one was provided. `--oracle` is the default. `--smoke` opens the configured window backend and submits one frame. `--exercise-oracle` runs the canonical counter interaction script headlessly, and `--exercise` runs the same script before submitting a frame through the window backend. Those modes feed synthetic input directly to the application. `--live` runs until the window closes, and `--frames N` stops after N pump iterations.

A successful run prints `count={} revision={} presented={} wayland={} window={} copy={} paste={} edit={}`. For Wayland, `presented` counts software buffer attachments and surface commits. Exit 0 is success. Exit 1 is usage or a bad native IR. Exit 5 is a window-backend or runtime failure.

## Limitations

- Single counter profile. Other scenes are not a runtime.
- No Vulkan. Frames use the software present path on the existing Wayland host.
- No IME qualification. Preedit and commit text are ignored.
- No sandbox.
- Resize does not reflow. Hit testing keeps the admitted DIP rectangles; the host clips.
