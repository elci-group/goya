# Second truncated release security

`tests::second_truncated_release_does_not_store` is immediately after `tests::saturated_release_is_a_miss` in `crates/emperor-runtime/src/lib.rs`. The next test is `tests::space_on_the_edit_does_not_insert`. It calls `on_pointer_press` and `on_pointer_release` on an admitted controller. `git diff --stat -- crates/emperor-runtime/src/host.rs` is empty. This note reviews the release path already in that file. The test does not require a `host.rs` change. The threat model is `docs/emperor/threat-model.md`. The gap ledger still lists EMP-M1-09, EMP-M1-11, and EMP-M1-25 as Unmet. This note does not change the gap ledger and does not pass any row.

## What the release does

`on_pointer_release` converts both coordinates with `dip_from_surface_px`, then hit-tests. A finite value truncates toward zero. `(215.9, 95.9)` becomes `(215, 95)`. The function then `arm.button_id.take()` before it compares that id with the hit. `press_add` runs only when they are equal. `press_add` is `KeyController::on_key` with `WM_KEYDOWN` and `VK_RETURN`. That is the existing Enter path into `CounterSession::activate`. One release consumes the arm, including when that release stores, because `take` runs before `press_add`. A second release finds no recorded id and returns `Step::Unchanged` with the hit still present. It does not call `press_add`. It does not assign `controller.focus`. It does not call `set_edit`. The unchanged count is that early return, not a second activation that stored nothing.

`window.add` is `(136, 64, 80, 32)`. Containment is half-open: `136 <= x < 216` and `64 <= y < 96`. `(215, 95)` is that button. `(176, 80)` is that button. The edit is `"1"`. The first release stores count 1, revision 1, result `"1"`. A second `press_add` would store count 2. The slice locks that the count stays 1. Do not round `215.9` to `216` or `95.9` to `96`. Those nearest integers fail the half-open test, so the first release would miss. Do not activate on press. The press records the id and does not call `press_add`.

Section 6 of `docs/emperor/GY-EMP-001.md`, after the saturation sentences, says: "A truncated activation consumes the arm. A second release on `window.add` does not activate." The next sentence is the allowed-paste sentence. `docs/emperor/contracts/gy-cross-parity-1.md`, after those same saturation sentences, says those same two sentences. The next sentence is "At 250 millipoints per pixel and buffer scale 1, that truncation does not scale." Those sentences match this release. The function does not multiply by 250 millipoints or by buffer scale.

## Not the first truncated release

`tests::release_truncates_toward_zero` releases `(215.9, 95.9)` once. That first truncated release stores count 1, revision 1, result `"1"`. That case has no second release. The off-button releases in that test, `(135.9, 80.0)` and `(176.0, 63.9)`, consume the arm without storing. The later `(176.0, 80.0)` in that test follows a release that did not store, and the count stays 0. This slice follows a release that did store, and the later hit is still `window.add`. Do not merge those paths.

## Not a later release after a miss

`tests::saturated_release_is_a_miss`, `tests::release_truncates_to_a_miss`, and `tests::non_finite_release_stays_inside_the_client` each follow a release that did not store. Their later `(176.0, 80.0)` releases keep count 0. The saturated release and the truncated miss are not `window.add` on the consuming release. The non-finite hit is `window.root`. This slice's consuming release did store. The second release and the later release are still `window.add`. Do not merge those paths.

## What the test locks

The test starts on Edit. Count is 0, revision is 0, edit is `"1"`, result is `"0"`.

A press at `(176.0, 80.0)` is hit `Some("window.add")`. Focus becomes Button. Count stays 0. The press does not store.

A release at `(215.9, 95.9)` is hit `Some("window.add")` and `Step::Changed`. Count is 1, revision is 1, result is `"1"`, and the edit stays `"1"`. Focus stays Button.

A second release at `(215.9, 95.9)` is hit `Some("window.add")`, not `None`, and `Step::Unchanged`. Count stays 1. Revision stays 1. Result stays `"1"`. The edit stays `"1"`. Focus stays Button. The hit is still the button. The step is not `Changed`. The count is not 2.

A later release at `(176.0, 80.0)` is hit `Some("window.add")` and `Step::Unchanged`. Count stays 1. Revision stays 1. Result stays `"1"`. The edit stays `"1"`. Focus stays Button.

The test does not read `button_id`. `PointerArm.button_id` stays private. The test sees the consumed arm only through the later releases. Do not make `button_id` public.

## Not a grab

Taking the arm on release stops a later in-button release from calling `press_add`. The field is the id recorded on a button press. `on_pointer_release` does not call a pointer-capture API. Consuming the arm is not a grab. This is not a pointer grab, not `SetCapture`, and not a `wl_pointer` grab. It does not qualify EMP-M1-25. EMP-M1-25 stays unmet. The ledger still says a scripted release check, including one Wayland present of that script, does not close that row.

## Not a boundary

This is not an authorization boundary and not a sandbox. `open_admitted` still only rejects a NIR whose capability strings fall outside the profile. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. Describing filesystem, network, subprocess, or web_content does not install Landlock or seccomp. This cycle does not call Landlock, seccomp, or a capability-bounding set. EMP-M1-11 stays unmet. This is not IME support. `on_event` still sends `Preedit`, `CommitText`, and `DeleteSurroundingText` to `ignore_ime`, which returns `Step::Unchanged`. EMP-M1-09 stays unmet.

`tests::second_truncated_release_does_not_store` calls the helpers on an admitted controller. It does not present a buffer, open a Wayland seat, or launch a compositor. A unit test is not a presented buffer, a Wayland seat, or a compositor launch. EMP-M1-25 stays unmet. EMP-M1-09 stays unmet. EMP-M1-11 stays unmet. No ledger row moves. This note does not pass any row.
