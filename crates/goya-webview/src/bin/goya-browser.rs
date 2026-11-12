use goya_layout::Rect;
use goya_runtime::Primitive;
use goya_wayland::{
    BrowserNavigation, GoyaWindow, KeyModifiers, KeySpecial, PlatformEvent, Shortcut,
};
use goya_wayland::{PixelRect, RgbaLayer};
use goya_webview::{
    normalize_navigation_input, BrowserConfig, BrowserError, BrowserSession, KeyAction,
    PointerAction, RgbaFrame, ViewportSize,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const WINDOW_WIDTH: i32 = 1024;
const WINDOW_HEIGHT: i32 = 768;
const ACTIVE_FRAME: Duration = Duration::from_millis(80);
const IDLE_FRAME: Duration = Duration::from_millis(1000);
const TOOLBAR_HEIGHT: u32 = 52;
const MAX_VIEWPORT_EDGE: u32 = 2048;

enum BrowserCommand {
    Pointer(PointerAction),
    Key(KeyAction),
    Text(String),
    CompositionRange {
        text: String,
        selection_start: i32,
        selection_end: i32,
    },
    UnsupportedSurroundingDelete,
    CopySelection,
    Navigate(BrowserNavigation),
    NavigateUrl(String),
}

type ResizeRequest = (u64, ViewportSize);
type PendingResize = Arc<Mutex<Option<ResizeRequest>>>;

fn queue_resize(pending: &PendingResize, generation: u64, size: ViewportSize) {
    if let Ok(mut request) = pending.lock() {
        *request = Some((generation, size));
    }
}

fn take_resize(pending: &PendingResize) -> Option<ResizeRequest> {
    pending.lock().ok()?.take()
}

fn shutdown_requested(stopping: &AtomicBool) -> bool {
    stopping.load(Ordering::Acquire)
}

fn try_report(events: &SyncSender<WorkerEvent>, event: WorkerEvent) {
    let _ = events.try_send(event);
}

enum WorkerEvent {
    Frame { generation: u64, frame: RgbaFrame },
    Selection(String),
    Failure(String),
    Navigated(String),
    Notice(String),
}

fn browser_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("GOYA_CHROMIUM_PATH") {
        return PathBuf::from(path);
    }
    for candidate in ["/usr/bin/google-chrome", "/usr/bin/chromium"] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from("google-chrome")
}

fn dispatch_key(command: &SyncSender<BrowserCommand>, key: &str, modifiers: u8) {
    let _ = command.try_send(BrowserCommand::Key(KeyAction::Down {
        key: key.to_owned(),
        modifiers,
    }));
    let _ = command.try_send(BrowserCommand::Key(KeyAction::Up {
        key: key.to_owned(),
        modifiers,
    }));
}

fn modifier_bits(modifiers: KeyModifiers) -> u8 {
    u8::from(modifiers.ctrl) * 2 + u8::from(modifiers.shift) * 8
}

fn special_key(key: KeySpecial) -> &'static str {
    match key {
        KeySpecial::ArrowLeft => "ArrowLeft",
        KeySpecial::ArrowRight => "ArrowRight",
        KeySpecial::ArrowUp => "ArrowUp",
        KeySpecial::ArrowDown => "ArrowDown",
        KeySpecial::Home => "Home",
        KeySpecial::End => "End",
        KeySpecial::Delete => "Delete",
        KeySpecial::PageUp => "PageUp",
        KeySpecial::PageDown => "PageDown",
        KeySpecial::Escape => "Escape",
        KeySpecial::Tab => "Tab",
    }
}

