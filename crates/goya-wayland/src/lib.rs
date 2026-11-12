//! GY-WAYLAND/1 — Native windowing host abstraction (Wave 1 completion).
//!
//! Minimal, honest scope per GY-WAYLAND-001: one window, pointer events
//! mapped to surface-local pixels, and the goya render list presented via a
//! shared-memory buffer. Keyboard input additionally surfaces ctrl+c /
//! ctrl+v / ctrl+q as [`PlatformEvent::Shortcut`] events (GY-ZCODE-004),
//! plus ctrl+Left / ctrl+Right as [`Shortcut::SessionPrevious`] /
//! [`Shortcut::SessionNext`] for multi-session switching (GY-ZCODE-006);
//! this shortcut path requires the compositor's xkb keymap — the keymap-free
//! evdev fallback tracks no ctrl modifier and never emits shortcuts. This
//! crate is where Goya's zero-dependency policy
//! deliberately ends — the Wayland host layer uses the official Smithay
//! client bindings rather than a hand-rolled wire protocol, because a
//! protocol reimplementation is a security liability, not an asset
//! (GOYA §13: Linux compositor fragmentation).
//!
//! ## IME text input (GY-ZCODE-005)
//!
//! The client side of `zwp_text_input_v3` is implemented on the standard
//! registry path: the global is bound when present, the text-input object is
//! created against the seat, and [`GoyaWindow::set_text_input_active`] gates
//! enable/disable around editable focus (disabled by default). Preedit,
//! delete-surrounding, and commit updates are staged by the pure
//! [`TextInputState`] machine and emitted only after `done`. Each batch clears
//! the previously applied preedit, deletes requested surrounding UTF-8 bytes,
//! applies commits in arrival order, then installs the new preedit and its
//! byte cursor/selection range. Legacy callers may still stage plain
//! [`PlatformEvent::Preedit`]; native cursor-bearing updates use
//! [`PlatformEvent::PreeditWithCursor`] and deletes use
//! [`PlatformEvent::DeleteSurroundingText`]. Commit text arrives whole and is
//! never synthesized into [`PlatformEvent::KeyChar`] events.
//!
//! Actual IME operation (candidate windows and CJK composition) is
//! compositor- and input-method-dependent: unit tests in this crate cover
//! only the compositor-free state machine and no-op paths. Whether a given
//! desktop (wlroots, GNOME, KDE) delivers useful text-input-v3 traffic is an
//! operator-verified acceptance item, not covered by tests here.
//!
//! ## Native pixel surfaces
//!
//! [`GoyaWindow::present_rgba`] presents tightly packed, top-down, straight-
//! alpha RGBA8 frames whose dimensions exactly match the current configured
//! surface. The method validates checked byte arithmetic, exact input length,
//! and a 16 MP cap, composites alpha over the most recent primitive-frame
//! background (black before the first primitive frame) directly into a
//! released SHM slot and follows the existing `wl_buffer` attach/release
//! ownership path. It rejects stale-size frames instead of scaling them;
//! producers should wait for the next [`PlatformEvent::Resized`] dimensions.
//!
//! ## System clipboard (GY-ZCODE-008)
//!
//! The client side of the core `wl_data_device_manager` stack is bound on
//! the same registry path: the manager global is bound when present, a
//! `wl_data_device` is created against the seat, and
//! [`GoyaWindow::set_clipboard_text`] serves a `text/plain` (plus
//! `text/plain;charset=utf-8`) selection from an in-memory payload; the
//! compositor's `data_source.send` requests are answered with chunked
//! writes from the pure [`ClipboardState`] serving cursor.
//! [`GoyaWindow::request_clipboard_text`] asks the compositor for the
//! current selection and completes as
//! [`PlatformEvent::ClipboardText`] (None = no text selection or the
//! offer carries no text/plain mime). Exactly one request may be
//! outstanding; a second call while one is pending is a documented no-op.
//!
//! When the compositor does not advertise `wl_data_device_manager` both
//! methods are safe no-ops (the same defensive pattern as the
//! text-input global). Whether a given desktop actually delivers
//! selection traffic round-trip is operator-verified acceptance
//! territory. goya-clipboard's in-memory ring (goya-zcode) can later be
//! backed by these host calls — that wiring belongs to goya-zcode, not
//! this crate.
//!
//! ## Per-node text colours (GY-ZCODE-009)
//!
//! [`GoyaWindow::present`] and [`GoyaWindow::present_dirty`] take a
//! `fgs: &[Option<u32>]` slice indexed by primitive order, mirroring the
//! wave-2 `fills` contract: `Some(0xRRGGBB)` replaces the glyph colour of
//! the primitive at that index; `None`, a short slice, or an empty slice
//! falls back to the flat `fg` argument. Only glyph colours are affected —
//! Panel and Button rectangle fills remain owned by `fills`, though a
//! Button's *label* (being text) honours an `fgs` override by design.
//! With an empty slice both raster paths (serial and parallel tiles) stay
//! byte-identical to the pre-wave-9 output; text scenes still rasterise
//! serially, so parallel/serial parity is asserted with `fgs` present.

use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use goya_csr::{self, Policy as CsrPolicy};
use goya_hcar::{AffinityMode, Classifier, CoreAssignment, WorkCost, WorkItem};
use goya_layout::Rect;
use rustix::fd::OwnedFd;
use rustix::fs::{ftruncate, memfd_create, MemfdFlags};
use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};
use rustix::pipe::pipe;
use std::fs::File;
use std::io::Read;
use std::os::fd::AsFd;
use std::sync::{Mutex, OnceLock};
use unicode_segmentation::UnicodeSegmentation;
use wayland_client::protocol::{
    wl_buffer::WlBuffer,
    wl_compositor::WlCompositor,
    wl_data_device::{self, WlDataDevice},
    wl_data_device_manager::{self, WlDataDeviceManager},
    wl_data_offer::{self, WlDataOffer},
    wl_data_source::{self, WlDataSource},
    wl_keyboard::{self, WlKeyboard},
    wl_pointer::{self, WlPointer},
    wl_region::WlRegion,
    wl_registry::{self, WlRegistry},
    wl_seat::{self, Capability, WlSeat},
    wl_shm::{self, WlShm},
    wl_shm_pool::{self, WlShmPool},
    wl_surface::{self, WlSurface},
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::{self, ZwpTextInputManagerV3},
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols::xdg::shell::client::{
    xdg_surface::{self, XdgSurface},
    xdg_toplevel::{self, XdgToplevel},
    xdg_wm_base::{self, XdgWmBase},
};

/// Contract identifier implemented by this crate.
pub const WAYLAND_CONTRACT: &str = "GY-WAYLAND/1";

/// Millipoints per pixel, matching goya-hello's 4 px per point.
pub const MP_PER_PX: i64 = 250;
fn legacy_text_input_event(event: PlatformEvent, extended: bool) -> Option<PlatformEvent> {
    match event {
        PlatformEvent::PreeditWithCursor { text, .. } if !extended => {
            Some(PlatformEvent::Preedit(Some(text)))
        }
        PlatformEvent::DeleteSurroundingText { .. } if !extended => None,
        event => Some(event),
    }
}

#[cfg(test)]
mod composited_layer_tests {
    use super::*;

    #[test]
    fn clips_negative_origin_and_alpha_blends_into_native_background() {
        let mut dst = vec![0xff00_0000; 12];
        let rgba = [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255];
        let layer = RgbaLayer {
            x: -1,
            y: 1,
            width: 3,
            height: 1,
            stride: 12,
            rgba: &rgba,
            clip: Some(PixelRect {
                x: 0,
                y: 1,
                width: 1,
                height: 1,
            }),
        };
        validate_rgba_layer(layer, 4, 3).unwrap();
        composite_layer(&mut dst, 4, 3, layer);
        assert_eq!(dst[4 + 0], 0xff00_8000); // half-alpha green over black
        assert_eq!(dst[4 + 1], 0xff00_0000); // clip excludes blue
        assert_eq!(dst[0], 0xff00_0000); // negative source red stays clipped
    }

    #[test]
    fn rejects_bad_stride_length_and_pixel_caps_before_composition() {
        let data = [0u8; 4];
        let short = RgbaLayer {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            stride: 3,
            rgba: &data,
            clip: None,
        };
        assert!(validate_rgba_layer(short, 1, 1).is_err());
        let large = RgbaLayer {
            x: 0,
            y: 0,
            width: (MAX_RGBA_PIXELS + 1) as u32,
            height: 1,
            stride: (MAX_RGBA_PIXELS + 1) * 4,
            rgba: &[],
            clip: None,
        };
        assert!(matches!(
            validate_rgba_layer(large, 1, 1),
            Err(WaylandError::PixelBufferTooLarge { .. })
        ));
    }
}

/// Maximum pixel count accepted by [`GoyaWindow::present_rgba`].
pub const MAX_RGBA_PIXELS: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct RgbaLayer<'a> {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub rgba: &'a [u8],
    pub clip: Option<PixelRect>,
}

fn validate_rgba_layer(
    layer: RgbaLayer<'_>,
    _surface_width: i32,
    _surface_height: i32,
) -> Result<(), WaylandError> {
    let pixels = usize::try_from(layer.width)
        .ok()
        .and_then(|w| {
            usize::try_from(layer.height)
                .ok()
                .and_then(|h| w.checked_mul(h))
        })
        .ok_or(WaylandError::PixelBufferTooLarge {
            width: layer.width,
            height: layer.height,
        })?;
    if layer.width == 0
        || layer.height == 0
        || pixels > MAX_RGBA_PIXELS
        || layer.stride < layer.width as usize * 4
    {
        return Err(WaylandError::PixelBufferTooLarge {
            width: layer.width,
            height: layer.height,
        });
    }
    let expected = layer.stride.checked_mul(layer.height as usize).ok_or(
        WaylandError::PixelBufferTooLarge {
            width: layer.width,
            height: layer.height,
        },
    )?;
    if expected > MAX_RGBA_PIXELS * 4 {
        return Err(WaylandError::PixelBufferTooLarge {
            width: layer.width,
            height: layer.height,
        });
    }
    if layer.rgba.len() != expected {
        return Err(WaylandError::PixelBufferLength {
            expected,
            actual: layer.rgba.len(),
        });
    }
    if let Some(clip) = layer.clip {
        if clip.width == 0
            || clip.height == 0
            || i64::from(clip.x)
                .checked_add(i64::from(clip.width))
                .is_none()
            || i64::from(clip.y)
                .checked_add(i64::from(clip.height))
                .is_none()
        {
            return Err(WaylandError::InvalidPixelLayer);
        }
    }
    Ok(())
}

/// Shaped text extent in surface-local pixels. Width is the widest visible
/// shaped line; height is the accumulated height of visible layout runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextExtent {
    pub width_px: f32,
    pub height_px: f32,
    pub lines: usize,
}

/// Caret rectangle for a UTF-8 byte offset in shaped text, in surface-local
/// pixels from the text box's top-left. Offsets snap backward to a grapheme
/// boundary before querying cosmic-text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextCaret {
    pub x_px: f32,
    pub y_px: f32,
    pub height_px: f32,
}

/// Optional caret and selection decoration for a text primitive. All offsets
/// are UTF-8 byte offsets into the primitive's original string. Selection is
/// rendered as a translucent overlay and the caret as a solid vertical bar;
/// neither marker is inserted into the shaped text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextOverlay {
    pub primitive_index: usize,
    pub caret_byte_offset: Option<usize>,
    pub selection_byte_range: Option<(usize, usize)>,
    pub caret_rgb: [u8; 3],
    pub selection_rgb: [u8; 3],
}

/// Shape text using Goya's renderer metrics and wrapping, returning extent
/// in pixels. Invalid dimensions or font sizes return `None`.
pub fn measure_text(text: &str, width_px: f32, height_px: f32, font_px: f32) -> Option<TextExtent> {
    if !valid_text_dimensions(width_px, height_px, font_px) {
        return None;
    }
    static SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    let Ok(mut font_system) = SYSTEM.get_or_init(|| Mutex::new(FontSystem::new())).lock() else {
        return None;
    };
    let buffer = shape_text(&mut font_system, text, width_px, height_px, font_px);
    let mut extent = TextExtent {
        width_px: 0.0,
        height_px: 0.0,
        lines: 0,
    };
    for run in buffer.layout_runs() {
        extent.width_px = extent.width_px.max(run.line_w);
        extent.height_px = extent.height_px.max(run.line_top + run.line_height);
        extent.lines += 1;
    }
    Some(extent)
}

/// Locate a UTF-8 byte offset in the same shaped layout used by rendering.
/// Dimensions and font size are pixels; the result is local pixel geometry.
/// Offsets beyond the end clamp to the end; offsets inside graphemes snap
/// backward. Invalid dimensions/font sizes or lock poisoning return `None`.
pub fn measure_text_caret(
    text: &str,
    width_px: f32,
    height_px: f32,
    font_px: f32,
    byte_offset: usize,
) -> Option<TextCaret> {
    if !valid_text_dimensions(width_px, height_px, font_px) {
        return None;
    }
    static SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    let Ok(mut font_system) = SYSTEM.get_or_init(|| Mutex::new(FontSystem::new())).lock() else {
        return None;
    };
    let buffer = shape_text(&mut font_system, text, width_px, height_px, font_px);
    if text.is_empty() {
        return Some(TextCaret {
            x_px: 0.0,
            y_px: 0.0,
            height_px: (font_px * 1.4).max(font_px + 1.0),
        });
    }
    let byte_offset = snap_grapheme_boundary(text, byte_offset.min(text.len()));
    let cursor = text_cursor(&buffer, text, byte_offset)?;
    for run in buffer.layout_runs() {
        if let Some((x, _)) = run.highlight(cursor, cursor) {
            return Some(TextCaret {
                x_px: x,
                y_px: run.line_top,
                height_px: run.line_height,
            });
        }
    }
    None
}

fn valid_text_dimensions(width: f32, height: f32, font: f32) -> bool {
    width.is_finite()
        && width > 0.0
        && height.is_finite()
        && height > 0.0
        && font.is_finite()
        && font > 0.0
}

fn shape_text(
    font_system: &mut FontSystem,
    text: &str,
    width_px: f32,
    height_px: f32,
    font_px: f32,
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(font_px, (font_px * 1.4).max(font_px + 1.0)),
    );
    buffer.set_size(font_system, Some(width_px), Some(height_px));
    buffer.set_wrap(font_system, Wrap::WordOrGlyph);
    buffer.set_text(font_system, text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, true);
    buffer
}

fn snap_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    if offset == text.len()
        || text
            .grapheme_indices(true)
            .any(|(start, _)| start == offset)
    {
        return offset;
    }
    text.grapheme_indices(true)
        .map(|(start, _)| start)
        .take_while(|start| *start < offset)
        .last()
        .unwrap_or(0)
}

fn text_cursor(buffer: &Buffer, text: &str, byte_offset: usize) -> Option<cosmic_text::Cursor> {
    let mut base = 0usize;
    for (line_i, line) in buffer.lines.iter().enumerate() {
        let line_text = line.text();
        let end = base.checked_add(line_text.len())?;
        if byte_offset <= end {
            let line_offset = byte_offset.saturating_sub(base);
            let affinity = if line_offset == 0 {
                cosmic_text::Affinity::After
            } else {
                cosmic_text::Affinity::Before
            };
            return Some(cosmic_text::Cursor::new_with_affinity(
                line_i,
                line_offset,
                affinity,
            ));
        }
        base = end.saturating_add(
            text.get(end..)
                .and_then(|tail| tail.chars().next())
                .map_or(0, char::len_utf8),
        );
    }
    let line_i = buffer.lines.len().checked_sub(1)?;
    Some(cosmic_text::Cursor::new(
        line_i,
        buffer.lines[line_i].text().len(),
    ))
}

/// Pointer/button events translated from the compositor, surface-local.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    PointerMove(f64, f64),
    PointerPress(f64, f64),
    PointerRelease(f64, f64),
    /// Printable Unicode character generated by the compositor's XKB keymap.
    KeyChar(char),
    /// Opt-in browser navigation shortcuts from the native keyboard.
    BrowserNavigation(BrowserNavigation),
    /// Non-printable navigation/editing key (see [`KeySpecial`]).
    KeySpecial(KeySpecial),
    /// Navigation/editing key with the XKB modifiers active at key press.
    /// Unmodified keys continue to use [`PlatformEvent::KeySpecial`] for API
    /// compatibility; modified navigation uses this event so hosts can
    /// implement selection extension and word movement.
    KeySpecialWithModifiers {
        key: KeySpecial,
        modifiers: KeyModifiers,
    },
    /// Mouse-wheel scroll in text lines (evdev REL_WHEEL via wl_pointer.axis);
    /// positive = wheel up, negative = wheel down. Devices that never emit a
    /// vertical axis produce nothing.
    PointerScroll(i32),
    /// Smooth pointer or touchpad scrolling in surface-coordinate units.
    /// Positive values mean up/left, matching [`PlatformEvent::PointerScroll`].
    /// Horizontal and vertical axes are emitted independently as Wayland
    /// reports them; consumers should accumulate fractional values.
    PointerScrollDelta {
        x: f64,
        y: f64,
    },
    /// Ctrl+c / ctrl+v / ctrl+q (GY-ZCODE-004). Only produced by the xkb
    /// state path; the keymap-free evdev fallback tracks no ctrl modifier
    /// and therefore never emits shortcuts.
    Shortcut(Shortcut),
    /// IME preedit text (GY-ZCODE-005), applied after the protocol `done`
    /// frame. `None` clears an existing preedit.
    Preedit(Option<String>),
    /// Native text-input-v3 preedit with byte cursor/selection endpoints.
    /// Kept separate from [`PlatformEvent::Preedit`] so existing hosts remain
    /// source-compatible while richer hosts can preserve the exact range.
    PreeditWithCursor {
        text: String,
        cursor_begin: i32,
        cursor_end: i32,
    },
    /// Whole commit string from the IME (GY-ZCODE-005). This is never
    /// synthesized as [`PlatformEvent::KeyChar`] events — hosts insert it at
    /// their own cursor. Applied after the protocol `done` frame.
    CommitText(String),
    /// Delete UTF-8 bytes around the current editor selection, as requested
    /// by text-input-v3. Delivered only after the corresponding `done` frame.
    DeleteSurroundingText {
        before_bytes: u32,
        after_bytes: u32,
    },
    /// Completed clipboard read (GY-ZCODE-008): `Some(text)` is the current
    /// selection's text/plain payload, `None` means no text selection exists
    /// (or the offer carries no text/plain mime, or the payload was empty).
    /// Delivered by [`GoyaWindow::request_clipboard_text`].
    ClipboardText(Option<String>),
    /// Compositor resized the window to new pixel dimensions.
    Resized(i32, i32),
    Configured,
    CloseRequested,
}

/// Application-level shortcut keys (GY-ZCODE-004): ctrl+c / ctrl+v / ctrl+q,
/// extended in GY-ZCODE-006 with ctrl+Left / ctrl+Right session switching.
/// All are recognised through the compositor's xkb state path (ctrl modifier
/// tracking plus evdev keycode). The keymap-free evdev fallback path never
/// emits these because it does not track ctrl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Copy,
    Paste,
    Quit,
    /// ctrl+Right (evdev 106): switch to the next session (GY-ZCODE-006).
    SessionNext,
    /// ctrl+Left (evdev 105): switch to the previous session (GY-ZCODE-006).
    SessionPrevious,
}

/// Navigation operations emitted only by opted-in browser windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserNavigation {
    Back,
    Forward,
    Reload,
}

/// Navigation/editing keys emitted as [`PlatformEvent::KeySpecial`].
/// Enter stays `KeyChar('\n')` and Backspace stays `KeyChar('\u{8}')`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySpecial {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    Delete,
    PageUp,
    PageDown,
    Escape,
    Tab,
}

/// Modifier state attached to a navigation key event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
}

/// Semantic input purpose communicated to compositor input methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputPurpose {
    Normal,
    Password,
    Pin,
    Digits,
    Email,
    Url,
}

/// Pure, compositor-free queue for text-input-v3 IME updates (GY-ZCODE-005).
///
/// The protocol delivers `preedit_string` / `commit_string` in an
/// uncommitted staging area and applies the
/// whole frame atomically at `done`. This state machine mirrors that rule:
/// [`TextInputState::feed_preedit`] and [`TextInputState::feed_commit`] only
/// stage data; [`TextInputState::drain`] emits [`PlatformEvent`]s only after
/// [`TextInputState::feed_done`] has armed a frame. The window event loop
/// calls `drain` once per poll and forwards the events upstream.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TextInputState {
    /// Preedit staged by the latest `preedit_string` (None = this frame
    /// clears the preedit). Replaced, not accumulated, per protocol.
    pub pending_preedit: Option<String>,
    pending_preedit_cursor: Option<(i32, i32)>,
    preedit_received: bool,
    /// Commit strings staged by `commit_string`, flushed in arrival order.
    pub committed: Vec<String>,
    /// Preedit the host currently has applied (updated on drain).
    pub applied_preedit: Option<String>,
    /// Native preedit cursor range currently applied by the host.
    pub applied_preedit_cursor: Option<(i32, i32)>,
    /// Delete-surrounding requests staged in compositor arrival order.
    deleted_surrounding: Vec<(u32, u32)>,
    events: Vec<PlatformEvent>,
    armed: bool,
}

