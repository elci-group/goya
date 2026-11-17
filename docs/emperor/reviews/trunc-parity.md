# Finite press parity

Sash hit testing uses integer DIP. Emperor receives surface `f64` pixels and truncates finite values toward zero. It does not round to nearest. This note compares that rule with `dip_from_surface_px`, `on_pointer_press`, and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`. `tests::finite_press_truncates_toward_zero` follows `tests::non_finite_press_stays_inside_the_client` and covers the fractional finite points and the 32-bit saturation. Sash has not been launched. This is not a Win32 pointer message.

EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `dip_from_surface_px`. It does not ask to activate on press.

## Coordinate rule

`dip_from_surface_px` returns 0 when the value is not finite. A finite value uses `trunc`, toward zero, then saturates at `i32::MAX` and `i32::MIN`. It does not multiply by 250 millipoints or by buffer scale. The admitted client is `(0, 0, 320, 160)`. `window.add` is `(136, 64, 80, 32)`, so `x < 216` and `y < 96`.

`(215.9, 95.9)` becomes `(215, 95)`. That point is inside `window.add`. Nearest would be `(216, 96)`, which is not the button. `(-0.9, 80)` becomes `(0, 80)`, which is `window.root`, not the outside-client miss at `x = -1`. A value whose truncation is at either 32-bit limit becomes `i32::MAX` or `i32::MIN`. Both are outside the client. The hit is `None`.

`on_pointer_press` asks the hit map with those DIP values. The miss branch returns hit `None` and clears `button_id`. A hit on `window.add` sets focus to the button and records that id. It does not call `press_add`. A hit on `window.root` clears `button_id` and does not write focus. A later release activates only when the hit id is still the armed id.

## What the test locks

`tests::finite_press_truncates_toward_zero` starts on Edit, edit `"1"`, count 0, revision 0, result `"0"`. A press at `(215.9, 95.9)` returns hit `window.add` and `Step::Changed`. Focus becomes Button. Count stays 0. That change is focus, not activation. A press at `(-0.9, 80.0)` returns hit `window.root` and `Step::Unchanged`. Focus stays Button. The session does not store. That press is not a miss.

A press at `(f64::from(i32::MAX) + 1.0, 80.0)` returns hit `None` and `Step::Unchanged`. Focus stays Button. Edit, count, revision, and result stay put. A release at `(215.9, 95.9)` stays `Unchanged` with count 0 and revision 0. A press at `(f64::from(i32::MIN) - 1.0, 80.0)` returns hit `None` and leaves focus on Button with count 0. The test does not read `button_id`. The miss branch clears the armed id, and the root press at `(-0.9, 80)` clears it as well. The failed release is the proof that the button does not activate afterwards.

## Not the non-finite press

`tests::non_finite_press_stays_inside_the_client` owns NaN and the infinities. Those values become 0 before truncation, so the hit is `window.root` and is `Some`. Positive infinity is not saturation at `i32::MAX`. It is not the miss this test locks with `f64::from(i32::MAX) + 1.0`. Negative infinity is not `i32::MIN`. NaN is not `(-0.9, 80)`.

## Not the outside-client press

`tests::outside_client_press_does_not_move_focus` owns `(-1.0, 0.0)`, `(320.0, 80.0)`, and `(8.0, 160.0)`. Those hits are `None`. `(-0.9, 80)` is not that `x = -1` miss. Saturation is also hit `None`, but it is the 32-bit limit, not the client edge those three points already own. Both the outside press and the saturated press leave focus where it was and do not activate. The fractional button press is the one whose hit is `window.add`.

## Wording

`docs/emperor/GY-EMP-001.md` section 6, after "A non-finite pointer coordinate becomes 0. The press is inside the client and is not a miss.", says: "A finite coordinate truncates toward zero. Saturation at the 32-bit limits is outside the client and is a miss."

`docs/emperor/contracts/gy-cross-parity-1.md`, after "A non-finite coordinate becomes 0 before that truncation. The press is inside the client and is not a miss.", says: "A finite coordinate truncates toward zero. Saturation at the 32-bit limits is outside the client and is a miss."

## Still unlaunched

Sash has not been launched. Emperor does not receive `WM_COMMAND`. This is not a Win32 pointer message. A host unit test is not a launched window, not a seat, and not a Windows launch. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met.