fn spawn_browser_worker(
    url: String,
    initial_size: ViewportSize,
    commands: Receiver<BrowserCommand>,
    events: SyncSender<WorkerEvent>,
    stopping: Arc<AtomicBool>,
    pending_resize: PendingResize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut browser = match BrowserSession::launch(BrowserConfig {
            executable: browser_executable(),
            viewport: initial_size,
        }) {
            Ok(browser) => browser,
            Err(error) => {
                try_report(&events, WorkerEvent::Failure(error.to_string()));
                return;
            }
        };
        if let Err(error) = browser.navigate(&url) {
            try_report(&events, WorkerEvent::Failure(error.to_string()));
            return;
        }
        let mut generation = 0;
        let mut dirty = true;
        let mut last_capture = Instant::now()
            .checked_sub(IDLE_FRAME)
            .unwrap_or_else(Instant::now);
        let mut stopped = false;
        while !stopped {
            if shutdown_requested(&stopping) {
                break;
            }
            if let Some((next_generation, next_size)) = take_resize(&pending_resize) {
                match browser.set_viewport(next_size.width, next_size.height) {
                    Ok(()) => {
                        generation = next_generation;
                        dirty = true;
                    }
                    Err(error) => {
                        try_report(&events, WorkerEvent::Failure(error.to_string()));
                        return;
                    }
                }
            }
            loop {
                if shutdown_requested(&stopping) {
                    stopped = true;
                    break;
                }
                match commands.try_recv() {
                    Ok(BrowserCommand::Pointer(action)) => {
                        if let Err(error) = browser.dispatch_pointer(action) {
                            try_report(&events, WorkerEvent::Failure(error.to_string()));
                        }
                        dirty = true;
                    }
                    Ok(BrowserCommand::Key(action)) => {
                        if let Err(error) = browser.dispatch_key(action) {
                            try_report(&events, WorkerEvent::Failure(error.to_string()));
                        }
                        dirty = true;
                    }
                    Ok(BrowserCommand::Text(text)) => {
                        if let Err(error) = browser.insert_text(&text) {
                            try_report(&events, WorkerEvent::Failure(error.to_string()));
                        }
                        dirty = true;
                    }
                    Ok(BrowserCommand::CompositionRange {
                        text,
                        selection_start,
                        selection_end,
                    }) => {
                        if let Err(error) =
                            browser.set_composition_range(&text, selection_start, selection_end)
                        {
                            try_report(&events, WorkerEvent::Failure(error.to_string()));
                        }
                        dirty = true;
                    }
                    Ok(BrowserCommand::UnsupportedSurroundingDelete) => {
                        try_report(
                            &events,
                            WorkerEvent::Failure(
                                "browser host does not yet map IME surrounding deletion".into(),
                            ),
                        );
                    }
                    Ok(BrowserCommand::CopySelection) => match browser.selected_text() {
                        Ok(text) => {
                            try_report(&events, WorkerEvent::Selection(text));
                        }
                        Err(error) => {
                            try_report(&events, WorkerEvent::Failure(error.to_string()));
                        }
                    },
                    Ok(BrowserCommand::Navigate(action)) => {
                        let result = match action {
                            BrowserNavigation::Back => browser.go_back().map(|_| ()),
                            BrowserNavigation::Forward => browser.go_forward().map(|_| ()),
                            BrowserNavigation::Reload => browser.reload(),
                        };
                        report_navigation(&mut browser, &events, result.err());
                        dirty = true;
                    }
                    Ok(BrowserCommand::NavigateUrl(url)) => {
                        let error = browser.navigate(&url).err();
                        report_navigation(&mut browser, &events, error);
                        dirty = true;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        stopped = true;
                        break;
                    }
                }
            }
            if stopped {
                break;
            }
            let interval = if dirty { ACTIVE_FRAME } else { IDLE_FRAME };
            if last_capture.elapsed() >= interval {
                match browser.capture_frame() {
                    Ok(frame) => match events.try_send(WorkerEvent::Frame { generation, frame }) {
                        Ok(()) | Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => break,
                    },
                    Err(error) => {
                        try_report(&events, WorkerEvent::Failure(error.to_string()));
                    }
                }
                dirty = false;
                last_capture = Instant::now();
            }
            thread::sleep(Duration::from_millis(5));
        }
        browser.close();
    })
}

