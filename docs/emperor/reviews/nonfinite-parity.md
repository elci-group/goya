# Non-finite press parity

A non-finite surface coordinate becomes 0 before truncation toward zero. That 0 is inside `window.root`. It is not the outside-client miss. Sash hit testing uses the admitted DIP rectangles. This note compares that rule with `dip_from_surface_px`, `on_pointer_press`, and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. `tests::non_finite_press_stays_inside_the_client` covers that press. Sash has not been launched. This is not a Win32 pointer message.

EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `dip_from_surface_px`. It does not ask to activate on press.

## Coordinate rule

`dip_from_surface_px` returns 0 when the value is not finite. A finite value truncates toward zero into an `i32` DIP. It does not multiply by 250 millipoints or by buffer scale. `surface_px_truncates_toward_zero` already maps `f64::NAN` to 0. `(0, 0)` is inside the admitted client `(0, 0, 320, 160)` and is `window.root`. NaN is not a miss.

`on_pointer_press` asks the hit map with those DIP values. The miss branch returns hit `None` and clears `button_id`. A hit that is neither the button nor the edit, including `window.root`, clears `button_id`, returns that id, and does not write focus. Neither branch calls `press_add`. A root press does not move focus. The recorded id is clear, so a later release on `window.add` does not activate.

## What the test locks

`tests::non_finite_press_stays_inside_the_client` follows `tests::outside_client_press_does_not_move_focus`. It starts on Edit, edit `"1"`, count 0, revision 0, result `"0"`. A press at `(f64::NAN, f64::NAN)` returns hit `window.root` and `Step::Unchanged`. The hit is `Some`. Focus stays Edit. The session does not store. A press at `(176.0, 80.0)` hits `window.add` and sets focus to Button. A press at `(f64::INFINITY, 80.0)` returns hit `window.root` and leaves focus on Button. A release at `(176.0, 80.0)` stays `Unchanged` with count 0 and revision 0. A press at `(80.0, f64::NEG_INFINITY)` returns hit `window.root` and leaves focus on Button. The test does not read `button_id`. The failed release is the proof that the infinity press cleared the record.

## Not the outside-client press

`tests::outside_client_press_does_not_move_focus` presses `(-1.0, 0.0)`, `(320.0, 80.0)`, and `(8.0, 160.0)`. Those are misses. The hit is `None`. A non-finite press does not take that branch. Positive infinity is not a point to the right of the client. It becomes 0 before truncation, so `(f64::INFINITY, 80.0)` is not the `(320, 80)` miss. Negative infinity is not the `(8, 160)` miss. NaN is not the `(-1, 0)` miss.

`tests::result_press_does_not_move_focus` presses `(8.0, 8.0)`, which is `window.root`, and `(88.0, 128.0)`, which is `state.count`. `(8, 8)` is a finite point. It is not this slice. This slice is NaN and the infinities. Both slices leave focus where it was and do not activate. The outside press is the only one of the three whose hit is `None`.

## Wording

`docs/emperor/GY-EMP-001.md` section 6, after "A later release on `window.add` does not activate.", says: "A non-finite pointer coordinate becomes 0. The press is inside the client and is not a miss."

`docs/emperor/contracts/gy-cross-parity-1.md`, after "Pixel coordinates truncate toward zero into DIP.", says: "A non-finite coordinate becomes 0 before that truncation. The press is inside the client and is not a miss."

## Still unlaunched

Sash has not been launched. Emperor does not receive `WM_COMMAND`. This is not a Win32 pointer message. A host unit test is not a launched window, not a seat, and not a Windows launch. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met.
