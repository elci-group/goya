# Non-finite release parity

Sash posts activation on release inside the control. Emperor activates only when the release hit id equals the id recorded on press. A non-finite surface coordinate becomes 0 before truncation toward zero. That 0 is inside `window.root`. It is not a miss. This note compares that rule with `dip_from_surface_px` and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. `tests::non_finite_release_stays_inside_the_client` covers that release. Sash has not been launched. This is not a Win32 pointer message and not `BN_CLICKED`.

EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `dip_from_surface_px`. It does not ask to activate on a root release.

## Shared hit rule

`dip_from_surface_px` returns 0 when `!value.is_finite()`, before truncation toward zero. A finite value truncates toward zero into an `i32` DIP. It does not multiply by 250 millipoints or by buffer scale. `HitMap::at` contains a point when `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, using 64-bit arithmetic. Containment is half-open. `window.root` is `(0, 0, 320, 160)`. `(0, 0)`, `(0, 80)`, and `(80, 0)` are inside that rectangle. `(0, 80)` and `(80, 0)` miss the edit, `window.add`, and `state.count`. The walk returns `window.root`. The hit is `Some("window.root")`, not `None`. `(176, 80)` is `window.add`.

`on_pointer_release` passes both surface coordinates through `dip_from_surface_px`, then asks the hit map. It takes `arm.button_id`. `press_add` runs only when the hit id equals that armed id. A different hit, including `window.root`, returns `Step::Unchanged`. The function does not write focus. The arm is consumed, so a later release on `window.add` does not activate. A non-finite release is inside the client. It is not a miss. It does not activate.

## What the later read shows

`tests::non_finite_release_stays_inside_the_client` is in `crates/emperor-runtime/src/lib.rs` immediately after `tests::release_truncates_toward_zero`. The next test is `tests::space_on_the_edit_does_not_insert`. It starts on Edit, edit `"1"`, count 0, revision 0, result `"0"`.

An unarmed release of `(f64::NAN, f64::NAN)` hits `Some("window.root")`. The hit is not `None`. The step is `Unchanged`. Focus stays Edit. Count, revision, edit, and result stay put. A press at `(176.0, 80.0)` hits `window.add` and sets focus to Button. Count stays 0. A release of `(f64::NAN, f64::NAN)` hits `Some("window.root")`, not `None` and not `window.add`. The step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0.

A new controller is pressed on `(176.0, 80.0)`, then released at `(f64::INFINITY, 80.0)`. The hit is `Some("window.root")` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0. A third controller is pressed on `(176.0, 80.0)`, then released at `(80.0, f64::NEG_INFINITY)`. The hit is `Some("window.root")` and the step is `Unchanged`. Focus stays Button. Count, revision, edit, and result stay put. A later release of `(176.0, 80.0)` is `Unchanged` and count stays 0.

The test does not read `button_id`. Each failed later release is the proof that the non-finite release consumed the record. The later release locks that step and that count. It does not assert the later hit.

## Wording

`docs/emperor/GY-EMP-001.md` section 6, after "A release that truncates off `window.add` does not.", says: "A non-finite release coordinate becomes 0. The release is inside the client and is not a miss. It does not activate. A later release on `window.add` does not activate." The next sentence is "An allowed paste payload still replaces the edit after focus has moved."

`docs/emperor/contracts/gy-cross-parity-1.md`, before "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale.", says: "A non-finite release coordinate becomes 0 before truncation. The release is inside the client and is not a miss. It does not activate. A later release on `window.add` does not activate."

## Not the non-finite press

`tests::non_finite_press_stays_inside_the_client` presses `(f64::NAN, f64::NAN)`, `(f64::INFINITY, 80.0)`, and `(80.0, f64::NEG_INFINITY)`. Each hits `window.root`. The infinity press clears the arm by assignment. A later release on `window.add` does not activate. That clear is the press path. This slice is the release. `on_pointer_release` consumes the arm with `take`, not by assigning `None` on a root hit. Release does not write focus. The press test does not lock a non-finite release.

## Not the truncated release

`tests::release_truncates_toward_zero` releases finite `(215.9, 95.9)`. That truncates onto `window.add` and stores count 1. `(135.9, 80)` and `(176, 63.9)` truncate onto `window.root`. Those values are finite. This slice is NaN, +infinity, and -infinity. It does not store.

## Not the positionless release

`tests::release_without_a_position_does_not_activate` releases `(-1, -1)`. That is a miss. The hit is `None`. A non-finite release does not take that branch. NaN becomes 0 before truncation, so `(f64::NAN, f64::NAN)` is not the `(-1, -1)` miss. Do not use NaN as that miss.

## Still unlaunched

Sash has not been launched. Emperor does not receive activation from a launched window. This is not a Win32 pointer message and not `BN_CLICKED`. A host unit test is not a launched window, not a seat, and not a compositor launch. It is not a Windows launch. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met.
