//! Sandboxed Chromium viewport for the native Goya host.
//!
//! This crate owns the browser subprocess, its private profile and its loopback
//! DevTools connection. It returns bounded RGBA8 frames; it never owns a Goya
//! window or exposes raw DevTools/JavaScript access to the renderer.

use base64::Engine as _;
use png::{BitDepth, ColorType, Transformations};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::io::Cursor;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{Builder, TempDir};
use tungstenite::client::client_with_config;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};
use url::Url;

const MAX_VIEWPORT_WIDTH: u32 = 2048;
const MAX_VIEWPORT_HEIGHT: u32 = 2048;
const MAX_VIEWPORT_PIXELS: usize = 4_194_304;
const MAX_FRAME_BYTES: usize = MAX_VIEWPORT_PIXELS * 4;
const MAX_CDP_MESSAGE_BYTES: usize = 24 * 1024 * 1024;
const MAX_AX_NODES: usize = 4096;
const MAX_AX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_PNG_BASE64_BYTES: usize = MAX_CDP_MESSAGE_BYTES - 1024;
const MAX_PROFILE_ENDPOINT_BYTES: u64 = 512;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_POLL: Duration = Duration::from_millis(100);
const MAX_BROWSER_EVENTS: usize = 256;

/// Initial viewport size. Both dimensions are CSS pixels at device scale 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
}

impl ViewportSize {
    pub fn validate(self) -> Result<Self, BrowserError> {
        let pixels = usize::try_from(self.width)
            .ok()
            .and_then(|w| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|h| w.checked_mul(h))
            })
            .ok_or(BrowserError::ViewportTooLarge)?;
        if self.width == 0
            || self.height == 0
            || self.width > MAX_VIEWPORT_WIDTH
            || self.height > MAX_VIEWPORT_HEIGHT
            || pixels > MAX_VIEWPORT_PIXELS
            || pixels
                .checked_mul(4)
                .is_none_or(|bytes| bytes > MAX_FRAME_BYTES)
        {
            return Err(BrowserError::ViewportTooLarge);
        }
        Ok(self)
    }
}

/// App-owned browser configuration. The executable path is trusted host
/// configuration and is passed directly to `Command`, never through a shell.
#[derive(Clone, Debug)]
pub struct BrowserConfig {
    pub executable: PathBuf,
    pub viewport: ViewportSize,
}

impl BrowserConfig {
    pub fn new(executable: impl Into<PathBuf>, width: u32, height: u32) -> Self {
        Self {
            executable: executable.into(),
            viewport: ViewportSize { width, height },
        }
    }
}

/// Tightly packed, top-down RGBA8 screenshot owned by the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub rgba: Vec<u8>,
}

impl RgbaFrame {
    fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, BrowserError> {
        let size = ViewportSize { width, height }.validate()?;
        let stride = usize::try_from(size.width)
            .ok()
            .and_then(|w| w.checked_mul(4))
            .ok_or(BrowserError::ViewportTooLarge)?;
        let expected = stride
            .checked_mul(usize::try_from(size.height).map_err(|_| BrowserError::ViewportTooLarge)?)
            .ok_or(BrowserError::ViewportTooLarge)?;
        if rgba.len() != expected || rgba.len() > MAX_FRAME_BYTES {
            return Err(BrowserError::InvalidFrameLength);
        }
        Ok(Self {
            width: size.width,
            height: size.height,
            stride,
            rgba,
        })
    }
}

/// Pointer actions accepted by the native viewport controller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerAction {
    Move {
        x: f64,
        y: f64,
    },
    Press {
        x: f64,
        y: f64,
    },
    Release {
        x: f64,
        y: f64,
    },
    Scroll {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    },
}

/// Stable browser key names expected from the native host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyAction {
    Down { key: String, modifiers: u8 },
    Up { key: String, modifiers: u8 },
}

/// Failure from startup, CDP, navigation, or bounded image decoding.
#[derive(Debug)]
pub enum BrowserError {
    InvalidExecutable,
    InvalidUrl,
    ViewportTooLarge,
    InvalidFrameLength,
    InvalidPng,
    InvalidDevToolsEndpoint,
    Timeout(&'static str),
    ChildExited,
    Protocol(String),
    Io(std::io::Error),
    Socket(tungstenite::Error),
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidExecutable => f.write_str("Chromium executable is not a file"),
            Self::InvalidUrl => f.write_str("only credential-free HTTP(S) URLs are supported"),
            Self::ViewportTooLarge => f.write_str("browser viewport exceeds its configured bounds"),
            Self::InvalidFrameLength => {
                f.write_str("RGBA frame length does not match its dimensions")
            }
            Self::InvalidPng => f.write_str("Chromium returned an invalid or unsupported PNG"),
            Self::InvalidDevToolsEndpoint => f.write_str("invalid loopback DevTools endpoint"),
            Self::Timeout(what) => write!(f, "timed out waiting for {what}"),
            Self::ChildExited => f.write_str("Chromium exited"),
            Self::Protocol(message) => write!(f, "DevTools protocol error: {message}"),
            Self::Io(error) => write!(f, "browser I/O error: {error}"),
            Self::Socket(error) => write!(f, "browser WebSocket error: {error}"),
        }
    }
}

