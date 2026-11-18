use std::io::ErrorKind;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use rustix::event::{poll, PollFd, PollFlags};
use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, ImageFormat, ImageOrder, KeyPressEvent, PropMode, Property,
    PropertyNotifyEvent, SelectionNotifyEvent, SelectionRequestEvent, VisualClass, WindowClass,
    SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event as XEvent;
use x11rb::rust_connection::{ConnectError, ConnectionError, RustConnection};

use crate::error::Error;
use crate::frame::{self, PixelFormat};
use crate::keymap::{Keymap, Modifiers};
use crate::phase::{self, Phase, Step};
use crate::queue;
use crate::selection::{self, Answer, Atoms, Incoming, Outgoing, Pending, Progress};
use crate::{Capabilities, Diagnostics, Event, WindowConfig, WindowId};

const BACKGROUND: [u8; 3] = [235, 235, 240];

/// One X11 connection and at most one window.
///
/// The session is `Connected` when `connect` returns. `Connecting` is the edge
/// inside that call. `close_window` destroys the window, drops the socket, and
/// ends at `Terminated`. A second close is a no-op and does not send another
/// `DestroyWindow`.
pub struct Session {
    connection: Option<RustConnection>,
    phase: Phase,
    screen: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    root: u32,
    format: PixelFormat,
    capabilities: Capabilities,
    keymap: Keymap,
    window: Option<u32>,
    window_id: Option<WindowId>,
    gc: Option<u32>,
    width: u16,
    height: u16,
    wm_protocols: u32,
    wm_delete: u32,
    queue: Vec<Event>,
    held: [Option<u8>; 32],
    diagnostics: Diagnostics,
    atoms: Atoms,
    /// Latest server timestamp seen on input. 0 until the first event.
    last_time: u32,
    /// Text this client owns on the selection, and the ownership time.
    owned: Option<(String, u32)>,
    pending: Option<Pending>,
    /// INCR read in progress for `pending`.
    incoming: Option<Incoming>,
    /// INCR writes in progress to other clients.
    transfers: Vec<Outgoing>,
    /// Largest property written in one request. `None` derives it from the
    /// server's maximum request length. Tests lower it to force INCR.
    single_limit: Option<usize>,
}

impl Session {
    pub fn connect() -> Result<Self, Error> {
        Self::connect_with_selection(b"CLIPBOARD")
    }

    /// Connect and use `selection` in place of `CLIPBOARD`.
    ///
    /// Hosts call [`Session::connect`]. This exists so tests and interop
    /// harnesses can use a private selection name and never take the
    /// desktop clipboard. The name must be 1..=255 bytes without NUL.
    pub fn connect_with_selection(selection: &[u8]) -> Result<Self, Error> {
        if selection.is_empty() || selection.len() > 255 || selection.contains(&0) {
            return Err(Error::InvalidConfig("selection name"));
        }
        let (connection, screen) =
            RustConnection::connect(None).map_err(|err| Error::Connect(connect_message(&err)))?;
        let (root_id, format) = {
            let setup = connection.setup();
            let root = setup
                .roots
                .get(screen)
                .ok_or_else(|| Error::Connect("screen index is outside the setup".into()))?;
            (root.root, pixel_format(setup, root)?)
        };
        let capabilities = Capabilities {
            mit_shm: extension(&connection, b"MIT-SHM"),
            randr: extension(&connection, b"RANDR"),
            xfixes: extension(&connection, b"XFIXES"),
            present: extension(&connection, b"Present"),
            xinput: extension(&connection, b"XInputExtension"),
            render: extension(&connection, b"RENDER"),
            big_requests: extension(&connection, b"BIG-REQUESTS"),
            xkb: extension(&connection, b"XKEYBOARD"),
        };
        let keymap = load_keymap(&connection)?;
        let wm_protocols = intern(&connection, b"WM_PROTOCOLS")?;
        let wm_delete = intern(&connection, b"WM_DELETE_WINDOW")?;
        let atoms = Atoms {
            selection: intern(&connection, selection)?,
            targets: intern(&connection, b"TARGETS")?,
            utf8_string: intern(&connection, b"UTF8_STRING")?,
            text_plain_utf8: intern(&connection, b"text/plain;charset=utf-8")?,
            text_plain: intern(&connection, b"text/plain")?,
            incr: intern(&connection, b"INCR")?,
            property: intern(&connection, b"GOYA_KUMIMANU_SELECTION")?,
        };
        let phase = phase::advance(Phase::Connecting, Step::Connected)
            .map_err(|_| Error::Connect("phase machine rejected the connection edge".into()))?;
        Ok(Self {
            connection: Some(connection),
            phase,
            screen,
            root: root_id,
            format,
            capabilities,
            keymap,
            window: None,
            window_id: None,
            gc: None,
            width: 0,
            height: 0,
            wm_protocols,
            wm_delete,
            queue: Vec::new(),
            held: [None; 32],
            diagnostics: Diagnostics::default(),
            atoms,
            last_time: selection::CURRENT_TIME,
            owned: None,
            pending: None,
            incoming: None,
            transfers: Vec::new(),
            single_limit: None,
        })
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }

    pub fn size(&self) -> Option<(u16, u16)> {
        self.window_id.map(|_| (self.width, self.height))
    }

    pub fn create_window(&mut self, config: WindowConfig) -> Result<WindowId, Error> {
        validate_config(&config)?;
        if self.window_id.is_some() {
            return Err(Error::InvalidConfig("only one window is supported"));
        }
        self.phase = next(self.phase, Step::Create, "create a window")?;
        let created = self.create_x_window(&config);
        match created {
            Ok(id) => Ok(id),
            Err(err) => {
                self.fail();
                Err(err)
            }
        }
    }

    pub fn show_window(&mut self, id: WindowId) -> Result<(), Error> {
        self.require_window(id, "map a window")?;
        if !matches!(self.phase, Phase::WindowCreated | Phase::Mapped) {
            return Err(wrong(self.phase, "map a window"));
        }
        let connection = self.connection()?;
        let window = self.window.ok_or(Error::StaleWindow)?;
        connection.map_window(window).map_err(map_connection)?;
        connection.flush().map_err(map_connection)?;
        self.sync()?;
        self.phase = next(self.phase, Step::Map, "map a window")?;
        Ok(())
    }

    pub fn poll_events(&mut self, timeout: Duration) -> Result<Vec<Event>, Error> {
        if matches!(self.phase, Phase::Terminated | Phase::Failed) && self.connection.is_none() {
            return Ok(std::mem::take(&mut self.queue));
        }
        self.pump(Duration::ZERO)?;
        if self.queue.is_empty() {
            let timeout = match self.pending {
                Some(pending) => {
                    timeout.min(pending.deadline.saturating_duration_since(Instant::now()))
                }
                None => timeout,
            };
            self.wait(timeout)?;
            self.pump(Duration::ZERO)?;
        }
        let now = Instant::now();
        if self.pending.is_some_and(|pending| pending.expired(now)) {
            self.pending = None;
            self.incoming = None;
            queue::push(&mut self.queue, Event::ClipboardText(None))?;
        }
        let stale: Vec<u32> = self
            .transfers
            .iter()
            .filter(|transfer| transfer.expired(now))
            .map(|transfer| transfer.requestor)
            .collect();
        if !stale.is_empty() {
            self.transfers.retain(|transfer| !transfer.expired(now));
            for requestor in stale {
                self.release_requestor(requestor)?;
            }
        }
        Ok(std::mem::take(&mut self.queue))
    }

    /// Own the clipboard selection with `text`.
    ///
    /// Returns `Ok(true)` when the server reports this window as the owner.
    /// `Ok(false)` means another client kept it, or `text` is empty or
    /// larger than [`selection::MAX_CLIPBOARD_BYTES`]; nothing is owned then.
    /// The text stays in this process and is written only to a requestor's
    /// property. It is dropped on `SelectionClear` or close.
    pub fn set_clipboard_text(&mut self, id: WindowId, text: &str) -> Result<bool, Error> {
        self.require_window(id, "own the clipboard")?;
        if text.is_empty() || text.len() > selection::MAX_CLIPBOARD_BYTES {
            self.owned = None;
            return Ok(false);
        }
        let window = self.window.ok_or(Error::StaleWindow)?;
        let time = self.last_time;
        let connection = self.connection()?;
        connection
            .set_selection_owner(window, self.atoms.selection, time)
            .map_err(map_connection)?;
        let owner = connection
            .get_selection_owner(self.atoms.selection)
            .map_err(map_connection)?
            .reply()
            .map_err(map_reply)?
            .owner;
        if owner == window {
            self.owned = Some((text.to_owned(), time));
            Ok(true)
        } else {
            self.owned = None;
            Ok(false)
        }
    }

