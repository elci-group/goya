# ADR-RESOLVER-001 — Interpret an admitted scene with the existing D1 machine

Status: Accepted for experimental contract `GY-RESOLVER/0.1` | Date: 2026-11-25 | Owner: Goya Resolver | Supersedes: none

## Context

GRD-001 asks Goya to execute `.gscene` source on the device, beside the existing compilers, without becoming a second language. The directive proposed a Resolved Scene Representation, new crates (`goya-semantics`, `goya-platform-contracts`, `goya-caprichio-registry`), and a later bytecode tier. Those names were proposals. Phase 0 had to reconcile them with the frontend, the D1 machine, the compiler crates, and the graphics crates before any shared infrastructure moved.

The repository already has one admission path and one D1 machine:

- Strict GSSH handoff in `goya-lang` (`admit` through `admit_v4_live`).
- `goya-executable::D1Program`, reached through `CompiledInteraction::execute`.
- Host publication rules in `goya-sdk::D1Application` (`GY-D1-APP/1`): the authored snapshot is revision 0, and the first successful action publishes revision 1.
- Pure scene layout in `goya_lang::layout`, and display text in `goya_lang::display_binding`.
- Platform compilers (Emperor, Sash, Neptune, Maja, Aragon) that consume the same admitted scene and emit native projects.

`goya-sdk` links compiler crates, including Nevada. A runtime that depends on it would carry a compiler toolchain, which GRD-001 §4.4 rules out.

## Decision

1. **One semantic pipeline.** The resolver admits through the GSSH handoff and executes D1 with the existing program. It does not parse source on each event, and it does not introduce a second IR, a second VM, or a bytecode format.

2. **Publication follows `GY-D1-APP/1`.** The loaded snapshot is revision 0. Values change only when an action's program and the whole-state check both succeed. A failed action leaves values and revision unchanged. The resolver does not seed `goya-state::Store`: `Store::commit` advances the revision, so writing the initials there would publish revision 2 on the first activation and diverge from Sash and `D1Application`. `goya-state` remains the transaction engine for other hosts. The resolver library does not depend on it.

3. **No compiler crate in the runtime library.** `goya-resolver` depends on `goya-lang` and `goya-executable` only. `goya-sdk` is a dev-dependency used by the differential test that locks resolver outcomes to `D1Application` on the same scene.

4. **D2 is compiled and refused.** Invalid D2 fails load (`GY-RESOLVER-TRANSITION`). Valid D2 loads, and `instantiate` returns `GY-RESOLVER-D2`. The refusal text names the interaction ids. Mixed D1 and D2 is not partially executed. The caller does not supply an effect journal.

5. **Schema keys need authored initials.** The resolver does not invent state. A D1 key without a matching `@state` initial fails load (`GY-RESOLVER-INITIAL`). Keys outside the D1 schema stay in a side map for display. Durable and session lifetimes are classified from `@state` and reported. This slice does not write them to disk.

6. **Display invalidation is indexed at resolve. Layout stays a full walk when it runs.** State key to display sites is built once, in source order. A dispatch recomputes only sites bound to written keys whose values changed. An unchanged write still advances the revision and evaluates no site. `goya_lang::layout` still walks the geometry tree on its first call. The resolved scene and the viewport do not change for the life of an instance, so a later D1 revision does not call layout again. See the amendment below.

7. **`@platform` exceptions fail closed.** A scene that admits and then carries platform exceptions is `GY-RESOLVER-PLATFORM`. The resolver does not pick a platform or continue down the auto-admit order. An empty exception list means platform `any`.

8. **Profile selection is explicit or strictest-first.** A single `// profile:` comment, or an explicit `--contract`, selects that GSSH contract. `both`, `all`, and other multi-headers are `GY-RESOLVER-PROFILE`. A comment that disagrees with the requested contract is the same code. With no comment and no request, admission tries GSSH/4-live, GSSH/4, GSSH/3-live, GSSH/3, GSSH/2, then GSSH/1. A profile-gate failure (`GY-E1006`, `GY-E1009`, `GY-E1010`, `GY-E1012`, `GY-E1015`) may fall through. Any other failure is reported from the weakest profile that produced a non-gate error.

9. **Gesture policy blocks GY-IR lowering and does not block execution.** `lower` returns `UnsupportedGesture` when a cell has a gesture policy, because GY-IR has no policy node. The application still loads, D1 still runs, and layout still runs. `ir_lowered` is false. Other lowering failures are `GY-RESOLVER-LOWER`.

10. **Performance figures in GRD-001 §10.1 are targets.** `constrained`, `standard`, and `performance` are recorded labels. This slice does not measure 32 MiB, 150 ms, 2 ms, or 60 FPS, and it does not enforce them. The enforced budgets are source size (default 1 MiB of UTF-8 bytes) and dispatch count (default 1_000_000, counted only when the D1 program is entered).

