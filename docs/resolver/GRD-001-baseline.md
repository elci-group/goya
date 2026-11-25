# GRD-001 baseline

Phase 0 reconciliation of the Goya Resolver directive with the live tree, plus the headless slice that directive calls P1. This document is the architectural baseline. It is not a release record, and it does not mark the §21 checklist done.

| | |
| --- | --- |
| Date | 2026-11-25 |
| Repository HEAD at the audit | `261b47e2c04fbf780200f514f6c694379aa2a365` |
| Resolver contract | `GY-RESOLVER/0.1` |
| Crate | `crates/goya-resolver` (uncommitted on that HEAD) |
| Decision record | `docs/adr/ADR-RESOLVER-001.md` |

The directive says its module list, CLI, performance budgets, and Linux-first order were proposals. The audit below is what the tree actually contains.

## What already existed

`.gscene` is GY-LANG/1: ontology, cells, buttons, inputs, and bounded D1/D2 interactions. It has no branches, loops, comparisons, or authored D0. Much of the behaviour in the exemplars still lives in Rust hosts under `crates/goya-hello`.

| Directive proposal | Repository fact | This slice |
| --- | --- | --- |
| Semantic resolver and a new RSR | Strict GSSH handoff in `goya-lang` (`GSSH/1` through `GSSH/4-live`) | Admit only through that handoff |
| Execution engine, then a bytecode tier | `goya-executable` D1 and D2 programs | Run D1. Compile D2 and refuse to run it |
| State engine on an indexed store | `goya-state` (`GY-STATE/1`) and `D1Application` (`GY-D1-APP/1`) | Match `D1Application` revision 0. Do not seed `Store` |
| Dependency engine that skips untouched components | `goya-change` records change potential. Layout is a pure full walk | Index display sites at resolve. A write evaluates only the sites bound to changed keys. Layout runs once per instance |
| Presentation engine | Emperor, Sash, Neptune, Maja, Aragon, Nevada | Six closed-surface hosts. No window is opened |
| Capability broker | D2 effect classes and host journals | Deny D2. No other capability grant |
| Resource governor | Per-host limits inside compilers and VMs | Source byte cap and dispatch cap |
| Lifecycle manager | Host-specific | Instance phases after a successful load |
| Diagnostics | `StrictParseError` `GY-E1000`–`GY-E1017`, with byte ranges | Keep those codes. Add `GY-RESOLVER-*` for resolver failures |
| Caprichio registry | `goya-caprichios`, selected by compilers and benchmarks | No runtime selection |
| GVI residency | `goya-gvi`, `goya-gva`, `goya-gru` | No graphics call |
| Hot reload | `goya-hotswap` swaps a compiled artifact after `stage_snapshot` | No scene reload |
| `goya resolve` CLI | No `goya` multiplexer | Binary `goya-resolve` |
| Quinta and Josefa hosts | Named in the directive; not wired as resolver hosts | Absent |
| TOV as a language runtime | `goya-tov` adapts an external compiler CLI | Not used |

Frampton is a separate server language. It is not a Goya backend and not a resolver target.

## Headless slice

`goya-resolver` is a workspace member. Its library depends on `goya-lang` and `goya-executable`. `goya-sdk` is a dev-dependency so a test can compare owners. The architecture-review model lists the crate as experimental, layer `execute`, contract `GY-RESOLVER/0.1`, with edges to `goya-lang` and `goya-executable`. `goya-resolver-host` depends on `goya-resolver` only. The curated crate count in that model is 53.

### Load

