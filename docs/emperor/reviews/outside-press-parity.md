# Outside press parity

A press outside the client rectangle is a miss. Focus stays. The recorded id is cleared. A later release on `window.add` does not activate. This note compares that rule with `on_pointer_press` and `on_pointer_release` in `crates/emperor-runtime/src/host.rs`, `press_add` in `crates/emperor-runtime/src/lib.rs`, `HitMap::at` in `crates/goya-parity/src/pointer.rs`, and `command_activates` in `crates/sash/src/interactive_commit.rs`. `tests::outside_client_press_does_not_move_focus` covers that press.

EMP-M1 is not passed. This is not a grab, not a seat, and not compositor qualification. Sash has not been launched. A host unit test is not a Win32 edit and not a Windows launch. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `host.rs`.

## Shared rule

Sash pointer activation is `WM_COMMAND` note 0 (`BN_CLICKED`). The window procedure reads the id from the low 16 bits of `wparam` and the note from the high 16 bits. `command_activates` is true only when that id is the button id and the note is 0. The system button posts that note on release inside the button. Emperor must not activate on press. A press on `window.add` only records that id and sets focus to Button. A release on that same id calls `press_add`. A release elsewhere does not. The miss branch of `on_pointer_press` already clears `button_id` and returns hit `None` without writing focus. Count, revision, edit, and result stay put.

## Hit

The client is `(0, 0, 320, 160)`. `HitMap::at` returns `None` when the point fails `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`, in 64-bit arithmetic. `(-1, 0)`, `(320, 80)`, and `(8, 160)` are misses. `goya-parity` `tests::one_pixel_left_of_the_client_is_none` uses `x = client.x - 1` and asserts the hit is none. It does not press. `(8, 8)` hits `window.root`. It is not a miss. `tests::result_press_does_not_move_focus` already presses that point. `(88, 128)` is `state.count` and is that other test. `dip_from_surface_px` truncates toward zero. `-1.0`, `320.0`, `8.0`, and `160.0` stay those integers. It does not multiply by 250 millipoints or by buffer scale.

## What this test locks

`tests::outside_client_press_does_not_move_focus` starts on Edit, edit `"1"`, count 0, revision 0, result `"0"`. A press at `(-1.0, 0.0)` returns hit `None` and `Step::Unchanged`. Focus stays Edit. The session does not store. A press at `(176.0, 80.0)` hits `window.add`, sets focus to Button, and leaves count at 0. A press at `(320.0, 80.0)` returns hit `None` and leaves focus on Button. A release at `(176.0, 80.0)` stays `Unchanged` with count 0 and revision 0. A press at `(8.0, 160.0)` returns hit `None` and leaves focus on Button. The test does not read `button_id`. The failed release is the proof that the outside press cleared the record.

## Not the result press, and not the positionless release

`tests::result_press_does_not_move_focus` presses inside the client. `(88.0, 128.0)` hits `state.count`. `(8.0, 8.0)` hits `window.root`. Those hits take the else branch: clear the record, return the hit id, and do not write focus. An outside press never reaches that branch. The hit is `None`.

`tests::release_without_a_position_does_not_activate` presses `window.add` and releases at `(-1.0, -1.0)`. That slice is the release. This slice is the press. The later release here is on `window.add`, after the outside press has already cleared the record.

## Wording

`docs/emperor/contracts/gy-cross-parity-1.md` already says a point outside the client is a miss, and that a press outside the button and the edit clears the record and does not move focus. Those older sentences stay. After the `state.count` sentence it now says: "A press outside the client rectangle is a miss. Focus stays where it was. The recorded id is cleared. A later release on `window.add` does not activate." This note does not ask to delete the older sentences.

`docs/emperor/GY-EMP-001.md` section 6, after the `state.count` sentence, says: "A press outside the client rectangle is a miss. Focus stays. A later release on `window.add` does not activate." Pointer capture stays Unmet. The record is not a pointer-capture API.

## Still unlaunched

Sash has not been launched. Emperor does not receive `WM_COMMAND`. The host unit test is not a Win32 edit and not a Windows launch. It is not a grab, not a seat, and not compositor qualification. EMP-M1-25 stays unmet. No ledger row moves. No Parity Lab phase is met.
