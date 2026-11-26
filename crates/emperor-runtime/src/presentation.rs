//! Desktop presentation for the admitted counter: paint colours and resize
//! policy.
//!
//! Presentation is not semantics. Admission, focus, hit-testing in DIP, and
//! the shared counter session are the same for every desktop profile, and
//! the headless oracle never reads anything in this module.
//!
//! - [`Presentation::GENERIC`] is the EMP-M1 behaviour: the constants the
//!   Wayland host always used and [`ResizePolicy::Fixed`] (keep the admitted
//!   DIP rectangles, resize only dirties the frame).
//! - [`Presentation::cosmic`] is the opt-in COSMIC profile (Nebula). Its
//!   colours are resolved by the compiler at build time and compiled into
//!   the binary; the running process reads no theme file and calls no
//!   service, so live theme changes are not followed. It selects
//!   [`COSMIC_RESIZE`], a proportional policy. Rendering and pointer
//!   hit-testing use the same [`goya_layout::proportional::Placement`]:
//!   a pointer position is mapped back to design space with `to_design` and
//!   then hit-tested against the unchanged admitted rectangles.

use goya_layout::proportional::{Anchor, Point, ResizePolicy, TiledEdges, Transform};
use goya_wayland::{ToplevelState, WmCapabilities, MP_PER_PX};

/// Text colour of the generic profile (EMP-M1).
pub const GENERIC_FG: [u8; 3] = [20, 20, 28];
/// Window background of the generic profile (EMP-M1).
pub const GENERIC_BG: [u8; 3] = [235, 235, 240];
/// Fill of the focused control in the generic profile (EMP-M1).
pub const GENERIC_FOCUS_FILL: u32 = 0xFFD6E4F5;
/// Glyph cell size at scale 1.
pub const GLYPH_PX: u32 = 8;

/// Resize policy of the COSMIC profile.
///
/// Scale is clamped to 0.5x..4x, one scale for both axes (letterbox), and
/// leftover space is centred on untiled axes.
pub const COSMIC_RESIZE: ResizePolicy = ResizePolicy::Proportional {
    min_scale_milli: 500,
    max_scale_milli: 4000,
    preserve_aspect: true,
    anchor: Anchor::Center,
};

/// Desktop profile a binary was built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desktop {
    Generic,
    Cosmic,
}

impl Desktop {
    pub const fn name(self) -> &'static str {
        match self {
            Desktop::Generic => "generic",
            Desktop::Cosmic => "cosmic",
        }
    }
}

/// Colours the Wayland host paints with. `0xAARRGGBB` values follow the host
/// `fills` contract (the alpha byte is ignored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paint {
    /// Glyph colour.
    pub fg: [u8; 3],
    /// Window background.
    pub bg: [u8; 3],
    /// Fill of the whole-window client title cell. `None` keeps the host default.
    pub client_fill: Option<u32>,
    /// Fill of unfocused edit, button and result cells. `None` keeps the host defaults.
    pub field_fill: Option<u32>,
    /// Fill of the focused control.
    pub focus_fill: u32,
    /// Glyph colour on the focused control. `None` keeps `fg`.
    pub focus_fg: Option<u32>,
}

impl Paint {
    /// The EMP-M1 constants.
    pub const GENERIC: Paint = Paint {
        fg: GENERIC_FG,
        bg: GENERIC_BG,
        client_fill: None,
        field_fill: None,
        focus_fill: GENERIC_FOCUS_FILL,
        focus_fg: None,
    };
}

/// Paint and resize policy for one binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    pub desktop: Desktop,
    pub paint: Paint,
    pub resize: ResizePolicy,
}

impl Presentation {
    /// EMP-M1: generic constants, fixed rectangles.
    pub const GENERIC: Presentation = Presentation {
        desktop: Desktop::Generic,
        paint: Paint::GENERIC,
        resize: ResizePolicy::Fixed,
    };

    /// COSMIC profile with colours baked at build time.
    pub const fn cosmic(paint: Paint) -> Presentation {
        Presentation {
            desktop: Desktop::Cosmic,
            paint,
            resize: COSMIC_RESIZE,
        }
    }

    /// `fixed` or `proportional`.
    pub fn resize_name(&self) -> &'static str {
        match self.resize {
            ResizePolicy::Fixed => "fixed",
            ResizePolicy::Proportional { .. } => "proportional",
        }
    }
}

impl Default for Presentation {
    fn default() -> Self {
        Presentation::GENERIC
    }
}

