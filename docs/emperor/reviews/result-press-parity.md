# Result press parity

A press on the result is not a focus change and not an activation. This note compares that rule with `on_pointer_press` and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`, `press_add` in `crates/emperor-runtime/src/lib.rs`, `hit_map` in `crates/goya-parity/src/pointer.rs`, `Focus` in `crates/goya-parity/src/keys.rs`, `command_activates` in `crates/sash/src/interactive_commit.rs`, and `focus_cycle` in `crates/sash/src/interactive.rs`. `tests::result_press_does_not_move_focus` covers that press.

EMP-M1 is not passed. This is not a launched window, not a seat, and not a Windows launch. Sash has not been launched. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves. The historical body of `docs/emperor/reviews/interaction-parity.md` says the host activates on press. That sentence is history. The current host arms on press and activates on release inside the same id. This note does not ask to restore press activation. It does not rewrite `docs/emperor/reviews/interaction-parity.md` or `docs/emperor/reviews/leave-parity.md`. It does not mark a Parity Lab phase met and does not set `phase_exit_met`.

## Sash

`command_activates` is true only when the control id equals the button id and the note is 0. The window procedure reads the id from the low 16 bits of `wparam` and the note from the high 16 bits of `WM_COMMAND`. Note 0 is `BN_CLICKED`. The system button posts that note on release inside the button. The result and the client are `STATIC`. They are not that button. `focus_cycle` walks only `INPUT_INDEX` and `BUTTON_INDEX`. The edit and the button are created with `WS_TABSTOP`. A `STATIC` control is `WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | SS_NOPREFIX`. The result is not a tabstop. That control has not been launched. Emperor does not receive `WM_COMMAND`.

## Focus

`Focus` is `Edit` or `Button`. `KeyController` starts on the edit. Tab is `FocusNext` and Shift-Tab is `FocusPrev`. Each swaps those two values. There is no result focus. A press on `state.count` or `window.root` must not change the enum. `on_pointer_press` writes `Focus::Button` only when the role is `button` or the id is `window.add`. It writes `Focus::Edit` only when the role is `edit` or the id is `window.amount`. The admitted result role is `result`. The admitted client role is `client`. Neither assignment runs.

## Hit

`hit_map` copies the admitted DIP rectangles. The walk is last to first. A point is inside when `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, in 64-bit arithmetic. The result is `(8, 112, 160, 32)`. `(88, 128)` is inside it: `88 >= 8`, `128 >= 112`, `88 < 168`, and `128 < 144`. The edit is `(8, 64, 120, 32)`. The button is `(136, 64, 80, 32)`. The client is `(0, 0, 320, 160)`. `(8, 8)` misses the edit, the button, and the result, and hits `window.root`. `(176, 80)` is inside `window.add`.

`dip_from_surface_px` truncates toward zero. It does not multiply by 250 millipoints or by buffer scale. At buffer scale 1 the surface pixel is the DIP. `88.0` and `128.0` stay `88` and `128`.

## Release

A press on the button stores that id on `PointerArm` and does not call `press_add`. A press that is neither the button nor the edit sets the recorded id to `None` and returns `Step::Unchanged` with the hit. It does not store. A result press therefore clears a button arm. Focus stays `Button` when the preceding press was `window.add`. `on_pointer_release` takes the recorded id before it compares. `press_add` runs only when the release hit equals that id. After the result press the record is empty, so a release on `(176.0, 80.0)` does not activate. A press on `window.root` takes the same clear. The record is not a pointer-capture API. EMP-M1-25 stays unmet.

## Wording

`docs/emperor/contracts/gy-cross-parity-1.md` says a press on `window.add` sets focus to the button and records that id, a release on that same id activates through `press_add`, a press on `window.amount` sets focus to the edit and clears the record, and a press outside those controls clears the record and does not move focus. That older sentence stays. The next sentences say a press on `state.count` is that press, focus stays where it was, and a later release on `window.add` does not activate.

`docs/emperor/GY-EMP-001.md` says pointer capture is Unmet, and that a press on `state.count` does not move focus and does not activate.

## Test

`tests::result_press_does_not_move_focus` presses `(88.0, 128.0)`, asserts the hit `state.count`, leaves focus on Edit, and does not store. It then presses `window.add`, presses the result, keeps focus on Button, and releases on `(176.0, 80.0)` without activating. It presses `(8.0, 8.0)` and asserts `window.root` without moving focus. Parent re-ran it, `button_release_inside_activates_and_outside_does_not`, and `edit_press_focuses_without_activating`: 1 passed each, 30 filtered. `point_inside_the_result_hits_state_count` still asserts the hit id only. It does not press and it does not read focus. A unit test is not a launched window, not a seat, and not a Windows launch. EMP-M1-09 stays unmet. EMP-M1-25 stays unmet. No ledger row moves.
