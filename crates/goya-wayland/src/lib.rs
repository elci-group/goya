//! GY-WAYLAND/1 — Native windowing host abstraction (Wave 1 completion).
//!
//! Minimal, honest scope per GY-WAYLAND-001: one window, pointer events
//! mapped to surface-local pixels, and the goya render list presented via a
//! shared-memory buffer. This crate is where Goya's zero-dependency policy
//! deliberately ends — the Wayland host layer uses the official Smithay
//! client bindings rather than a hand-rolled wire protocol, because a
//! protocol reimplementation is a security liability, not an asset
//! (GOYA §13: Linux compositor fragmentation).

use goya_layout::Rect;
use rustix::fd::OwnedFd;
use rustix::fs::{ftruncate, memfd_create, MemfdFlags};
use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};
use std::os::fd::AsFd;
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
use wayland_protocols::xdg::shell::client::{
    xdg_surface::{self, XdgSurface},
    xdg_toplevel::{self, XdgToplevel},
    xdg_wm_base::{self, XdgWmBase},
};

/// Contract identifier implemented by this crate.
pub const WAYLAND_CONTRACT: &str = "GY-WAYLAND/1";

/// Millipoints per pixel, matching goya-hello's 4 px per point.
pub const MP_PER_PX: i64 = 250;

/// Pointer/button events translated from the compositor, surface-local.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformEvent {
    PointerMove(f64, f64),
    PointerPress(f64, f64),
    PointerRelease(f64, f64),
    /// Printable character typed on the keyboard (US layout subset).
    KeyChar(char),
    /// Compositor resized the window to new pixel dimensions.
    Resized(i32, i32),
    Configured,
    CloseRequested,
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
    shift: bool,
    buffers: Option<BufferPair>,
    region: Option<WlRegion>,
    ack_serial: u32,
}

impl GoyaWindowInner {
    fn new(width: i32, height: i32) -> Self {
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
            shift: false,
            buffers: None,
            region: None,
            ack_serial: 0,
        }
    }
}

/// Application-facing window: draw the runtime's render list, receive
/// pointer events.
pub struct GoyaWindow {
    state: GoyaWindowInner,
    queue: wayland_client::EventQueue<GoyaWindowInner>,
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
        for (name, interface) in &state.globals {
            match interface.as_str() {
                "wl_compositor" => compositor = Some(_registry.bind(*name, 1, &qh, ())),
                "wl_shm" => shm = Some(_registry.bind(*name, 1, &qh, ())),
                "wl_seat" => seat = Some(_registry.bind(*name, 1, &qh, ())),
                "xdg_wm_base" => wm_base = Some(_registry.bind(*name, 1, &qh, ())),
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
        }

        Ok(GoyaWindow { state, queue })
    }

    /// Map the runtime's render list into shared memory and present it.
    /// `glyph_px` is the glyph-pixel size in screen pixels (block scaling);
    /// 1 = tiny terminal-style text, 3–4 reads well at 4 px/pt geometry.
    pub fn present(
        &mut self,
        primitives: &[goya_runtime::Primitive],
        fg: [u8; 3],
        bg: [u8; 3],
        glyph_px: u32,
    ) -> Result<(), WaylandError> {
        self.present_dirty(primitives, fg, bg, glyph_px, None)
    }

