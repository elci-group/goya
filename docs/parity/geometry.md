# Admitted counter geometry

Source: `crates/sash/fixtures/interactive-counter.gscene`

Profile: `sash.win32.interactive/0.0.2`

Client: 320 by 160 DIP

Rectangles (x, y, width, height) in DIP:

- heading `window.root`: (0, 0, 320, 160)
- edit `window.amount`: (8, 64, 120, 32)
- button `window.add`: (136, 64, 80, 32)
- result `state.count`: (8, 112, 160, 32)

This is the admitted DIP layout from sash.win32.interactive/0.0.2. It is not a pixel comparison. No visual tolerance is declared. The image has not been launched on Windows. parity-counter does not draw these rectangles.
