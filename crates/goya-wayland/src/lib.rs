//! GY-WAYLAND/1 — Native windowing host abstraction (Wave 1 completion).
//!
//! Minimal, honest scope per GY-WAYLAND-001: one window, pointer events
//! mapped to surface-local pixels, and the goya render list presented via a
//! shared-memory buffer. Keyboard input additionally surfaces ctrl+c /
//! ctrl+v / ctrl+q as [`PlatformEvent::Shortcut`] events (GY-ZCODE-004);
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
//! enable/disable around editable focus (disabled by default). Preedit and
//! commit updates are queued by the pure [`TextInputState`] machine and
//! delivered as [`PlatformEvent::Preedit`] / [`PlatformEvent::CommitText`]
//! after the protocol `done` frame; commit text arrives whole and is never
//! synthesized into [`PlatformEvent::KeyChar`] events.
//!
//! Full IME operation (candidate windows, actual CJK composition) is
//! compositor- and input-method-dependent: unit tests in this crate cover
//! only the compositor-free state machine and no-op paths. Whether a given
//! desktop (wlroots, GNOME, KDE) actually delivers text-input-v3 traffic is
//! an operator-verified acceptance item, not covered by tests here.

use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache, Wrap};
use goya_csr::{self, Policy as CsrPolicy};
use goya_hcar::{AffinityMode, Classifier, CoreAssignment, WorkCost, WorkItem};
use goya_layout::Rect;
use rustix::fd::OwnedFd;
use rustix::fs::{ftruncate, memfd_create, MemfdFlags};
use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsFd;
use std::sync::{Mutex, OnceLock};
use unicode_segmentation::UnicodeSegmentation;
use wayland_client::protocol::{
    wl_buffer::WlBuffer,
    wl_compositor::WlCompositor,
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
    /// Full text-input-v3 preedit update, including byte-indexed cursor or
    /// selection endpoints. `text: None` clears an existing preedit.
    PreeditWithCursor {
        text: Option<String>,
        cursor_begin: i32,
        cursor_end: i32,
    },
    /// Whole commit string from the IME (GY-ZCODE-005). This is never
    /// synthesized as [`PlatformEvent::KeyChar`] events — hosts insert it at
    /// their own cursor. Applied after the protocol `done` frame.
    CommitText(String),
    /// Delete UTF-8 bytes around the committed-text cursor, as requested by
    /// text-input-v3. Hosts expand endpoints to valid grapheme boundaries.
    DeleteSurroundingText {
        before_bytes: u32,
        after_bytes: u32,
    },
    /// Compositor resized the window to new pixel dimensions.
    Resized(i32, i32),
    Configured,
    CloseRequested,
}

/// Application-level shortcut keys (GY-ZCODE-004): ctrl+c / ctrl+v / ctrl+q
/// recognised through the compositor's xkb state path (ctrl modifier tracking
/// plus evdev keycode). The keymap-free evdev fallback path never emits these
/// because it does not track ctrl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Copy,
    Paste,
    Quit,
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
/// The protocol delivers `preedit_string` / `commit_string` /
/// `delete_surrounding_text` in an uncommitted staging area and applies the
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
    pending_delete: Option<(u32, u32)>,
    /// Commit strings staged by `commit_string`, flushed in arrival order.
    pub committed: Vec<String>,
    /// Preedit the host currently has applied (updated on drain).
    pub applied_preedit: Option<String>,
    events: Vec<PlatformEvent>,
    armed: bool,
}

