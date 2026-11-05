//! NDJSON protocol client for the ZCode CLI `app-server`.
//!
//! Wire contract (see docs/goya-zcode.md and drowzy
//! `packages/shared/src/zcode-protocol`): requests `{id, method, params}`,
//! responses `{id, result|error}`, server notifications
//! `{method: "session/event", params: <envelope>}`. Server request schemas
//! are strict, so params must match exactly; response parsing is lenient
//! (unknown fields are ignored, unknown event types are inert).

use crate::view::{LinkState, ProjEvent, SessionEntry, SessionView, ToolState};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{
    channel, sync_channel, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError,
    TrySendError,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_DELAY_MS: u64 = 5_000;
const POLL_INTERVAL_MS: u64 = 200;
const COMMAND_QUEUE_CAPACITY: usize = 8;

/// Pending request id → one-shot response channel.
type PendingMap = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;

/// UI → agent-thread command (GY-ZCODE-003). The UI thread enqueues on
/// Enter; the agent thread (stdin owner) drains the queue between poll
/// iterations and issues the wire request itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentCmd {
    /// Send a composed line to the target session. `wire == false` when
    /// the monitor runs without `--allow-send`: admission is identical
    /// but no `session/send` request is written to the child's stdin.
    SendText {
        line: String,
        wire: bool,
        session_id: String,
        ticket: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SendPhase {
    AwaitingResponse,
    Accepted,
    Started,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingSend {
    ticket: u64,
    session_id: String,
    phase: SendPhase,
}

/// Per-connection send admission and event/response reconciliation.
#[derive(Debug, Default)]
struct SendLifecycle {
    next_ticket: u64,
    pending: Option<PendingSend>,
}

impl SendLifecycle {
    fn admit(&mut self, session_id: &str) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        let ticket = self.next_ticket.checked_add(1)?;
        self.next_ticket = ticket;
        self.pending = Some(PendingSend {
            ticket,
            session_id: session_id.to_string(),
            phase: SendPhase::AwaitingResponse,
        });
        Some(ticket)
    }

    /// Return a status only for the still-pending send. If a start or
    /// terminal event raced ahead of this response, it cannot regress state.
    fn response(&mut self, ticket: u64, accepted: bool) -> Option<&'static str> {
        let pending = self.pending.as_mut()?;
        if pending.ticket != ticket {
            return None;
        }
        if !accepted {
            self.pending = None;
            return Some("send rejected by server");
        }
        match pending.phase {
            SendPhase::AwaitingResponse => {
                pending.phase = SendPhase::Accepted;
                Some("sending")
            }
            SendPhase::Accepted => Some("sending"),
            SendPhase::Started => Some("running"),
        }
    }

    fn event(&mut self, session_id: &str, kind: &str) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        if pending.session_id != session_id {
            return;
        }
        match kind {
            "turn.started" => pending.phase = SendPhase::Started,
            "turn.completed" | "turn.failed" => self.pending = None,
            _ => {}
        }
    }

    fn disconnect(&mut self) {
        self.pending = None;
    }

    fn cancel(&mut self, ticket: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.ticket == ticket)
        {
            self.pending = None;
        }
    }
}

/// State shared between the agent thread and the render loop. The agent
/// thread is the only writer; the render loop polls `take_dirty` and reads
/// a view snapshot.
#[derive(Clone)]
pub struct MonitorHandle {
    pub view: Arc<Mutex<SessionView>>,
    dirty: Arc<AtomicBool>,
    pub stop: Arc<AtomicBool>,
    /// Inbound command queue: `SendText` and friends. Sender and receiver
    /// both sit behind Arc'd mutexes so every `MonitorHandle` clone can
    /// enqueue and the queue survives agent-thread reconnects.
    cmd_tx: SyncSender<AgentCmd>,
    cmd_rx: Arc<Mutex<Receiver<AgentCmd>>>,
    active_session_id: Arc<Mutex<Option<String>>>,
    send_lifecycle: Arc<Mutex<SendLifecycle>>,
}

