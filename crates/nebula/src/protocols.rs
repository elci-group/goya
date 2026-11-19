//! Wayland global listing and the COSMIC application profile.
//!
//! [`list_globals`] connects to the compositor named by `WAYLAND_DISPLAY`,
//! reads the registry, binds each `wl_seat` only to read its capability
//! bits, and disconnects. It creates no surface and opens no window.
//!
//! [`classify`] is a pure function from a list of globals to a
//! [`ProtocolReport`]. Advertisement is not behaviour (GY-EMP-001 §4.3): a
//! present global says nothing about whether the compositor implements it
//! correctly, and a missing optional global maps to the documented fallback
//! in [`COSMIC_PROFILE`].

use serde::Serialize;
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

/// Identifier of the profile table below.
pub const PROFILE_ID: &str = "nebula.cosmic.app-profile/0.1";

/// One advertised global.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Global {
    pub interface: String,
    pub version: u32,
}

impl Global {
    pub fn new(interface: &str, version: u32) -> Self {
        Global {
            interface: interface.to_string(),
            version,
        }
    }
}

/// How the COSMIC application profile treats an interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Need {
    /// The Goya Wayland host cannot run without it.
    Required,
    /// Absence maps to the documented fallback.
    Optional,
    /// COSMIC shell protocol, observed but not used by applications.
    CosmicPrivateNotUsed,
}

/// One row of the profile table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileEntry {
    pub interface: &'static str,
    pub need: Need,
    /// Minimum version Nebula's profile asks for. 0 for private rows.
    pub min_version: u32,
    /// The Goya feature the interface serves.
    pub serves: &'static str,
    /// Documented behaviour when the interface is absent or too old.
    pub fallback: &'static str,
    /// True when `goya-wayland` binds the interface today (all at version 1).
    pub bound_by_goya_wayland: bool,
}

const fn row(
    interface: &'static str,
    need: Need,
    min_version: u32,
    serves: &'static str,
    fallback: &'static str,
    bound_by_goya_wayland: bool,
) -> ProfileEntry {
    ProfileEntry {
        interface,
        need,
        min_version,
        serves,
        fallback,
        bound_by_goya_wayland,
    }
}

use Need::{CosmicPrivateNotUsed as Private, Optional, Required};

const PRIVATE_FALLBACK: &str =
    "not applicable: applications must not depend on COSMIC shell protocols";

