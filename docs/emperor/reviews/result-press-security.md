# Result press security

`tests::result_press_does_not_move_focus` presses `state.count` and `window.root` through `on_pointer_press` and `on_pointer_release`. Parent re-ran it: 1 passed, 30 filtered. This note reviews those helpers in `crates/emperor-runtime/src/host.rs`. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## Else branch

`HitMap::at` walks last control to first. `state.count` is `(8, 112, 160, 32)`. Surface `(88.0, 128.0)` truncates to `(88, 128)` and hits `state.count`. `window.root` is `(0, 0, 320, 160)`. Surface `(8.0, 8.0)` hits `window.root`. That point is inside the client. It is not a miss. `window.add` is `(136, 64, 80, 32)`, and `(176.0, 80.0)` hits that id.

The role of `state.count` is `result`. The role of `window.root` is `client`. Neither is `button` or `edit`, and neither id is `window.add` or `window.amount`. The else branch of `on_pointer_press` clears `button_id` and returns `Step::Unchanged` with that hit id. It does not write focus. Focus stays where it was. Count, revision, edit, and result stay put. A fresh admitted controller is count 0, revision 0, edit `"1"`, result `"0"`, and focus Edit, and this press does not move them.

Only role `button` or id `window.add` sets `Focus::Button` and arms. Only role `edit` or id `window.amount` sets `Focus::Edit` and clears the arm. A press on `window.add` at `(176.0, 80.0)` sets focus to Button and records that id. A following press on `state.count` leaves focus on Button and clears the arm. `on_pointer_release` takes `button_id` before it compares. Release activates only when the release hit equals that armed id, and only then calls `press_add`. A release on `(88.0, 128.0)` after the result press has no armed id, so it does not activate. A release at `(176.0, 80.0)` after that press cleared the arm does not activate.

`PointerArm.button_id` stays private. The arm is visible only by the release. `on_pointer_press` does not call `press_add`. A press must not call `press_add`.

## Not an activation

A result press is not an activation and not a store. Do not ask to activate on press. The else branch returns `Step::Unchanged` and does not call `step_if_changed`. It does not call `press_add`, `on_key`, or `set_edit`.

## Not a grab

Clearing the arm is not a pointer grab and not a capture API. The field is the id recorded on a button press. `on_pointer_press` does not call a pointer-capture API. EMP-M1-25 stays unmet.

## Not Leave

`wl_pointer` `Leave` sets `last_pointer` to `None`. It does not push `PointerRelease`. Do not emit a synthetic `PointerRelease` on pointer Leave. A later left-button release with no stored point already emits `PointerRelease(-1.0, -1.0)`. `dip_from_surface_px` maps a non-finite value to `0`, and `(0, 0)` is inside `window.root`. Do not use NaN as a miss sentinel.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. Describing filesystem, network, subprocess, or web_content does not install Landlock or seccomp. EMP-M1-11 stays unmet. No ledger row moves. This note does not claim a signed release, sandbox enforcement, or SOTA.

## Not IME

This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. Ignoring those events is not support. EMP-M1-09 stays unmet.

## Not a pass

`tests::result_press_does_not_move_focus` calls the helpers on an admitted controller. Parent re-ran it, `button_release_inside_activates_and_outside_does_not`, and `edit_press_focuses_without_activating`: 1 passed each, 30 filtered. A unit test is not a Wayland seat. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