impl std::error::Error for BrowserError {}

impl From<std::io::Error> for BrowserError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<tungstenite::Error> for BrowserError {
    fn from(value: tungstenite::Error) -> Self {
        Self::Socket(value)
    }
}

/// Owns one Chromium child, page target, WebSocket, and private profile.
/// Drop kills and reaps the exact child before removing its profile.
pub struct BrowserSession {
    child: Option<Child>,
    profile: Option<TempDir>,
    socket: Option<WebSocket<TcpStream>>,
    session_id: String,
    next_id: u64,
    viewport: ViewportSize,
    mouse_buttons: u8,
    events: VecDeque<Value>,
    context_origins: HashMap<i64, String>,
}

impl BrowserSession {
    /// Launch a fresh sandboxed browser process with a private temporary profile.
    pub fn launch(config: BrowserConfig) -> Result<Self, BrowserError> {
        let viewport = config.viewport.validate()?;
        if !config.executable.is_file() {
            return Err(BrowserError::InvalidExecutable);
        }
        let profile = Builder::new().prefix("goya-webview-").tempdir()?;
        restrict_profile_permissions(profile.path())?;
        let mut process = Command::new(&config.executable);
        process
            .arg("--headless")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg("--remote-debugging-port=0")
            .arg(format!("--user-data-dir={}", profile.path().display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-default-apps")
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            process.process_group(0);
        }
        let mut child = process.spawn()?;

        let endpoint = match wait_for_endpoint(&mut child, profile.path()) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                stop_child(&mut child);
                return Err(error);
            }
        };
        let mut socket = match connect_loopback(endpoint) {
            Ok(socket) => socket,
            Err(error) => {
                stop_child(&mut child);
                return Err(error);
            }
        };

        let mut next_id = 1;
        let mut startup_events = VecDeque::new();
        let target = match command(
            &mut socket,
            &mut next_id,
            None,
            "Target.createTarget",
            json!({"url":"about:blank"}),
            &mut startup_events,
        ) {
            Ok(value) => value["targetId"].as_str().map(str::to_owned),
            Err(error) => {
                stop_child(&mut child);
                return Err(error);
            }
        }
        .ok_or_else(|| {
            stop_child(&mut child);
            BrowserError::Protocol("Target.createTarget omitted targetId".into())
        })?;
        let session = match command(
            &mut socket,
            &mut next_id,
            None,
            "Target.attachToTarget",
            json!({"targetId":target,"flatten":true}),
            &mut startup_events,
        ) {
            Ok(value) => value["sessionId"].as_str().map(str::to_owned),
            Err(error) => {
                stop_child(&mut child);
                return Err(error);
            }
        }
        .ok_or_else(|| {
            stop_child(&mut child);
            BrowserError::Protocol("Target.attachToTarget omitted sessionId".into())
        })?;