1. Reject source over the byte budget (`GY-RESOLVER-BUDGET`, default 1 MiB).
2. Admit. Explicit contract or a single `// profile:` comment wins. Otherwise try strictest-first and keep the weakest non-gate diagnostic. With no target, every `@platform` exception is `GY-RESOLVER-PLATFORM` and does not fall through. A selected target ignores other platforms' exceptions and refuses its own. The surface stays closed. See `docs/resolver/P7-hosts.md`.
3. Compile every D1 and D2 interaction. A compile failure is `GY-RESOLVER-TRANSITION`.
4. Require an authored id on every interaction (`GY-RESOLVER-ACTION-ID`) and a field binding on every input (`GY-RESOLVER-UNBOUND`).
5. Materialise revision 0 from `@state` initials. A missing or mistyped D1 initial is `GY-RESOLVER-INITIAL`.
6. Lower to GY-IR when the scene has no gesture policy. Gesture policy leaves `ir_lowered` false and the load succeeds.
7. Intern structural names, assign depth-first component indexes (root is 0), and build the display index. Each `text-from` site stores its document order, announcement context, and authored id.

`instantiate` clones that owner. A scene with D2 returns `GY-RESOLVER-D2` and does not start. The refusal string is:

`D2 interactions ({ids}) are compiled and are not executed; live host effects are denied`

### Instance

Phases a caller can observe: `initialising`, `running`, `suspended`, `terminated`, `failed`. Load failures never produce an instance, so `unloaded`, `validating`, and `resolving` are not phases. Suspend, resume, and terminate finish inside the call, so `suspending`, `resuming`, and `terminating` are not phases either.

`start` moves `initialising` to `running`. `dispatch` runs only while `running`. `set_field` runs during `initialising` or `running`. `suspend` and `resume` move between `running` and `suspended` and keep the revision. `terminate` accepts `initialising`, `running`, `suspended`, or `failed`. `failed` does not resume. A second `start`, or a dispatch after terminate, is `GY-RESOLVER-PHASE`.

Dispatch address order: authored action id, then `button.<structural-name>.activate` when that button name is unique, then a unique structural interaction name, then a unique bare trigger. The parser stores the structural button name only (`add`), not the full trigger text.

Field conversion uses `goya_lang::handoff::convert_field`. Integers are canonical decimals. A bad field is `GY-RESOLVER-FIELD`, the action does not run, and the revision stays. Text fields are capped at 64 KiB. Bool inputs cannot bind to a text field. Field text is presentation state; committed D1 values stay in the owner. `enter` replaces the text of the input last focused by `point`. It runs only while `running`. A miss or a published button activation clears that focus. The revision stays. The caret moves to the end of that replacement. `edit` inserts and deletes one grapheme at that caret. The offset is a UTF-8 byte. No caret bar is drawn. `scroll` stores a clamped `goya_lang::ScrollOffsets` value for the viewport under the point. `point` and `present` then use that offset. Scroll does not publish a revision. Reload reconciles the offset.

A dispatch slot is consumed only when the D1 program is entered, including when the program fails. Unknown actions and bad fields do not consume a slot. Exhaustion is `refuse` (stay `running`), `suspend`, `terminate`, or `fail`. The crossing call is not applied.

Viewport matches the SDK limits: finite, `1..=32768` points. The builder default is 800×600.

A successful dispatch calls `propagate` on the display index. `sites_evaluated` counts sites bound to written keys whose values changed. Other displays are not visited. `displays()` still projects every binding when the caller asks for the full list. `frame` calls `goya_lang::layout` on the first request and keeps that frame. `layout_generations` counts those calls. A D1 write does not increment it. The walk inside `layout`, on the call that runs, is still the whole geometry tree.

### CLI

`cargo run -p goya-resolver --bin goya-resolve --offline -- <file.gscene>`