impl TextInputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage a `preedit_string` frame update. `None` stages a clear.
    pub fn feed_preedit(&mut self, preedit: Option<String>) {
        self.pending_preedit = preedit;
        self.pending_preedit_cursor = None;
        self.preedit_received = true;
    }

    /// Stage a native preedit string and its UTF-8 byte cursor/selection
    /// endpoints. Values are preserved verbatim for the editor to snap to
    /// grapheme boundaries at its own buffer boundary.
    pub fn feed_preedit_with_cursor(
        &mut self,
        preedit: Option<String>,
        cursor_begin: i32,
        cursor_end: i32,
    ) {
        self.pending_preedit = preedit;
        self.pending_preedit_cursor = Some((cursor_begin, cursor_end));
        self.preedit_received = true;
    }

    /// Stage one text-input-v3 delete-surrounding request. It is ignored by
    /// consumers until the matching `done` event drains the batch.
    pub fn feed_delete_surrounding(&mut self, before_bytes: u32, after_bytes: u32) {
        self.deleted_surrounding.push((before_bytes, after_bytes));
    }

    /// Stage a `commit_string` frame update. Commit text arrives whole —
    /// it is never split into key events.
    pub fn feed_commit(&mut self, commit: Option<String>) {
        if let Some(text) = commit {
            // An empty commit string carries no text; the protocol uses it
            // only to confirm the frame, so it stages nothing.
            if !text.is_empty() {
                self.committed.push(text);
            }
        }
    }

    /// Arm the staged frame: the next drain applies it atomically. A staged
    /// clear (`preedit_string` with no text) also arms when a preedit is
    /// currently applied — clearing is a visible frame change.
    pub fn feed_done(&mut self) {
        if self.preedit_received
            || !self.committed.is_empty()
            || !self.deleted_surrounding.is_empty()
        {
            self.armed = true;
        }
    }

    /// Surface left the text input: preedit is implicitly cleared, staged
    /// data is discarded.
    pub fn feed_leave(&mut self) {
        self.pending_preedit = None;
        self.pending_preedit_cursor = None;
        self.preedit_received = false;
        self.committed.clear();
        self.deleted_surrounding.clear();
        self.armed = false;
        if self.applied_preedit.is_some() {
            self.events.push(PlatformEvent::Preedit(None));
            self.applied_preedit = None;
            self.applied_preedit_cursor = None;
        }
    }

    /// Emit the armed protocol frame in text-input-v3 order: clear existing
    /// preedit, delete surrounding bytes, apply commits, then install preedit
    /// and its cursor range. Draining without an armed frame is a no-op.
    pub fn drain(&mut self) -> Vec<PlatformEvent> {
        if self.armed {
            let has_update = self.preedit_received
                || !self.committed.is_empty()
                || !self.deleted_surrounding.is_empty();
            if has_update && self.applied_preedit.take().is_some() {
                self.events.push(PlatformEvent::Preedit(None));
                self.applied_preedit_cursor = None;
            }
            for (before_bytes, after_bytes) in self.deleted_surrounding.drain(..) {
                self.events.push(PlatformEvent::DeleteSurroundingText {
                    before_bytes,
                    after_bytes,
                });
            }
            for text in self.committed.drain(..) {
                self.events.push(PlatformEvent::CommitText(text));
            }
            if self.preedit_received {
                let preedit = self.pending_preedit.take();
                if let Some(text) = preedit.as_ref().filter(|text| !text.is_empty()) {
                    if let Some((cursor_begin, cursor_end)) = self.pending_preedit_cursor {
                        self.events.push(PlatformEvent::PreeditWithCursor {
                            text: text.clone(),
                            cursor_begin,
                            cursor_end,
                        });
                    } else {
                        self.events.push(PlatformEvent::Preedit(Some(text.clone())));
                    }
                }
                self.applied_preedit = preedit.filter(|text| !text.is_empty());
                self.applied_preedit_cursor = self
                    .applied_preedit
                    .as_ref()
                    .and(self.pending_preedit_cursor);
                self.pending_preedit_cursor = None;
            }
            self.preedit_received = false;
            self.armed = false;
        }
        std::mem::take(&mut self.events)
    }
}

/// Mime preferred for clipboard reads, in negotiation priority order
/// (GY-ZCODE-008 frozen contract): UTF-8-tagged text/plain first, then
/// bare text/plain, otherwise no text offer exists.
const TEXT_MIME_PREFERENCE: [&str; 2] = ["text/plain;charset=utf-8", "text/plain"];

/// Pure mime negotiation (GY-ZCODE-008): pick the best text mime an offer
/// advertises — `text/plain;charset=utf-8` beats bare `text/plain`; anything
/// else (including an empty mime list) yields None.
fn pick_text_mime(mimes: &[String]) -> Option<String> {
    TEXT_MIME_PREFERENCE
        .iter()
        .find(|preferred| mimes.iter().any(|mime| mime == *preferred))
        .map(|preferred| (*preferred).to_string())
}

/// Request lifecycle of a clipboard read: `Idle → Requested → Answered`.
/// A second request while `Requested` is rejected (documented no-op at the
/// [`GoyaWindow::request_clipboard_text`] surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ClipboardRequestPhase {
    #[default]
    Idle,
    Requested,
    Answered,
}

/// Pure, compositor-free clipboard state (GY-ZCODE-008): the outgoing
/// selection payload with its serving cursor, plus the pending-request
/// state machine. Wayland plumbing feeds it; [`GoyaWindow::poll`] forwards
/// the completed [`PlatformEvent::ClipboardText`].
#[derive(Debug, Default)]
struct ClipboardState {
    /// Payload this client advertises for the selection (set path).
    outgoing: Option<String>,
    /// Serving cursor into `outgoing`: bytes already written for the
    /// current `data_source.send` request.
    served: usize,
    /// Mimes advertised by the current selection offer (receive path).
    selection_mimes: Vec<String>,
    phase: ClipboardRequestPhase,
}

impl ClipboardState {
    fn new() -> Self {
        Self::default()
    }

    /// Store a new outgoing payload and reset the serving cursor.
    fn set_outgoing(&mut self, text: &str) {
        self.outgoing = Some(text.to_owned());
        self.served = 0;
    }

    /// Drop the outgoing payload (data-source cancelled or replaced).
    fn take_outgoing(&mut self) -> Option<String> {
        self.served = 0;
        self.outgoing.take()
    }

    /// Record the mimes of the newly installed selection offer.
    fn stage_selection_mimes(&mut self, mimes: Vec<String>) {
        self.selection_mimes = mimes;
    }

    /// The mime to receive from the current selection, if it offers text.
    fn receive_target(&self) -> Option<String> {
        pick_text_mime(&self.selection_mimes)
    }

    /// Admit a clipboard read request. Returns false (rejected) only while
    /// a request is already outstanding — one outstanding request at a time.
    fn begin_request(&mut self) -> bool {
        if self.phase == ClipboardRequestPhase::Requested {
            return false;
        }
        self.phase = ClipboardRequestPhase::Requested;
        true
    }

    /// Complete the outstanding request with `text`, producing the
    /// [`PlatformEvent::ClipboardText`] to surface. Completing without an
    /// outstanding request (or with one already answered) yields None.
    fn complete_request(&mut self, text: Option<String>) -> Option<PlatformEvent> {
        if self.phase != ClipboardRequestPhase::Requested {
            return None;
        }
        self.phase = ClipboardRequestPhase::Answered;
        Some(PlatformEvent::ClipboardText(text))
    }

    /// The selection changed underneath an outstanding request: resolve it
    /// as "no text selection" so the caller is never left pending forever.
    fn cancel_request(&mut self) -> Option<PlatformEvent> {
        self.complete_request(None)
    }

    /// Next chunk of the outgoing payload for `data_source.send`, at most
    /// `max` bytes, never splitting a UTF-8 scalar. Returns None when the
    /// payload is exhausted (or absent). Advances the serving cursor.
    fn serve_chunk(&mut self, max: usize) -> Option<&[u8]> {
        let payload = self.outgoing.as_deref()?;
        if self.served >= payload.len() {
            return None;
        }
        let start = self.served;
        let mut end = payload.len().min(start.saturating_add(max.max(1)));
        while !payload.is_char_boundary(end) {
            end -= 1;
        }
        self.served = end;
        Some(&payload.as_bytes()[start..end])
    }
}

/// Pure activation decision for the text-input enable/disable state.
/// `None` means "no protocol traffic needed" — either the global is missing
/// (`None` input) or the requested state already holds. Returning `Some(_)`
/// means the caller must send enable/disable followed by a state commit.
fn text_input_activation<T>(bound: Option<T>, enabled: bool, active: bool) -> Option<(T, bool)> {
    let input = bound?;
    if enabled == active {
        return None;
    }
    Some((input, active))
}

/// Raster clip region in pixels.
#[derive(Debug, Clone, Copy)]
struct PixelClip {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

fn prim_rect(prim: &goya_runtime::Primitive) -> goya_layout::Rect {
    match prim {
        goya_runtime::Primitive::Panel(r) => *r,
        goya_runtime::Primitive::Text(r, _) => *r,
        goya_runtime::Primitive::Button(r, _) => *r,
    }
}

/// Wayland failures surfaced as data, never panics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaylandError {
    /// No compositor reachable (headless/CI): the app stays operational.
    NoDisplay,
    MissingGlobal(&'static str),
    ShmSetup(&'static str),
    /// RGBA dimensions do not match the currently configured surface size.
    PixelSurfaceMismatch {
        width: u32,
        height: u32,
        surface_width: i32,
        surface_height: i32,
    },
    /// RGBA dimensions exceed the checked 16 MP surface cap or overflow size arithmetic.
    PixelBufferTooLarge {
        width: u32,
        height: u32,
    },
    /// The tightly-packed RGBA byte slice does not have exactly 4 bytes per pixel.
    PixelBufferLength {
        expected: usize,
        actual: usize,
    },
    InvalidPixelLayer,
}

impl std::fmt::Display for WaylandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaylandError::NoDisplay => write!(f, "no Wayland display reachable"),
            WaylandError::MissingGlobal(g) => write!(f, "compositor lacks {g}"),
            WaylandError::ShmSetup(e) => write!(f, "shared memory setup failed: {e}"),
            WaylandError::PixelSurfaceMismatch {
                width,
                height,
                surface_width,
                surface_height,
            } => write!(
                f,
                "RGBA size {width}x{height} does not match surface {surface_width}x{surface_height}"
            ),
            WaylandError::PixelBufferTooLarge { width, height } => {
                write!(f, "RGBA surface {width}x{height} exceeds the pixel limit")
            }
            WaylandError::PixelBufferLength { expected, actual } => write!(
                f,
                "RGBA buffer has {actual} bytes; expected exactly {expected}"
            ),
            WaylandError::InvalidPixelLayer => write!(f, "invalid RGBA layer geometry or stride"),
        }
    }
}

impl std::error::Error for WaylandError {}

/// One back-buffer: shared-memory slot plus its wl_buffer.
struct ShmFrame {
    map: *mut u8,
    len: usize,
    buffer: WlBuffer,
    /// wl_buffer Release received: compositor is done reading, safe to write.
    released: bool,
}
// The mapping is owned exclusively by this struct and lives as long as the fd.
unsafe impl Send for ShmFrame {}

impl ShmFrame {
    fn pixels_mut(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.map.cast::<u32>(), self.len / 4) }
    }

    fn pixels(&self) -> &[u32] {
        unsafe { std::slice::from_raw_parts(self.map.cast::<u32>(), self.len / 4) }
    }
}

impl Drop for ShmFrame {
    fn drop(&mut self) {
        if !self.map.is_null() {
            let _ = unsafe { munmap(self.map.cast(), self.len) };
        }
        self.buffer.destroy();
    }
}

/// Triple-buffered surface memory: one memfd-backed pool, three slots.
/// The compositor reads one slot while we rasterise another; the third
/// absorbs latency so we NEVER write an unreleased buffer (audit P0):
/// if no slot is released, presentation is skipped for this frame and
/// retried after the next pump processes wl_buffer releases.
struct BufferPair {
    /// Kept alive so the mapped memory's backing file persists; the pool
    /// holds its own fd reference, so this field is never read directly.
    #[allow(dead_code)]
    fd: OwnedFd,
    map: *mut u8,
    len_per_slot: usize,
    slots: [ShmFrame; 3],
    /// Index of the slot presented most recently (the compositor's front).
    front: usize,
}

impl BufferPair {
    fn new(
        shm: &WlShm,
        qh: &QueueHandle<GoyaWindowInner>,
        width: i32,
        height: i32,
    ) -> Result<Self, WaylandError> {
        let stride = width * 4;
        let len = (stride * height) as usize;
        let fd = memfd_create("goya-frame", MemfdFlags::CLOEXEC)
            .map_err(|_| WaylandError::ShmSetup("memfd"))?;
        ftruncate(&fd, (len * 3) as u64).map_err(|_| WaylandError::ShmSetup("ftruncate"))?;
        let map = unsafe {
            mmap(
                std::ptr::null_mut(),
                len * 3,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &fd,
                0,
            )
        }
        .map_err(|_| WaylandError::ShmSetup("mmap"))? as *mut u8;
        let pool = shm.create_pool(fd.as_fd(), (len * 3) as i32, qh, ());
        let mk = |offset: usize| ShmFrame {
            map: unsafe { map.byte_add(offset) },
            len,
            buffer: pool.create_buffer(
                offset as i32,
                width,
                height,
                stride,
                wl_shm::Format::Argb8888,
                qh,
                (),
            ),
            released: true,
        };
        // All slots start "released": the compositor owns nothing yet.
        let slots = [mk(0), mk(len), mk(len * 2)];
        Ok(BufferPair {
            fd,
            map,
            len_per_slot: len,
            slots,
            front: 0,
        })
    }
}

impl Drop for BufferPair {
    fn drop(&mut self) {
        // The pair owns the mapping. Each slot destroys its wl_buffer once
        // in `ShmFrame::drop`. Destroying here as well is a protocol error
        // and the compositor closes the connection on the next resize.
        for slot in &mut self.slots {
            slot.map = std::ptr::null_mut();
            slot.len = 0;
        }
        let _ = unsafe { munmap(self.map.cast(), self.len_per_slot * 3) };
    }
}

/// Everything the window needs, both application state and protocol objects.
struct GoyaWindowInner {
    width: i32,
    height: i32,
    configured: bool,
    close_requested: bool,
    events: Vec<PlatformEvent>,
    globals: Vec<(u32, String)>,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    seat: Option<WlSeat>,
    xdg_wm_base: Option<XdgWmBase>,
    surface: Option<WlSurface>,
    xdg_surface: Option<XdgSurface>,
    toplevel: Option<XdgToplevel>,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    last_pointer: Option<(f64, f64)>,
    xkb_context: Option<xkbcommon::xkb::Context>,
    xkb_state: Option<xkbcommon::xkb::State>,
    _compose_table: Option<xkbcommon::xkb::compose::Table>,
    compose_state: Option<xkbcommon::xkb::compose::State>,
    text_input_manager: Option<ZwpTextInputManagerV3>,
    text_input: Option<ZwpTextInputV3>,
    text_input_enabled: bool,
    text_input_state: TextInputState,
    data_device_manager: Option<WlDataDeviceManager>,
    data_device: Option<WlDataDevice>,
    data_source: Option<WlDataSource>,
    /// Current selection offer: announced by `data_device.data_offer`, its
    /// mimes still arriving in `pending_offer_mimes`, then confirmed by
    /// `selection`. Receiving reads from this object.
    pending_offer: Option<WlDataOffer>,
    /// Mimes announced for `pending_offer` so far.
    pending_offer_mimes: Vec<String>,
    /// Serial of the latest keyboard enter, required by `set_selection`.
    keyboard_enter_serial: u32,
    clipboard: ClipboardState,
    shift: bool,
    ctrl: bool,
    alt: bool,
    browser_navigation_keys: bool,
    buffers: Option<BufferPair>,
    region: Option<WlRegion>,
    ack_serial: u32,
    /// Serial from the newest `xdg_surface.configure` that still needs an ack
    /// on the next commit. A resize or activation stays pending until then.
    pending_configure: Option<u32>,
    /// True only when the latest present attached a buffer and committed.
    /// A skipped frame leaves this false so the caller can retry after pump
    /// observes `wl_buffer.release`.
    frame_committed: bool,
    /// Size last published through `wl_surface.set_opaque_region`.
    region_extent: Option<(i32, i32)>,
    /// Explicitly opt into general-core parallel tile raster. No core
    /// affinity is requested; Linux remains responsible for placement.
    parallel_raster: bool,
    /// Background used by the most recent primitive frame for alpha-compositing
    /// pixels supplied through `present_rgba`.
    background_rgb: [u8; 3],
    extended_text_input_events: bool,
}

impl GoyaWindowInner {
    fn new(width: i32, height: i32) -> Self {
        let xkb_context = Some(xkbcommon::xkb::Context::new(0));
        let locale = std::env::var_os("LC_ALL")
            .or_else(|| std::env::var_os("LC_CTYPE"))
            .or_else(|| std::env::var_os("LANG"))
            .unwrap_or_else(|| std::ffi::OsString::from("C.UTF-8"));
        let compose_table = xkb_context.as_ref().and_then(|context| {
            xkbcommon::xkb::compose::Table::new_from_locale(
                context,
                &locale,
                xkbcommon::xkb::compose::COMPILE_NO_FLAGS,
            )
            .ok()
        });
        let compose_state = compose_table.as_ref().map(|table| {
            xkbcommon::xkb::compose::State::new(table, xkbcommon::xkb::compose::STATE_NO_FLAGS)
        });
        GoyaWindowInner {
            width,
            height,
            configured: false,
            close_requested: false,
            events: Vec::new(),
            globals: Vec::new(),
            compositor: None,
            shm: None,
            seat: None,
            xdg_wm_base: None,
            surface: None,
            xdg_surface: None,
            toplevel: None,
            pointer: None,
            keyboard: None,
            last_pointer: None,
            xkb_context,
            xkb_state: None,
            _compose_table: compose_table,
            compose_state,
            text_input_manager: None,
            text_input: None,
            text_input_enabled: false,
            text_input_state: TextInputState::new(),
            data_device_manager: None,
            data_device: None,
            data_source: None,
            pending_offer: None,
            pending_offer_mimes: Vec::new(),
            keyboard_enter_serial: 0,
            clipboard: ClipboardState::new(),
            shift: false,
            ctrl: false,
            alt: false,
            browser_navigation_keys: false,
            buffers: None,
            region: None,
            ack_serial: 0,
            pending_configure: None,
            frame_committed: false,
            region_extent: None,
            parallel_raster: false,
            background_rgb: [0, 0, 0],
            extended_text_input_events: false,
        }
    }
}

/// Application-facing window: draw the runtime's render list, receive
/// pointer events.
pub struct GoyaWindow {
    state: GoyaWindowInner,
    queue: wayland_client::EventQueue<GoyaWindowInner>,
}

impl GoyaWindowInner {
    /// Compositor-free key press handling (GY-ZCODE-004): resolve one pressed
    /// evdev keycode against the current modifier state into platform events.
    /// Order matters: ctrl+c/v/q and ctrl+Left/Right map to
    /// `PlatformEvent::Shortcut` first;
    /// otherwise navigation keeps its `KeySpecial` semantics and text keys go
    /// through XKB Compose → `KeyChar`, byte-identical to the pre-shortcut
    /// behavior. Other ctrl combos produce no events (the char path returns
    /// nothing while ctrl is held).
    fn handle_key_press(&mut self, key: u32) {
        if self.browser_navigation_keys {
            let navigation = match key {
                105 if self.alt && !self.ctrl => Some(BrowserNavigation::Back),
                106 if self.alt && !self.ctrl => Some(BrowserNavigation::Forward),
                19 if self.ctrl && !self.alt => Some(BrowserNavigation::Reload),
                63 if !self.ctrl && !self.alt => Some(BrowserNavigation::Reload),
                _ => None,
            };
            if let Some(navigation) = navigation {
                self.events
                    .push(PlatformEvent::BrowserNavigation(navigation));
                return;
            }
        }
        if let Some(shortcut) = shortcut_from_key(self.ctrl, key) {
            self.events.push(PlatformEvent::Shortcut(shortcut));
            return;
        }
        if let Some(ks) = key_special(key) {
            if let Some(compose) = self.compose_state.as_mut() {
                compose.reset();
            }
            let modifiers = KeyModifiers {
                shift: self.shift,
                ctrl: self.ctrl,
            };
            if modifiers.shift || modifiers.ctrl {
                self.events
                    .push(PlatformEvent::KeySpecialWithModifiers { key: ks, modifiers });
            } else {
                self.events.push(PlatformEvent::KeySpecial(ks));
            }
        } else {
            let chars = key_chars_with_compose(
                self.xkb_state.as_ref(),
                self.compose_state.as_mut(),
                key,
                self.shift,
                self.ctrl,
            );
            for c in chars {
                self.events.push(PlatformEvent::KeyChar(c));
            }
        }
    }
}

impl GoyaWindow {
    /// Connect to the compositor and create a configured window.
    pub fn create(title: &str, width: i32, height: i32) -> Result<Self, WaylandError> {
        let Ok(conn) = Connection::connect_to_env() else {
            return Err(WaylandError::NoDisplay);
        };
        let display = conn.display();
        let mut queue = conn.new_event_queue::<GoyaWindowInner>();
        let qh = queue.handle();
        let _registry = display.get_registry(&qh, ());
        let mut state = GoyaWindowInner::new(width, height);

        // Roundtrip 1: collect globals from the registry.
        queue
            .roundtrip(&mut state)
            .map_err(|_| WaylandError::NoDisplay)?;

        // Bind the globals we need by interface name.
        let mut compositor: Option<WlCompositor> = None;
        let mut shm: Option<WlShm> = None;
        let mut seat: Option<WlSeat> = None;
        let mut wm_base: Option<XdgWmBase> = None;
        let mut text_input_manager: Option<ZwpTextInputManagerV3> = None;
        let mut data_device_manager: Option<WlDataDeviceManager> = None;
        for (name, interface) in &state.globals {
            match interface.as_str() {
                "wl_compositor" => compositor = Some(_registry.bind(*name, 1, &qh, ())),
                "wl_shm" => shm = Some(_registry.bind(*name, 1, &qh, ())),
                "wl_seat" => seat = Some(_registry.bind(*name, 1, &qh, ())),
                "xdg_wm_base" => wm_base = Some(_registry.bind(*name, 1, &qh, ())),
                "zwp_text_input_manager_v3" => {
                    text_input_manager = Some(_registry.bind(*name, 1, &qh, ()))
                }
                "wl_data_device_manager" => {
                    data_device_manager = Some(_registry.bind(*name, 1, &qh, ()))
                }
                _ => {}
            }
        }
        state.globals.clear();
        let compositor = compositor.ok_or(WaylandError::MissingGlobal("wl_compositor"))?;
        let shm = shm.ok_or(WaylandError::MissingGlobal("wl_shm"))?;
        let wm_base = wm_base.ok_or(WaylandError::MissingGlobal("xdg_wm_base"))?;

        // Window objects.
        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
        let toplevel = xdg_surface.get_toplevel(&qh, ());
        toplevel.set_title(title.to_string());
        state.surface = Some(surface.clone());
        state.xdg_surface = Some(xdg_surface.clone());
        state.toplevel = Some(toplevel.clone());
        state.compositor = Some(compositor.clone());
        state.shm = Some(shm.clone());
        state.xdg_wm_base = Some(wm_base.clone());
        state.seat = seat.clone();
        state.text_input_manager = text_input_manager;
        state.data_device_manager = data_device_manager;
        // Initial commit: the compositor responds with a configure.
        surface.commit();

        // Roundtrips until configured.
        for _ in 0..50 {
            queue
                .roundtrip(&mut state)
                .map_err(|_| WaylandError::NoDisplay)?;
            if state.configured {
                break;
            }
        }
        if !state.configured {
            return Err(WaylandError::MissingGlobal("configure"));
        }
        xdg_surface.ack_configure(state.ack_serial);
        state.pending_configure = None;
        surface.commit();

        // Pointer and keyboard, if the seat has them.
        if let Some(seat) = seat {
            state.pointer = Some(seat.get_pointer(&qh, ()));
            state.keyboard = Some(seat.get_keyboard(&qh, ()));
            if let Some(manager) = &state.text_input_manager {
                state.text_input = Some(manager.get_text_input(&seat, &qh, ()))
            }
            if let Some(manager) = &state.data_device_manager {
                state.data_device = Some(manager.get_data_device(&seat, &qh, ()))
            }
        }

        Ok(GoyaWindow { state, queue })
    }

