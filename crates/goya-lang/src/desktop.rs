//! GSSH/4 initial desktop state (decision D16).
//!
//! Every admitted scene has one resolved desktop. Omitting `@desktop` is
//! fullscreen with the plain chrome library. `place` is the shared geometry:
//! a target that cannot apply it must refuse an authored declaration rather
//! than drop it. The browser shell measures a hidden document as at least 1
//! DIP before calling `place`; `place` itself rejects a work area below 1.

/// Largest authored window edge, in DIP.
pub const MAX_EXTENT_DIP: i32 = 8192;
/// Title bar height inside a framed window, in DIP.
pub const TITLE_BAR_DIP: i32 = 32;
/// Minimised chip width, in DIP, before the work area clamps it.
pub const CHIP_WIDTH_DIP: i32 = 240;
/// Gap between a minimised chip and the work-area edges, in DIP.
pub const CHIP_MARGIN_DIP: i32 = 8;
/// Windowed size used when leaving fullscreen or maximised with no remembered rect.
pub const DEFAULT_WINDOW_DIP: (i32, i32) = (960, 640);
/// Chrome used when the scene does not author controls.
pub const DEFAULT_CHROME: &str = PLAIN.id;

/// Five opaque colours, stored as `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromeColours {
    pub bar: u32,
    pub glyph: u32,
    pub close: u32,
    pub close_glyph: u32,
    pub desk: u32,
}

/// A named chrome style shipped with the language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromeStyle {
    pub id: &'static str,
    pub colours: ChromeColours,
}

const PLAIN: ChromeStyle = ChromeStyle {
    id: "goya.chrome.plain",
    colours: ChromeColours {
        bar: 0xf4f1ea,
        glyph: 0x1c1915,
        close: 0x8c2f39,
        close_glyph: 0xfffaf0,
        desk: 0xe7e1d6,
    },
};

const INK: ChromeStyle = ChromeStyle {
    id: "goya.chrome.ink",
    colours: ChromeColours {
        bar: 0x1c1915,
        glyph: 0xf4f1ea,
        close: 0x8c2f39,
        close_glyph: 0xfffaf0,
        desk: 0x2a2622,
    },
};

const NIGHT: ChromeStyle = ChromeStyle {
    id: "goya.chrome.night",
    colours: ChromeColours {
        bar: 0x12161c,
        glyph: 0xd7dee8,
        close: 0x9a3b45,
        close_glyph: 0xf7f4ef,
        desk: 0x0c0f14,
    },
};

/// Library a scene may name, or that `--chrome` may select.
pub const CHROME_LIBRARY: &[ChromeStyle] = &[PLAIN, INK, NIGHT];

/// Look up a library id. Unknown names are refused by strict admission.
pub fn library(id: &str) -> Option<&'static ChromeStyle> {
    CHROME_LIBRARY.iter().find(|style| style.id == id)
}

/// How the window first meets the work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopState {
    Fullscreen,
    Maximized,
    Windowed,
}

impl DesktopState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fullscreen => "fullscreen",
            Self::Maximized => "maximized",
            Self::Windowed => "windowed",
        }
    }
}

/// Origin of a windowed rect. Center uses integer division toward zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopPosition {
    Center,
    Dip { x: i32, y: i32 },
}

/// Authored controls: one library name, or a colour block that may still be incomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChromeChoice {
    Library(String),
    Colours(ChromeDraft),
}

/// Colour keys seen in one `controls` block. Strict admission requires all five.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChromeDraft {
    pub bar: Option<u32>,
    pub glyph: Option<u32>,
    pub close: Option<u32>,
    pub close_glyph: Option<u32>,
    pub desk: Option<u32>,
}

impl ChromeDraft {
    pub fn complete(self) -> Option<ChromeColours> {
        Some(ChromeColours {
            bar: self.bar?,
            glyph: self.glyph?,
            close: self.close?,
            close_glyph: self.close_glyph?,
            desk: self.desk?,
        })
    }
}

/// Desktop after GSSH/4 checks. Omission is fullscreen and plain chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedDesktop {
    pub state: DesktopState,
    pub position: DesktopPosition,
    pub extent_dip: Option<(i32, i32)>,
    pub colours: ChromeColours,
    /// Set when the colours come from the library, including the default.
    pub library_id: Option<&'static str>,
    /// The source contains `@desktop`, even when the block is empty.
    pub authored: bool,
    /// The block names `controls`, either a library or a colour block.
    pub controls_authored: bool,
}