| Flag | Behaviour |
| --- | --- |
| `--validate-only` | Admit and resolve. Exit 0 when execution is ready. Exit 1 when D2 would be refused at instantiate, after printing the refusal. Combined with `--dispatch` or `--set`, exit 2 |
| `--inspect` | Accepted. A normal run always prints state |
| `--viewport WxH` | Also `W,H` and `WXH` |
| `--contract` | One GSSH contract |
| `--profile` | `constrained`, `standard`, or `performance`. Printed as a recorded label |
| `--set id=text` | Repeatable. Applied before later dispatches |
| `--dispatch NAME` | Repeatable |
| `--suspend`, `--resume` | Repeatable. Suspend prints `suspend suspended`. Resume prints `resume running`. Neither needs `--target` or publishes a revision. Dispatch and input while suspended exit 1. Combined with `--validate-only`, exit 2 |
| `--pointer x,y` | Repeatable. Requires `--target`. Millipoint hit on the closed surface |
| `--enter text` | Repeatable. Requires `--target`. Replaces the focused input and puts the caret at the end |
| `--insert text` | Repeatable. Requires `--target`. Inserts at the caret |
| `--backspace`, `--delete` | Require `--target`. Delete one grapheme before or after the caret |
| `--caret motion` | `left`, `right`, `home`, or `end`. Requires `--target` |
| `--scroll x,y,dx,dy` | Repeatable. Requires `--target`. Scrolls the viewport under that point |
| `--watch` | Poll the file every 50 ms and call `Instance::reload` when the bytes change. Print `reload revision {n} preserved {keys} initialised {keys} dropped {keys}`. A failed replacement stays on stderr and the process keeps running. With `--present`, a successful reload prints the closed surface again. Combined with `--validate-only`, exit 2 |
| `--trace` | Print `sites` and `layout` generations for a dispatch, a point, a present, a reload, a suspend, a resume, and validate-only. Not a timer. A D1 write leaves `layout` unchanged. Suspend and resume do not lay the scene out |
| `--benchmark` | Exit 2. It does no work |
| `--help`, `-h` | Exit 0 |

Usage errors and unknown flags exit 2 on stderr as `goya-resolve: ...`. Diagnostics exit 1. The process starts, runs the ordered operations, prints the summary at the phase those operations left, then terminates. With no `--suspend`, that phase is `running`.

Stdout is stable and uncoloured: `contract`, `phase`, `revision`, `ir: lowered` or `ir: unavailable`, `persistence: classified-only`, `performance`, one line per action, state, field, and display element. Each dispatch prints `dispatch {action} revision {n} affected {ids-or-dash}` and an announcement line when the display binding produces one. A point prints `point miss`, `point focus {id-or-dash}`, or `point activate {name} revision {n} affected {ids-or-dash}`. `enter` prints `enter {id}`. A scroll prints `scroll miss` or `scroll {name} {x},{y}`. A projected node with a scroll clip appends `clip x,y,w,h`. A caret edit prints `insert {id} {byte}`, `backspace {id} {byte}`, `delete {id} {byte}`, or `caret {id} {byte}`. A focused node appends `caret {byte}`. A filesystem reload prints `reload revision {n} preserved {keys-or-dash} initialised {keys-or-dash} dropped {keys-or-dash}`. With `--trace`, a dispatch adds `trace dispatch {action} sites {n} layout {g}`. A point adds `trace point miss layout {g}`, `trace point focus {id} layout {g}`, or `trace point {name} sites {n} layout {g}`. A present adds `trace present closed nodes {n} layout {g}`. A successful reload adds `trace reload revision {n} layout {g}` before the closed surface is printed again. Validate-only adds `trace validate actions {n} execution ready` or `execution refused`. Suspend adds `suspend suspended` and, with `--trace`, `trace suspend layout {g}`. Resume adds `resume running` and `trace resume layout {g}`. Those lines are not durations.

### Differential check in this slice

`tests/headless_counter.rs` compares the resolver with `D1Application` on `crates/sash/fixtures/interactive-live.gscene` (GSSH/4-live, gestures, IR unavailable). At 320×160 the `amount` field is millipoints `(20000, 60000, 120000, 30000)`. Start is revision 0, count 0. `action.add` with field `"1"` publishes count 1, revision 1, announcement `count increased by 1, from 0 to 1`, and display element `window.total`. A following add of `0` stays at count 1, advances to revision 2, and announces nothing. The shared owner matches those values.

Also covered by that file: `gssh2-counter.gscene` stays on GSSH/2 and lowers; `tests/maja/fixtures/display-counter.gscene` invalidates only the written element; overflow and a non-canonical integer leave the snapshot in place; two instances do not share state; duplicate structural names are interned; source over the byte cap fails before admission; an Android `@platform` exception is refused; a D2 network interaction compiles and is refused at instantiate.