impl MonitorHandle {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(COMMAND_QUEUE_CAPACITY);
        Self {
            view: Arc::new(Mutex::new(SessionView::default())),
            dirty: Arc::new(AtomicBool::new(true)),
            stop: Arc::new(AtomicBool::new(false)),
            cmd_tx: tx,
            cmd_rx: Arc::new(Mutex::new(rx)),
            active_session_id: Arc::new(Mutex::new(None)),
            send_lifecycle: Arc::new(Mutex::new(SendLifecycle::default())),
        }
    }

    /// Take and clear the dirty flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Enqueue without waiting for queue capacity. Each command captures the
    /// active session at enqueue time so reconnects cannot retarget it.
    pub fn send_text(&self, line: String, wire: bool) {
        let session_id = self
            .active_session_id
            .lock()
            .ok()
            .and_then(|active| active.clone());
        let Some(session_id) = session_id else {
            self.set_status("send unavailable — no active session");
            return;
        };
        let ticket = if wire {
            let admitted = self
                .send_lifecycle
                .lock()
                .ok()
                .and_then(|mut lifecycle| lifecycle.admit(&session_id));
            let Some(ticket) = admitted else {
                self.set_status("busy — send pending");
                return;
            };
            Some(ticket)
        } else {
            None
        };
        match self.cmd_tx.try_send(AgentCmd::SendText {
            line,
            wire,
            session_id,
            ticket,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                if let Some(ticket) = ticket {
                    if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
                        lifecycle.cancel(ticket);
                    }
                }
                self.set_status("busy — send queue full");
            }
            Err(TrySendError::Disconnected(_)) => {
                if let Some(ticket) = ticket {
                    if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
                        lifecycle.cancel(ticket);
                    }
                }
                self.set_status("send unavailable");
            }
        }
    }

    fn set_status(&self, status: &str) {
        if let Ok(mut view) = self.view.lock() {
            view.status = status.to_string();
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Record which session sends should target. Called by the agent run
    /// loop on subscribe; public so hosts/tests can pin a session without
    /// a live connection.
    pub fn set_active_session(&self, session_id: Option<String>) {
        if let Ok(mut active) = self.active_session_id.lock() {
            *active = session_id;
        }
    }

    /// Non-blocking command drain used by the agent run loop (and the
    /// headless acceptance): never blocks.
    pub fn try_take_cmd(&self) -> Option<AgentCmd> {
        let rx = self.cmd_rx.lock().ok()?;
        match rx.try_recv() {
            Ok(cmd) => Some(cmd),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
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
    pending: PendingMap,
    send_lifecycle: Arc<Mutex<SendLifecycle>>,
}

impl Drop for AgentRun {
    fn drop(&mut self) {
        // Every early return (including protocol errors) must reap app-server.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
            lifecycle.disconnect();
        }
        self.handle.set_active_session(None);
    }
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
        let send_lifecycle = Arc::clone(&handle.send_lifecycle);
        Ok(Self {
            workspace: workspace.to_path_buf(),
            handle,
            child,
            next_id: AtomicU64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
            send_lifecycle,
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
        let reader_done = Arc::new(AtomicBool::new(false));
        let thread_done = Arc::clone(&reader_done);
        let session_filter = Arc::new(Mutex::new(None::<String>));
        let reader_filter = Arc::clone(&session_filter);
        if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
            lifecycle.disconnect();
        }
        let reader_send_lifecycle = Arc::clone(&self.send_lifecycle);
        std::thread::spawn(move || {
            read_frames(
                BufReader::new(stdout),
                reader_pending,
                reader_view,
                reader_dirty,
                reader_stop,
                reader_filter,
                reader_send_lifecycle,
            );
            thread_done.store(true, Ordering::Release);
        });

        // Local workspace identity rule: workspaceKey = trimmed path.
        let ws_path = self.workspace.to_string_lossy().to_string();
        let workspace_ref = json!({
            "workspacePath": ws_path,
            "workspaceKey": ws_path.trim().to_string(),
        });
        let list = self
            .request(
                &mut stdin,
                "session/list",
                json!({ "workspace": workspace_ref }),
            )
            .map_err(|e| format!("session/list failed: {e}"))?;
        self.set_view(|v| v.link = LinkState::Up);
        let entries = parse_sessions(&list);
        if entries.is_empty() {
            self.set_view(|v| {
                v.sessions = Vec::new();
                v.status = "no sessions".to_string();
            });
            return Ok(());
        }
        let target = match session.as_ref() {
            Some(id) => entries.iter().find(|e| &e.session_id == id).cloned(),
            None => entries.first().cloned(),
        };
        let Some(target) = target else {
            self.set_view(|v| v.status = "session not found".to_string());
            return Ok(());
        };
        if let Ok(mut filter) = session_filter.lock() {
            *filter = Some(target.session_id.clone());
        }
        self.handle
            .set_active_session(Some(target.session_id.clone()));
        self.set_view(|v| {
            v.sessions = entries.clone();
            v.title = target.title.clone();
            v.status = target.status.clone();
        });

        // subscribe; live events now flow through the reader thread.
        let subscribed = self
            .request(
                &mut stdin,
                "session/subscribe",
                json!({
                    "sessionId": target.session_id,
                    "deliveryKind": "desktop-continuous",
                }),
            )
            .map_err(|e| format!("session/subscribe failed: {e}"))?;
        if let Some(events) = subscribed.get("events").and_then(Value::as_array) {
            for envelope in events {
                if envelope.get("sessionId").and_then(Value::as_str)
                    == Some(target.session_id.as_str())
                {
                    if let Some(event) = map_event(envelope) {
                        self.set_view(|v| v.apply(&event));
                    }
                }
            }
        }
        self.set_view(|v| v.status = "subscribed".to_string());

        // Hold the connection open until stop or child exit. The command
        // queue is drained non-blocking before every poll sleep, so the
        // loop never waits more than POLL_INTERVAL_MS on the channel.
        while !self.handle.stop.load(Ordering::Relaxed) {
            if reader_done.load(Ordering::Acquire) {
                self.set_view(|v| v.link = LinkState::Down("app-server stdout closed".into()));
                return Err("app-server stdout closed".to_string());
            }
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.set_view(|v| v.link = LinkState::Down("app-server exited".into()));
                    return Err("app-server exited".to_string());
                }
                Ok(None) => {}
                Err(e) => return Err(format!("wait failed: {e}")),
            }
            while let Some(cmd) = self.handle.try_take_cmd() {
                self.handle_cmd(&mut stdin, &target.session_id, cmd);
            }
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }

    /// Handle one UI command. The run loop owns stdin, so the wire
    /// request is issued here, serialised with setup traffic by `&mut
    /// ChildStdin`. The loop blocks on the response (up to
    /// `REQUEST_TIMEOUT`), never on the command channel itself.
    fn handle_cmd(&self, stdin: &mut ChildStdin, session_id: &str, cmd: AgentCmd) {
        let AgentCmd::SendText {
            line,
            wire,
            session_id: queued_session_id,
            ticket,
        } = cmd;
        if queued_session_id != session_id {
            if let Some(ticket) = ticket {
                if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
                    lifecycle.cancel(ticket);
                }
            }
            self.set_view(|v| v.status = "dropped stale send — session changed".to_string());
            return;
        }
        if !wire {
            // No --allow-send: admission-shaped no-op, nothing on the wire.
            return;
        }
        let Some(ticket) = ticket else {
            self.set_view(|v| v.status = "send unavailable — admission missing".to_string());
            return;
        };
        let result = self.request(
            stdin,
            "session/send",
            json!({ "sessionId": session_id, "content": line }),
        );
        match result {
            Ok(res) if res.get("accepted").and_then(Value::as_bool) == Some(true) => {
                self.update_send_response(ticket, true);
            }
            Ok(_) => self.update_send_response(ticket, false),
            // Server errors (e.g. "-32010: A prompt is already running
            // for this session") surface as status text, never a crash.
            Err(err) => {
                self.update_send_response(ticket, false);
                self.set_view(|v| v.status = err);
            }
        }
    }

    fn update_send_response(&self, ticket: u64, accepted: bool) {
        let status = self
            .send_lifecycle
            .lock()
            .ok()
            .and_then(|mut lifecycle| lifecycle.response(ticket, accepted));
        if let Some(status) = status {
            self.set_view(|v| v.status = status.to_string());
        }
    }
}

