# Release miss parity

Sash posts activation on release inside the control. Emperor activates only when the release hit id equals the id recorded on press. A finite surface coordinate truncates toward zero. It does not round to nearest. A value that truncates outside the client is a miss. A value that truncates onto the client stays inside. This note compares that rule with `dip_from_surface_px` and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. `tests::release_truncates_to_a_miss` covers that release. Sash has not been launched. This is not a Win32 pointer message and not `BN_CLICKED`.

EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `dip_from_surface_px`. It does not ask to activate on a miss or on `window.root`.

## Shared hit rule

`dip_from_surface_px` returns 0 when `!value.is_finite()`, before truncation toward zero. A finite value uses `trunc`, toward zero, then saturates at `i32::MAX` and `i32::MIN`. `(-1.9f64).trunc()` is `-1`, not `-2`. `(320.9f64).trunc()` is `320`. `(160.9f64).trunc()` is `160`. `(319.9f64).trunc()` is `319`. `(159.9f64).trunc()` is `159`. Those five values are finite and inside the `i32` range, so the cast is that truncation. The function does not round to nearest. It does not multiply by 250 millipoints or by buffer scale.

`HitMap::at` contains a point when `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, using 64-bit arithmetic. Containment is half-open. `window.root` is `(0, 0, 320, 160)`. `(-1, 80)`, `(320, 80)`, and `(80, 160)` are misses. The hit is `None`. `(319, 159)` is inside that rectangle and is not `window.add`. The hit is `Some("window.root")`. `(176, 80)` is `window.add`. Nearest would send `(319.9, 159.9)` to `(320, 160)`, which is outside the half-open client. Truncation keeps `(319, 159)` inside.

`on_pointer_release` passes both surface coordinates through `dip_from_surface_px`, then asks the hit map. It takes `arm.button_id`. `press_add` runs only when the hit id equals that armed id. A miss is not that id, so it does not activate. The arm is consumed, so a later release on `window.add` does not activate. The inside truncated point is `window.root`, not the armed id, so it does not activate either. The function does not write focus. This slice does not lock `i32::MIN` or `i32::MAX` on release.

## What the later read shows

`tests::release_truncates_to_a_miss` is in `crates/emperor-runtime/src/lib.rs` immediately after `tests::non_finite_release_stays_inside_the_client`. The next test is `tests::space_on_the_edit_does_not_insert`. It starts on Edit, edit `"1"`, count 0, revision 0, result `"0"`.

An unarmed release of `(-1.9, 80.0)` hits `None`. There is no recorded id, so `press_add` does not run. The step is `Unchanged`. Focus stays Edit. Count, revision, edit, and result stay put. A press at `(176.0, 80.0)` hits `window.add` and sets focus to Button. Count stays 0. A release of `(320.9, 80.0)` hits `None`. The step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0.

A new controller is pressed on `(176.0, 80.0)`, then released at `(80.0, 160.9)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0. A third controller is pressed on `(176.0, 80.0)`, then released at `(-1.9, 80.0)`. The hit is `None` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0.

A fourth controller is pressed on `(176.0, 80.0)`, then released at `(319.9, 159.9)`. The hit is `Some("window.root")`, not `None` and not `window.add`. The step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0.

The test does not read `button_id`. Each failed later release is the proof that the release consumed the record. The later release locks that step and that count. It does not assert the later hit. The test locks those hits. It does not assert the raw `i32` from `dip_from_surface_px`. No release in this test stores.

## Wording

`docs/emperor/GY-EMP-001.md` section 6, after "A non-finite release coordinate becomes 0. The release is inside the client and is not a miss. It does not activate. A later release on `window.add` does not activate.", says: "A finite release that truncates outside the client is a miss. It does not activate. A release that truncates onto the client stays inside. A later release on `window.add` does not activate." The next sentence is "An allowed paste payload still replaces the edit after focus has moved."

`docs/emperor/contracts/gy-cross-parity-1.md`, after "A non-finite release coordinate becomes 0 before truncation. The release is inside the client and is not a miss. It does not activate. A later release on `window.add` does not activate.", says those same four sentences. The next sentence is "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale."

## Not the outside-client press

`tests::outside_client_press_does_not_move_focus` presses exact `(-1.0, 0.0)`, `(320.0, 80.0)`, and `(8.0, 160.0)`. Each hit is `None`. Focus stays where it was. The miss branch of `on_pointer_press` clears `button_id` by assignment. A later release on `window.add` does not activate. That slice is the press. This slice is the release. `on_pointer_release` consumes the arm with `take` and does not write focus. The coordinates here are fractional: `(-1.9, 80.0)`, `(320.9, 80.0)`, and `(80.0, 160.9)`.

## Not the positionless release

`tests::release_without_a_position_does_not_activate` presses `window.add` and releases exact `(-1.0, -1.0)`. That point truncates to `(-1, -1)` and is a miss. This slice does not use that sentinel. `(-1.9, 80.0)` truncates to `(-1, 80)`. `(320.9, 80.0)` truncates to `(320, 80)`. `(80.0, 160.9)` truncates to `(80, 160)`. Those misses are the truncated client edges. The test asserts the hit is `None`.

## Not the non-finite release

`tests::non_finite_release_stays_inside_the_client` releases `(f64::NAN, f64::NAN)`, `(f64::INFINITY, 80.0)`, and `(80.0, f64::NEG_INFINITY)`. Each becomes 0 before truncation and hits `Some("window.root")`. The hit is not `None`. A non-finite release is inside the client. It is not a miss. This slice is finite. `(-1.9, 80.0)`, `(320.9, 80.0)`, and `(80.0, 160.9)` hit `None`. `(319.9, 159.9)` hits `Some("window.root")` because truncation lands inside the client, not because a non-finite value became 0. Neither release activates.

## Not the release onto and off the button

`tests::release_truncates_toward_zero` releases finite `(215.9, 95.9)`. That truncates onto `window.add` and stores count 1, revision 1, result `"1"` when armed. `(135.9, 80.0)` and `(176.0, 63.9)` truncate onto `window.root`, still inside the client, and do not store. Those points are not the open client edge. This slice does not store. Its outside releases are misses. `(319.9, 159.9)` stays inside and does not activate.

## Not the finite press

`tests::finite_press_truncates_toward_zero` presses `(215.9, 95.9)`, `(-0.9, 80.0)`, `(f64::from(i32::MAX) + 1.0, 80.0)`, and `(f64::from(i32::MIN) - 1.0, 80.0)`. `(-0.9, 80)` truncates to `(0, 80)`, which is `window.root`, not the `x = -1` miss. Saturation at either 32-bit limit is a miss on the press path and clears the arm. The release of `(215.9, 95.9)` in that test comes after the saturated press, so it stays `Unchanged`. This slice does not lock `i32::MIN` or `i32::MAX` on release. `(-1.9, 80)` truncates outside. It is not the `(-0.9, 80)` press.

## Still unlaunched

Sash has not been launched. Emperor does not receive activation from a launched window. This is not a Win32 pointer message and not `BN_CLICKED`. A host unit test is not a launched window, not a seat, and not a compositor launch. It is not a Windows launch. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met.