## Release checklist

GRD-001 §21. Nothing below is a production gate pass.

| # | Item | This baseline |
| --- | --- | --- |
| 1 | Direct `.gscene` execution without a native compiler | Headless D1 only |
| 2 | Shared semantic contracts with compiled Goya | GSSH handoff and `GY-D1-APP/1` publication. No differential harness against Emperor, Sash, Neptune, Maja, or Aragon output |
| 3 | Typed state and incremental dependency resolution | Typed D1 state. Dispatch evaluates changed display sites only, and does not rerun layout. The first layout call still walks the whole geometry tree. `--trace` prints the site count and the layout generation. It is not a timer |
| 4 | Native or platform-appropriate presentation | Hosts for `linux`, `windows`, `ios`, `android`, `browser`, and `android_tv`. `present` returns the projected frame and `opened` is false. `point` hit-tests that frame. `enter` replaces the focused input's text. `scroll` applies language scroll offsets on the closed surface. `edit` inserts and deletes graphemes at a byte caret. No caret bar is drawn. No window, document, or device surface is opened |
| 5 | Runtime Caprichio selection | Absent |
| 6 | GVI graphics integration | Absent |
| 7 | Enforceable resource budgets | Source bytes and dispatch count. Memory, CPU, GPU, storage, network, and frame budgets are unmeasured |
| 8 | Application isolation and capability authorisation | D2 denied. No process isolation, no filesystem jail, no network broker |
| 9 | Persistent state and lifecycle management | Lifetimes are classified. Nothing is written to disk. `--suspend` and `--resume` change the in-memory phase. A suspended instance still projects. Dispatch and input wait until resume |
| 10 | Hot reloading with state preservation | `Instance::reload` keeps compatible values and the revision, and a failed replacement does not swap. `--watch` polls the file and calls that reload. A failed replacement does not swap and the process keeps running. A successful reload with `--present` prints the closed surface again. No window is updated |
| 11 | Source-mapped runtime diagnostics | Admission keeps frontend ranges. A bad field, an unbound input, a missing initial, and a reload kind change also keep the declaration range. Phase, budget, and an unknown action name have no range. Cells have no range in the frontend |
| 12 | Differential compiler–Resolver conformance | One shared-owner test against `D1Application` |
| 13 | Physical hardware benchmark evidence | Absent. `--benchmark` exits 2 |
| 14 | Versioned embedding API and CLI | `GY-RESOLVER/0.1` and `goya-resolve` |
| 15 | Documented platform limitations | This file and `docs/resolver/P7-hosts.md`. `macos`, `webos`, `tvos`, and Quitasol are not hosts. Every served surface is closed |
| 16 | No regressions in existing Goya compilers | This slice does not edit compiler crates. A workspace compiler suite was not re-run |

## Still open

P3 and P7 project a closed surface for each served host (`docs/resolver/P7-hosts.md`). Opening a Wayland, Win32, UIKit, Android, DOM, or television surface is still open. A shaped caret bar is still not drawn. Also open: P4 capability brokering and process isolation, P5 compact caching and GVI residency, Quinta, Josefa, and P8 hardware benchmarks and fuzzing. A cached RSR, if added later, stays disposable and must not skip capability checks.

## Evidence

