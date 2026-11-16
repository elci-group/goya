# Leave security

The slice reports a positionless release outside the client and does not keep the old point.

Reviewed `push_left_button` and the `wl_pointer` match in `crates/goya-wayland/src/lib.rs`, and `on_pointer_press` and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`, against the sandbox paragraph in `docs/emperor/threat-model.md`. This note does not change the gap ledger and does not pass any row.

## Leave

`wl_pointer` `Leave` sets `last_pointer` to `None`. Enter and Motion replace it with the live surface point. Leave does not copy the old point and does not clear `PointerArm`. The arm is cleared only when a release reaches the host.

## Left button

Button `0x110` calls `push_left_button`. A press with a stored point emits `PointerPress` at that point. A release with a stored point emits `PointerRelease` at that point. A press with no stored point emits nothing. A release with no stored point emits `PointerRelease(-1.0, -1.0)`. It does not reuse the coordinates from before leave. Those can sit inside `window.add`. The admitted button is `(136, 64, 80, 32)`, and `(176, 80)` is inside it. Feeding that point to the host after leave would match the armed id and call `press_add`. Any other button code still emits nothing.

## Host

`on_pointer_release` truncates each coordinate toward zero, then asks the hit map. `dip_from_surface_px(-1.0)` is `-1`. A non-finite value truncates to `0`, and `(0, 0)` is inside the admitted client `(0, 0, 320, 160)`. The sentinel stays `-1.0` so the truncated point stays outside.

`HitMap::at` returns `None` when the point is outside the client, before it walks the other controls. Containment requires `x >= left` and `y >= top`. `(-1, -1)` fails both. The hit is `None`.

The host takes `PointerArm.button_id` before it compares. It calls `press_add` only when the release hit equals that id. A miss does not. The arm is clear either way. A later release on the button has no recorded id, so it does not activate. A press still only records the button. It does not call `press_add`. A positionless press never reaches the host.

A release after a later Enter or Motion uses that new stored point, not the sentinel. The positionless release is only the path where `last_pointer` is still `None`.

## Not a grab

This is not a pointer-capture API and not a grab. The arm is the id recorded on press. Leave does not keep motion after the clear and does not request a release point. EMP-M1-25 stays unmet.

## No sandbox

No sandbox is installed. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set, and it must not report those as enabled. The running `App` is not confined, because no sandbox is attempted. Failure to establish a sandbox cannot be observed because no sandbox is attempted. That is an honest gap, not a pass. EMP-M1-11 stays unmet.

## Not a pass

This note does not mark a ledger row passed. A unit test is not a seat. `release_after_leave_is_outside` sets `last_pointer` and calls `push_left_button`. `release_without_a_position_does_not_activate` calls `on_pointer_release` with `(-1.0, -1.0)`. Neither opens a seat. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.