fn report_navigation(
    browser: &mut BrowserSession,
    events: &SyncSender<WorkerEvent>,
    error: Option<BrowserError>,
) {
    if let Some(error) = error {
        try_report(events, WorkerEvent::Notice(error.to_string()));
        return;
    }
    match browser.committed_url() {
        Ok(url) => try_report(events, WorkerEvent::Navigated(url)),
        Err(error) => try_report(events, WorkerEvent::Notice(error.to_string())),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let Some(url) = arguments.next() else {
        eprintln!("usage: goya-browser <http(s)-url>");
        std::process::exit(2);
    };
    let duration = match arguments.next().as_deref() {
        None => None,
        Some("--duration") => {
            let seconds: u64 = arguments.next().ok_or("missing duration")?.parse()?;
            if seconds == 0 || seconds > 3600 || arguments.next().is_some() {
                return Err("invalid duration".into());
            }
            Some(Duration::from_secs(seconds))
        }
        _ => return Err("usage: goya-browser <http(s)-url> [--duration <seconds>]".into()),
    };
    let mut window = GoyaWindow::create("Goya Browser", WINDOW_WIDTH, WINDOW_HEIGHT)?;
    let (width, height) = window.size();
    let surface_size = ViewportSize {
        width: u32::try_from(width)?,
        height: u32::try_from(height)?,
    };
    let initial_size = content_viewport(surface_size)?;
    window.set_extended_text_input_events(true);
    window.set_text_input_active(true);
    window.set_browser_navigation_keys(true);
    let (command_tx, command_rx) = mpsc::sync_channel(64);
    let (event_tx, event_rx) = mpsc::sync_channel(4);
    let stopping = Arc::new(AtomicBool::new(false));
    let pending_resize = Arc::new(Mutex::new(None));
    let browser_thread = spawn_browser_worker(
        url.clone(),
        initial_size,
        command_rx,
        event_tx,
        stopping.clone(),
        pending_resize.clone(),
    );
    let mut generation = 0u64;
    let mut last_size = surface_size;
    let mut content_size = initial_size;
    let mut latest_frame: Option<(u64, RgbaFrame)> = None;
    let mut frame_serial = 0u64;
    let mut presented_serial = 0u64;
    let mut last_pointer = (0.0, 0.0);
    let mut address = AddressEdit::new(url);
    let mut address_focus = false;
    let mut status = String::new();
    let mut page_pointer = false;
    let mut running = true;
    let mut engine_alive = true;
    let mut chrome_dirty = true;
    let started = Instant::now();
    let mut presented = 0_u64;
    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        while running
            && !window.close_requested()
            && duration.is_none_or(|limit| started.elapsed() < limit)
        {
            let mut typed_this_frame = String::new();
            for event in window.poll() {
                match event {
                    PlatformEvent::CloseRequested => running = false,
                    PlatformEvent::Resized(width, height) => {
                        if let (Ok(width), Ok(height)) =
                            (u32::try_from(width), u32::try_from(height))
                        {
                            let size = ViewportSize { width, height };
                            if width != 0 && height != 0 && size != last_size {
                                match content_viewport(size) {
                                    Ok(content) => {
                                        generation = generation.saturating_add(1);
                                        last_size = size;
                                        content_size = content;
                                        queue_resize(&pending_resize, generation, content);
                                        latest_frame = None;
                                    }
                                    Err(_) => {
                                        status = "This window size is outside the page viewport limits.".into();
                                    }
                                }
                                chrome_dirty = true;
                            }
                        }
                    }
                    PlatformEvent::PointerMove(x, y) => {
                        last_pointer = (x, y);
                        if page_pointer {
                            if let Some((x, y)) = content_coordinates(x, y) {
                                let _ = command_tx.try_send(BrowserCommand::Pointer(
                                    PointerAction::Move { x, y },
                                ));
                            }
                        }
                    }
                    PlatformEvent::PointerPress(x, y) => {
                        last_pointer = (x, y);
                        if y < f64::from(TOOLBAR_HEIGHT) {
                            let _ = toolbar_action(x, &mut address, &mut address_focus, &command_tx);
                            chrome_dirty = true;
                        } else {
                            if address_focus {
                                address_focus = false;
                                chrome_dirty = true;
                            }
                            page_pointer = true;
                            if let Some((x, y)) = content_coordinates(x, y) {
                                let _ = command_tx.try_send(BrowserCommand::Pointer(
                                    PointerAction::Press { x, y },
                                ));
                            }
                        }
                    }
                    PlatformEvent::PointerRelease(x, y) => {
                        last_pointer = (x, y);
                        if page_pointer {
                            if let Some((x, y)) = content_coordinates(x, y) {
                                let _ = command_tx.try_send(BrowserCommand::Pointer(
                                    PointerAction::Release { x, y },
                                ));
                            }
                        }
                        page_pointer = false;
                    }
                    PlatformEvent::PointerScroll(lines) => {
                        let (x, y) = last_pointer;
                        if y >= f64::from(TOOLBAR_HEIGHT) {
                            let _ = command_tx.try_send(BrowserCommand::Pointer(
                                PointerAction::Scroll {
                                    x,
                                    y: y - f64::from(TOOLBAR_HEIGHT),
                                    delta_x: 0.0,
                                    delta_y: f64::from(-lines) * 80.0,
                                },
                            ));
                        }
                    }
                    PlatformEvent::PointerScrollDelta { x, y } => {
                        let (pointer_x, pointer_y) = last_pointer;
                        if pointer_y >= f64::from(TOOLBAR_HEIGHT) {
                            let _ = command_tx.try_send(BrowserCommand::Pointer(
                                PointerAction::Scroll {
                                    x: pointer_x,
                                    y: pointer_y - f64::from(TOOLBAR_HEIGHT),
                                    delta_x: -x,
                                    delta_y: -y,
                                },
                            ));
                        }
                    }
                    PlatformEvent::KeyChar('\n' | '\r') if address_focus => {
                        submit_address(&address, &command_tx, &mut status, &mut address_focus);
                        chrome_dirty = true;
                    }
                    PlatformEvent::KeyChar('\u{8}') if address_focus => {
                        address.backspace();
                        chrome_dirty = true;
                    }
                    PlatformEvent::KeyChar('\n' | '\r') => dispatch_key(&command_tx, "Enter", 0),
                    PlatformEvent::KeyChar('\u{8}') => dispatch_key(&command_tx, "Backspace", 0),
                    PlatformEvent::KeyChar('\t') => dispatch_key(&command_tx, "Tab", 0),
                    PlatformEvent::KeyChar(character) => {
                        if address_focus {
                            address.insert(character);
                            typed_this_frame.push(character);
                            chrome_dirty = true;
                        } else {
                            let _ =
                                command_tx.try_send(BrowserCommand::Text(character.to_string()));
                        }
                    }
                    PlatformEvent::KeySpecial(KeySpecial::Escape) if address_focus => {
                        address_focus = false;
                        address.collapse();
                        chrome_dirty = true;
                    }
                    PlatformEvent::KeySpecial(key) if address_focus => {
                        edit_address_key(&mut address, key);
                        chrome_dirty = true;
                    }
                    PlatformEvent::KeySpecial(key) => {
                        dispatch_key(&command_tx, special_key(key), 0)
                    }
                    PlatformEvent::KeySpecialWithModifiers { key, modifiers } if address_focus => {
                        edit_address_key(&mut address, key);
                        chrome_dirty = true;
                        let _ = modifiers;
                    }
                    PlatformEvent::KeySpecialWithModifiers { key, modifiers } => {
                        dispatch_key(&command_tx, special_key(key), modifier_bits(modifiers));
                    }
                    PlatformEvent::Shortcut(Shortcut::Quit) => running = false,
                    PlatformEvent::Shortcut(Shortcut::Copy) => {
                        let _ = command_tx.try_send(BrowserCommand::CopySelection);
                    }
                    PlatformEvent::Shortcut(Shortcut::Paste) => window.request_clipboard_text(),
                    PlatformEvent::Shortcut(Shortcut::SessionNext | Shortcut::SessionPrevious) => {}
                    PlatformEvent::BrowserNavigation(action) => {
                        let _ = command_tx.try_send(BrowserCommand::Navigate(action));
                    }
                    PlatformEvent::Preedit(_) if address_focus => {}
                    PlatformEvent::PreeditWithCursor { .. } if address_focus => {}
                    PlatformEvent::Preedit(text) => {
                        let text = text.unwrap_or_default();
                        let cursor = i32::try_from(text.len()).unwrap_or(i32::MAX);
                        let _ = command_tx.try_send(BrowserCommand::CompositionRange {
                            text,
                            selection_start: cursor,
                            selection_end: cursor,
                        });
                    }
                    PlatformEvent::PreeditWithCursor {
                        text,
                        cursor_begin,
                        cursor_end,
                    } => {
                        let selection_start = byte_offset_to_utf16(&text, cursor_begin);
                        let selection_end = byte_offset_to_utf16(&text, cursor_end);
                        let _ = command_tx.try_send(BrowserCommand::CompositionRange {
                            text,
                            selection_start: selection_start.min(selection_end),
                            selection_end: selection_start.max(selection_end),
                        });
                    }
                    PlatformEvent::CommitText(text) => {
                        if address_focus {
                            apply_address_commit(
                                &mut address,
                                &text,
                                &typed_this_frame,
                                &command_tx,
                                &mut status,
                                &mut address_focus,
                            );
                            chrome_dirty = true;
                        } else {
                            let _ = command_tx.try_send(BrowserCommand::Text(text));
                            let _ = command_tx.try_send(BrowserCommand::CompositionRange {
                                text: String::new(),
                                selection_start: 0,
                                selection_end: 0,
                            });
                        }
                    }
                    PlatformEvent::ClipboardText(Some(text)) if address_focus => {
                        address.insert_str(&text);
                        chrome_dirty = true;
                    }
                    PlatformEvent::ClipboardText(Some(text)) => {
                        let _ = command_tx.try_send(BrowserCommand::Text(text));
                    }
                    PlatformEvent::DeleteSurroundingText { .. } => {
                        let _ = command_tx.try_send(BrowserCommand::UnsupportedSurroundingDelete);
                    }
                    PlatformEvent::ClipboardText(None) | PlatformEvent::Configured => {}
                }
            }
            loop {
                match event_rx.try_recv() {
                    Ok(WorkerEvent::Frame {
                        generation: frame_generation,
                        frame,
                    }) => {
                        latest_frame = Some((frame_generation, frame));
                        frame_serial = frame_serial.saturating_add(1);
                    }
                    Ok(WorkerEvent::Selection(text)) => window.set_clipboard_text(&text),
                    Ok(WorkerEvent::Failure(message)) => eprintln!("goya-browser: {message}"),
                    Ok(WorkerEvent::Navigated(url)) => {
                        if !address_focus {
                            address = AddressEdit::new(url);
                        }
                        status.clear();
                        chrome_dirty = true;
                    }
                    Ok(WorkerEvent::Notice(message)) => {
                        status = message;
                        chrome_dirty = true;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if engine_alive {
                            status = "The page engine stopped. Close the window to exit.".into();
                            engine_alive = false;
                            chrome_dirty = true;
                        }
                        break;
                    }
                }
            }
            if chrome_dirty || frame_serial != presented_serial {
                let (width, height) = window.size();
                let content_height = (height - TOOLBAR_HEIGHT as i32).max(0) as u32;
                let layer = latest_frame.as_ref().and_then(|(frame_generation, frame)| {
                    frame_matches(*frame_generation, generation, frame, content_size).then_some(
                        RgbaLayer {
                            x: 0,
                            y: TOOLBAR_HEIGHT as i32,
                            width: frame.width,
                            height: frame.height,
                            stride: frame.stride as usize,
                            rgba: &frame.rgba,
                            clip: Some(PixelRect {
                                x: 0,
                                y: TOOLBAR_HEIGHT as i32,
                                width: width.max(0) as u32,
                                height: content_height,
                            }),
                        },
                    )
                });
                let (primitives, fills) =
                    toolbar_primitives(width.max(0) as u32, &address, address_focus, &status);
                window.present_composited(
                    &primitives,
                    &fills,
                    &[],
                    [245, 247, 250],
                    [19, 24, 32],
                    2,
                    layer,
                )?;
                presented_serial = frame_serial;
                chrome_dirty = false;
                presented += 1;
            }
            window.pump(1);
            thread::sleep(Duration::from_millis(16));
        }
        if duration.is_some() && presented == 0 {
            return Err("no native browser frame presented".into());
        }
        Ok(())
    })();
    stopping.store(true, Ordering::Release);
    drop(command_tx);
    drop(event_rx);
    browser_thread
        .join()
        .map_err(|_| "browser worker panicked")?;
    outcome?;
    println!("native browser presentation PASS: {presented} frames");
    Ok(())
}

