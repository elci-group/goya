# Space on the button

Host space on the button is `goya_parity::keys::KeyController`. `on_char(' ')` calls `on_key` with `WM_KEYDOWN` (`0x0100`), `VK_SPACE` (`0x20`), and shift false. It does not call `set_edit`. The host test is `tests::space_on_the_button_activates` in `crates/emperor-runtime/src/lib.rs`, immediately after `tests::space_on_the_edit_does_not_insert`. Sash has not been launched. This is not a Win32 `WM_KEYDOWN` from a launched window. Do not claim a Windows launch.

EMP-M1 is not passed. EMP-M1-25 stays unmet. A unit test is not a seat and not a compositor launch. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `on_char`. A second space is not this slice.

## Dispatch

`key_action` returns `Activate` only for `WM_KEYDOWN` and `VK_RETURN` (`13`). Tab is focus. Every other wparam on that message is `Dispatch`, including `VK_SPACE` (`0x20`). A message that is not `WM_KEYDOWN` is `Dispatch`. The file comment says Space is not an activation chord: it is delivered to whichever control has focus, and the button's own handler activates on Space. That handler has not been launched. `key_action` does not write the session. `crates/sash/src/interactive.rs` already expects `key_action(0x0100, 0x20, false)` to be `Dispatch`.

`KeyController::on_key` returns that action. Inside `Dispatch` it calls `activate` only when focus is `Button`, the message is `WM_KEYDOWN`, and the wparam is `VK_SPACE`:

```rust
sash::interactive::KeyAction::Dispatch => {
    if self.focus == Focus::Button && wparam == VK_SPACE && message == WM_KEYDOWN {
        self.activate();
    }
}
```

On the edit, space does not change the edit or the count. The returned action stays `Dispatch` even when the button activates. `activate` asks Sash `parse_canonical_i64` and `commit`, then `CounterSession::activate`. The `Store` arm writes count, revision, and `format_i64` of the next value. It does not assign the edit. There is no second adder. The host does not add.

## Host path

```rust
// Space is Dispatch. The button activates. The edit does not change.
' ' => {
    controller.on_key(WM_KEYDOWN, VK_SPACE, false);
}
```

The return value is discarded. `on_char` returns `Step::Changed` only when the view differs. The view is count, revision, edit, result, and focus. The space arm does not call `set_edit` and does not call `press_add`.

`on_tab(false)` calls `on_key` with `WM_KEYDOWN`, `VK_TAB` (`9`), and shift false. That action is `FocusNext`. Focus moves from the edit to the button. `on_tab` returns `Changed` because focus moved. It does not write the session.

## What the test locks

`tests::space_on_the_button_activates` admits the counter and starts on the edit, edit `"1"`, count 0, revision 0, result `"0"`. Tab does not store. Space then stores.

```rust
fn space_on_the_button_activates() {
    let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_char(&mut controller, ' '), host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
}
```

The step is `Changed` because count, revision, and result changed. The controller still returned `Dispatch`. The edit stays `"1"`. Focus stays Button. The next test is `tests::admitted_cut_publishes_the_edit_and_clears_it`.

## Sentences

The sentences already exist. This note does not ask for a new sentence.

`docs/emperor/GY-EMP-001.md` section 6:

> Space does not insert. The host calls `KeyController::on_key` with `WM_KEYDOWN` and `VK_SPACE`. The edit stays unchanged. The button activates.

`docs/emperor/contracts/gy-cross-parity-1.md`:

> Space is `Dispatch` from `key_action`. `KeyController` activates on Space only when focus is the button (`VK_SPACE` `0x20` on `WM_KEYDOWN`). Space on the edit does not activate.

The same contract, in the Linux paragraph:

> Space on the edit calls `KeyController::on_key` with `VK_SPACE` and does not change the edit, the count, the revision, or the result. Space on the button activates.

The host test matches those sentences. Space on the edit does not insert. Space on the button stores through `KeyController`. The edit stays `"1"`.

## Not space on the edit

`tests::space_on_the_edit_does_not_insert` does not tab. `on_char(' ')` is `Step::Unchanged`. The edit stays `"1"`. Count stays 0. Focus stays Edit. That is the same `on_char` arm with focus still on the edit, so `Dispatch` does not call `activate`.

## Not the oracle's count 3

`keys::tests::enter_and_space_follow_button_focus` is the oracle, not the host `on_char` path. It calls `KeyController::on_key` directly. Two `VK_RETURN` presses store count 2, revision 2. Space on the edit returns `Dispatch`, leaves focus on the edit, leaves count at 2, and leaves the edit `"1"`. Tab is `FocusNext` onto the button. Space then returns `Dispatch` and stores count 3, revision 3. That count is 3 because two Enters already ran. The host test starts at count 0 and one space stores 1. Same rule. Different prefix.

## Not a pointer release

A release on `window.add` uses `PointerArm` and `press_add`. `on_pointer_release` takes `arm.button_id` and calls `press_add` only when the release hit is that id. `press_add` is `on_key(WM_KEYDOWN, VK_RETURN, false)`, which is `Activate` with no focus test. Space does not use `PointerArm`. Space does not call `press_add`. It enters `on_key` with `VK_SPACE`, and `activate` runs only from the `Dispatch` arm while focus is the button.

## Not a launch

Sash has not been launched. The host message is built inside `on_char`. It is not a `WM_KEYDOWN` from a launched window. A unit test is not a seat and not a compositor launch. EMP-M1-25 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. It does not claim SOTA. It does not ask to change `on_char`. A second space is not this slice.