/// Why a desktop declaration cannot be resolved. Reported as `GY-E1018`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopFault {
    PositionRequiresWindowed,
    ExtentRequiresWindowed,
    WindowedNeedsExtent,
    UnknownLibrary(String),
    IncompleteChrome,
}

impl DesktopFault {
    pub fn message(&self) -> String {
        match self {
            Self::PositionRequiresWindowed => {
                "position is only valid when state is windowed".into()
            }
            Self::ExtentRequiresWindowed => {
                "extent_dip is only valid when state is windowed".into()
            }
            Self::WindowedNeedsExtent => "windowed state requires extent_dip".into(),
            Self::UnknownLibrary(id) => format!("unknown window chrome library '{id}'"),
            Self::IncompleteChrome => {
                "window chrome colour block requires bar, glyph, close, close_glyph and desk".into()
            }
        }
    }
}

/// Resolve omission or one `@desktop` block. Does not look at the work area.
pub fn resolve(
    authored: bool,
    state: Option<DesktopState>,
    position: Option<DesktopPosition>,
    extent_dip: Option<(i32, i32)>,
    controls: Option<&ChromeChoice>,
) -> Result<ResolvedDesktop, DesktopFault> {
    let state = state.unwrap_or(DesktopState::Fullscreen);
    if state != DesktopState::Windowed {
        if position.is_some() {
            return Err(DesktopFault::PositionRequiresWindowed);
        }
        if extent_dip.is_some() {
            return Err(DesktopFault::ExtentRequiresWindowed);
        }
    } else if extent_dip.is_none() {
        return Err(DesktopFault::WindowedNeedsExtent);
    }
    let position = if state == DesktopState::Windowed {
        position.unwrap_or(DesktopPosition::Center)
    } else {
        DesktopPosition::Center
    };
    let (colours, library_id, controls_authored) = match controls {
        None => (PLAIN.colours, Some(PLAIN.id), false),
        Some(ChromeChoice::Library(id)) => {
            let style = library(id).ok_or_else(|| DesktopFault::UnknownLibrary(id.clone()))?;
            (style.colours, Some(style.id), true)
        }
        Some(ChromeChoice::Colours(draft)) => {
            let colours = draft.complete().ok_or(DesktopFault::IncompleteChrome)?;
            (colours, None, true)
        }
    };
    Ok(ResolvedDesktop {
        state,
        position,
        extent_dip,
        colours,
        library_id,
        authored,
        controls_authored,
    })
}

/// Inputs to [`place`]. A windowed spec needs an extent in `1..=MAX_EXTENT_DIP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceSpec {
    pub state: DesktopState,
    pub position: DesktopPosition,
    pub extent_dip: Option<(i32, i32)>,
}

/// Window rectangle in the work area. The origin is the top-left, y grows down.
/// A framed rect includes the title bar; fullscreen does not (`frame == false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub state: DesktopState,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub frame: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceError {
    /// A work-area edge was below 1 DIP.
    WorkArea,
    /// Windowed placement had no extent.
    MissingExtent,
    /// An extent edge was outside `1..=MAX_EXTENT_DIP`.
    Extent,
}

/// Place the initial window inside a work area. An extent larger than the
/// work area is clamped; it is not an error, because the work area is not
/// known when the scene is compiled. A DIP origin is clamped into
/// `[0, work - size]`. Center uses toward-zero division.
pub fn place(spec: PlaceSpec, work_w: i32, work_h: i32) -> Result<Placement, PlaceError> {
    if work_w < 1 || work_h < 1 {
        return Err(PlaceError::WorkArea);
    }
    match spec.state {
        DesktopState::Fullscreen => Ok(Placement {
            state: DesktopState::Fullscreen,
            x: 0,
            y: 0,
            width: work_w,
            height: work_h,
            frame: false,
        }),
        DesktopState::Maximized => Ok(Placement {
            state: DesktopState::Maximized,
            x: 0,
            y: 0,
            width: work_w,
            height: work_h,
            frame: true,
        }),
        DesktopState::Windowed => {
            let (extent_w, extent_h) = spec.extent_dip.ok_or(PlaceError::MissingExtent)?;
            if !(1..=MAX_EXTENT_DIP).contains(&extent_w)
                || !(1..=MAX_EXTENT_DIP).contains(&extent_h)
            {
                return Err(PlaceError::Extent);
            }
            let width = extent_w.min(work_w);
            let height = extent_h.min(work_h);
            let (x, y) = match spec.position {
                DesktopPosition::Center => ((work_w - width) / 2, (work_h - height) / 2),
                DesktopPosition::Dip { x, y } => {
                    (x.clamp(0, work_w - width), y.clamp(0, work_h - height))
                }
            };
            Ok(Placement {
                state: DesktopState::Windowed,
                x,
                y,
                width,
                height,
                frame: true,
            })
        }
    }
}