Package-local commands on 2026-11-25, against the uncommitted resolver on HEAD `261b47e2c04fbf780200f514f6c694379aa2a365`. These are not the workspace CI gate (`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`).

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver` | Exit 0 |
| `CARGO_TARGET_DIR=/tmp/goya-resolver-reload cargo test -p goya-resolver --offline --lib --tests --bins` | 22 passed, 0 failed (15 headless, 3 reactive, 4 reload) on 2026-11-25. The temporary target was removed. A separate `cargo test --workspace` was already using `target/` and was left running |
| `cargo clippy -p goya-resolver --offline --all-targets --no-deps -- -D warnings` | Exit 0 |
| `CARGO_TARGET_DIR=/tmp/goya-arch-review cargo test --manifest-path website/architecture-review/Cargo.toml --offline` | 8 passed, 0 failed, including `crate_ids_are_unique` at 52. That count is the prior slice |

Host slice, same day, after `docs/resolver/P7-hosts.md`. The default `target/` was free. The architecture-review temporary target was removed.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 29 passed, 0 failed (25 resolver: 15 headless, 3 reactive, 4 reload, 3 target; 4 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |
| `CARGO_TARGET_DIR=/tmp/goya-arch-review cargo test --manifest-path website/architecture-review/Cargo.toml --offline` | 8 passed, 0 failed, including `crate_ids_are_unique` at 53 |

Projection follows that host slice. `present` returns the frame and leaves `opened` false.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 30 passed, 0 failed (25 resolver, 5 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

Pointer hits follow that projection. A button publishes through the existing D1 action. An input is focus only.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 31 passed, 0 failed (25 resolver, 6 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

Focused text entry follows that hit test. `enter` writes the focused field and does not publish a revision. A miss or a published button activation clears the focus.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 32 passed, 0 failed (25 resolver, 7 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

Scroll offsets follow that text entry. `scroll` clamps a viewport with `goya_lang::ScrollOffsets`. `point` and `present` use the stored offset. The revision stays until a button on that scrolled frame publishes.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 33 passed, 0 failed (25 resolver, 8 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

The caret follows that scroll offset. `edit` inserts and deletes graphemes in the focused field. `present` reports the byte on that input. The revision stays until a button publishes.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 34 passed, 0 failed (25 resolver, 9 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

Filesystem `--watch` follows that caret. The process polls the admitted file and calls `Instance::reload` when the bytes change. A missing file, a non-UTF-8 replacement, and a rejected scene leave the revision in place. The next good file reloads. `--present` prints the closed surface again. `--trace` and `--benchmark` still exit 2.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 35 passed, 0 failed (26 resolver: 15 headless, 3 reactive, 5 reload, 3 target; 9 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

Declaration ranges follow that watcher. A field that is not a canonical integer points at `amount: int from "window.amount"`. An unbound GSSH/1 input points at `amount: int`. A state key with no authored initial points at `read count: int`. A reload that changes `count` from int to text points at that key in the replacement. An unknown action name has no range. Phase and budget failures still have no range.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 36 passed, 0 failed (27 resolver: 16 headless, 3 reactive, 5 reload, 3 target; 9 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

`--trace` follows those ranges. It prints the display-site count and the layout generation for a dispatch, a point, a present, a successful reload, and validate-only. The first dispatch on the live counter reports `sites 1` and `layout 0`. Setting the field to `0` and dispatching again reports `sites 0` and `layout 0`. The following present of four nodes reports `layout 1`. A run without `--trace` does not print `trace `. `--benchmark` still exits 2.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 37 passed, 0 failed (28 resolver: 17 headless, 3 reactive, 5 reload, 3 target; 9 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

`--suspend` and `--resume` follow that trace. Suspending after one add, then resuming and adding again, publishes count 2 at revision 2. A second dispatch while suspended is `GY-RESOLVER-PHASE` and leaves revision 1. A suspended present of the live counter is `present linux wayland closed revision 0`, with `trace suspend layout 0` before `trace present closed nodes 4 layout 1`. `opened` stays false. `--validate-only` with `--suspend` exits 2. `--benchmark` still exits 2.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver -p goya-resolver-host` | Exit 0 |
| `cargo test -p goya-resolver -p goya-resolver-host --offline --lib --tests --bins` | 38 passed, 0 failed (29 resolver: 18 headless, 3 reactive, 5 reload, 3 target; 9 host) |
| `cargo clippy -p goya-resolver -p goya-resolver-host --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

`goya-sdk` already fails `clippy::result_large_err` in its Android and Nevada modules. This slice does not edit that crate. Resolver clippy uses `--no-deps` so those existing warnings are not treated as resolver failures. A clippy run without `--no-deps` still type-checks `goya-sdk` and stops on those warnings.
