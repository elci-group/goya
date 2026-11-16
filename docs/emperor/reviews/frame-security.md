# Frame security

The working tree already paints this frame. The tests were the slice. `scene_frame` in `crates/emperor-runtime/src/host.rs` becomes `pub(crate)` so the unit test `focused_frame_uses_the_live_session` can call it. That visibility stays inside the crate. It must not become `pub`. The paint stays the software frame already in that function. This note does not change the gap ledger and does not pass any row. EMP-M1-10 stays unmet. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.

Reviewed `scene_frame` and the `present_dirty` call in `crates/emperor-runtime/src/host.rs` against `docs/emperor/contracts/gy-emp-render-1.md`, the sandbox paragraph in `docs/emperor/threat-model.md`, and the EMP-M1-10 and EMP-M1-11 rows in `docs/emperor/gap-ledger.md`.

## Paint that stays

`scene_frame` builds five primitives from the admitted client, edit, button, and result. The client is a panel and then a text primitive of the authored client string. The edit is a text primitive of `session.edit`. The button is a button primitive of the authored button string. The result is a text primitive of `session.result`. The frame copies `session.edit` and `session.result` into those text primitives. Client and button text come from the admitted elements, not from the session. A missing role returns an error. The function does not change the session.

Fills are only `Some(0xFFD6E4F5)` on the focused edit or button. Edit focus fills the edit text. Button focus fills the button. Every other fill is `None`, including the client panel, the client text, the unfocused control, and the result. Foregrounds on the frame are `None`. The present call still passes foreground `[20, 20, 28]`, background `[235, 235, 240]`, glyph size 8, and damage `None`.

## Not an accessibility tree

That fill is not an accessibility tree. It does not publish a name, a role, a value, or focus to AT-SPI or to any other screen reader. EMP-M1-10 stays unmet. A unit test that sees `Some(0xFFD6E4F5)` does not close that row.

## No sandbox

The frame does not read the clipboard, the filesystem, the network, or a subprocess. It does not call `request_clipboard_text` or `set_clipboard_text`. Painting the edit string is not a clipboard read. Admission still rejects a filesystem value other than `none`, and rejects `network`, `subprocess`, or `web_content` when true. That check does not call Landlock, seccomp, a namespace, or a privilege drop. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. The running `App` is not confined, because no sandbox is attempted. Describing `filesystem: none` does not install Landlock or seccomp. EMP-M1-11 stays unmet.

## What this slice does not add

No caret, selection, `TextDecoration`, or Vulkan path is added. The edit remains the whole string. `present_dirty` stays the software path. It still passes no clips, no text sizes, and no text overlays. Damage stays `None` at the present call. `None` means a full frame when a frame is presented. It is not a new damage rectangle. A unit test of `scene_frame` is not a presented buffer, not a compositor, and not EMP-M1-25. The test does not call `GoyaWindow::present_dirty`. This note does not claim Windows.

## Paste, copy, and cut stay

Paste, copy, and cut policy are unchanged. A paste request stays `capabilities.paste && focus == Edit`. Button focus does not request. `on_clipboard_text` stays capability-gated only. This slice does not gate `on_clipboard_text` on focus. A later `Some` payload still calls `set_edit` when paste is true. `None` does not. Copy stays the whole edit, and only when copy is true and focus is Edit. Cut still publishes and clears only when cut is `allow` or `default`, focus is Edit, and the edit is non-empty. There is still no selection.

## Not a pass

This note does not mark a ledger row passed. EMP-M1-10 stays unmet. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.
