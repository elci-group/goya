# P7 — native hosts for the served platforms

Status: accepted for experimental contract `GY-RESOLVER/0.1`. This note freezes the host variants. It does not open a surface, emit a native binary, or mark GRD-001 §21 done.

## Served set

The scene language accepts nine `@platform` tokens. Six of them have an in-tree compiler. Those six are the resolver targets.

| Target id | Host type | Compiler product | Crate the host does not link | Surface, closed |
| --- | --- | --- | --- | --- |
| `linux` | `LinuxHost` | Emperor | `emperor` | `wayland` |
| `windows` | `WindowsHost` | Sash | `sash` | `win32` |
| `ios` | `IosHost` | Neptune | `neptune` | `uikit` |
| `android` | `AndroidHost` | Aragon | `goya-sdk` (the Aragon bins live there) | `android.view` |
| `browser` | `BrowserHost` | Maja | `maja` | `dom` |
| `android_tv` | `TelevisionHost` | Nevada | `goya-nevada` | `remote` |

Not served, and `Target::parse` rejects them:

- `macos` — persistence adapter and a clipboard sample. No scene compiler.
- `webos`, `tvos` — scene tokens. No compiler.
- Quitasol — proposed Fire TV work. Not a host.

## Behaviour

`goya-resolver-host` depends on `goya-resolver` only. Each `open` builds a resolver with that `Target`.

A selected target admits a scene whose exceptions name other platforms. Those rules stay on the scene and are not applied. An exception for the selected platform is `GY-RESOLVER-PLATFORM` with `unresolved {platform} platform exception for '{feature}'` and the rule's source range. No rule is implemented, matching the compilers' empty inventories. The headless resolver, which has no target, still refuses every exception and does not fall through.

`instantiate` on a host rejects an application whose target differs. `present` rejects an instance whose target differs with `GY-RESOLVER-PLATFORM`. A terminated or failed instance is `GY-RESOLVER-PHASE`. When the target matches and the phase is `initialising`, `running`, or `suspended`, `present` returns a `NativeSurface`: the layout nodes in depth-first order, with field text and `text-from` values in place of the authored strings. `opened` is false. The call lays out once and does not change phase or revision.

`goya-resolve --target <id> --present` prints `present {id} {surface} closed revision {n}` and one `node` line per element. It does not open the surface. `--present` without `--target` exits 2.

`--suspend` and `--resume` change that in-memory phase. They do not need `--target`, do not publish a revision, and do not lay the scene out. A suspended instance still presents, and `opened` stays false. Dispatch, point, enter, scroll, and edit stay `GY-RESOLVER-PHASE` until `--resume`. Nothing is written to disk. `--validate-only` rejects both flags.

`point` takes millipoints on a running instance. It calls `goya_lang::hit_test_scrolled` with the offsets stored on that instance. A button dispatches `button.<name>.activate`. An input is reported as focus and its field text stays until `enter`. Any other point is a miss and leaves the revision unchanged. A miss clears focus. A button clears focus only after the action publishes. A point before start, or while suspended, is `GY-RESOLVER-PHASE`. `--pointer x,y` requires `--target` and prints `point miss`, `point focus {id}`, or `point activate {name} revision {n} affected {ids}`.

`scroll` moves the scrollable viewport under a point. It uses `Frame::scroll_target_at` and `ScrollOffsets::scroll_by`, and the offset is clamped to that viewport's content. A point on no viewport is a miss. Scroll does not publish state, does not move focus, and does not lay the scene out again. The next `present` uses `scrolled_nodes`, so rects are translated and a clip is set when a scroll ancestor covers the node. Reload reconciles offsets against the new frame and drops a viewport that is gone or no longer scrollable. `--scroll x,y,dx,dy` requires `--target` and prints `scroll miss` or `scroll {name} {x},{y}`.

`enter` replaces the focused input through `set_field`. It requires phase `running`. The revision does not change. The caret moves to the end of the replacement. An input with no authored id stays focused and `enter` is `GY-RESOLVER-FIELD`. Suspend keeps the focus and rejects `enter` until resume. Reload keeps the focus when that field id survives. `--enter <text>` requires `--target` and prints `enter {id}`.

`edit` changes that field at a grapheme caret. `Insert` writes at the caret. `Backspace` and `Delete` remove one grapheme. Motion is left, right, home, or end. The offset is a UTF-8 byte on a grapheme boundary. A rejected write leaves the text and the caret in place. The revision does not change, and no caret bar is drawn. A focused `present` puts that byte on the input node. `--insert`, `--backspace`, `--delete`, and `--caret` require `--target`. They print `insert {id} {byte}`, `backspace {id} {byte}`, `delete {id} {byte}`, or `caret {id} {byte}`.

`Instance::reload` keeps the target selected at load. A replacement that adds an exception for that target does not swap. `goya-resolve --watch` calls that reload when the file bytes change. With `--present`, a successful reload prints this closed surface again. `opened` stays false.

`goya-resolve --target <id>` prints `platform:`, `compiler:`, and `surface: {name} closed`. It dispatches the same D1 machine. An unknown id exits 2. Omitting `--target` keeps the previous stdout. `--present` adds the projected nodes after that summary.

## Out of this note

Opening Wayland, Win32, UIKit, an Android view, a DOM document, or a television remote. Caprichio resolution of a platform rule. Linking a compiler. A shaped caret bar. Measured budgets. GRD-001 §21 stays open.