/// The COSMIC application profile. Minimum versions are the versions
/// `goya-wayland` binds (1) for the rows it uses; other rows name the version
/// that introduced the behaviour the row describes.
pub const COSMIC_PROFILE: &[ProfileEntry] = &[
    // Required: the host fails without these.
    row("wl_compositor", Required, 1, "surface creation", "none: the host cannot create a surface", true),
    row("wl_shm", Required, 1, "software presentation (shared-memory frames)", "none: the only presentation path is wl_shm", true),
    row("wl_seat", Required, 1, "pointer and keyboard input", "none: an application without a seat receives no input", true),
    row("xdg_wm_base", Required, 1, "xdg_toplevel window creation, title, configure", "none: no toplevel can be created", true),
    // Optional: documented fallbacks.
    row("wl_data_device_manager", Optional, 1, "CLIPBOARD copy and paste under the scene's clipboard policy", "in-process clipboard only; nothing crosses to other applications", true),
    row("zwp_text_input_manager_v3", Optional, 1, "input-method text input (disabled unless a caller enables it; IME is not qualified, EMP-M1-09)", "keyboard events only; no preedit or commit", true),
    row("wl_output", Optional, 2, "output scale and geometry", "assume buffer scale 1", false),
    row("zxdg_output_manager_v1", Optional, 3, "logical output size and name", "use wl_output geometry only", false),
    row("wp_fractional_scale_manager_v1", Optional, 1, "fractional scale (not qualified, EMP-M1-17)", "integer buffer scale", false),
    row("wp_viewporter", Optional, 1, "buffer-to-surface scaling for fractional scale", "integer buffer scale", false),
    row("zxdg_decoration_manager_v1", Optional, 1, "server-side decoration negotiation", "compositor default decoration mode; the host draws none", false),
    row("xdg_activation_v1", Optional, 1, "focus activation requests", "no activation request; the compositor decides focus (GY-EMP-001 §4.2)", false),
    row("wp_cursor_shape_manager_v1", Optional, 1, "named cursor shapes", "compositor default cursor", false),
    row("wp_presentation", Optional, 1, "presentation timing feedback", "frame callbacks only; no presented-at timestamps", false),
    row("wl_subcompositor", Optional, 1, "subsurfaces", "single surface per toplevel", false),
    row("zwp_linux_dmabuf_v1", Optional, 3, "GPU buffer import (Vulkan is not started, EMP-M1-08)", "wl_shm software buffers", false),
    row("zwp_primary_selection_device_manager_v1", Optional, 1, "PRIMARY selection (not used by Emperor)", "no PRIMARY selection", false),
    row("zwp_idle_inhibit_manager_v1", Optional, 1, "idle inhibition while a surface is visible", "no idle inhibition", false),
    row("zxdg_exporter_v2", Optional, 1, "exported toplevel handles for portal dialog parenting", "portal dialogs are not parented", false),
    row("xdg_toplevel_icon_manager_v1", Optional, 1, "per-window icon", "icon resolved by the compositor from the app_id and desktop entry", false),
    row("xdg_wm_dialog_v1", Optional, 1, "modal dialog hint", "xdg_toplevel.set_parent only; no modal hint", false),
    row("xdg_toplevel_tag_manager_v1", Optional, 1, "stable toplevel tags for session restore", "no toplevel tag", false),
    row("wp_color_manager_v1", Optional, 1, "colour management (HDR and wide gamut are not qualified, EMP-M1-26)", "sRGB assumed", false),
    // COSMIC private: observed, not used. They serve the panel and applets.
    row("cosmic_a11y_manager_v1", Private, 0, "COSMIC shell accessibility settings (screen reader, magnifier toggles)", PRIVATE_FALLBACK, false),
    row("cosmic_corner_radius_manager_v1", Private, 0, "libcosmic window corner-radius hints to the compositor", PRIVATE_FALLBACK, false),
    row("zcosmic_output_manager_v1", Private, 0, "COSMIC output configuration extensions for cosmic-settings", PRIVATE_FALLBACK, false),
    row("zcosmic_overlap_notify_v1", Private, 0, "panel autohide overlap notifications", PRIVATE_FALLBACK, false),
    row("zcosmic_toplevel_info_v1", Private, 0, "panel and dock toplevel list", PRIVATE_FALLBACK, false),
    row("zcosmic_toplevel_manager_v1", Private, 0, "panel and dock toplevel control", PRIVATE_FALLBACK, false),
    row("zcosmic_workspace_image_capture_source_manager_v1", Private, 0, "workspace overview thumbnails", PRIVATE_FALLBACK, false),
    row("zcosmic_workspace_manager_v2", Private, 0, "panel workspace indicator", PRIVATE_FALLBACK, false),
];

/// True when an interface name belongs to COSMIC's private namespace.
pub fn is_cosmic_private(interface: &str) -> bool {
    interface.starts_with("zcosmic_") || interface.starts_with("cosmic_")
}

/// Presence of one profile row on the observed list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Presence {
    Advertised,
    Absent,
    /// Advertised below the profile's minimum version.
    BelowMinimum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Classified {
    pub interface: String,
    pub need: Need,
    pub min_version: u32,
    pub advertised_version: Option<u32>,
    pub presence: Presence,
    pub serves: &'static str,
    pub fallback: &'static str,
    pub bound_by_goya_wayland: bool,
    /// Whether the documented fallback is in effect on this list.
    pub fallback_in_effect: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub required_total: usize,
    pub required_advertised: usize,
    pub optional_total: usize,
    pub optional_advertised: usize,
    pub cosmic_private_observed: usize,
    pub unlisted_observed: usize,
    pub globals_observed: usize,
}

/// Capability bits of one `wl_seat`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SeatCapabilities {
    pub pointer: bool,
    pub keyboard: bool,
    pub touch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtocolReport {
    pub profile: &'static str,
    /// One entry per profile row, plus any COSMIC-private global seen that
    /// the table does not name.
    pub entries: Vec<Classified>,
    /// Observed globals outside the profile (shell, privileged or legacy
    /// protocols). Recorded, not used.
    pub unlisted: Vec<Global>,
    /// Required rows that are absent or below minimum.
    pub required_missing: Vec<String>,
    pub counts: Counts,
    /// The full observed list, sorted by interface.
    pub observed: Vec<Global>,
    /// Seat capabilities, when a live probe read them. `None` for a pure
    /// classification of a fixed list.
    pub seats: Option<Vec<SeatCapabilities>>,
    pub statement: &'static str,
}

impl ProtocolReport {
    /// Highest advertised version of an interface on the observed list.
    pub fn advertised(&self, interface: &str) -> Option<u32> {
        self.observed
            .iter()
            .filter(|g| g.interface == interface)
            .map(|g| g.version)
            .max()
    }

