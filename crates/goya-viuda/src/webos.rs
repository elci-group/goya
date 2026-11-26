//! The webOS host (V-Native, Developer Mode).
//!
//! It connects to the TV's compositor (`/tmp/xdg/wayland-0` in the native
//! app jail) with the pure-Rust Wayland client. It opens one `wl_shell`
//! surface, tagged with the app id through `wl_webos_shell`, and presents
//! software-rendered frames through two `wl_shm` buffers in `XBGR8888`,
//! whose bytes are tiny-skia's RGBA order, so no channel swizzle is
//! needed. Keys arrive as evdev codes from every `wl_seat`, because webOS
//! advertises one seat per input device.
//!
//! Lifecycle: `wl_webos_shell_surface.state_changed` to minimised suspends
//! the resolver instance, and any other state resumes it. `close`
//! terminates it and the process exits.
//!
//! The protocol file is LG's `webos-shell.xml` (Apache-2.0) from
//! webOS OSE. webOS appends requests between releases without changing the
//! interface version. The requests used here (`get_shell_surface`,
//! `set_state`, `set_property`) sit at the same opcodes from webOS 1.2
//! through 10.2.

use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rustix::event::{poll, PollFd, PollFlags};
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_registry, wl_seat, wl_shell,
    wl_shell_surface, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle, WEnum};

use crate::{Key, Viuda};

#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    missing_docs,
    unused_imports,
    clippy::all
)]
pub mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocol/webos-shell.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocol/webos-shell.xml");
}
use protocol::{wl_webos_shell, wl_webos_shell_surface};

/// `wl_webos_shell_surface` states.
const STATE_MINIMIZED: u32 = 1;
const STATE_FULLSCREEN: u32 = 3;

/// Map an evdev key code from the TV remote. Arrows, OK (Enter, keypad
/// Enter, `KEY_OK`, `KEY_SELECT`) and the digits. Back, Home and Exit are
/// left to the platform: the default `wl_webos_shell` key mask does not
/// deliver them.
pub fn map_key(code: u32) -> Option<Key> {
    match code {
        103 => Some(Key::Up),
        108 => Some(Key::Down),
        105 => Some(Key::Left),
        106 => Some(Key::Right),
        28 | 96 | 352 | 353 => Some(Key::Ok),
        2..=10 => Some(Key::Digit((code - 1) as u8)),
        11 => Some(Key::Digit(0)),
        0x200..=0x209 => Some(Key::Digit((code - 0x200) as u8)),
        _ => None,
    }
}

pub struct Options {
    pub app_id: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Shortest interval between frames that only ambient motion needs.
    /// The scene's own [`Viuda::ambient_interval`] is used when longer.
    pub ambient_interval: Duration,
    /// When the process started, for the first-frame time.
    pub started: Instant,
}