Caprichio selection, GVI residency, process isolation, a persistent cache, a filesystem watch, Quinta, and Josefa are outside this decision. Headless reload is Amendment 2. Closed-surface platform hosts are Amendment 3. Later phases must extend this crate or add a host crate. They must not fork admission or the D1 machine.

## Consequences

- Public contract string: `GY-RESOLVER/0.1`. CLI binary: `goya-resolve`. There is no `goya` multiplexer in this slice.
- The parser stores only the structural button name in `trigger_button`. Dispatch accepts the authored id, then `button.<name>.activate` when that name is unique, then the structural interaction name, then a unique bare trigger.
- Two instances loaded from one `Application` do not share state.
- Invalid field text (`GY-RESOLVER-FIELD`), an unknown action, and a lookup failure do not consume a dispatch slot and do not change the revision. Entering the D1 program consumes a slot even when the program returns an error.
- Workspace CI for the compilers is unchanged by this crate's feature set: the crate is a normal member, and it does not alter compiler output. A full workspace test run was not part of this slice.
- Release acceptance in GRD-001 §21 stays open. See `docs/resolver/GRD-001-baseline.md`.

## Amendment 1 — dispatch does not rerun layout

The first slice cached layout until the revision changed, which reran geometry after every successful action. Layout reads authored geometry and the viewport. Those are fixed on an instance. `frame` therefore calls `goya_lang::layout` once. `layout_generations` counts those calls.

Dispatch no longer calls `scene_announcement`. That function walks every `text-from` cell. `propagate` reads the index built at resolve and evaluates only the changed sites. `sites_evaluated` is that count. The geometry walk inside `layout` is unchanged and is still total on the call that does run. `displays()` remains a full explicit query. The contract string stays `GY-RESOLVER/0.1`. The frozen swarm note is `docs/resolver/P2-reactive.md`.

## Amendment 2 — headless hot reload

`Instance::reload` admits the replacement under the contract the running application already uses. Compatible keys keep their current values and the current revision. New keys take authored initials. Missing keys are dropped. A value-kind change is `GY-RESOLVER-RELOAD` and does not swap. A replacement that admits and then carries D2 is `GY-RESOLVER-D2` and does not swap. Admission failures propagate their own codes and do not swap. Field text is kept when the id survives and the text fits the new max length. The cached frame is dropped because geometry may have changed. `layout_generations` and the dispatch count are not reset. Another instance that shares the previous `Application` is left on that `Arc`. Component ids are recomputed. `--watch` is still unimplemented. The note is `docs/resolver/P6-reload.md`.

## Amendment 3 — one closed-surface host per served platform

The served targets are the `@platform` tokens that have an in-tree compiler: `linux` (Emperor), `windows` (Sash), `ios` (Neptune), `android` (Aragon), `browser` (Maja), and `android_tv` (Nevada). `macos`, `webos`, `tvos`, and Quitasol are not targets. `goya-resolver-host` depends on `goya-resolver` only. A selected target ignores exceptions for other platforms and refuses an exception for itself, because no platform rule is implemented. The headless resolver still refuses every exception. `present` returns `GY-RESOLVER-PRESENT` and does not open the named surface. `goya-resolve --target` prints that closed surface and runs the same D1 machine. The contract string stays `GY-RESOLVER/0.1`. The note is `docs/resolver/P7-hosts.md`.

## Amendment 4 — project the frame, do not open it

`present` on a matching running, initialising, or suspended instance returns the depth-first layout with live field text and `text-from` values. `NativeSurface.opened` is false. A second present does not lay the scene out again. `--present` prints that projection and requires `--target`. No compositor, window, document, or device is contacted. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 5 — a point uses the existing hit test

`point` requires phase `running`. It calls `goya_lang::hit_test` in millipoints and does not apply scroll offsets. The topmost button dispatches `button.<name>.activate`. The topmost input is returned as focus and the field is not edited. A cell, or a point outside every button and input, is a miss. The revision changes only when the button's action publishes. `--pointer` requires `--target`. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 6 — enter replaces the focused field

`enter` requires phase `running` and a focused input. It writes that field through `set_field` and does not change the revision. `point` still does not edit the field. A miss clears focus. A button activation clears focus only after the action publishes. An input with no authored id stays focused and `enter` returns `GY-RESOLVER-FIELD`. `--enter` requires `--target`. Suspend keeps the focus and rejects `enter` until resume. Reload keeps the focus when that field id survives. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 7 — scroll uses the language offsets

`scroll` requires phase `running`. It finds the viewport with `Frame::scroll_target_at` and stores the clamped delta with `ScrollOffsets::scroll_by`. `point` calls `hit_test_scrolled` with those offsets. `present` draws `scrolled_nodes`, including the clip. A scroll does not publish a revision and does not move focus. A fresh layout reconciles offsets, including the layout after reload. `--scroll` requires `--target`. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 8 — the caret is a grapheme offset

