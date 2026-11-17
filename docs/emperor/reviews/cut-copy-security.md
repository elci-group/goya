# Cut and copy security

`tests::cut_ignores_the_copy_flag` sits immediately after `tests::on_cut_clears_a_focused_edit_unless_blocked`. It admits the counter, then sets `capabilities.copy` false. `copy_text` returns `None` and leaves the edit `"1"`. Count, revision, and result stay 0. `on_cut` with cut `"default"` still returns `Step::Changed` and clipboard `Some("1")`, then the edit is `""`. The same copy-denied cut of `"00"` under cut `"default"` returns `Some("00")` and clears the edit. Count, revision, and result stay 0, 0, `"0"`. Cut `"allow"` with copy false cuts `"1"` the same way. Cut `"deny"` returns `Step::Unchanged` and clipboard `None` and leaves the edit `"1"`. Button focus under cut `"default"` with copy false does the same and leaves the edit `"1"` and the count 0. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note does not change the gap ledger and does not pass any row.

## What on_cut does

`on_cut` in `crates/emperor-runtime/src/host.rs` does not read `capabilities.copy`:

```rust
pub(crate) fn on_cut(ir: &NativeIr, controller: &mut KeyController) -> SpecialEffect {
    if ir.capabilities.cut == "deny"
        || controller.focus != Focus::Edit
        || controller.session.edit.is_empty()
    {
        return SpecialEffect {
            step: Step::Unchanged,
            clipboard: None,
        };
    }
    let text = controller.session.edit.clone();
    controller.session.set_edit("");
    SpecialEffect {
        step: Step::Changed,
        clipboard: Some(text),
    }
}
```

`allow` and `default` take the publish path. Deny, focus that is not Edit, and an empty edit return `Step::Unchanged` and clipboard `None` and do not call `set_edit`. `set_edit` replaces the edit string and does not change count, revision, or result. The published string is the edit, including `"00"`, not the result `"0"`. There is no selection and no filter.

`copy_text` still returns `None` when `capabilities.copy` is false, and still returns `Some("")` for an empty edit when copy is allowed and focus is Edit. `Shortcut::Cut` calls `on_cut` and calls `set_clipboard_text` only when that effect carries clipboard text. It does not consult the copy flag and does not filter the string.

Section 6 of `docs/emperor/GY-EMP-001.md` now says: "Cut does not read the copy flag. Copy denied still publishes and clears a focused non-empty edit when cut is allow or default." `docs/emperor/contracts/gy-cross-parity-1.md` now says: "That cut does not read the copy flag. Copy denied still publishes and clears a focused non-empty edit when cut is allow or default."

Denied copy must not become a way to block cut. Gating cut on the copy flag would be a policy change. Do not filter the payload. Do not turn an empty allowed copy into `None`.

## What the test locks

With copy false, a focused non-empty edit still cuts under cut `"default"` and under cut `"allow"`. The clipboard text is `"1"` or `"00"`, then the edit is empty, and count, revision, and result stay 0. Cut `"deny"` publishes nothing and does not clear. Button focus publishes nothing and does not clear. Copy denial and cut denial stay separate bits.

## What it does not prove

The new test does not call `on_cut` on an empty edit. The function above still publishes nothing for an empty edit, and that return does not read the copy flag. `on_cut_clears_a_focused_edit_unless_blocked` still locks the empty edit with copy left allowed. The new test calls `on_cut` and `copy_text` on an admitted controller. It does not enter `on_event`, so it does not show `set_clipboard_text`. A unit test is not a launched clipboard, a selection, or a Win32 edit. `host.rs` is unchanged. This is not a sandbox, not Landlock, and not a signed release. No ledger row moves. This note does not pass any row.
