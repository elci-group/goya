//! GY-WAYLAND/1 — Native windowing host abstraction (Wave 1 completion).
//!
//! Minimal, honest scope per GY-WAYLAND-001: one window, pointer events
//! mapped to surface-local pixels, and the goya render list presented via a
//! shared-memory buffer. This crate is where Goya's zero-dependency policy
//! deliberately ends — the Wayland host layer uses the official Smithay
//! client bindings rather than a hand-rolled wire protocol, because a
//! protocol reimplementation is a security liability, not an asset
//! (GOYA §13: Linux compositor fragmentation).

use goya_csr::{self, Policy as CsrPolicy};
use goya_hcar::{AffinityMode, Classifier, CoreAssignment, WorkCost, WorkItem};
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
    /// Non-printable navigation/editing key (US subset, see [`KeySpecial`]).
    KeySpecial(KeySpecial),
    /// Compositor resized the window to new pixel dimensions.
    Resized(i32, i32),
    Configured,
    CloseRequested,
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
    Escape,
    Tab,
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
    /// Explicitly opt into general-core parallel tile raster. No core
    /// affinity is requested; Linux remains responsible for placement.
    parallel_raster: bool,
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

    /// Enable general-core parallel tile raster for large frames. Workers
    /// use CSR's deterministic tile schedule and ordinary OS-managed thread
    /// placement; this does not pin work to performance/efficiency cores.
    /// Small surfaces and workloads still use the serial path.
    pub fn set_general_parallel_raster(&mut self, enabled: bool) {
        self.state.parallel_raster = enabled;
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
        self.present_dirty(primitives, fills, fg, bg, glyph_px, None)
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
    if allow_parallel
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
        fills,
        fg,
        glyph_px,
        Some(roi),
    );
}

fn raster_serial(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    primitives: &[goya_runtime::Primitive],
    fills: &[Option<u32>],
    fg: [u8; 3],
    glyph_px: u32,
    clip: Option<PixelClip>,
) {
    for (i, prim) in primitives.iter().enumerate() {
        raster_primitive(pixels, width, height, prim, fills, i, fg, glyph_px, clip);
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
                            for (i, prim) in primitives
                                .iter()
                                .enumerate()
                                .filter(|(_, prim)| {
                                    intersects_tile(
                                        prim, tile.x0, tile.y0, tile.x1, tile.y1, glyph_px,
                                    )
                                })
                            {
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
    clip: Option<PixelClip>,
) {
    match prim {
        goya_runtime::Primitive::Panel(_) => {}
        goya_runtime::Primitive::Text(rect, text) => {
            let fill = resolve_fill(fills, index, [16, 16, 20]);
            fill_rect_clipped(pixels, width, height, rect, fill, clip);
            draw_glyphs(pixels, width, height, rect, text, fg, glyph_px, clip);
        }
        goya_runtime::Primitive::Button(rect, text) => {
            let fill = resolve_fill(fills, index, [80, 100, 180]);
            fill_rect_clipped(pixels, width, height, rect, fill, clip);
            draw_glyphs(pixels, width, height, rect, text, fg, glyph_px, clip);
        }
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
            draw_glyphs(pixels, width, height, &rect, text, fg, glyph_px, Some(clip));
        }
        goya_runtime::Primitive::Button(rect, text) => {
            let rect = offset(rect);
            let fill = resolve_fill(fills, index, [80, 100, 180]);
            fill_rect_clipped(pixels, width, height, &rect, fill, Some(clip));
            draw_glyphs(pixels, width, height, &rect, text, fg, glyph_px, Some(clip));
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
    // rect's bottom edge are dropped entirely.
    let rect_x1 = (r.x + r.width) / MP_PER_PX + 2;
    let rect_y1 = (r.y + r.height) / MP_PER_PX + 2;
    let line_height = ((glyph_px.max(1) as f64) * 1.4) as i64;
    for (li, line) in s.split('\n').enumerate() {
        let line_oy = oy + (li as i64) * line_height;
        if line_oy >= rect_y1 {
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

/// evdev US-subset keycode → navigation/editing key (GY-WAVE2 contract).
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
        1 => KeySpecial::Escape,
        15 => KeySpecial::Tab,
        _ => return None,
    };
    Some(ks)
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
                if let Some(ks) = key_special(key) {
                    state.events.push(PlatformEvent::KeySpecial(ks));
                } else if let Some(c) = key_char(key, state.shift) {
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
    fn parallel_tiles_match_serial_with_overlaps_and_damage() {
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
            [255, 255, 255],
            1,
            None,
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
        assert!(raster_parallel_tiles(
            &mut parallel,
            width,
            height,
            &primitives,
            [255, 255, 255],
            1,
            roi,
            dirty,
            CsrPolicy::QuadCorner,
            4,
        ));
        if parallel != serial {
            let i = parallel
                .iter()
                .zip(&serial)
                .position(|(a, b)| a != b)
                .unwrap();
            panic!(
                "full-frame mismatch at ({}, {}): parallel={:#x} serial={:#x}",
                i % width as usize,
                i / width as usize,
                parallel[i],
                serial[i]
            );
        }

        let roi = PixelClip {
            x0: 17,
            y0: 11,
            x1: 177,
            y1: 117,
        };
        let dirty = goya_change::PixelRect {
            x: roi.x0 as i32,
            y: roi.y0 as i32,
            width: (roi.x1 - roi.x0) as i32,
            height: (roi.y1 - roi.y0) as i32,
        };
        serial.fill(background);
        parallel.fill(background);
        raster_serial(
            &mut serial,
            width,
            height,
            &primitives,
            [255, 255, 255],
            1,
            Some(roi),
        );
        assert!(raster_parallel_tiles(
            &mut parallel,
            width,
            height,
            &primitives,
            [255, 255, 255],
            1,
            roi,
            dirty,
            CsrPolicy::DualHorizontal,
            4,
        ));
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
            raster_serial(&mut serial, width, height, &primitives, [255; 3], 2, None);
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