fn byte_offset_to_utf16(text: &str, offset: i32) -> i32 {
    let mut byte_offset = usize::try_from(offset.max(0))
        .unwrap_or(usize::MAX)
        .min(text.len());
    while !text.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    i32::try_from(text[..byte_offset].encode_utf16().count()).unwrap_or(i32::MAX)
}

fn content_viewport(surface: ViewportSize) -> Result<ViewportSize, BrowserError> {
    let width = surface.width.min(MAX_VIEWPORT_EDGE);
    let height = surface
        .height
        .saturating_sub(TOOLBAR_HEIGHT)
        .min(MAX_VIEWPORT_EDGE);
    ViewportSize { width, height }.validate()
}

fn content_coordinates(x: f64, y: f64) -> Option<(f64, f64)> {
    (y >= f64::from(TOOLBAR_HEIGHT)).then_some((x, y - f64::from(TOOLBAR_HEIGHT)))
}
fn frame_matches(
    generation: u64,
    current_generation: u64,
    frame: &RgbaFrame,
    viewport: ViewportSize,
) -> bool {
    generation == current_generation
        && frame.width == viewport.width
        && frame.height == viewport.height
}

fn toolbar_action(
    x: f64,
    address: &mut AddressEdit,
    address_focus: &mut bool,
    commands: &SyncSender<BrowserCommand>,
) -> bool {
    match x as i32 {
        0..=39 => {
            let _ = commands.try_send(BrowserCommand::Navigate(BrowserNavigation::Back));
            *address_focus = false;
            true
        }
        40..=79 => {
            let _ = commands.try_send(BrowserCommand::Navigate(BrowserNavigation::Forward));
            *address_focus = false;
            true
        }
        80..=119 => {
            let _ = commands.try_send(BrowserCommand::Navigate(BrowserNavigation::Reload));
            *address_focus = false;
            true
        }
        132.. => {
            *address_focus = true;
            address.select_all();
            true
        }
        _ => false,
    }
}