/// Reader loop: route responses to pending requests, apply
/// `session/event` notifications to the view. Other notifications are
/// inert; a non-JSON line means the child lacks the protocol boundary.
fn read_frames(
    reader: BufReader<std::process::ChildStdout>,
    pending: PendingMap,
    view: Arc<Mutex<SessionView>>,
    dirty: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    session_filter: Arc<Mutex<Option<String>>>,
    send_lifecycle: Arc<Mutex<SendLifecycle>>,
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
                    // Keep the code in the status text: "-32010: …" is the
                    // contract for "a prompt is already running".
                    Err(
                        match (
                            err.get("code").and_then(Value::as_i64),
                            err.get("message").and_then(Value::as_str),
                        ) {
                            (Some(code), Some(m)) => format!("{code}: {m}"),
                            (_, Some(m)) => m.to_string(),
                            (_, None) => err.to_string(),
                        },
                    )
                } else {
                    Ok(msg["result"].clone())
                };
                let _ = sender.send(payload);
            }
            continue;
        }
        if msg.get("method").and_then(Value::as_str) == Some("session/event") {
            let envelope = msg.get("params").unwrap_or(&Value::Null);
            let matches_session = session_filter
                .lock()
                .ok()
                .and_then(|filter| filter.clone())
                .is_none_or(|id| {
                    envelope.get("sessionId").and_then(Value::as_str) == Some(id.as_str())
                });
            if matches_session {
                if let (Some(session_id), Some(kind)) = (
                    envelope.get("sessionId").and_then(Value::as_str),
                    envelope.get("type").and_then(Value::as_str),
                ) {
                    if let Ok(mut lifecycle) = send_lifecycle.lock() {
                        lifecycle.event(session_id, kind);
                    }
                }
                if let Some(event) = map_event(envelope) {
                    if let Ok(mut v) = view.lock() {
                        v.apply(&event);
                    }
                    dirty.store(true, Ordering::Relaxed);
                }
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
        "tool.updated" => map_tool_update(payload),
        _ => None,
    }
}

