# Saturated release security

`tests::saturated_release_is_a_miss` is immediately after `tests::release_truncates_to_a_miss` in `crates/emperor-runtime/src/lib.rs`. The next test is `tests::space_on_the_edit_does_not_insert`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the release path already in that file. The test does not require a `host.rs` change. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## What the release does

`dip_from_surface_px` returns `0` when `!value.is_finite()`, before truncation. A finite value uses `trunc`, toward zero, then saturates. `truncated >= f64::from(i32::MAX)` returns `i32::MAX`. `truncated <= f64::from(i32::MIN)` returns `i32::MIN`. The `as i32` cast runs only inside that range, so it is not a wrapping cast. `f64::from(i32::MAX) + 1.0` is `2147483648`. `f64::from(i32::MIN) - 1.0` is `-2147483649`. Both are finite and already integral, so truncation does not change them. The comparisons saturate them to `i32::MAX` and `i32::MIN`. Do not round. Do not wrap a huge finite value into the client.

`on_pointer_release` hit-tests with that function, then `arm.button_id.take()`. No recorded id returns `Step::Unchanged` with the hit. `press_add` runs only when the hit id equals the armed id. `press_add` is `KeyController::on_key` with `WM_KEYDOWN` and `VK_RETURN`. The release does not assign `controller.focus`. It does not call `set_edit`.

`window.root` is `(0, 0, 320, 160)`. Containment is half-open: `x >= 0`, `y >= 0`, `x < 320`, and `y < 160`, widened to `i64`. `i32::MAX` and `i32::MIN` fail that test. The hit is `None`. They do not land inside the 320 by 160 client. `window.add` is `(136, 64, 80, 32)`, half-open `136 <= x < 216` and `64 <= y < 96`. `(176, 80)` is that button.

A saturated release is not the armed id, so it does not activate. `take` still consumes the arm, so a later release at `(176, 80)` does not activate either. On the x cases the y coordinate is `80`, which is inside the button's vertical span. A wrap of that x into `[136, 216)` would be `window.add` and would call `press_add`. Saturation keeps x at the limit, outside the client, so the hit stays `None`. On the y cases the x coordinate is `80`, left of the button. The miss is the saturated y.

A 32-bit wrap of these two integers is `i32::MIN` and `i32::MAX`, which are also outside this client. Hit `None` on these two inputs does not by itself reject a wrap of some other huge finite whose low 32 bits would land inside the window. The conversion does: values outside the `i32` range never reach `as i32`, and `HitMap::at` compares the saturated limits in `i64`.

Section 6 of `docs/emperor/GY-EMP-001.md`, after the truncated-miss sentences, says: "A release that saturates at the 32-bit limits is outside the client and is a miss. It does not activate. A later release on `window.add` does not activate." The next sentence is the allowed-paste sentence. `docs/emperor/contracts/gy-cross-parity-1.md`, after those same truncated-miss sentences, says those same three sentences. The next sentence is "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale." Those sentences match this release. The function does not multiply by 250 millipoints or by buffer scale.

## Not the saturated press

`tests::finite_press_truncates_toward_zero` is the press path. It presses `f64::from(i32::MAX) + 1.0` and `f64::from(i32::MIN) - 1.0` on x. Each hit is `None`. The miss branch assigns `arm.button_id = None` and returns before writing focus. This slice is the release, which consumes the arm with `take`. The release does not write focus either. Do not merge those paths. The press test's later release is `(215.9, 95.9)`, after the arm was cleared by assignment. This test's later release is `(176, 80)`, after `take` on a saturated miss.

## Not the truncated miss

`tests::release_truncates_to_a_miss` releases fractional edges `-1.9`, `320.9`, and `160.9`, and the inside point `319.9`. Those truncations are `-1`, `320`, `160`, and `319`. They are inside the `i32` range. This slice is saturation at the 32-bit limits. Do not merge those paths. Do not round those fractional edges.

## Not the no-position release

`tests::release_without_a_position_does_not_activate` releases exact `(-1, -1)`. That hit is `None`. `(-1, -1)` is the no-position miss. `i32::MIN` is not that sentinel. Do not merge those paths.

## Not the non-finite release

`tests::non_finite_release_stays_inside_the_client` releases `f64::NAN`, `f64::INFINITY`, and `f64::NEG_INFINITY`. Each becomes `0` before truncation and hits `Some("window.root")`. That hit is not `None`. This slice's values are finite. They saturate and the hit is `None`. Do not merge those paths. Do not ask to treat NaN as a miss.

## What the test locks

The test starts on Edit. Count is 0, revision is 0, edit is `"1"`, result is `"0"`.

A release at `(f64::from(i32::MAX) + 1.0, 80.0)` with no recorded press is hit `None` and `Step::Unchanged`. Focus stays Edit. Count, revision, edit, and result stay put. That release does not activate. No later release follows it. The arm was already empty.

A press at `(176.0, 80.0)` is hit `Some("window.add")`. Focus becomes Button. Count stays 0. The press records the id and does not call `press_add`.

A release at `(f64::from(i32::MAX) + 1.0, 80.0)` is hit `None` and `Step::Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0. The test locks that step and that count. It does not assert the later hit, revision, result, or edit.

A new controller is pressed on `(176.0, 80.0)`, then released at `(f64::from(i32::MIN) - 1.0, 80.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0.

A third controller is pressed on `(176.0, 80.0)`, then released at `(80.0, f64::from(i32::MAX) + 1.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0. The x coordinate is `80`. The miss is the saturated y.

A fourth controller is pressed on `(176.0, 80.0)`, then released at `(80.0, f64::from(i32::MIN) - 1.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0.

`PointerArm.button_id` stays private. The test sees the consumed arm only through the later release. Do not make `button_id` public.

## Not a grab

Taking the arm on release stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_release` does not call a pointer-capture API. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## What it does not prove

This slice does not lock a second release after a successful truncated activation. That stays open. This note does not implement it. `tests::release_truncates_toward_zero` stores on `(215.9, 95.9)` and has no second release after that store. This test never activates, so it does not add one.

It locks hits, not the raw `i32` from `dip_from_surface_px`. Each saturated release in this test is `None`. None of them stores.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. Describing those capabilities does not install Landlock or seccomp. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet.

`tests::saturated_release_is_a_miss` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a presented buffer, a Wayland seat, or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
