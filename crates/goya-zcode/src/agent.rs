//! NDJSON protocol client for the ZCode CLI `app-server`.
//!
//! Wire contract (see docs/goya-zcode.md and drowzy
//! `packages/shared/src/zcode-protocol`): requests `{id, method, params}`,
//! responses `{id, result|error}`, server notifications
//! `{method: "session/event", params: <envelope>}`. Server request schemas
//! are strict, so params must match exactly; response parsing is lenient
//! (unknown fields are ignored, unknown event types are inert).

use crate::view::{LinkState, ProjEvent, SessionEntry, SessionView};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Handle shared between the agent thread and the render loop.
#[derive(Clone)]
pub struct MonitorHandle {
    pub view: Arc<Mutex<SessionView>>,
    pub dirty: Arc<AtomicBool>,
    pub stop: Arc<AtomicBool>,
}

impl MonitorHandle {
    pub fn new() -> Self {
        Self {
            view: Arc::new(Mutex::new(SessionView::default())),
            dirty: Arc::new(AtomicBool::new(true)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Take and clear the dirty flag (render loop polls this).
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }
}

impl Default for MonitorHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// One agent-thread run: spawn the CLI, list sessions, subscribe, stream
/// events into the view. Exits when `stop` is set or the child dies;
/// `spawn_agent` wraps it with reconnect backoff.
struct AgentRun {
    cli: String,
    workspace: PathBuf,
    session: Option<String>,
    handle: MonitorHandle,
    child: Child,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>,
}

impl AgentRun {
    fn spawn(cli: &str, workspace: &Path) -> Result<Self, String> {
        let mut child = Command::new(cli)
            .arg("app-server")
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn `{cli} app-server`: {e}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "app-server stdin unavailable".to_string())?;
        Ok(Self {
            cli: cli.to_string(),
            workspace: workspace.to_path_buf(),
            session: None,
            handle: MonitorHandle::new(),
            child,
            next_id: AtomicU64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn set_view(&self, f: impl FnOnce(&mut SessionView)) {
        if let Ok(mut view) = self.handle.view.lock() {
            f(&mut view);
        }
        self.handle.dirty.store(true, Ordering::Relaxed);
    }

    /// Send one request and await its response (responses are routed to us
    /// by the reader thread through `pending`).
    fn request(
        &self,
        stdin: &mut ChildStdin,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.pending
            .lock()
            .expect("pending map poisoned")
            .insert(id, tx);
        let frame = json!({"id": id, "method": method, "params": params});
        let line = frame.to_string();
        if writeln!(stdin, "{line}")
            .and_then(|_| stdin.flush())
            .is_err()
        {
            self.pending
                .lock()
                .expect("pending map poisoned")
                .remove(&id);
            return Err("app-server stdin write failed".to_string());
        }
        match rx.recv_timeout(REQUEST_TIMEOUT) {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(err)) => Err(err),
            Err(RecvTimeoutError::Timeout) => {
                self.pending
                    .lock()
                    .expect("pending map poisoned")
                    .remove(&id);
                Err(format!("{method} timed out"))
            }
            Err(RecvTimeoutError::Disconnected) => Err("reader thread gone".to_string()),
        }
    }

    /// Reader loop: route responses to pending requests, apply
    /// `session/event` notifications to the view.
    fn read_loop(&self, reader: BufReader<std::process::ChildStdout>) {
        let pending = Arc::clone(&self.pending);
        for line in reader.lines() {
            if self.handle.stop.load(Ordering::Relaxed) {
                return;
            }
            let Ok(line) = line else { return };
            if line.trim().is_empty() {
                continue;
            }
            let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                // The stdout channel is strict JSON; a foreign line means
                // the child was launched without the protocol boundary.
                self.set_view(|v| v.link = LinkState::Down("non-JSON frame on stdout".into()));
                return;
            };
            if msg.get("id").is_some()
                && (msg.get("result").is_some() || msg.get("error").is_some())
            {
                let id = msg["id"].as_u64().unwrap_or(0);
                let sender = pending.lock().expect("pending map poisoned").remove(&id);
                if let Some(sender) = sender {
                    if let Some(err) = msg.get("error") {
                        let _ = sender.send(Err(err.to_string()));
                    } else {
                        let _ = sender.send(Ok(msg["result"].clone()));
                    }
                }
                continue;
            }
            if msg.get("method").and_then(Value::as_str) == Some("session/event") {
                if let Some(event) = map_event(msg.get("params").unwrap_or(&Value::Null)) {
                    self.set_view(|v| v.apply(&event));
                }
            }
            // Other notifications (state.updated, process/*) are inert.
        }
    }

    fn run(&mut self, session_arg: Option<String>) -> Result<(), String> {
        let mut stdin = self.child.stdin.take().expect("stdin taken at spawn");
        let stdout = self.child.stdout.take().expect("stdout taken at spawn");
        let reader = BufReader::new(stdout);
        // The reader needs &self's pending map but the borrow checker
        // dislikes splitting across threads; route through a scoped thread
        // is impossible with std, so keep reads on this thread between
        // requests instead: spawn a lightweight thread sharing Arc clones.
        {
            let pending = Arc::clone(&self.pending);
            let view = Arc::clone(&self.handle.view);
            let dirty = Arc::clone(&self.handle.dirty);
            let stop = Arc::clone(&self.handle.stop);
            let _reader_thread = std::thread::spawn(move || {
                read_events(reader, pending, view, dirty, stop);
            });
            // (The thread detaches; the child's EOF ends it.)
        }

        // 1. session/list for the workspace (workspaceKey = trimmed path,
        //    the local-workspace identity rule).
        let ws_path = self.workspace.to_string_lossy();
        let workspace_ref = json!({
            "workspacePath": ws_path,
            "workspaceKey": ws_path.trim(),
        });
        self.set_view(|v| v.link = LinkState::Up);
        let list = self
            .request(
                &mut stdin,
                "session/list",
                json!({ "workspace": workspace_ref }),
            )
            .map_err(|e| format!("session/list failed: {e}"))?;
        let entries = parse_sessions(&list);
        if entries.is_empty() {
            self.set_view(|v| {
                v.link = LinkState::Up;
                v.sessions = Vec::new();
                v.status = "no sessions".to_string();
            });
            return Ok(());
        }
        let chosen = self
            .session
            .clone()
            .or(session_arg)
            .and_then(|id| entries.iter().find(|e| e.session_id == id).cloned())
            .or_else(|| entries.first().cloned());
        let Some(target) = chosen else {
            self.set_view(|v| v.status = "session not found".to_string());
            return Ok(());
        };
        self.set_view(|v| {
            v.sessions = entries.clone();
            v.title = target.title.clone();
            v.status = target.status.clone();
        });

        // 2. subscribe; live events flow through the reader thread.
        let _sub = self.request(
            &mut stdin,
            "session/subscribe",
            json!({
                "sessionId": target.session_id,
                "deliveryKind": "desktop-continuous",
            }),
        )?;
        self.set_view(|v| v.status = "subscribed".to_string());

        // 3. wait for stop or child exit.
        while !self.handle.stop.load(Ordering::Relaxed) {
            if let Ok(Some(_status)) = self.child.try_wait() {
                self.set_view(|v| v.link = LinkState::Down("app-server exited".into()));
                return Err("app-server exited".to_string());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = self.child.kill();
        Ok(())
    }
}

/// Free-standing reader (owns everything it touches so it can move into a
/// thread while the run loop keeps issuing requests).
fn read_events(
    reader: BufReader<std::process::ChildStdout>,
    pending: Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>,
    view: Arc<Mutex<SessionView>>,
    dirty: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    for line in reader.lines() {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            if let Ok(mut v) = view.lock() {
                v.link = LinkState::Down("non-JSON frame on stdout".into());
            }
            dirty.store(true, Ordering::Relaxed);
            return;
        };
        if msg.get("id").is_some() && (msg.get("result").is_some() || msg.get("error").is_some()) {
            let id = msg["id"].as_u64().unwrap_or(0);
            let sender = pending.lock().expect("pending map poisoned").remove(&id);
            if let Some(sender) = sender {
                let payload = if let Some(err) = msg.get("error") {
                    Err(match err.get("message").and_then(Value::as_str) {
                        Some(m) => m.to_string(),
                        None => err.to_string(),
                    })
                } else {
                    Ok(msg["result"].clone())
                };
                let _ = sender.send(payload);
            }
            continue;
        }
        if msg.get("method").and_then(Value::as_str) == Some("session/event") {
            if let Some(event) = map_event(msg.get("params").unwrap_or(&Value::Null)) {
                if let Ok(mut v) = view.lock() {
                    v.apply(&event);
                }
                dirty.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Map a wire event envelope onto a projection event. Unknown types are
/// inert: `None`.
fn map_event(envelope: &Value) -> Option<ProjEvent> {
    let kind = envelope.get("type").and_then(Value::as_str)?;
    let payload = envelope.get("payload").unwrap_or(&Value::Null);
    match kind {
        "turn.started" => Some(ProjEvent::TurnStarted {
            input: payload
                .get("input")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        "part.delta" => {
            let field = payload.get("field").and_then(Value::as_str);
            if matches!(field, None | Some("text")) {
                Some(ProjEvent::PartDelta {
                    delta: payload
                        .get("delta")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
            } else {
                None
            }
        }
        "turn.completed" => Some(ProjEvent::TurnCompleted {
            response: payload
                .get("response")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            result: payload
                .get("resultType")
                .and_then(Value::as_str)
                .unwrap_or("success")
                .to_string(),
        }),
        "turn.failed" => Some(ProjEvent::TurnFailed {
            message: payload
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("turn failed")
                .to_string(),
        }),
        _ => None,
    }
}

/// Lenient session list parse: we need only id/title/status.
fn parse_sessions(result: &Value) -> Vec<SessionEntry> {
    result
        .get("sessions")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    Some(SessionEntry {
                        session_id: s.get("sessionId")?.as_str()?.to_string(),
                        title: s
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        status: s
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Spawn the CLI app-server and keep the view fed, reconnecting with
/// backoff until `stop`. Intended to run on its own thread.
pub fn spawn_agent(cli: &str, workspace: &Path, session: Option<String>) -> MonitorHandle {
    let handle = MonitorHandle::new();
    let cli = cli.to_string();
    let workspace = workspace.to_path_buf();
    let h = handle.clone();
    std::thread::spawn(move || {
        loop {
            if h.stop.load(Ordering::Relaxed) {
                return;
            }
            match AgentRun::spawn(&cli, &workspace) {
                Ok(mut run) => {
                    let _ = run.run(None);
                }
                Err(err) => {
                    if let Ok(mut v) = h.view.lock() {
                        v.link = LinkState::Down(err);
                    }
                    h.dirty.store(true, Ordering::Relaxed);
                }
            }
            // Reconnect backoff; stop immediately if asked.
            for _ in 0..RECONNECT_DELAY.as_millis() / 100 {
                if h.stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    handle
}