#[derive(Default)]
struct Host {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    shell: Option<wl_shell::WlShell>,
    webos: Option<wl_webos_shell::WlWebosShell>,
    seats: Vec<wl_seat::WlSeat>,
    keyboards: Vec<u32>,
    keys: Vec<u32>,
    busy: Vec<bool>,
    frame_done: bool,
    closed: bool,
    minimized: bool,
    output_mode: Option<(i32, i32)>,
    notes: Vec<String>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Host {
    fn event(
        host: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => host.compositor = Some(registry.bind(name, version.min(3), qh, ())),
            "wl_shm" => host.shm = Some(registry.bind(name, 1, qh, ())),
            "wl_shell" => host.shell = Some(registry.bind(name, 1, qh, ())),
            "wl_webos_shell" => host.webos = Some(registry.bind(name, 1, qh, ())),
            "wl_seat" => host.seats.push(registry.bind(name, version.min(4), qh, ())),
            "wl_output" => {
                registry.bind::<wl_output::WlOutput, _, _>(name, version.min(2), qh, ());
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Host {
    fn event(
        host: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        {
            let id = seat.id().protocol_id();
            if caps.contains(wl_seat::Capability::Keyboard) && !host.keyboards.contains(&id) {
                host.keyboards.push(id);
                seat.get_keyboard(qh, ());
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Host {
    fn event(
        host: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            key,
            state: WEnum::Value(wl_keyboard::KeyState::Pressed),
            ..
        } = event
        {
            host.keys.push(key);
        }
    }
}

impl Dispatch<wl_shell_surface::WlShellSurface, ()> for Host {
    fn event(
        host: &mut Self,
        surface: &wl_shell_surface::WlShellSurface,
        event: wl_shell_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_shell_surface::Event::Ping { serial } => surface.pong(serial),
            wl_shell_surface::Event::Configure { width, height, .. } => {
                host.notes.push(format!("configure {width}x{height}"));
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_webos_shell_surface::WlWebosShellSurface, ()> for Host {
    fn event(
        host: &mut Self,
        _: &wl_webos_shell_surface::WlWebosShellSurface,
        event: wl_webos_shell_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_webos_shell_surface::Event::StateChanged { state } => {
                host.notes.push(format!("state {state}"));
                host.minimized = state == STATE_MINIMIZED;
            }
            wl_webos_shell_surface::Event::Close => {
                host.notes.push("close".into());
                host.closed = true;
            }
            wl_webos_shell_surface::Event::StateAboutToChange { state } => {
                host.notes.push(format!("state-about-to-change {state}"));
            }
            wl_webos_shell_surface::Event::PositionChanged { x, y } => {
                host.notes.push(format!("position {x},{y}"));
            }
            wl_webos_shell_surface::Event::Exposed { .. } => host.notes.push("exposed".into()),
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, usize> for Host {
    fn event(
        host: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            if let Some(b) = host.busy.get_mut(*index) {
                *b = false;
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for Host {
    fn event(
        host: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            host.frame_done = true;
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Host {
    fn event(
        host: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Mode {
            flags: WEnum::Value(flags),
            width,
            height,
            ..
        } = event
        {
            if flags.contains(wl_output::Mode::Current) {
                host.output_mode = Some((width, height));
            }
        }
    }
}

delegate_noop!(Host: ignore wl_compositor::WlCompositor);
delegate_noop!(Host: ignore wl_surface::WlSurface);
delegate_noop!(Host: ignore wl_shm::WlShm);
delegate_noop!(Host: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Host: ignore wl_shell::WlShell);
delegate_noop!(Host: ignore wl_webos_shell::WlWebosShell);

/// Two shared-memory frames in one memfd.
struct Frames {
    map: *mut u8,
    frame_bytes: usize,
    buffers: Vec<wl_buffer::WlBuffer>,
}

impl Frames {
    fn new(
        shm: &wl_shm::WlShm,
        width: u32,
        height: u32,
        qh: &QueueHandle<Host>,
    ) -> Result<Frames, String> {
        let stride = width as usize * 4;
        let frame_bytes = stride * height as usize;
        let total = frame_bytes * 2;
        let fd = rustix::fs::memfd_create("viuda-frames", rustix::fs::MemfdFlags::CLOEXEC)
            .map_err(|e| format!("memfd: {e}"))?;
        rustix::fs::ftruncate(&fd, total as u64).map_err(|e| format!("ftruncate: {e}"))?;
        // SAFETY: a fresh shared mapping of a memfd this function owns. The
        // mapping lives as long as the process (Frames is never dropped
        // before exit), and only this thread writes it.
        let map = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut(),
                total,
                rustix::mm::ProtFlags::READ | rustix::mm::ProtFlags::WRITE,
                rustix::mm::MapFlags::SHARED,
                &fd,
                0,
            )
        }
        .map_err(|e| format!("mmap: {e}"))? as *mut u8;
        let pool = shm.create_pool(fd.as_fd(), total as i32, qh, ());
        let buffers = (0..2)
            .map(|i| {
                pool.create_buffer(
                    (i * frame_bytes) as i32,
                    width as i32,
                    height as i32,
                    stride as i32,
                    wl_shm::Format::Xbgr8888,
                    qh,
                    i,
                )
            })
            .collect();
        pool.destroy();
        Ok(Frames {
            map,
            frame_bytes,
            buffers,
        })
    }

    /// Copy the rectangles `rects` of `frame` into buffer `index`, row by
    /// row. Rectangles are clamped to the frame.
    fn copy_rects(&mut self, index: usize, frame: &crate::Pixmap, rects: &[(i32, i32, i32, i32)]) {
        let (w, h) = (frame.width() as i32, frame.height() as i32);
        let stride = w as usize * 4;
        let src = frame.data();
        if src.len() > self.frame_bytes {
            return;
        }
        // SAFETY: `index` is 0 or 1 and each frame is `frame_bytes` long
        // inside the mapping. The compositor has released this buffer, so
        // it is not reading it.
        let dst = unsafe {
            std::slice::from_raw_parts_mut(self.map.add(index * self.frame_bytes), self.frame_bytes)
        };
        for &(x0, y0, x1, y1) in rects {
            let (x0, y0, x1, y1) = (x0.clamp(0, w), y0.clamp(0, h), x1.clamp(0, w), y1.clamp(0, h));
            if x0 >= x1 {
                continue;
            }
            for y in y0..y1 {
                let a = y as usize * stride + x0 as usize * 4;
                let b = y as usize * stride + x1 as usize * 4;
                dst[a..b].copy_from_slice(&src[a..b]);
            }
        }
    }
}

/// How often the host logs its own CPU time and memory.
const STATUS_EVERY: Duration = Duration::from_secs(10);

/// This process's CPU time and memory, from `/proc/self`. The jail's
/// SSH user cannot see the app's process, so the app reports itself.
/// CPU ticks are converted at 100 Hz, the kernel's `USER_HZ` on this TV.
pub fn self_status(started: Instant) -> String {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    // Fields after the parenthesised command: state is field 3, utime 14,
    // stime 15.
    let rest = stat.rsplit_once(')').map_or("", |(_, r)| r);
    let f: Vec<&str> = rest.split_whitespace().collect();
    let tick = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) * 10;
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kb = |k: &str| {
        status
            .lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("?")
            .to_owned()
    };
    format!(
        "uptime_ms={} utime_ms={} stime_ms={} rss_kb={} hwm_kb={}",
        started.elapsed().as_millis(),
        tick(11),
        tick(12),
        kb("VmRSS:"),
        kb("VmHWM:")
    )
}

/// Order statistics of a sample, in microseconds.
pub fn summary(name: &str, sample: &[u128]) -> String {
    summary_in(name, "us", sample)
}

/// Order statistics of a sample in `unit`.
pub fn summary_in(name: &str, unit: &str, sample: &[u128]) -> String {
    if sample.is_empty() {
        return format!("{name} n=0");
    }
    let mut s = sample.to_vec();
    s.sort_unstable();
    let at = |q: f64| s[((s.len() - 1) as f64 * q).round() as usize];
    format!(
        "{name} n={} median_{unit}={} p95_{unit}={} max_{unit}={}",
        s.len(),
        at(0.5),
        at(0.95),
        s[s.len() - 1]
    )
}

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp/xdg".into());
    let name = std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
    PathBuf::from(dir).join(name)
}

/// Run the app until the compositor closes it. Every event and timing goes
/// to `log`.
pub fn run(mut viuda: Viuda, opts: Options, log: &mut dyn Write) -> Result<(), String> {
    let path = socket_path();
    let stream =
        UnixStream::connect(&path).map_err(|e| format!("GY-VIUDA-WAYLAND: {}: {e}", path.display()))?;
    let conn = Connection::from_socket(stream).map_err(|e| format!("GY-VIUDA-WAYLAND: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut host = Host {
        frame_done: true,
        busy: vec![false, false],
        ..Host::default()
    };
    queue.roundtrip(&mut host).map_err(|e| format!("GY-VIUDA-WAYLAND: {e}"))?;
    queue.roundtrip(&mut host).map_err(|e| format!("GY-VIUDA-WAYLAND: {e}"))?;
    let missing = |what: &str| format!("GY-VIUDA-WAYLAND: the compositor has no {what}");
    let compositor = host.compositor.clone().ok_or_else(|| missing("wl_compositor"))?;
    let shm = host.shm.clone().ok_or_else(|| missing("wl_shm"))?;
    let shell = host.shell.clone().ok_or_else(|| missing("wl_shell"))?;
    let webos = host.webos.clone().ok_or_else(|| missing("wl_webos_shell"))?;
    let _ = writeln!(
        log,
        "wayland seats={} keyboards={} output={:?}",
        host.seats.len(),
        host.keyboards.len(),
        host.output_mode
    );

    let surface = compositor.create_surface(&qh, ());
    let shell_surface = shell.get_shell_surface(&surface, &qh, ());
    shell_surface.set_toplevel();
    shell_surface.set_title(opts.title.clone());
    let webos_surface = webos.get_shell_surface(&surface, &qh, ());
    webos_surface.set_property("appId".into(), opts.app_id.clone());
    webos_surface.set_property("title".into(), opts.title.clone());
    webos_surface.set_state(STATE_FULLSCREEN);

    let mut frames = Frames::new(&shm, opts.width, opts.height, &qh)?;
    // The retained frame, repainted in place, and per buffer the damage it
    // has not received yet (each buffer is two frames behind at most).
    let mut retained = crate::Pixmap::new(1, 1).ok_or("GY-VIUDA-HOST: pixmap")?;
    let whole = (0, 0, opts.width as i32, opts.height as i32);
    let mut pending: [Vec<(i32, i32, i32, i32)>; 2] = [vec![whole], vec![whole]];
    let mut damaged_px: Vec<u128> = Vec::new();
    let mut renders: Vec<u128> = Vec::new();
    let mut dispatches: Vec<u128> = Vec::new();
    let mut presents: Vec<u128> = Vec::new();
    let mut dirty = true;
    let mut first = true;
    let mut next_ambient = Instant::now();
    let mut pending_input: Option<Instant> = None;
    let mut input_to_commit: Vec<u128> = Vec::new();
    let mut next_status = Instant::now() + STATUS_EVERY;

    loop {
        queue
            .dispatch_pending(&mut host)
            .map_err(|e| format!("GY-VIUDA-WAYLAND: {e}"))?;
        for note in host.notes.drain(..) {
            let _ = writeln!(log, "event {note}");
        }
        if host.closed {
            break;
        }
        let was_suspended = viuda.instance().phase() == goya_resolver::Phase::Suspended;
        if host.minimized && !was_suspended {
            let r = viuda.suspend();
            let _ = writeln!(log, "lifecycle suspend {r:?}");
        } else if !host.minimized && was_suspended {
            let r = viuda.resume();
            let _ = writeln!(log, "lifecycle resume {r:?}");
            dirty = true;
        }
        for code in std::mem::take(&mut host.keys) {
            let received = Instant::now();
            match map_key(code) {
                Some(key) if !host.minimized => {
                    let out = viuda.key(key);
                    if out.action.is_some() {
                        dispatches.push(out.dispatch.as_micros());
                    }
                    let _ = writeln!(
                        log,
                        "key {code} {key:?} action={} revision={} dispatch_us={}",
                        out.action.as_deref().unwrap_or("-"),
                        out.revision,
                        out.dispatch.as_micros()
                    );
                    if let Some(a) = out.announcement {
                        let _ = writeln!(log, "announce {a}");
                    }
                    pending_input.get_or_insert(received);
                    dirty = true;
                }
                Some(_) => {}
                None => {
                    let _ = writeln!(log, "key {code} unmapped");
                }
            }
        }
        for e in viuda.errors.drain(..) {
            let _ = writeln!(log, "error {e}");
        }

        let animating = viuda.tick();
        let ambient = viuda.ambient_interval().map(|i| i.max(opts.ambient_interval));
        let ambient_due = ambient.is_some() && Instant::now() >= next_ambient;
        if (dirty || animating || ambient_due) && host.frame_done && !host.minimized {
            if let Some(index) = host.busy.iter().position(|b| !b) {
                let t = Instant::now();
                let damage = viuda.render_into(&mut retained, ambient_due);
                renders.push(t.elapsed().as_micros());
                dirty = false;
                if ambient_due {
                    next_ambient = Instant::now() + ambient.unwrap_or(opts.ambient_interval);
                }
                if damage.is_empty() {
                    pending_input = None;
                } else {
                    damaged_px.push(damage.iter().map(|r| ((r.2 - r.0) * (r.3 - r.1)) as u128).sum());
                    let t = Instant::now();
                    for p in pending.iter_mut() {
                        p.extend_from_slice(&damage);
                    }
                    let rects = std::mem::take(&mut pending[index]);
                    frames.copy_rects(index, &retained, &rects);
                    surface.attach(Some(&frames.buffers[index]), 0, 0);
                    for r in &damage {
                        surface.damage(r.0, r.1, r.2 - r.0, r.3 - r.1);
                    }
                    surface.frame(&qh, ());
                    surface.commit();
                    presents.push(t.elapsed().as_micros());
                    host.busy[index] = true;
                    host.frame_done = false;
                    if let Some(at) = pending_input.take() {
                        input_to_commit.push(at.elapsed().as_micros());
                    }
                }
                if first {
                    first = false;
                    let _ = writeln!(
                        log,
                        "first-frame committed_ms={}",
                        opts.started.elapsed().as_millis()
                    );
                }
                if renders.len().is_multiple_of(100) {
                    let _ = writeln!(log, "{}", summary("render", &renders));
                    let _ = writeln!(log, "{}", summary_in("damaged", "px", &damaged_px));
                }
            }
        }
        if Instant::now() >= next_status {
            let _ = writeln!(log, "status {}", self_status(opts.started));
            next_status = Instant::now() + STATUS_EVERY;
        }
        let _ = log.flush();
        queue.flush().map_err(|e| format!("GY-VIUDA-WAYLAND: {e}"))?;

        let until_status = next_status.saturating_duration_since(Instant::now());
        let timeout: i32 = if dirty || animating {
            16
        } else if ambient.is_some() && !host.minimized {
            next_ambient
                .saturating_duration_since(Instant::now())
                .min(until_status)
                .as_millis() as i32
        } else {
            until_status.as_millis() as i32
        };
        if let Some(guard) = queue.prepare_read() {
            let ready = {
                let fd = guard.connection_fd();
                let mut fds = [PollFd::new(&fd, PollFlags::IN)];
                poll(&mut fds, timeout).unwrap_or(0)
            };
            if ready > 0 {
                if let Err(e) = guard.read() {
                    if !matches!(e, wayland_client::backend::WaylandError::Io(ref io) if io.kind() == std::io::ErrorKind::WouldBlock)
                    {
                        return Err(format!("GY-VIUDA-WAYLAND: {e}"));
                    }
                }
            }
        }
    }

    let _ = writeln!(log, "status {}", self_status(opts.started));
    let r = viuda.terminate();
    let _ = writeln!(log, "lifecycle terminate {r:?}");
    let _ = writeln!(log, "{}", summary("render", &renders));
    let _ = writeln!(log, "{}", summary_in("damaged", "px", &damaged_px));
    let _ = writeln!(log, "{}", summary("present", &presents));
    let _ = writeln!(log, "{}", summary("dispatch", &dispatches));
    let _ = writeln!(log, "{}", summary("input-to-commit", &input_to_commit));
    let _ = log.flush();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_keys_map() {
        assert_eq!(map_key(103), Some(Key::Up));
        assert_eq!(map_key(28), Some(Key::Ok));
        assert_eq!(map_key(2), Some(Key::Digit(1)));
        assert_eq!(map_key(11), Some(Key::Digit(0)));
        assert_eq!(map_key(0x205), Some(Key::Digit(5)));
        assert_eq!(map_key(158), None);
    }

    #[test]
    fn summary_reports_order_statistics() {
        let s = summary("x", &[5, 1, 3, 2, 4]);
        assert_eq!(s, "x n=5 median_us=3 p95_us=5 max_us=5");
        assert_eq!(summary("x", &[]), "x n=0");
    }
}
