# Non-finite press security

`tests::non_finite_press_stays_inside_the_client` follows `tests::outside_client_press_does_not_move_focus` in `crates/emperor-runtime/src/lib.rs`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the non-finite path already in that file. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## Conversion

`dip_from_surface_px` returns `0` when `!value.is_finite()`. `f64::NAN`, `f64::INFINITY`, and `f64::NEG_INFINITY` all take that path, before truncation. A finite value truncates toward zero, then saturates at `i32::MIN` or `i32::MAX`. This slice does not use a huge finite value. Saturation is not this press.

`window.root` is `(0, 0, 320, 160)`. `(0, 0)` is inside it. `window.amount` starts at `(8, 64)`. `(0, 80)` and `(80, 0)` miss the edit, `window.add`, and `state.count`. The walk returns `window.root`. That hit is not `None`. A non-finite point is not reported as outside the window.

The root role is `client`. It is not `button` or `edit`, and the id is not `window.add` or `window.amount`. The else branch clears `button_id`, does not write `controller.focus`, and returns `Step::Unchanged` with `hit: Some("window.root")`. It does not call `press_add`, `on_key`, or `set_edit`. The press does not activate.

Section 6 of `docs/emperor/GY-EMP-001.md` says: "A non-finite pointer coordinate becomes 0. The press is inside the client and is not a miss." `docs/emperor/contracts/gy-cross-parity-1.md` says: "A non-finite coordinate becomes 0 before that truncation. The press is inside the client and is not a miss." Those sentences match this conversion. Do not treat NaN as outside. Do not use NaN as a miss sentinel.

## What the test locks

The test starts on Edit. Count is 0, revision is 0, edit is `"1"`, result is `"0"`.

A press at `(f64::NAN, f64::NAN)` is hit `Some("window.root")` and `Step::Unchanged`. The hit is `Some`. Focus stays Edit. Count, revision, edit, and result stay put.

A press at `(176.0, 80.0)` hits `window.add`, sets focus to Button, and does not activate. Count stays 0. That press records the id. `on_pointer_press` does not call `press_add`.

A press at `(f64::INFINITY, 80.0)` is hit `Some("window.root")` and `Step::Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. The root press clears the recorded id.

`on_pointer_release` takes `button_id` before it compares. `press_add` runs only when the release hit equals that id. A release at `(176.0, 80.0)` after the root press is `Step::Unchanged`. Count and revision stay 0. The test locks that step and those integers. It does not assert the release hit.

A press at `(80.0, f64::NEG_INFINITY)` is hit `Some("window.root")` and `Step::Unchanged`. Focus stays Button. Count stays 0. That press does not re-assert revision, edit, or result.

`PointerArm.button_id` stays private. The test sees the clear only through the release. Do not make `button_id` public.

## Not a grab

Clearing the arm on `window.root` stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_press` does not call a pointer-capture API. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## What it does not prove

The test does not feed a huge finite value, so it does not lock saturation at `i32::MIN` or `i32::MAX`. It does not show that a seat delivered a non-finite coordinate. `surface_px_truncates_toward_zero` already maps `f64::NAN` to `0` and does not assert the infinities. The press test is the lock for those three values as client hits, not as a compositor event.

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet.

`tests::non_finite_press_stays_inside_the_client` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a seat or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
