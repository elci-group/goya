# Finite press security

`tests::finite_press_truncates_toward_zero` follows `tests::non_finite_press_stays_inside_the_client` in `crates/emperor-runtime/src/lib.rs`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the finite truncation path already in that file. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## Conversion

`dip_from_surface_px` returns `0` when `!value.is_finite()`, before truncation. A finite value uses `trunc` (toward zero), then saturates. `truncated >= f64::from(i32::MAX)` returns `i32::MAX`. `truncated <= f64::from(i32::MIN)` returns `i32::MIN`. The `as i32` cast runs only when the truncated value is strictly inside that range, so it is not a wrapping cast. `i32::MAX` and `i32::MIN` are not reduced to their low 32 bits.

`HitMap::at` rejects a point outside the client before it walks children. The client is `window.root` at `(0, 0, 320, 160)`. Containment widens every edge to `i64` and uses `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`. `i32::MAX` and `i32::MIN` fail that test. They do not wrap into the 320 by 160 client. The miss branch sets `button_id` to `None`, does not write `controller.focus`, and returns `Step::Unchanged` with `hit: None`. It does not call `press_add`, `on_key`, or `set_edit`.

`(-0.9f64).trunc()` is `-0.0`. That value is finite and is not `<= i32::MIN`, so the cast is `0`. `(0, 80)` is inside `window.root`. `window.amount` starts at x `8`, `window.add` is `(136, 64, 80, 32)`, and `state.count` starts at y `112`. The walk returns `window.root`. That hit is not `None`. The root role is `client`. The else branch clears `button_id`, does not write focus, and returns `Step::Unchanged`. It does not activate.

`(215.9, 95.9)` truncates to `(215, 95)`. The button is half-open on `[136, 216)` by `[64, 96)`, so that point hits `window.add`. The press sets focus to Button and records the id. It does not call `press_add`. Round-to-nearest would make `(216, 96)`, which is not the button. It is still inside the client, so the hit would be `window.root`. Do not replace `trunc` with round-to-nearest. `215.9` stays on the button. Do not treat `-0.9` as outside. Do not wrap a huge finite value into the client. This slice does not use NaN. A non-finite value still returns `0`.

Section 6 of `docs/emperor/GY-EMP-001.md` says: "A finite coordinate truncates toward zero. Saturation at the 32-bit limits is outside the client and is a miss." `docs/emperor/contracts/gy-cross-parity-1.md` says the same two sentences. Those sentences match this conversion.

## What the test locks

The test starts on Edit. Count is 0, revision is 0, edit is `"1"`, result is `"0"`.

A press at `(215.9, 95.9)` is hit `Some("window.add")` and `Step::Changed`. Focus becomes Button. Count stays 0. `Changed` is the focus move. The press does not activate.

A press at `(-0.9, 80.0)` is hit `Some("window.root")` and `Step::Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. The hit is not `None`. That root press clears the recorded id.

A press at `(f64::from(i32::MAX) + 1.0, 80.0)` is hit `None` and `Step::Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. The y coordinate is `80`. The miss is the saturated x. The miss branch clears `button_id` again.

`on_pointer_release` takes `button_id` before it compares. `press_add` runs only when the release hit equals that id. A release at `(215.9, 95.9)` is `Step::Unchanged`. Count and revision stay 0. That release truncates onto the button, so a still-armed `window.add` would activate. The test locks that step and those integers. It does not assert the release hit.

A press at `(f64::from(i32::MIN) - 1.0, 80.0)` is hit `None` and `Step::Unchanged`. Focus stays Button. Count stays 0. That press does not re-assert revision, edit, or result, and no release follows it.

`PointerArm.button_id` stays private. The test sees the clear only through the release. Do not make `button_id` public.

## Not a grab

Clearing the arm on `window.root`, or on a miss, stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_press` does not call a pointer-capture API. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## What it does not prove

The release follows the root press at `(-0.9, 80.0)`, which already cleared the arm, and then the `i32::MAX` miss. The unchanged release shows the arm was clear. It does not show that the saturated press was the press that cleared a live arm. The `i32::MIN` press has no later release.

`f64::from(i32::MAX) + 1.0` is `2147483648` and `f64::from(i32::MIN) - 1.0` is `-2147483649`. Both saturate to the limits and miss. A 32-bit modular reduction of those same two integers is `i32::MIN` and `i32::MAX`, which are also outside this client. Hit `None` on these two inputs does not by itself reject a wrap of some other huge finite whose low 32 bits would land inside the window. The conversion does: values outside the `i32` range never reach `as i32`, and `HitMap::at` compares the saturated limits in `i64`.

`surface_px_truncates_toward_zero` maps `1.9` to `1`, `-1.9` to `-1`, `8.0` to `8`, and `f64::NAN` to `0`. It does not assert `215.9`, `-0.9`, or either limit. The press test locks those values as hits, not as the raw `i32` from `dip_from_surface_px`. It does not feed NaN.

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet.

`tests::finite_press_truncates_toward_zero` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a seat or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
