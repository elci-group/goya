# Keyboard leave security

Keyboard leave drops stale modifiers and does not keep Control held.

Reviewed the `wl_keyboard` match (`Modifiers`, `Key`, `Leave`) and `shortcut_from_key` in `crates/goya-wayland/src/lib.rs`, against the sandbox paragraph in `docs/emperor/threat-model.md`. This note does not change the gap ledger and does not pass any row.

## Leave

`Leave` sets `shift`, `ctrl`, and `alt` to false. When an xkb state exists, it calls `update_mask(0, 0, 0, 0, 0, stored_group)`. Depressed, latched, and locked modifiers drop. The layout group remains. The keymap stays. `keyboard_enter_serial` stays. `last_pointer` stays. Leave still resets compose when a compose state exists.

`Enter` is what stores `keyboard_enter_serial`. `Keymap` replaces `xkb_state` only when a new map is accepted. Keyboard leave writes neither. Pointer leave is what sets `last_pointer` to `None`. A keyboard leave does not drop the stored point and does not change the button arm.

Keeping the group does not keep Shift, Control, Alt, or a lock. The three modifier words in that `update_mask` are zero. The bool `ctrl` is only the depressed Control bit (`mods_depressed & 0x4`). Leave clears that bool as well. A later key does not set it.

## Cut

`shortcut_from_key` is unchanged. Evdev 45 is `Shortcut::Cut` only when ctrl is true and shift is false. After leave both are false, so 45 is not `Shortcut::Cut`. `Key` still calls `handle_key_press`, which passes those bools into the helper. Ctrl becomes true again only when a later `Modifiers` event sets it from `mods_depressed & 0x4`. Until that event, 45 is not Cut.

The same clear drops shift. The helper still returns `None` when ctrl is false, except Shift+Insert, and that chord needs shift. Leave does not edit the helper.

## Keymap-free path

The keymap-free path still ignores ctrl. `key_chars` with no xkb state does not consult ctrl. That GY-ZCODE-004 asymmetry is not fixed. With an xkb state, ctrl still suppresses `KeyChar`. Leave does not change that split.

## Not GetKeyState and not a grab

This is not GetKeyState. It does not read a live key bitmap. It is not a keyboard grab and not a pointer grab. The enter serial that stays is not a selection grant. EMP-M1-25 stays unmet.

## No sandbox

No sandbox is installed. The threat model states that a manifest which says `filesystem` is `none`, and network, subprocess, and web content are false, does not confine a process. This cycle does not call Landlock, seccomp, or a capability-bounding set, and it must not report those as enabled. The running `App` is not confined, because no sandbox is attempted. Failure to establish a sandbox cannot be observed because no sandbox is attempted. That is an honest gap, not a pass. EMP-M1-11 stays unmet.

## Not a pass

This note does not mark a ledger row passed. A unit test is not a seat. EMP-M1-11 stays unmet. EMP-M1-25 stays unmet.
