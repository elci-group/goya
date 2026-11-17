# Saturated release parity

Sash posts activation on release inside the control. Emperor activates only when the release hit id equals the id recorded on press. A finite surface coordinate truncates toward zero, then saturates at `i32::MAX` and `i32::MIN`. It does not round. It does not wrap. A saturated release is outside the client and is a miss. This note compares that rule with `dip_from_surface_px` and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. `tests::saturated_release_is_a_miss` covers that release. Sash has not been launched. This is not a Win32 pointer message and not `BN_CLICKED`.

EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `dip_from_surface_px`. It does not ask to activate on a saturated miss.

## Shared hit rule

`dip_from_surface_px` returns 0 when `!value.is_finite()`, before truncation toward zero. A finite value uses `trunc`, toward zero, then saturates. `truncated >= f64::from(i32::MAX)` returns `i32::MAX`. `truncated <= f64::from(i32::MIN)` returns `i32::MIN`. The `as i32` cast runs only when the truncated value is strictly inside that range, so it is not a wrapping cast. `f64::from(i32::MAX) + 1.0` is `2147483648` and becomes `i32::MAX`. `f64::from(i32::MIN) - 1.0` is `-2147483649` and becomes `i32::MIN`. The function does not wrap those values into the client. It does not round to nearest. It does not multiply by 250 millipoints or by buffer scale.

`HitMap::at` contains a point when `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, using 64-bit arithmetic. Containment is half-open. `window.root` is `(0, 0, 320, 160)`. `i32::MAX` and `i32::MIN` fail that test on either axis. Both saturated points are outside the client. The hit is `None`. `(176, 80)` is `window.add`.

`on_pointer_release` passes both surface coordinates through `dip_from_surface_px`, then asks the hit map. It takes `arm.button_id`. `press_add` runs only when the hit id equals that armed id. A saturated release is not that id, so it does not activate. The arm is consumed, so a later release on `window.add` does not activate. The function does not write focus.

## What the later read shows

`tests::saturated_release_is_a_miss` is in `crates/emperor-runtime/src/lib.rs` immediately after `tests::release_truncates_to_a_miss`. The next test is `tests::space_on_the_edit_does_not_insert`. It starts on Edit, edit `"1"`, count 0, revision 0, result `"0"`.

An unarmed release of `(f64::from(i32::MAX) + 1.0, 80.0)` hits `None`. There is no recorded id, so `press_add` does not run. The step is `Unchanged`. Focus stays Edit. Count, revision, edit, and result stay put. A press at `(176.0, 80.0)` hits `window.add` and sets focus to Button. Count stays 0. A release of `(f64::from(i32::MAX) + 1.0, 80.0)` hits `None`. The step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0.

A new controller is pressed on `(176.0, 80.0)`, then released at `(f64::from(i32::MIN) - 1.0, 80.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0. A third controller is pressed on `(176.0, 80.0)`, then released at `(80.0, f64::from(i32::MAX) + 1.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0. A fourth controller is pressed on `(176.0, 80.0)`, then released at `(80.0, f64::from(i32::MIN) - 1.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0.

The test does not read `button_id`. Each failed later release is the proof that the release consumed the record. The later release locks that step and that count. It does not assert the later hit. The test locks those hits as `None`. It does not assert the raw `i32` from `dip_from_surface_px`. No release in this test stores. Release does not write focus.

## Wording

`docs/emperor/GY-EMP-001.md` section 6, after "A finite release that truncates outside the client is a miss. It does not activate. A release that truncates onto the client stays inside. A later release on `window.add` does not activate.", says: "A release that saturates at the 32-bit limits is outside the client and is a miss. It does not activate. A later release on `window.add` does not activate." The next sentence is "An allowed paste payload still replaces the edit after focus has moved."

`docs/emperor/contracts/gy-cross-parity-1.md`, after those same four sentences, says those same three sentences. The next sentence is "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale."

## Not the finite press

`tests::finite_press_truncates_toward_zero` presses `(f64::from(i32::MAX) + 1.0, 80.0)` and `(f64::from(i32::MIN) - 1.0, 80.0)`. Each hit is `None`. That slice is the saturated press, not the release. The press miss clears the arm by assignment (`arm.button_id = None`). The release of `(215.9, 95.9)` in that test comes after the saturated press, so it stays `Unchanged`. This slice is the release. `on_pointer_release` consumes the arm with `take` and does not write focus.

## Not the truncated client edge

`tests::release_truncates_to_a_miss` releases fractional client edges that are still inside the `i32` range. `(-1.9, 80.0)` truncates to `(-1, 80)`. `(320.9, 80.0)` truncates to `(320, 80)`. `(80.0, 160.9)` truncates to `(80, 160)`. `(319.9, 159.9)` truncates to `(319, 159)` and stays inside as `window.root`. This slice is saturation. `f64::from(i32::MAX) + 1.0` becomes `i32::MAX`. `f64::from(i32::MIN) - 1.0` becomes `i32::MIN`. Both are outside `window.root` `(0, 0, 320, 160)`. The hit is `None`. It does not wrap them into the client. It does not round.

## Not the release onto the button

`tests::release_truncates_toward_zero` releases finite `(215.9, 95.9)`. That truncates onto `window.add` and stores count 1, revision 1, result `"1"` when armed. This slice does not store. A second release after a successful truncated activation stays open. Do not treat this slice as that lock. Its later releases follow a miss, not a store.

## Not the non-finite release

`tests::non_finite_release_stays_inside_the_client` releases `(f64::NAN, f64::NAN)`, `(f64::INFINITY, 80.0)`, and `(80.0, f64::NEG_INFINITY)`. Each becomes 0 before truncation and hits `Some("window.root")`. The hit is not `None`. A non-finite release is inside the client. This slice is finite. Saturation at either 32-bit limit is a miss.

## Not the positionless release

`tests::release_without_a_position_does_not_activate` presses `window.add` and releases exact `(-1.0, -1.0)`. That point truncates to `(-1, -1)` and is a miss. This slice does not use that sentinel. The saturated limits are not `(-1, -1)`.

## Still unlaunched

Sash has not been launched. Emperor does not receive activation from a launched window. Sash posts activation on release inside the control. Emperor activates only on an equal armed id. This is not a Win32 pointer message and not `BN_CLICKED`. A host unit test is not a launched window, not a seat, and not a compositor launch. It is not a Windows launch. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met. This note does not claim SOTA.