`edit` requires phase `running` and a focused input. `Insert` writes at the caret. `Backspace` and `Delete` remove one grapheme. `Move` is left, right, home, or end. Offsets are UTF-8 bytes snapped to grapheme boundaries, the same rule as `goya_runtime::TextBuffer`. The resolver does not depend on that crate. `enter` still replaces the whole field and leaves the caret at the end. A focused `present` reports the byte. No caret bar is drawn. The revision does not change. `--insert`, `--backspace`, `--delete`, and `--caret` require `--target`. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 9 — the CLI watches the scene file

`--watch` polls the loaded file and calls `Instance::reload` when the bytes change. The poll interval is 50 ms and is not a performance measurement. Comparison uses the file bytes, not the modification time. A read error, a non-UTF-8 replacement, or a failed reload is printed and does not swap, so a later good write can still reload. The process does not exit. `--validate-only` does not watch. `--trace` and `--benchmark` still exit 2. When `--present` is set, a successful reload prints the closed surface again. `opened` stays false, and no window is updated. This does not change Amendment 2: a failed reload does not swap, and the revision is preserved. No persistent cache is added. The contract string stays `GY-RESOLVER/0.1`. The note is `docs/resolver/P6-reload.md`.

## Amendment 10 — a declaration diagnostic keeps its source range

A resolver failure that names an authored declaration uses that declaration's UTF-8 byte range. An input that fails to convert, exceeds its max length, or is unbound uses the interaction input's range. A state key with no authored initial uses the first `read` or `write` of that key. A reload that changes a key's kind uses that key's range in the replacement source. Phase, budget, and an unknown action name are not source locations and stay without a range. A scene cell still has no range in the frontend, so the binding stands in for the field. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 11 — trace reports sites and layout generations

`--trace` prints facts the summary hides. A dispatch prints `trace dispatch {action} sites {n} layout {g}`. A point prints `trace point miss layout {g}`, `trace point focus {id} layout {g}`, or `trace point {name} sites {n} layout {g}`. A present prints `trace present closed nodes {n} layout {g}`. A successful reload prints `trace reload revision {n} layout {g}` before that present line. `--validate-only` prints `trace validate actions {n} execution ready` or `execution refused`. `sites` is the display-site count from the dispatch. `layout` is `layout_generations`. A D1 write does not increment it. None of these lines is a duration, a frame time, or a memory measurement. `--benchmark` still exits 2. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 12 — the CLI suspends and resumes

`--suspend` and `--resume` are ordered operations on the running instance. They do not need `--target`. `--suspend` prints `suspend suspended`. `--resume` prints `resume running`. Neither publishes a revision nor lays the scene out. Dispatch, point, enter, scroll, and edit while suspended stay `GY-RESOLVER-PHASE`. A second suspend, or a resume while running, is the same code. `present` still returns the closed frame. The summary `phase:` line is the phase after those operations and before terminate. With `--trace`, the lines are `trace suspend layout {g}` and `trace resume layout {g}`. They are not durations. `--validate-only` does not suspend or resume. Nothing is written to disk. `--benchmark` still exits 2. The contract string stays `GY-RESOLVER/0.1`.

## Amendment 13 — `webos` is served by Viuda

`webos` becomes a served target, and its host is Viuda (`crates/goya-viuda`, contract `GY-VIUDA/0.1`). This amendment does not add a variant to `goya-resolver-host::Target`: Viuda links `goya-resolver` directly and runs a real surface, whereas the closed-surface hosts of Amendment 3 do not.

* Semantics stay in this crate. Viuda loads the source with `Resolver::load`, starts one `Instance`, and runs every action with `Instance::activate("button.<name>.activate")`. It shows what `Instance::state` publishes. It does not execute D1 itself, and it does not admit the source for semantics a second time.
* Presentation is `goya-scene-runtime` in published mode (`Scene::show_published`). It admits the same source again only to build its own cell tree and to compile `@graphic`s with `goya_scene_graphics::compile`. That admission makes no semantic decision. A source this crate admits but presentation refuses is `GY-VIUDA-PRESENT`.
* Lifecycle maps onto `Phase`: a minimised surface suspends the instance, restoring it resumes it, and a close terminates it.
* Durable state remains classified only (Decision 5). Viuda does not write a D8 record yet.
* Viuda is the first host that runs outside this repository's machines, on a 32-bit ARM webOS TV in Developer Mode. The resolver needed no change there except the `host` lifetime arm in the CLI on the Arachnid line.

The contract string stays `GY-RESOLVER/0.1`. The note is `docs/viuda/VIUDA-1.md`.
