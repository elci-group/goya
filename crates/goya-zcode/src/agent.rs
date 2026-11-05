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
const RECONNECT_DELAY_MS: u64 = 5_000;
const POLL_INTERVAL_MS: u64 = 200;

/// State shared between the agent thread and the render loop. The agent
/// thread is the only writer; the render loop polls `take_dirty` and reads
/// a view snapshot.
#[derive(Clone)]
pub struct MonitorHandle {
    pub view: Arc<Mutex<SessionView>>,
    dirty: Arc<AtomicBool>,
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

    /// Take and clear the dirty flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }
}

impl Default for MonitorHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// One connection attempt: spawn the CLI, list sessions, subscribe, then
/// hold the process open while events stream through the reader thread.
struct AgentRun {
    workspace: PathBuf,
    handle: MonitorHandle,
    child: Child,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>,
}

impl AgentRun {
    fn spawn(cli: &str, workspace: &Path, handle: MonitorHandle) -> Result<Self, String> {
        let child = Command::new(cli)
            .arg("app-server")
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn `{cli} app-server`: {e}"))?;
        // Assert both pipes exist; the run loop re-takes them.
        if child.stdin.is_none() {
            return Err("app-server stdin unavailable".to_string());
        }
        if child.stdout.is_none() {
            return Err("app-server stdout unavailable".to_string());
        }
        Ok(Self {
            workspace: workspace.to_path_buf(),
            handle,
            child,
            next_id: AtomicU64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn set_view(&self, f: impl FnOnce(&mut SessionView)) {
        if let Ok(mut view) = self.handle.view.lock() {
            f(&mut view);
        }
        self.handle.mark_dirty();
    }

    /// Send one request and await its response (routed back by the reader
    /// thread through `pending`). Serialised by `&mut ChildStdin` ownership.
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
        if writeln!(stdin, "{frame}")
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
            Ok(result) => result,
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

    fn run(mut self, session: Option<String>) -> Result<(), String> {
        let mut stdin: ChildStdin = self.child.stdin.take().expect("checked at spawn");
        let stdout = self.child.stdout.take().expect("checked at spawn");

        // Reader thread routes responses into `pending` and applies
        // session/event notifications to the shared view.
        let reader_pending = Arc::clone(&self.pending);
        let reader_view = Arc::clone(&self.handle.view);
        let reader_dirty = Arc::clone(&self.handle.dirty);
        let reader_stop = Arc::clone(&self.handle.stop);
        std::thread::spawn(move || {
            read_frames(
                BufReader::new(stdout),
                reader_pending,
                reader_view,
                reader_dirty,
                reader_stop,
            );
        });

        // Local workspace identity rule: workspaceKey = trimmed path.
        let ws_path = self.workspace.to_string_lossy().to_string();
        let workspace_ref = json!({
            "workspacePath": ws_path,
            "workspaceKey": ws_path.trim().to_string(),
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
                v.sessions = Vec::new();
                v.status = "no sessions".to_string();
            });
            return Ok(());
        }
        let target = match session {
            Some(id) => entries.iter().find(|e| e.session_id == id).cloned(),
            None => entries.first().cloned(),
        };
        let Some(target) = target else {
            self.set_view(|v| v.status = "session not found".to_string());
            return Ok(());
        };
        self.set_view(|v| {
            v.sessions = entries.clone();
            v.title = target.title.clone();
            v.status = target.status.clone();
        });

        // subscribe; live events now flow through the reader thread.
        self.request(
            &mut stdin,
            "session/subscribe",
            json!({
                "sessionId": target.session_id,
                "deliveryKind": "desktop-continuous",
            }),
        )
        .map_err(|e| format!("session/subscribe failed: {e}"))?;
        self.set_view(|v| v.status = "subscribed".to_string());

        // Hold the connection open until stop or child exit.
        while !self.handle.stop.load(Ordering::Relaxed) {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.set_view(|v| v.link = LinkState::Down("app-server exited".into()));
                    return Err("app-server exited".to_string());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS)),
                Err(e) => return Err(format!("wait failed: {e}")),
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

/// Reader loop: route responses to pending requests, apply
/// `session/event` notifications to the view. Other notifications are
/// inert; a non-JSON line means the child lacks the protocol boundary.
fn read_frames(
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
    std::thread::spawn(move || loop {
        if h.stop.load(Ordering::Relaxed) {
            return;
        }
        match AgentRun::spawn(&cli, &workspace, h.clone()) {
            Ok(run) => {
                let _ = run.run(session.clone());
            }
            Err(err) => {
                if let Ok(mut v) = h.view.lock() {
                    v.link = LinkState::Down(err);
                }
                h.mark_dirty();
            }
        }
        for _ in 0..RECONNECT_DELAY_MS / 100 {
            if h.stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    handle
}
