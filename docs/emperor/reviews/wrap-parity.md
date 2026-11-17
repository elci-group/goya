# Wrap parity

Shift-Tab on the edit wraps to the button and does not store. A second Shift-Tab returns to the edit. Tab from the button returns to the edit. This note compares that cycle with `key_action` in `crates/sash/src/interactive_keys.rs`, `focus_cycle` in `crates/sash/src/interactive.rs`, `KeyController::on_key` in `crates/goya-parity/src/keys.rs`, and `on_tab` in `crates/emperor-runtime/src/host.rs`. The host test is `tests::shift_tab_on_the_edit_wraps_to_the_button` in `crates/emperor-runtime/src/lib.rs`, after `tests::delete_does_not_append_and_backspace_drops_one_cluster`.

EMP-M1 is not passed. Sash has not been launched. Emperor does not call `SetFocus`. A unit test is not a Win32 edit and not a Wayland seat. EMP-M1-10 stays unmet. No ledger row moves. This note does not mark a Parity Lab phase met and does not claim SOTA.

## Shared cycle

`key_action` returns `FocusPrev` for `WM_KEYDOWN` `VK_TAB` (`9`) when shift is down, and `FocusNext` for Tab without shift. The unlaunched image sends those to `focus_cycle`. That walk is `[INPUT_INDEX, BUTTON_INDEX]` with modulo. `INPUT_INDEX` is the edit and `BUTTON_INDEX` is the button. Reverse from the edit index wraps to the button. Forward from the button wraps to the edit. A missing current handle starts at the edit. `focus_cycle` calls `SetFocus` when the next handle is non-null. That image has not been launched. Emperor must not call `SetFocus`. `crates/emperor-runtime` does not.

`KeyController::on_key` performs the same two-value wrap. `FocusNext` moves edit to button and button to edit. `FocusPrev` moves button to edit and edit to button. Focus starts on the edit. The client and the result are not in the cycle. `on_tab` calls `on_key` with `WM_KEYDOWN`, `VK_TAB`, and the shift flag, then returns `Changed` only when focus moved. It does not write the session. `on_modified_special` uses that path for `KeySpecial::Tab` and returns no clipboard. It does not activate.

Both documents carry the same sentence. In `docs/emperor/GY-EMP-001.md` section 6 it follows the sentence that names the focus cycle, the pointer hit, the clipboard policy, and the focus fill. In `docs/emperor/contracts/gy-cross-parity-1.md` it follows "Focus moves only between the edit and the button. It starts on the edit."

> Shift-Tab on the edit moves focus to the button and does not store. A second Shift-Tab returns to the edit. Tab from the button returns to the edit.

## What the test locks

`tests::shift_tab_on_the_edit_wraps_to_the_button` admits the counter on the edit with text `"1"`, count 0, revision 0, and result `"0"`. The first press is `on_modified_special` for Tab with shift true and ctrl false. The step is `Changed`, the clipboard is `None`, and focus is the button. The edit, count, revision, and result stay put. A second press with the same modifiers returns focus to the edit, again with no clipboard and no store.

Plain Tab then uses `on_tab` with shift false. Focus moves to the button and the edit and count stay. A second plain Tab returns to the edit. The edit stays `"1"`, count stays 0, revision stays 0, and the result stays `"0"`. Tab does not activate.

## Oracle and Ctrl+Tab

`keys::tests::tab_then_shift_tab_returns_to_the_edit` already tabs forward, then shift-tabs back, on `KeyController::on_key`. It starts on the edit, records `FocusNext` then `FocusPrev`, and checks that count stays 0. This host slice starts with Shift-Tab on the edit, wraps to the button, Shift-Tabs back, then plain-Tabs off the edit and back from the button. It also checks the edit, the revision, the result, and that the Shift-Tab path offers no clipboard. This note does not ask to change the oracle or Sash.

`tests::digits_append_only_on_the_edit` already moves with Ctrl+Tab and Ctrl+Shift+Tab. This slice is plain Shift-Tab (`ctrl` false) and plain Tab. This note does not ask to special-case Ctrl+Tab.

## Unlaunched

Focus stays on the edit or the button. The client and the result are not focusable. This note does not ask for a third target and does not ask Tab to activate. `focus_cycle` still sits in the unlaunched Sash image, including its `SetFocus` call. The host wrap is the in-process `KeyController` cycle, not that call. EMP-M1-10 stays unmet. A focus move in a unit test is not an AT-SPI pass. No ledger row moves. This note does not mark a Parity Lab phase met.