    /// Ask for the clipboard text. The answer arrives once as
    /// [`Event::ClipboardText`]: `Some` for valid text, `None` when there is
    /// no owner, the owner refused, the payload is empty, invalid, larger
    /// than 1 MiB, needs INCR, or [`selection::REQUEST_TIMEOUT`] passed.
    ///
    /// One request may be outstanding. A second call while one is pending
    /// is a no-op. When this window owns the selection the answer is the
    /// owned text, without a round trip through another client.
    pub fn request_clipboard_text(&mut self, id: WindowId) -> Result<(), Error> {
        self.require_window(id, "read the clipboard")?;
        if self.pending.is_some() {
            return Ok(());
        }
        let window = self.window.ok_or(Error::StaleWindow)?;
        let owner = self
            .connection()?
            .get_selection_owner(self.atoms.selection)
            .map_err(map_connection)?
            .reply()
            .map_err(map_reply)?
            .owner;
        if owner == x11rb::NONE {
            self.owned = None;
            return queue::push(&mut self.queue, Event::ClipboardText(None));
        }
        if owner == window {
            let text = self
                .owned
                .as_ref()
                .map(|(text, _)| text.clone())
                .filter(|text| !text.is_empty());
            return queue::push(&mut self.queue, Event::ClipboardText(text));
        }
        self.convert(self.atoms.utf8_string)
    }

    fn convert(&mut self, target: u32) -> Result<(), Error> {
        let window = self.window.ok_or(Error::StaleWindow)?;
        let connection = self.connection()?;
        connection
            .delete_property(window, self.atoms.property)
            .map_err(map_connection)?;
        connection
            .convert_selection(
                window,
                self.atoms.selection,
                target,
                self.atoms.property,
                self.last_time,
            )
            .map_err(map_connection)?;
        connection.flush().map_err(map_connection)?;
        self.pending = Some(Pending::new(target, Instant::now()));
        Ok(())
    }

    pub fn present_rgba(
        &mut self,
        id: WindowId,
        width: u16,
        height: u16,
        rgba: &[u8],
    ) -> Result<(), Error> {
        self.require_window(id, "present")?;
        if !matches!(self.phase, Phase::WindowCreated | Phase::Mapped) {
            return Err(wrong(self.phase, "present"));
        }
        if width != self.width || height != self.height {
            return Err(Error::SizeMismatch {
                window_width: self.width,
                window_height: self.height,
                frame_width: width,
                frame_height: height,
            });
        }
        let encoded = frame::encode(rgba, width, height, self.format, BACKGROUND)?;
        self.put_image(&encoded)?;
        Ok(())
    }

    pub fn close_window(&mut self, id: WindowId) -> Result<(), Error> {
        if self.phase == Phase::Terminated {
            return Ok(());
        }
        if self.window_id != Some(id) {
            return Err(Error::StaleWindow);
        }
        self.phase = next(self.phase, Step::Close, "close a window")?;
        self.owned = None;
        self.pending = None;
        self.incoming = None;
        self.transfers.clear();
        self.destroy_ids();
        self.connection = None;
        self.phase = next(self.phase, Step::Destroyed, "close a window")?;
        Ok(())
    }