impl TextInputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage a `preedit_string` frame update. `None` stages a clear.
    pub fn feed_preedit(&mut self, preedit: Option<String>) {
        let cursor_end = preedit
            .as_ref()
            .map_or(0, |text| text.len().min(i32::MAX as usize) as i32);
        self.pending_preedit = preedit;
        self.pending_preedit_cursor = Some((0, cursor_end));
        self.preedit_received = true;
    }

    /// Stage a protocol preedit and its byte-indexed caret/selection. Invalid
    /// UTF-8 boundaries are normalized by the receiving text buffer.
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

    /// Stage the latest delete-surrounding-text request in this protocol
    /// frame; the value remains unapplied until [`Self::feed_done`].
    pub fn feed_delete_surrounding(&mut self, before_bytes: u32, after_bytes: u32) {
        self.pending_delete = Some((before_bytes, after_bytes));
    }

    /// Arm the staged frame: the next drain applies it atomically. A staged
    /// clear (`preedit_string` with no text) also arms when a preedit is
    /// currently applied — clearing is a visible frame change.
    pub fn feed_done(&mut self) {
        if self.preedit_received || self.pending_delete.is_some() || !self.committed.is_empty() {
            self.armed = true;
        }
    }

    /// Surface left the text input: preedit is implicitly cleared, staged
    /// data is discarded.
    pub fn feed_leave(&mut self) {
        self.pending_preedit = None;
        self.pending_preedit_cursor = None;
        self.preedit_received = false;
        self.pending_delete = None;
        self.committed.clear();
        self.armed = false;
        if self.applied_preedit.is_some() {
            self.events.push(PlatformEvent::Preedit(None));
            self.applied_preedit = None;
        }
    }

    /// Emit the armed protocol frame in text-input-v3 order: clear existing
    /// preedit, delete surrounding bytes, apply commits, then install preedit.
    /// Draining without an armed frame is a no-op.
    pub fn drain(&mut self) -> Vec<PlatformEvent> {
        if self.armed {
            let has_update = self.preedit_received
                || self.pending_delete.is_some()
                || !self.committed.is_empty();
            if has_update && self.applied_preedit.take().is_some() {
                self.events.push(PlatformEvent::Preedit(None));
            }
            if let Some((before_bytes, after_bytes)) = self.pending_delete.take() {
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
                    let (cursor_begin, cursor_end) = self
                        .pending_preedit_cursor
                        .unwrap_or((0, text.len().min(i32::MAX as usize) as i32));
                    if cursor_begin == 0 && cursor_end == text.len().min(i32::MAX as usize) as i32 {
                        self.events.push(PlatformEvent::Preedit(Some(text.clone())));
                    } else {
                        self.events.push(PlatformEvent::PreeditWithCursor {
                            text: Some(text.clone()),
                            cursor_begin,
                            cursor_end,
                        });
                    }
                }
                self.applied_preedit = preedit.filter(|text| !text.is_empty());
            }
            self.pending_preedit_cursor = None;
            self.preedit_received = false;
            self.armed = false;
        }
        std::mem::take(&mut self.events)
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
}

