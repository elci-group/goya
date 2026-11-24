# COSMIC protocol profile

Profile `nebula.cosmic.app-profile/0.1`. Copied 19 November 2026 from `COSMIC_PROFILE` in `crates/nebula/src/protocols.rs` at base commit `6193d310603fa2668d8bde6ac3159cdec80f2ef9`. The source table is authoritative. Where this page and the source differ, the source wins and this page is stale.

The columns Interface, Min version, Serves, Fallback, and Bound by `goya-wayland` are the fields of each `row(...)` in that table, copied without change. The column Advertised is `advertised_version` from `evidence/nebula/probe-2026-11-19.json` (`protocols.entries`), one Pop!_OS 24.04 COSMIC host, `cosmic-comp` package `0.1~1776708607~24.04~21679f2`. That column is protocol presence. It is not behaviour (GY-EMP-001 §4.3). No window was opened by that probe.

The table has 31 rows: 4 required, 19 optional, 8 COSMIC-private. On the recorded host the required rows were 4 of 4, the optional rows 15 of 19, and all 8 private rows were present.

## Groups

- Required. A missing required global makes `nebula probe` exit 4. `goya-wayland` window creation fails without `wl_compositor`, `wl_shm`, or `xdg_wm_base` (`crates/goya-wayland/src/lib.rs`, around line 1373). Without `wl_seat` it does not fail, but the window receives no input, so the profile treats the seat as required.
- Optional. Absence, or a version below the minimum, puts the documented fallback in effect. The probe records `fallback_in_effect` per row.
- COSMIC-private, not used. Interfaces in the `zcosmic_*` and `cosmic_*` namespaces. They serve the COSMIC panel, dock, applets, and settings. Applications SHALL NOT bind them (GY-NEB-001 §5). Their fallback is "not applicable: applications must not depend on COSMIC shell protocols". A private global that the table does not name is still classified here.
- Unlisted. Every other global. Recorded, not used.

The minimum version is the version `goya-wayland` binds for the rows it uses. Other rows name the version that introduced the behaviour the row describes (source comment on `COSMIC_PROFILE`). "Bound by `goya-wayland`" is a yes or no field. Since commit f85db7a, `goya-wayland` binds `xdg_wm_base` at `min(advertised, 7)`, which was 7 on this host. It binds the other rows marked yes at version 1 (`crates/goya-wayland/src/lib.rs`, around line 1352). The source comment that says every bound row is at version 1 is stale (NEB-GAP-21).

## Required

| Interface | Min version | Serves | Fallback when absent or below minimum | Bound by `goya-wayland` | Advertised on 19 November 2026 |
| --- | --- | --- | --- | --- | --- |
| `wl_compositor` | 1 | surface creation | none: the host cannot create a surface | yes | 5 |
| `wl_shm` | 1 | software presentation (shared-memory frames) | none: the only presentation path is wl_shm | yes | 2 |
| `wl_seat` | 1 | pointer and keyboard input | none: an application without a seat receives no input | yes | 9 |
| `xdg_wm_base` | 1 | xdg_toplevel window creation, title, configure | none: no toplevel can be created | yes | 7 |

## Optional