    fn create_x_window(&mut self, config: &WindowConfig) -> Result<WindowId, Error> {
        let connection = self.connection()?;
        let window = connection.generate_id().map_err(map_id)?;
        let gc = connection.generate_id().map_err(map_id)?;
        let setup = connection.setup();
        let screen = &setup.roots[self.screen];
        let aux = CreateWindowAux::new()
            .background_pixel(screen.black_pixel)
            .border_pixel(screen.black_pixel)
            .colormap(screen.default_colormap)
            .override_redirect(u32::from(config.override_redirect))
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::STRUCTURE_NOTIFY
                    | EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::FOCUS_CHANGE
                    | EventMask::PROPERTY_CHANGE,
            );
        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                config.x,
                config.y,
                config.width,
                config.height,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &aux,
            )
            .map_err(map_connection)?;
        connection
            .create_gc(gc, window, &CreateGCAux::new())
            .map_err(map_connection)?;
        self.write_properties(window, config)?;
        connection.flush().map_err(map_connection)?;
        let id = WindowId(
            NonZeroU32::new(window)
                .ok_or_else(|| Error::Connect("server returned window 0".into()))?,
        );
        self.window = Some(window);
        self.window_id = Some(id);
        self.gc = Some(gc);
        self.width = config.width;
        self.height = config.height;
        Ok(id)
    }

    fn write_properties(&self, window: u32, config: &WindowConfig) -> Result<(), Error> {
        let connection = self.connection()?;
        let latin1 = if config.title.bytes().all(|byte| byte < 128) {
            config.title.as_bytes()
        } else {
            b"Goya"
        };
        connection
            .change_property(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                8,
                latin1.len() as u32,
                latin1,
            )
            .map_err(map_connection)?;
        let mut class = Vec::with_capacity(config.instance.len() + config.class.len() + 2);
        class.extend_from_slice(config.instance.as_bytes());
        class.push(0);
        class.extend_from_slice(config.class.as_bytes());
        class.push(0);
        connection
            .change_property(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                8,
                class.len() as u32,
                &class,
            )
            .map_err(map_connection)?;
        let hints = size_hints(config.width, config.height);
        let mut hint_bytes = Vec::with_capacity(hints.len() * 4);
        for value in hints {
            hint_bytes.extend_from_slice(&value.to_ne_bytes());
        }
        connection
            .change_property(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NORMAL_HINTS,
                AtomEnum::WM_SIZE_HINTS,
                32,
                hints.len() as u32,
                &hint_bytes,
            )
            .map_err(map_connection)?;
        let protocol = self.wm_delete.to_ne_bytes();
        connection
            .change_property(
                PropMode::REPLACE,
                window,
                self.wm_protocols,
                AtomEnum::ATOM,
                32,
                1,
                &protocol,
            )
            .map_err(map_connection)?;
        if let Ok(utf8) = intern(connection, b"UTF8_STRING") {
            if let Ok(net_name) = intern(connection, b"_NET_WM_NAME") {
                let bytes = config.title.as_bytes();
                let _ = connection.change_property(
                    PropMode::REPLACE,
                    window,
                    net_name,
                    utf8,
                    8,
                    bytes.len() as u32,
                    bytes,
                );
            }
        }
        Ok(())
    }

    fn put_image(&self, encoded: &frame::EncodedFrame) -> Result<(), Error> {
        let connection = self.connection()?;
        let window = self.window.ok_or(Error::StaleWindow)?;
        let gc = self.gc.ok_or(Error::StaleWindow)?;
        let max_bytes = usize::from(connection.setup().maximum_request_length).saturating_mul(4);
        let budget = max_bytes.saturating_sub(64).max(encoded.stride);
        let rows_per = u16::try_from((budget / encoded.stride).max(1)).unwrap_or(u16::MAX);
        let mut y = 0u16;
        while y < encoded.height {
            let rows = rows_per.min(encoded.height - y);
            let start = usize::from(y) * encoded.stride;
            let end = start + usize::from(rows) * encoded.stride;
            connection
                .put_image(
                    ImageFormat::Z_PIXMAP,
                    window,
                    gc,
                    encoded.width,
                    rows,
                    0,
                    i16::try_from(y).unwrap_or(i16::MAX),
                    0,
                    encoded.depth,
                    &encoded.bytes[start..end],
                )
                .map_err(map_connection)?
                .check()
                .map_err(map_reply)?;
            y = y.saturating_add(rows);
        }
        Ok(())
    }

    fn pump(&mut self, _timeout: Duration) -> Result<(), Error> {
        loop {
            let event = {
                let Some(connection) = self.connection.as_ref() else {
                    return Ok(());
                };
                match connection.poll_for_event() {
                    Ok(Some(event)) => event,
                    Ok(None) => return Ok(()),
                    Err(err) => return self.fail_connection(err),
                }
            };
            self.ingest(event)?;
        }
    }

    fn wait(&mut self, timeout: Duration) -> Result<(), Error> {
        let Some(connection) = self.connection.as_ref() else {
            return Ok(());
        };
        connection.flush().map_err(map_connection)?;
        let mut fds = [PollFd::new(connection.stream(), PollFlags::IN)];
        let timeout_ms = timeout_ms(timeout);
        loop {
            match poll(&mut fds, timeout_ms) {
                Ok(_) => return Ok(()),
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => return Err(Error::Io(err.to_string())),
            }
        }
    }

    fn ingest(&mut self, event: XEvent) -> Result<(), Error> {
        let translated = match event {
            XEvent::KeyPress(event) => {
                self.note_time(event.time);
                self.key_event(event, true)
            }
            XEvent::KeyRelease(event) => {
                self.note_time(event.time);
                self.key_event(event, false)
            }
            XEvent::ButtonPress(event) => {
                self.note_time(event.time);
                self.button_event(event.detail, event.event_x, event.event_y, true)
            }
            XEvent::ButtonRelease(event) => {
                self.note_time(event.time);
                self.button_event(event.detail, event.event_x, event.event_y, false)
            }
            XEvent::MotionNotify(event) => {
                self.note_time(event.time);
                Some(Event::PointerMove {
                    x: event.event_x,
                    y: event.event_y,
                })
            }
            XEvent::SelectionRequest(event) => {
                self.serve(event)?;
                None
            }
            XEvent::SelectionClear(event) => {
                if event.selection == self.atoms.selection && Some(event.owner) == self.window {
                    self.owned = None;
                }
                None
            }
            XEvent::SelectionNotify(event) => self.selection_notify(event)?,
            XEvent::PropertyNotify(event) => self.property_notify(event)?,
            XEvent::Error(_) => {
                self.note_protocol();
                None
            }
            XEvent::ConfigureNotify(event) => {
                self.configure_event(event.window, event.width, event.height)
            }
            XEvent::MapNotify(_) => {
                if self.phase == Phase::WindowCreated {
                    let _ =
                        next(self.phase, Step::Map, "map a window").map(|phase| self.phase = phase);
                }
                Some(Event::Mapped)
            }
            XEvent::Expose(_) => Some(Event::Exposed),
            XEvent::FocusIn(_) => Some(Event::Focus { focused: true }),
            XEvent::FocusOut(_) => Some(Event::Focus { focused: false }),
            XEvent::ClientMessage(event) => self.client_message(event),
            XEvent::MappingNotify(_) => {
                if let Err(err) = self.reload_keymap() {
                    self.note_protocol();
                    return Err(err);
                }
                None
            }
            XEvent::DestroyNotify(event) if Some(event.window) == self.window => {
                self.window = None;
                self.window_id = None;
                self.gc = None;
                self.phase = Phase::Terminated;
                self.connection = None;
                Some(Event::CloseRequested)
            }
            _ => {
                self.diagnostics.ignored_events = self.diagnostics.ignored_events.saturating_add(1);
                None
            }
        };
        if let Some(event) = translated {
            queue::push(&mut self.queue, event)?;
        }
        Ok(())
    }

    fn key_event(&mut self, event: KeyPressEvent, pressed: bool) -> Option<Event> {
        let modifiers = Modifiers::from_state(u16::from(event.state));
        let interpreted = self.keymap.interpret(event.detail, modifiers);
        let repeat = self.note_held(event.detail, pressed);
        Some(Event::Key {
            keycode: event.detail,
            keysym: interpreted.keysym,
            text: interpreted.text,
            named: interpreted.named,
            modifiers,
            pressed,
            repeat,
        })
    }

    fn button_event(&mut self, button: u8, x: i16, y: i16, pressed: bool) -> Option<Event> {
        let Some((vertical, horizontal)) = scroll_motion(button) else {
            return Some(Event::PointerButton {
                x,
                y,
                button,
                pressed,
            });
        };
        if !pressed {
            return None;
        }
        Some(Event::Scroll {
            x,
            y,
            vertical,
            horizontal,
        })
    }

    fn configure_event(&mut self, window: u32, width: u16, height: u16) -> Option<Event> {
        let (width, height) = resize_event(
            self.window,
            (self.width, self.height),
            window,
            width,
            height,
        )?;
        self.width = width;
        self.height = height;
        Some(Event::Resized { width, height })
    }

    fn client_message(&self, event: ClientMessageEvent) -> Option<Event> {
        is_wm_delete(
            self.wm_protocols,
            self.wm_delete,
            event.type_,
            event.format,
            event.data.as_data32()[0],
        )
        .then_some(Event::CloseRequested)
    }

    fn note_time(&mut self, time: u32) {
        if time != selection::CURRENT_TIME {
            self.last_time = time;
        }
    }

    /// Answer one `SelectionRequest`. Requests for another selection, or
    /// for a window this session does not own the selection with, are refused.
    fn serve(&mut self, request: SelectionRequestEvent) -> Result<(), Error> {
        let ours = request.selection == self.atoms.selection && Some(request.owner) == self.window;
        let (text, owned_at) = match (&self.owned, ours) {
            (Some((text, at)), true) => (Some(text.as_str()), *at),
            _ => (None, selection::CURRENT_TIME),
        };
        let connection = self.connection.as_ref().ok_or(Error::ServerDisconnect)?;
        // Obsolete requestors send property None and expect the target.
        let property = if request.property == x11rb::NONE {
            request.target
        } else {
            request.property
        };
        let single_limit = self.single_limit.unwrap_or_else(|| {
            connection
                .maximum_request_bytes()
                .saturating_sub(32)
                .min(selection::INCR_THRESHOLD)
        });
        let answer = selection::answer(
            &self.atoms,
            text,
            owned_at,
            request.time,
            request.target,
            single_limit,
        );
        let mut started: Option<Outgoing> = None;
        let written = match answer {
            Answer::Targets(list) => {
                let mut bytes = Vec::with_capacity(list.len() * 4);
                for atom in &list {
                    bytes.extend_from_slice(&atom.to_ne_bytes());
                }
                connection
                    .change_property(
                        PropMode::REPLACE,
                        request.requestor,
                        property,
                        AtomEnum::ATOM,
                        32,
                        list.len() as u32,
                        &bytes,
                    )
                    .map_err(map_connection)?;
                property
            }
            Answer::Text { type_, bytes } => {
                connection
                    .change_property(
                        PropMode::REPLACE,
                        request.requestor,
                        property,
                        type_,
                        8,
                        bytes.len() as u32,
                        bytes,
                    )
                    .map_err(map_connection)?;
                property
            }
            Answer::Incr { type_, bytes } => {
                let busy = self.transfers.len() >= selection::MAX_TRANSFERS
                    && !self.transfers.iter().any(|transfer| {
                        transfer.requestor == request.requestor && transfer.property == property
                    });
                // Never retarget this session's own window mask.
                if busy || Some(request.requestor) == self.window {
                    x11rb::NONE
                } else {
                    let chunk = single_limit.clamp(1, selection::INCR_CHUNK);
                    let transfer = Outgoing::new(
                        request.requestor,
                        property,
                        type_,
                        bytes,
                        chunk,
                        Instant::now(),
                    );
                    // Watch the requestor's deletes before writing the marker.
                    connection
                        .change_window_attributes(
                            request.requestor,
                            &ChangeWindowAttributesAux::new()
                                .event_mask(EventMask::PROPERTY_CHANGE),
                        )
                        .map_err(map_connection)?;
                    connection
                        .change_property(
                            PropMode::REPLACE,
                            request.requestor,
                            property,
                            self.atoms.incr,
                            32,
                            1,
                            &transfer.total().to_ne_bytes(),
                        )
                        .map_err(map_connection)?;
                    started = Some(transfer);
                    property
                }
            }
            Answer::Refuse => x11rb::NONE,
        };
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: written,
        };
        connection
            .send_event(false, request.requestor, EventMask::NO_EVENT, notify)
            .map_err(map_connection)?;
        connection.flush().map_err(map_connection)?;
        if let Some(transfer) = started {
            self.transfers.retain(|old| {
                old.requestor != transfer.requestor || old.property != transfer.property
            });
            self.transfers.push(transfer);
            self.diagnostics.incr_writes = self.diagnostics.incr_writes.saturating_add(1);
        }
        Ok(())
    }

    /// Route one `PropertyNotify`. A delete on a requestor's property asks
    /// for the next outgoing chunk. A new value on our own conversion
    /// property is the next incoming chunk.
    fn property_notify(&mut self, event: PropertyNotifyEvent) -> Result<Option<Event>, Error> {
        self.note_time(event.time);
        if event.state == Property::DELETE {
            self.advance_transfer(event.window, event.atom)?;
            return Ok(None);
        }
        if Some(event.window) != self.window
            || event.atom != self.atoms.property
            || self.incoming.is_none()
        {
            return Ok(None);
        }
        let reply = self
            .connection()?
            .get_property(
                true,
                event.window,
                event.atom,
                AtomEnum::ANY,
                0,
                selection::property_words(),
            )
            .map_err(map_connection)?
            .reply()
            .map_err(map_reply)?;
        if reply.bytes_after != 0 {
            // The delete above did not happen. Do it so the owner continues.
            self.connection()?
                .delete_property(event.window, event.atom)
                .map_err(map_connection)?;
        }
        let Some(incoming) = self.incoming.as_mut() else {
            return Ok(None);
        };
        match incoming.push(
            &self.atoms,
            reply.type_,
            reply.format,
            reply.bytes_after,
            reply.value,
        ) {
            Progress::More => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.touch(Instant::now());
                }
                Ok(None)
            }
            Progress::Done(text) => {
                self.incoming = None;
                self.pending = None;
                Ok(Some(Event::ClipboardText(text)))
            }
        }
    }

    /// Write the next chunk of the transfer to `requestor`'s `property`.
    fn advance_transfer(&mut self, requestor: u32, property: u32) -> Result<(), Error> {
        let Some(index) = self
            .transfers
            .iter()
            .position(|transfer| transfer.requestor == requestor && transfer.property == property)
        else {
            return Ok(());
        };
        let connection = self.connection.as_ref().ok_or(Error::ServerDisconnect)?;
        let transfer = &mut self.transfers[index];
        let type_ = transfer.type_;
        if let Some(chunk) = transfer.next_chunk(Instant::now()) {
            connection
                .change_property(
                    PropMode::REPLACE,
                    requestor,
                    property,
                    type_,
                    8,
                    chunk.len() as u32,
                    chunk,
                )
                .map_err(map_connection)?;
        }
        if self.transfers[index].ended() {
            self.transfers.remove(index);
            self.release_requestor(requestor)?;
        }
        self.connection()?.flush().map_err(map_connection)?;
        Ok(())
    }

    /// Stop watching `requestor` once no transfer to it remains.
    fn release_requestor(&mut self, requestor: u32) -> Result<(), Error> {
        if self
            .transfers
            .iter()
            .any(|transfer| transfer.requestor == requestor)
            || Some(requestor) == self.window
        {
            return Ok(());
        }
        // A requestor that already went away answers BadWindow, which is
        // counted as a protocol error and does not fail the session.
        self.connection()?
            .change_window_attributes(
                requestor,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
            )
            .map_err(map_connection)?;
        Ok(())
    }

    /// Finish the outstanding conversion. A notify for another selection,
    /// another target, or with no request pending is ignored.
    fn selection_notify(&mut self, event: SelectionNotifyEvent) -> Result<Option<Event>, Error> {
        let Some(pending) = self.pending else {
            return Ok(None);
        };
        if event.selection != self.atoms.selection
            || Some(event.requestor) != self.window
            || event.target != pending.target
        {
            return Ok(None);
        }
        self.pending = None;
        if event.property == x11rb::NONE {
            if let Some(next) = selection::fallback(&self.atoms, pending.target) {
                self.convert(next)?;
                return Ok(None);
            }
            return Ok(Some(Event::ClipboardText(None)));
        }
        let window = self.window.ok_or(Error::StaleWindow)?;
        let reply = self
            .connection()?
            .get_property(
                true,
                window,
                event.property,
                AtomEnum::ANY,
                0,
                selection::property_words(),
            )
            .map_err(map_connection)?
            .reply()
            .map_err(map_reply)?;
        if reply.bytes_after != 0 {
            // Leave nothing behind on our window when the reply was partial.
            let _ = self.connection()?.delete_property(window, event.property);
        }
        if reply.type_ == self.atoms.incr && reply.bytes_after == 0 {
            // The delete above asked the owner for the first chunk.
            let mut pending = pending;
            pending.touch(Instant::now());
            self.pending = Some(pending);
            self.incoming = Some(Incoming::default());
            self.diagnostics.incr_reads = self.diagnostics.incr_reads.saturating_add(1);
            self.connection()?.flush().map_err(map_connection)?;
            return Ok(None);
        }
        let text = selection::decode(
            &self.atoms,
            reply.type_,
            reply.format,
            reply.bytes_after,
            reply.value,
        );
        Ok(Some(Event::ClipboardText(text)))
    }

    fn note_held(&mut self, keycode: u8, pressed: bool) -> bool {
        note_held(&mut self.held, keycode, pressed)
    }

    fn reload_keymap(&mut self) -> Result<(), Error> {
        let connection = self.connection()?;
        self.keymap = load_keymap(connection)?;
        Ok(())
    }

    fn sync(&self) -> Result<(), Error> {
        self.connection()?
            .get_input_focus()
            .map_err(map_connection)?
            .reply()
            .map_err(map_reply)?;
        Ok(())
    }

    fn require_window(&self, id: WindowId, attempted: &'static str) -> Result<(), Error> {
        if matches!(self.phase, Phase::Terminated | Phase::Failed) {
            return Err(wrong(self.phase, attempted));
        }
        if self.window_id != Some(id) {
            return Err(Error::StaleWindow);
        }
        Ok(())
    }

    fn connection(&self) -> Result<&RustConnection, Error> {
        self.connection.as_ref().ok_or(Error::ServerDisconnect)
    }

    fn destroy_ids(&mut self) {
        if let Some(connection) = self.connection.as_ref() {
            if let Some(gc) = self.gc.take() {
                let _ = connection.free_gc(gc);
            }
            if let Some(window) = self.window.take() {
                let _ = connection.destroy_window(window);
            }
            let _ = connection.flush();
        }
        self.gc = None;
        self.window = None;
        self.window_id = None;
    }

    fn fail(&mut self) {
        self.destroy_ids();
        self.connection = None;
        self.phase = phase::advance(self.phase, Step::Fault).unwrap_or(Phase::Failed);
    }

    fn fail_connection(&mut self, err: ConnectionError) -> Result<(), Error> {
        let mapped = map_connection(err);
        self.fail();
        Err(mapped)
    }

    fn note_protocol(&mut self) {
        self.diagnostics.protocol_errors = self.diagnostics.protocol_errors.saturating_add(1);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.phase == Phase::Terminated && self.window.is_none() {
            return;
        }
        self.destroy_ids();
        self.connection = None;
        self.phase = Phase::Terminated;
    }
}