        let mut browser = Self {
            child: Some(child),
            profile: Some(profile),
            socket: Some(socket),
            session_id: session,
            next_id,
            viewport,
            mouse_buttons: 0,
            events: VecDeque::new(),
            context_origins: HashMap::new(),
        };
        browser.command_page("Page.enable", json!({}))?;
        browser.command_page("Runtime.enable", json!({}))?;
        browser.set_viewport(viewport.width, viewport.height)?;
        Ok(browser)
    }

    pub fn viewport(&self) -> ViewportSize {
        self.viewport
    }

    /// Resize the Chromium page before the host accepts frames at that size.
    pub fn set_viewport(&mut self, width: u32, height: u32) -> Result<(), BrowserError> {
        let viewport = ViewportSize { width, height }.validate()?;
        self.command_page(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": viewport.width,
                "height": viewport.height,
                "deviceScaleFactor": 1,
                "mobile": false
            }),
        )?;
        self.viewport = viewport;
        Ok(())
    }

    /// Navigate the owned page, rejecting active/script and local-file URLs.
    pub fn navigate(&mut self, value: &str) -> Result<(), BrowserError> {
        let url = validate_navigation_url(value)?;
        let result = self.command_page("Page.navigate", json!({"url":url.as_str()}))?;
        if result["errorText"].as_str().is_some() {
            return Err(BrowserError::Protocol(
                result["errorText"].as_str().unwrap_or_default().to_owned(),
            ));
        }
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                return Err(BrowserError::Timeout("page load"));
            }
            let ready = self.command_page(
                "Runtime.evaluate",
                json!({"expression":"({state:document.readyState,href:location.href})","returnByValue":true}),
            )?;
            if ready["result"]["value"]["state"].as_str() == Some("complete")
                && ready["result"]["value"]["href"].as_str() != Some("about:blank")
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    pub fn go_back(&mut self) -> Result<bool, BrowserError> {
        self.navigate_history(-1)
    }
    pub fn go_forward(&mut self) -> Result<bool, BrowserError> {
        self.navigate_history(1)
    }
    pub fn reload(&mut self) -> Result<(), BrowserError> {
        self.command_page("Page.reload", json!({"ignoreCache":false}))?;
        Ok(())
    }
    fn navigate_history(&mut self, delta: isize) -> Result<bool, BrowserError> {
        let history = self.command_page("Page.getNavigationHistory", json!({}))?;
        let entries = history["entries"]
            .as_array()
            .ok_or_else(|| BrowserError::Protocol("history omitted entries".into()))?;
        let current = history["currentIndex"]
            .as_i64()
            .ok_or_else(|| BrowserError::Protocol("history omitted index".into()))?;
        let target = current + delta as i64;
        let Some(entry) = usize::try_from(target).ok().and_then(|i| entries.get(i)) else {
            return Ok(false);
        };
        let id = entry["id"]
            .as_i64()
            .ok_or_else(|| BrowserError::Protocol("history entry omitted id".into()))?;
        self.command_page("Page.navigateToHistoryEntry", json!({"entryId":id}))?;
        Ok(true)
    }

    /// Capture and bounded-decode one full viewport screenshot.
    pub fn capture_frame(&mut self) -> Result<RgbaFrame, BrowserError> {
        let result = self.command_page(
            "Page.captureScreenshot",
            json!({"format":"png","fromSurface":true,"captureBeyondViewport":false,"optimizeForSpeed":true}),
        )?;
        let encoded = result["data"].as_str().ok_or(BrowserError::InvalidPng)?;
        decode_png_base64(encoded, self.viewport)
    }

    pub fn dispatch_pointer(&mut self, action: PointerAction) -> Result<(), BrowserError> {
        let next_buttons = match action {
            PointerAction::Press { .. } => 1,
            PointerAction::Release { .. } => 0,
            _ => self.mouse_buttons,
        };
        let (x, y, event_type, extra) = match action {
            PointerAction::Move { x, y } => (x, y, "mouseMoved", json!({})),
            PointerAction::Press { x, y } => (
                x,
                y,
                "mousePressed",
                json!({"button":"left","clickCount":1}),
            ),
            PointerAction::Release { x, y } => (
                x,
                y,
                "mouseReleased",
                json!({"button":"left","clickCount":1}),
            ),
            PointerAction::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => (
                x,
                y,
                "mouseWheel",
                json!({"deltaX":delta_x,"deltaY":delta_y}),
            ),
        };
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= f64::from(self.viewport.width)
            || y >= f64::from(self.viewport.height)
            || matches!(action, PointerAction::Scroll { delta_x, delta_y, .. } if !delta_x.is_finite() || !delta_y.is_finite())
        {
            return Err(BrowserError::Protocol(
                "pointer coordinate out of viewport".into(),
            ));
        }
        let mut params = json!({
            "type": event_type,
            "x": x,
            "y": y,
            "modifiers": 0,
            "buttons": next_buttons
        });
        if let (Some(target), Some(additional)) = (params.as_object_mut(), extra.as_object()) {
            target.extend(additional.clone());
        }
        self.command_page("Input.dispatchMouseEvent", params)?;
        self.mouse_buttons = next_buttons;
        Ok(())
    }

    pub fn dispatch_key(&mut self, action: KeyAction) -> Result<(), BrowserError> {
        let (event_type, key, modifiers) = match action {
            KeyAction::Down { key, modifiers } => ("keyDown", key, modifiers),
            KeyAction::Up { key, modifiers } => ("keyUp", key, modifiers),
        };
        if key.is_empty() || key.len() > 64 || key.chars().any(char::is_control) || modifiers > 15 {
            return Err(BrowserError::Protocol("invalid key name".into()));
        }
        self.command_page(
            "Input.dispatchKeyEvent",
            json!({"type":event_type,"key":key,"modifiers":modifiers}),
        )?;
        Ok(())
    }

    /// Insert committed IME text. Composition/preedit is not yet exposed.
    pub fn insert_text(&mut self, text: &str) -> Result<(), BrowserError> {
        if text.len() > 64 * 1024 {
            return Err(BrowserError::Protocol("text input exceeds 64 KiB".into()));
        }
        self.command_page("Input.insertText", json!({"text":text}))?;
        Ok(())
    }

    /// Apply or clear IME preedit text in the focused page element.
    pub fn set_composition(&mut self, text: &str) -> Result<(), BrowserError> {
        let cursor = i32::try_from(text.encode_utf16().count()).unwrap_or(i32::MAX);
        self.set_composition_range(text, cursor, cursor)
    }

    pub fn set_composition_range(
        &mut self,
        text: &str,
        selection_start: i32,
        selection_end: i32,
    ) -> Result<(), BrowserError> {
        if text.len() > 64 * 1024 {
            return Err(BrowserError::Protocol(
                "IME composition exceeds 64 KiB".into(),
            ));
        }
        let max = i32::try_from(text.encode_utf16().count()).unwrap_or(i32::MAX);
        let selection_start = selection_start.clamp(0, max);
        let selection_end = selection_end.clamp(selection_start, max);
        self.command_page(
            "Input.imeSetComposition",
            json!({
                "text":text,
                "selectionStart":selection_start,
                "selectionEnd":selection_end
            }),
        )?;
        Ok(())
    }

    /// Read selected page text only in response to an explicit host copy action.
    pub fn selected_text(&mut self) -> Result<String, BrowserError> {
        let result = self.command_page(
            "Runtime.evaluate",
            json!({
                "expression":"String(window.getSelection())",
                "returnByValue":true
            }),
        )?;
        let selected = result["result"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if selected.len() > 64 * 1024 {
            return Err(BrowserError::Protocol(
                "selection exceeds text bound".into(),
            ));
        }
        Ok(selected)
    }

    /// Read a bounded semantic snapshot of the current page for a native AT
    /// adapter. The result is data only; callers must not treat it as actions.
    pub fn accessibility_snapshot(&mut self) -> Result<Value, BrowserError> {
        let result = self.command_page("Accessibility.getFullAXTree", json!({}))?;
        let nodes = result["nodes"]
            .as_array()
            .ok_or_else(|| BrowserError::Protocol("accessibility tree omitted nodes".into()))?;
        if nodes.len() > MAX_AX_NODES {
            return Err(BrowserError::Protocol(
                "accessibility tree exceeds node bound".into(),
            ));
        }
        let projection: Vec<Value> = nodes
            .iter()
            .map(|node| {
                json!({
                    "role": node["role"]["value"].as_str().unwrap_or_default(),
                    "name": node["name"]["value"].as_str().unwrap_or_default(),
                    "value": node["value"]["value"].as_str().unwrap_or_default(),
                    "description": node["description"]["value"].as_str().unwrap_or_default(),
                    "childIds": node["childIds"].as_array().cloned().unwrap_or_default()
                })
            })
            .collect();
        let snapshot = Value::Array(projection);
        if serde_json::to_vec(&snapshot)
            .map_err(|e| BrowserError::Protocol(e.to_string()))?
            .len()
            > MAX_AX_TEXT_BYTES
        {
            return Err(BrowserError::Protocol(
                "accessibility tree exceeds byte bound".into(),
            ));
        }
        Ok(snapshot)
    }

    /// Return the URL and title of the owned page without exposing CDP.
    pub fn committed_url(&mut self) -> Result<String, BrowserError> {
        let result = self.command_page("Page.getFrameTree", json!({}))?;
        result["frameTree"]["frame"]["url"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| BrowserError::Protocol("missing committed frame URL".into()))
    }

    pub fn committed_origin(&mut self) -> Result<String, BrowserError> {
        Url::parse(&self.committed_url()?)
            .map(|url| url.origin().ascii_serialization())
            .map_err(|_| BrowserError::InvalidUrl)
    }

    /// Return the URL and title of the owned page without exposing CDP.
    pub fn page_metadata(&mut self) -> Result<(String, String), BrowserError> {
        let result = self.command_page(
            "Runtime.evaluate",
            json!({
                "expression":"({url:location.href,title:document.title})",
                "returnByValue":true
            }),
        )?;
        let value = &result["result"]["value"];
        Ok((
            value["url"].as_str().unwrap_or_default().to_owned(),
            value["title"].as_str().unwrap_or_default().to_owned(),
        ))
    }

    /// Browser product/build identifier for acceptance evidence.
    pub fn browser_version(&mut self) -> Result<String, BrowserError> {
        let socket = self.socket.as_mut().ok_or(BrowserError::ChildExited)?;
        let product = command(
            socket,
            &mut self.next_id,
            None,
            "Browser.getVersion",
            json!({}),
            &mut self.events,
        )?;
        product["product"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| BrowserError::Protocol("Browser.getVersion omitted product".into()))
    }

    fn command_page(&mut self, method: &str, params: Value) -> Result<Value, BrowserError> {
        command(
            self.socket.as_mut().ok_or(BrowserError::ChildExited)?,
            &mut self.next_id,
            Some(&self.session_id),
            method,
            params,
            &mut self.events,
        )
    }

    /// Install one named Runtime binding for native bridge traffic.
    pub fn install_binding(&mut self, name: &str) -> Result<(), BrowserError> {
        if name.is_empty()
            || name.len() > 64
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(BrowserError::Protocol("invalid binding name".into()));
        }
        self.command_page("Runtime.addBinding", json!({"name":name}))?;
        Ok(())
    }

    pub fn initialize_plugin_bridge(
        &mut self,
        instance_id: &str,
        session_id: &str,
        generation: u64,
    ) -> Result<(), BrowserError> {
        let instance = serde_json::to_string(instance_id)
            .map_err(|e| BrowserError::Protocol(e.to_string()))?;
        let session =
            serde_json::to_string(session_id).map_err(|e| BrowserError::Protocol(e.to_string()))?;
        self.command_page("Runtime.evaluate", json!({"expression":format!("globalThis.__goyaBridge={{instanceId:{instance},sessionId:{session},generation:{generation}}};"),"returnByValue":true}))?;
        Ok(())
    }

    /// Invoke the plugin's documented request callback; no caller-supplied JS
    /// is accepted. Plugins may expose this callback as their bridge client.
    pub fn request_host_summary(&mut self) -> Result<(), BrowserError> {
        self.command_page(
            "Runtime.evaluate",
            json!({"expression":"window.requestGoyaSummary()","returnByValue":true}),
        )?;
        Ok(())
    }

    pub fn plugin_bridge_result(&mut self) -> Result<String, BrowserError> {
        let result = self.command_page("Runtime.evaluate", json!({"expression":"document.querySelector('#bridge-result')?.textContent || ''","returnByValue":true}))?;
        let text = result["result"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if text.len() > 64 * 1024 {
            return Err(BrowserError::Protocol("bridge result exceeds bound".into()));
        }
        Ok(text)
    }

    /// Drain queued Chromium events, retaining only bounded binding calls.
    pub fn poll_binding_events(&mut self, binding: &str) -> Vec<Value> {
        let mut result = Vec::new();
        while let Some(event) = self.events.pop_front() {
            if event["method"] == "Runtime.executionContextCreated" {
                if let (Some(id), Some(origin)) = (
                    event["params"]["context"]["id"].as_i64(),
                    event["params"]["context"]["origin"].as_str(),
                ) {
                    if origin.len() <= 2048 {
                        self.context_origins.insert(id, origin.to_owned());
                    }
                }
            }
            if event["method"] == "Runtime.executionContextDestroyed" {
                if let Some(id) = event["params"]["executionContextId"].as_i64() {
                    self.context_origins.remove(&id);
                }
            }
            if event["method"] == "Runtime.executionContextsCleared" {
                self.context_origins.clear();
            }
            if event["method"] == "Runtime.bindingCalled"
                && event["params"]["name"].as_str() == Some(binding)
                && event["params"]["payload"]
                    .as_str()
                    .is_some_and(|s| s.len() <= 64 * 1024)
            {
                result.push(event["params"].clone());
            }
        }
        result
    }

    pub fn context_origin(&self, context_id: i64) -> Option<&str> {
        self.context_origins.get(&context_id).map(String::as_str)
    }

    /// Return a serialized bridge reply through a context ID received from a
    /// queued Runtime.bindingCalled event. The ID is numeric and bounded.
    pub fn reply_binding(
        &mut self,
        context_id: i64,
        binding: &str,
        expected_origin: &str,
        reply: &Value,
    ) -> Result<(), BrowserError> {
        if context_id <= 0 || binding.is_empty() || binding.len() > 64 {
            return Err(BrowserError::Protocol("invalid bridge reply target".into()));
        }
        if self.context_origins.get(&context_id).map(String::as_str) != Some(expected_origin) {
            return Err(BrowserError::Protocol(
                "bridge context origin mismatch".into(),
            ));
        }
        let payload =
            serde_json::to_string(reply).map_err(|e| BrowserError::Protocol(e.to_string()))?;
        if payload.len() > 64 * 1024 {
            return Err(BrowserError::Protocol("bridge reply exceeds bound".into()));
        }
        let expression = format!(
            "globalThis.dispatchEvent(new MessageEvent('message',{{data:{payload},origin:{origin}}}))",
            origin = serde_json::to_string(expected_origin).unwrap()
        );
        self.command_page(
            "Runtime.evaluate",
            json!({"expression":expression,"contextId":context_id,"returnByValue":true}),
        )?;
        Ok(())
    }

    /// Close and reap Chromium immediately. Safe to call more than once.
    pub fn close(&mut self) {
        if let Some(mut socket) = self.socket.take() {
            // Ask Chromium to close itself while the control channel is still
            // live. The process-group fallback below handles a missing ACK.
            let _ = command(
                &mut socket,
                &mut self.next_id,
                None,
                "Browser.close",
                json!({}),
                &mut self.events,
            );
            drop(socket);
        }
        if let Some(mut child) = self.child.take() {
            stop_child(&mut child);
        }
        self.profile.take();
    }
}