| Interface | Min version | Serves | Fallback when absent or below minimum | Bound by `goya-wayland` | Advertised on 19 November 2026 |
| --- | --- | --- | --- | --- | --- |
| `wl_data_device_manager` | 1 | CLIPBOARD copy and paste under the scene's clipboard policy | in-process clipboard only; nothing crosses to other applications | yes | 3 |
| `zwp_text_input_manager_v3` | 1 | input-method text input (disabled unless a caller enables it; IME is not qualified, EMP-M1-09) | keyboard events only; no preedit or commit | yes | 1 |
| `wl_output` | 2 | output scale and geometry | assume buffer scale 1 | no | 4 |
| `zxdg_output_manager_v1` | 3 | logical output size and name | use wl_output geometry only | no | 3 |
| `wp_fractional_scale_manager_v1` | 1 | fractional scale (not qualified, EMP-M1-17) | integer buffer scale | no | 1 |
| `wp_viewporter` | 1 | buffer-to-surface scaling for fractional scale | integer buffer scale | no | 1 |
| `zxdg_decoration_manager_v1` | 1 | server-side decoration negotiation | compositor default decoration mode; the host draws none | no | 1 |
| `xdg_activation_v1` | 1 | focus activation requests | no activation request; the compositor decides focus (GY-EMP-001 §4.2) | no | 1 |
| `wp_cursor_shape_manager_v1` | 1 | named cursor shapes | compositor default cursor | no | 2 |
| `wp_presentation` | 1 | presentation timing feedback | frame callbacks only; no presented-at timestamps | no | 2 |
| `wl_subcompositor` | 1 | subsurfaces | single surface per toplevel | no | 1 |
| `zwp_linux_dmabuf_v1` | 3 | GPU buffer import (Vulkan is not started, EMP-M1-08) | wl_shm software buffers | no | 5 |
| `zwp_primary_selection_device_manager_v1` | 1 | PRIMARY selection (not used by Emperor) | no PRIMARY selection | no | 1 |
| `zwp_idle_inhibit_manager_v1` | 1 | idle inhibition while a surface is visible | no idle inhibition | no | 1 |
| `zxdg_exporter_v2` | 1 | exported toplevel handles for portal dialog parenting | portal dialogs are not parented | no | 1 |
| `xdg_toplevel_icon_manager_v1` | 1 | per-window icon | icon resolved by the compositor from the app_id and desktop entry | no | absent |
| `xdg_wm_dialog_v1` | 1 | modal dialog hint | xdg_toplevel.set_parent only; no modal hint | no | absent |
| `xdg_toplevel_tag_manager_v1` | 1 | stable toplevel tags for session restore | no toplevel tag | no | absent |
| `wp_color_manager_v1` | 1 | colour management (HDR and wide gamut are not qualified, EMP-M1-26) | sRGB assumed | no | absent |

## COSMIC-private, not used

| Interface | Serves (in COSMIC) | Advertised on 19 November 2026 |
| --- | --- | --- |
| `cosmic_a11y_manager_v1` | COSMIC shell accessibility settings (screen reader, magnifier toggles) | 2 |
| `cosmic_corner_radius_manager_v1` | libcosmic window corner-radius hints to the compositor | 1 |
| `zcosmic_output_manager_v1` | COSMIC output configuration extensions for cosmic-settings | 3 |
| `zcosmic_overlap_notify_v1` | panel autohide overlap notifications | 1 |
| `zcosmic_toplevel_info_v1` | panel and dock toplevel list | 3 |
| `zcosmic_toplevel_manager_v1` | panel and dock toplevel control | 4 |
| `zcosmic_workspace_image_capture_source_manager_v1` | workspace overview thumbnails | 1 |
| `zcosmic_workspace_manager_v2` | panel workspace indicator | 2 |

## Unlisted globals on the recorded host

These 28 globals were advertised and have no profile row. They are recorded in `protocols.unlisted` and are not used. Most are shell, privileged, capture, or legacy protocols. A future profile revision may add rows for the ones an application needs, for example `zwp_pointer_gestures_v1` for NEB-004 (NEB-GAP-25).

`ext_data_control_manager_v1` v1, `ext_foreign_toplevel_image_capture_source_manager_v1` v1, `ext_foreign_toplevel_list_v1` v1, `ext_idle_notifier_v1` v2, `ext_image_copy_capture_manager_v1` v1, `ext_output_image_capture_source_manager_v1` v1, `ext_session_lock_manager_v1` v1, `ext_workspace_manager_v1` v1, `org_kde_kwin_server_decoration_manager` v1, `wl_drm` v2, `wl_fixes` v1, `wp_alpha_modifier_v1` v1, `wp_drm_lease_device_v1` v1, `wp_linux_drm_syncobj_manager_v1` v1, `wp_security_context_manager_v1` v1, `wp_single_pixel_buffer_manager_v1` v1, `zwlr_data_control_manager_v1` v2, `zwlr_layer_shell_v1` v5, `zwlr_output_manager_v1` v4, `zwlr_output_power_manager_v1` v1, `zwp_input_method_manager_v2` v1, `zwp_keyboard_shortcuts_inhibit_manager_v1` v1, `zwp_pointer_constraints_v1` v1, `zwp_pointer_gestures_v1` v3, `zwp_relative_pointer_manager_v1` v1, `zwp_tablet_manager_v2` v1, `zwp_virtual_keyboard_manager_v1` v1, `zxdg_importer_v2` v1

## Regenerating this page

Read `COSMIC_PROFILE` in `crates/nebula/src/protocols.rs` and `protocols.entries` and `protocols.unlisted` in the probe record. Do not add a row here that the source does not have. A new probe on another date SHALL be added as a new Advertised column with its own date and record path, not by overwriting this one.
