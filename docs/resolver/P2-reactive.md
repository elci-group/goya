# P2 reactive dispatch — frozen contract

Status: frozen for the swarm that implements it. Agents implement this contract. They do not renegotiate it.

Date: 2026-11-25. Parent crate: `crates/goya-resolver`. Contract string stays `GY-RESOLVER/0.1`.

## Problem

`Instance::activate` calls `scene_announcement`, which walks every `text-from` cell. `Instance::frame` reruns `goya_lang::layout` after every revision bump. Layout reads authored geometry and the viewport only. A D1 write does not change geometry.

## Rules

1. Dispatch recomputes only display sites bound to written keys whose before and after values both exist and differ.
2. Sites are recorded once, at resolve, in the same depth-first order as `goya_lang::display_binding::visit_displays` (children pushed in reverse).
3. Announcements use `goya_lang::display_binding::announcement` on those sites in that order. The string must match `scene_announcement` for the same snapshots.
4. A site with no authored id still counts for the announcement and for `sites_evaluated`. It is omitted from `affected_elements`.
5. Unchanged writes and missing keys add nothing to `sites_evaluated`.
6. `sites_evaluated` is the number of selected sites, not the number of components and not the number of display sites in the scene.
7. The propagate function does not take a scene or a cell. It cannot walk the tree.
8. `frame` calls `goya_lang::layout` only the first time for that instance. Later calls return the stored frame. `layout_generations` counts those calls.
9. No new VM, no `goya-state` dependency, no D2 execution, no window, no measured performance claim.
10. The geometry walk inside `goya_lang::layout` stays a full walk. This contract does not claim that walk is incremental. It claims a state write does not invoke it again.

## File ownership

| Owner | Path | May edit |
| --- | --- | --- |
| J1 | `crates/goya-resolver/src/dependencies.rs` | yes, this file only |
| J2 | `crates/goya-resolver/src/lib.rs` | yes, this file only |
| J3 | `crates/goya-resolver/tests/reactive.rs` | yes, this file only |

No agent runs Cargo. The parent formats, tests, and clippy.

## Evidence

Parent gate after the swarm, 2026-11-25:

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver` | Exit 0 |
| `cargo test -p goya-resolver --offline --lib --tests --bins` | 18 passed, 0 failed |
| `cargo clippy -p goya-resolver --offline --all-targets --no-deps -- -D warnings` | Exit 0 |

`a_local_write_skips_unrelated_displays` admits a GSSH/4-live scene with 200 idle displays. Writing `title` and `count` reports `sites_evaluated == 2`, affected ids `title` then `count`, and the announcement `Title changed from "a" to "b"; Count increased by 1, from 0 to 1`, matching `scene_announcement`. Adding zero on the live counter evaluates 0 sites. Layout generations stay 1 across that write. The amount rect stays `(20000, 60000, 120000, 30000)` millipoints.

Parent review: `affected_elements` now drops an empty authored id, matching `element_ids`. Locksmithd is not installed, so the swarm used file ownership instead of leases.