impl std::fmt::Display for WaylandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaylandError::NoDisplay => write!(f, "no Wayland display reachable"),
            WaylandError::MissingGlobal(g) => write!(f, "compositor lacks {g}"),
            WaylandError::ShmSetup(e) => write!(f, "shared memory setup failed: {e}"),
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
        // Neutralise per-slot mappings first: the pair owns the whole map.
        for slot in &mut self.slots {
            slot.map = std::ptr::null_mut();
            slot.len = 0;
            slot.buffer.destroy();
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
    shift: bool,
    ctrl: bool,
    buffers: Option<BufferPair>,
    region: Option<WlRegion>,
    ack_serial: u32,
    /// Explicitly opt into general-core parallel tile raster. No core
    /// affinity is requested; Linux remains responsible for placement.
    parallel_raster: bool,
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
            shift: false,
            ctrl: false,
            buffers: None,
            region: None,
            ack_serial: 0,
            parallel_raster: false,
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
    /// Order matters: ctrl+c/v/q map to `PlatformEvent::Shortcut` first;
    /// otherwise navigation keeps its `KeySpecial` semantics and text keys go
    /// through XKB Compose → `KeyChar`, byte-identical to the pre-shortcut
    /// behavior. Other ctrl combos produce no events (the char path returns
    /// nothing while ctrl is held).
    fn handle_key_press(&mut self, key: u32) {
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
        for (name, interface) in &state.globals {
            match interface.as_str() {
                "wl_compositor" => compositor = Some(_registry.bind(*name, 1, &qh, ())),
                "wl_shm" => shm = Some(_registry.bind(*name, 1, &qh, ())),
                "wl_seat" => seat = Some(_registry.bind(*name, 1, &qh, ())),
                "xdg_wm_base" => wm_base = Some(_registry.bind(*name, 1, &qh, ())),
                "zwp_text_input_manager_v3" => {
                    text_input_manager = Some(_registry.bind(*name, 1, &qh, ()))
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
        surface.commit();

        // Pointer and keyboard, if the seat has them.
        if let Some(seat) = seat {
            state.pointer = Some(seat.get_pointer(&qh, ()));
            state.keyboard = Some(seat.get_keyboard(&qh, ()));
            if let Some(manager) = &state.text_input_manager {
                state.text_input = Some(manager.get_text_input(&seat, &qh, ()))
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

    /// Map the runtime's render list into shared memory and present it.
    /// `glyph_px` is the glyph-pixel size in screen pixels (block scaling);
    /// 1 = tiny terminal-style text, 3–4 reads well at 4 px/pt geometry.
    /// `fills` is a parallel slice indexed by primitive order (GY-WAVE2
    /// contract): `Some(0xRRGGBB)` overrides that primitive's flat fill
    /// colour; `None` keeps the existing default.
    pub fn present(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_internal(primitives, fills, None, None, None, fg, bg, glyph_px, None)
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
        self.present_internal(primitives, fills, Some(clips), None, None, fg, bg, glyph_px, None)
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
            Some(clips),
            Some(text_sizes_pt),
            None,
            fg,
            bg,
            glyph_px,
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
            None,
            Some(text_sizes_pt),
            None,
            fg,
            bg,
            glyph_px,
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
            Some(clips),
            Some(text_sizes_pt),
            Some(overlays),
            fg,
            bg,
            glyph_px,
            None,
        )
    }

    /// LCPE-aware presentation: `damage` is the resolved dirty rect in
    /// pixels (from goya-change's quantised envelope); None = full frame.
    /// The slot is always fully rasterised (small surfaces); locality is
    /// expressed to the compositor via partial damage. See [`GoyaWindow::present`]
    /// for the `fills` contract.
    pub fn present_dirty(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
        damage: Option<(i32, i32, i32, i32)>,
    ) -> Result<(), WaylandError> {
        self.present_internal(primitives, fills, None, None, None, fg, bg, glyph_px, damage)
    }

    #[allow(clippy::too_many_arguments)]
    fn present_internal(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fills: &[Option<u32>],
        clips: Option<&[Option<Rect>]>,
        text_sizes_pt: Option<&[Option<f32>]>,
        text_overlays: Option<&[TextOverlay]>,
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
        damage: Option<(i32, i32, i32, i32)>,
    ) -> Result<(), WaylandError> {
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
            fg,
            glyph_px,
            clips,
            text_sizes_pt,
            clip,
            parallel_raster,
        );
        // Opaque hint: the window always fills its bounds — the compositor
        // can skip blending against undefined content (flicker source).
        {
            let surface = self
                .state
                .surface
                .as_ref()
                .ok_or(WaylandError::MissingGlobal("surface"))?;
            if self.state.region.is_none() {
                if let Some(comp) = self.state.compositor.clone() {
                    let region = comp.create_region(&self.queue.handle(), ());
                    region.add(0, 0, self.state.width, self.state.height);
                    surface.set_opaque_region(Some(&region));
                    self.state.region = Some(region);
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
        surface.commit();
        pair.slots[idx].released = false;
        pair.front = idx;
        Ok(())
    }

    /// Dispatch pending compositor messages and return queued platform events.
    /// Text-input-v3 preedit/commit frames are drained from the pure
    /// [`TextInputState`] machine here, after `done` dispatch, so upstream
    /// consumers always see atomically applied IME updates.
    pub fn poll(&mut self) -> Vec<PlatformEvent> {
        let _ = self.queue.dispatch_pending(&mut self.state);
        let _ = self.queue.flush();
        let drained = self.state.text_input_state.drain();
        let mut events = std::mem::take(&mut self.state.events);
        events.extend(drained);
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

/// Resolve a primitive's fill colour for its index `i` (GY-WAVE2 contract):
/// `fills[i] = Some(0xRRGGBB)` wins; `None` (or a short slice) falls back to
/// the existing flat default. Returns the RGB triple for rasterisation.
fn resolve_fill(fills: &[Option<u32>], i: usize, default: [u8; 3]) -> [u8; 3] {
    match fills.get(i).copied().flatten() {
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
            pixels, width, height, primitives, fills, fg, glyph_px, roi, dirty, policy, workers,
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
                fg,
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
                fg,
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
                fg,
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
                fg,
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

/// 5x7 glyphs (leftmost pixel = bit 4); unknown characters render as a box.
fn glyph5x7(c: char) -> [u8; 7] {
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
        _: &mut Self,
        _: &WlBuffer,
        _: wayland_client::protocol::wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
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

impl Dispatch<ZwpTextInputV3, ()> for GoyaWindowInner {
    fn event(
        state: &mut Self,
        _: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                state
                    .text_input_state
                    .feed_preedit_with_cursor(text, cursor_begin, cursor_end);
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                state.text_input_state.feed_commit(text);
            }
            zwp_text_input_v3::Event::Done { .. } => {
                state.text_input_state.feed_done();
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                state.text_input_state.feed_leave();
            }
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => state
                .text_input_state
                .feed_delete_surrounding(before_length, after_length),
            zwp_text_input_v3::Event::Enter { .. }
            | zwp_text_input_v3::Event::Language { .. }
            | zwp_text_input_v3::Event::Action { .. }
            | zwp_text_input_v3::Event::PreeditHint { .. } => {}
            _ => {}
        }
    }
}

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
    let mut file = File::from(fd);
    let mut bytes = Vec::with_capacity(size as usize);
    file.by_ref()
        .take(u64::from(size))
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() != size as usize {
        return None;
    }
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
        return Vec::new();
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

/// Pure decision helper (GY-ZCODE-004 contract): map ctrl + evdev keycode to
/// an application shortcut. Only ctrl+c (46), ctrl+v (47) and ctrl+q (16)
/// are recognised; every other combination — including plain (non-ctrl)
/// presses of those keys — yields None and stays inert, exactly as in the
/// pre-shortcut behavior.
fn shortcut_from_key(ctrl: bool, keycode: u32) -> Option<Shortcut> {
    if !ctrl {
        return None;
    }
    match keycode {
        46 => Some(Shortcut::Copy),  // KEY_C
        47 => Some(Shortcut::Paste), // KEY_V
        16 => Some(Shortcut::Quit),  // KEY_Q
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
    fn compositor_keymap_fd_is_parsed_and_oversized_maps_are_rejected() {
        use super::{keymap_state_from_fd, wl_keyboard, WEnum, MAX_XKB_KEYMAP_BYTES};
        use std::fs::File;
        use std::io::{Seek, SeekFrom, Write};
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
        file.seek(SeekFrom::Start(0)).unwrap();
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
                                                                       // Non-ctrl presses of the same keys stay plain text, not shortcuts.
        assert_eq!(shortcut_from_key(false, 46), None);
        assert_eq!(shortcut_from_key(false, 47), None);
        assert_eq!(shortcut_from_key(false, 16), None);
        assert_eq!(shortcut_from_key(false, 999), None);
    }

    #[test]
    fn other_ctrl_combos_stay_inert() {
        // Ctrl held, but not c/v/q: helper must yield None for letters,
        // digits, navigation codes and unknown codes alike.
        for kc in [16 - 1, 30, 31, 32, 44, 45, 48, 2, 11, 57, 1, 105, 999, 0] {
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

        inner.events.clear();
        inner.shift = false;
        inner.ctrl = true;
        inner.handle_key_press(106); // Right
        assert_eq!(
            inner.events,
            vec![PlatformEvent::KeySpecialWithModifiers {
                key: super::KeySpecial::ArrowRight,
                modifiers: KeyModifiers {
                    shift: false,
                    ctrl: true,
                },
            }]
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
    use super::{text_input_activation, PlatformEvent, TextInputState};

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
    fn delete_and_preedit_cursor_are_forwarded_in_protocol_order() {
        let mut s = TextInputState::new();
        s.feed_preedit(Some("compose".into()));
        s.feed_done();
        assert_eq!(
            s.drain(),
            vec![PlatformEvent::Preedit(Some("compose".into()))]
        );

        s.feed_delete_surrounding(3, 2);
        s.feed_commit(Some("語".into()));
        s.feed_preedit_with_cursor(Some("かな".into()), 3, 6);
        assert!(s.drain().is_empty(), "the batch waits for done");
        s.feed_done();
        assert_eq!(
            s.drain(),
            vec![
                PlatformEvent::Preedit(None),
                PlatformEvent::DeleteSurroundingText {
                    before_bytes: 3,
                    after_bytes: 2,
                },
                PlatformEvent::CommitText("語".into()),
                PlatformEvent::PreeditWithCursor {
                    text: Some("かな".into()),
                    cursor_begin: 3,
                    cursor_end: 6,
                },
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
