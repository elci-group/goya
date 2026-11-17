# Release truncation parity

Sash posts `BN_CLICKED` on release inside the button. Emperor activates only when the release hit id equals the id recorded on press. Both coordinates are truncated toward zero before the hit test. Emperor does not round. This note compares that rule with `on_pointer_release` in `crates/emperor-runtime/src/host.rs`, read after `tests::finite_press_truncates_toward_zero`. Sash has not been launched. This is not a Win32 pointer message.

EMP-M1 is not passed. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `host.rs`. It does not ask Emperor to round.

## Shared release rule

`on_pointer_release` passes both surface coordinates through `dip_from_surface_px`, then asks the hit map. It takes `arm.button_id`. No recorded press returns `Step::Unchanged`. A hit id other than that armed id returns `Step::Unchanged`. Equal ids call `press_add`.

`(215.9, 95.9)` truncates to `(215, 95)` and is inside `window.add`, so an armed release stores count 1. `(135.9, 80)` truncates to `(135, 80)`, which is `window.root`, so it does not store. `(176, 63.9)` truncates to `(176, 63)`, also `window.root`, so it does not store. A press at `(176, 80)` only arms the button. It does not activate.

## What the later read shows

`tests::release_truncates_toward_zero` is in `crates/emperor-runtime/src/lib.rs` immediately after `tests::finite_press_truncates_toward_zero`. The next test is `tests::space_on_the_edit_does_not_insert`. An armed release of `(215.9, 95.9)` hits `window.add` and stores count 1, revision 1, result `"1"`. A release of `(135.9, 80.0)` hits `window.root` and does not store. A later release of `(176.0, 80.0)` on that same arm does not store. A release of `(176.0, 63.9)` hits `window.root` and does not store.

`docs/emperor/GY-EMP-001.md` section 6, after "A finite coordinate truncates toward zero. Saturation at the 32-bit limits is outside the client and is a miss.", says: "A release uses that same truncation. A release that truncates onto `window.add` activates. A release that truncates off `window.add` does not." The next sentence is the allowed-paste sentence.

`docs/emperor/contracts/gy-cross-parity-1.md`, after that same finite-coordinate sentence, says those same three sentences. The next sentence is "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale."

## Not the finite-press test

`tests::finite_press_truncates_toward_zero` presses `(215.9, 95.9)`. The hit is `window.add`, focus becomes Button, and count stays 0. A later press at `(-0.9, 80.0)` is `window.root`. A press at `f64::from(i32::MAX) + 1.0` is a miss and clears the arm. The release of `(215.9, 95.9)` comes only after that saturated press, so it stays `Unchanged` with count 0 and revision 0. That release does not activate. This slice is the armed release. That test does not lock it.

`tests::button_release_inside_activates_and_outside_does_not` uses exact points. A press and release at `(176.0, 80.0)` store count 1. A release at `(8.0, 8.0)` does not. It does not truncate a fractional release onto or off `window.add`.

## Still unlaunched

Sash has not been launched. Emperor does not receive `BN_CLICKED` from a launched window. This is not a Win32 pointer message. A host unit test is not a launched window, not a seat, and not a Windows launch. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met.
