# Cut and copy parity

Emperor `on_cut` and Sash `clipboard_blocked` share one cut gate. Cut is not gated by the copy flag. This note reads `tests::cut_ignores_the_copy_flag` in `crates/emperor-runtime/src/lib.rs`, `on_cut` in `crates/emperor-runtime/src/host.rs`, and `clipboard_blocked` in `crates/sash/src/interactive_clipboard.rs`.

Sash has not been launched. A host unit test is not a Win32 edit. EMP-M1-25 stays unmet. This note does not mark a Parity Lab phase met and does not set `phase_exit_met`. No ledger row moves.

## Shared gate

`clipboard_blocked` blocks `OP_COPY` when `copy_allowed` is false, blocks `OP_PASTE` when `paste_allowed` is false, and blocks `OP_CUT` only when `cut_mode == CUT_DENY` (`2`). Cut does not read `copy_allowed`.

`on_cut` matches that gate. `"deny"`, focus other than the edit, and an empty edit return `Step::Unchanged` and clipboard `None`. `"allow"` and `"default"` publish the whole edit and clear it through `set_edit("")`. The function does not read `capabilities.copy`. That read stays out. Button focus does not cut.

Copy of an empty allowed edit stays `Some("")`. That is not a Sash empty selection. Sash copies a selection; an empty selection does not replace the clipboard. Emperor whole-field empty copy stays `Some("")`. It stays that way.

A non-canonical edit such as `"00"` is offered as that text, not as the result `"0"`. Cutting it does not store.

## Test

`tests::cut_ignores_the_copy_flag` follows `tests::on_cut_clears_a_focused_edit_unless_blocked`. With copy set false and cut `"default"`, `copy_text` returns `None` and leaves the edit `"1"`. `on_cut` then returns `Step::Changed`, clipboard `Some("1")`, and an empty edit. Count, revision, and result stay `0`, `0`, `"0"`. The same copy-denied default cut of `"00"` publishes `Some("00")`, clears the edit, and does not change count, revision, or result. Copy denied with cut `"allow"` publishes `Some("1")` and clears the edit. Copy denied with cut `"deny"` publishes nothing and leaves `"1"`. Copy denied with button focus publishes nothing and leaves `"1"`.

## Wording

Section 6 of `docs/emperor/GY-EMP-001.md`, after "Cut of an empty edit publishes nothing.", says: "Cut does not read the copy flag. Copy denied still publishes and clears a focused non-empty edit when cut is allow or default."

`docs/emperor/contracts/gy-cross-parity-1.md`, after "Cut publishes that same text, clears the edit, and does not store.", says: "That cut does not read the copy flag. Copy denied still publishes and clears a focused non-empty edit when cut is allow or default."

## Unlaunched

The Sash image has not been launched. Matching the gate in a host unit test is not a Win32 edit and not a launched clipboard. EMP-M1-25 stays unmet.