    /// Enable general-core parallel tile raster for large frames. Workers
    /// use CSR's deterministic tile schedule and ordinary OS-managed thread
    /// placement; this does not pin work to performance/efficiency cores.
    /// Small surfaces and workloads still use the serial path.
    pub fn set_general_parallel_raster(&mut self, enabled: bool) {
        self.state.parallel_raster = enabled;
    }

    /// Enable or disable the compositor text input for this window's seat
    /// (GY-ZCODE-005). Disabled by default; hosts activate it only while an
    /// editable field has focus. A no-op when the compositor does not
    /// advertise `zwp_text_input_manager_v3` (the global is absent, so there
    /// is nothing to enable) or when the requested state already holds.
    /// Preedit/commit updates arrive later as [`PlatformEvent::Preedit`] and
    /// [`PlatformEvent::CommitText`].
    pub fn set_text_input_active(&mut self, active: bool) {
        let bound = self.state.text_input.clone();
        if let Some((input, new_state)) =
            text_input_activation(bound, self.state.text_input_enabled, active)
        {
            if new_state {
                input.enable();
            } else {
                input.disable();
            }
            input.commit();
            self.state.text_input_enabled = new_state;
        }
    }

    /// Enable or disable the compositor IME for the focused editable field.
    /// `cursor_rect` is surface-local pixel geometry for candidate windows;
    /// surrounding cursor/anchor offsets are UTF-8 byte offsets as required
    /// by text-input-v3. Returns `false` when the compositor lacks v3.
    pub fn set_text_input_state(
        &mut self,
        enabled: bool,
        surrounding_text: &str,
        cursor_byte: u32,
        anchor_byte: u32,
        cursor_rect: (i32, i32, i32, i32),
    ) -> bool {
        self.set_text_input_state_with_purpose(
            enabled,
            surrounding_text,
            cursor_byte,
            anchor_byte,
            cursor_rect,
            TextInputPurpose::Normal,
            false,
        )
    }

    /// Variant for fields with a semantic purpose or sensitive contents.
    /// Password/PIN hints ask the compositor not to retain typed input.
    #[allow(clippy::too_many_arguments)]
    pub fn set_text_input_state_with_purpose(
        &mut self,
        enabled: bool,
        surrounding_text: &str,
        cursor_byte: u32,
        anchor_byte: u32,
        cursor_rect: (i32, i32, i32, i32),
        purpose: TextInputPurpose,
        sensitive: bool,
    ) -> bool {
        let (Some(input), Some(_surface)) = (&self.state.text_input, &self.state.surface) else {
            return false;
        };
        if enabled {
            if !self.state.text_input_enabled {
                input.enable();
            }
            let secure =
                sensitive || matches!(purpose, TextInputPurpose::Password | TextInputPurpose::Pin);
            let (surrounding, cursor, anchor) = if secure {
                (String::new(), 0, 0)
            } else {
                (
                    surrounding_text.to_owned(),
                    utf8_byte_offset(surrounding_text, cursor_byte),
                    utf8_byte_offset(surrounding_text, anchor_byte),
                )
            };
            input.set_surrounding_text(surrounding, cursor, anchor);
            let mut hint = zwp_text_input_v3::ContentHint::None;
            let purpose = match purpose {
                TextInputPurpose::Normal => zwp_text_input_v3::ContentPurpose::Normal,
                TextInputPurpose::Password => zwp_text_input_v3::ContentPurpose::Password,
                TextInputPurpose::Pin => zwp_text_input_v3::ContentPurpose::Pin,
                TextInputPurpose::Digits => zwp_text_input_v3::ContentPurpose::Digits,
                TextInputPurpose::Email => zwp_text_input_v3::ContentPurpose::Email,
                TextInputPurpose::Url => zwp_text_input_v3::ContentPurpose::Url,
            };
            if secure {
                hint |= zwp_text_input_v3::ContentHint::SensitiveData;
            }
            if matches!(
                purpose,
                zwp_text_input_v3::ContentPurpose::Password
                    | zwp_text_input_v3::ContentPurpose::Pin
            ) {
                hint |= zwp_text_input_v3::ContentHint::HiddenText;
            }
            input.set_content_type(hint, purpose);
            input.set_cursor_rectangle(
                cursor_rect.0,
                cursor_rect.1,
                cursor_rect.2.max(0),
                cursor_rect.3.max(0),
            );
        } else if self.state.text_input_enabled {
            input.disable();
        } else {
            return true;
        }
        input.commit();
        self.state.text_input_enabled = enabled;
        true
    }

    /// Place `text` on the compositor selection as a `text/plain` /
    /// `text/plain;charset=utf-8` data source (GY-ZCODE-008). The payload is
    /// kept in an in-memory buffer and served from there when the compositor
    /// issues `data_source.send`. A safe no-op (documented) when the
    /// compositor does not advertise `wl_data_device_manager` or the seat has
    /// no data device.
    pub fn set_clipboard_text(&mut self, text: &str) {
        let (Some(manager), Some(device)) = (
            self.state.data_device_manager.clone(),
            self.state.data_device.clone(),
        ) else {
            // No data-device global: clipboard is unavailable, nothing to
            // set. Mirrors the defensive text-input pattern.
            return;
        };
        let qh = self.queue.handle();
        let source = manager.create_data_source(&qh, ());
        source.offer("text/plain".to_string());
        source.offer("text/plain;charset=utf-8".to_string());
        // Serial from the latest keyboard enter; compositors reject
        // set_selection with a stale serial, which the operator-verified
        // acceptance path covers.
        device.set_selection(Some(&source), self.state.keyboard_enter_serial);
        self.state.data_source = Some(source);
        self.state.clipboard.set_outgoing(text);
    }

    /// Request the current selection's text (GY-ZCODE-008). The completed
    /// value arrives as [`PlatformEvent::ClipboardText`] — `Some(text)` for a
    /// text payload, `None` when there is no selection, the offer has no
    /// text/plain mime, or the payload is empty. Exactly one request may be
    /// outstanding; a second call while one is pending is a documented
    /// no-op. Also a safe no-op without the data-device global. The read
    /// blocks briefly on the compositor writing the payload into a pipe.
    pub fn request_clipboard_text(&mut self) {
        if self.state.data_device.is_none() {
            return;
        }
        if !self.state.clipboard.begin_request() {
            // A read is already outstanding; drop the duplicate (frozen
            // contract: rejected, not queued).
            return;
        }
        let Some(mime) = self.state.clipboard.receive_target() else {
            // No selection offer (or no text mime): answered immediately.
            if let Some(event) = self.state.clipboard.complete_request(None) {
                self.state.events.push(event);
            }
            return;
        };
        let Some(offer) = self.state.pending_offer.clone() else {
            // Selection mimes exist but the offer object was lost (compositor
            // race); answer defensively rather than staying pending forever.
            if let Some(event) = self.state.clipboard.complete_request(None) {
                self.state.events.push(event);
            }
            return;
        };
        let Ok((read_fd, write_fd)) = pipe() else {
            if let Some(event) = self.state.clipboard.complete_request(None) {
                self.state.events.push(event);
            }
            return;
        };
        offer.receive(mime, write_fd.as_fd());
        // Flush so the compositor starts writing before we block on the pipe.
        let _ = self.queue.flush();
        let mut payload = Vec::new();
        let _ = File::from(read_fd).read_to_end(&mut payload);
        // Empty payload means the selection carried no usable text.
        let text = String::from_utf8(payload).ok().filter(|t| !t.is_empty());
        if let Some(event) = self.state.clipboard.complete_request(text) {
            self.state.events.push(event);
        }
    }