/// Tiled edges passed to `place`, or `None` for "no anchoring information".
///
/// COSMIC was observed to report a floating window as tiled on all four
/// edges, so all-four-tiled does not mean "inside the tiling grid". All four,
/// none, maximized and fullscreen all give `None`; the policy's anchor then
/// decides. Only a partial set of tiled edges, which does say where the
/// neighbours are, is passed through.
pub fn tiled_hint(state: &ToplevelState) -> Option<TiledEdges> {
    let edges = state.tiled;
    let all = edges.left && edges.right && edges.top && edges.bottom;
    if !edges.any() || all || state.maximized || state.fullscreen {
        return None;
    }
    Some(TiledEdges {
        left: edges.left,
        right: edges.right,
        top: edges.top,
        bottom: edges.bottom,
    })
}

/// How a surface pointer position becomes a DIP position for `hit_map`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerMap {
    /// EMP-M1: truncate surface pixels toward zero. Scale is 1.
    Fixed,
    /// Map through the transform of the frame on screen, back to design space.
    Placed(Transform),
}

impl PointerMap {
    /// DIP coordinates to hit-test against the admitted rectangles.
    pub fn dip(&self, x: f64, y: f64) -> (i32, i32) {
        match self {
            PointerMap::Fixed => (dip_from_surface_px(x), dip_from_surface_px(y)),
            PointerMap::Placed(transform) => {
                let design = transform.to_design(Point::new(surface_mp(x), surface_mp(y)));
                (dip_from_design_mp(design.x), dip_from_design_mp(design.y))
            }
        }
    }
}

/// Truncate a surface pixel toward zero into an i32 DIP.
pub(crate) fn dip_from_surface_px(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let truncated = value.trunc();
    if truncated >= f64::from(i32::MAX) {
        i32::MAX
    } else if truncated <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        truncated as i32
    }
}

/// Floor a surface pixel into layout units (`MP_PER_PX` per pixel). A
/// non-finite value maps to 0.
fn surface_mp(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    let scaled = (value * MP_PER_PX as f64).floor();
    if scaled >= i64::MAX as f64 {
        i64::MAX
    } else if scaled <= i64::MIN as f64 {
        i64::MIN
    } else {
        scaled as i64
    }
}

/// Floor design layout units into DIP.
fn dip_from_design_mp(value: i64) -> i32 {
    let dip = value.div_euclid(MP_PER_PX);
    dip.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Glyph size for a frame drawn at `transform`. Scale 1 keeps [`GLYPH_PX`].
pub(crate) fn glyph_px(transform: &Transform) -> u32 {
    let scale = transform.scale_x_milli.min(transform.scale_y_milli);
    let scaled = u64::from(GLYPH_PX) * u64::from(scale) / 1000;
    scaled.clamp(1, u64::from(u32::MAX)) as u32
}

/// What the Wayland host saw of the compositor during one windowed run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WmObservation {
    pub xdg_wm_base_version: u32,
    /// Last committed toplevel state.
    pub state: ToplevelState,
    pub capabilities: Option<WmCapabilities>,
    pub state_generation: u64,
    /// Surface size in pixels at the end of the run.
    pub size: (i32, i32),
    pub desktop: Desktop,
    pub resize: &'static str,
    /// Tiled hint passed to `place` for the last frame.
    pub tiled_hint: Option<TiledEdges>,
    /// Transform of the last presented frame (identity under `fixed`).
    pub transform: Transform,
    /// Resized events seen.
    pub resizes: u32,
}

fn edges_text(left: bool, right: bool, top: bool, bottom: bool) -> String {
    let names: Vec<&str> = [
        (left, "left"),
        (right, "right"),
        (top, "top"),
        (bottom, "bottom"),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| *name)
    .collect();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join("+")
    }
}

