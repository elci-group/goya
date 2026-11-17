# Non-finite release security

`tests::non_finite_release_stays_inside_the_client` follows `tests::release_truncates_toward_zero` in `crates/emperor-runtime/src/lib.rs`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the release path already in that file. The test does not require a `host.rs` change. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## What the release does

`dip_from_surface_px` returns `0` when `!value.is_finite()`, before truncation. `f64::NAN`, `f64::INFINITY`, and `f64::NEG_INFINITY` all take that path. A finite value truncates toward zero, then saturates at `i32::MIN` or `i32::MAX`. This slice does not use a huge finite value.

`on_pointer_release` hit-tests with that function, then `arm.button_id.take()`. No recorded id returns `Step::Unchanged` with the hit. `press_add` runs only when the hit id equals the armed id. `press_add` is `KeyController::on_key` with `WM_KEYDOWN` and `VK_RETURN`. The release does not assign `controller.focus`. It does not call `set_edit`.

A non-finite axis becomes 0. `(f64::NAN, f64::NAN)` is `(0, 0)`. `(f64::INFINITY, 80.0)` is `(0, 80)`. `(80.0, f64::NEG_INFINITY)` is `(80, 0)`. `window.root` is `(0, 0, 320, 160)`. All three points are inside it. `window.amount` starts at `(8, 64)`. Those points miss the edit, `window.add`, and `state.count`. The hit is `Some("window.root")`, not `None`. That is not a miss. It does not activate.

`window.add` is `(136, 64, 80, 32)`. Containment is half-open: `136 <= x < 216` and `64 <= y < 96`. `(176, 80)` is that button. A root hit is not the armed id, so `press_add` does not run. The `take` has already consumed the arm, so a later release at `(176, 80)` does not activate either. Do not activate on the non-finite release. Do not treat NaN as outside. Do not use NaN as a miss sentinel. `(-1, -1)` is the no-position miss. `tests::release_without_a_position_does_not_activate` already releases there. That hit is `None`. This hit is `Some("window.root")`.

Section 6 of `docs/emperor/GY-EMP-001.md`, after "A release that truncates off `window.add` does not.", says: "A non-finite release coordinate becomes 0. The release is inside the client and is not a miss. It does not activate. A later release on `window.add` does not activate." The next sentence is the allowed-paste sentence. `docs/emperor/contracts/gy-cross-parity-1.md`, after that same lead-in and before the 250 millipoints sentence, says: "A non-finite release coordinate becomes 0 before truncation. The release is inside the client and is not a miss. It does not activate. A later release on `window.add` does not activate." Those sentences match this release.

## Not the non-finite press

`tests::non_finite_press_stays_inside_the_client` presses `window.root`. That press assigns `arm.button_id = None` and returns before writing focus. This release `take`s the arm and also does not write focus. Both leave a later button release unable to call `press_add`. Do not merge those paths. The press test's later `(176.0, 80.0)` release follows a root press. This test's later release follows a root release that consumed an armed id.

## What the test locks

The test starts on Edit. Count is 0, revision is 0, edit is `"1"`, result is `"0"`.

A release at `(f64::NAN, f64::NAN)` with no recorded press is hit `Some("window.root")`, not `None`, and `Step::Unchanged`. Focus stays Edit. Count, revision, edit, and result stay put. That release does not activate.

A press at `(176.0, 80.0)` is hit `Some("window.add")`. Focus becomes Button. Count stays 0. The press records the id and does not call `press_add`.

A release at `(f64::NAN, f64::NAN)` is hit `Some("window.root")`, not `None`, and not `window.add`. The step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0. The test locks that step and that count. It does not assert the later hit, revision, result, or edit.

A new controller is pressed on `(176.0, 80.0)`, then released at `(f64::INFINITY, 80.0)`. The hit is `Some("window.root")` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0.

A third controller is pressed on `(176.0, 80.0)`, then released at `(80.0, f64::NEG_INFINITY)`. The hit is `Some("window.root")` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0.

`PointerArm.button_id` stays private. The test sees the consumed arm only through the later release. Do not make `button_id` public.

## Not a grab

Taking the arm on release stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_release` does not call a pointer-capture API. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## What it does not prove

The test does not lock a truncated miss. It does not lock saturation at `i32::MIN` or `i32::MAX` on release. It does not lock a second release after a successful truncated activation. Those stay open. This note does not implement them.

The non-finite releases are inside the client. None of them stores. `tests::release_truncates_toward_zero` stores on `(215.9, 95.9)`, and that storing release has no second release after it. This test does not add one.

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. Describing those capabilities does not install Landlock or seccomp. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet.

`tests::non_finite_release_stays_inside_the_client` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a presented buffer, a Wayland seat, or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