/// Wave 5 (GY-ZCODE-005): decode one `tool.updated` envelope payload
/// (drowzy `zcodeToolUpdatedEventPayloadSchema`, strict base fields):
/// `toolCallId` (required), `toolName?`, `description?`, and a `kind` of
/// scheduled/started/progress/completed/failed. Unknown kinds and missing
/// ids are inert. The display name is toolName, else description, else a
/// short form of the id; the raw id travels in `ProjEvent::ToolUpdate.id`
/// for the view's dedup (kept separate from the display name on purpose:
/// ids must be unique even when names collide).
fn map_tool_update(payload: &Value) -> Option<ProjEvent> {
    let id = payload.get("toolCallId").and_then(Value::as_str)?;
    if id.is_empty() {
        return None;
    }
    let state = match payload.get("kind").and_then(Value::as_str) {
        Some("scheduled") => ToolState::Scheduled,
        // Progress folds into Started: the view opens the row once and
        // ignores repeated Started events for the same id.
        Some("started") | Some("progress") => ToolState::Started,
        Some("completed") => ToolState::Completed,
        Some("failed") => ToolState::Failed,
        // Unknown kinds are inert (schema may grow).
        _ => return None,
    };
    let tool = payload
        .get("toolName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            payload
                .get("description")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .map(str::to_string)
        .unwrap_or_else(|| short_tool_id(id));
    Some(ProjEvent::ToolUpdate {
        id: id.to_string(),
        tool,
        state,
    })
}

/// Short display form of a tool call id: the last 8 chars (call ids are
/// typically prefixed), or the whole id when shorter.
fn short_tool_id(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() <= 8 {
        id.to_string()
    } else {
        chars[chars.len() - 8..].iter().collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admit_send_allows_one_and_drops_the_rest() {
        let mut lifecycle = SendLifecycle::default();
        let first = lifecycle.admit("sess_a");
        assert!(first.is_some(), "first send is admitted with a ticket");
        assert!(
            lifecycle.admit("sess_a").is_none(),
            "second send while pending is dropped"
        );
        // Terminal response frees admission.
        lifecycle.response(first.unwrap(), true);
        lifecycle.event("sess_a", "turn.started");
        lifecycle.event("sess_a", "turn.completed");
        assert!(lifecycle.admit("sess_a").is_some(), "admission re-opens");
    }

    #[test]
    fn command_queue_round_trips_and_survives_clone() {
        let handle = MonitorHandle::new();
        handle.set_active_session(Some("sess_a".to_string()));
        let clone = handle.clone();
        clone.send_text("hello".to_string(), true);
        let cmd = handle.try_take_cmd();
        assert!(
            matches!(
                &cmd,
                Some(AgentCmd::SendText {
                    line,
                    wire: true,
                    session_id,
                    ticket: Some(_)
                }) if line == "hello" && session_id == "sess_a"
            ),
            "admitted send carries session and ticket: {cmd:?}"
        );
        // Queue drained.
        assert_eq!(handle.try_take_cmd(), None);
    }

    #[test]
    fn command_queue_carries_wire_gate() {
        let handle = MonitorHandle::new();
        handle.set_active_session(Some("sess_a".to_string()));
        handle.send_text("nope".to_string(), false);
        assert!(
            matches!(
                handle.try_take_cmd(),
                Some(AgentCmd::SendText {
                    wire: false,
                    ticket: None,
                    ..
                })
            ),
            "gated send flows with wire=false and no ticket"
        );
    }

    // --- Wave 5 (GY-ZCODE-005): tool.updated decoding ---

    #[test]
    fn tool_updated_maps_kinds_and_display_names() {
        let env = |kind: &str| {
            serde_json::json!({
                "type": "tool.updated",
                "payload": {
                    "kind": kind,
                    "toolCallId": "call_abcdefghij",
                    "toolName": "read_file",
                }
            })
        };
        assert_eq!(
            map_event(&env("started")),
            Some(ProjEvent::ToolUpdate {
                id: "call_abcdefghij".into(),
                tool: "read_file".into(),
                state: ToolState::Started,
            })
        );
        // Progress folds onto Started.
        assert_eq!(
            map_event(&env("progress")),
            map_event(&env("started")),
            "progress must fold into Started"
        );
        assert_eq!(
            map_event(&env("completed")),
            Some(ProjEvent::ToolUpdate {
                id: "call_abcdefghij".into(),
                tool: "read_file".into(),
                state: ToolState::Completed,
            })
        );
        // Unknown kinds are inert.
        assert_eq!(map_event(&env("archived")), None);
        // Missing toolCallId is inert.
        assert_eq!(
            map_event(&serde_json::json!({
                "type": "tool.updated",
                "payload": {"kind": "started"},
            })),
            None
        );
    }

    #[test]
    fn tool_updated_display_falls_back_description_then_short_id() {
        let payload = |mut extra: Value| {
            if extra.is_null() {
                serde_json::json!({"kind": "started", "toolCallId": "call_1234567890abcdef"})
            } else {
                let obj = extra.as_object_mut().expect("object payload");
                obj.insert("kind".into(), "started".into());
                obj.insert("toolCallId".into(), "call_1234567890abcdef".into());
                extra
            }
        };
        let ev = map_event(&payload(serde_json::json!({"description": "reading src"})));
        assert_eq!(
            ev,
            Some(ProjEvent::ToolUpdate {
                id: "call_1234567890abcdef".into(),
                tool: "reading src".into(),
                state: ToolState::Started,
            })
        );
        let ev = map_event(&payload(Value::Null));
        assert_eq!(
            ev.and_then(|e| match e {
                ProjEvent::ToolUpdate { tool, .. } => Some(tool),
                _ => None,
            })
            .as_deref(),
            Some("90abcdef"),
            "short id form is the last 8 chars"
        );
    }
}
