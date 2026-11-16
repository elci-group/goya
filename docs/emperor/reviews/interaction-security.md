# Interaction security

Commit `594bac7bdda15b36dd296aaf2dfc1c0dbbacd1b4`. Reviewed `crates/emperor-runtime/src/host.rs` and `crates/emperor-runtime/src/lib.rs` against `docs/emperor/threat-model.md`. This runtime does not install a sandbox. EMP-M1-11 stays unmet.

## Denied paste

Absent as a write. `on_event` calls `request_clipboard_text` from `Shortcut::Paste` only when `ir.capabilities.paste` is true. `ClipboardText` calls `set_edit` only for `Some(text)` when paste is true. `None`, an empty payload, and any payload while paste is false return `Step::Unchanged`. `attempt_paste` writes `"999"` only when paste is true. `prove_paste_denied` errors if a denied paste changes the edit.

## Copy

Absent as an unconditional copy. `on_event` calls `GoyaWindow::set_clipboard_text` only from `Shortcut::Copy` when `ir.capabilities.copy` is true. Other shortcuts do not copy. `open_admitted` rejects a `copy_allowed` mismatch.

## Cut

Present. Shift+Delete returns `Step::Unchanged` when `ir.capabilities.cut == "deny"` and does not call `set_edit`. On this host, `allow` and `default` also leave the edit unchanged. That shared no-op does not rewrite `default` into `deny`. `open_admitted` still accepts all three strings.

## Described privileges

Absent as a grant. `open_admitted` rejects `filesystem` other than `"none"` and rejects `network`, `subprocess`, or `web_content` when true. Describing those fields does not install Landlock, seccomp, a namespace, or a privilege drop.

## Suite constraints

A scripted interaction suite must not call `set_edit` on denied paste, must not call `set_clipboard_text` when copy is false, and must not let Shift+Delete change the edit when cut is `deny`. It must not admit another filesystem value or a true network, subprocess, or web_content flag. It must not treat a description as an enforced sandbox. Edit and count changes stay on `KeyController` and `CounterSession`. A headless paste check is not a launched clipboard.

## Working tree

The cut paragraph above describes commit `594bac7`. The working tree now publishes and clears the focused edit on Shift+Delete when cut is `allow` or `default` and the edit is non-empty. `deny` still returns unchanged and does not call `set_edit`. The publish is the whole edit, not a selection. Button focus does not cut. Denied paste and the copy gate are unchanged. This still does not install a sandbox. EMP-M1-11 stays unmet.