    /// Map the runtime's render list into shared memory and present it.
    /// `glyph_px` is the glyph-pixel size in screen pixels (block scaling);
    /// 1 = tiny terminal-style text, 3–4 reads well at 4 px/pt geometry.
    /// `fills` is a parallel slice indexed by primitive order (GY-WAVE2
    /// contract): `Some(0xRRGGBB)` overrides that primitive's flat fill
    /// colour; `None` keeps the existing default.
    ///
    /// `fgs` is a parallel slice indexed by primitive order (GY-ZCODE-009):
    /// `Some(0xRRGGBB)` overrides that primitive's text (glyph) colour;
    /// `None`, a short slice, or an empty slice falls back to the flat `fg`.
    /// Text primitives only — Panel fills and Button rectangle fills are
    /// unaffected (fills already own those), but a Button's *label* colour
    /// override via `fgs` is honoured by design (the label is text).
    pub fn present(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        fgs: &[Option<u32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_internal(
            primitives, fills, fgs, None, None, None, fg, bg, glyph_px, None, None, None,
        )
    }

    /// Present a tightly packed, top-down straight-alpha RGBA8 surface.
    /// Dimensions must match the current surface exactly; resizing belongs to
    /// the compositor configure path and the producer must render that size.
    /// The input is validated before any SHM write and is capped at
    /// [`MAX_RGBA_PIXELS`]. Pixels are copied into a released SHM slot using
    /// the same ownership and attach/commit path as [`GoyaWindow::present`].
    pub fn present_rgba(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), WaylandError> {
        validate_rgba_surface(rgba, width, height, self.state.width, self.state.height)?;
        let background = self.state.background_rgb;
        self.present_internal(
            &[],
            &[],
            &[],
            None,
            None,
            None,
            [0, 0, 0],
            background,
            1,
            None,
            Some(rgba),
            None,
        )
    }

    /// Render native scene primitives and then one clipped RGBA8 content pane
    /// in the same SHM frame. The borrowed source is sampled directly; no
    /// full-window intermediate RGBA buffer is created.
    #[allow(clippy::too_many_arguments)]
    pub fn present_composited(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        fgs: &[Option<u32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
        layer: Option<RgbaLayer<'_>>,
    ) -> Result<(), WaylandError> {
        if let Some(layer) = layer {
            validate_rgba_layer(layer, self.state.width, self.state.height)?;
        }
        self.present_internal(
            primitives, fills, fgs, None, None, None, fg, bg, glyph_px, None, None, layer,
        )
    }

    /// Present with per-primitive rectangular clip regions. Each optional
    /// clip is in surface-local millipoints and uses the same index as the
    /// primitive. Missing entries and `None` retain the full-surface clip.
    /// This keeps legacy [`GoyaWindow::present`] behavior unchanged while
    /// allowing scene hosts to enforce nested viewport clipping.
    pub fn present_clipped(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        clips: &[Option<Rect>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_internal(
            primitives,
            fills,
            &[],
            Some(clips),
            None,
            None,
            fg,
            bg,
            glyph_px,
            None,
            None,
            None,
        )
    }

    /// Present with both per-primitive clips and text sizes. The slices use
    /// primitive order, and missing entries retain the surface clip/default
    /// font metrics respectively.
    #[allow(clippy::too_many_arguments)]
    pub fn present_clipped_with_text_sizes(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        clips: &[Option<Rect>],
        text_sizes_pt: &[Option<f32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_internal(
            primitives,
            fills,
            &[],
            Some(clips),
            Some(text_sizes_pt),
            None,
            fg,
            bg,
            glyph_px,
            None,
            None,
            None,
        )
    }

    /// Present with per-primitive text sizes in points. Each optional size is
    /// indexed like `primitives`; absent or invalid sizes use `glyph_px`.
    /// The Wayland surface scale is four pixels per point.
    pub fn present_with_text_sizes(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        text_sizes_pt: &[Option<f32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_internal(
            primitives,
            fills,
            &[],
            None,
            Some(text_sizes_pt),
            None,
            fg,
            bg,
            glyph_px,
            None,
            None,
            None,
        )
    }

    /// Present text caret and selection overlays without modifying source
    /// strings. Overlay offsets are UTF-8 byte offsets, and geometry uses the
    /// same shaping, wrapping, size, clip and damage inputs as the text.
    #[allow(clippy::too_many_arguments)]
    pub fn present_with_text_overlays(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        clips: &[Option<Rect>],
        text_sizes_pt: &[Option<f32>],
        overlays: &[TextOverlay],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_internal(
            primitives,
            fills,
            &[],
            Some(clips),
            Some(text_sizes_pt),
            Some(overlays),
            fg,
            bg,
            glyph_px,
            None,
            None,
            None,
        )
    }

    /// LCPE-aware presentation: `damage` is the resolved dirty rect in
    /// pixels (from goya-change's quantised envelope); None = full frame.
    /// The slot is always fully rasterised (small surfaces); locality is
    /// expressed to the compositor via partial damage. See [`GoyaWindow::present`]
    /// for the `fills` and `fgs` contracts.
    // Parallel slices (fills/fgs) plus damage make this the one public
    // setter that legitimately exceeds the arity lint; the internal raster
    // paths carry the same allowance.
    #[allow(clippy::too_many_arguments)]
    pub fn present_dirty(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        fgs: &[Option<u32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
        damage: Option<(i32, i32, i32, i32)>,
    ) -> Result<(), WaylandError> {
        self.present_internal(
            primitives, fills, fgs, None, None, None, fg, bg, glyph_px, damage, None, None,
        )
    }

    /// Preserve per-node foregrounds and authored text sizes in one frame.
    #[allow(clippy::too_many_arguments)]
    pub fn present_styled(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        fgs: &[Option<u32>],
        text_sizes_pt: &[Option<f32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_internal(
            primitives,
            fills,
            fgs,
            None,
            Some(text_sizes_pt),
            None,
            fg,
            bg,
            glyph_px,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    /// Whether the latest present attached a buffer and committed the surface.
    pub fn frame_committed(&self) -> bool {
        self.state.frame_committed
    }

    fn present_internal(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        fgs: &[Option<u32>],
        clips: Option<&[Option<Rect>]>,
        text_sizes_pt: Option<&[Option<f32>]>,
        text_overlays: Option<&[TextOverlay]>,
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
        damage: Option<(i32, i32, i32, i32)>,
        rgba_pixels: Option<&[u8]>,
        rgba_layer: Option<RgbaLayer<'_>>,
    ) -> Result<(), WaylandError> {
        self.state.frame_committed = false;
        self.state.background_rgb = bg;
        if self.state.buffers.is_none() {
            let shm = self
                .state
                .shm
                .clone()
                .ok_or(WaylandError::MissingGlobal("wl_shm"))?;
            let qh = self.queue.handle();
            self.state.buffers = Some(BufferPair::new(
                &shm,
                &qh,
                self.state.width,
                self.state.height,
            )?);
        }
        // P0 (audit): NEVER write an unreleased slot. Pick any released
        // slot (preferring not-the-front); if all three are still owned by
        // the compositor, skip this presentation entirely — pump() will
        // process releases and the caller retries next frame.
        let pair = self.state.buffers.as_mut().unwrap();
        let idx = (0..3)
            .find(|&i| i != pair.front && pair.slots[i].released)
            .or_else(|| {
                // Fall back to the front slot itself if it was released
                // (compositor already swapped away from it).
                pair.slots[pair.front].released.then_some(pair.front)
            });
        let Some(idx) = idx else {
            return Ok(()); // all slots owned; nothing safe to write
        };
        // Incremental raster (audit P1): with a resolved damage rect, the
        // undamaged area is preserved by copying the front slot wholesale
        // (compositor-safe read), then repainting only the damage area.
        if damage.is_some() && idx != pair.front {
            // Distinct slots in one allocation; split borrows via indices.
            let (front, mid) = pair.slots.split_at_mut(pair.front.max(idx));
            let (src, dst) = if pair.front > idx {
                (&mid[0], &mut front[idx])
            } else {
                (&front[pair.front], &mut mid[0])
            };
            dst.pixels_mut().copy_from_slice(src.pixels());
        }
        let parallel_raster = self.state.parallel_raster;
        let (width, height) = (self.state.width, self.state.height);
        let pixels = pair.slots[idx].pixels_mut();
        if let Some(raw_pixels) = rgba_pixels {
            if raw_pixels.len() != pixels.len().saturating_mul(4) {
                return Err(WaylandError::PixelBufferLength {
                    expected: pixels.len().saturating_mul(4),
                    actual: raw_pixels.len(),
                });
            }
            for (destination, source) in pixels.iter_mut().zip(raw_pixels.chunks_exact(4)) {
                *destination = rgba_pixel_to_argb8888(source, bg);
            }
        } else {
            match damage {
                Some((dx, dy, dw, dh)) => {
                    // Clear the damage area with the background first.
                    let drect = goya_layout::Rect {
                        x: i64::from(dx) * MP_PER_PX,
                        y: i64::from(dy) * MP_PER_PX,
                        width: i64::from(dw) * MP_PER_PX,
                        height: i64::from(dh) * MP_PER_PX,
                    };
                    fill_rect(pixels, width, height, &drect, bg);
                }
                None => {
                    pixels.fill(argb(bg));
                }
            }
            let clip = damage.map(|(dx, dy, dw, dh)| PixelClip {
                x0: dx.max(0) as usize,
                y0: dy.max(0) as usize,
                x1: (dx + dw).min(self.state.width).max(0) as usize,
                y1: (dy + dh).min(self.state.height).max(0) as usize,
            });
            raster_primitives(
                pixels,
                width,
                height,
                primitives,
                fills,
                fgs,
                fg,
                glyph_px,
                clips,
                text_sizes_pt,
                clip,
                parallel_raster,
            );
            if let Some(overlays) = text_overlays {
                raster_text_overlays(
                    pixels,
                    width,
                    height,
                    primitives,
                    clips,
                    text_sizes_pt,
                    overlays,
                    glyph_px,
                    clip,
                );
            }
            if let Some(layer) = rgba_layer {
                composite_layer(pixels, width, height, layer);
            }
        }
        // Opaque hint: the window always fills its bounds — the compositor
        // can skip blending against undefined content (flicker source).
        // `wl_region.add` is additive, so a resize replaces the region.
        {
            let surface = self
                .state
                .surface
                .as_ref()
                .ok_or(WaylandError::MissingGlobal("surface"))?;
            let extent = (self.state.width, self.state.height);
            if self.state.region_extent != Some(extent) {
                if let Some(region) = self.state.region.take() {
                    region.destroy();
                }
                self.state.region_extent = None;
            }
            if self.state.region.is_none() {
                if let Some(comp) = self.state.compositor.clone() {
                    let region = comp.create_region(&self.queue.handle(), ());
                    region.add(0, 0, extent.0, extent.1);
                    surface.set_opaque_region(Some(&region));
                    self.state.region = Some(region);
                    self.state.region_extent = Some(extent);
                }
            }
        }
        let surface = self
            .state
            .surface
            .as_ref()
            .ok_or(WaylandError::MissingGlobal("surface"))?;
        surface.attach(Some(&pair.slots[idx].buffer), 0, 0);
        // LCPE locality: damage only the changed region when the caller
        // resolved one; otherwise the whole surface.
        match damage {
            Some((dx, dy, dw, dh)) => surface.damage(dx, dy, dw, dh),
            None => surface.damage(0, 0, self.state.width, self.state.height),
        }
        if let Some(serial) = self.state.pending_configure.take() {
            if let Some(xdg_surface) = &self.state.xdg_surface {
                xdg_surface.ack_configure(serial);
            }
        }
        surface.commit();
        pair.slots[idx].released = false;
        pair.front = idx;
        self.state.frame_committed = true;
        Ok(())
    }

    /// Dispatch pending compositor messages and return queued platform events.
    /// Text-input-v3 preedit/commit frames are drained from the pure
    /// [`TextInputState`] machine here, after `done` dispatch, so upstream
    /// consumers always see atomically applied IME updates.
    /// Opt into cursor-aware preedit and surrounding-text deletion events.
    /// Enable Alt+Left/Right and Ctrl+R/F5 browser navigation events.
    pub fn set_browser_navigation_keys(&mut self, enabled: bool) {
        self.state.browser_navigation_keys = enabled;
    }

    pub fn set_extended_text_input_events(&mut self, enabled: bool) {
        self.state.extended_text_input_events = enabled;
    }

    pub fn poll(&mut self) -> Vec<PlatformEvent> {
        let _ = self.queue.dispatch_pending(&mut self.state);
        let _ = self.queue.flush();
        let drained = self.state.text_input_state.drain();
        let mut events = std::mem::take(&mut self.state.events);
        events.extend(drained.into_iter().filter_map(|event| {
            legacy_text_input_event(event, self.state.extended_text_input_events)
        }));
        events
    }

    /// Run bounded dispatch roundtrips so configure/pointer events land.
    pub fn pump(&mut self, rounds: u32) {
        for _ in 0..rounds {
            let _ = self.queue.dispatch_pending(&mut self.state);
            let _ = self.queue.roundtrip(&mut self.state);
            if self.state.close_requested {
                break;
            }
        }
    }

    /// Whether the window was closed.
    pub fn close_requested(&self) -> bool {
        self.state.close_requested
    }

    /// Compositor width in pixels (fixed in this minimal scope).
    pub fn size(&self) -> (i32, i32) {
        (self.state.width, self.state.height)
    }
}

fn composite_layer(pixels: &mut [u32], width: i32, height: i32, layer: RgbaLayer<'_>) {
    let mut x0 = i64::from(layer.x).max(0);
    let mut y0 = i64::from(layer.y).max(0);
    let mut x1 = (i64::from(layer.x) + i64::from(layer.width)).min(i64::from(width));
    let mut y1 = (i64::from(layer.y) + i64::from(layer.height)).min(i64::from(height));
    if let Some(clip) = layer.clip {
        x0 = x0.max(i64::from(clip.x));
        y0 = y0.max(i64::from(clip.y));
        x1 = x1.min(i64::from(clip.x) + i64::from(clip.width));
        y1 = y1.min(i64::from(clip.y) + i64::from(clip.height));
    }
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for dy in y0..y1 {
        for dx in x0..x1 {
            let sx = (dx - i64::from(layer.x)) as usize;
            let sy = (dy - i64::from(layer.y)) as usize;
            let source = &layer.rgba[sy * layer.stride + sx * 4..sy * layer.stride + sx * 4 + 4];
            let alpha = u32::from(source[3]);
            let dst = &mut pixels[dy as usize * width as usize + dx as usize];
            let old = dst.to_le_bytes();
            let blend = |src: u8, back: u8| {
                ((u32::from(src) * alpha + u32::from(back) * (255 - alpha) + 127) / 255) as u8
            };
            *dst = u32::from_le_bytes([
                blend(source[0], old[2]),
                blend(source[1], old[1]),
                blend(source[2], old[0]),
                255,
            ]);
        }
    }
}

fn utf8_byte_offset(text: &str, requested: u32) -> i32 {
    let mut offset = (requested as usize).min(text.len()).min(i32::MAX as usize);
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset as i32
}

fn argb(rgb: [u8; 3]) -> u32 {
    0xFF00_0000 | ((rgb[2] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[0] as u32
}

fn checked_rgba_len(width: u32, height: u32) -> Result<(usize, usize), WaylandError> {
    let Some(pixel_count) = (width as usize).checked_mul(height as usize) else {
        return Err(WaylandError::PixelBufferTooLarge { width, height });
    };
    let Some(byte_len) = pixel_count.checked_mul(4) else {
        return Err(WaylandError::PixelBufferTooLarge { width, height });
    };
    if width == 0 || height == 0 || pixel_count > MAX_RGBA_PIXELS {
        return Err(WaylandError::PixelBufferTooLarge { width, height });
    }
    Ok((pixel_count, byte_len))
}

fn validate_rgba_surface(
    rgba: &[u8],
    width: u32,
    height: u32,
    surface_width: i32,
    surface_height: i32,
) -> Result<usize, WaylandError> {
    let (_, expected) = checked_rgba_len(width, height)?;
    if width != surface_width.max(0) as u32 || height != surface_height.max(0) as u32 {
        return Err(WaylandError::PixelSurfaceMismatch {
            width,
            height,
            surface_width,
            surface_height,
        });
    }
    if rgba.len() != expected {
        return Err(WaylandError::PixelBufferLength {
            expected,
            actual: rgba.len(),
        });
    }
    Ok(expected)
}

/// Convert tightly packed, top-down straight-alpha RGBA8 into the opaque word
/// layout used by the primitive rasterizer, compositing against the current
/// background before the buffer is advertised as opaque.
fn rgba_pixel_to_argb8888(pixel: &[u8], background: [u8; 3]) -> u32 {
    let alpha = u32::from(pixel[3]);
    let inverse = 255 - alpha;
    let blend = |channel: usize| {
        ((u32::from(pixel[channel]) * alpha + u32::from(background[channel]) * inverse + 127) / 255)
            as u8
    };
    argb([blend(0), blend(1), blend(2)])
}

#[cfg(test)]
mod pixel_surface_tests {
    use super::{
        argb, checked_rgba_len, rgba_pixel_to_argb8888, validate_rgba_surface, WaylandError,
        MAX_RGBA_PIXELS,
    };

    #[test]
    fn straight_alpha_rgba_composites_over_background() {
        let background = [0, 0, 255];
        let pixels = [
            rgba_pixel_to_argb8888(&[255, 0, 0, 128], background),
            rgba_pixel_to_argb8888(&[12, 34, 56, 255], background),
            rgba_pixel_to_argb8888(&[1, 2, 3, 0], background),
        ];
        assert_eq!(
            pixels,
            [argb([128, 0, 127]), argb([12, 34, 56]), argb(background)]
        );
    }

    #[test]
    fn pixel_surface_validation_checks_dimensions_length_and_limits() {
        assert_eq!(validate_rgba_surface(&[0; 8], 2, 1, 2, 1), Ok(8));
        assert_eq!(
            validate_rgba_surface(&[0; 7], 2, 1, 2, 1),
            Err(WaylandError::PixelBufferLength {
                expected: 8,
                actual: 7,
            })
        );
        assert!(matches!(
            validate_rgba_surface(&[0; 8], 2, 1, 1, 2),
            Err(WaylandError::PixelSurfaceMismatch { .. })
        ));
        assert_eq!(
            checked_rgba_len((MAX_RGBA_PIXELS as u32) + 1, 1),
            Err(WaylandError::PixelBufferTooLarge {
                width: (MAX_RGBA_PIXELS as u32) + 1,
                height: 1,
            })
        );
        assert_eq!(
            checked_rgba_len(u32::MAX, u32::MAX),
            Err(WaylandError::PixelBufferTooLarge {
                width: u32::MAX,
                height: u32::MAX,
            })
        );
    }
}

/// Resolve a primitive's fill colour for its index `i` (GY-WAVE2 contract):
/// `fills[i] = Some(0xRRGGBB)` wins; `None` (or a short slice) falls back to
/// the existing flat default. Returns the RGB triple for rasterisation.
fn resolve_fill(fills: &[Option<u32>], i: usize, default: [u8; 3]) -> [u8; 3] {
    resolve_color(fills, i, default)
}

/// Shared resolver for per-primitive colour slices (GY-WAVE2 `fills`
/// contract, extended in GY-ZCODE-009 to text `fgs`): `slice[i] =
/// Some(0xRRGGBB)` wins; `None`, a short slice, or an empty slice falls
/// back to the flat default, keeping empty-slice output byte-identical to
/// the pre-slice behaviour.
fn resolve_color(slice: &[Option<u32>], i: usize, default: [u8; 3]) -> [u8; 3] {
    match slice.get(i).copied().flatten() {
        Some(rgb) => [
            ((rgb >> 16) & 0xFF) as u8,
            ((rgb >> 8) & 0xFF) as u8,
            (rgb & 0xFF) as u8,
        ],
        None => default,
    }
}

const RASTER_TILE_SIZE: u32 = 32;
const MAX_RASTER_WORKERS: u32 = 4;

#[allow(clippy::too_many_arguments)]
fn raster_primitives(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    primitives: &[goya_runtime::Primitive],
    fills: &[Option<u32>],
    fgs: &[Option<u32>],
    fg: [u8; 3],
    glyph_px: u32,
    clips: Option<&[Option<Rect>]>,
    text_sizes_pt: Option<&[Option<f32>]>,
    clip: Option<PixelClip>,
    allow_parallel: bool,
) {
    let roi = clip.unwrap_or(PixelClip {
        x0: 0,
        y0: 0,
        x1: width.max(0) as usize,
        y1: height.max(0) as usize,
    });
    let dirty = goya_change::PixelRect {
        x: roi.x0 as i32,
        y: roi.y0 as i32,
        width: roi.x1.saturating_sub(roi.x0) as i32,
        height: roi.y1.saturating_sub(roi.y0) as i32,
    };
    let workers = std::thread::available_parallelism()
        .map(|n| n.get().min(MAX_RASTER_WORKERS as usize) as u32)
        .unwrap_or(1);
    let policy = goya_csr::select_policy(
        u64::from(dirty.width.max(0) as u32) * u64::from(dirty.height.max(0) as u32),
        RASTER_TILE_SIZE,
        workers,
    );
    // Cosmic-text raster callbacks use local glyph images whose fractional
    // placement can differ at tile boundaries. Keep text scenes serial until
    // the shaped runs can be cached and rasterised with a shared global origin.
    let contains_text = primitives.iter().any(|primitive| {
        matches!(
            primitive,
            goya_runtime::Primitive::Text(_, _) | goya_runtime::Primitive::Button(_, _)
        )
    });
    let has_custom_options = clips.is_some_and(|items| items.iter().any(Option::is_some))
        || text_sizes_pt.is_some_and(|items| items.iter().any(Option::is_some));
    if allow_parallel
        && !contains_text
        && !has_custom_options
        && policy != CsrPolicy::Serial
        && workers > 1
        && raster_parallel_tiles(
            pixels, width, height, primitives, fills, fgs, fg, glyph_px, roi, dirty, policy,
            workers,
        )
    {
        return;
    }
    raster_serial(
        pixels,
        width,
        height,
        primitives,
        RasterConfig {
            fills,
            fgs,
            fg,
            glyph_px,
            clips,
            text_sizes_pt,
            clip: Some(roi),
        },
    );
}

struct RasterConfig<'a> {
    fills: &'a [Option<u32>],
    fgs: &'a [Option<u32>],
    fg: [u8; 3],
    glyph_px: u32,
    clips: Option<&'a [Option<Rect>]>,
    text_sizes_pt: Option<&'a [Option<f32>]>,
    clip: Option<PixelClip>,
}

fn raster_serial(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    primitives: &[goya_runtime::Primitive],
    config: RasterConfig<'_>,
) {
    for (i, prim) in primitives.iter().enumerate() {
        raster_primitive(
            pixels,
            width,
            height,
            prim,
            config.fills,
            config.fgs,
            i,
            config.fg,
            config.glyph_px,
            config
                .clips
                .and_then(|clips| clips.get(i).copied().flatten()),
            config
                .text_sizes_pt
                .and_then(|sizes| sizes.get(i).copied().flatten()),
            config.clip,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn raster_parallel_tiles(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    primitives: &[goya_runtime::Primitive],
    fills: &[Option<u32>],
    fgs: &[Option<u32>],
    fg: [u8; 3],
    glyph_px: u32,
    roi: PixelClip,
    dirty: goya_change::PixelRect,
    policy: CsrPolicy,
    worker_budget: u32,
) -> bool {
    if primitives.iter().any(|primitive| {
        matches!(
            primitive,
            goya_runtime::Primitive::Text(_, _) | goya_runtime::Primitive::Button(_, _)
        )
    }) {
        return false;
    }
    let territories = goya_csr::partition(dirty, RASTER_TILE_SIZE, policy, worker_budget);
    let plan = goya_csr::schedule(&territories);
    if plan.len() <= 4 {
        return false;
    }

    // No P/E topology is detected or affinity applied here. HCAR classifies
    // each tile as General, so explicitly enabled workers remain OS-managed.
    let classifier = Classifier {
        heterogeneous: false,
        affinity_mode: AffinityMode::OsManaged,
    };
    let mut by_worker: Vec<Vec<PixelClip>> = vec![Vec::new(); worker_budget as usize];
    for (tile, worker) in plan {
        let x0 = (tile.x.max(roi.x0 as i32)) as usize;
        let y0 = (tile.y.max(roi.y0 as i32)) as usize;
        let x1 = (tile
            .x
            .saturating_add(tile.size as i32)
            .min(roi.x1 as i32)
            .min(width))
        .max(0) as usize;
        let y1 = (tile
            .y
            .saturating_add(tile.size as i32)
            .min(roi.y1 as i32)
            .min(height))
        .max(0) as usize;
        if x0 >= x1 || y0 >= y1 || worker as usize >= by_worker.len() {
            return false;
        }
        let item = WorkItem {
            deadline_ms: 16.67,
            critical: false,
            cost: WorkCost {
                time_p: (primitives.len().max(1) as f64) * 0.01,
                time_e: (primitives.len().max(1) as f64) * 0.02,
                energy_p: 1.0,
                energy_e: 0.7,
            },
        };
        if classifier.classify(&item) != CoreAssignment::General {
            return false;
        }
        by_worker[worker as usize].push(PixelClip { x0, y0, x1, y1 });
    }

    let expected_tiles: usize = by_worker.iter().map(Vec::len).sum();
    let base: &[u32] = pixels;
    let rendered = std::thread::scope(|scope| {
        let handles = by_worker
            .into_iter()
            .map(|tiles| {
                scope.spawn(move || {
                    tiles
                        .into_iter()
                        .map(|tile| {
                            let tile_w = tile.x1 - tile.x0;
                            let tile_h = tile.y1 - tile.y0;
                            let mut output = Vec::with_capacity(tile_w * tile_h);
                            for y in tile.y0..tile.y1 {
                                let start = y * width as usize + tile.x0;
                                output.extend_from_slice(&base[start..start + tile_w]);
                            }
                            let local_clip = PixelClip {
                                x0: 0,
                                y0: 0,
                                x1: tile_w,
                                y1: tile_h,
                            };
                            for (i, prim) in primitives.iter().enumerate().filter(|(_, prim)| {
                                intersects_tile(prim, tile.x0, tile.y0, tile.x1, tile.y1, glyph_px)
                            }) {
                                raster_primitive_offset(
                                    &mut output,
                                    tile_w as i32,
                                    tile_h as i32,
                                    prim,
                                    fills,
                                    fgs,
                                    i,
                                    fg,
                                    glyph_px,
                                    local_clip,
                                    tile.x0,
                                    tile.y0,
                                );
                            }
                            (tile, output)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .flatten()
            .collect::<Vec<_>>()
    });
    if rendered.len() != expected_tiles {
        return false;
    }

    // Merge complete, disjoint tile results in a fixed order. Each tile has
    // replayed the full primitive list in original draw order.
    for (tile, output) in rendered {
        let tile_w = tile.x1 - tile.x0;
        for row in 0..(tile.y1 - tile.y0) {
            let dst = (tile.y0 + row) * width as usize + tile.x0;
            let src = row * tile_w;
            pixels[dst..dst + tile_w].copy_from_slice(&output[src..src + tile_w]);
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn raster_primitive(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    prim: &goya_runtime::Primitive,
    fills: &[Option<u32>],
    fgs: &[Option<u32>],
    index: usize,
    fg: [u8; 3],
    glyph_px: u32,
    node_clip: Option<Rect>,
    text_size_pt: Option<f32>,
    clip: Option<PixelClip>,
) {
    let clip = match node_clip {
        Some(rect) => intersect_pixel_clips(clip, Some(pixel_clip_for_rect(&rect, width, height))),
        None => clip,
    };
    // GY-ZCODE-009: fgs overrides only the glyph colour; Panel/Button
    // rectangle fills stay owned by `fills`. A Button's label is text, so its
    // label colour override via `fgs` is honoured by design.
    let text_fg = resolve_color(fgs, index, fg);
    match prim {
        goya_runtime::Primitive::Panel(_) => {}
        goya_runtime::Primitive::Text(rect, text) => {
            let fill = resolve_fill(fills, index, [16, 16, 20]);
            fill_rect_clipped(pixels, width, height, rect, fill, clip);
            draw_glyphs(
                pixels,
                width,
                height,
                rect,
                text,
                text_fg,
                glyph_px,
                text_size_pt,
                clip,
            );
        }
        goya_runtime::Primitive::Button(rect, text) => {
            let fill = resolve_fill(fills, index, [80, 100, 180]);
            fill_rect_clipped(pixels, width, height, rect, fill, clip);
            draw_glyphs(
                pixels,
                width,
                height,
                rect,
                text,
                text_fg,
                glyph_px,
                text_size_pt,
                clip,
            );
        }
    }
}

fn pixel_clip_for_rect(rect: &Rect, width: i32, height: i32) -> PixelClip {
    let x0 = (rect.x / MP_PER_PX).max(0) as usize;
    let y0 = (rect.y / MP_PER_PX).max(0) as usize;
    let right = rect.x.saturating_add(rect.width.max(0));
    let bottom = rect.y.saturating_add(rect.height.max(0));
    let x1 = ((right.saturating_add(MP_PER_PX - 1)) / MP_PER_PX)
        .min(i64::from(width.max(0)))
        .max(0) as usize;
    let y1 = ((bottom.saturating_add(MP_PER_PX - 1)) / MP_PER_PX)
        .min(i64::from(height.max(0)))
        .max(0) as usize;
    PixelClip { x0, y0, x1, y1 }
}

fn intersect_pixel_clips(a: Option<PixelClip>, b: Option<PixelClip>) -> Option<PixelClip> {
    match (a, b) {
        (Some(a), Some(b)) => Some(PixelClip {
            x0: a.x0.max(b.x0),
            y0: a.y0.max(b.y0),
            x1: a.x1.min(b.x1),
            y1: a.y1.min(b.y1),
        }),
        (Some(clip), None) | (None, Some(clip)) => Some(clip),
        (None, None) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn raster_primitive_offset(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    prim: &goya_runtime::Primitive,
    fills: &[Option<u32>],
    fgs: &[Option<u32>],
    index: usize,
    fg: [u8; 3],
    glyph_px: u32,
    clip: PixelClip,
    origin_x: usize,
    origin_y: usize,
) {
    let offset = |r: &Rect| Rect {
        x: r.x - origin_x as i64 * MP_PER_PX,
        y: r.y - origin_y as i64 * MP_PER_PX,
        width: r.width,
        height: r.height,
    };
    // GY-ZCODE-009: same fgs-overrides-glyph-colour contract as
    // raster_primitive, so serial and parallel tiles stay byte-identical.
    let text_fg = resolve_color(fgs, index, fg);
    match prim {
        goya_runtime::Primitive::Panel(_) => {}
        goya_runtime::Primitive::Text(rect, text) => {
            let rect = offset(rect);
            let fill = resolve_fill(fills, index, [16, 16, 20]);
            fill_rect_clipped(pixels, width, height, &rect, fill, Some(clip));
            draw_glyphs(
                pixels,
                width,
                height,
                &rect,
                text,
                text_fg,
                glyph_px,
                None,
                Some(clip),
            );
        }
        goya_runtime::Primitive::Button(rect, text) => {
            let rect = offset(rect);
            let fill = resolve_fill(fills, index, [80, 100, 180]);
            fill_rect_clipped(pixels, width, height, &rect, fill, Some(clip));
            draw_glyphs(
                pixels,
                width,
                height,
                &rect,
                text,
                text_fg,
                glyph_px,
                None,
                Some(clip),
            );
        }
    }
}

fn intersects_tile(
    prim: &goya_runtime::Primitive,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    glyph_px: u32,
) -> bool {
    let rect = prim_rect(prim);
    let (mut rx0, mut ry0) = (rect.x, rect.y);
    let (mut rx1, mut ry1) = (rect.x + rect.width, rect.y + rect.height);
    if let goya_runtime::Primitive::Text(_, text) | goya_runtime::Primitive::Button(_, text) = prim
    {
        let glyph = i64::from(glyph_px.max(1));
        // Multi-line text stacks at 1.4 * glyph_px line height.
        let lines = text.split('\n').count() as i64;
        rx0 = rx0.min(rect.x + 2 * MP_PER_PX);
        ry0 = ry0.min(rect.y + 2 * MP_PER_PX);
        rx1 =
            rx1.max((rect.x / MP_PER_PX + 2 + text.chars().count() as i64 * 6 * glyph) * MP_PER_PX);
        ry1 = ry1.max((rect.y / MP_PER_PX + 2 + 7 * glyph) * MP_PER_PX * lines);
    }
    let rx0 = rx0 / MP_PER_PX;
    let ry0 = ry0 / MP_PER_PX;
    let rx1 = (rx1 + MP_PER_PX - 1) / MP_PER_PX;
    let ry1 = (ry1 + MP_PER_PX - 1) / MP_PER_PX;
    rx0 < x1 as i64 && ry0 < y1 as i64 && rx1 > x0 as i64 && ry1 > y0 as i64
}

fn fill_rect(pixels: &mut [u32], win_w: i32, win_h: i32, r: &Rect, rgb: [u8; 3]) {
    fill_rect_clipped(pixels, win_w, win_h, r, rgb, None)
}

fn fill_rect_clipped(
    pixels: &mut [u32],
    win_w: i32,
    win_h: i32,
    r: &Rect,
    rgb: [u8; 3],
    clip: Option<PixelClip>,
) {
    let a = argb(rgb);
    let x0 = (r.x / MP_PER_PX).max(0);
    let y0 = (r.y / MP_PER_PX).max(0);
    let x1 = ((r.x + r.width) / MP_PER_PX).min(i64::from(win_w));
    let y1 = ((r.y + r.height) / MP_PER_PX).min(i64::from(win_h));
    let (cx0, cy0, cx1, cy1) = match clip {
        Some(c) => (c.x0 as i64, c.y0 as i64, c.x1 as i64, c.y1 as i64),
        None => (0, 0, i64::from(win_w), i64::from(win_h)),
    };
    let x0 = x0.max(cx0);
    let y0 = y0.max(cy0);
    let x1 = x1.min(cx1);
    let y1 = y1.min(cy1);
    for y in y0..y1 {
        for x in x0..x1 {
            let idx = (y as usize) * (win_w as usize) + (x as usize);
            if idx < pixels.len() {
                pixels[idx] = a;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyphs(
    pixels: &mut [u32],
    win_w: i32,
    win_h: i32,
    r: &Rect,
    s: &str,
    rgb: [u8; 3],
    glyph_px: u32,
    text_size_pt: Option<f32>,
    clip: Option<PixelClip>,
) {
    if draw_shaped_glyphs(
        pixels,
        win_w,
        win_h,
        r,
        s,
        rgb,
        glyph_px,
        text_size_pt,
        clip,
    ) {
        return;
    }
    draw_bitmap_glyphs(pixels, win_w, win_h, r, s, rgb, glyph_px, clip);
}

#[allow(clippy::too_many_arguments)]
fn draw_shaped_glyphs(
    pixels: &mut [u32],
    win_w: i32,
    win_h: i32,
    r: &Rect,
    s: &str,
    rgb: [u8; 3],
    glyph_px: u32,
    text_size_pt: Option<f32>,
    clip: Option<PixelClip>,
) -> bool {
    static TEXT: OnceLock<Mutex<(FontSystem, SwashCache)>> = OnceLock::new();
    let Ok(mut text) = TEXT
        .get_or_init(|| Mutex::new((FontSystem::new(), SwashCache::new())))
        .lock()
    else {
        return false;
    };
    let (font_system, cache) = &mut *text;
    let fallback_font_size = (glyph_px.max(1) as f32) * 7.0;
    let font_size = text_size_pt
        .filter(|size| size.is_finite() && *size > 0.0)
        .map(|size| size.clamp(0.25, 256.0) * 4.0)
        .unwrap_or(fallback_font_size);
    let width = (r.width / MP_PER_PX - 4).max(1) as f32;
    let height = (r.height / MP_PER_PX - 4).max(1) as f32;
    let buffer = shape_text(font_system, s, width, height, font_size);

    let (cx0, cy0, cx1, cy1) = match clip {
        Some(c) => (c.x0 as i64, c.y0 as i64, c.x1 as i64, c.y1 as i64),
        None => (0, 0, i64::from(win_w), i64::from(win_h)),
    };
    let origin_x = r.x / MP_PER_PX + 2;
    let origin_y = r.y / MP_PER_PX + 2;
    let rect_right = (r.x + r.width) / MP_PER_PX + 2;
    let rect_bottom = (r.y + r.height) / MP_PER_PX + 2;
    let color = Color::rgb(rgb[0], rgb[1], rgb[2]);
    let mut drew = false;
    buffer.draw(font_system, cache, color, |x, y, w, h, color| {
        let [cr, cg, cb, alpha] = color.as_rgba();
        if alpha == 0 {
            return;
        }
        for dy in 0..h as i64 {
            let py = origin_y + i64::from(y) + dy;
            if py < cy0 || py >= cy1 || py < 0 || py >= i64::from(win_h) || py >= rect_bottom {
                continue;
            }
            for dx in 0..w as i64 {
                let px = origin_x + i64::from(x) + dx;
                if px < cx0 || px >= cx1 || px < 0 || px >= i64::from(win_w) || px >= rect_right {
                    continue;
                }
                let index = py as usize * win_w as usize + px as usize;
                if index >= pixels.len() {
                    continue;
                }
                let dst = pixels[index];
                let inv = 255 - alpha as u32;
                let blend = |src: u8, old: u32, shift: u32| -> u32 {
                    (u32::from(src) * alpha as u32 + ((old >> shift) & 0xff) * inv + 127) / 255
                };
                pixels[index] = 0xff00_0000
                    | (blend(cr, dst, 16) << 16)
                    | (blend(cg, dst, 8) << 8)
                    | blend(cb, dst, 0);
                drew = true;
            }
        }
    });
    drew
}

#[allow(clippy::too_many_arguments)]
fn raster_text_overlays(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    primitives: &[goya_runtime::Primitive],
    clips: Option<&[Option<Rect>]>,
    text_sizes_pt: Option<&[Option<f32>]>,
    overlays: &[TextOverlay],
    glyph_px: u32,
    damage: Option<PixelClip>,
) {
    static TEXT: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    let Ok(mut font_system) = TEXT.get_or_init(|| Mutex::new(FontSystem::new())).lock() else {
        return;
    };
    for overlay in overlays {
        let Some(primitive) = primitives.get(overlay.primitive_index) else {
            continue;
        };
        let (rect, text) = match primitive {
            goya_runtime::Primitive::Text(rect, text)
            | goya_runtime::Primitive::Button(rect, text) => (rect, text.as_str()),
            goya_runtime::Primitive::Panel(_) => continue,
        };
        let size_pt = text_sizes_pt
            .and_then(|sizes| sizes.get(overlay.primitive_index).copied().flatten())
            .filter(|size| size.is_finite() && *size > 0.0);
        let font_px = size_pt
            .map(|size| size.clamp(0.25, 256.0) * 4.0)
            .unwrap_or((glyph_px.max(1) as f32) * 7.0);
        let text_width = (rect.width / MP_PER_PX - 4).max(1) as f32;
        let text_height = (rect.height / MP_PER_PX - 4).max(1) as f32;
        let buffer = shape_text(&mut font_system, text, text_width, text_height, font_px);
        let node_clip = clips
            .and_then(|items| items.get(overlay.primitive_index).copied().flatten())
            .map(|clip| pixel_clip_for_rect(&clip, width, height));
        let clip = intersect_pixel_clips(
            damage,
            intersect_pixel_clips(node_clip, Some(pixel_clip_for_rect(rect, width, height))),
        );
        let origin_x = (rect.x / MP_PER_PX + 2) as f32;
        let origin_y = (rect.y / MP_PER_PX + 2) as f32;

        if let Some((start, end)) = overlay.selection_byte_range {
            let start = snap_grapheme_boundary(text, start.min(text.len()));
            let end = snap_grapheme_boundary(text, end.min(text.len()));
            if start != end {
                let (start, end) = if start < end {
                    (start, end)
                } else {
                    (end, start)
                };
                if let (Some(start_cursor), Some(end_cursor)) = (
                    text_cursor(&buffer, text, start),
                    text_cursor(&buffer, text, end),
                ) {
                    for run in buffer.layout_runs() {
                        if run.line_i < start_cursor.line || run.line_i > end_cursor.line {
                            continue;
                        }
                        let line_start = if run.line_i == start_cursor.line {
                            start_cursor.index
                        } else {
                            0
                        };
                        let line_end = if run.line_i == end_cursor.line {
                            end_cursor.index
                        } else {
                            run.text.len()
                        };
                        for glyph in run.glyphs {
                            if glyph.end <= line_start || glyph.start >= line_end {
                                continue;
                            }
                            draw_overlay_rect(
                                pixels,
                                width,
                                height,
                                origin_x + glyph.x,
                                origin_y + run.line_top,
                                glyph.w.max(1.0),
                                run.line_height,
                                overlay.selection_rgb,
                                112,
                                clip,
                            );
                        }
                    }
                }
            }
        }

        if let Some(offset) = overlay.caret_byte_offset {
            let offset = snap_grapheme_boundary(text, offset.min(text.len()));
            if let Some(cursor) = text_cursor(&buffer, text, offset) {
                let mut caret = None;
                for run in buffer.layout_runs() {
                    if let Some(x) = caret_x_for_run(&run, cursor) {
                        caret = Some((origin_x + x, origin_y + run.line_top, run.line_height));
                        break;
                    }
                }
                if let Some((x, y, caret_height)) = caret {
                    draw_overlay_rect(
                        pixels,
                        width,
                        height,
                        x,
                        y,
                        1.0,
                        caret_height,
                        overlay.caret_rgb,
                        255,
                        clip,
                    );
                }
            }
        }
    }
}

fn caret_x_for_run(run: &cosmic_text::LayoutRun<'_>, cursor: cosmic_text::Cursor) -> Option<f32> {
    if run.line_i != cursor.line {
        return None;
    }
    let mut alternate = None;
    for glyph in run.glyphs {
        let ltr = glyph.level.is_ltr();
        if cursor.index == glyph.start {
            let x = if ltr { glyph.x } else { glyph.x + glyph.w };
            if cursor.affinity == cosmic_text::Affinity::After {
                return Some(x);
            }
            alternate = Some(x);
        }
        if cursor.index == glyph.end {
            let x = if ltr { glyph.x + glyph.w } else { glyph.x };
            if cursor.affinity == cosmic_text::Affinity::Before {
                return Some(x);
            }
            alternate = Some(x);
        }
    }
    alternate
}

#[allow(clippy::too_many_arguments)]
fn draw_overlay_rect(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rgb: [u8; 3],
    alpha: u8,
    clip: Option<PixelClip>,
) {
    if ![x, y, w, h].into_iter().all(f32::is_finite) || w <= 0.0 || h <= 0.0 {
        return;
    }
    let (cx0, cy0, cx1, cy1) = clip.map_or(
        (
            0usize,
            0usize,
            width.max(0) as usize,
            height.max(0) as usize,
        ),
        |c| (c.x0, c.y0, c.x1, c.y1),
    );
    let x0 = (x.floor() as i64).clamp(cx0 as i64, cx1 as i64);
    let y0 = (y.floor() as i64).clamp(cy0 as i64, cy1 as i64);
    let x1 = ((x + w).ceil() as i64).clamp(cx0 as i64, cx1 as i64);
    let y1 = ((y + h).ceil() as i64).clamp(cy0 as i64, cy1 as i64);
    for py in y0..y1 {
        for px in x0..x1 {
            if px < 0 || py < 0 || px >= i64::from(width) || py >= i64::from(height) {
                continue;
            }
            let index = py as usize * width as usize + px as usize;
            if index >= pixels.len() {
                continue;
            }
            let dst = pixels[index];
            let a = u32::from(alpha);
            let inv = 255 - a;
            let blend = |src: u8, shift: u32| {
                (u32::from(src) * a + ((dst >> shift) & 0xff) * inv + 127) / 255
            };
            pixels[index] = 0xff00_0000
                | (blend(rgb[0], 16) << 16)
                | (blend(rgb[1], 8) << 8)
                | blend(rgb[2], 0);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_bitmap_glyphs(
    pixels: &mut [u32],
    win_w: i32,
    win_h: i32,
    r: &Rect,
    s: &str,
    rgb: [u8; 3],
    glyph_px: u32,
    clip: Option<PixelClip>,
) {
    let a = argb(rgb);
    let g = glyph_px.max(1) as usize;
    let (cx0, cy0, cx1, cy1) = match clip {
        Some(c) => (c.x0, c.y0, c.x1, c.y1),
        None => (0, 0, win_w as usize, win_h as usize),
    };
    // Keep origins signed: tile-local rasterisation shifts a primitive by
    // the tile origin, and clamping here would move a glyph into the tile
    // instead of clipping the portion that begins before it.
    let ox = r.x / MP_PER_PX + 2;
    let oy = r.y / MP_PER_PX + 2;
    // Node rect bounds in pixels; every line is clipped to this rect
    // (GY-WAVE2 multi-line contract). Lines that start at or beyond the
    // rect's bottom edge are dropped entirely (threshold without the +2
    // glyph-origin inset, matching the rect the caller declared).
    let rect_x1 = (r.x + r.width) / MP_PER_PX + 2;
    let rect_bottom = (r.y + r.height) / MP_PER_PX;
    let rect_y1 = rect_bottom + 2;
    let line_height = ((glyph_px.max(1) as f64) * 1.4) as i64;
    let columns = (r.width.max(0) / MP_PER_PX / (6 * g as i64)).max(1) as usize;
    let lines = wrap_bitmap_text(s, columns);
    for (li, line) in lines.iter().enumerate() {
        let line_oy = oy + (li as i64) * line_height;
        if line_oy >= rect_bottom {
            break;
        }
        for (ci, ch) in line.chars().enumerate() {
            let glyph = glyph5x7(ch);
            for (gy, row) in glyph.iter().enumerate() {
                for gx in 0..5 {
                    if row & (1 << (4 - gx)) != 0 {
                        // Block-scale: each glyph pixel is g×g screen pixels.
                        for dy in 0..g {
                            let y = line_oy + (gy * g + dy) as i64;
                            if y >= cy1 as i64
                                || y < cy0 as i64
                                || y >= i64::from(win_h)
                                || y >= rect_y1
                            {
                                continue;
                            }
                            for dx in 0..g {
                                let x = ox + ((ci * 6 + gx) * g + dx) as i64;
                                if x >= cx1 as i64
                                    || x < cx0 as i64
                                    || x >= i64::from(win_w)
                                    || x >= rect_x1
                                {
                                    continue;
                                }
                                let idx = y as usize * (win_w as usize) + x as usize;
                                if idx < pixels.len() {
                                    pixels[idx] = a;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Wrap to the fixed bitmap font's measured cell width. Authored newlines
/// are retained; whitespace separates words; oversized words are split at
/// extended grapheme boundaries so the renderer never emits out-of-viewport
/// glyphs. This path intentionally matches the renderer's 6-cell advance.
fn wrap_bitmap_text(text: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut output = Vec::new();
    for authored_line in text.split('\n') {
        let mut line = String::new();
        let mut width = 0usize;
        for original_word in authored_line.split_whitespace() {
            let mut word = original_word;
            loop {
                let word_width = word.graphemes(true).count();
                let separator = usize::from(!line.is_empty());
                if width + separator + word_width <= columns {
                    if separator != 0 {
                        line.push(' ');
                    }
                    line.push_str(word);
                    width += separator + word_width;
                    break;
                }
                if !line.is_empty() {
                    output.push(std::mem::take(&mut line));
                    width = 0;
                    continue;
                }
                let split = word
                    .grapheme_indices(true)
                    .nth(columns)
                    .map(|(index, _)| index)
                    .unwrap_or(word.len());
                let head = &word[..split];
                word = &word[split..];
                output.push(head.to_owned());
                if word.is_empty() {
                    break;
                }
            }
        }
        output.push(line);
    }
    output
}

#[cfg(test)]
mod bitmap_wrap_tests {
    use super::wrap_bitmap_text;

    #[test]
    fn wraps_to_rendered_node_width_and_keeps_explicit_lines() {
        assert_eq!(
            wrap_bitmap_text("alpha beta\n\nlongword", 5),
            ["alpha", "beta", "", "longw", "ord"]
        );
    }

    #[test]
    fn hard_wrap_keeps_combining_and_zwj_graphemes_intact() {
        let wrapped = wrap_bitmap_text("a\u{301}👨‍👩‍👧‍👦z", 1);
        assert_eq!(wrapped, ["a\u{301}", "👨‍👩‍👧‍👦", "z"]);
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::glyph5x7;

    #[test]
    fn address_characters_are_distinct_from_the_unknown_box() {
        let unknown = [0b111110; 7];
        for ch in
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:/?._-~&=%#+<>|".chars()
        {
            assert_ne!(glyph5x7(ch), unknown, "{ch}");
        }
    }
}

/// 5x7 glyphs (leftmost pixel = bit 4); unknown characters render as a box.
/// ASCII letters are case-insensitive so an address can show `HTTPS`.
fn glyph5x7(c: char) -> [u8; 7] {
    let c = if c.is_ascii_uppercase() {
        c.to_ascii_lowercase()
    } else {
        c
    };
    match c {
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '/' => [
            0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        '*' => [0, 0b00100, 0b10101, 0b01110, 0b10101, 0b00100, 0],
        '%' => [
            0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011,
        ],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        'a' => [
            0b011110, 0b100010, 0b100010, 0b111110, 0b100010, 0b100010, 0b100010,
        ],
        'c' => [
            0b011110, 0b100000, 0b100000, 0b100000, 0b100000, 0b100000, 0b011110,
        ],
        'd' => [
            0b111100, 0b100010, 0b100010, 0b100010, 0b100010, 0b100010, 0b111100,
        ],
        'e' => [
            0b111110, 0b100000, 0b100000, 0b111100, 0b100000, 0b100000, 0b111110,
        ],
        'g' => [
            0b011110, 0b100000, 0b100000, 0b101110, 0b100010, 0b100010, 0b011110,
        ],
        'h' => [
            0b100010, 0b100010, 0b100010, 0b111110, 0b100010, 0b100010, 0b100010,
        ],
        'i' => [
            0b010000, 0b000000, 0b010000, 0b010000, 0b010000, 0b010000, 0b011110,
        ],
        'l' => [
            0b010000, 0b010000, 0b010000, 0b010000, 0b010000, 0b010000, 0b011110,
        ],
        'm' => [
            0b110011, 0b111111, 0b101101, 0b101101, 0b100001, 0b100001, 0b100001,
        ],
        'n' => [
            0b100010, 0b110010, 0b101010, 0b100110, 0b100010, 0b100010, 0b100010,
        ],
        'o' => [
            0b011110, 0b100010, 0b100010, 0b100010, 0b100010, 0b100010, 0b011110,
        ],
        'r' => [
            0b100100, 0b101000, 0b110000, 0b100000, 0b100000, 0b100000, 0b100000,
        ],
        't' => [
            0b010000, 0b010000, 0b111110, 0b010000, 0b010000, 0b010010, 0b001100,
        ],
        'u' => [
            0b100010, 0b100010, 0b100010, 0b100010, 0b100010, 0b100010, 0b011110,
        ],
        'v' => [
            0b100010, 0b100010, 0b100010, 0b100010, 0b010100, 0b010100, 0b001000,
        ],
        'y' => [
            0b100010, 0b100010, 0b011110, 0b001000, 0b001000, 0b010000, 0b010000,
        ],
        'b' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'f' => [
            0b00111, 0b00100, 0b01110, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'j' => [0b00001, 0, 0b00001, 0b00001, 0b00001, 0b10001, 0b01110],
        'k' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'p' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'q' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b00001,
        ],
        's' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'w' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'x' => [
            0b10001, 0b01010, 0b01010, 0b00100, 0b01010, 0b01010, 0b10001,
        ],
        'z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '?' => [0b01110, 0b10001, 0b00010, 0b00100, 0b00100, 0, 0b00100],
        '&' => [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
        '#' => [
            0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010,
        ],
        '@' => [
            0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
        ],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '~' => [0, 0, 0b01001, 0b10101, 0b10010, 0, 0],
        '|' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        '<' => [
            0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
        ],
        '>' => [
            0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
        ],
        ',' => [0, 0, 0, 0, 0b001100, 0b001000, 0b010000],
        '=' => [0, 0, 0b111110, 0, 0b111110, 0, 0],
        '!' => [
            0b001000, 0b001000, 0b001000, 0b001000, 0b001000, 0, 0b001000,
        ],
        ' ' => [0; 7],
        _ => [0b111110; 7],
    }
}

// ---- Dispatch plumbing -------------------------------------------------

impl Dispatch<WlRegistry, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            state.globals.push((name, interface));
        }
    }
}

impl Dispatch<WlCompositor, ()> for GoyaWindowInner {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: wayland_client::protocol::wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShm, ()> for GoyaWindowInner {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for GoyaWindowInner {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        buffer: &WlBuffer,
        event: wayland_client::protocol::wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_buffer::Event::Release = event {
            if let Some(pair) = state.buffers.as_mut() {
                for slot in &mut pair.slots {
                    if &slot.buffer == buffer {
                        slot.released = true;
                    }
                }
            }
        }
    }
}

impl Dispatch<WlSurface, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Store the surface handle the first time we create one via create().
        let _ = state;
    }
}

impl Dispatch<XdgWmBase, ()> for GoyaWindowInner {
    fn event(
        _: &mut Self,
        wm: &XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Must answer pings or the compositor kills the client.
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.configured = true;
            state.ack_serial = serial;
            state.pending_configure = Some(serial);
            state.events.push(PlatformEvent::Configured);
        }
    }
}

impl Dispatch<XdgToplevel, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Close = event {
            state.close_requested = true;
            state.events.push(PlatformEvent::CloseRequested);
        }
        if let xdg_toplevel::Event::Configure { width, height, .. } = event {
            if (width > 0 && width != state.width) || (height > 0 && height != state.height) {
                if width > 0 {
                    state.width = width;
                }
                if height > 0 {
                    state.height = height;
                }
                // Drop the fixed-size frame: the next present() reallocates.
                state.buffers = None;
                state
                    .events
                    .push(PlatformEvent::Resized(state.width, state.height));
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        {
            if caps.contains(Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for GoyaWindowInner {
    fn event(
        _: &mut Self,
        _: &ZwpTextInputManagerV3,
        _: zwp_text_input_manager_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Map native text-input-v3 callbacks into the staged batch. Kept separate
/// from the Wayland dispatch trait so the real adapter mapping is exercised
/// without a compositor in platform tests.
fn stage_text_input_event(state: &mut TextInputState, event: zwp_text_input_v3::Event) {
    match event {
        zwp_text_input_v3::Event::PreeditString {
            text,
            cursor_begin,
            cursor_end,
        } => state.feed_preedit_with_cursor(text, cursor_begin, cursor_end),
        zwp_text_input_v3::Event::CommitString { text } => state.feed_commit(text),
        zwp_text_input_v3::Event::DeleteSurroundingText {
            before_length,
            after_length,
        } => state.feed_delete_surrounding(before_length, after_length),
        zwp_text_input_v3::Event::Done { .. } => state.feed_done(),
        zwp_text_input_v3::Event::Leave { .. } => state.feed_leave(),
        zwp_text_input_v3::Event::Enter { .. }
        | zwp_text_input_v3::Event::Language { .. }
        | zwp_text_input_v3::Event::Action { .. }
        | zwp_text_input_v3::Event::PreeditHint { .. } => {}
        _ => {}
    }
}

impl Dispatch<ZwpTextInputV3, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        stage_text_input_event(&mut state.text_input_state, event);
    }
}

impl Dispatch<WlDataDeviceManager, ()> for GoyaWindowInner {
    fn event(
        _: &mut Self,
        _: &WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDevice, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // The compositor creates a fresh offer before every `selection`;
            // its `offer` mime announcements follow on the offer object.
            wl_data_device::Event::DataOffer { id } => {
                state.pending_offer = Some(id);
                state.pending_offer_mimes.clear();
            }
            wl_data_device::Event::Selection { id } => {
                match id {
                    Some(offer) => {
                        // Only accept the offer we saw announced; a foreign
                        // offer (or missing announcements) carries unknown
                        // mimes, so the read path must answer None, not guess.
                        let known = state.pending_offer.as_ref() == Some(&offer);
                        let mimes = if known {
                            std::mem::take(&mut state.pending_offer_mimes)
                        } else {
                            state.pending_offer_mimes.clear();
                            Vec::new()
                        };
                        state.pending_offer = Some(offer);
                        state.clipboard.stage_selection_mimes(mimes);
                    }
                    None => {
                        state.pending_offer = None;
                        state.pending_offer_mimes.clear();
                        state.clipboard.stage_selection_mimes(Vec::new());
                    }
                }
                // Any selection change resolves an in-flight read as "no
                // text" (answer None) so the caller never stays pending on
                // stale state.
                if let Some(resolved) = state.clipboard.cancel_request() {
                    state.events.push(resolved);
                }
            }
            _ => {}
        }
    }

    // `data_offer` is opcode 0 and creates the offer the selection event uses.
    wayland_client::event_created_child!(GoyaWindowInner, WlDataDevice, [
        0 => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataOffer, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            if state.pending_offer.as_ref() == Some(offer) {
                state.pending_offer_mimes.push(mime_type);
            }
        }
    }
}

impl Dispatch<WlDataSource, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        source: &WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // The compositor pasted our selection: serve the buffered
            // payload in bounded chunks from the pure serving cursor.
            wl_data_source::Event::Send { fd, .. } => {
                let mut file = File::from(fd);
                while let Some(chunk) = state.clipboard.serve_chunk(SEND_CHUNK_BYTES) {
                    if std::io::Write::write_all(&mut file, chunk).is_err() {
                        break;
                    }
                }
            }
            // The compositor replaced or dropped our selection; the source
            // and its payload are dead. Keep the bound data_source alive
            // only if it is a different object (defensive).
            wl_data_source::Event::Cancelled => {
                if state.data_source.as_ref() == Some(source) {
                    state.data_source = None;
                    state.clipboard.take_outgoing();
                }
                source.destroy();
            }
            _ => {}
        }
    }
}

/// Upper bound for one `data_source.send` write, keeping pipe backpressure
/// and buffer sizes bounded regardless of payload length.
const SEND_CHUNK_BYTES: usize = 16 * 1024;

/// US QWERTY evdev keycode → printable char, honouring shift.
/// Subset contract: printable ASCII only; unlisted keys yield None.
fn key_char(keycode: u32, shift: bool) -> Option<char> {
    let shifted = |mut c: char| {
        if shift {
            c = c.to_ascii_uppercase();
        }
        c
    };
    // evdev US QWERTY keycodes.
    let c = match keycode {
        16..=25 => {
            let row = ['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'];
            return Some(shifted(row[(keycode - 16) as usize]));
        }
        30..=38 => {
            let row = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
            return Some(shifted(row[(keycode - 30) as usize]));
        }
        44..=50 => {
            // z x c v b n m — not ASCII-contiguous; explicit table.
            let row = ['z', 'x', 'c', 'v', 'b', 'n', 'm'];
            return Some(shifted(row[(keycode - 44) as usize]));
        }
        2 => {
            if shift {
                '!'
            } else {
                '1'
            }
        }
        3 => {
            if shift {
                '@'
            } else {
                '2'
            }
        }
        4 => {
            if shift {
                '#'
            } else {
                '3'
            }
        }
        5 => {
            if shift {
                '$'
            } else {
                '4'
            }
        }
        6 => {
            if shift {
                '%'
            } else {
                '5'
            }
        }
        7 => {
            if shift {
                '^'
            } else {
                '6'
            }
        }
        8 => {
            if shift {
                '&'
            } else {
                '7'
            }
        }
        9 => {
            if shift {
                '*'
            } else {
                '8'
            }
        }
        10 => {
            if shift {
                '('
            } else {
                '9'
            }
        }
        11 => {
            if shift {
                ')'
            } else {
                '0'
            }
        }
        12 => {
            if shift {
                '_'
            } else {
                '-'
            }
        }
        13 => {
            if shift {
                '+'
            } else {
                '='
            }
        }
        26 => {
            if shift {
                '{'
            } else {
                '['
            }
        }
        27 => {
            if shift {
                '}'
            } else {
                ']'
            }
        }
        14 => '\u{8}', // backspace
        28 => '\n',    // enter
        39 => {
            if shift {
                ':'
            } else {
                ';'
            }
        } // semicolon
        40 => {
            if shift {
                '"'
            } else {
                '\''
            }
        } // apostrophe
        41 => {
            if shift {
                '~'
            } else {
                '`'
            }
        } // grave
        42 | 54 => return None, // shift keys
        43 => {
            if shift {
                '|'
            } else {
                '\\'
            }
        } // backslash
        51 => {
            if shift {
                '<'
            } else {
                ','
            }
        }
        52 => {
            if shift {
                '>'
            } else {
                '.'
            }
        }
        53 => {
            if shift {
                '?'
            } else {
                '/'
            }
        }
        57 => ' ', // space
        _ => return None,
    };
    Some(c)
}

/// evdev keycode → navigation/editing key (GY-WAVE2 contract, extended in
/// GY-ZCODE-003 with PageUp 104 / PageDown 109).
/// Unmapped keycodes yield None; Enter/Backspace stay in `key_char`.
fn key_special(keycode: u32) -> Option<KeySpecial> {
    let ks = match keycode {
        105 => KeySpecial::ArrowLeft,
        106 => KeySpecial::ArrowRight,
        103 => KeySpecial::ArrowUp,
        108 => KeySpecial::ArrowDown,
        102 => KeySpecial::Home,
        107 => KeySpecial::End,
        111 => KeySpecial::Delete,
        104 => KeySpecial::PageUp,
        109 => KeySpecial::PageDown,
        1 => KeySpecial::Escape,
        15 => KeySpecial::Tab,
        _ => return None,
    };
    Some(ks)
}

/// Maximum compositor keymap accepted by the Wayland client. Real XKB text
/// keymaps are normally well below this bound; rejecting larger descriptors
/// avoids unbounded allocations from a malformed or hostile compositor.
const MAX_XKB_KEYMAP_BYTES: u32 = 16 * 1024 * 1024;

fn keymap_state_from_fd(
    context: &xkbcommon::xkb::Context,
    format: WEnum<wl_keyboard::KeymapFormat>,
    fd: OwnedFd,
    size: u32,
) -> Option<xkbcommon::xkb::State> {
    if format != WEnum::Value(wl_keyboard::KeymapFormat::XkbV1)
        || size == 0
        || size > MAX_XKB_KEYMAP_BYTES
    {
        return None;
    }
    // The descriptor's shared offset is not part of the Wayland keymap
    // contract. Read from byte zero without changing that offset.
    use std::os::unix::fs::FileExt;
    let file = File::from(fd);
    let mut bytes = vec![0; size as usize];
    file.read_exact_at(&mut bytes, 0).ok()?;
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    let keymap_text = String::from_utf8(bytes).ok()?;
    let keymap = xkbcommon::xkb::Keymap::new_from_string(
        context,
        keymap_text,
        xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
        xkbcommon::xkb::COMPILE_NO_FLAGS,
    )?;
    Some(xkbcommon::xkb::State::new(&keymap))
}

/// Pure decision helper (GY-ZCODE-003 contract addendum): given a live
/// `xkb::State`, an evdev keycode and the ctrl modifier, resolve the single
/// Unicode character the key produces, if any.
///
/// This helper reads one already-resolved keysym. The event path wraps it
/// with XKB Compose for locale-defined dead-key sequences and exposes
/// multi-scalar commits as ordered `KeyChar` events.
///
/// Modifier rules (frozen contract):
/// - Ctrl held: emit no `KeyChar`. Ctrl+c/v/q are promoted to
///   `PlatformEvent::Shortcut` upstream (see [`shortcut_from_key`]); all
///   other ctrl combos stay inert.
/// - Plain / shifted characters flow through as `KeyChar` via the xkb map.
fn key_char_from_state(
    xkb_state: &xkbcommon::xkb::State,
    keycode: u32,
    ctrl: bool,
) -> Option<char> {
    if ctrl {
        return None;
    }
    // Wayland key events use evdev keycodes; XKB keycodes include the
    // conventional X11 offset of 8.
    let xkb_keycode = xkbcommon::xkb::Keycode::new(keycode.checked_add(8)?);
    let keysym = xkb_state.key_get_one_sym(xkb_keycode);
    if keysym.is_pf_key() {
        // Function/navigation keysyms (arrows, home, page keys, ...) carry
        // no text; navigation keeps its KeySpecial semantics upstream in
        // the dispatch path.
        return None;
    }
    let c = xkb_state.key_get_utf32(xkb_keycode);
    // key_get_utf32 returns 0 when the keysym has no Unicode mapping.
    let c = char::from_u32(c).filter(|c| !c.is_control())?;
    Some(c)
}

fn key_chars(
    xkb_state: Option<&xkbcommon::xkb::State>,
    keycode: u32,
    shift: bool,
    ctrl: bool,
) -> Vec<char> {
    // Preserve the existing public event contract for these control keys,
    // even when a custom keymap assigns a different UTF-8 representation.
    match keycode {
        14 => return vec!['\u{8}'],
        28 => return vec!['\n'],
        _ => {}
    }
    if let Some(xkb_state) = xkb_state {
        return key_char_from_state(xkb_state, keycode, ctrl)
            .into_iter()
            .collect();
    }
    // NOTE (GY-ZCODE-004): this keymap-free evdev fallback tracks no ctrl
    // modifier, so it emits NO `PlatformEvent::Shortcut` events — the
    // ctrl+c/v/q shortcut contract is served exclusively by the xkb state
    // path (`shortcut_from_key` via `handle_key_press`). This asymmetry is
    // intentional and documented in the frozen contract.
    key_char(keycode, shift)
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\u{8}'))
        .into_iter()
        .collect()
}

fn key_chars_with_compose(
    xkb_state: Option<&xkbcommon::xkb::State>,
    compose_state: Option<&mut xkbcommon::xkb::compose::State>,
    evdev_keycode: u32,
    shift: bool,
    ctrl: bool,
) -> Vec<char> {
    let Some(state) = xkb_state else {
        return key_chars(None, evdev_keycode, shift, ctrl);
    };
    let Some(keycode) = evdev_keycode
        .checked_add(8)
        .map(xkbcommon::xkb::Keycode::new)
    else {
        return Vec::new();
    };
    let keysym = state.key_get_one_sym(keycode);
    let Some(compose) = compose_state else {
        return key_chars(xkb_state, evdev_keycode, shift, ctrl);
    };
    if compose.feed(keysym) == xkbcommon::xkb::compose::FeedResult::Ignored {
        return key_chars(xkb_state, evdev_keycode, shift, ctrl);
    }
    match compose.status() {
        xkbcommon::xkb::compose::Status::Composing => Vec::new(),
        xkbcommon::xkb::compose::Status::Composed => {
            let chars = compose.utf8().unwrap_or_default().chars().collect();
            compose.reset();
            chars
        }
        xkbcommon::xkb::compose::Status::Cancelled => {
            compose.reset();
            key_chars(xkb_state, evdev_keycode, shift, ctrl)
        }
        xkbcommon::xkb::compose::Status::Nothing => {
            key_chars(xkb_state, evdev_keycode, shift, ctrl)
        }
    }
}

/// Pure decision helper (GY-ZCODE-004 contract, extended by GY-ZCODE-006):
/// map ctrl + evdev keycode to an application shortcut. Recognised combos are
/// ctrl+c (46), ctrl+v (47), ctrl+q (16), ctrl+Left (105 →
/// `SessionPrevious`) and ctrl+Right (106 → `SessionNext`); every other
/// combination — including plain (non-ctrl) presses of those keys — yields
/// None and keeps its existing semantics (arrows stay `KeySpecial`).
fn shortcut_from_key(ctrl: bool, keycode: u32) -> Option<Shortcut> {
    if !ctrl {
        return None;
    }
    match keycode {
        46 => Some(Shortcut::Copy),             // KEY_C
        47 => Some(Shortcut::Paste),            // KEY_V
        16 => Some(Shortcut::Quit),             // KEY_Q
        105 => Some(Shortcut::SessionPrevious), // KEY_LEFT
        106 => Some(Shortcut::SessionNext),     // KEY_RIGHT
        _ => None,
    }
}

/// wl_pointer vertical-axis value → [`PlatformEvent::PointerScroll`] line
/// count (GY-ZCODE-003). Wayland reports positive axis value = scroll
/// down/towards the user; our contract is positive = wheel up, so the sign
/// is inverted. One pointer notch maps to one line; a zero value yields None
/// (nothing to emit).
fn scroll_lines(axis_value: f64) -> Option<i32> {
    if axis_value > 0.0 {
        Some(-1)
    } else if axis_value < 0.0 {
        Some(1)
    } else {
        None
    }
}

fn scroll_delta(axis: wl_pointer::Axis, axis_value: f64) -> Option<(f64, f64)> {
    if !axis_value.is_finite() || axis_value == 0.0 {
        return None;
    }
    match axis {
        wl_pointer::Axis::HorizontalScroll => Some((-axis_value, 0.0)),
        wl_pointer::Axis::VerticalScroll => Some((0.0, -axis_value)),
        _ => None,
    }
}

impl Dispatch<WlKeyboard, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // Serial required later by data_device.set_selection (GY-ZCODE-008).
            wl_keyboard::Event::Enter { serial, .. } => {
                state.keyboard_enter_serial = serial;
            }
            wl_keyboard::Event::Keymap { format, fd, size } => {
                if let Some(context) = state.xkb_context.as_ref() {
                    // Keep a previously accepted keymap if the compositor
                    // sends an unsupported or malformed replacement.
                    if let Some(keymap_state) = keymap_state_from_fd(context, format, fd, size) {
                        state.xkb_state = Some(keymap_state);
                    }
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                state.shift = mods_depressed & 0x1 != 0; // ShiftMask
                state.ctrl = mods_depressed & 0x4 != 0; // ControlMask
                if let Some(xkb_state) = state.xkb_state.as_mut() {
                    xkb_state.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                    state.alt = xkb_state.mod_name_is_active(
                        xkbcommon::xkb::MOD_NAME_ALT,
                        xkbcommon::xkb::STATE_MODS_EFFECTIVE,
                    );
                }
            }
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(wl_keyboard::KeyState::Pressed),
                ..
            } => state.handle_key_press(key),
            wl_keyboard::Event::Leave { .. } => {
                if let Some(compose) = state.compose_state.as_mut() {
                    compose.reset();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            } => {
                state.last_pointer = Some((surface_x, surface_y));
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.last_pointer = Some((surface_x, surface_y));
                state
                    .events
                    .push(PlatformEvent::PointerMove(surface_x, surface_y));
            }
            wl_pointer::Event::Button {
                button: 0x110,
                state: WEnum::Value(bs),
                ..
            } => {
                if let Some((x, y)) = state.last_pointer {
                    match bs {
                        wl_pointer::ButtonState::Pressed => {
                            state.events.push(PlatformEvent::PointerPress(x, y));
                        }
                        wl_pointer::ButtonState::Released => {
                            state.events.push(PlatformEvent::PointerRelease(x, y));
                        }
                        _ => {}
                    }
                }
            }
            // Vertical wheel (REL_WHEEL equivalent): only the vertical axis
            // is mapped; horizontal-axis devices emit nothing (GY-ZCODE-003).
            wl_pointer::Event::Axis {
                axis: WEnum::Value(wl_pointer::Axis::VerticalScroll),
                value,
                ..
            } => {
                if let Some(lines) = scroll_lines(value) {
                    state.events.push(PlatformEvent::PointerScroll(lines));
                }
                if let Some((x, y)) = scroll_delta(wl_pointer::Axis::VerticalScroll, value) {
                    state
                        .events
                        .push(PlatformEvent::PointerScrollDelta { x, y });
                }
            }
            wl_pointer::Event::Axis {
                axis: WEnum::Value(wl_pointer::Axis::HorizontalScroll),
                value,
                ..
            } => {
                if let Some((x, y)) = scroll_delta(wl_pointer::Axis::HorizontalScroll, value) {
                    state
                        .events
                        .push(PlatformEvent::PointerScrollDelta { x, y });
                }
            }
            wl_pointer::Event::Leave { .. } => state.last_pointer = None,
            _ => {}
        }
    }
}

impl Dispatch<WlRegion, ()> for GoyaWindowInner {
    fn event(
        _: &mut Self,
        _: &WlRegion,
        _: wayland_client::protocol::wl_region::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod raster_tests {
    use super::*;
    use std::time::Instant;

    fn overlapping_primitives() -> Vec<goya_runtime::Primitive> {
        (0..48)
            .map(|i| {
                let x = (i % 8) * 20;
                let y = (i / 8) * 20;
                let rect = Rect {
                    x: i64::from(x) * MP_PER_PX,
                    y: i64::from(y) * MP_PER_PX,
                    width: 28 * MP_PER_PX,
                    height: 28 * MP_PER_PX,
                };
                if i % 2 == 0 {
                    goya_runtime::Primitive::Button(rect, format!("B{i}"))
                } else {
                    goya_runtime::Primitive::Text(rect, format!("T{i}"))
                }
            })
            .collect()
    }

    #[test]
    fn parallel_tiles_defer_text_scenes_to_serial_raster() {
        let (width, height) = (192, 128);
        let primitives = overlapping_primitives();
        let background = argb([4, 8, 12]);

        let mut serial = vec![background; (width * height) as usize];
        let mut parallel = serial.clone();
        raster_serial(
            &mut serial,
            width,
            height,
            &primitives,
            RasterConfig {
                fills: &[],
                fgs: &[],
                fg: [255, 255, 255],
                glyph_px: 1,
                clips: None,
                text_sizes_pt: None,
                clip: None,
            },
        );
        let roi = PixelClip {
            x0: 0,
            y0: 0,
            x1: width as usize,
            y1: height as usize,
        };
        let dirty = goya_change::PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        };
        assert!(!raster_parallel_tiles(
            &mut parallel,
            width,
            height,
            &primitives,
            &[],
            &[],
            [255, 255, 255],
            1,
            roi,
            dirty,
            CsrPolicy::QuadCorner,
            4,
        ));
        assert_eq!(parallel, vec![background; (width * height) as usize]);
        // The public raster dispatcher falls back to serial for text scenes.
        raster_primitives(
            &mut parallel,
            width,
            height,
            &primitives,
            &[],
            &[],
            [255; 3],
            1,
            None,
            None,
            None,
            true,
        );
        assert_eq!(parallel, serial);
    }

    #[test]
    fn automatic_policy_keeps_small_workloads_serial() {
        let policy = goya_csr::select_policy(32 * 32 * 4, RASTER_TILE_SIZE, 4);
        assert_eq!(policy, CsrPolicy::Serial);
    }

    #[test]
    #[ignore = "manual timing evidence; run with --ignored --nocapture"]
    fn benchmark_serial_and_parallel_tile_raster() {
        let (width, height) = (768, 512);
        let primitives = (0..240)
            .map(|i| {
                let rect = Rect {
                    x: i64::from((i * 23) % 700) * MP_PER_PX,
                    y: i64::from((i * 47) % 460) * MP_PER_PX,
                    width: 96 * MP_PER_PX,
                    height: 48 * MP_PER_PX,
                };
                goya_runtime::Primitive::Text(rect, format!("tile workload {i}"))
            })
            .collect::<Vec<_>>();
        let roi = PixelClip {
            x0: 0,
            y0: 0,
            x1: width as usize,
            y1: height as usize,
        };
        let dirty = goya_change::PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let mut serial = vec![argb([4, 8, 12]); (width * height) as usize];
        let mut parallel = serial.clone();
        let start = Instant::now();
        for _ in 0..5 {
            serial.fill(argb([4, 8, 12]));
            raster_serial(
                &mut serial,
                width,
                height,
                &primitives,
                RasterConfig {
                    fills: &[],
                    fgs: &[],
                    fg: [255; 3],
                    glyph_px: 2,
                    clips: None,
                    text_sizes_pt: None,
                    clip: None,
                },
            );
        }
        let serial_elapsed = start.elapsed();
        let start = Instant::now();
        for _ in 0..5 {
            parallel.fill(argb([4, 8, 12]));
            assert!(raster_parallel_tiles(
                &mut parallel,
                width,
                height,
                &primitives,
                &[],
                &[],
                [255; 3],
                2,
                roi,
                dirty,
                CsrPolicy::QuadCorner,
                4,
            ));
        }
        let parallel_elapsed = start.elapsed();
        assert_eq!(parallel, serial);
        eprintln!(
            "serial_5_ns={} parallel_5_ns={} speedup={:.3}",
            serial_elapsed.as_nanos(),
            parallel_elapsed.as_nanos(),
            serial_elapsed.as_secs_f64() / parallel_elapsed.as_secs_f64()
        );
    }
}

#[cfg(test)]
mod key_tests {
    use super::{key_char, key_char_from_state, key_chars, key_chars_with_compose};
    #[test]
    fn us_layout_spot_checks() {
        assert_eq!(key_char(30, false), Some('a'));
        assert_eq!(key_char(31, false), Some('s'));
        assert_eq!(key_char(36, false), Some('j'));
        assert_eq!(key_char(30, true), Some('A'));
        assert_eq!(key_char(16, false), Some('q'));
        assert_eq!(key_char(17, false), Some('w'));
        assert_eq!(key_char(18, false), Some('e'));
        assert_eq!(key_char(44, false), Some('z'));
        assert_eq!(key_char(50, false), Some('m'));
        assert_eq!(key_char(39, false), Some(';'));
        assert_eq!(key_char(39, true), Some(':'));
        assert_eq!(key_char(57, false), Some(' '));
        assert_eq!(key_char(11, true), Some(')'));
        assert_eq!(key_char(42, false), None); // shift itself
        assert_eq!(key_char(999, false), None);
    }

    #[test]
    fn enter_and_backspace_stay_key_chars() {
        // GY-WAVE2 contract: Enter stays '\n', Backspace stays '\u{8}'.
        assert_eq!(key_char(28, false), Some('\n'));
        assert_eq!(key_char(14, false), Some('\u{8}'));
        // Tab moved to KeySpecial, so it must NOT emit a char.
        assert_eq!(key_char(15, false), None);
    }

    #[test]
    fn built_in_fallback_is_bounded_and_preserves_control_keys() {
        assert_eq!(key_chars(None, 2, false, false), vec!['1']);
        assert_eq!(key_chars(None, 2, true, false), vec!['!']);
        assert_eq!(key_chars(None, 28, false, false), vec!['\n']);
        assert_eq!(key_chars(None, 14, false, false), vec!['\u{8}']);
        assert!(key_chars(None, 999, false, false).is_empty());
    }

    fn state_for_layout(layout: &str) -> (xkbcommon::xkb::Keymap, xkbcommon::xkb::State) {
        use xkbcommon::xkb::{self, Context, Keymap};
        let context = Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = Keymap::new_from_names(
            &context,
            "",
            "",
            layout,
            "",
            Some(String::new()),
            xkb::COMPILE_NO_FLAGS,
        )
        .unwrap_or_else(|| panic!("installed XKB rules did not provide {layout:?}"));
        let state = xkb::State::new(&keymap);
        (keymap, state)
    }

    #[test]
    fn xkb_state_honors_shift_and_caps_lock() {
        use xkbcommon::xkb;

        let (keymap, mut state) = state_for_layout("us");
        let q = 16;
        assert_eq!(key_chars(Some(&state), q, false, false), vec!['q']);

        let shift = keymap.mod_get_index(&"Shift");
        assert_ne!(shift, xkb::MOD_INVALID);
        state.update_mask(1 << shift, 0, 0, 0, 0, 0);
        assert_eq!(key_chars(Some(&state), q, false, false), vec!['Q']);

        let lock = keymap.mod_get_index(&"Lock");
        assert_ne!(lock, xkb::MOD_INVALID);
        state.update_mask(0, 0, 1 << lock, 0, 0, 0);
        assert_eq!(key_chars(Some(&state), q, false, false), vec!['Q']);
    }

    #[test]
    fn xkb_state_honors_german_altgr_mapping() {
        use xkbcommon::xkb;

        let (keymap, mut state) = state_for_layout("de");
        let mod5 = keymap.mod_get_index(&"Mod5");
        assert_ne!(mod5, xkb::MOD_INVALID);
        state.update_mask(1 << mod5, 0, 0, 0, 0, 0);
        // German AltGr+Q emits @; evdev KEY_Q is 16.
        assert_eq!(key_chars(Some(&state), 16, false, false), vec!['@']);
    }

    #[test]
    fn locale_compose_completes_dead_acute_sequence() {
        use std::ffi::OsStr;
        use xkbcommon::xkb::{self, Context, Keymap};

        let context = Context::new(xkb::CONTEXT_NO_FLAGS);
        let Some(keymap) = Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "intl",
            Some(String::new()),
            xkb::COMPILE_NO_FLAGS,
        ) else {
            return;
        };
        let state = xkb::State::new(&keymap);
        let Ok(table) = xkb::compose::Table::new_from_locale(
            &context,
            OsStr::new("C.UTF-8"),
            xkb::compose::COMPILE_NO_FLAGS,
        ) else {
            return;
        };
        let mut compose = xkb::compose::State::new(&table, xkb::compose::STATE_NO_FLAGS);
        assert!(
            key_chars_with_compose(Some(&state), Some(&mut compose), 40, false, false).is_empty()
        );
        assert_eq!(
            key_chars_with_compose(Some(&state), Some(&mut compose), 18, false, false),
            vec!['é']
        );
    }

    #[test]
    fn compose_without_keymap_preserves_keyboard_fallback() {
        assert_eq!(
            key_chars_with_compose(None, None, 16, false, false),
            vec!['q']
        );
        assert_eq!(
            key_chars_with_compose(None, None, 28, false, false),
            vec!['\n']
        );
    }

    #[test]
    fn compositor_keymap_fd_is_parsed_and_oversized_maps_are_rejected() {
        use super::{keymap_state_from_fd, wl_keyboard, WEnum, MAX_XKB_KEYMAP_BYTES};
        use std::fs::File;
        use std::io::Write;
        use xkbcommon::xkb::{self, Context, Keymap};

        let context = Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            Some(String::new()),
            xkb::COMPILE_NO_FLAGS,
        )
        .unwrap();
        let keymap_text = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
        let mut file = File::from(
            rustix::fs::memfd_create("goya-xkb-test", rustix::fs::MemfdFlags::CLOEXEC).unwrap(),
        );
        file.write_all(keymap_text.as_bytes()).unwrap();
        // Leave the shared offset at EOF, as a compositor is allowed to do.
        let state = keymap_state_from_fd(
            &context,
            WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
            file.into(),
            keymap_text.len() as u32,
        )
        .unwrap();
        assert_eq!(key_chars(Some(&state), 16, false, false), vec!['q']);

        let oversized_fd =
            rustix::fs::memfd_create("goya-xkb-oversized-test", rustix::fs::MemfdFlags::CLOEXEC)
                .unwrap();
        assert!(keymap_state_from_fd(
            &context,
            WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
            oversized_fd,
            MAX_XKB_KEYMAP_BYTES + 1,
        )
        .is_none());
    }

    #[test]
    fn xkb_path_emits_non_ascii_chars() {
        use super::key_char_from_state;
        use xkbcommon::xkb;

        // German layout: the key next to Enter (evdev 12, minus/ß).
        let (_, state) = state_for_layout("de");
        assert_eq!(key_char_from_state(&state, 12, false), Some('ß'));
        assert_eq!(key_chars(Some(&state), 12, false, false), vec!['ß']);

        // Ctrl held: combos are inert for now (frozen contract).
        assert_eq!(key_char_from_state(&state, 12, true), None);
        let (keymap, mut state) = state_for_layout("us");
        let ctrl = keymap.mod_get_index(&"Control");
        assert_ne!(ctrl, xkb::MOD_INVALID);
        state.update_mask(1 << ctrl, 0, 0, 0, 0, 0);
        assert_eq!(key_char_from_state(&state, 30, true), None); // KEY_C + ctrl
    }

    #[test]
    fn xkb_and_evdev_paths_agree_on_overlapping_keycodes() {
        // 'us' layout vs the built-in evdev US-subset table must produce
        // identical KeyChar results for every printable keycode both know.
        let printable = [
            2, 3, 11, 12, 13, 16, 17, 18, 19, 20, 26, 30, 31, 32, 33, 44, 45, 50, 51, 52, 53, 57,
        ];
        for kc in printable {
            for shift in [false, true] {
                // Construct a fresh state per case so modifier state from a
                // previous key cannot leak into this comparison.
                let (keymap, mut state) = state_for_layout("us");
                if shift {
                    let shift_index = keymap.mod_get_index(&"Shift");
                    state.update_mask(1 << shift_index, 0, 0, 0, 0, 0);
                }
                assert_eq!(
                    key_chars(Some(&state), kc, shift, false),
                    key_chars(None, kc, shift, false),
                    "path mismatch at keycode {kc} shift={shift}"
                );
            }
        }
        // Navigation keys: xkb path must not steal them into KeyChar —
        // key_special owns them upstream, and the xkb path yields no char.
        let (_, state) = state_for_layout("us");
        for kc in [1, 15, 102, 103, 104, 105, 106, 107, 108, 109, 111] {
            assert!(
                key_char_from_state(&state, kc, false).is_none(),
                "xkb path leaked a char for navigation keycode {kc}"
            );
        }
    }
}

#[cfg(test)]
mod key_special_tests {
    use super::{key_special, KeySpecial};
    use wayland_client::protocol::wl_pointer;

    #[test]
    fn evdev_us_subset_mapping_table() {
        assert_eq!(key_special(105), Some(KeySpecial::ArrowLeft));
        assert_eq!(key_special(106), Some(KeySpecial::ArrowRight));
        assert_eq!(key_special(103), Some(KeySpecial::ArrowUp));
        assert_eq!(key_special(108), Some(KeySpecial::ArrowDown));
        assert_eq!(key_special(102), Some(KeySpecial::Home));
        assert_eq!(key_special(107), Some(KeySpecial::End));
        assert_eq!(key_special(111), Some(KeySpecial::Delete));
        // GY-ZCODE-003: PageUp/PageDown.
        assert_eq!(key_special(104), Some(KeySpecial::PageUp));
        assert_eq!(key_special(109), Some(KeySpecial::PageDown));
        assert_eq!(key_special(1), Some(KeySpecial::Escape));
        assert_eq!(key_special(15), Some(KeySpecial::Tab));
    }

    #[test]
    fn unmapped_keycodes_stay_none() {
        // Letters, digits, shift, enter, backspace, space, unknown codes.
        for kc in [16, 30, 44, 2, 11, 42, 54, 28, 14, 57, 999, 0] {
            assert_eq!(key_special(kc), None, "keycode {kc} should be unmapped");
        }
    }

    #[test]
    fn scroll_lines_sign_is_inverted() {
        use super::scroll_lines;
        // Wayland positive axis value = scroll down; contract positive = up.
        assert_eq!(scroll_lines(10.0), Some(-1));
        assert_eq!(scroll_lines(-10.0), Some(1));
        // Zero value emits nothing.
        assert_eq!(scroll_lines(0.0), None);
    }

    #[test]
    fn smooth_scroll_preserves_fractional_and_horizontal_deltas() {
        use super::scroll_delta;
        assert_eq!(
            scroll_delta(wl_pointer::Axis::VerticalScroll, 0.25),
            Some((0.0, -0.25))
        );
        assert_eq!(
            scroll_delta(wl_pointer::Axis::HorizontalScroll, -1.5),
            Some((1.5, 0.0))
        );
        assert_eq!(scroll_delta(wl_pointer::Axis::VerticalScroll, 0.0), None);
        assert_eq!(
            scroll_delta(wl_pointer::Axis::VerticalScroll, f64::NAN),
            None
        );
        assert_eq!(
            scroll_delta(wl_pointer::Axis::VerticalScroll, f64::INFINITY),
            None
        );
    }
}

#[cfg(test)]
mod shortcut_tests {
    use super::{shortcut_from_key, GoyaWindowInner, KeyModifiers, PlatformEvent, Shortcut};
    use xkbcommon::xkb::{self, Context, Keymap};

    #[test]
    fn helper_mapping_table() {
        // GY-ZCODE-004: ctrl+c/v/q via evdev keycodes.
        assert_eq!(shortcut_from_key(true, 46), Some(Shortcut::Copy)); // KEY_C
        assert_eq!(shortcut_from_key(true, 47), Some(Shortcut::Paste)); // KEY_V
        assert_eq!(shortcut_from_key(true, 16), Some(Shortcut::Quit)); // KEY_Q
                                                                       // GY-ZCODE-006: ctrl+Left/Right cycle sessions.
        assert_eq!(
            shortcut_from_key(true, 105),
            Some(Shortcut::SessionPrevious)
        ); // KEY_LEFT
        assert_eq!(shortcut_from_key(true, 106), Some(Shortcut::SessionNext)); // KEY_RIGHT
                                                                               // Non-ctrl presses of the same keys stay plain text / KeySpecial,
                                                                               // not shortcuts.
        assert_eq!(shortcut_from_key(false, 46), None);
        assert_eq!(shortcut_from_key(false, 47), None);
        assert_eq!(shortcut_from_key(false, 16), None);
        assert_eq!(shortcut_from_key(false, 105), None);
        assert_eq!(shortcut_from_key(false, 106), None);
        assert_eq!(shortcut_from_key(false, 999), None);
    }

    #[test]
    fn other_ctrl_combos_stay_inert() {
        // Ctrl held, but not c/v/q or Left/Right: helper must yield None for
        // letters, digits, navigation codes and unknown codes alike.
        for kc in [16 - 1, 30, 31, 32, 44, 45, 48, 2, 11, 57, 1, 999, 0] {
            assert_eq!(
                shortcut_from_key(true, kc),
                None,
                "ctrl+keycode {kc} must stay inert"
            );
        }
    }

    /// Full event-path test without a compositor: build a `us` keymap via
    /// `Keymap::new_from_names`, install it in a `GoyaWindowInner`, raise the
    /// ctrl modifier exactly as the `wl_keyboard.modifiers` dispatch does,
    /// then run the extracted press handler.
    fn inner_with_ctrl(ctrl: bool) -> GoyaWindowInner {
        let mut inner = GoyaWindowInner::new(80, 24);
        let context = Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            Some(String::new()),
            xkb::COMPILE_NO_FLAGS,
        )
        .unwrap_or_else(|| panic!("installed XKB rules did not provide 'us'"));
        let state = xkb::State::new(&keymap);
        inner.xkb_state = Some(state);
        let ctrl_index = keymap.mod_get_index(&"Control");
        assert_ne!(ctrl_index, xkb::MOD_INVALID);
        inner.ctrl = ctrl;
        if ctrl {
            inner
                .xkb_state
                .as_mut()
                .unwrap()
                .update_mask(1 << ctrl_index, 0, 0, 0, 0, 0);
        }
        inner
    }

    #[test]
    fn xkb_path_emits_shortcut_paste_for_ctrl_v() {
        let mut inner = inner_with_ctrl(true);
        inner.handle_key_press(47); // KEY_V
        assert_eq!(inner.events, vec![PlatformEvent::Shortcut(Shortcut::Paste)]);
    }

    #[test]
    fn navigation_events_preserve_shift_and_control_modifiers() {
        let mut inner = inner_with_ctrl(false);
        inner.shift = true;
        inner.handle_key_press(105); // Left
        assert_eq!(
            inner.events,
            vec![PlatformEvent::KeySpecialWithModifiers {
                key: super::KeySpecial::ArrowLeft,
                modifiers: KeyModifiers {
                    shift: true,
                    ctrl: false,
                },
            }]
        );

        // GY-ZCODE-006 regression: plain (non-ctrl) arrows keep their
        // KeySpecial semantics — no shortcuts without the ctrl modifier.
        inner.events.clear();
        inner.shift = false;
        inner.handle_key_press(106); // Right
        assert_eq!(
            inner.events,
            vec![PlatformEvent::KeySpecial(super::KeySpecial::ArrowRight)]
        );
        inner.events.clear();
        inner.handle_key_press(105);
        assert_eq!(
            inner.events,
            vec![PlatformEvent::KeySpecial(super::KeySpecial::ArrowLeft)]
        );
    }

    #[test]
    fn xkb_path_emits_session_shortcuts_for_ctrl_arrows() {
        // GY-ZCODE-006: with ctrl held, Left/Right become session-switch
        // shortcuts instead of KeySpecial navigation events.
        let mut inner = inner_with_ctrl(true);
        inner.handle_key_press(105); // KEY_LEFT
        assert_eq!(
            inner.events,
            vec![PlatformEvent::Shortcut(Shortcut::SessionPrevious)]
        );
        inner.events.clear();
        inner.handle_key_press(106); // KEY_RIGHT
        assert_eq!(
            inner.events,
            vec![PlatformEvent::Shortcut(Shortcut::SessionNext)]
        );
    }

    #[test]
    fn xkb_path_emits_shortcut_copy_and_quit() {
        let mut inner = inner_with_ctrl(true);
        inner.handle_key_press(46); // KEY_C
        assert_eq!(inner.events, vec![PlatformEvent::Shortcut(Shortcut::Copy)]);
        inner.events.clear();
        inner.handle_key_press(16); // KEY_Q
        assert_eq!(inner.events, vec![PlatformEvent::Shortcut(Shortcut::Quit)]);
    }

    #[test]
    fn non_ctrl_behavior_is_unchanged_and_other_ctrl_combos_emit_nothing() {
        // Plain 'c'/'v'/'q' still flow through as KeyChar (wave-3 contract).
        let mut inner = inner_with_ctrl(false);
        for (kc, ch) in [(46, 'c'), (47, 'v'), (16, 'q')] {
            inner.handle_key_press(kc);
            assert_eq!(inner.events, vec![PlatformEvent::KeyChar(ch)]);
            inner.events.clear();
        }
        // Ctrl+a (and any other ctrl combo) emits nothing at all.
        let mut inner = inner_with_ctrl(true);
        inner.handle_key_press(30); // KEY_A
        assert!(inner.events.is_empty());
    }
}

#[cfg(test)]
mod text_input_tests {
    use super::zwp_text_input_v3;
    use super::{stage_text_input_event, text_input_activation, PlatformEvent, TextInputState};

    #[test]
    fn preedit_stays_staged_until_done_then_applies_once() {
        let mut s = TextInputState::new();
        s.feed_preedit(Some("ni".into()));
        assert!(
            s.drain().is_empty(),
            "no protocol done yet: nothing may be applied"
        );
        s.feed_preedit(Some("ni hao".into()));
        s.feed_done();
        assert_eq!(
            s.drain(),
            vec![PlatformEvent::Preedit(Some("ni hao".into()))]
        );
        // Draining again emits nothing: the frame was already applied.
        assert!(s.drain().is_empty());
        assert_eq!(s.applied_preedit.as_deref(), Some("ni hao"));
    }

    #[test]
    fn preedit_none_clears_and_is_visible_after_done() {
        let mut s = TextInputState::new();
        s.feed_preedit(Some("abc".into()));
        s.feed_done();
        let _ = s.drain();
        s.feed_preedit(None);
        assert!(s.drain().is_empty(), "clear also waits for done");
        s.feed_done();
        assert_eq!(s.drain(), vec![PlatformEvent::Preedit(None)]);
        assert_eq!(s.applied_preedit, None);
    }

    #[test]
    fn commit_text_arrives_whole_and_before_preedit() {
        let mut s = TextInputState::new();
        s.feed_preedit(Some("pin".into()));
        s.feed_commit(Some("拼".into()));
        s.feed_done();
        assert_eq!(
            s.drain(),
            vec![
                PlatformEvent::CommitText("拼".into()),
                PlatformEvent::Preedit(Some("pin".into()))
            ]
        );
    }

    #[test]
    fn multiple_commits_keep_arrival_order() {
        let mut s = TextInputState::new();
        s.feed_commit(Some("a".into()));
        s.feed_commit(Some("b".into()));
        s.feed_commit(Some("c".into()));
        s.feed_done();
        assert_eq!(
            s.drain(),
            vec![
                PlatformEvent::CommitText("a".into()),
                PlatformEvent::CommitText("b".into()),
                PlatformEvent::CommitText("c".into())
            ]
        );
    }

    #[test]
    fn staged_frame_applies_in_protocol_order_after_done() {
        let mut s = TextInputState::new();
        s.feed_preedit(Some("compose".into()));
        s.feed_done();
        assert_eq!(
            s.drain(),
            vec![PlatformEvent::Preedit(Some("compose".into()))]
        );

        s.feed_commit(Some("語".into()));
        s.feed_preedit(Some("かな".into()));
        assert!(s.drain().is_empty(), "the batch waits for done");
        s.feed_done();
        assert_eq!(
            s.drain(),
            vec![
                PlatformEvent::Preedit(None),
                PlatformEvent::CommitText("語".into()),
                PlatformEvent::Preedit(Some("かな".into())),
            ]
        );
        assert_eq!(s.applied_preedit.as_deref(), Some("かな"));
    }

    #[test]
    fn done_without_staged_data_is_a_noop_and_empty_commit_stages_nothing() {
        let mut s = TextInputState::new();
        s.feed_done();
        assert!(s.drain().is_empty());
        // Protocol uses empty commit_string purely as frame confirmation.
        s.feed_commit(Some(String::new()));
        s.feed_done();
        assert!(s.drain().is_empty());
    }

    #[test]
    fn leave_discards_staged_frame_and_clears_applied_preedit() {
        let mut s = TextInputState::new();
        s.feed_preedit(Some("draft".into()));
        s.feed_done();
        let _ = s.drain();
        // A staged frame lost by leave must never surface.
        s.feed_commit(Some("x".into()));
        s.feed_leave();
        assert_eq!(s.drain(), vec![PlatformEvent::Preedit(None)]);
        // Leave without applied preedit emits nothing.
        s.feed_leave();
        assert!(s.drain().is_empty());
    }

    #[test]
    fn fresh_state_drains_nothing_without_compositor_traffic() {
        let mut s = TextInputState::new();
        assert!(s.drain().is_empty());
        assert_eq!(s.pending_preedit, None);
        assert!(s.committed.is_empty());
    }

    #[test]
    fn activation_decision_noops_when_global_missing_or_state_holds() {
        // The registry never bound a text-input global (Option::None): the
        // activation API must be a safe no-op, not a panic or protocol error.
        assert_eq!(text_input_activation::<()>(None, false, true), None);
        assert_eq!(text_input_activation::<()>(None, false, false), None);
        // Bound but already in the requested state: no protocol traffic.
        // (Generic helper: `()` stands in for the proxy in unit tests.)
        assert_eq!(text_input_activation(Some(()), true, true), None);
        assert_eq!(text_input_activation(Some(()), false, false), None);
        // Bound and state changes: the decision carries the new state.
        assert_eq!(
            text_input_activation(Some(()), false, true),
            Some(((), true))
        );
        assert_eq!(
            text_input_activation(Some(()), true, false),
            Some(((), false))
        );
    }

    #[test]
    fn drain_only_emits_after_done_even_across_interleaved_feeds() {
        let mut s = TextInputState::new();
        s.feed_commit(Some("one".into()));
        s.feed_preedit(Some("p".into()));
        assert!(s.drain().is_empty());
        s.feed_commit(Some("two".into()));
        s.feed_done();
        let events = s.drain();
        assert_eq!(
            events,
            vec![
                PlatformEvent::CommitText("one".into()),
                PlatformEvent::CommitText("two".into()),
                PlatformEvent::Preedit(Some("p".into()))
            ]
        );
    }

    #[test]
    fn native_handler_preserves_preedit_range_and_protocol_done_order() {
        let mut state = TextInputState::new();
        stage_text_input_event(
            &mut state,
            zwp_text_input_v3::Event::PreeditString {
                text: Some("old".into()),
                cursor_begin: 1,
                cursor_end: 2,
            },
        );
        stage_text_input_event(&mut state, zwp_text_input_v3::Event::Done { serial: 1 });
        assert_eq!(
            state.drain(),
            vec![PlatformEvent::PreeditWithCursor {
                text: "old".into(),
                cursor_begin: 1,
                cursor_end: 2,
            }]
        );
        assert_eq!(state.applied_preedit_cursor, Some((1, 2)));

        // Intentionally feed callbacks in a different arrival order: done
        // applies the protocol-defined sequence, not callback order.
        stage_text_input_event(
            &mut state,
            zwp_text_input_v3::Event::PreeditString {
                text: Some("かな".into()),
                cursor_begin: 3,
                cursor_end: 6,
            },
        );
        stage_text_input_event(
            &mut state,
            zwp_text_input_v3::Event::CommitString {
                text: Some("字".into()),
            },
        );
        stage_text_input_event(
            &mut state,
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length: 2,
                after_length: 1,
            },
        );
        assert!(state.drain().is_empty(), "native updates wait for done");
        stage_text_input_event(&mut state, zwp_text_input_v3::Event::Done { serial: 2 });
        assert_eq!(
            state.drain(),
            vec![
                PlatformEvent::Preedit(None),
                PlatformEvent::DeleteSurroundingText {
                    before_bytes: 2,
                    after_bytes: 1,
                },
                PlatformEvent::CommitText("字".into()),
                PlatformEvent::PreeditWithCursor {
                    text: "かな".into(),
                    cursor_begin: 3,
                    cursor_end: 6,
                },
            ]
        );
        assert_eq!(state.applied_preedit.as_deref(), Some("かな"));
        assert_eq!(state.applied_preedit_cursor, Some((3, 6)));
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::{pick_text_mime, ClipboardRequestPhase, ClipboardState, PlatformEvent};

    fn mimes(list: &[&str]) -> Vec<String> {
        list.iter().map(|m| (*m).to_string()).collect()
    }

    #[test]
    fn utf8_tagged_mime_wins_over_bare_text_plain() {
        assert_eq!(
            pick_text_mime(&mimes(&[
                "text/plain",
                "UTF8_STRING",
                "text/plain;charset=utf-8"
            ])),
            Some("text/plain;charset=utf-8".to_string())
        );
        // Bare text/plain alone is still usable.
        assert_eq!(
            pick_text_mime(&mimes(&["image/png", "text/plain"])),
            Some("text/plain".to_string())
        );
    }

    #[test]
    fn no_text_mime_or_empty_offer_negotiates_none() {
        assert_eq!(pick_text_mime(&mimes(&["image/png", "UTF8_STRING"])), None);
        assert_eq!(pick_text_mime(&[]), None);
    }

    #[test]
    fn second_request_while_pending_is_rejected_then_admitted_after_answer() {
        let mut s = ClipboardState::new();
        assert_eq!(s.phase, ClipboardRequestPhase::Idle);
        assert!(s.begin_request(), "first request is admitted");
        assert!(!s.begin_request(), "pending request rejects duplicates");
        // Completing surfaces exactly one ClipboardText event.
        assert_eq!(
            s.complete_request(Some("hi".into())),
            Some(PlatformEvent::ClipboardText(Some("hi".into())))
        );
        assert_eq!(s.phase, ClipboardRequestPhase::Answered);
        // Answered (not pending): a new request is admitted again.
        assert!(s.begin_request());
    }

    #[test]
    fn completing_or_cancelling_without_outstanding_request_yields_nothing() {
        let mut s = ClipboardState::new();
        assert_eq!(s.complete_request(Some("x".into())), None);
        assert_eq!(s.cancel_request(), None);
        s.begin_request();
        s.complete_request(None);
        // Double completion stays inert.
        assert_eq!(s.complete_request(Some("y".into())), None);
        assert_eq!(s.cancel_request(), None);
    }

    #[test]
    fn selection_change_resolves_pending_read_as_no_text() {
        let mut s = ClipboardState::new();
        s.begin_request();
        assert_eq!(s.cancel_request(), Some(PlatformEvent::ClipboardText(None)));
    }

    #[test]
    fn receive_target_needs_a_staged_text_mime() {
        let mut s = ClipboardState::new();
        assert_eq!(s.receive_target(), None);
        s.stage_selection_mimes(mimes(&["text/plain;charset=utf-8"]));
        assert_eq!(
            s.receive_target(),
            Some("text/plain;charset=utf-8".to_string())
        );
    }

    #[test]
    fn serve_chunk_walks_payload_and_never_splits_scalars() {
        let mut s = ClipboardState::new();
        // No payload: nothing to serve.
        assert_eq!(s.serve_chunk(8), None);
        s.set_outgoing("拼拼a");
        let mut served = Vec::new();
        while let Some(chunk) = s.serve_chunk(4) {
            served.push(String::from_utf8(chunk.to_vec()).unwrap());
        }
        // 9 bytes / 4-byte budget: chunks must stay valid UTF-8 scalars.
        assert_eq!(served, vec!["拼", "拼a"]);
        // Exhausted payload serves nothing more.
        assert_eq!(s.serve_chunk(8), None);
    }

    #[test]
    fn fresh_payload_resets_the_serving_cursor() {
        let mut s = ClipboardState::new();
        s.set_outgoing("abcdef");
        assert_eq!(s.serve_chunk(3), Some(b"abc".as_slice()));
        s.set_outgoing("xy");
        assert_eq!(s.serve_chunk(8), Some(b"xy".as_slice()));
    }

    #[test]
    fn take_outgoing_drops_payload_and_resets_cursor() {
        let mut s = ClipboardState::new();
        s.set_outgoing("keep");
        assert_eq!(s.take_outgoing().as_deref(), Some("keep"));
        assert_eq!(s.take_outgoing(), None);
        assert_eq!(s.serve_chunk(4), None);
    }

    #[test]
    fn zero_or_max_budget_is_safe() {
        let mut s = ClipboardState::new();
        s.set_outgoing("ab");
        // Degenerate budget still makes progress (never an infinite loop).
        assert_eq!(s.serve_chunk(0), Some(b"a".as_slice()));
        assert_eq!(s.serve_chunk(0), Some(b"b".as_slice()));
        assert_eq!(s.serve_chunk(0), None);
    }
}

#[cfg(test)]
mod fill_tests {
    use super::{argb, raster_serial, resolve_fill, RasterConfig, MP_PER_PX};
    use goya_layout::Rect;

    #[test]
    fn resolve_fill_overrides_and_falls_back() {
        let fills = [Some(0xFF4444), None];
        assert_eq!(resolve_fill(&fills, 0, [16, 16, 20]), [0xFF, 0x44, 0x44]);
        assert_eq!(resolve_fill(&fills, 1, [16, 16, 20]), [16, 16, 20]);
        // Short slice / empty slice fall back (caller may pass &[]).
        assert_eq!(resolve_fill(&[], 0, [80, 100, 180]), [80, 100, 180]);
        assert_eq!(resolve_fill(&fills, 5, [7, 8, 9]), [7, 8, 9]);
    }

    #[test]
    fn fill_colour_reaches_rasterized_buffer() {
        let (width, height) = (64, 32);
        let rect = Rect {
            x: 0,
            y: 0,
            width: 20 * MP_PER_PX,
            height: 20 * MP_PER_PX,
        };
        let primitives = [goya_runtime::Primitive::Button(rect, "C".to_string())];
        // The single 'C' glyph spans x=2..6; sample plain fill at (15, 15).
        let probe = 15 + 15 * width as usize;
        let default = argb([80, 100, 180]);
        let overridden = argb([0x1A, 0x2A, 0x3B]);

        let mut buf = vec![0u32; (width * height) as usize];
        raster_serial(
            &mut buf,
            width,
            height,
            &primitives,
            RasterConfig {
                fills: &[],
                fgs: &[],
                fg: [255; 3],
                glyph_px: 1,
                clips: None,
                text_sizes_pt: None,
                clip: None,
            },
        );
        assert_eq!(buf[probe], default, "no fill entry: flat default expected");

        raster_serial(
            &mut buf,
            width,
            height,
            &primitives,
            RasterConfig {
                fills: &[Some(0x1A2A3B)],
                fgs: &[],
                fg: [255; 3],
                glyph_px: 1,
                clips: None,
                text_sizes_pt: None,
                clip: None,
            },
        );
        assert_eq!(buf[probe], overridden, "fills[0] must override the fill");

        // Index is by primitive order: fill on the wrong index does nothing.
        raster_serial(
            &mut buf,
            width,
            height,
            &primitives,
            RasterConfig {
                fills: &[None, Some(0x1A2A3B)],
                fgs: &[],
                fg: [255; 3],
                glyph_px: 1,
                clips: None,
                text_sizes_pt: None,
                clip: None,
            },
        );
        assert_eq!(buf[probe], default);
    }
}

#[cfg(test)]
mod fgs_tests {
    //! GY-ZCODE-009: per-primitive text (fg) colour overrides, mirroring the
    //! wave-2 `fill_tests` structure exactly.
    use super::{
        argb, raster_parallel_tiles, raster_primitives, raster_serial, resolve_color, CsrPolicy,
        PixelClip, RasterConfig, MP_PER_PX,
    };
    use goya_layout::Rect;

    #[test]
    fn resolve_color_overrides_falls_back_and_ignores_wrong_index() {
        let fgs = [Some(0x22AA88), None];
        assert_eq!(resolve_color(&fgs, 0, [255, 255, 255]), [0x22, 0xAA, 0x88]);
        assert_eq!(resolve_color(&fgs, 1, [255, 255, 255]), [255, 255, 255]);
        // Short slice / empty slice fall back (caller may pass &[]).
        assert_eq!(resolve_color(&[], 0, [10, 20, 30]), [10, 20, 30]);
        assert_eq!(resolve_color(&fgs, 5, [7, 8, 9]), [7, 8, 9]);
    }

    fn text_scene() -> (i32, i32, Vec<goya_runtime::Primitive>, usize) {
        let (width, height) = (64, 32);
        let rect = Rect {
            x: 0,
            y: 0,
            width: 40 * MP_PER_PX,
            height: 20 * MP_PER_PX,
        };
        let primitives = vec![goya_runtime::Primitive::Text(rect, "xyz".to_string())];
        (width, height, primitives, (width * height) as usize)
    }

    fn render_with_fgs(fgs: &[Option<u32>]) -> Vec<u32> {
        let (width, height, primitives, len) = text_scene();
        let mut buf = vec![0u32; len];
        raster_serial(
            &mut buf,
            width,
            height,
            &primitives,
            RasterConfig {
                fills: &[],
                fgs,
                fg: [255; 3],
                glyph_px: 1,
                clips: None,
                text_sizes_pt: None,
                clip: None,
            },
        );
        buf
    }

    #[test]
    fn text_glyph_colour_reaches_rasterized_buffer() {
        // Baseline: no fgs entry — glyphs drawn in the flat fg.
        let flat = render_with_fgs(&[]);
        let overridden_flat = render_with_fgs(&[Some(0x123456)]);
        let overridden_blue = render_with_fgs(&[Some(0x0000FF)]);
        let none_entry = render_with_fgs(&[None]);
        // Short slice falls back: identical to the flat-fg render.
        let short = render_with_fgs(&[None, Some(0x123456)]);
        assert_eq!(flat, none_entry, "fgs[0]=None must keep the flat fg");
        assert_eq!(flat, short, "short fgs slice must keep the flat fg");
        // Overridden buffers must actually differ from the flat render
        // (glyph pixels exist, so colour change is observable).
        assert_ne!(flat, overridden_blue, "fgs[0] must change glyph pixels");
        // Overriding with the flat colour itself is pixel-identical: proves
        // fgs replaces (not blends with) the glyph colour.
        assert_eq!(
            overridden_blue,
            overridden_flat_blue_snapshot(),
            "override must be a full replacement of the glyph colour"
        );
        // Wrong index: the override sits at index 1 but the scene has one
        // primitive, so nothing changes.
        let wrong_index = render_with_fgs(&[None, Some(0x123456)]);
        assert_eq!(flat, wrong_index);
        assert_ne!(flat, overridden_flat);
    }

    fn overridden_flat_blue_snapshot() -> Vec<u32> {
        // Rendering with fgs=[Some(0x0000FF)] must be identical to rendering
        // with flat fg [0, 0, 255] and no override.
        let (width, height, primitives, len) = text_scene();
        let mut buf = vec![0u32; len];
        raster_serial(
            &mut buf,
            width,
            height,
            &primitives,
            RasterConfig {
                fills: &[],
                fgs: &[],
                fg: [0, 0, 255],
                glyph_px: 1,
                clips: None,
                text_sizes_pt: None,
                clip: None,
            },
        );
        buf
    }

    #[test]
    fn parallel_tiles_match_serial_with_fgs_present() {
        // Text scenes stay serial (see raster_primitives), so exercise both
        // paths through the public dispatcher and direct tile raster.
        let (width, height) = (192, 128);
        let rect = Rect {
            x: 0,
            y: 0,
            width: 180 * MP_PER_PX,
            height: 120 * MP_PER_PX,
        };
        let primitives = vec![goya_runtime::Primitive::Text(
            rect,
            "fgs parity".to_string(),
        )];
        let fgs = [Some(0x00FF7F)];
        let background = argb([4, 8, 12]);
        let mut serial = vec![background; (width * height) as usize];
        let mut parallel = serial.clone();
        raster_serial(
            &mut serial,
            width,
            height,
            &primitives,
            RasterConfig {
                fills: &[],
                fgs: &fgs,
                fg: [255; 3],
                glyph_px: 1,
                clips: None,
                text_sizes_pt: None,
                clip: None,
            },
        );
        // Direct parallel-tiles call defers text scenes to serial.
        let roi = PixelClip {
            x0: 0,
            y0: 0,
            x1: width as usize,
            y1: height as usize,
        };
        let dirty = goya_change::PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        };
        assert!(!raster_parallel_tiles(
            &mut parallel,
            width,
            height,
            &primitives,
            &[],
            &fgs,
            [255, 255, 255],
            1,
            roi,
            dirty,
            CsrPolicy::QuadCorner,
            4,
        ));
        // The public dispatcher serialises the text scene, so both buffers
        // match byte-for-byte with fgs present.
        raster_primitives(
            &mut parallel,
            width,
            height,
            &primitives,
            &[],
            &fgs,
            [255, 255, 255],
            1,
            None,
            None,
            None,
            true,
        );
        assert_eq!(parallel, serial);
    }
}

#[cfg(test)]
mod multiline_tests {
    use super::{
        argb, draw_bitmap_glyphs, draw_glyphs, measure_text, measure_text_caret, raster_primitives,
        snap_grapheme_boundary, MP_PER_PX,
    };
    use goya_layout::Rect;

    fn rect(x: i64, y: i64, w: i64, h: i64) -> Rect {
        Rect {
            x: x * MP_PER_PX,
            y: y * MP_PER_PX,
            width: w * MP_PER_PX,
            height: h * MP_PER_PX,
        }
    }

    #[test]
    fn measured_extent_and_caret_follow_wrapped_shaped_layout() {
        let text = "alpha beta gamma delta";
        let extent = measure_text(text, 48.0, 180.0, 14.0).unwrap();
        assert!(extent.lines >= 2);
        assert!(extent.width_px <= 48.0);
        let first = measure_text_caret(text, 48.0, 180.0, 14.0, 1).unwrap();
        let later = measure_text_caret(text, 48.0, 180.0, 14.0, text.len()).unwrap();
        assert!(first.height_px > 0.0);
        assert!(later.y_px > first.y_px, "{first:?} -> {later:?}");
    }

    #[test]
    fn measured_caret_handles_empty_text_line_ends_and_newlines() {
        let empty = measure_text_caret("", 80.0, 40.0, 12.0, 0).unwrap();
        assert_eq!(empty.x_px, 0.0);
        let text = "abc\ndef";
        let before_newline = measure_text_caret(text, 80.0, 60.0, 12.0, 3).unwrap();
        let after_newline = measure_text_caret(text, 80.0, 60.0, 12.0, 4).unwrap();
        assert!(after_newline.y_px > before_newline.y_px);
        let far_past_end = measure_text_caret(text, 80.0, 60.0, 12.0, usize::MAX).unwrap();
        assert!(far_past_end.x_px.is_finite());
    }

    #[test]
    fn text_metrics_snap_offsets_to_grapheme_boundaries() {
        let text = "a\u{301}👩‍💻z";
        assert_eq!(snap_grapheme_boundary(text, 2), 0);
        assert_eq!(snap_grapheme_boundary(text, 3), 3);
        assert_eq!(snap_grapheme_boundary(text, 7), 3);
        assert_eq!(snap_grapheme_boundary(text, text.len() + 20), text.len());
        let start = measure_text_caret(text, 100.0, 60.0, 12.0, 3).unwrap();
        let inside_emoji = measure_text_caret(text, 100.0, 60.0, 12.0, 7).unwrap();
        assert_eq!(start, inside_emoji);
        assert!(measure_text(text, 0.0, 50.0, 12.0).is_none());
    }

    #[test]
    fn second_line_rasterizes_at_line_height_offset() {
        let (width, height) = (64, 64);
        let r = rect(0, 0, 40, 40);
        let fg = argb([255, 255, 255]);
        let bg = argb([0, 0, 0]);
        // glyph_px = 10 -> line height 14 px; glyph pixels start at (2, 2).
        let line_height = ((10.0_f64) * 1.4) as i64;

        let mut two = vec![bg; (width * height) as usize];
        draw_bitmap_glyphs(&mut two, width, height, &r, "i\ni", [255; 3], 10, None);
        // Column x=2 is inked at the top of each line for the 'i' glyph.
        assert_eq!(two[2 + 2 * width as usize], fg, "line 0 inked");
        assert_eq!(
            two[2 + (2 + line_height) as usize * width as usize],
            fg,
            "line 1 offset by glyph_px * 1.4"
        );

        // Single-line baseline: nothing at the second line offset.
        let mut one = vec![bg; (width * height) as usize];
        draw_bitmap_glyphs(&mut one, width, height, &r, "i", [255; 3], 10, None);
        assert_eq!(one[2 + 2 * width as usize], fg);
        assert_eq!(
            one[2 + (2 + line_height) as usize * width as usize],
            bg,
            "no second line on single-line text"
        );
    }

    #[test]
    fn lines_beyond_node_rect_are_dropped() {
        let (width, height) = (64, 64);
        let fg = argb([255, 255, 255]);
        let bg = argb([0, 0, 0]);
        // Rect height 16 px, glyph_px 10 -> line height 14; line 1 starts
        // at y=16 which equals rect bottom, so only line 0 renders.
        let r = rect(0, 0, 40, 16);
        let mut buf = vec![bg; (width * height) as usize];
        draw_bitmap_glyphs(&mut buf, width, height, &r, "i\ni", [255; 3], 10, None);
        assert_eq!(buf[2 + 2 * width as usize], fg, "line 0 inside rect");
        assert_eq!(
            buf[2 + 16 * width as usize],
            bg,
            "line 1 at rect bottom edge is dropped"
        );

        // A taller rect admits line 1.
        let r_tall = rect(0, 0, 40, 30);
        let mut buf2 = vec![bg; (width * height) as usize];
        draw_bitmap_glyphs(
            &mut buf2, width, height, &r_tall, "i\ni", [255; 3], 10, None,
        );
        assert_eq!(buf2[2 + 16 * width as usize], fg, "line 1 inside tall rect");
    }

    #[test]
    fn glyphs_are_clipped_to_node_rect_right_edge() {
        let (width, height) = (64, 32);
        let bg = argb([0, 0, 0]);
        // Rect ends at x=8 px; a long single-line string must not paint
        // past the rect's right edge.
        let r = rect(0, 0, 8, 20);
        let mut buf = vec![bg; (width * height) as usize];
        draw_bitmap_glyphs(&mut buf, width, height, &r, "iiiiiiii", [255; 3], 1, None);
        let rect_x1 = 8 + 2; // rect right edge + glyph origin inset
        for y in 0..height as usize {
            for x in rect_x1..width as usize {
                assert_eq!(
                    buf[y * width as usize + x],
                    bg,
                    "ink outside node rect at ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn shaped_path_accepts_unicode_and_respects_node_clip() {
        let (width, height) = (48, 32);
        let bg = argb([0, 0, 0]);
        let mut buf = vec![bg; (width * height) as usize];
        let r = rect(0, 0, 24, 24);
        draw_glyphs(
            &mut buf,
            width,
            height,
            &r,
            "Latin Ελληνικά العربية 日本語",
            [255, 255, 255],
            1,
            None,
            None,
        );
        assert!(
            buf.iter().any(|pixel| *pixel != bg),
            "text should rasterize"
        );
        for y in 0..height as usize {
            for x in 26..width as usize {
                assert_eq!(buf[y * width as usize + x], bg, "ink escaped at ({x},{y})");
            }
        }
    }

    #[test]
    fn shaped_path_wraps_at_the_available_width() {
        let (width, height) = (64, 40);
        let bg = argb([0, 0, 0]);
        let r = rect(0, 0, 20, 34);
        let mut buf = vec![bg; (width * height) as usize];
        draw_glyphs(
            &mut buf, width, height, &r, "abcdefgh", [255; 3], 1, None, None,
        );
        let has_ink = |range: std::ops::Range<usize>| {
            range.into_iter().any(|y| {
                buf[y * width as usize..(y + 1) * width as usize]
                    .iter()
                    .any(|pixel| *pixel != bg)
            })
        };
        assert!(has_ink(0..9), "first line should rasterize");
        assert!(has_ink(9..20), "narrow node should wrap to another line");
    }

    #[test]
    fn primitive_clip_constrains_fill_and_shaped_glyphs() {
        let (width, height) = (40, 32);
        let bg = argb([0, 0, 0]);
        let mut buf = vec![bg; (width * height) as usize];
        let node_rect = rect(0, 0, 30, 24);
        let clip_rect = rect(0, 0, 10, 14);
        let primitives = [goya_runtime::Primitive::Button(node_rect, "Clipped".into())];
        raster_primitives(
            &mut buf,
            width,
            height,
            &primitives,
            &[],
            &[],
            [255; 3],
            1,
            Some(&[Some(clip_rect)]),
            None,
            None,
            false,
        );
        assert_eq!(buf[1 + width as usize], argb([80, 100, 180]));
        for y in 0..height as usize {
            for x in 10..width as usize {
                assert_eq!(buf[y * width as usize + x], bg, "clip leaked at ({x},{y})");
            }
        }
        for y in 14..height as usize {
            for x in 0..width as usize {
                assert_eq!(buf[y * width as usize + x], bg, "clip leaked at ({x},{y})");
            }
        }
    }

    #[test]
    fn point_size_changes_native_glyph_metrics_and_invalid_size_falls_back() {
        let (width, height) = (48, 52);
        let bg = argb([0, 0, 0]);
        let r = rect(0, 0, 40, 48);
        let mut regular = vec![bg; (width * height) as usize];
        let mut large = regular.clone();
        let mut invalid = regular.clone();
        draw_glyphs(
            &mut regular,
            width,
            height,
            &r,
            "M",
            [255; 3],
            1,
            None,
            None,
        );
        draw_glyphs(
            &mut large,
            width,
            height,
            &r,
            "M",
            [255; 3],
            1,
            Some(8.0),
            None,
        );
        draw_glyphs(
            &mut invalid,
            width,
            height,
            &r,
            "M",
            [255; 3],
            1,
            Some(f32::NAN),
            None,
        );
        let coverage = |pixels: &[u32]| pixels.iter().filter(|pixel| **pixel != bg).count();
        assert!(coverage(&large) > coverage(&regular));
        assert_eq!(
            invalid, regular,
            "invalid size should use the default metrics"
        );
    }
}

#[cfg(test)]
mod text_overlay_tests {
    use super::{measure_text, raster_text_overlays, TextOverlay};
    use goya_layout::Rect;
    use goya_runtime::Primitive;

    fn text_primitive(text: &str) -> Primitive {
        Primitive::Text(
            Rect {
                x: 0,
                y: 0,
                width: 6_000,
                height: 7_500,
            },
            text.to_owned(),
        )
    }

    #[test]
    fn overlays_draw_without_mutating_text_or_wrapping() {
        let text = "alpha beta gamma delta";
        let primitive = text_primitive(text);
        let primitives = [primitive.clone()];
        let before = measure_text(text, 20.0, 26.0, 12.0).unwrap();
        let mut pixels = vec![0xff00_0000; 24 * 30];
        raster_text_overlays(
            &mut pixels,
            24,
            30,
            &primitives,
            None,
            None,
            &[TextOverlay {
                primitive_index: 0,
                caret_byte_offset: Some(8),
                selection_byte_range: Some((0, 5)),
                caret_rgb: [255, 255, 255],
                selection_rgb: [40, 100, 240],
            }],
            2,
            None,
        );
        assert!(pixels.iter().any(|pixel| *pixel != 0xff00_0000));
        assert_eq!(primitives[0], primitive);
        assert_eq!(measure_text(text, 20.0, 26.0, 12.0).unwrap(), before);
    }

    #[test]
    fn selection_overlay_handles_rtl_and_utf8_cluster_boundaries() {
        let text = "אבגד";
        let primitives = [text_primitive(text)];
        let mut pixels = vec![0xff00_0000; 24 * 30];
        raster_text_overlays(
            &mut pixels,
            24,
            30,
            &primitives,
            None,
            None,
            &[TextOverlay {
                primitive_index: 0,
                caret_byte_offset: Some(1),
                selection_byte_range: Some((1, 7)),
                caret_rgb: [255, 255, 255],
                selection_rgb: [40, 100, 240],
            }],
            2,
            None,
        );
        assert!(pixels.iter().any(|pixel| *pixel != 0xff00_0000));
    }
}

#[cfg(test)]
mod legacy_ime_compatibility {
    use super::*;
    #[test]
    fn legacy_windows_receive_preedit_and_native_windows_keep_cursor() {
        let event = || PlatformEvent::PreeditWithCursor {
            text: "compose".into(),
            cursor_begin: 1,
            cursor_end: 2,
        };
        assert!(
            matches!(legacy_text_input_event(event(), false), Some(PlatformEvent::Preedit(text)) if text.as_deref() == Some("compose"))
        );
        assert!(matches!(
            legacy_text_input_event(event(), true),
            Some(PlatformEvent::PreeditWithCursor {
                cursor_begin: 1,
                cursor_end: 2,
                ..
            })
        ));
    }
}

#[cfg(test)]
mod browser_navigation_compatibility {
    use super::*;
    #[test]
    fn navigation_is_opt_in_and_preserves_session_shortcuts() {
        let mut inner = GoyaWindowInner::new(400, 240);
        inner.ctrl = true;
        inner.handle_key_press(106);
        assert_eq!(
            inner.events.pop(),
            Some(PlatformEvent::Shortcut(Shortcut::SessionNext))
        );
        inner.browser_navigation_keys = true;
        inner.handle_key_press(106);
        assert_eq!(
            inner.events.pop(),
            Some(PlatformEvent::Shortcut(Shortcut::SessionNext))
        );
        inner.ctrl = false;
        inner.alt = true;
        inner.handle_key_press(105);
        assert_eq!(
            inner.events.pop(),
            Some(PlatformEvent::BrowserNavigation(BrowserNavigation::Back))
        );
        inner.handle_key_press(106);
        assert_eq!(
            inner.events.pop(),
            Some(PlatformEvent::BrowserNavigation(BrowserNavigation::Forward))
        );
        inner.alt = false;
        inner.ctrl = true;
        inner.handle_key_press(19);
        assert_eq!(
            inner.events.pop(),
            Some(PlatformEvent::BrowserNavigation(BrowserNavigation::Reload))
        );
    }
}