impl Drop for BrowserSession {
    fn drop(&mut self) {
        self.close();
    }
}

fn validate_navigation_url(value: &str) -> Result<Url, BrowserError> {
    let url = Url::parse(value).map_err(|_| BrowserError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || value.len() > 8192
    {
        return Err(BrowserError::InvalidUrl);
    }
    Ok(url)
}

fn decode_png_base64(encoded: &str, expected: ViewportSize) -> Result<RgbaFrame, BrowserError> {
    expected.validate()?;
    if encoded.is_empty() || encoded.len() > MAX_PNG_BASE64_BYTES {
        return Err(BrowserError::InvalidPng);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| BrowserError::InvalidPng)?;
    if bytes.len() > MAX_CDP_MESSAGE_BYTES {
        return Err(BrowserError::InvalidPng);
    }
    decode_png_bytes(&bytes, expected)
}

fn decode_png_bytes(bytes: &[u8], expected: ViewportSize) -> Result<RgbaFrame, BrowserError> {
    if bytes.len() > MAX_CDP_MESSAGE_BYTES {
        return Err(BrowserError::InvalidPng);
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(Transformations::ALPHA | Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| BrowserError::InvalidPng)?;
    let info = reader.info();
    if info.width != expected.width || info.height != expected.height {
        return Err(BrowserError::InvalidPng);
    }
    let (color_type, depth) = reader.output_color_type();
    if depth != BitDepth::Eight {
        return Err(BrowserError::InvalidPng);
    }
    let channels = match color_type {
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Indexed => return Err(BrowserError::InvalidPng),
    };
    let pixel_count = usize::try_from(info.width)
        .ok()
        .and_then(|w| {
            usize::try_from(info.height)
                .ok()
                .and_then(|h| w.checked_mul(h))
        })
        .ok_or(BrowserError::InvalidPng)?;
    if pixel_count > MAX_VIEWPORT_PIXELS {
        return Err(BrowserError::InvalidPng);
    }
    let source_len = pixel_count
        .checked_mul(channels)
        .ok_or(BrowserError::InvalidPng)?;
    let output_len = pixel_count
        .checked_mul(4)
        .filter(|len| *len <= MAX_FRAME_BYTES)
        .ok_or(BrowserError::InvalidPng)?;
    if reader
        .output_buffer_size()
        .is_none_or(|size| size != source_len || size > MAX_FRAME_BYTES)
    {
        return Err(BrowserError::InvalidPng);
    }
    let mut source = vec![0; source_len];
    let output = reader
        .next_frame(&mut source)
        .map_err(|_| BrowserError::InvalidPng)?;
    if output.buffer_size() != source_len {
        return Err(BrowserError::InvalidPng);
    }
    let mut rgba = Vec::with_capacity(output_len);
    for pixel in source.chunks_exact(channels) {
        match channels {
            1 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255]),
            2 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]),
            3 => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            4 => rgba.extend_from_slice(pixel),
            _ => unreachable!("validated channel count"),
        }
    }
    RgbaFrame::new(expected.width, expected.height, rgba)
}

