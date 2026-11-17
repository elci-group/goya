# Truncated release miss security

`tests::release_truncates_to_a_miss` is immediately after `tests::non_finite_release_stays_inside_the_client` in `crates/emperor-runtime/src/lib.rs`. The next test is `tests::space_on_the_edit_does_not_insert`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the release path already in that file. The test does not require a `host.rs` change. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## What the release does

`dip_from_surface_px` returns `0` when `!value.is_finite()`, before truncation. A finite value uses `trunc`, toward zero, then saturates. `truncated >= f64::from(i32::MAX)` returns `i32::MAX`. `truncated <= f64::from(i32::MIN)` returns `i32::MIN`. The `as i32` cast runs only inside that range. `(-1.9f64).trunc()` is `-1`, not `-2`. `(320.9f64).trunc()` is `320`. `(160.9f64).trunc()` is `160`. `(319.9f64).trunc()` is `319`. `(159.9f64).trunc()` is `159`. Do not round to nearest. Do not use `floor`. Floor of `-1.9` is `-2`. Nearest of `319.9` is `320`, and nearest of `159.9` is `160`.

`on_pointer_release` hit-tests with that function, then `arm.button_id.take()`. No recorded id returns `Step::Unchanged` with the hit. `press_add` runs only when the hit id equals the armed id. `press_add` is `KeyController::on_key` with `WM_KEYDOWN` and `VK_RETURN`. The release does not assign `controller.focus`. It does not call `set_edit`.

`window.root` is `(0, 0, 320, 160)`. Containment is half-open: `x >= 0`, `y >= 0`, `x < 320`, and `y < 160`, in 64-bit arithmetic. `(-1, 80)`, `(320, 80)`, and `(80, 160)` are misses. The hit is `None`. `(319, 159)` is inside the client. The hit is `Some("window.root")`, not `None` and not `window.add`. `window.add` is `(136, 64, 80, 32)`, half-open `136 <= x < 216` and `64 <= y < 96`. `(176, 80)` is that button.

A miss is not the armed id, so the release does not activate. `take` still consumes the arm, so a later release at `(176, 80)` does not activate either. `(319.9, 159.9)` truncates to `(319, 159)`, which is `window.root`. That release does not activate, and it also consumes the arm. Do not round `319.9` up to `320`. That would turn an inside point into a miss. Do not round `-1.9` away from zero.

Section 6 of `docs/emperor/GY-EMP-001.md`, after the non-finite release sentences, says: "A finite release that truncates outside the client is a miss. It does not activate. A release that truncates onto the client stays inside. A later release on `window.add` does not activate." The next sentence is the allowed-paste sentence. `docs/emperor/contracts/gy-cross-parity-1.md`, after those same non-finite release sentences, says those same four sentences. The next sentence is "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale." Those sentences match this release. The function does not multiply by 250 millipoints or by buffer scale.

## Not the outside press

`tests::outside_client_press_does_not_move_focus` presses exact `(-1.0, 0.0)`, `(320.0, 80.0)`, and `(8.0, 160.0)`. Each hit is `None`. The press assigns `arm.button_id = None` and returns before writing focus. This release `take`s the arm after the hit test. It does not write focus either. Do not merge those paths. The press test clears the arm on the press. This test consumes the arm on a truncated release. The press points are exact integers. This release uses `-1.9`, `320.9`, and `160.9`. The press bottom edge is `(8, 160)`, not `(80, 160)`.

## Not the no-position release

`tests::release_without_a_position_does_not_activate` releases exact `(-1.0, -1.0)`. That hit is `None`. `(-1, -1)` is the no-position miss. This slice's `(-1.9, 80.0)` truncates to `(-1, 80)`, which is also `None`, but it is not that sentinel. Do not merge those paths. Do not ask to treat NaN as a miss.

## Not the non-finite release

`tests::non_finite_release_stays_inside_the_client` releases `f64::NAN`, `f64::INFINITY`, and `f64::NEG_INFINITY`. Each becomes `0` before truncation and hits `Some("window.root")`. That hit is not `None`. This slice's outside points are misses. `(319.9, 159.9)` is also `Some("window.root")`, because truncation lands inside the client, not because a non-finite value became `0`. Do not merge those paths.

## Not the in-client truncated release

`tests::release_truncates_toward_zero` releases `(215.9, 95.9)`. That truncates onto `window.add` and stores. `(135.9, 80)` truncates onto `window.root` and does not store. Those points stay inside the client. This slice's outside truncations are `None`. The inside corner `(319.9, 159.9)` is `window.root` at the far edge of the client, not the button edge. Do not merge those paths. The storing release in that test has no second release after it. This test does not add one.

## What the test locks

The test starts on Edit. Count is 0, revision is 0, edit is `"1"`, result is `"0"`.

A release at `(-1.9, 80.0)` with no recorded press is hit `None` and `Step::Unchanged`. Focus stays Edit. Count, revision, edit, and result stay put. That release does not activate.

A press at `(176.0, 80.0)` is hit `Some("window.add")`. Focus becomes Button. Count stays 0. The press records the id and does not call `press_add`.

A release at `(320.9, 80.0)` is hit `None` and `Step::Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0. The test locks that step and that count. It does not assert the later hit, revision, result, or edit.

A new controller is pressed on `(176.0, 80.0)`, then released at `(80.0, 160.9)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0.

A third controller is pressed on `(176.0, 80.0)`, then released at `(-1.9, 80.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0.

A fourth controller is pressed on `(176.0, 80.0)`, then released at `(319.9, 159.9)`. The hit is `Some("window.root")`, not `None`, and not `window.add`. The step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0.

`PointerArm.button_id` stays private. The test sees the consumed arm only through the later release. Do not make `button_id` public.

## Not a grab

Taking the arm on release stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_release` does not call a pointer-capture API. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## What it does not prove

The test does not lock saturation at `i32::MIN` or `i32::MAX` on release. It does not lock a second release after a successful truncated activation. Those stay open. This note does not implement them.

It locks hits, not the raw `i32` from `dip_from_surface_px`. `(-1.9, 80.0)`, `(320.9, 80.0)`, and `(80.0, 160.9)` are `None`. `(319.9, 159.9)` is `window.root`. None of those releases stores.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. Describing those capabilities does not install Landlock or seccomp. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet.

`tests::release_truncates_to_a_miss` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a presented buffer, a Wayland seat, or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
