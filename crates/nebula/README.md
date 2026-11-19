# Nebula

Nebula is the COSMIC desktop profile for Emperor. It specialises Emperor and does not duplicate it: there is no Nebula lowering, codegen, window host or scene language. Version `0.1.8` reads the session, the Wayland registry and the COSMIC theme files, and writes a probe record. It does not open a window.

```
cargo run -p nebula --offline -- probe --json --out evidence/nebula/probe-$(date -u +%F).json
cargo run -p nebula --offline -- theme
```

`probe` checks that `XDG_CURRENT_DESKTOP` has a `COSMIC` component (case-insensitive), `XDG_SESSION_TYPE` is `wayland` and `WAYLAND_DISPLAY` is set. It reads the `cosmic-comp` version with `dpkg-query -W -f='${Version}' cosmic-comp` and records `unknown` with the reason when `dpkg-query` is missing or fails. That is the installed package version; Nebula does not check that it is the running compositor. An X11 `DISPLAY` in a Wayland session is recorded as Xwayland and never counts toward COSMIC qualification.

It then connects to the Wayland display, reads the registry, binds each `wl_seat` at version 1 only to read its capability bits, and disconnects. No surface is created. The globals are classified by a pure function against the profile table in `src/protocols.rs` (`nebula.cosmic.app-profile/0.1`). `wl_compositor`, `wl_shm`, `wl_seat` and `xdg_wm_base` are required. Every optional row names the Goya feature it serves, a minimum version, whether `goya-wayland` binds it today, and the fallback when it is absent. `zcosmic_*` and `cosmic_*` globals are recorded as COSMIC-private and not used: they serve the panel, dock and applets, and application correctness must not depend on them. Other globals (layer shell, wlroots output management, data control and so on) are listed as unlisted and not used. A present global is not evidence that the compositor implements it correctly (GY-EMP-001 §4.3).

`theme` reads `<config>/cosmic/com.system76.CosmicTheme.{Dark,Light}/v1/` and the mode file, where `<config>` is `$XDG_CONFIG_HOME`, else `~/.config`, or `--config-root DIR`. Only format `v1` is read. Any other requested version is refused, and a config with no `v1` directory is reported with the versions it does have. Unknown fields are ignored. A missing or unparsable key degrades that key only and is listed. Nothing is written.

COSMIC colours are absolute and Chroma themes are relational, so the mapping fills Chroma's `Palette` slots directly and does not synthesise relations: `bg` from `background.base`, `fg` from `background.on`, `accent` from `accent.base`, `border` from `background.divider`, `ok` from `success.base`, `err` from `destructive.base`. `muted` uses Chroma's 55% blend of `fg` toward `bg`, since COSMIC has no such key. Warning, text on accent, and the primary and secondary containers have no `Palette` slot and are carried as extras. The accent is also given as an OKLCH anchor. Contrast is measured against AA, or AAA when COSMIC high contrast is on, and reported, not enforced. Corner radii, spacing, gaps, the active hint width and the frosted flag go to `CosmicGeometry`; nothing in Goya consumes them yet.

The Caprichio selection keys five draft catalog entries on generic Wayland interfaces: keyboard, pointer and touch from seat capabilities, `display.surfaces` from `wl_output`, and `display.color_profile` from `wp_color_manager_v1`, which is absent on the recorded host. None of them is COSMIC-specific, and none is an implementation. A selection only says the protocol basis is present.

Exit codes: 0 ok, 2 usage, 3 not a COSMIC Wayland session or the display did not answer (`BLOCKED`, the report is still written), 4 a required global is missing. `theme` exits 1 when no theme can be read.

## Not claimed

`evidence/nebula/probe-2026-11-19.json` is protocol presence on one host, Pop!_OS 24.04 with `cosmic-comp 0.1~1776708607~24.04~21679f2`. No window was opened. It is not a COSMIC qualification run and not a release qualification. EMP-M1-07, the compositor matrix, stays `BLOCKED`: a COSMIC row still needs a real Emperor window with the input and present checks from EMP-M1 on a pinned compositor version.

Not done in this crate: notifications, portal appearance and other freedesktop D-Bus integration, generic xdg-shell state, and proportional layout (other crates own these); applying the theme to Emperor's paint; tiling behaviour; fractional scale; IME; accessibility. Theme format `v2` is not read, though the recorded host has an empty `v2` directory next to `v1`.