    pub fn required_ok(&self) -> bool {
        self.required_missing.is_empty()
    }
}

/// Classify a list of globals against [`COSMIC_PROFILE`]. Pure.
pub fn classify(globals: &[Global]) -> ProtocolReport {
    let mut observed = globals.to_vec();
    observed.sort();
    let max_version = |iface: &str| {
        observed
            .iter()
            .filter(|g| g.interface == iface)
            .map(|g| g.version)
            .max()
    };

    let mut entries = Vec::new();
    let mut counts = Counts {
        globals_observed: observed.len(),
        ..Counts::default()
    };
    let mut required_missing = Vec::new();

    for e in COSMIC_PROFILE {
        let v = max_version(e.interface);
        let presence = match v {
            None => Presence::Absent,
            Some(v) if v < e.min_version => Presence::BelowMinimum,
            Some(_) => Presence::Advertised,
        };
        let ok = presence == Presence::Advertised;
        match e.need {
            Need::Required => {
                counts.required_total += 1;
                if ok {
                    counts.required_advertised += 1;
                } else {
                    required_missing.push(e.interface.to_string());
                }
            }
            Need::Optional => {
                counts.optional_total += 1;
                counts.optional_advertised += usize::from(ok);
            }
            Need::CosmicPrivateNotUsed => {
                counts.cosmic_private_observed += usize::from(v.is_some());
            }
        }
        entries.push(Classified {
            interface: e.interface.to_string(),
            need: e.need,
            min_version: e.min_version,
            advertised_version: v,
            presence,
            serves: e.serves,
            fallback: e.fallback,
            bound_by_goya_wayland: e.bound_by_goya_wayland,
            fallback_in_effect: e.need != Need::CosmicPrivateNotUsed && !ok,
        });
    }

    let mut unlisted = Vec::new();
    let mut seen_extra = std::collections::BTreeSet::new();
    for g in &observed {
        if COSMIC_PROFILE.iter().any(|e| e.interface == g.interface) {
            continue;
        }
        if is_cosmic_private(&g.interface) {
            if seen_extra.insert(g.interface.clone()) {
                counts.cosmic_private_observed += 1;
                entries.push(Classified {
                    interface: g.interface.clone(),
                    need: Need::CosmicPrivateNotUsed,
                    min_version: 0,
                    advertised_version: max_version(&g.interface),
                    presence: Presence::Advertised,
                    serves: "COSMIC shell protocol not named in the profile table",
                    fallback: PRIVATE_FALLBACK,
                    bound_by_goya_wayland: false,
                    fallback_in_effect: false,
                });
            }
        } else if !unlisted.iter().any(|u: &Global| u.interface == g.interface) {
            unlisted.push(g.clone());
        }
    }
    counts.unlisted_observed = unlisted.len();

    ProtocolReport {
        profile: PROFILE_ID,
        entries,
        unlisted,
        required_missing,
        counts,
        observed,
        seats: None,
        statement:
            "protocol advertisement only; no behaviour is inferred from presence (GY-EMP-001 §4.3)",
    }
}

/// Failure to read the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    Connect(String),
    Roundtrip(String),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::Connect(e) => write!(f, "cannot connect to the Wayland display: {e}"),
            ProbeError::Roundtrip(e) => write!(f, "Wayland roundtrip failed: {e}"),
        }
    }
}

impl std::error::Error for ProbeError {}

