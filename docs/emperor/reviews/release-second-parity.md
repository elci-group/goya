# Second truncated release parity

Sash posts activation on release inside the control. Emperor activates only when the release hit id equals the id recorded on press. `on_pointer_release` takes `arm.button_id` before the compare. One release consumes the record. A second release on the same id does not call `press_add`. `(215.9, 95.9)` truncates toward zero to `(215, 95)`, which is inside `window.add` `(136, 64, 80, 32)`. `(176, 80)` is also `window.add`. It does not round `215.9` to `216`. This note compares that second release with `dip_from_surface_px` and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. `tests::second_truncated_release_does_not_store` covers it. Sash has not been launched. This is not a Win32 pointer message and not `BN_CLICKED`.

EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `dip_from_surface_px`. It does not ask the second release to activate. Consuming the arm is not a pointer grab.

## Shared hit rule

`dip_from_surface_px` returns 0 when `!value.is_finite()`, before truncation toward zero. A finite value uses `trunc`, toward zero, then saturates at `i32::MAX` and `i32::MIN`. `(215.9f64).trunc()` is `215`, not `216`. `(95.9f64).trunc()` is `95`, not `96`. Both values are finite and inside the `i32` range, so the cast is that truncation. The function does not round to nearest. It does not multiply by 250 millipoints or by buffer scale.

`HitMap::at` contains a point when `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, using 64-bit arithmetic. Containment is half-open. `window.add` is `(136, 64, 80, 32)`, so the button is `x >= 136`, `y >= 64`, `x < 216`, and `y < 96`. `(215, 95)` is inside. `(176, 80)` is inside. Nearest would send `(215.9, 95.9)` to `(216, 96)`, which fails both open edges. Truncation keeps `(215, 95)` on `window.add`. The walk is front to back, last control to first, so the hit is `window.add`, not `window.root`.

`on_pointer_release` passes both surface coordinates through `dip_from_surface_px`, then asks the hit map. It then runs `let armed = arm.button_id.take()`. `press_add` runs only when that taken id is `Some` and the hit id equals it. The take runs before the compare, so the record is gone even when the first release activates. A second release still reports its hit. There is no recorded id, so it returns `Step::Unchanged` and does not call `press_add`. The function does not write focus.

## What the later read shows

`tests::second_truncated_release_does_not_store` is in `crates/emperor-runtime/src/lib.rs` immediately after `tests::saturated_release_is_a_miss`. The next test is `tests::space_on_the_edit_does_not_insert`. It starts on Edit, edit `"1"`, count 0, revision 0, result `"0"`.

A press at `(176.0, 80.0)` hits `window.add` and sets focus to Button. Count stays 0. A release of `(215.9, 95.9)` hits `window.add`. The step is `Changed`. Count is 1, revision is 1, result is `"1"`, and edit stays `"1"`. Focus stays Button.

A second release of `(215.9, 95.9)` still hits `window.add`. The hit is not `None`. The step is `Unchanged`. Count stays 1, revision stays 1, result stays `"1"`, and edit stays `"1"`. Focus stays Button. A later release of `(176.0, 80.0)` also hits `window.add`. The step is `Unchanged`. Count stays 1, revision stays 1, result stays `"1"`, and edit stays `"1"`. Focus stays Button.

The test does not read `button_id`. The second hit is still the button, and the count stays 1, which is the proof that the first release consumed the record. The later release locks that same hit, that step, and that count. Release does not write focus. Do not round `215.9` to `216`.

## Wording

`docs/emperor/GY-EMP-001.md` section 6, after "A release that saturates at the 32-bit limits is outside the client and is a miss. It does not activate. A later release on `window.add` does not activate.", says: "A truncated activation consumes the arm. A second release on `window.add` does not activate." The next sentence is "An allowed paste payload still replaces the edit after focus has moved."

`docs/emperor/contracts/gy-cross-parity-1.md`, after those same three sentences, says those same two sentences. The next sentence is "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale."

## Not the single truncated store

`tests::release_truncates_toward_zero` stores once on `(215.9, 95.9)`. Count becomes 1, revision 1, result `"1"`. That arm is not released again. Its other releases are `(135.9, 80.0)` and `(176.0, 63.9)`. Both truncate off the button onto `window.root` and do not store. The later `(176.0, 80.0)` in that test follows a root hit, and the count stays 0. This slice releases `(215.9, 95.9)` a second time on the arm that just stored. The second hit is still `window.add`. Count stays 1.

## Not the exact inside release

`tests::button_release_inside_activates_and_outside_does_not` presses and releases exact `(176.0, 80.0)`. That pair stores count 1, revision 1, edit `"1"`. It does not truncate. It does not release a second time on the button. The outside release at `(8.0, 8.0)` is a different arm and does not store. This slice truncates `(215.9, 95.9)` onto `window.add`, stores, then releases that point again.

## Not a later release after a miss

`tests::saturated_release_is_a_miss` and `tests::release_truncates_to_a_miss` follow a miss with a later `(176.0, 80.0)`. The count stays 0. This slice follows a store. The later hit is still the button, and the count stays 1.

## Still unlaunched

Sash has not been launched. Emperor does not receive activation from a launched window. Sash posts activation on release inside the control. Emperor activates only on an equal armed id, and only on the release that still holds that id. This is not a Win32 pointer message and not `BN_CLICKED`. A host unit test is not a launched window, not a seat, and not a compositor launch. It is not a Windows launch. Consuming the arm is not a pointer grab. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met. This note does not claim SOTA.
