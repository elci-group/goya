//! Kumimanu — Goya's native X11 backend (`KUM-X11/0.0.1`).
//!
//! Emperor owns Linux machine code and ELF linking. This crate owns the X11
//! connection, one window, core events, and a software `PutImage` surface.
//! It does not compile ELF files, speak Wayland, or select a GPU API.
//!
//! # Coordinates
//!
//! Pointer positions are X11 window pixels. [`LOGICAL_SCALE`] is 1, so those
//! pixels are the Goya logical coordinates for this slice. High-DPI, RandR
//! output scale, and fractional scale are discovered only as capability bits.
//! They are not applied.
//!
//! # What this slice does not do
//!
//! `CLIPBOARD` text is owned and converted, with INCR in both directions
//! for text over 256 KiB: at most [`MAX_CLIPBOARD_BYTES`] in total, valid UTF-8 or `STRING`, never
//! empty. At most four INCR transfers are served at once; one idle for five
//! seconds is dropped. `PRIMARY`, XDND, XIM, XInput2 devices, the
//! Present extension, MIT-SHM transport, Render acceleration, Vulkan, and EGL
//! are not implemented. Missing extensions reduce [`Capabilities`] and do not
//! abort the connection. Accessibility is not bridged. A hostile client on
//! the same X server can still observe or inject input; Kumimanu does not
//! claim isolation the X11 protocol cannot provide.
//!
//! Text shaping stays out of this crate. Callers present pixels.

#![forbid(unsafe_code)]

mod backend;
mod error;
mod frame;
mod keymap;
mod phase;
mod queue;
mod selection;

pub use backend::Session;
pub use error::Error;
pub use frame::{EncodedFrame, PixelFormat, MAX_PIXELS};
pub use keymap::{InterpretedKey, Keymap, Modifiers, NamedKey};
pub use phase::Phase;
pub use selection::MAX_CLIPBOARD_BYTES;

/// Contract implemented by this crate.
pub const CONTRACT: &str = "KUM-X11/0.0.1";

/// Logical pixels per X11 window pixel. This slice does not scale.
pub const LOGICAL_SCALE: f64 = 1.0;

/// Convert an X11 window pixel to the Goya logical coordinate.
///
/// The scale is fixed at 1. Non-finite inputs are not produced by X11.
pub fn logical_xy(x: i16, y: i16) -> (f64, f64) {
    (f64::from(x) * LOGICAL_SCALE, f64::from(y) * LOGICAL_SCALE)
}

/// One window created by [`Session::create_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) std::num::NonZeroU32);

impl WindowId {
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Requested top-level window. Lengths are bounded and reject interior NUL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: String,
    pub class: String,
    pub instance: String,
    pub width: u16,
    pub height: u16,
    pub x: i16,
    pub y: i16,
    pub override_redirect: bool,
}

impl WindowConfig {
    pub fn new(title: impl Into<String>, width: u16, height: u16) -> Self {
        Self {
            title: title.into(),
            class: "Kumimanu".to_owned(),
            instance: "goya".to_owned(),
            width,
            height,
            x: 0,
            y: 0,
            override_redirect: false,
        }
    }
}

/// Extensions the server advertised. A false bit is a reduced capability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub mit_shm: bool,
    pub randr: bool,
    pub xfixes: bool,
    pub present: bool,
    pub xinput: bool,
    pub render: bool,
    pub big_requests: bool,
    pub xkb: bool,
}

/// Counters that stay numeric. They do not contain event or clipboard bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Diagnostics {
    pub ignored_events: u32,
    pub protocol_errors: u32,
    /// INCR reads started for this session's own conversions.
    pub incr_reads: u32,
    /// INCR writes started to other clients.
    pub incr_writes: u32,
}

/// Events translated from the core X11 stream.
///
/// Key text and key identity are separate fields. Scroll wheel buttons are
/// [`Event::Scroll`] and are not also delivered as button presses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Mapped,
    Exposed,
    Resized {
        width: u16,
        height: u16,
    },
    Key {
        keycode: u8,
        keysym: u32,
        text: Option<char>,
        named: Option<NamedKey>,
        modifiers: Modifiers,
        pressed: bool,
        repeat: bool,
    },
    PointerMove {
        x: i16,
        y: i16,
    },
    PointerButton {
        x: i16,
        y: i16,
        button: u8,
        pressed: bool,
    },
    Scroll {
        x: i16,
        y: i16,
        vertical: i32,
        horizontal: i32,
    },
    Focus {
        focused: bool,
    },
    /// Answer to [`Session::request_clipboard_text`]. `None` is no text.
    ClipboardText(Option<String>),
    CloseRequested,
}

impl Event {
    pub(crate) fn is_droppable(&self) -> bool {
        matches!(self, Self::PointerMove { .. } | Self::Exposed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_coordinates_match_window_pixels() {
        assert_eq!(logical_xy(-4, 12), (-4.0, 12.0));
        assert_eq!(LOGICAL_SCALE, 1.0);
        assert_eq!(CONTRACT, "KUM-X11/0.0.1");
    }
}