fn restrict_profile_permissions(path: &Path) -> Result<(), BrowserError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

struct DevToolsEndpoint {
    port: u16,
    path: String,
}

fn wait_for_endpoint(child: &mut Child, profile: &Path) -> Result<DevToolsEndpoint, BrowserError> {
    let file = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Some(_status) = child.try_wait()? {
            return Err(BrowserError::ChildExited);
        }
        if let Ok(metadata) = fs::metadata(&file) {
            if metadata.len() > MAX_PROFILE_ENDPOINT_BYTES {
                return Err(BrowserError::InvalidDevToolsEndpoint);
            }
            if let Ok(contents) = fs::read_to_string(&file) {
                if let Some(endpoint) = parse_devtools_endpoint(&contents) {
                    return Ok(endpoint);
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(BrowserError::Timeout("Chromium DevTools endpoint"));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn parse_devtools_endpoint(contents: &str) -> Option<DevToolsEndpoint> {
    let mut lines = contents.lines();
    let port = lines.next()?.parse::<u16>().ok()?;
    let path = lines.next()?;
    if port == 0
        || !path.starts_with("/devtools/browser/")
        || path.len() > 256
        || !path[1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
        || lines.next().is_some()
    {
        return None;
    }
    Some(DevToolsEndpoint {
        port,
        path: path.to_owned(),
    })
}

fn connect_loopback(endpoint: DevToolsEndpoint) -> Result<WebSocket<TcpStream>, BrowserError> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    // The HTTP Upgrade handshake can take longer than the command poll
    // interval, especially while Chromium is starting its headless target.
    stream.set_read_timeout(Some(STARTUP_TIMEOUT))?;
    stream.set_write_timeout(Some(COMMAND_TIMEOUT))?;
    stream.set_nodelay(true)?;
    let request = format!("ws://127.0.0.1:{}{}", endpoint.port, endpoint.path);
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_CDP_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_CDP_MESSAGE_BYTES))
        .max_write_buffer_size(1024 * 1024);
    let (mut socket, _response) = client_with_config(request, stream, Some(config))
        .map_err(|error| BrowserError::Protocol(error.to_string()))?;
    socket.get_mut().set_read_timeout(Some(SOCKET_POLL))?;
    Ok(socket)
}

fn command(
    socket: &mut WebSocket<TcpStream>,
    next_id: &mut u64,
    session_id: Option<&str>,
    method: &str,
    params: Value,
    events: &mut VecDeque<Value>,
) -> Result<Value, BrowserError> {
    let id = *next_id;
    *next_id = next_id
        .checked_add(1)
        .ok_or_else(|| BrowserError::Protocol("request id exhausted".into()))?;
    let mut request = json!({"id":id,"method":method,"params":params});
    if let Some(session_id) = session_id {
        request["sessionId"] = Value::String(session_id.to_owned());
    }
    socket.send(Message::Text(request.to_string().into()))?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        if Instant::now() >= deadline {
            return Err(BrowserError::Timeout("DevTools response"));
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if text.len() > MAX_CDP_MESSAGE_BYTES {
                    return Err(BrowserError::Protocol(
                        "response exceeds message bound".into(),
                    ));
                }
                let message: Value = serde_json::from_str(&text)
                    .map_err(|_| BrowserError::Protocol("invalid JSON response".into()))?;
                if message["id"].as_u64() != Some(id) {
                    if message.get("method").and_then(Value::as_str).is_some() {
                        if events.len() == MAX_BROWSER_EVENTS {
                            events.pop_front();
                        }
                        events.push_back(message);
                    }
                    continue;
                }
                if let Some(error) = message.get("error") {
                    return Err(BrowserError::Protocol(error.to_string()));
                }
                return message
                    .get("result")
                    .cloned()
                    .ok_or_else(|| BrowserError::Protocol("response omitted result".into()));
            }
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload))?,
            Ok(Message::Close(_)) => return Err(BrowserError::ChildExited),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(BrowserError::Socket(error)),
        }
    }
}

fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    {
        let process_group = -(child.id() as i32);
        // SAFETY: the child is launched in a dedicated process group (see
        // `process_group(0)` in `launch`), so this signal cannot reach the host.
        unsafe {
            libc::kill(process_group, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        // Signal the whole private process group even if Chromium's browser
        // process exited cleanly; zygote/renderer children must not outlive
        // the temporary profile. The group was created specifically for this
        // child and is never shared with the host.
        unsafe {
            libc::kill(process_group, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_or_unacknowledged_pointer_input_does_not_advance_button_state() {
        let mut browser = BrowserSession {
            child: None,
            profile: None,
            socket: None,
            session_id: String::new(),
            next_id: 1,
            viewport: ViewportSize {
                width: 10,
                height: 10,
            },
            mouse_buttons: 0,
            events: VecDeque::new(),
            context_origins: HashMap::new(),
        };
        assert!(browser
            .dispatch_pointer(PointerAction::Press {
                x: f64::NAN,
                y: 1.0
            })
            .is_err());
        assert_eq!(browser.mouse_buttons, 0);
        assert!(browser
            .dispatch_pointer(PointerAction::Press { x: 1.0, y: 1.0 })
            .is_err());
        assert_eq!(browser.mouse_buttons, 0);
    }
    fn png_rgba(width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(data).unwrap();
        }
        bytes
    }

    #[test]
    fn navigation_policy_accepts_only_credential_free_http_urls() {
        assert!(validate_navigation_url("https://example.test/docs?q=1").is_ok());
        assert!(validate_navigation_url("http://127.0.0.1:8888/page").is_ok());
        for invalid in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
            "https://user:pass@example.test/",
            "ftp://example.test/",
        ] {
            assert!(validate_navigation_url(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn viewport_and_decoded_image_are_bounded() {
        assert!(ViewportSize {
            width: 0,
            height: 1
        }
        .validate()
        .is_err());
        assert!(ViewportSize {
            width: 2049,
            height: 1
        }
        .validate()
        .is_err());
        let png = png_rgba(2, 1, &[255, 0, 0, 255, 0, 255, 0, 128]);
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let frame = decode_png_base64(
            &encoded,
            ViewportSize {
                width: 2,
                height: 1,
            },
        )
        .unwrap();
        assert_eq!(frame.stride, 8);
        assert_eq!(&frame.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&frame.rgba[4..], &[0, 255, 0, 128]);
        assert!(decode_png_base64(
            &encoded,
            ViewportSize {
                width: 1,
                height: 2
            }
        )
        .is_err());
        assert!(decode_png_base64(
            &"A".repeat(MAX_PNG_BASE64_BYTES + 4),
            ViewportSize {
                width: 1,
                height: 1
            }
        )
        .is_err());
    }

    #[test]
    fn endpoint_parser_requires_private_chrome_path_shape() {
        let endpoint = parse_devtools_endpoint("43210\n/devtools/browser/abc-123\n").unwrap();
        assert_eq!(endpoint.port, 43210);
        assert_eq!(endpoint.path, "/devtools/browser/abc-123");
        assert!(parse_devtools_endpoint("0\n/devtools/browser/x\n").is_none());
        assert!(parse_devtools_endpoint("43210\n/devtools/page/x\n").is_none());
        assert!(parse_devtools_endpoint("43210\n/devtools/browser/x?host=evil\n").is_none());
    }

    #[test]
    #[ignore = "launches real sandboxed Chromium; run explicitly for engine smoke evidence"]
    fn sandboxed_chromium_renders_local_fixture_and_receives_pointer_input() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        struct FixtureServer {
            url: String,
            stop: Arc<AtomicBool>,
            thread: Option<thread::JoinHandle<()>>,
        }
        impl Drop for FixtureServer {
            fn drop(&mut self) {
                self.stop.store(true, Ordering::Release);
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let fixture = include_str!("../../../apps/webview-fixture.html")
            .as_bytes()
            .to_vec();
        let server_thread = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Chromium 会预连接但暂不发请求；不能让空闲 socket 阻塞整个夹具服务器。
                        stream
                            .set_read_timeout(Some(Duration::from_millis(200)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut request = [0u8; 4096];
                        let read = stream.read(&mut request).unwrap_or(0);
                        let path = std::str::from_utf8(&request[..read])
                            .ok()
                            .and_then(|text| text.lines().next())
                            .unwrap_or_default();
                        if path.starts_with("GET / ") {
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                fixture.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(&fixture);
                        } else {
                            let _ = stream.write_all(
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            );
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        let server = FixtureServer {
            url: format!("http://{address}/"),
            stop,
            thread: Some(server_thread),
        };

        let executable = std::env::var_os("GOYA_CHROMIUM_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/bin/google-chrome"));
        assert!(
            executable.is_file(),
            "set GOYA_CHROMIUM_PATH to a Chromium executable"
        );
        let mut browser = BrowserSession::launch(BrowserConfig::new(executable, 640, 480)).unwrap();
        let pid = browser.child.as_ref().unwrap().id();
        let profile = browser.profile.as_ref().unwrap().path().to_path_buf();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        let command_line = fs::read(format!("/proc/{pid}/cmdline")).unwrap();
        assert!(!command_line
            .split(|byte| *byte == 0 || byte.is_ascii_whitespace())
            .any(|argument| argument == b"--no-sandbox"));
        assert!(
            command_line
                .split(|byte| *byte == 0 || byte.is_ascii_whitespace())
                .any(|argument| argument.starts_with(b"--remote-debugging-address=127.0.0.1")),
            "unexpected owned browser argv: {}",
            String::from_utf8_lossy(&command_line).replace('\0', " ")
        );
        // An idle preconnect must not starve the actual navigation request.
        let _idle_preconnect = TcpStream::connect(address).unwrap();
        browser.navigate(&server.url).unwrap();
        let (page_url, title) = browser.page_metadata().unwrap();
        assert_eq!(page_url, server.url);
        assert_eq!(title, "Goya Webview local fixture");
        let version = browser.browser_version().unwrap();
        eprintln!("GYP-WEBVIEW smoke: pid={pid} product={version} url={page_url}");
        assert!(
            linux_renderer_seccomp_active(pid),
            "Chromium renderer did not enter seccomp sandbox"
        );

        let before = browser.capture_frame().unwrap();
        assert_eq!(
            (before.width, before.height, before.stride),
            (640, 480, 2560)
        );
        let sample = (20 * before.width as usize + 20) * 4;
        let before_color = before.rgba[sample..sample + 3].to_vec();
        assert_eq!(before_color, [0x16, 0x20, 0x33]);
        // 使用实际布局命中控件；CSS margin 会让硬编码 y=165 落在按钮外。
        let target = browser.command_page("Runtime.evaluate", json!({
            "expression":"(()=>{const r=document.querySelector('#toggle').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()",
            "returnByValue":true
        })).unwrap();
        let x = target["result"]["value"]["x"].as_f64().unwrap();
        let y = target["result"]["value"]["y"].as_f64().unwrap();
        assert!(x > 0.0 && x < 640.0 && y > 0.0 && y < 480.0);
        browser
            .dispatch_pointer(PointerAction::Press { x, y })
            .unwrap();
        browser
            .dispatch_pointer(PointerAction::Release { x, y })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut changed = None;
        while Instant::now() < deadline {
            let frame = browser.capture_frame().unwrap();
            let color = &frame.rgba[sample..sample + 3];
            if color != before_color.as_slice() {
                changed = Some(color.to_vec());
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(
            changed.is_some(),
            "native pointer event did not change fixture pixels"
        );
        browser.close();
        assert!(
            !profile.exists(),
            "private browser profile was not removed after close"
        );
        assert!(
            fs::metadata(format!("/proc/{pid}")).is_err(),
            "Chromium child was not reaped"
        );
        #[cfg(target_os = "linux")]
        {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline && linux_process_group_exists(pid) {
                thread::sleep(Duration::from_millis(20));
            }
            assert!(
                !linux_process_group_exists(pid),
                "Chromium descendants remained in the owned process group"
            );
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_process_group_exists(group_id: u32) -> bool {
        let Ok(entries) = fs::read_dir("/proc") else {
            return false;
        };
        entries.flatten().any(|entry| {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
            else {
                return false;
            };
            let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return false;
            };
            stat.rsplit_once(')')
                .and_then(|(_, tail)| tail.split_whitespace().nth(2))
                .and_then(|value| value.parse::<u32>().ok())
                == Some(group_id)
        })
    }

    #[cfg(target_os = "linux")]
    fn linux_renderer_seccomp_active(browser_pid: u32) -> bool {
        fn descendants(pid: u32, result: &mut Vec<u32>) {
            let path = format!("/proc/{pid}/task/{pid}/children");
            let Ok(children) = fs::read_to_string(path) else {
                return;
            };
            for child in children
                .split_whitespace()
                .filter_map(|value| value.parse::<u32>().ok())
            {
                result.push(child);
                descendants(child, result);
            }
        }
        let mut candidates = Vec::new();
        descendants(browser_pid, &mut candidates);
        candidates.into_iter().any(|pid| {
            let command_line = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            if !command_line
                .split(|byte| *byte == 0 || byte.is_ascii_whitespace())
                .any(|argument| argument == b"--type=renderer")
            {
                return false;
            }
            fs::read_to_string(format!("/proc/{pid}/status"))
                .map(|status| status.lines().any(|line| line == "Seccomp:\t2"))
                .unwrap_or(false)
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn linux_renderer_seccomp_active(_browser_pid: u32) -> bool {
        true
    }
}