impl WmObservation {
    /// One `key=value` line, prefixed `wm `, printed after the report line.
    pub fn line(&self) -> String {
        let s = &self.state;
        let bounds = match s.bounds {
            Some((w, h)) => format!("{w}x{h}"),
            None => "none".to_owned(),
        };
        let caps = match self.capabilities {
            None => "not-sent".to_owned(),
            Some(c) => {
                let names: Vec<&str> = [
                    (c.window_menu, "window_menu"),
                    (c.maximize, "maximize"),
                    (c.fullscreen, "fullscreen"),
                    (c.minimize, "minimize"),
                ]
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, name)| *name)
                .collect();
                if names.is_empty() {
                    "none".to_owned()
                } else {
                    names.join("+")
                }
            }
        };
        let hint = match self.tiled_hint {
            None => "none".to_owned(),
            Some(t) => edges_text(t.left, t.right, t.top, t.bottom),
        };
        format!(
            "wm xdg_wm_base={} size={}x{} maximized={} fullscreen={} resizing={} activated={} suspended={} tiled={} constrained={} bounds={} wm_capabilities={} state_generation={} resizes={} desktop={} resize={} tiled_hint={} scale_milli={}x{} offset_mp={},{}",
            self.xdg_wm_base_version,
            self.size.0,
            self.size.1,
            s.maximized,
            s.fullscreen,
            s.resizing,
            s.activated,
            s.suspended,
            edges_text(s.tiled.left, s.tiled.right, s.tiled.top, s.tiled.bottom),
            edges_text(
                s.constrained.left,
                s.constrained.right,
                s.constrained.top,
                s.constrained.bottom
            ),
            bounds,
            caps,
            self.state_generation,
            self.resizes,
            self.desktop.name(),
            self.resize,
            hint,
            self.transform.scale_x_milli,
            self.transform.scale_y_milli,
            self.transform.offset_x,
            self.transform.offset_y,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_presentation_keeps_the_emp_m1_constants() {
        let generic = Presentation::GENERIC;
        assert_eq!(generic.desktop, Desktop::Generic);
        assert_eq!(generic.resize, ResizePolicy::Fixed);
        assert_eq!(generic.paint.fg, [20, 20, 28]);
        assert_eq!(generic.paint.bg, [235, 235, 240]);
        assert_eq!(generic.paint.focus_fill, 0xFFD6E4F5);
        assert_eq!(generic.paint.client_fill, None);
        assert_eq!(generic.paint.field_fill, None);
        assert_eq!(generic.paint.focus_fg, None);
        assert_eq!(Presentation::default(), generic);
        assert_eq!(generic.resize_name(), "fixed");
        assert_eq!(GLYPH_PX, 8);
    }

    #[test]
    fn cosmic_presentation_selects_the_proportional_policy() {
        let cosmic = Presentation::cosmic(Paint::GENERIC);
        assert_eq!(cosmic.desktop, Desktop::Cosmic);
        assert_eq!(cosmic.resize_name(), "proportional");
        assert_eq!(
            cosmic.resize,
            ResizePolicy::Proportional {
                min_scale_milli: 500,
                max_scale_milli: 4000,
                preserve_aspect: true,
                anchor: Anchor::Center,
            }
        );
    }

    #[test]
    fn all_four_tiled_is_no_anchoring_information() {
        let mut state = ToplevelState::default();
        assert_eq!(tiled_hint(&state), None);
        state.tiled.left = true;
        state.tiled.right = true;
        state.tiled.top = true;
        state.tiled.bottom = true;
        assert_eq!(tiled_hint(&state), None);
        state.tiled.right = false;
        assert_eq!(
            tiled_hint(&state),
            Some(TiledEdges {
                left: true,
                right: false,
                top: true,
                bottom: true,
            })
        );
        state.maximized = true;
        assert_eq!(tiled_hint(&state), None);
        state.maximized = false;
        state.fullscreen = true;
        assert_eq!(tiled_hint(&state), None);
    }

    #[test]
    fn fixed_pointer_map_truncates_toward_zero() {
        assert_eq!(PointerMap::Fixed.dip(1.9, -1.9), (1, -1));
        assert_eq!(PointerMap::Fixed.dip(f64::NAN, 8.0), (0, 8));
    }

    #[test]
    fn placed_identity_matches_fixed_for_non_negative_points() {
        let map = PointerMap::Placed(Transform::IDENTITY);
        for (x, y) in [(0.0, 0.0), (1.9, 7.2), (175.99, 80.0), (319.5, 159.9)] {
            assert_eq!(map.dip(x, y), PointerMap::Fixed.dip(x, y));
        }
    }

    #[test]
    fn glyph_size_follows_the_scale() {
        assert_eq!(glyph_px(&Transform::IDENTITY), 8);
        let double = Transform {
            scale_x_milli: 2000,
            scale_y_milli: 2000,
            offset_x: 0,
            offset_y: 0,
        };
        assert_eq!(glyph_px(&double), 16);
        let tiny = Transform {
            scale_x_milli: 1,
            scale_y_milli: 1,
            offset_x: 0,
            offset_y: 0,
        };
        assert_eq!(glyph_px(&tiny), 1);
    }

    #[test]
    fn wm_line_names_every_field() {
        let mut state = ToplevelState::default();
        state.activated = true;
        state.tiled.left = true;
        state.tiled.right = true;
        state.tiled.top = true;
        state.tiled.bottom = true;
        state.bounds = Some((1920, 1080));
        let observation = WmObservation {
            xdg_wm_base_version: 7,
            state,
            capabilities: None,
            state_generation: 1,
            size: (320, 160),
            desktop: Desktop::Cosmic,
            resize: "proportional",
            tiled_hint: None,
            transform: Transform::IDENTITY,
            resizes: 0,
        };
        assert_eq!(
            observation.line(),
            "wm xdg_wm_base=7 size=320x160 maximized=false fullscreen=false resizing=false activated=true suspended=false tiled=left+right+top+bottom constrained=none bounds=1920x1080 wm_capabilities=not-sent state_generation=1 resizes=0 desktop=cosmic resize=proportional tiled_hint=none scale_milli=1000x1000 offset_mp=0,0"
        );
    }
}