fn submit_address(
    address: &AddressEdit,
    commands: &SyncSender<BrowserCommand>,
    status: &mut String,
    address_focus: &mut bool,
) {
    match normalize_navigation_input(&address.text) {
        Ok(url) => {
            status.clear();
            *address_focus = false;
            if commands
                .send_timeout(
                    BrowserCommand::NavigateUrl(url),
                    Duration::from_millis(200),
                )
                .is_err()
            {
                *status = "The page engine is busy.".into();
                *address_focus = true;
            }
        }
        Err(error) => *status = error.to_string(),
    }
}

fn apply_address_commit(
    address: &mut AddressEdit,
    text: &str,
    typed_this_frame: &str,
    commands: &SyncSender<BrowserCommand>,
    status: &mut String,
    address_focus: &mut bool,
) {
    let submit = text == "\n"
        || text == "\r"
        || text.ends_with('\n')
        || text.ends_with('\r');
    let body = text.trim_end_matches(['\n', '\r']);
    if !body.is_empty() && !commit_duplicates(typed_this_frame, body) {
        address.insert_str(body);
    }
    if submit {
        submit_address(address, commands, status, address_focus);
    }
}

fn commit_duplicates(typed: &str, commit: &str) -> bool {
    !typed.is_empty() && (commit == typed || typed.ends_with(commit) || commit.ends_with(typed))
}