#[derive(Default)]
struct RegistryState {
    globals: Vec<(u32, Global)>,
    seats: Vec<SeatCapabilities>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for RegistryState {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => state.globals.push((name, Global { interface, version })),
            wl_registry::Event::GlobalRemove { name } => state.globals.retain(|(n, _)| *n != name),
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, usize> for RegistryState {
    fn event(
        state: &mut Self,
        _: &wl_seat::WlSeat,
        event: wl_seat::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(c),
        } = event
        {
            if let Some(seat) = state.seats.get_mut(*index) {
                *seat = SeatCapabilities {
                    pointer: c.contains(wl_seat::Capability::Pointer),
                    keyboard: c.contains(wl_seat::Capability::Keyboard),
                    touch: c.contains(wl_seat::Capability::Touch),
                };
            }
        }
    }
}

/// Live globals and seat capabilities from `WAYLAND_DISPLAY`.
pub struct LiveRegistry {
    pub globals: Vec<Global>,
    pub seats: Vec<SeatCapabilities>,
}

/// Connect, read the registry, read seat capabilities, disconnect.
///
/// Binds `wl_seat` (version 1) and nothing else. No surface is created.
pub fn list_globals() -> Result<LiveRegistry, ProbeError> {
    let conn = Connection::connect_to_env().map_err(|e| ProbeError::Connect(e.to_string()))?;
    let mut queue = conn.new_event_queue::<RegistryState>();
    let qh = queue.handle();
    let registry = conn.display().get_registry(&qh, ());
    let mut state = RegistryState::default();
    queue
        .roundtrip(&mut state)
        .map_err(|e| ProbeError::Roundtrip(e.to_string()))?;

    let seat_names: Vec<u32> = state
        .globals
        .iter()
        .filter(|(_, g)| g.interface == "wl_seat")
        .map(|(n, _)| *n)
        .collect();
    let mut seats = Vec::new();
    for (i, name) in seat_names.iter().enumerate() {
        state.seats.push(SeatCapabilities::default());
        seats.push(registry.bind::<wl_seat::WlSeat, usize, RegistryState>(*name, 1, &qh, i));
    }
    if !seats.is_empty() {
        queue
            .roundtrip(&mut state)
            .map_err(|e| ProbeError::Roundtrip(e.to_string()))?;
    }
    Ok(LiveRegistry {
        globals: state.globals.into_iter().map(|(_, g)| g).collect(),
        seats: state.seats,
    })
}

/// Live probe: list globals and classify them.
pub fn probe() -> Result<ProtocolReport, ProbeError> {
    let live = list_globals()?;
    let mut report = classify(&live.globals);
    report.seats = Some(live.seats);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list `nebula probe` read from the Pop!_OS 24.04 COSMIC host
    /// (cosmic-comp 0.1~1776708607~24.04~21679f2) on 2026-11-19.
    pub(crate) fn pop_cosmic_2026_11_19() -> Vec<Global> {
        [
            ("cosmic_a11y_manager_v1", 2),
            ("cosmic_corner_radius_manager_v1", 1),
            ("ext_data_control_manager_v1", 1),
            ("ext_foreign_toplevel_image_capture_source_manager_v1", 1),
            ("ext_foreign_toplevel_list_v1", 1),
            ("ext_idle_notifier_v1", 2),
            ("ext_image_copy_capture_manager_v1", 1),
            ("ext_output_image_capture_source_manager_v1", 1),
            ("ext_session_lock_manager_v1", 1),
            ("ext_workspace_manager_v1", 1),
            ("org_kde_kwin_server_decoration_manager", 1),
            ("wl_compositor", 5),
            ("wl_data_device_manager", 3),
            ("wl_drm", 2),
            ("wl_fixes", 1),
            ("wl_output", 4),
            ("wl_seat", 9),
            ("wl_shm", 2),
            ("wl_subcompositor", 1),
            ("wp_alpha_modifier_v1", 1),
            ("wp_cursor_shape_manager_v1", 2),
            ("wp_drm_lease_device_v1", 1),
            ("wp_fractional_scale_manager_v1", 1),
            ("wp_linux_drm_syncobj_manager_v1", 1),
            ("wp_presentation", 2),
            ("wp_security_context_manager_v1", 1),
            ("wp_single_pixel_buffer_manager_v1", 1),
            ("wp_viewporter", 1),
            ("xdg_activation_v1", 1),
            ("xdg_wm_base", 7),
            ("zcosmic_output_manager_v1", 3),
            ("zcosmic_overlap_notify_v1", 1),
            ("zcosmic_toplevel_info_v1", 3),
            ("zcosmic_toplevel_manager_v1", 4),
            ("zcosmic_workspace_image_capture_source_manager_v1", 1),
            ("zcosmic_workspace_manager_v2", 2),
            ("zwlr_data_control_manager_v1", 2),
            ("zwlr_layer_shell_v1", 5),
            ("zwlr_output_manager_v1", 4),
            ("zwlr_output_power_manager_v1", 1),
            ("zwp_idle_inhibit_manager_v1", 1),
            ("zwp_input_method_manager_v2", 1),
            ("zwp_keyboard_shortcuts_inhibit_manager_v1", 1),
            ("zwp_linux_dmabuf_v1", 5),
            ("zwp_pointer_constraints_v1", 1),
            ("zwp_pointer_gestures_v1", 3),
            ("zwp_primary_selection_device_manager_v1", 1),
            ("zwp_relative_pointer_manager_v1", 1),
            ("zwp_tablet_manager_v2", 1),
            ("zwp_text_input_manager_v3", 1),
            ("zwp_virtual_keyboard_manager_v1", 1),
            ("zxdg_decoration_manager_v1", 1),
            ("zxdg_exporter_v2", 1),
            ("zxdg_importer_v2", 1),
            ("zxdg_output_manager_v1", 3),
        ]
        .iter()
        .map(|(i, v)| Global::new(i, *v))
        .collect()
    }

    fn entry<'a>(r: &'a ProtocolReport, iface: &str) -> &'a Classified {
        r.entries.iter().find(|e| e.interface == iface).unwrap()
    }

    #[test]
    fn profile_table_is_well_formed() {
        let mut seen = std::collections::BTreeSet::new();
        for e in COSMIC_PROFILE {
            assert!(seen.insert(e.interface), "duplicate {}", e.interface);
            assert!(!e.serves.is_empty() && !e.fallback.is_empty());
            assert_eq!(
                e.need == Need::CosmicPrivateNotUsed,
                is_cosmic_private(e.interface),
                "{}: private rows and the private namespace must coincide",
                e.interface
            );
            if e.need == Need::CosmicPrivateNotUsed {
                assert_eq!(e.min_version, 0);
                assert!(!e.bound_by_goya_wayland);
            } else {
                assert!(e.min_version >= 1);
            }
        }
        let required: Vec<_> = COSMIC_PROFILE
            .iter()
            .filter(|e| e.need == Need::Required)
            .map(|e| e.interface)
            .collect();
        assert_eq!(
            required,
            ["wl_compositor", "wl_shm", "wl_seat", "xdg_wm_base"]
        );
    }

    #[test]
    fn recorded_cosmic_list_meets_required_and_reports_fallbacks() {
        let r = classify(&pop_cosmic_2026_11_19());
        assert!(r.required_ok());
        assert_eq!(r.counts.required_total, 4);
        assert_eq!(r.counts.required_advertised, 4);
        assert_eq!(r.counts.cosmic_private_observed, 8);
        assert_eq!(r.counts.globals_observed, 55);
        // Absent on this host: icon, dialog, tag, colour management.
        for iface in [
            "xdg_toplevel_icon_manager_v1",
            "xdg_wm_dialog_v1",
            "xdg_toplevel_tag_manager_v1",
            "wp_color_manager_v1",
        ] {
            let e = entry(&r, iface);
            assert_eq!(e.presence, Presence::Absent, "{iface}");
            assert!(e.fallback_in_effect);
        }
        assert_eq!(r.counts.optional_advertised, r.counts.optional_total - 4);
        let private = entry(&r, "zcosmic_toplevel_info_v1");
        assert_eq!(private.need, Need::CosmicPrivateNotUsed);
        assert_eq!(private.advertised_version, Some(3));
        assert!(!private.fallback_in_effect);
        assert!(r
            .unlisted
            .iter()
            .any(|g| g.interface == "zwlr_layer_shell_v1"));
        assert!(!r.unlisted.iter().any(|g| is_cosmic_private(&g.interface)));
    }

    #[test]
    fn missing_required_is_reported() {
        let globals = vec![Global::new("wl_compositor", 6), Global::new("wl_shm", 1)];
        let r = classify(&globals);
        assert!(!r.required_ok());
        assert_eq!(r.required_missing, ["wl_seat", "xdg_wm_base"]);
        assert_eq!(r.counts.optional_advertised, 0);
        assert_eq!(r.counts.cosmic_private_observed, 0);
    }

    #[test]
    fn below_minimum_counts_as_fallback() {
        let mut g = pop_cosmic_2026_11_19();
        g.retain(|g| g.interface != "wl_output");
        g.push(Global::new("wl_output", 1));
        let r = classify(&g);
        let e = entry(&r, "wl_output");
        assert_eq!(e.presence, Presence::BelowMinimum);
        assert!(e.fallback_in_effect);
    }

    #[test]
    fn unknown_cosmic_private_global_is_classified_not_used() {
        let mut g = pop_cosmic_2026_11_19();
        g.push(Global::new("zcosmic_future_thing_v1", 1));
        let r = classify(&g);
        let e = entry(&r, "zcosmic_future_thing_v1");
        assert_eq!(e.need, Need::CosmicPrivateNotUsed);
        assert_eq!(r.counts.cosmic_private_observed, 9);
    }

    #[test]
    fn duplicate_globals_use_highest_version() {
        let g = vec![Global::new("wl_output", 1), Global::new("wl_output", 4)];
        let r = classify(&g);
        assert_eq!(entry(&r, "wl_output").advertised_version, Some(4));
        assert_eq!(r.advertised("wl_output"), Some(4));
    }

    #[test]
    fn classification_is_order_independent() {
        let mut g = pop_cosmic_2026_11_19();
        let a = classify(&g);
        g.reverse();
        assert_eq!(a, classify(&g));
    }
}