pub(crate) fn validate_config(config: &WindowConfig) -> Result<(), Error> {
    check_label(&config.title, 255, "title")?;
    check_label(&config.class, 127, "class")?;
    check_label(&config.instance, 127, "instance")?;
    if config.width == 0 || config.height == 0 || config.width > 4096 || config.height > 4096 {
        return Err(Error::InvalidConfig("size must be 1..=4096"));
    }
    Ok(())
}

fn check_label(text: &str, max: usize, field: &'static str) -> Result<(), Error> {
    if text.is_empty() || text.len() > max || text.bytes().any(|byte| byte == 0) {
        return Err(Error::InvalidConfig(field));
    }
    Ok(())
}

/// Scroll delta for an X11 wheel button, or `None` for a plain button.
///
/// The core protocol reports wheels 4/5 as vertical up/down and wheels 6/7
/// as horizontal left/right button presses. Any other detail is a real
/// button and translates to nothing here.
fn scroll_motion(button: u8) -> Option<(i32, i32)> {
    match button {
        4 => Some((1, 0)),
        5 => Some((-1, 0)),
        6 => Some((0, -1)),
        7 => Some((0, 1)),
        _ => None,
    }
}

/// Filter one `ConfigureNotify` into a resize, if it is one.
///
/// Events for another window, and reports of a zero or unchanged size, are
/// window-manager noise and translate to nothing.
fn resize_event(
    own: Option<u32>,
    current: (u16, u16),
    window: u32,
    width: u16,
    height: u16,
) -> Option<(u16, u16)> {
    if Some(window) != own || width == 0 || height == 0 {
        return None;
    }
    if (width, height) == current {
        return None;
    }
    Some((width, height))
}

/// Recognize the `WM_PROTOCOLS` `WM_DELETE_WINDOW` close request.
///
/// Any other message type, format, or protocol atom is not a close.
fn is_wm_delete(wm_protocols: u32, wm_delete: u32, type_: u32, format: u8, first: u32) -> bool {
    type_ == wm_protocols && format == 32 && first == wm_delete
}

/// Track autorepeat for one keycode and answer whether the press repeats.
///
/// A press for a key already held is a repeat; a release frees its slot.
/// The table holds 32 keys. A 33rd simultaneously held key finds no slot
/// and only loses repeat detection for itself.
fn note_held(held: &mut [Option<u8>; 32], keycode: u8, pressed: bool) -> bool {
    if pressed {
        let repeat = held.contains(&Some(keycode));
        if !repeat {
            if let Some(slot) = held.iter_mut().find(|slot| slot.is_none()) {
                *slot = Some(keycode);
            }
        }
        repeat
    } else {
        for slot in held.iter_mut() {
            if *slot == Some(keycode) {
                *slot = None;
            }
        }
        false
    }
}

fn size_hints(width: u16, height: u16) -> [u32; 18] {
    let flags = (1 << 3) | (1 << 4) | (1 << 9);
    [
        flags,
        0,
        0,
        u32::from(width),
        u32::from(height),
        1,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        1,
    ]
}

fn next(phase: Phase, step: Step, attempted: &'static str) -> Result<Phase, Error> {
    phase::advance(phase, step).map_err(|current| wrong(current, attempted))
}

fn wrong(current: Phase, attempted: &'static str) -> Error {
    Error::WrongPhase { current, attempted }
}

fn pixel_format(
    setup: &x11rb::protocol::xproto::Setup,
    screen: &x11rb::protocol::xproto::Screen,
) -> Result<PixelFormat, Error> {
    let mut depth = None;
    let mut red_mask = 0;
    let mut green_mask = 0;
    let mut blue_mask = 0;
    let mut class = None;
    for allowed in &screen.allowed_depths {
        for visual in &allowed.visuals {
            if visual.visual_id == screen.root_visual {
                depth = Some(allowed.depth);
                red_mask = visual.red_mask;
                green_mask = visual.green_mask;
                blue_mask = visual.blue_mask;
                class = Some(visual.class);
            }
        }
    }
    let depth = depth.unwrap_or(screen.root_depth);
    if !matches!(
        class,
        Some(VisualClass::TRUE_COLOR | VisualClass::DIRECT_COLOR)
    ) {
        return Err(Error::UnsupportedVisual {
            depth,
            bits_per_pixel: 0,
        });
    }
    let pixmap = setup
        .pixmap_formats
        .iter()
        .find(|format| format.depth == depth)
        .ok_or(Error::UnsupportedVisual {
            depth,
            bits_per_pixel: 0,
        })?;
    if !matches!(pixmap.bits_per_pixel, 16 | 32) {
        return Err(Error::UnsupportedVisual {
            depth,
            bits_per_pixel: pixmap.bits_per_pixel,
        });
    }
    Ok(PixelFormat {
        depth,
        bits_per_pixel: pixmap.bits_per_pixel,
        scanline_pad: pixmap.scanline_pad,
        red_mask,
        green_mask,
        blue_mask,
        byte_order_lsb: setup.image_byte_order == ImageOrder::LSB_FIRST,
    })
}