fn edit_address_key(address: &mut AddressEdit, key: KeySpecial) {
    match key {
        KeySpecial::Delete => address.delete_forward(),
        KeySpecial::ArrowLeft => address.move_by(-1),
        KeySpecial::ArrowRight => address.move_by(1),
        KeySpecial::Home => address.move_home(),
        KeySpecial::End => address.move_end(),
        _ => {}
    }
}

fn toolbar_primitives(
    width: u32,
    address: &AddressEdit,
    focused: bool,
    status: &str,
) -> (Vec<Primitive>, Vec<Option<u32>>) {
    let rect = |x: u32, y: u32, w: u32, h: u32| Rect {
        x: i64::from(x) * 250,
        y: i64::from(y) * 250,
        width: i64::from(w) * 250,
        height: i64::from(h) * 250,
    };
    let mut primitives = vec![
        Primitive::Panel(rect(0, 0, width, TOOLBAR_HEIGHT)),
        Primitive::Button(rect(4, 6, 32, 40), "<".into()),
        Primitive::Button(rect(44, 6, 32, 40), ">".into()),
        Primitive::Button(rect(84, 6, 32, 40), "R".into()),
        Primitive::Text(
            rect(140, 8, width.saturating_sub(152), 36),
            address.chars().take(1024).collect(),
        ),
    ];
    let mut fills = vec![
        Some(0x202a_3b),
        Some(0x3441_59),
        Some(0x3441_59),
        Some(0x3441_59),
        Some(if focused { 0x354d_68 } else { 0x2833_47 }),
    ];
    if width < 160 {
        primitives.truncate(4);
        fills.truncate(4);
    }
    (primitives, fills)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(width: u32, height: u32) -> ViewportSize {
        ViewportSize { width, height }
    }

    #[test]
    fn resize_updates_coalesce_even_when_command_queue_is_full() {
        let (commands, receiver) = mpsc::sync_channel(1);
        commands.try_send(BrowserCommand::CopySelection).unwrap();
        let pending = Arc::new(Mutex::new(None));

        queue_resize(&pending, 1, size(800, 600));
        queue_resize(&pending, 2, size(1200, 700));

        assert_eq!(take_resize(&pending), Some((2, size(1200, 700))));
        assert!(take_resize(&pending).is_none());
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserCommand::CopySelection)
        ));
    }

    #[test]
    fn toolbar_coordinate_mapping_and_stale_capture_checks() {
        assert_eq!(content_coordinates(27.0, 51.0), None);
        assert_eq!(content_coordinates(27.0, 52.0), Some((27.0, 0.0)));
        let frame = RgbaFrame {
            width: 640,
            height: 500,
            stride: 2560,
            rgba: vec![0; 640 * 500 * 4],
        };
        assert!(frame_matches(3, 3, &frame, size(640, 500)));
        assert!(!frame_matches(2, 3, &frame, size(640, 500)));
        assert!(!frame_matches(3, 3, &frame, size(640, 499)));
    }

    #[test]
    fn shutdown_signal_preempts_full_command_and_event_queues() {
        let (commands, receiver) = mpsc::sync_channel(1);
        commands.try_send(BrowserCommand::CopySelection).unwrap();
        let (events, _event_receiver) = mpsc::sync_channel(1);
        try_report(&events, WorkerEvent::Selection("first".into()));
        try_report(&events, WorkerEvent::Failure("dropped".into()));
        let stopping = AtomicBool::new(false);

        stopping.store(true, Ordering::Release);

        assert!(shutdown_requested(&stopping));
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserCommand::CopySelection)
        ));
    }
}
