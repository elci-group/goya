# Outside press security

`tests::outside_client_press_does_not_move_focus` follows `tests::result_press_does_not_move_focus` in `crates/emperor-runtime/src/lib.rs`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the miss path already in that file. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## Miss branch

`HitMap::at` returns `None` when the point is outside the client. The client is `window.root` at `(0, 0, 320, 160)`. A point is inside when `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, using 64-bit arithmetic. `dip_from_surface_px` truncates a finite value toward zero. `-1.0`, `320.0`, and `160.0` stay those integers. `(-1, 0)` is left of the client. `(320, 80)` is the open right edge. `(8, 160)` is the open bottom edge. Each is a miss.

```rust
let Some(control) = hits.at(dip_from_surface_px(x), dip_from_surface_px(y)) else {
    arm.button_id = None;
    return PointerEffect {
        step: Step::Unchanged,
        hit: None,
    };
};
```

That branch does not write `controller.focus`. It does not call `press_add`, `on_key`, or `set_edit`. Count, revision, edit, and result stay put. A fresh admitted controller is count 0, revision 0, edit `"1"`, result `"0"`, and focus Edit, and a miss does not move them.

A non-finite surface value becomes `0`. `(0, 0)` is inside `window.root`. `surface_px_truncates_toward_zero` already maps `f64::NAN` to `0`. NaN is not this slice. Do not use NaN as a miss sentinel.

## What the test locks

The test starts on Edit. A press at `(-1.0, 0.0)` is hit `None` and `Step::Unchanged`. Focus stays Edit. Count stays 0. Edit stays `"1"`. Result stays `"0"`.

`window.add` is `(136, 64, 80, 32)`. A press at `(176.0, 80.0)` hits that id, sets focus to Button, and does not activate. Count stays 0. `on_pointer_press` does not call `press_add`. Do not ask to activate on press.

A press at `(320.0, 80.0)` is hit `None` and `Step::Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. The miss sets `button_id` to `None`.

`on_pointer_release` takes `button_id` before it compares. `press_add` runs only when the release hit equals that id. A release at `(176.0, 80.0)` after the miss is `Step::Unchanged`. Count and revision stay 0. The test locks that step and those integers. It does not assert the release hit.

A press at `(8.0, 160.0)` is hit `None` and `Step::Unchanged`. Focus stays Button. Count stays 0.

`(8, 8)` is `window.root`. `(88, 128)` is `state.count`. Both are inside the client. `tests::result_press_does_not_move_focus` presses them. This test does not.

`PointerArm.button_id` stays private. Do not make it public. The test sees the clear only through the release.

Section 6 of `docs/emperor/GY-EMP-001.md` says: "A press outside the client rectangle is a miss. Focus stays. A later release on `window.add` does not activate." `docs/emperor/contracts/gy-cross-parity-1.md` says: "A press outside the client rectangle is a miss. Focus stays where it was. The recorded id is cleared. A later release on `window.add` does not activate." Those sentences match the miss branch.

## Not a grab

Clearing the arm on a miss stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_press` does not call a pointer-capture API. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## Not Leave

`wl_pointer` `Leave` sets `last_pointer` to `None`. It does not push `PointerRelease`. Do not emit a synthetic `PointerRelease` on pointer Leave. A later left-button release with no stored point already emits `PointerRelease(-1.0, -1.0)`. This test does not dispatch Leave.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. EMP-M1-11 stays unmet. No ledger row moves. This note does not claim a signed release, sandbox enforcement, or SOTA.

## Not IME

This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. Ignoring those events is not support. EMP-M1-09 stays unmet.

## Not a pass

`tests::outside_client_press_does_not_move_focus` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a presented buffer, a Wayland seat, or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
