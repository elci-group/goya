# Leave parity

The working tree is gaining a positionless release at `(-1, -1)` so a dropped leave-then-release cannot leave the arm set. This note compares that slice with Sash button activation. Sash is `command_activates` in `crates/sash/src/interactive_commit.rs`, called from the generated `WM_COMMAND` arm in `crates/sash/src/interactive.rs`. The host is `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. The seat path is the `wl_pointer` dispatch in `crates/goya-wayland/src/lib.rs`.

EMP-M1 is not passed. The system button has not been launched. A unit test is not a Wayland seat and not a Win32 button. No ledger row moves. EMP-M1-25 stays unmet.

## Sash

`command_activates` is true only when the control id equals the button id and the note is 0. The window procedure reads the id from the low 16 bits of `wparam` and the note from the high 16 bits. Note 0 is `BN_CLICKED`. The button is created as `BS_DEFPUSHBUTTON`. The system button posts that note on release inside the control. A release outside the control does not post it. That control has not been launched. The generated source has no button procedure. Emperor does not receive `WM_COMMAND`.

## Leave, then release

`wl_pointer` `Leave` sets `last_pointer` to `None`. Enter and Motion store the live surface point. Button `0x110` emits `PointerPress` or `PointerRelease` only while that point is present, so a left-button release after Leave pushes nothing. The host never runs. `PointerArm.button_id` stays at the id recorded on press. A later release that does land on the button would call `press_add`. That is the stuck arm.

The slice keeps the clear on Leave. A later left-button release (`0x110`) with no position emits `PointerRelease(-1.0, -1.0)` instead of being dropped. It does not reuse the pre-leave point. A release that still has a surface point keeps that point. A press with no position is not given this sentinel.

## Host

`on_pointer_release` truncates each coordinate toward zero and asks `hit_map`. It takes the recorded id off the arm before it compares. `press_add` runs only when the release hit id equals the id recorded on press. Any other hit, including a miss, leaves the step unchanged. The arm stays clear either way.

`dip_from_surface_px(-1.0)` is `-1`. The admitted client is `(0, 0, 320, 160)`. `window.add` is `(136, 64, 80, 32)`. Containment requires `x >= left` and `y >= top`. `(-1, -1)` is outside the client, so the hit is `None`. That is not the armed button. `press_add` does not run, and the arm is cleared. A later release on the button has no recorded id, so it does not activate.

Release inside the armed id still activates. Release outside still does not. The host test already uses `(176, 80)`, which is `window.add`, and `(8, 8)`, which is not. After the outside release takes the arm, the next release on the button does not activate. The sentinel is that same miss. It is not a new hit rule.

`press_add` still enters `on_key` with `WM_KEYDOWN` and `VK_RETURN`. The integers stay on `KeyController`. The gesture is not a key.

## Not the last inside point

Reusing the last inside point after leave would be a false click. The slice must not do that. The point stored before Leave can sit inside `window.add`. Feeding it to `on_pointer_release` would match the armed id and call `press_add`. Sash posts note 0 only for a release inside the control. A coordinate from before Leave is not that release. `0` is the wrong substitute as well. A non-finite coordinate truncates to `0`, and `(0, 0)` is inside the client. `(-1, -1)` stays outside.

## Not a grab

The arm is only the id recorded on press. It is not `SetCapture` and it is not a `wl_pointer` grab. Leave still means the seat has no surface position. The slice does not keep motion after Leave and does not ask the compositor for the release point. This is not a seat grab. EMP-M1-25 stays unmet.

## Not a pass

A unit test is not a Wayland seat and not a Win32 button. The scripted interaction suite is not a seat event. EMP-M1 is not passed. No ledger row moves.