fn load_keymap(connection: &RustConnection) -> Result<Keymap, Error> {
    let setup = connection.setup();
    let min = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
    let reply = connection
        .get_keyboard_mapping(min, count)
        .map_err(map_connection)?
        .reply()
        .map_err(map_reply)?;
    Ok(Keymap {
        min_keycode: min,
        per_keycode: reply.keysyms_per_keycode,
        keysyms: reply.keysyms,
    })
}

fn intern(connection: &RustConnection, name: &[u8]) -> Result<u32, Error> {
    let reply = connection
        .intern_atom(false, name)
        .map_err(map_connection)?
        .reply()
        .map_err(map_reply)?;
    if reply.atom == 0 {
        return Err(Error::Connect("server returned atom 0".into()));
    }
    Ok(reply.atom)
}

fn extension(connection: &RustConnection, name: &[u8]) -> bool {
    connection
        .query_extension(name)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| reply.present)
}

fn connect_message(err: &ConnectError) -> String {
    err.to_string()
}

fn map_connection(err: ConnectionError) -> Error {
    match err {
        ConnectionError::IoError(err)
            if matches!(
                err.kind(),
                ErrorKind::UnexpectedEof
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::BrokenPipe
            ) =>
        {
            Error::ServerDisconnect
        }
        ConnectionError::IoError(err) => Error::Io(err.to_string()),
        ConnectionError::ParseError(_) => Error::Protocol {
            code: 0,
            sequence: 0,
        },
        other => Error::Io(other.to_string()),
    }
}

fn map_reply(err: x11rb::rust_connection::ReplyError) -> Error {
    match err {
        x11rb::rust_connection::ReplyError::ConnectionError(err) => map_connection(err),
        x11rb::rust_connection::ReplyError::X11Error(err) => Error::Protocol {
            code: err.error_code,
            sequence: err.sequence,
        },
    }
}

fn map_id(err: x11rb::rust_connection::ReplyOrIdError) -> Error {
    match err {
        x11rb::rust_connection::ReplyOrIdError::IdsExhausted => {
            Error::Connect("X11 ids exhausted".into())
        }
        x11rb::rust_connection::ReplyOrIdError::ConnectionError(err) => map_connection(err),
        x11rb::rust_connection::ReplyOrIdError::X11Error(err) => Error::Protocol {
            code: err.error_code,
            sequence: err.sequence,
        },
    }
}

