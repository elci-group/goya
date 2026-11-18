# Space button security

`tests::space_on_the_button_activates` is immediately after `tests::space_on_the_edit_does_not_insert` in `crates/emperor-runtime/src/lib.rs`. The next test is `tests::admitted_cut_publishes_the_edit_and_clears_it`. It calls `on_tab` and `on_char` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the space path already in that file. The test does not require a `host.rs` change. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## What space does

`on_char` matches `' '` and calls `controller.on_key(WM_KEYDOWN, VK_SPACE, false)`. `WM_KEYDOWN` is `0x0100`. `VK_SPACE` is `0x20`. That arm does not call `set_edit`. It does not push `' '` onto the edit.

`key_action` returns `Dispatch` for that message and wparam. Return is the only `Activate`. Tab is focus movement. `KeyController::on_key` returns that `Dispatch`. Inside `Dispatch` it calls `activate` only when focus is `Button`, the message is `WM_KEYDOWN`, and `wparam` is `VK_SPACE`.

`activate` parses the edit with `parse_canonical_i64` and calls `commit`. It then calls `session.activate(false, true, false, visible)`. A store writes count, revision, and result. It does not replace the edit. Focus is not assigned on that path.

`on_char` does not take a `PointerArm`. Space does not read `button_id`.

## What the test locks

The test starts on Edit. The edit is `"1"`. Count is 0. Revision is 0. Result is `"0"`.

`on_tab(controller, false)` is `Step::Changed`. Focus becomes Button. The edit stays `"1"`. Count stays 0. Revision stays 0. Result stays `"0"`. Tab is `FocusNext`. It does not store.

`on_char` of `' '` is `Step::Changed`. Count is 1. Revision is 1. Result is `"1"`. The edit stays `"1"`. Focus stays Button. The stored value is the edit `"1"`, not a space character.

## Not space on the edit

`tests::space_on_the_edit_does_not_insert` sends `' '` while focus is Edit. That step is `Unchanged`. The edit stays `"1"`. Count stays 0. Focus stays Edit. `Dispatch` does not call `activate` when focus is Edit. This slice is the button path. Do not activate space on the edit. Do not append the space character.

## Not the digit path

`tests::digits_append_only_on_the_edit` appends non-control characters only while focus is the edit. Newline is a different arm. `on_char` for `'\n'` calls `press_add` with no focus check. `press_add` is `on_key` with `WM_KEYDOWN` and `VK_RETURN`, which is `Activate` on either focus. That test returns focus to the edit, sets the edit back to `"1"`, and then sends newline. Space is the focus-gated dispatch. Do not merge those arms.

## Not a pointer release

`on_pointer_release` takes `arm.button_id` once, before it compares the id with the hit. One release consumes `PointerArm`. A matching id calls `press_add`. Space does not read the arm and does not take it. This note does not ask to make space consume `PointerArm`.

A second space is not in `tests::space_on_the_button_activates`. This note does not lock a second space and does not implement one.

## Not a grab

Nothing in this path calls a pointer-capture API. Space is not a grab, not `SetCapture`, and not a `wl_pointer` grab. This note does not ask for a pointer grab. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

This note does not ask to special-case Ctrl+Space or evdev 57.

## Sentences already present

Section 6 of `docs/emperor/GY-EMP-001.md` already says: "Space does not insert. The host calls `KeyController::on_key` with `WM_KEYDOWN` and `VK_SPACE`. The edit stays unchanged. The button activates."

`docs/emperor/contracts/gy-cross-parity-1.md` already says: "Space on the edit calls `KeyController::on_key` with `VK_SPACE` and does not change the edit, the count, the revision, or the result. Space on the button activates."

The same contract already says: "Space is `Dispatch` from `key_action`. `KeyController` activates on Space only when focus is the button (`VK_SPACE` `0x20` on `WM_KEYDOWN`). Space on the edit does not activate." This note does not ask for a new sentence.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. Describing those capabilities does not install Landlock or seccomp. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. Space is not that path. EMP-M1-09 stays unmet.

`tests::space_on_the_button_activates` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a presented buffer, a Wayland seat, or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row. It does not claim EMP-M1.