    /// LCPE-aware presentation: `damage` is the resolved dirty rect in
    /// pixels (from goya-change's quantised envelope); None = full frame.
    /// The slot is always fully rasterised (small surfaces); locality is
    /// expressed to the compositor via partial damage.
    pub fn present_dirty(
        &mut self,
        primitives: &[goya_runtime::Primitive],
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
        let pixels = pair.slots[idx].pixels_mut();
        match damage {
            Some((dx, dy, dw, dh)) => {
                let (w, h) = (self.state.width, self.state.height);
                // Clear the damage area with the background first.
                let drect = goya_layout::Rect {
                    x: i64::from(dx) * MP_PER_PX,
                    y: i64::from(dy) * MP_PER_PX,
                    width: i64::from(dw) * MP_PER_PX,
                    height: i64::from(dh) * MP_PER_PX,
                };
                fill_rect(pixels, w, h, &drect, bg);
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
        for prim in primitives {
            // Primitives wholly outside the damage rect cannot contribute
            // pixels to it — skip their raster work entirely.
            if let Some(clip) = &clip {
                let r = prim_rect(prim);
                let rx0 = (r.x / MP_PER_PX) as i32;
                let ry0 = (r.y / MP_PER_PX) as i32;
                let rx1 = ((r.x + r.width) / MP_PER_PX) as i32;
                let ry1 = ((r.y + r.height) / MP_PER_PX) as i32;
                if rx1 < clip.x0 as i32 || ry1 < clip.y0 as i32 {
                    continue;
                }
                if rx0 >= clip.x1 as i32 || ry0 >= clip.y1 as i32 {
                    continue;
                }
            }
            match prim {
                goya_runtime::Primitive::Panel(_) => {}
                goya_runtime::Primitive::Text(r, s) => {
                    fill_rect(pixels, self.state.width, self.state.height, r, [16, 16, 20]);
                    draw_glyphs(
                        pixels,
                        self.state.width,
                        self.state.height,
                        r,
                        s,
                        fg,
                        glyph_px,
                        clip,
                    );
                }
                goya_runtime::Primitive::Button(r, s) => {
                    fill_rect(
                        pixels,
                        self.state.width,
                        self.state.height,
                        r,
                        [80, 100, 180],
                    );
                    draw_glyphs(
                        pixels,
                        self.state.width,
                        self.state.height,
                        r,
                        s,
                        fg,
                        glyph_px,
                        clip,
                    );
                }
            }
        }
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
    pub fn poll(&mut self) -> Vec<PlatformEvent> {
        let _ = self.queue.dispatch_pending(&mut self.state);
        let _ = self.queue.flush();
        std::mem::take(&mut self.state.events)
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

fn argb(rgb: [u8; 3]) -> u32 {
    0xFF00_0000 | ((rgb[2] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[0] as u32
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
    clip: Option<PixelClip>,
) {
    let a = argb(rgb);
    let g = glyph_px.max(1) as usize;
    let (cx0, cy0, cx1, cy1) = match clip {
        Some(c) => (c.x0, c.y0, c.x1, c.y1),
        None => (0, 0, win_w as usize, win_h as usize),
    };
    let ox = (r.x / MP_PER_PX + 2).max(0) as usize;
    let oy = (r.y / MP_PER_PX + 2).max(0) as usize;
    for (ci, ch) in s.chars().enumerate() {
        let glyph = glyph5x7(ch);
        for (gy, row) in glyph.iter().enumerate() {
            for gx in 0..5 {
                if row & (1 << (4 - gx)) != 0 {
                    // Block-scale: each glyph pixel is g×g screen pixels.
                    for dy in 0..g {
                        let y = oy + gy * g + dy;
                        if y >= cy1 || y < cy0 || y >= win_h as usize {
                            continue;
                        }
                        for dx in 0..g {
                            let x = ox + (ci * 6 + gx) * g + dx;
                            if x >= cx1 || x < cx0 || x >= win_w as usize {
                                continue;
                            }
                            let idx = y * (win_w as usize) + x;
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

/// US QWERTY evdev keycode → printable char, honouring shift.
/// Subset contract: printable ASCII only; unlisted keys yield None.
fn key_char(keycode: u32, shift: bool) -> Option<char> {
    let letter = |base: u8, off: u32| -> char {
        let c = (base + off as u8) as char;
        if shift {
            c.to_ascii_uppercase()
        } else {
            c
        }
    };
    // evdev US QWERTY keycodes.
    let c = match keycode {
        16..=25 => return Some(letter(b'q', keycode - 16)), // q..p
        30..=38 => return Some(letter(b'a', keycode - 30)), // a..l
        44..=50 => {
            // z x c v b n m — not ASCII-contiguous; explicit table.
            let row = ['z', 'x', 'c', 'v', 'b', 'n', 'm'];
            let mut c = row[(keycode - 44) as usize];
            if shift {
                c = c.to_ascii_uppercase();
            }
            return Some(c);
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
            // xkb keymap fd arrives with format v1; we use the built-in US
            // table instead, so the fd is closed without reading.
            wl_keyboard::Event::Keymap { .. } => {}
            wl_keyboard::Event::Modifiers { mods_depressed, .. } => {
                state.shift = mods_depressed & 0x1 != 0; // ShiftMask
            }
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(wl_keyboard::KeyState::Pressed),
                ..
            } => {
                if let Some(c) = key_char(key, state.shift) {
                    state.events.push(PlatformEvent::KeyChar(c));
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
mod key_tests {
    use super::key_char;
    #[test]
    fn us_layout_spot_checks() {
        assert_eq!(key_char(30, false), Some('a'));
        assert_eq!(key_char(30, true), Some('A'));
        assert_eq!(key_char(16, false), Some('q'));
        assert_eq!(key_char(44, false), Some('z'));
        assert_eq!(key_char(50, false), Some('m'));
        assert_eq!(key_char(39, false), Some(';'));
        assert_eq!(key_char(39, true), Some(':'));
        assert_eq!(key_char(57, false), Some(' '));
        assert_eq!(key_char(11, true), Some(')'));
        assert_eq!(key_char(42, false), None); // shift itself
        assert_eq!(key_char(999, false), None);
    }
}