fn timeout_ms(timeout: Duration) -> i32 {
    let millis = timeout.as_millis();
    if millis > 60_000 {
        60_000
    } else {
        millis as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_CLIPBOARD_BYTES;
    use x11rb::protocol::xproto::{
        ClientMessageData, ConfigureWindowAux, CLIENT_MESSAGE_EVENT, KEY_PRESS_EVENT,
    };

    #[test]
    fn empty_title_is_rejected() {
        let mut config = WindowConfig::new("", 32, 32);
        config.title.clear();
        assert_eq!(validate_config(&config), Err(Error::InvalidConfig("title")));
    }

    #[test]
    fn zero_size_is_rejected() {
        let config = WindowConfig::new("goya", 0, 10);
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn empty_class_and_instance_are_rejected() {
        let mut config = WindowConfig::new("goya", 32, 32);
        config.instance.clear();
        assert_eq!(
            validate_config(&config),
            Err(Error::InvalidConfig("instance"))
        );
        config.instance = "goya".into();
        config.class.clear();
        assert_eq!(validate_config(&config), Err(Error::InvalidConfig("class")));
    }

    #[test]
    fn oversize_labels_are_rejected() {
        let long_title = "a".repeat(256);
        let config = WindowConfig::new(&long_title, 32, 32);
        assert_eq!(validate_config(&config), Err(Error::InvalidConfig("title")));
        let ok_title = "a".repeat(255);
        let mut config = WindowConfig::new(&ok_title, 32, 32);
        assert_eq!(validate_config(&config), Ok(()));
        let long_class = "c".repeat(128);
        config.class = long_class;
        assert_eq!(validate_config(&config), Err(Error::InvalidConfig("class")));
        let long_instance = "i".repeat(128);
        config.class = "c".repeat(127);
        config.instance = long_instance;
        assert_eq!(
            validate_config(&config),
            Err(Error::InvalidConfig("instance"))
        );
    }

    #[test]
    fn a_nul_in_a_label_is_rejected() {
        let mut config = WindowConfig::new("go\0ya", 32, 32);
        assert_eq!(validate_config(&config), Err(Error::InvalidConfig("title")));
        config.title = "goya".into();
        config.class = "c\0".into();
        assert_eq!(validate_config(&config), Err(Error::InvalidConfig("class")));
    }

    #[test]
    fn sizes_past_4096_are_rejected_and_the_bound_holds() {
        assert!(validate_config(&WindowConfig::new("goya", 4096, 4096)).is_ok());
        assert!(validate_config(&WindowConfig::new("goya", 4097, 1)).is_err());
        assert!(validate_config(&WindowConfig::new("goya", 1, 4097)).is_err());
    }

    #[test]
    fn wheel_buttons_map_to_scroll_and_releases_are_dropped() {
        let mut session = headless_session();
        let press = |session: &mut Session, button: u8| {
            session
                .button_event(button, 3, 4, true)
                .expect("some event")
        };
        assert_eq!(
            press(&mut session, 4),
            Event::Scroll {
                x: 3,
                y: 4,
                vertical: 1,
                horizontal: 0
            }
        );
        assert_eq!(
            press(&mut session, 5),
            Event::Scroll {
                x: 3,
                y: 4,
                vertical: -1,
                horizontal: 0
            }
        );
        assert_eq!(
            press(&mut session, 6),
            Event::Scroll {
                x: 3,
                y: 4,
                vertical: 0,
                horizontal: -1
            }
        );
        assert_eq!(
            press(&mut session, 7),
            Event::Scroll {
                x: 3,
                y: 4,
                vertical: 0,
                horizontal: 1
            }
        );
        for button in 4..=7 {
            assert_eq!(session.button_event(button, 0, 0, false), None);
        }
        assert_eq!(
            session.button_event(1, 5, 6, true),
            Some(Event::PointerButton {
                x: 5,
                y: 6,
                button: 1,
                pressed: true
            })
        );
    }

    #[test]
    fn configure_events_filter_to_real_resizes() {
        let window = 42;
        assert_eq!(
            resize_event(Some(window), (100, 60), window, 120, 80),
            Some((120, 80))
        );
        // Another window's configure is not our resize.
        assert_eq!(
            resize_event(Some(window), (100, 60), window + 1, 120, 80),
            None
        );
        // No window yet, and zero sizes.
        assert_eq!(resize_event(None, (0, 0), window, 120, 80), None);
        assert_eq!(resize_event(Some(window), (100, 60), window, 0, 60), None);
        assert_eq!(resize_event(Some(window), (100, 60), window, 100, 0), None);
        // An unchanged size is noise from the window manager.
        assert_eq!(resize_event(Some(window), (100, 60), window, 100, 60), None);
    }

    #[test]
    fn only_the_wm_delete_protocol_closes() {
        let protocols = 900;
        let delete = 901;
        assert!(is_wm_delete(protocols, delete, protocols, 32, delete));
        assert!(!is_wm_delete(protocols, delete, protocols, 32, delete + 1));
        assert!(!is_wm_delete(protocols, delete, protocols + 1, 32, delete));
        assert!(!is_wm_delete(protocols, delete, protocols, 8, delete));
    }

    #[test]
    fn key_presses_repeat_while_held_and_releases_reset() {
        let mut held = [None; 32];
        assert!(!note_held(&mut held, 38, true), "first press");
        assert!(note_held(&mut held, 38, true), "held key repeats");
        assert!(!note_held(&mut held, 39, true), "second key");
        assert!(note_held(&mut held, 39, true), "second key repeats");
        assert!(!note_held(&mut held, 38, false), "release");
        assert!(!note_held(&mut held, 38, true), "press after release");
        assert!(!note_held(&mut held, 39, false), "release");
        assert!(!note_held(&mut held, 40, true), "unknown release first");
    }

    #[test]
    fn a_release_for_an_unheld_key_is_accepted() {
        let mut held = [None; 32];
        assert!(!note_held(&mut held, 9, false));
        assert!(held.iter().all(|slot| slot.is_none()));
    }

    #[test]
    fn the_held_table_does_not_grow_past_32_keys() {
        let mut held = [None; 32];
        for keycode in 1..=32u8 {
            assert!(!note_held(&mut held, keycode, true));
        }
        assert!(note_held(&mut held, 7, true), "held key still repeats");
        assert!(!note_held(&mut held, 33, true), "no slot left");
        assert!(held
            .iter()
            .all(|slot| slot.is_none() || slot.unwrap() <= 32));
    }

    #[test]
    fn timeouts_are_clamped_into_the_poll_range() {
        assert_eq!(timeout_ms(Duration::ZERO), 0);
        assert_eq!(timeout_ms(Duration::from_millis(5)), 5);
        assert_eq!(timeout_ms(Duration::from_secs(60)), 60_000);
        assert_eq!(timeout_ms(Duration::from_secs(120)), 60_000);
        assert_eq!(timeout_ms(Duration::from_secs(u64::from(u32::MAX))), 60_000);
        assert!(timeout_ms(Duration::from_millis(1)) >= 0);
    }

    // ---- Stream 5: edge case and error handling robustness ----

    #[test]
    fn size_boundaries_at_one_and_4096_hold() {
        assert!(validate_config(&WindowConfig::new("goya", 1, 1)).is_ok());
        assert!(validate_config(&WindowConfig::new("goya", 1, 4096)).is_ok());
        assert!(validate_config(&WindowConfig::new("goya", 4096, 1)).is_ok());
        assert!(validate_config(&WindowConfig::new("goya", 4096, 4096)).is_ok());
    }

    #[test]
    fn labels_at_the_exact_byte_boundaries_are_accepted() {
        let mut config = WindowConfig::new("t", 64, 64);
        config.title = "t".repeat(255);
        assert_eq!(config.title.len(), 255);
        config.class = "c".repeat(127);
        config.instance = "i".repeat(127);
        assert_eq!(config.class.len(), 127);
        assert_eq!(config.instance.len(), 127);
        assert_eq!(validate_config(&config), Ok(()));
    }

    #[test]
    fn size_hints_advertise_resize_with_a_one_by_one_minimum() {
        let hints = size_hints(320, 240);
        assert_eq!(
            hints[0],
            (1 << 3) | (1 << 4) | (1 << 9),
            "PMinSize | PMaxSize | PBaseSize"
        );
        assert_eq!(hints[1], 0, "pad");
        assert_eq!(hints[2], 0, "pad");
        assert_eq!(hints[3], 320, "min width");
        assert_eq!(hints[4], 240, "min height");
        assert_eq!(hints[5], 1, "max width");
        assert_eq!(hints[6], 1, "max height");
        assert_eq!(hints[7], 0, "base width");
        assert_eq!(hints[8], 0, "base height");
        assert!(hints[9..17].iter().all(|value| *value == 0));
        assert_eq!(hints[17], 1, "win gravity");
        // The dimensions travel as u32 even at the u16 range edge.
        assert_eq!(size_hints(u16::MAX, 0)[3], u32::from(u16::MAX));
        assert_eq!(size_hints(0, u16::MAX)[4], u32::from(u16::MAX));
    }

    #[test]
    fn wrong_phase_errors_name_the_phase_and_the_action() {
        let err =
            next(Phase::Connected, Step::Map, "map a window").expect_err("Connected cannot map");
        assert_eq!(
            err,
            Error::WrongPhase {
                current: Phase::Connected,
                attempted: "map a window",
            }
        );
        assert_eq!(err.to_string(), "cannot map a window from Connected");
        let err = wrong(Phase::Failed, "present");
        assert_eq!(
            err,
            Error::WrongPhase {
                current: Phase::Failed,
                attempted: "present",
            }
        );
    }

    #[test]
    fn legal_phase_edges_advance_without_error() {
        assert_eq!(
            next(Phase::Connected, Step::Create, "create a window"),
            Ok(Phase::WindowCreated)
        );
        assert_eq!(
            next(Phase::WindowCreated, Step::Map, "map a window"),
            Ok(Phase::Mapped)
        );
        // Re-mapping a mapped window is a legal self-edge.
        assert_eq!(
            next(Phase::Mapped, Step::Map, "map a window"),
            Ok(Phase::Mapped)
        );
        assert_eq!(
            next(Phase::Closing, Step::Destroyed, "close a window"),
            Ok(Phase::Terminated)
        );
    }

    #[test]
    fn stale_calls_in_a_dead_phase_report_wrong_phase_not_stale_window() {
        let id = WindowId(NonZeroU32::new(42).expect("nonzero"));
        let mut session = headless_session();
        session.window_id = Some(id);
        session.phase = Phase::Terminated;
        assert_eq!(
            session.require_window(id, "own the clipboard"),
            Err(Error::WrongPhase {
                current: Phase::Terminated,
                attempted: "own the clipboard",
            })
        );
        session.phase = Phase::Failed;
        assert_eq!(
            session.require_window(id, "read the clipboard"),
            Err(Error::WrongPhase {
                current: Phase::Failed,
                attempted: "read the clipboard",
            })
        );
        // A live phase with a foreign id is StaleWindow.
        session.phase = Phase::Mapped;
        session.window_id = Some(WindowId(NonZeroU32::new(7).expect("nonzero")));
        assert_eq!(
            session.require_window(id, "present"),
            Err(Error::StaleWindow)
        );
    }

    /// An X11 error as it would arrive on the wire, for mapping tests.
    fn wire_error(code: u8, sequence: u16) -> x11rb::x11_utils::X11Error {
        x11rb::x11_utils::X11Error {
            error_kind: x11rb::protocol::ErrorKind::Window,
            error_code: code,
            sequence,
            bad_value: 42,
            minor_opcode: 0,
            major_opcode: 1,
            extension_name: None,
            request_name: None,
        }
    }

    #[test]
    fn dropped_sockets_map_to_server_disconnect() {
        let kinds = [
            ErrorKind::UnexpectedEof,
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted,
            ErrorKind::BrokenPipe,
        ];
        for kind in kinds {
            let err = std::io::Error::new(kind, "gone");
            assert_eq!(
                map_connection(ConnectionError::IoError(err)),
                Error::ServerDisconnect,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn other_io_errors_keep_their_message() {
        let err = std::io::Error::new(ErrorKind::PermissionDenied, "denied");
        let mapped = map_connection(ConnectionError::IoError(err));
        assert!(matches!(mapped, Error::Io(_)), "{mapped:?}");
        assert!(mapped.to_string().contains("denied"));
    }

    #[test]
    fn parse_errors_map_to_protocol_error_zero() {
        assert_eq!(
            map_connection(ConnectionError::ParseError(
                x11rb::errors::ParseError::InsufficientData
            )),
            Error::Protocol {
                code: 0,
                sequence: 0
            }
        );
    }

    #[test]
    fn closed_connection_errors_map_to_io_messages() {
        let mapped = map_connection(ConnectionError::UnsupportedExtension);
        assert!(matches!(mapped, Error::Io(_)), "{mapped:?}");
        assert_eq!(mapped.to_string(), "socket: Unsupported extension");
        for variant in [
            ConnectionError::UnknownError,
            ConnectionError::InsufficientMemory,
            ConnectionError::MaximumRequestLengthExceeded,
            ConnectionError::FdPassingFailed,
        ] {
            assert!(matches!(map_connection(variant), Error::Io(_)));
        }
    }

    #[test]
    fn replies_map_x11_errors_with_their_code_and_sequence() {
        assert_eq!(
            map_reply(x11rb::rust_connection::ReplyError::X11Error(wire_error(
                3, 17
            ))),
            Error::Protocol {
                code: 3,
                sequence: 17
            }
        );
        // A connection failure behind a reply maps like a connection failure.
        let eof = std::io::Error::new(ErrorKind::UnexpectedEof, "eof");
        assert_eq!(
            map_reply(x11rb::rust_connection::ReplyError::ConnectionError(
                ConnectionError::IoError(eof)
            )),
            Error::ServerDisconnect
        );
    }

    #[test]
    fn id_errors_map_ids_exhausted_and_delegate_the_rest() {
        assert_eq!(
            map_id(x11rb::rust_connection::ReplyOrIdError::IdsExhausted),
            Error::Connect("X11 ids exhausted".into())
        );
        assert_eq!(
            map_id(x11rb::rust_connection::ReplyOrIdError::X11Error(
                wire_error(2, 9)
            )),
            Error::Protocol {
                code: 2,
                sequence: 9
            }
        );
        let reset = std::io::Error::new(ErrorKind::ConnectionReset, "reset");
        assert_eq!(
            map_id(x11rb::rust_connection::ReplyOrIdError::ConnectionError(
                ConnectionError::IoError(reset)
            )),
            Error::ServerDisconnect
        );
    }

    #[test]
    fn a_reload_with_a_shifted_min_keycode_moves_the_table() {
        // MappingNotify swaps the whole table. A server whose keycodes start
        // at 8 maps 'a' at 8; one starting at 255 leaves 8 outside the table
        // and puts 'a' at 255.
        let before = Keymap {
            min_keycode: 8,
            per_keycode: 2,
            keysyms: vec![0x61, 0x41],
        };
        let plain = Modifiers::default();
        assert_eq!(before.interpret(8, plain).text, Some('a'));
        assert_eq!(before.interpret(255, plain).keysym, 0);
        let after = Keymap {
            min_keycode: 255,
            per_keycode: 2,
            keysyms: vec![0x61, 0x41],
        };
        assert_eq!(after.interpret(255, plain).text, Some('a'));
        assert_eq!(after.interpret(8, plain).keysym, 0);
    }

    #[test]
    fn a_reload_that_changes_per_keycode_changes_the_columns() {
        let plain = Modifiers::default();
        let shifted = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        // Two columns per keycode: 8 is 'a'/'A', 9 is 'b'/'B'.
        let wide = Keymap {
            min_keycode: 8,
            per_keycode: 2,
            keysyms: vec![0x61, 0x41, 0x62, 0x42],
        };
        assert_eq!(wide.interpret(9, shifted).text, Some('B'));
        // The same flat list read one column per keycode: 8 is 'a' and
        // 9 is 0x41 ('A'), the old second column.
        let narrow = Keymap {
            min_keycode: 8,
            per_keycode: 1,
            keysyms: vec![0x61, 0x41, 0x62, 0x42],
        };
        assert_eq!(narrow.interpret(9, plain).text, Some('A'));
        assert_eq!(narrow.interpret(9, shifted).text, Some('A'));
    }

    #[test]
    fn key_events_use_the_current_table_after_a_reload() {
        let mut session = headless_session();
        session.keymap = Keymap {
            min_keycode: 8,
            per_keycode: 2,
            keysyms: vec![0x61, 0x41],
        };
        let press_text = |session: &mut Session, keycode: u8| {
            let event = session
                .key_event(
                    KeyPressEvent {
                        response_type: KEY_PRESS_EVENT,
                        detail: keycode,
                        sequence: 0,
                        time: 0,
                        root: 0,
                        event: 42,
                        child: 0,
                        root_x: 0,
                        root_y: 0,
                        event_x: 0,
                        event_y: 0,
                        state: x11rb::protocol::xproto::KeyButMask::from(0u16),
                        same_screen: true,
                    },
                    true,
                )
                .expect("some event");
            match event {
                Event::Key { text, .. } => text,
                other => panic!("not a key event: {other:?}"),
            }
        };
        assert_eq!(press_text(&mut session, 8), Some('a'));
        // The reload replaces the table wholesale; old code points go stale.
        session.keymap = Keymap {
            min_keycode: 20,
            per_keycode: 1,
            keysyms: vec![0x62],
        };
        assert_eq!(press_text(&mut session, 8), None);
        assert_eq!(press_text(&mut session, 20), Some('b'));
    }

    #[test]
    fn a_delete_for_a_foreign_transfer_does_nothing_headlessly() {
        let mut session = headless_session();
        session.transfers.push(Outgoing::new(
            77,
            8,
            session.atoms.utf8_string,
            b"payload",
            4,
            Instant::now(),
        ));
        // A delete on a window with no transfer is silently ignored.
        session.advance_transfer(88, 8).expect("no match, no error");
        assert_eq!(session.transfers.len(), 1);
        // A delete on the matching window needs the connection to write the
        // next chunk, so without one the session reports the disconnect.
        assert_eq!(
            session.advance_transfer(77, 8),
            Err(Error::ServerDisconnect)
        );
    }

    #[test]
    fn requestor_release_skips_live_transfers_and_own_window() {
        let mut session = headless_session();
        session.window = Some(42);
        session.transfers.push(Outgoing::new(
            77,
            8,
            session.atoms.utf8_string,
            b"payload",
            4,
            Instant::now(),
        ));
        // The session's own window is never unwatched.
        session.release_requestor(42).expect("own window");
        // Another live transfer to this requestor keeps the watch on.
        session.release_requestor(77).expect("transfer still live");
        // With nothing left the release needs the connection, and the
        // headless shell reports the disconnect instead.
        session.transfers.clear();
        assert_eq!(session.release_requestor(77), Err(Error::ServerDisconnect));
    }

    #[test]
    fn expired_transfers_are_retired_even_when_the_release_fails() {
        let mut session = headless_session();
        session.transfers.push(Outgoing::new(
            77,
            8,
            session.atoms.utf8_string,
            b"payload",
            4,
            Instant::now(),
        ));
        session.transfers.push(Outgoing::new(
            78,
            9,
            session.atoms.utf8_string,
            b"more",
            4,
            Instant::now(),
        ));
        // Retire only the first transfer.
        session.transfers[0].deadline = Instant::now() - Duration::from_secs(1);
        // The release of 77 hits the missing connection, but the transfer is
        // already gone and the live one is untouched.
        assert_eq!(
            session.poll_events(Duration::ZERO),
            Err(Error::ServerDisconnect)
        );
        assert_eq!(session.transfers.len(), 1);
        assert_eq!(session.transfers[0].requestor, 78);
    }

    /// A session shell without a connection, for methods that are pure.
    ///
    /// `connect` needs DISPLAY, so tests that only exercise translated
    /// events build the state by hand instead.
    fn headless_session() -> Session {
        Session {
            connection: None,
            phase: Phase::Mapped,
            screen: 0,
            root: 0,
            format: crate::frame::PixelFormat {
                depth: 24,
                bits_per_pixel: 32,
                scanline_pad: 32,
                red_mask: 0x00ff_0000,
                green_mask: 0x0000_ff00,
                blue_mask: 0x0000_00ff,
                byte_order_lsb: true,
            },
            capabilities: crate::Capabilities {
                mit_shm: false,
                randr: false,
                xfixes: false,
                present: false,
                xinput: false,
                render: false,
                big_requests: false,
                xkb: false,
            },
            keymap: Keymap {
                min_keycode: 8,
                per_keycode: 1,
                keysyms: Vec::new(),
            },
            window: Some(42),
            window_id: None,
            gc: None,
            width: 100,
            height: 60,
            wm_protocols: 900,
            wm_delete: 901,
            queue: Vec::new(),
            held: [None; 32],
            diagnostics: crate::Diagnostics::default(),
            atoms: crate::selection::Atoms {
                selection: 1,
                targets: 2,
                utf8_string: 3,
                text_plain_utf8: 4,
                text_plain: 5,
                incr: 6,
                property: 7,
            },
            last_time: 0,
            owned: None,
            pending: None,
            incoming: None,
            transfers: Vec::new(),
            single_limit: None,
        }
    }

    fn display_ready() -> bool {
        std::env::var("DISPLAY")
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    }

    fn offscreen(title: &str, width: u16, height: u16) -> WindowConfig {
        let mut config = WindowConfig::new(title, width, height);
        config.override_redirect = true;
        config.x = -80;
        config.y = -80;
        config
    }

    #[test]
    #[ignore = "opens an X11 window; run with --ignored when DISPLAY is set"]
    fn one_hundred_lifecycles_do_not_leak_fds() {
        assert!(display_ready(), "DISPLAY is unset");
        let before = fd_count();
        for index in 0..100 {
            let mut session = Session::connect().expect("connect");
            assert_eq!(session.phase(), Phase::Connected);
            let id = session
                .create_window(offscreen(&format!("kumimanu-{index}"), 16, 16))
                .expect("create");
            session.show_window(id).expect("map");
            let rgba = [20u8, 40, 80, 255].repeat(16 * 16);
            session.present_rgba(id, 16, 16, &rgba).expect("present");
            let _ = session.poll_events(Duration::from_millis(1));
            session.close_window(id).expect("close");
            session.close_window(id).expect("second close");
            assert_eq!(session.phase(), Phase::Terminated);
        }
        let after = fd_count();
        assert!(
            after <= before + 8,
            "fd count grew from {before} to {after}"
        );
    }

    #[test]
    #[ignore = "opens an X11 window; run with --ignored when DISPLAY is set"]
    fn sent_input_resize_and_delete_are_translated() {
        assert!(display_ready(), "DISPLAY is unset");
        let mut session = Session::connect().expect("connect");
        let id = session
            .create_window(offscreen("kumimanu-input", 64, 48))
            .expect("create");
        session.show_window(id).expect("map");
        let rgba = [255u8, 0, 0, 255].repeat(64 * 48);
        session.present_rgba(id, 64, 48, &rgba).expect("present");
        let keycode = session
            .keymap
            .keysyms
            .chunks(usize::from(session.keymap.per_keycode.max(1)))
            .position(|syms| syms.contains(&0x61))
            .map(|index| session.keymap.min_keycode + index as u8)
            .expect("keycode for a");
        send_key(&session, keycode);
        send_button(&session, 1, 8, 9);
        session
            .connection
            .as_ref()
            .unwrap()
            .configure_window(
                session.window.unwrap(),
                &ConfigureWindowAux::new().width(80).height(40),
            )
            .unwrap();
        send_delete(&session);
        session.connection.as_ref().unwrap().flush().unwrap();
        let mut saw_key = false;
        let mut saw_button = false;
        let mut saw_resize = false;
        let mut saw_close = false;
        for _ in 0..40 {
            for event in session
                .poll_events(Duration::from_millis(50))
                .expect("poll")
            {
                match event {
                    Event::Key {
                        text: Some('a'),
                        pressed: true,
                        ..
                    } => saw_key = true,
                    Event::PointerButton {
                        x,
                        y,
                        button: 1,
                        pressed: true,
                    } => {
                        assert_eq!((x, y), (8, 9));
                        saw_button = true;
                    }
                    Event::Resized { width, height } => {
                        assert_eq!((width, height), (80, 40));
                        saw_resize = true;
                    }
                    Event::CloseRequested => saw_close = true,
                    _ => {}
                }
            }
            if saw_key && saw_button && saw_resize && saw_close {
                break;
            }
        }
        assert!(saw_key, "missing key");
        assert!(saw_button, "missing button");
        assert!(saw_resize, "missing resize");
        assert!(saw_close, "missing delete");
        session.close_window(id).unwrap();
    }

    /// A selection name no desktop client owns, so tests never take the
    /// user's clipboard.
    fn private_selection(tag: &str) -> Vec<u8> {
        format!("GOYA_KUMIMANU_TEST_{tag}_{}", std::process::id()).into_bytes()
    }

    fn open(selection: &[u8], title: &str) -> (Session, WindowId) {
        let mut session = Session::connect_with_selection(selection).expect("connect");
        let id = session
            .create_window(offscreen(title, 8, 8))
            .expect("create");
        session.show_window(id).expect("map");
        (session, id)
    }

    /// Stand in for a user event: a zero-length append on our own window
    /// makes the server stamp a `PropertyNotify` with the current time.
    fn fresh_time(session: &mut Session) {
        let before = session.last_time;
        // Server time is in milliseconds, so the fresh stamp may equal the
        // old one. Clear it and wait for any stamp that is not older.
        session.last_time = selection::CURRENT_TIME;
        let window = session.window.unwrap();
        session
            .connection
            .as_ref()
            .unwrap()
            .change_property(
                PropMode::APPEND,
                window,
                session.atoms.property,
                AtomEnum::STRING,
                8,
                0,
                &[],
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let _ = session.poll_events(Duration::from_millis(5)).unwrap();
            if session.last_time != selection::CURRENT_TIME && session.last_time >= before {
                return;
            }
        }
        panic!("no PropertyNotify timestamp");
    }

    /// Pump both sessions until `reader` answers. The owner must be pumped
    /// too, since it serves the request from its own event stream.
    /// The deadline is wall-clock so parallel window tests on the same
    /// server slow this down without failing it.
    fn read_through(owner: &mut Session, reader: &mut Session) -> Option<String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let _ = owner.poll_events(Duration::from_millis(5)).expect("owner");
            for event in reader
                .poll_events(Duration::from_millis(5))
                .expect("reader")
            {
                if let Event::ClipboardText(text) = event {
                    return text;
                }
            }
        }
        panic!("no clipboard answer");
    }

    #[test]
    #[ignore = "opens X11 windows; run with --ignored when DISPLAY is set"]
    fn text_crosses_between_two_clients_and_clear_drops_ownership() {
        assert!(display_ready(), "DISPLAY is unset");
        let name = private_selection("cross");
        let (mut owner, owner_id) = open(&name, "kumimanu-owner");
        let (mut reader, reader_id) = open(&name, "kumimanu-reader");

        assert!(owner.set_clipboard_text(owner_id, "héllo 1").expect("own"));
        reader.request_clipboard_text(reader_id).expect("request");
        // A second request while one is pending is dropped, not queued.
        reader.request_clipboard_text(reader_id).expect("duplicate");
        assert_eq!(
            read_through(&mut owner, &mut reader).as_deref(),
            Some("héllo 1")
        );
        for _ in 0..5 {
            for event in reader.poll_events(Duration::from_millis(5)).unwrap() {
                assert!(
                    !matches!(event, Event::ClipboardText(_)),
                    "duplicate request produced a second answer"
                );
            }
        }

        // The reader takes ownership. The first owner sees SelectionClear.
        assert!(reader.set_clipboard_text(reader_id, "2").expect("own"));
        for _ in 0..20 {
            let _ = owner.poll_events(Duration::from_millis(5)).unwrap();
            if owner.owned.is_none() {
                break;
            }
        }
        assert!(owner.owned.is_none(), "SelectionClear was not applied");
        // A real paste is stamped by its Ctrl+V. Without one, the last time
        // this session saw predates the reader's ownership and the reader
        // correctly refuses it.
        fresh_time(&mut owner);
        owner.request_clipboard_text(owner_id).expect("request");
        assert_eq!(read_through(&mut reader, &mut owner).as_deref(), Some("2"));

        reader.close_window(reader_id).unwrap();
        owner.close_window(owner_id).unwrap();
    }

    #[test]
    #[ignore = "opens X11 windows; run with --ignored when DISPLAY is set"]
    fn incr_moves_text_in_chunks_and_the_cap_holds() {
        assert!(display_ready(), "DISPLAY is unset");
        let name = private_selection("incr");
        let (mut owner, owner_id) = open(&name, "kumimanu-incr-owner");
        let (mut reader, reader_id) = open(&name, "kumimanu-incr-reader");
        // Seven-byte chunks split the two-byte scalars.
        owner.single_limit = Some(7);
        let text = "é1".repeat(40);
        assert!(owner.set_clipboard_text(owner_id, &text).expect("own"));
        reader.request_clipboard_text(reader_id).expect("request");
        assert_eq!(read_through(&mut owner, &mut reader), Some(text));
        for _ in 0..5 {
            let _ = owner.poll_events(Duration::from_millis(5)).unwrap();
        }
        assert!(owner.transfers.is_empty(), "transfer was not retired");

        // An owned copy past the cap is refused by the owner, so the reader
        // gets no text. The reader's own cap across INCR chunks is locked by
        // `selection::tests::incoming_poisons_on_overflow_type_change_and_partial_then_drains`.
        owner.single_limit = Some(64 * 1024);
        let big = "a".repeat(MAX_CLIPBOARD_BYTES);
        assert!(owner.set_clipboard_text(owner_id, &big).expect("own"));
        owner.owned.as_mut().unwrap().0.push('a');
        reader.request_clipboard_text(reader_id).expect("request");
        assert_eq!(read_through(&mut owner, &mut reader), None);

        reader.close_window(reader_id).unwrap();
        owner.close_window(owner_id).unwrap();
    }

    #[test]
    #[ignore = "opens X11 windows; run with --ignored when DISPLAY is set"]
    fn no_owner_self_owner_and_empty_text() {
        assert!(display_ready(), "DISPLAY is unset");
        let name = private_selection("self");
        let (mut session, id) = open(&name, "kumimanu-self");

        session.request_clipboard_text(id).expect("request");
        let events = session.poll_events(Duration::ZERO).unwrap();
        assert!(events.contains(&Event::ClipboardText(None)), "{events:?}");

        assert!(!session.set_clipboard_text(id, "").expect("empty"));
        let big = "a".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert!(!session.set_clipboard_text(id, &big).expect("big"));

        assert!(session.set_clipboard_text(id, "42").expect("own"));
        session.request_clipboard_text(id).expect("request");
        let events = session.poll_events(Duration::ZERO).unwrap();
        assert!(
            events.contains(&Event::ClipboardText(Some("42".into()))),
            "{events:?}"
        );
        session.close_window(id).unwrap();
    }

    fn send_key(session: &Session, keycode: u8) {
        let window = session.window.unwrap();
        let event = KeyPressEvent {
            response_type: KEY_PRESS_EVENT,
            detail: keycode,
            sequence: 0,
            time: 0,
            root: session.root,
            event: window,
            child: 0,
            root_x: 0,
            root_y: 0,
            event_x: 1,
            event_y: 1,
            state: x11rb::protocol::xproto::KeyButMask::from(0u16),
            same_screen: true,
        };
        let connection = session.connection.as_ref().unwrap();
        connection
            .send_event(false, window, EventMask::NO_EVENT, event)
            .unwrap();
    }

    fn send_button(session: &Session, button: u8, x: i16, y: i16) {
        let window = session.window.unwrap();
        let event = x11rb::protocol::xproto::ButtonPressEvent {
            response_type: x11rb::protocol::xproto::BUTTON_PRESS_EVENT,
            detail: button,
            sequence: 0,
            time: 0,
            root: session.root,
            event: window,
            child: 0,
            root_x: x,
            root_y: y,
            event_x: x,
            event_y: y,
            state: x11rb::protocol::xproto::KeyButMask::from(0u16),
            same_screen: true,
        };
        session
            .connection
            .as_ref()
            .unwrap()
            .send_event(false, window, EventMask::NO_EVENT, event)
            .unwrap();
    }

    fn send_delete(session: &Session) {
        let window = session.window.unwrap();
        let event = ClientMessageEvent {
            response_type: CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window,
            type_: session.wm_protocols,
            data: ClientMessageData::from([session.wm_delete, 0, 0, 0, 0]),
        };
        session
            .connection
            .as_ref()
            .unwrap()
            .send_event(false, window, EventMask::NO_EVENT, event)
            .unwrap();
    }

    fn fd_count() -> usize {
        std::fs::read_dir("/proc/self/fd")
            .map(|entries| entries.filter(|entry| entry.is_ok()).count())
            .unwrap_or(0)
    }
}