/// Centered window of [`DEFAULT_WINDOW_DIP`], clamped to the work area.
pub fn default_windowed(work_w: i32, work_h: i32) -> Result<Placement, PlaceError> {
    place(
        PlaceSpec {
            state: DesktopState::Windowed,
            position: DesktopPosition::Center,
            extent_dip: Some(DEFAULT_WINDOW_DIP),
        },
        work_w,
        work_h,
    )
}

/// Minimised chip: `(x, y, width, height)`. It is not draggable.
/// The chip sits on the bottom-left margin, clamped inside the work area.
pub fn minimized_chip(work_w: i32, work_h: i32) -> Result<(i32, i32, i32, i32), PlaceError> {
    if work_w < 1 || work_h < 1 {
        return Err(PlaceError::WorkArea);
    }
    let width = CHIP_WIDTH_DIP.min(work_w);
    let height = TITLE_BAR_DIP.min(work_h);
    let x = CHIP_MARGIN_DIP.min(work_w - width);
    let y = (work_h - height - CHIP_MARGIN_DIP).max(0);
    Ok((x, y, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_is_the_work_area_without_a_frame() {
        let placed = place(
            PlaceSpec {
                state: DesktopState::Fullscreen,
                position: DesktopPosition::Center,
                extent_dip: None,
            },
            1920,
            1080,
        )
        .unwrap();
        assert_eq!(
            (
                placed.x,
                placed.y,
                placed.width,
                placed.height,
                placed.frame
            ),
            (0, 0, 1920, 1080, false)
        );
    }

    #[test]
    fn windowed_center_and_clamp_match_the_contract() {
        let center = place(
            PlaceSpec {
                state: DesktopState::Windowed,
                position: DesktopPosition::Center,
                extent_dip: Some((960, 640)),
            },
            1920,
            1080,
        )
        .unwrap();
        assert_eq!(
            (center.x, center.y, center.width, center.height),
            (480, 220, 960, 640)
        );

        let clamped = place(
            PlaceSpec {
                state: DesktopState::Windowed,
                position: DesktopPosition::Dip { x: 100, y: 50 },
                extent_dip: Some((900, 100)),
            },
            800,
            600,
        )
        .unwrap();
        assert_eq!(
            (clamped.x, clamped.y, clamped.width, clamped.height),
            (0, 50, 800, 100)
        );

        let negative = place(
            PlaceSpec {
                state: DesktopState::Windowed,
                position: DesktopPosition::Dip { x: -10, y: 900 },
                extent_dip: Some((200, 200)),
            },
            1000,
            800,
        )
        .unwrap();
        assert_eq!(
            (negative.x, negative.y, negative.width, negative.height),
            (0, 600, 200, 200)
        );
    }

    #[test]
    fn chip_sits_on_the_bottom_margin_and_a_tiny_work_area_is_refused() {
        assert_eq!(minimized_chip(1920, 1080).unwrap(), (8, 1040, 240, 32));
        assert_eq!(minimized_chip(100, 20).unwrap(), (0, 0, 100, 20));
        assert_eq!(
            place(
                PlaceSpec {
                    state: DesktopState::Fullscreen,
                    position: DesktopPosition::Center,
                    extent_dip: None,
                },
                0,
                100,
            ),
            Err(PlaceError::WorkArea)
        );
        let fallback = default_windowed(800, 600).unwrap();
        assert_eq!(
            (fallback.x, fallback.y, fallback.width, fallback.height),
            (0, 0, 800, 600)
        );
    }

    #[test]
    fn omission_is_fullscreen_plain_chrome() {
        let resolved = resolve(false, None, None, None, None).unwrap();
        assert!(!resolved.authored);
        assert_eq!(resolved.state, DesktopState::Fullscreen);
        assert_eq!(resolved.library_id, Some(DEFAULT_CHROME));
        assert!(library("goya.chrome.missing").is_none());
        assert_eq!(
            resolve(true, Some(DesktopState::Windowed), None, None, None,).unwrap_err(),
            DesktopFault::WindowedNeedsExtent
        );
    }
}
