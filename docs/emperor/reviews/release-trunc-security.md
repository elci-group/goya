# Release truncation security

`tests::release_truncates_toward_zero` follows `tests::finite_press_truncates_toward_zero` in `crates/emperor-runtime/src/lib.rs`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the release path already in that file. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## What release does

`on_pointer_release` converts both coordinates with `dip_from_surface_px`, then hit-tests. A finite value truncates toward zero. The function takes `arm.button_id` before it compares that id with the hit. One release consumes the arm. `press_add` runs only when the hit id is the armed id. `press_add` is `KeyController::on_key` with `WM_KEYDOWN` and `VK_RETURN`. That is the existing Enter path into `CounterSession::activate`. The release does not add, and it does not call `set_edit`.

`window.add` is `(136, 64, 80, 32)`. Containment is half-open: `x >= 136`, `y >= 64`, `x < 216`, and `y < 96`. `(215.9, 95.9)` becomes `(215, 95)`, which is `window.add`. `(135.9, 80.0)` becomes `(135, 80)`, which is `window.root`. `(176.0, 63.9)` becomes `(176, 63)`, which is `window.root`. A root hit is not the armed id, so the release does not store. The arm is already gone, so a later release on `(176, 80)` does not store either. The press records the id and does not call `press_add`. Do not round `135.9` up to `136` or `63.9` up to `64`. Those points would land on the button and store. Do not activate on press. A press store would run before the truncated release is checked.

Section 6 of `docs/emperor/GY-EMP-001.md` says: "A release uses that same truncation. A release that truncates onto `window.add` activates. A release that truncates off `window.add` does not." `docs/emperor/contracts/gy-cross-parity-1.md` says the same three sentences. Those sentences match this release.

## What the test locks

The test starts on Edit. Count is 0, revision is 0, edit is `"1"`, result is `"0"`.

A press at `(176.0, 80.0)` is hit `Some("window.add")`. Focus becomes Button. Count stays 0. The press does not store.

A release at `(215.9, 95.9)` is hit `Some("window.add")` and `Step::Changed`. Count is 1, revision is 1, result is `"1"`, and the edit stays `"1"`. Focus stays Button. That store is the `press_add` path above. The test locks the session fields. It does not spy on the callee.

A new controller is pressed on `(176.0, 80.0)`, then released at `(135.9, 80.0)`. The hit is `Some("window.root")`, not `None`, and not `window.add`. The step is `Unchanged`. Count, revision, result, and edit stay put. Focus stays Button. A later release at `(176.0, 80.0)` is `Unchanged` and count stays 0. The test locks that step and that count. It does not assert the later hit, revision, result, or edit.

A third controller is pressed on `(176.0, 80.0)`, then released at `(176.0, 63.9)`. The hit is `Some("window.root")` and the step is `Unchanged`. Count, revision, result, and edit stay put. Focus stays Button. No later release follows that one.

`PointerArm.button_id` stays private. The test sees the consumed arm only through the later release. Do not make `button_id` public.

## Not a grab

Taking the arm on release stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_release` does not call a pointer-capture API. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## What it does not prove

The storing release has no second release after it, so the test does not lock that a successful truncated activation consumed the arm. The code does: `take` runs before `press_add`. The `(176.0, 63.9)` release also has no later in-button release. The `(135.9, 80.0)` case is the one that shows the arm is gone.

The off-button releases are inside the client. The test does not lock a truncated miss, a non-finite release, or saturation at `i32::MIN` or `i32::MAX`. It locks hits, not the raw `i32` from `dip_from_surface_px`.

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet.

`tests::release_truncates_toward_zero` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a seat or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
