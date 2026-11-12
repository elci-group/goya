//! Owns the desktop-continuous v4 app-server connection, command admission,
//! durable conversation recovery, and projection updates for the monitor.

use crate::composer::ComposerSnapshot;
use crate::protocol::{
    self, CommandOutcome, ConversationProjection, FrameAssembler, SessionSummary,
    SessionsIndexProjection,
};
use crate::view::{LinkState, SessionEntry, SessionTranscript, SessionView};
#[path = "transport.rs"]
mod transport;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{
    channel, sync_channel, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError,
    TrySendError,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};
pub use transport::HostAttachmentDescriptor;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_DELAY_MS: u64 = 5_000;
const POLL_INTERVAL_MS: u64 = 200;
const COMMAND_QUEUE_CAPACITY: usize = 8;
const DEFERRED_FRAME_CAPACITY: usize = 64;

/// Wave 7 (GY-ZCODE-007): retention capacity of the per-session
/// transcript/composer LRU; the oldest session (by first-saved order) is
/// evicted past the cap.
pub const RETENTION_CAP: usize = 8;

/// Wave 7: bounded LRU of per-session snapshots, insertion-ordered via a
/// side `VecDeque` (no external LRU crate). Re-saving a session moves it
/// to the newest slot.
#[derive(Debug, Default)]
struct SessionRetention {
    map: HashMap<String, (SessionTranscript, ComposerSnapshot)>,
    order: VecDeque<String>,
}

impl SessionRetention {
    fn store(&mut self, session_id: String, snap: (SessionTranscript, ComposerSnapshot)) {
        if let Some(pos) = self.order.iter().position(|k| k == &session_id) {
            self.order.remove(pos);
        }
        self.map.insert(session_id.clone(), snap);
        self.order.push_back(session_id);
        while self.order.len() > RETENTION_CAP {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.map.remove(&oldest);
        }
    }

    fn take(&mut self, session_id: &str) -> Option<(SessionTranscript, ComposerSnapshot)> {
        if let Some(pos) = self.order.iter().position(|k| k == session_id) {
            self.order.remove(pos);
        }
        self.map.remove(session_id)
    }
}

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
        session_id: String,
        ticket: Option<u64>,
        command_id: String,
        client_id: String,
        delivery: SendDelivery,
        connection_epoch: u64,
        base_revision: u64,
    },
    /// Any other idempotent v4 session command. Session id is captured by the
    /// UI caller and checked again by the agent thread before it is written.
    SessionCommand {
        session_id: String,
        command_id: String,
        client_id: String,
        command_type: String,
        payload: Value,
        connection_epoch: u64,
        base_revision: u64,
    },
    /// Wave 6 (GY-ZCODE-006): switch the pinned session. The run loop
    /// tears down the current run/connection (works while up, down or
    /// reconnecting) and starts a fresh run pinned to `session_id`
    /// (fresh `session/list` + `session/subscribe`). Any in-flight send
    /// is abandoned — its lifecycle is reset and the status hint becomes
    /// `"<n>/<m> switching session…"` (see [`MonitorHandle::switch_session`]).
    SwitchSession {
        session_id: String,
        connection_epoch: u64,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SendDelivery {
    #[default]
    Auto,
    StartNow,
    Queue,
    Guide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowupMode {
    Queue,
    Guide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionAction {
    Accept,
    Decline,
    Cancel,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionAnswer {
    pub option_id: Option<String>,
    pub free_text: Option<String>,
    pub action: Option<InteractionAction>,
    pub content: Option<serde_json::Map<String, Value>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingSend {
    ticket: u64,
    session_id: String,
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
        self.pending = None;
        Some(if accepted {
            "sending"
        } else {
            "send rejected by CLI"
        })
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
    worker: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    /// Inbound command queue: `SendText` and friends. Sender and receiver
    /// both sit behind Arc'd mutexes so every `MonitorHandle` clone can
    /// enqueue and the queue survives agent-thread reconnects.
    cmd_tx: SyncSender<AgentCmd>,
    cmd_rx: Arc<Mutex<Receiver<AgentCmd>>>,
    active_session_id: Arc<Mutex<Option<String>>>,
    /// Local transport lifetime. Queued UI commands are invalid after any
    /// reconnect, even when the new connection selects the same session.
    connection_epoch: Arc<AtomicU64>,
    send_lifecycle: Arc<Mutex<SendLifecycle>>,
    /// Wave 6: session the run loop pins the next (re)connection to.
    /// Consulted by the reconnect loop on every iteration, so a switch
    /// takes effect whether the connection is up, down or reconnecting.
    pinned_session: Arc<Mutex<Option<String>>>,
    /// Wave 6: set by `switch_session` so the reconnect loop skips the
    /// backoff delay and re-connects immediately.
    switch_requested: Arc<AtomicBool>,
    /// Wave 7: per-session transcript + composer LRU (cap
    /// [`RETENTION_CAP`], oldest-inserted evicted). Written by
    /// `switch_session` (save the outgoing session), drained by
    /// `on_session_acquired` (restore the incoming one).
    retention: Arc<Mutex<SessionRetention>>,
    /// Wave 7: composer snapshot staged by `on_session_acquired` for the
    /// host binary (the composer instance lives in the bin, not on the
    /// handle) to pick up via `take_composer_restore`.
    pending_composer: Arc<Mutex<Option<ComposerSnapshot>>>,
}

impl MonitorHandle {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(COMMAND_QUEUE_CAPACITY);
        Self {
            view: Arc::new(Mutex::new(SessionView::default())),
            dirty: Arc::new(AtomicBool::new(true)),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Arc::new(Mutex::new(None)),
            cmd_tx: tx,
            cmd_rx: Arc::new(Mutex::new(rx)),
            active_session_id: Arc::new(Mutex::new(None)),
            connection_epoch: Arc::new(AtomicU64::new(0)),
            send_lifecycle: Arc::new(Mutex::new(SendLifecycle::default())),
            pinned_session: Arc::new(Mutex::new(None)),
            switch_requested: Arc::new(AtomicBool::new(false)),
            retention: Arc::new(Mutex::new(SessionRetention::default())),
            pending_composer: Arc::new(Mutex::new(None)),
        }
    }

    /// Stop the connection owner and reap its app-server before window exit.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        let worker = self.worker.lock().ok().and_then(|mut slot| slot.take());
        if let Some(worker) = worker {
            let _ = worker.join();
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
    /// Synchronously reserve the one outbound slot and enqueue a command.
    /// `false` means no command was enqueued (disabled, unavailable, or
    /// already busy); it does not mean the CLI rejected a command.
    pub fn send_text(&self, line: String, wire: bool) -> bool {
        self.enqueue_text(line, wire, SendDelivery::Auto)
    }

    pub fn send_text_with_delivery(&self, line: String, delivery: SendDelivery) -> bool {
        self.enqueue_text(line, true, delivery)
    }

    fn enqueue_text(&self, line: String, wire: bool, delivery: SendDelivery) -> bool {
        let session_id = self
            .active_session_id
            .lock()
            .ok()
            .and_then(|active| active.clone());
        let Some(session_id) = session_id else {
            self.set_status("send unavailable — no active session");
            return false;
        };
        if !wire {
            self.set_status("send disabled — draft retained");
            return false;
        }
        let command_id = protocol::new_command_id();
        let client_id = protocol::client_id();
        let admitted = self
            .send_lifecycle
            .lock()
            .ok()
            .and_then(|mut lifecycle| lifecycle.admit(&session_id));
        let Some(ticket) = admitted else {
            self.set_status("busy — send pending");
            return false;
        };
        match self.cmd_tx.try_send(AgentCmd::SendText {
            line,
            session_id,
            ticket: Some(ticket),
            command_id,
            client_id,
            delivery,
            connection_epoch: self.connection_epoch.load(Ordering::Acquire),
            base_revision: self.projection_revision(),
        }) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
                    lifecycle.cancel(ticket);
                }
                self.set_status("busy — send queue full");
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
                    lifecycle.cancel(ticket);
                }
                self.set_status("send unavailable");
                false
            }
        }
    }

    pub fn set_followup_mode(&self, mode: FollowupMode) -> bool {
        self.enqueue_session_command(
            "setFollowupMode",
            json!({"mode": match mode { FollowupMode::Queue => "queue", FollowupMode::Guide => "guide" }}),
            false,
            &[],
        )
    }

    pub fn set_auto_drain(&self, auto_drain: bool) -> bool {
        self.enqueue_session_command(
            "setAutoDrain",
            json!({"autoDrain": auto_drain}),
            false,
            &["queue_auto_drain"],
        )
    }

    pub fn delete_queue_item(&self, queue_item_id: String) -> bool {
        if queue_item_id.trim().is_empty() {
            self.set_status("queue action unavailable — missing item");
            return false;
        }
        let control_id = self
            .view
            .lock()
            .ok()
            .and_then(|view| {
                view.workflow
                    .queue_items
                    .iter()
                    .position(|item| item.queue_item_id == queue_item_id)
                    .map(|index| format!("queue_remove_{index:02}"))
            })
            .unwrap_or_default();
        let pending_ids = [control_id.as_str()];
        let pending_controls: &[&str] = if control_id.is_empty() {
            &[]
        } else {
            &pending_ids
        };
        self.enqueue_session_command(
            "deleteQueueItem",
            json!({"queueItemId": queue_item_id}),
            false,
            pending_controls,
        )
    }

    pub fn resolve_interaction(&self, interaction_id: String, answer: InteractionAnswer) -> bool {
        if interaction_id.trim().is_empty() {
            self.set_status("interaction action unavailable — missing id");
            return false;
        }
        let mut answer_payload = serde_json::Map::new();
        if let Some(value) = answer.option_id {
            answer_payload.insert("optionId".into(), Value::String(value));
        }
        if let Some(value) = answer.free_text {
            answer_payload.insert("freeText".into(), Value::String(value));
        }
        if let Some(value) = answer.action {
            answer_payload.insert(
                "action".into(),
                Value::String(
                    match value {
                        InteractionAction::Accept => "accept",
                        InteractionAction::Decline => "decline",
                        InteractionAction::Cancel => "cancel",
                    }
                    .into(),
                ),
            );
        }
        if let Some(value) = answer.content {
            answer_payload.insert("content".into(), Value::Object(value));
        }
        self.enqueue_session_command(
            "resolveInteraction",
            json!({
                "interactionId": interaction_id,
                "answer": Value::Object(answer_payload)
            }),
            true,
            &[],
        )
    }

    pub fn stop_session(&self) -> bool {
        let execution_id = {
            let view = self.view.lock().ok();
            view.and_then(|view| {
                view.workflow
                    .control
                    .active_works
                    .first()
                    .and_then(|work| work.foreground_execution_id.clone())
            })
        };
        let payload = match execution_id {
            Some(id) => json!({ "expectedForegroundExecutionId": id }),
            None => json!({}),
        };
        self.enqueue_session_command("stop", payload, false, &["stop"])
    }

    pub fn pause_goal(&self) -> bool {
        self.enqueue_session_command("pauseGoal", json!({}), false, &["goal_pause"])
    }

    pub fn resume_goal(&self) -> bool {
        self.enqueue_session_command("resumeGoal", json!({}), false, &["goal_resume"])
    }

    pub fn compact_session(&self) -> bool {
        self.enqueue_session_command("compact", json!({}), false, &["compact_session"])
    }

    pub fn send_queued_now(&self, queue_item_id: String) -> bool {
        if queue_item_id.trim().is_empty() {
            self.set_status("queue action unavailable — missing item");
            return false;
        }
        let control_id = self
            .view
            .lock()
            .ok()
            .and_then(|view| {
                view.workflow
                    .queue_items
                    .iter()
                    .position(|item| item.queue_item_id == queue_item_id)
                    .map(|index| format!("queue_now_{index:02}"))
            })
            .unwrap_or_default();
        self.enqueue_pending_command(
            "sendQueuedNow",
            json!({ "queueItemId": queue_item_id }),
            &control_id,
        )
    }

    pub fn reorder_queue_item(&self, queue_item_id: String, before_queue_item_id: String) -> bool {
        if queue_item_id.trim().is_empty() || before_queue_item_id.trim().is_empty() {
            self.set_status("queue action unavailable — missing item");
            return false;
        }
        let control_id = self
            .view
            .lock()
            .ok()
            .and_then(|view| {
                view.workflow
                    .queue_items
                    .iter()
                    .position(|item| item.queue_item_id == queue_item_id)
                    .map(|index| format!("queue_earlier_{index:02}"))
            })
            .unwrap_or_default();
        self.enqueue_pending_command(
            "reorderQueueItem",
            json!({
                "queueItemId": queue_item_id,
                "beforeQueueItemId": before_queue_item_id
            }),
            &control_id,
        )
    }

    pub fn cancel_background_work(&self, work_id: String) -> bool {
        if work_id.trim().is_empty() {
            self.set_status("background action unavailable — missing work");
            return false;
        }
        let control_id = self
            .view
            .lock()
            .ok()
            .and_then(|view| {
                view.workflow
                    .background_works
                    .iter()
                    .position(|work| work.work_id == work_id)
                    .map(|index| format!("work_cancel_{index:02}"))
            })
            .unwrap_or_default();
        self.enqueue_pending_command(
            "cancelBackgroundWork",
            json!({ "workId": work_id }),
            &control_id,
        )
    }

    fn enqueue_pending_command(
        &self,
        command_type: &str,
        payload: Value,
        control_id: &str,
    ) -> bool {
        let pending_ids = [control_id];
        let pending_controls: &[&str] = if control_id.is_empty() {
            &[]
        } else {
            &pending_ids
        };
        self.enqueue_session_command(command_type, payload, false, pending_controls)
    }

    pub fn clear_command_pending(&self, command_id: &str) {
        if let Ok(mut view) = self.view.lock() {
            view.clear_command_pending(command_id);
        }
    }

    fn enqueue_session_command(
        &self,
        command_type: &str,
        payload: Value,
        pending_answers: bool,
        pending_controls: &[&str],
    ) -> bool {
        let session_id = self
            .active_session_id
            .lock()
            .ok()
            .and_then(|active| active.clone());
        let Some(session_id) = session_id else {
            self.set_status("action unavailable — no active session");
            return false;
        };
        let command_id = protocol::new_command_id();
        // 先记下在途按钮再入队。命令结果只按自己的 command id 解除，
        // 不会把后发出的另一条命令提前重新打开。
        if let Ok(mut view) = self.view.lock() {
            if pending_answers {
                view.mark_answer_controls_pending(&command_id);
            }
            for id in pending_controls {
                view.mark_control_pending(&command_id, id);
            }
        }
        match self.cmd_tx.try_send(AgentCmd::SessionCommand {
            session_id,
            command_id: command_id.clone(),
            client_id: protocol::client_id(),
            command_type: command_type.into(),
            payload,
            connection_epoch: self.connection_epoch.load(Ordering::Acquire),
            base_revision: self.projection_revision(),
        }) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                self.clear_command_pending(&command_id);
                self.set_status("action unavailable — command queue full");
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.clear_command_pending(&command_id);
                self.set_status("action unavailable");
                false
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
            if *active != session_id {
                if let Ok(mut view) = self.view.lock() {
                    view.projection_revision = 0;
                }
            }
            *active = session_id;
        }
    }

    fn projection_revision(&self) -> u64 {
        self.view
            .lock()
            .map(|view| view.projection_revision)
            .unwrap_or_default()
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

    /// Wave 6/7: switch the pinned session (`GY-ZCODE-006`). This is the
    /// single owner of the switch/retain/reset sequence:
    /// 1. retain the outgoing session — its transcript is snapshotted from
    ///    the view and, with the caller-supplied composer snapshot, stored
    ///    in the LRU keyed by the active session id (nothing is stored
    ///    when no session is active — there is no key);
    /// 2. pin `session_id` — the reconnect loop starts the next run there;
    /// 3. abandon any in-flight send (the `SendLifecycle` pending entry is
    ///    reset — the stale "sending"/"busy" hint is replaced below);
    /// 4. reset the transcript for instant feedback (the incoming
    ///    session's retained state is restored by `on_session_acquired`);
    /// 5. set the `"<n>/<m> switching session…"` status;
    /// 6. flag the reconnect loop to skip its backoff;
    /// 7. enqueue `AgentCmd::SwitchSession` so a live connection tears
    ///    down within one poll interval.
    pub fn switch_session(&self, session_id: String, composer: ComposerSnapshot) {
        // Wave 7: snapshot BEFORE the reset, keyed by the outgoing session.
        let outgoing = self
            .active_session_id
            .lock()
            .ok()
            .and_then(|active| active.clone());
        if let Some(from) = outgoing {
            let transcript = self.view.lock().ok().map(|v| v.snapshot_transcript());
            if let Some(transcript) = transcript {
                if let Ok(mut retention) = self.retention.lock() {
                    retention.store(from, (transcript, composer));
                }
            }
        }
        if let Ok(mut pinned) = self.pinned_session.lock() {
            *pinned = Some(session_id.clone());
        }
        if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
            lifecycle.disconnect(); // abandon the in-flight send
        }
        if let Ok(mut view) = self.view.lock() {
            view.reset_transcript();
            view.projection_revision = 0;
            view.status = view.switching_status();
        }
        self.switch_requested.store(true, Ordering::Relaxed);
        if self
            .cmd_tx
            .try_send(AgentCmd::SwitchSession {
                session_id,
                connection_epoch: self.connection_epoch.load(Ordering::Acquire),
            })
            .is_err()
        {
            self.set_status("switch unavailable — queue full");
        }
    }

    /// Wave 6: the session the next run is pinned to.
    pub fn pinned_session(&self) -> Option<String> {
        self.pinned_session.lock().ok().and_then(|p| p.clone())
    }

    /// Wave 7 (GY-ZCODE-007): run-loop hook, called once a run acquires
    /// `session_id` (after `session/list` resolved the target, before the
    /// subscribe replay so replayed events append to the restored
    /// history). A retained snapshot is restored into the view and its
    /// composer half staged via `stage_composer_restore`; an unknown
    /// session leaves the default (empty) transcript untouched — the
    /// `switch_session` reset already put the view in that state.
    pub fn on_session_acquired(&self, session_id: &str) {
        let retained = self
            .retention
            .lock()
            .ok()
            .and_then(|mut r| r.take(session_id));
        if let Some((transcript, composer)) = retained {
            if let Ok(mut view) = self.view.lock() {
                view.restore_transcript(transcript);
            }
            if let Ok(mut pending) = self.pending_composer.lock() {
                *pending = Some(composer);
            }
            self.mark_dirty();
        }
    }

    /// Wave 7: take the composer snapshot staged by the last
    /// `on_session_acquired` (if any). The composer instance lives in the
    /// host binary, so the restore itself is the bin's job: it applies
    /// `Composer::restore` to its own composer (focus stays wherever the
    /// switch left it — unfocused).
    pub fn take_composer_restore(&self) -> Option<ComposerSnapshot> {
        self.pending_composer.lock().ok().and_then(|mut p| p.take())
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
    child: Option<Child>,
    socket: Option<UnixStream>,
    connection_epoch: u64,
    next_id: AtomicU64,
    pending: PendingMap,
    send_lifecycle: Arc<Mutex<SendLifecycle>>,
    connection_id: String,
    frame_overflow: Arc<AtomicBool>,
}

impl Drop for AgentRun {
    fn drop(&mut self) {
        // Every early return (including protocol errors) must reap app-server.
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.socket.take();
        if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
            lifecycle.disconnect();
        }
        self.handle.set_active_session(None);
        let _ = self.handle.connection_epoch.compare_exchange(
            self.connection_epoch,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
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
        let connection_epoch = handle.connection_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        Ok(Self {
            workspace: workspace.to_path_buf(),
            handle,
            child: Some(child),
            socket: None,
            connection_epoch,
            next_id: AtomicU64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
            send_lifecycle,
            connection_id: protocol::new_connection_id(),
            frame_overflow: Arc::new(AtomicBool::new(false)),
        })
    }

    fn attach(descriptor_path: &Path, handle: MonitorHandle) -> Result<Self, String> {
        let descriptor = HostAttachmentDescriptor::load(descriptor_path)?;
        let socket = descriptor.connect(&protocol::client_id())?;
        let connection_epoch = handle.connection_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        Ok(Self {
            workspace: PathBuf::from(&descriptor.workspace_path),
            handle,
            child: None,
            socket: Some(socket),
            connection_epoch,
            next_id: AtomicU64::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
            send_lifecycle: Arc::new(Mutex::new(SendLifecycle::default())),
            connection_id: protocol::new_connection_id(),
            frame_overflow: Arc::new(AtomicBool::new(false)),
        })
    }

    fn set_view(&self, f: impl FnOnce(&mut SessionView)) {
        if let Ok(mut view) = self.handle.view.lock() {
            f(&mut view);
        }
        self.handle.mark_dirty();
    }

    /// Send one request and await its response (routed back by the reader
    /// thread through `pending`). Serialised by the connection writer.
    fn request(&self, stdin: &mut dyn Write, method: &str, params: Value) -> Result<Value, String> {
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
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            // 关闭窗口必须中断等待 ACK，随后由 AgentRun::drop 回收子进程。
            if self.handle.stop.load(Ordering::Acquire) || Instant::now() >= deadline {
                self.pending
                    .lock()
                    .expect("pending map poisoned")
                    .remove(&id);
                return Err(format!("{method} cancelled or timed out"));
            }
            match rx.recv_timeout(
                Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
            ) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err("reader thread gone".into()),
            }
        }
    }

    fn run(mut self, session: Option<String>) -> Result<(), String> {
        let (mut stdin, stdout): (Box<dyn Write + Send>, Box<dyn BufRead + Send>) =
            if let Some(child) = self.child.as_mut() {
                let stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| "app-server stdin unavailable".to_string())?;
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| "app-server stdout unavailable".to_string())?;
                (Box::new(stdin), Box::new(BufReader::new(stdout)))
            } else if let Some(socket) = self.socket.take() {
                let writer = socket
                    .try_clone()
                    .map_err(|error| format!("clone Host attachment writer: {error}"))?;
                (Box::new(writer), Box::new(BufReader::new(socket)))
            } else {
                return Err("agent transport is unavailable".into());
            };

        // Reader thread owns stdout. RPC replies go to pending requests and
        // v4 physical frames are queued for the single agent-thread reducer.
        let reader_pending = Arc::clone(&self.pending);
        let reader_stop = Arc::clone(&self.handle.stop);
        let reader_done = Arc::new(AtomicBool::new(false));
        let thread_done = Arc::clone(&reader_done);
        let (frame_tx, frame_rx) = sync_channel(64);
        let reader_overflow = Arc::clone(&self.frame_overflow);
        let mut deferred_frames = VecDeque::new();
        if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
            lifecycle.disconnect();
        }
        std::thread::spawn(move || {
            read_frames(
                stdout,
                reader_pending,
                reader_stop,
                reader_overflow,
                frame_tx,
            );
            thread_done.store(true, Ordering::Release);
        });

        // Local fallback: identity is the trimmed workspace path. The path
        // remains separate from the identity key for file operations.
        let ws_path = self.workspace.to_string_lossy().to_string();
        let ws_key = protocol::workspace_key(&ws_path, None);
        let index_topic = format!("sessions-index/{ws_key}");
        let index_ack_value = self
            .request(
                &mut stdin,
                "v4/conversation/subscribe",
                protocol::subscribe_params(&index_topic, &self.connection_id, &ws_path, &ws_key),
            )
            .map_err(|e| format!("sessions-index subscribe failed: {e}"))?;
        let index_ack = protocol::parse_subscribe_ack(&index_ack_value)
            .map_err(|e| format!("invalid sessions-index ACK: {e}"))?;
        let mut assembler = FrameAssembler::default();
        let index_frame = wait_for_frame(
            &frame_rx,
            &mut assembler,
            &mut deferred_frames,
            &index_topic,
            &index_ack.subscription_id,
            &self.frame_overflow,
            &self.handle.stop,
        )
        .map_err(|e| format!("sessions-index initial frame failed: {e}"))?;
        let mut index = SessionsIndexProjection {
            subscription_id: index_ack.subscription_id.clone(),
            ..Default::default()
        };
        index
            .apply_frame(&index_frame)
            .map_err(|e| format!("invalid sessions-index frame: {e}"))?;
        if index.log_epoch != index_ack.log_epoch {
            return Err("sessions-index snapshot epoch does not match its ACK".into());
        }
        self.set_view(|v| v.link = LinkState::Up);
        let mut entries = index.sessions.iter().map(session_entry).collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            let left_activity = index
                .sessions
                .iter()
                .find(|item| item.session_id == left.session_id)
                .map(|item| item.last_activity_at)
                .unwrap_or_default();
            let right_activity = index
                .sessions
                .iter()
                .find(|item| item.session_id == right.session_id)
                .map(|item| item.last_activity_at)
                .unwrap_or_default();
            right_activity.cmp(&left_activity)
        });
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
        self.handle
            .set_active_session(Some(target.session_id.clone()));
        self.set_view(|v| {
            v.sessions = entries.clone();
            v.title = target.title.clone();
            v.status = target.status.clone();
        });
        // Wave 7: restore any retained transcript/composer for the newly
        // acquired session (must precede the subscribe replay so replayed
        // events append to the restored history).
        self.handle.on_session_acquired(&target.session_id);

        // The conversation snapshot is the durable transcript source; its
        // ACK is followed by owned physical frames on stdout.
        let conversation_topic = format!("conversation/{}", target.session_id);
        let subscribed = self
            .request(
                &mut stdin,
                "v4/conversation/subscribe",
                protocol::subscribe_params(
                    &conversation_topic,
                    &self.connection_id,
                    &ws_path,
                    &ws_key,
                ),
            )
            .map_err(|e| format!("conversation subscribe failed: {e}"))?;
        let conversation_ack = protocol::parse_subscribe_ack(&subscribed)
            .map_err(|e| format!("invalid conversation ACK: {e}"))?;
        let initial = wait_for_frame(
            &frame_rx,
            &mut assembler,
            &mut deferred_frames,
            &conversation_topic,
            &conversation_ack.subscription_id,
            &self.frame_overflow,
            &self.handle.stop,
        )
        .map_err(|e| format!("conversation initial frame failed: {e}"))?;
        let mut conversation = ConversationProjection::new(&target.session_id, &conversation_ack);
        conversation
            .apply_frame(&initial)
            .map_err(|e| format!("invalid conversation snapshot: {e}"))?;
        self.hydrate_older_rows(&mut stdin, &mut conversation)?;
        self.set_view(|view| {
            view.replace_projected_lines(&conversation.display_lines(), &conversation.status);
            view.replace_workflow(conversation.workflow_state());
            view.projection_revision = conversation.revision;
        });

        // Hold the connection open until stop or child exit. The command
        // queue is drained non-blocking before every poll sleep, so the
        // loop never waits more than POLL_INTERVAL_MS on the channel.
        while !self.handle.stop.load(Ordering::Relaxed) {
            if self.frame_overflow.swap(false, Ordering::AcqRel) {
                self.resync_sessions_index(
                    &mut stdin,
                    &frame_rx,
                    &mut deferred_frames,
                    &mut assembler,
                    &mut index,
                )?;
                self.resync_conversation(
                    &mut stdin,
                    &frame_rx,
                    &mut deferred_frames,
                    &mut assembler,
                    &mut conversation,
                )?;
                self.hydrate_older_rows(&mut stdin, &mut conversation)?;
                self.set_view(|view| {
                    view.replace_projected_lines(
                        &conversation.display_lines(),
                        &conversation.status,
                    );
                    view.replace_workflow(conversation.workflow_state());
                    view.projection_revision = conversation.revision;
                });
            }
            if reader_done.load(Ordering::Acquire) {
                self.set_view(|v| v.link = LinkState::Down("app-server stdout closed".into()));
                return Err("app-server stdout closed".to_string());
            }
            match self.child.as_mut().map(Child::try_wait).transpose() {
                Ok(Some(Some(_))) => {
                    self.set_view(|v| v.link = LinkState::Down("app-server exited".into()));
                    return Err("app-server exited".to_string());
                }
                Ok(Some(None)) | Ok(None) => {}
                Err(e) => return Err(format!("wait failed: {e}")),
            }
            while let Some(frame) = deferred_frames.pop_front() {
                self.apply_logical_frame(
                    &mut stdin,
                    &frame_rx,
                    &mut deferred_frames,
                    &mut assembler,
                    &mut index,
                    &index_topic,
                    &mut entries,
                    &mut conversation,
                    &conversation_topic,
                    frame,
                )?;
            }
            while let Ok(notification) = frame_rx.try_recv() {
                if notification.get("method").and_then(Value::as_str)
                    != Some("v4/conversation/frame")
                {
                    continue;
                }
                let wire = notification.get("params").unwrap_or(&Value::Null);
                match assembler.push(wire) {
                    Ok(Some(frame)) => self.apply_logical_frame(
                        &mut stdin,
                        &frame_rx,
                        &mut deferred_frames,
                        &mut assembler,
                        &mut index,
                        &index_topic,
                        &mut entries,
                        &mut conversation,
                        &conversation_topic,
                        frame,
                    )?,
                    Ok(None) => {}
                    Err(err) => {
                        let topic = wire
                            .get("topic")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let subscription_id = wire
                            .get("subscriptionId")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if topic == index_topic && subscription_id == index.subscription_id {
                            assembler.prepare_subscription_recovery(topic, subscription_id);
                            self.set_view(|view| {
                                view.status = format!("sessions-index frame rejected: {err}")
                            });
                            self.resync_sessions_index(
                                &mut stdin,
                                &frame_rx,
                                &mut deferred_frames,
                                &mut assembler,
                                &mut index,
                            )?;
                            entries = index.sessions.iter().map(session_entry).collect();
                            entries.sort_by(|left, right| {
                                let left_activity = index
                                    .sessions
                                    .iter()
                                    .find(|item| item.session_id == left.session_id)
                                    .map(|item| item.last_activity_at)
                                    .unwrap_or_default();
                                let right_activity = index
                                    .sessions
                                    .iter()
                                    .find(|item| item.session_id == right.session_id)
                                    .map(|item| item.last_activity_at)
                                    .unwrap_or_default();
                                right_activity.cmp(&left_activity)
                            });
                            self.set_view(|view| view.sessions = entries.clone());
                        } else if topic == conversation_topic
                            && subscription_id == conversation.subscription_id
                        {
                            assembler.prepare_subscription_recovery(topic, subscription_id);
                            self.set_view(|view| {
                                view.status = format!("conversation frame rejected: {err}")
                            });
                            self.resync_conversation(
                                &mut stdin,
                                &frame_rx,
                                &mut deferred_frames,
                                &mut assembler,
                                &mut conversation,
                            )?;
                            self.hydrate_older_rows(&mut stdin, &mut conversation)?;
                            self.set_view(|view| {
                                view.replace_projected_lines(
                                    &conversation.display_lines(),
                                    &conversation.status,
                                );
                                view.replace_workflow(conversation.workflow_state());
                                view.projection_revision = conversation.revision;
                            });
                        } else {
                            return Err(format!("unowned or stale protocol frame rejected: {err}"));
                        }
                    }
                }
            }
            while let Some(cmd) = self.handle.try_take_cmd() {
                if command_connection_epoch(&cmd) != self.connection_epoch {
                    if let AgentCmd::SendText {
                        ticket: Some(ticket),
                        ..
                    } = &cmd
                    {
                        if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
                            lifecycle.cancel(*ticket);
                        }
                    }
                    if let AgentCmd::SessionCommand { command_id, .. } = &cmd {
                        self.handle.clear_command_pending(command_id);
                    }
                    self.set_view(|view| {
                        view.status = "dropped stale action — transport changed".into()
                    });
                    continue;
                }
                match cmd {
                    // Wave 6: a switch tears the connection down; the
                    // reconnect loop restarts immediately, pinned to the
                    // new session. A switch to the already-pinned target
                    // is a no-op (it can linger in the queue across a
                    // reconnect).
                    AgentCmd::SwitchSession { session_id, .. } => {
                        if session_id != target.session_id {
                            if let Ok(mut view) = self.handle.view.lock() {
                                view.reset_transcript();
                                view.status = view.switching_status();
                            }
                            if let Some(child) = self.child.as_mut() {
                                let _ = child.kill();
                            }
                            return Ok(());
                        }
                    }
                    cmd => self.handle_cmd(&mut stdin, &target.session_id, cmd),
                }
            }
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
        }
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_logical_frame(
        &self,
        stdin: &mut dyn Write,
        frame_rx: &Receiver<Value>,
        deferred: &mut VecDeque<Value>,
        assembler: &mut FrameAssembler,
        index: &mut SessionsIndexProjection,
        index_topic: &str,
        entries: &mut Vec<SessionEntry>,
        conversation: &mut ConversationProjection,
        conversation_topic: &str,
        frame: Value,
    ) -> Result<(), String> {
        match frame["topic"].as_str() {
            Some(topic) if topic == index_topic => {
                if index.apply_frame(&frame).is_err() {
                    self.resync_sessions_index(stdin, frame_rx, deferred, assembler, index)?;
                }
                *entries = index.sessions.iter().map(session_entry).collect();
                entries.sort_by(|left, right| {
                    let left_activity = index
                        .sessions
                        .iter()
                        .find(|item| item.session_id == left.session_id)
                        .map(|item| item.last_activity_at)
                        .unwrap_or_default();
                    let right_activity = index
                        .sessions
                        .iter()
                        .find(|item| item.session_id == right.session_id)
                        .map(|item| item.last_activity_at)
                        .unwrap_or_default();
                    right_activity.cmp(&left_activity)
                });
                self.set_view(|view| view.sessions = entries.clone());
            }
            Some(topic) if topic == conversation_topic => {
                if frame["subscriptionId"].as_str() != Some(conversation.subscription_id.as_str()) {
                    return Ok(()); // Stale subscription: reject without touching state.
                }
                if let Err(err) = conversation.apply_frame(&frame) {
                    self.set_view(|view| view.status = format!("protocol frame rejected: {err}"));
                    self.resync_conversation(stdin, frame_rx, deferred, assembler, conversation)?;
                }
                self.set_view(|view| {
                    view.replace_projected_lines(
                        &conversation.display_lines(),
                        &conversation.status,
                    );
                    view.replace_workflow(conversation.workflow_state());
                    view.projection_revision = conversation.revision;
                });
            }
            _ => {} // A connection-scoped notification for another owned topic.
        }
        Ok(())
    }

    fn resync_sessions_index(
        &self,
        stdin: &mut dyn Write,
        frame_rx: &Receiver<Value>,
        deferred: &mut VecDeque<Value>,
        assembler: &mut FrameAssembler,
        index: &mut SessionsIndexProjection,
    ) -> Result<(), String> {
        let mut force_snapshot = false;
        for attempt in 0..2 {
            let params = json!({
                "topic": format!("sessions-index/{}", index.workspace_id),
                "connectionId": self.connection_id,
                "subscriptionId": index.subscription_id,
                "base": if force_snapshot { Value::Null } else { json!({"logEpoch": index.log_epoch, "seq": index.seq}) },
                "forceSnapshot": force_snapshot,
            });
            let response = self
                .request(stdin, "v4/conversation/resync", params)
                .map_err(|err| format!("sessions-index resync failed: {err}"))?;
            let ack = protocol::parse_subscribe_ack(&response)
                .map_err(|err| format!("invalid sessions-index resync ACK: {err}"))?;
            if ack.subscription_id != index.subscription_id {
                return Err("sessions-index resync changed subscription".into());
            }
            let topic = format!("sessions-index/{}", index.workspace_id);
            let frame = wait_for_frame(
                frame_rx,
                assembler,
                deferred,
                &topic,
                &index.subscription_id,
                &self.frame_overflow,
                &self.handle.stop,
            )
            .map_err(|err| format!("sessions-index resync frame failed: {err}"))?;
            match index.apply_frame(&frame) {
                Ok(()) if index.log_epoch == ack.log_epoch => return Ok(()),
                _ if attempt == 0 => force_snapshot = true,
                Err(err) => return Err(format!("sessions-index resync rejected: {err}")),
                _ => return Err("sessions-index resync epoch mismatch".into()),
            }
        }
        Err("sessions-index resync exhausted".into())
    }

    fn hydrate_older_rows(
        &self,
        stdin: &mut dyn Write,
        projection: &mut ConversationProjection,
    ) -> Result<(), String> {
        while projection.row_count() < protocol::MAX_ROWS {
            let Some(before) = projection.first_row_id() else {
                break;
            };
            let result = self
                .request(
                    stdin,
                    "v4/conversation/rowsRange",
                    protocol::rows_range_query(&projection.session_id, before, 200),
                )
                .map_err(|err| format!("conversation rowsRange failed: {err}"))?;
            let has_more = result["hasMore"].as_bool().unwrap_or(false);
            let row_count = result["rows"].as_array().map(Vec::len).unwrap_or_default();
            match projection.merge_rows_range(&result) {
                Ok(true) => {}
                Ok(false) => break, // Query waterline raced a projection update; do not mix epochs.
                Err(err) => return Err(format!("invalid conversation rowsRange: {err}")),
            }
            if !has_more || row_count == 0 {
                break;
            }
        }
        Ok(())
    }

    fn resync_conversation(
        &self,
        stdin: &mut dyn Write,
        frame_rx: &Receiver<Value>,
        deferred: &mut VecDeque<Value>,
        assembler: &mut FrameAssembler,
        projection: &mut ConversationProjection,
    ) -> Result<(), String> {
        let base = json!({ "logEpoch": projection.log_epoch, "seq": projection.seq });
        let mut force_snapshot = false;
        for attempt in 0..2 {
            let params = json!({
                "topic": projection.topic,
                "connectionId": self.connection_id,
                "subscriptionId": projection.subscription_id,
                "base": if force_snapshot { Value::Null } else { base.clone() },
                "forceSnapshot": force_snapshot,
            });
            let response = self
                .request(stdin, "v4/conversation/resync", params)
                .map_err(|err| format!("conversation resync failed: {err}"))?;
            let ack = protocol::parse_subscribe_ack(&response)
                .map_err(|err| format!("invalid resync ACK: {err}"))?;
            if ack.subscription_id != projection.subscription_id {
                return Err("resync changed owned subscription id".into());
            }
            projection.log_epoch = ack.log_epoch;
            let frame = wait_for_frame(
                frame_rx,
                assembler,
                deferred,
                &projection.topic,
                &projection.subscription_id,
                &self.frame_overflow,
                &self.handle.stop,
            )
            .map_err(|err| format!("conversation resync frame failed: {err}"))?;
            match projection.apply_frame(&frame) {
                Ok(()) => return Ok(()),
                Err(_) if attempt == 0 => force_snapshot = true,
                Err(err) => return Err(format!("resync snapshot rejected: {err}")),
            }
        }
        Err("conversation resync exhausted".into())
    }

    /// Handle one UI command. The run loop owns stdin, so the wire
    /// request is issued here, serialised with setup traffic by `&mut
    /// writer`. The loop blocks on the response (up to
    /// `REQUEST_TIMEOUT`), never on the command channel itself.
    fn handle_cmd(&self, stdin: &mut dyn Write, session_id: &str, cmd: AgentCmd) {
        match cmd {
            AgentCmd::SendText {
                line,
                session_id: queued_session_id,
                ticket,
                command_id,
                client_id,
                delivery,
                base_revision,
                ..
            } => {
                if queued_session_id != session_id {
                    if let Some(ticket) = ticket {
                        if let Ok(mut lifecycle) = self.send_lifecycle.lock() {
                            lifecycle.cancel(ticket);
                        }
                    }
                    self.set_view(|v| {
                        v.status = "dropped stale send — session changed".to_string()
                    });
                    return;
                }
                let Some(ticket) = ticket else {
                    self.set_view(|v| {
                        v.status = "send unavailable — admission missing".to_string()
                    });
                    return;
                };
                let mut payload = protocol::send_text_command(
                    &command_id,
                    &client_id,
                    session_id,
                    base_revision,
                    &line,
                );
                match delivery {
                    SendDelivery::Auto => {}
                    SendDelivery::StartNow => {
                        payload["payload"]["requestedDelivery"] = json!("startNow")
                    }
                    SendDelivery::Queue => payload["payload"]["requestedDelivery"] = json!("queue"),
                    SendDelivery::Guide => payload["payload"]["requestedDelivery"] = json!("guide"),
                }
                match self.execute_command(stdin, session_id, &command_id, payload) {
                    Ok(outcome) => self.apply_command_outcome(ticket, &outcome),
                    Err(error) => {
                        self.update_send_response(ticket, false);
                        self.set_view(|view| view.status = error);
                    }
                }
            }
            AgentCmd::SessionCommand {
                session_id: queued_session_id,
                command_id,
                client_id,
                command_type,
                payload,
                base_revision,
                ..
            } => {
                if queued_session_id != session_id {
                    self.handle.clear_command_pending(&command_id);
                    self.set_view(|view| {
                        view.status = "dropped stale action — session changed".into()
                    });
                    return;
                }
                let envelope = command_envelope(
                    &command_id,
                    &client_id,
                    session_id,
                    base_revision,
                    &command_type,
                    payload,
                );
                match self.execute_command(stdin, session_id, &command_id, envelope) {
                    Ok(outcome) => {
                        self.handle.clear_command_pending(&command_id);
                        self.set_view(|view| {
                            view.status =
                                format!("command {}: {}", outcome.command_id, outcome.status)
                        });
                    }
                    Err(error) => {
                        self.handle.clear_command_pending(&command_id);
                        self.set_view(|view| view.status = error);
                    }
                }
            }
            AgentCmd::SwitchSession { .. } => {}
        }
    }

    fn execute_command(
        &self,
        stdin: &mut dyn Write,
        session_id: &str,
        command_id: &str,
        envelope: Value,
    ) -> Result<CommandOutcome, String> {
        match self.request(stdin, "v4/command", envelope) {
            Ok(response) => protocol::parse_command_ack(&response, command_id)
                .map_err(|error| format!("command ACK rejected: {error}")),
            Err(error) => self
                .request(
                    stdin,
                    "v4/commands/query",
                    protocol::commands_query(session_id, command_id),
                )
                .and_then(|value| {
                    protocol::parse_command_query(&value, session_id, command_id)
                        .map_err(|parse_error| parse_error.to_string())
                })
                .and_then(|outcome| {
                    outcome.ok_or_else(|| format!("{error}; command outcome unknown — not resent"))
                }),
        }
    }

    fn apply_command_outcome(&self, ticket: u64, outcome: &CommandOutcome) {
        let accepted = matches!(outcome.status.as_str(), "accepted" | "duplicate");
        self.update_send_response(ticket, accepted);
        if !accepted {
            let status = outcome
                .reason_code
                .as_deref()
                .unwrap_or(&outcome.status)
                .to_string();
            self.set_view(|view| view.status = format!("command {}: {status}", outcome.status));
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

/// Reader loop: route replies to pending requests and enqueue only v4 topic
/// frames. Legacy session/event notifications are intentionally not a fallback.
fn read_frames(
    reader: Box<dyn BufRead + Send>,
    pending: PendingMap,
    stop: Arc<AtomicBool>,
    overflow: Arc<AtomicBool>,
    frame_tx: SyncSender<Value>,
) {
    let mut reader = reader;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let line = match read_bounded_line(&mut reader) {
            Ok(Some(line)) => line,
            Ok(None) | Err(_) => return,
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(msg) = serde_json::from_slice::<Value>(&line) else {
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
        if msg.get("method").and_then(Value::as_str) == Some("v4/conversation/frame") {
            match frame_tx.try_send(msg) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => overflow.store(true, Ordering::Release),
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
    }
}

fn read_bounded_line(reader: &mut impl BufRead) -> std::io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }
        let take = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if line.len().saturating_add(take) > protocol::MAX_FRAME_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "NDJSON frame exceeds v4 physical limit",
            ));
        }
        let ends_line = available.get(take.saturating_sub(1)) == Some(&b'\n');
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if ends_line {
            return Ok(Some(line));
        }
    }
}

fn wait_for_frame(
    frame_rx: &Receiver<Value>,
    assembler: &mut FrameAssembler,
    deferred: &mut VecDeque<Value>,
    topic: &str,
    subscription_id: &str,
    overflow: &AtomicBool,
    stop: &AtomicBool,
) -> Result<Value, String> {
    if let Some(position) = deferred.iter().position(|frame| {
        frame.get("topic").and_then(Value::as_str) == Some(topic)
            && frame.get("subscriptionId").and_then(Value::as_str) == Some(subscription_id)
    }) {
        return Ok(deferred.remove(position).expect("position exists"));
    }
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    loop {
        if stop.load(Ordering::Acquire) {
            return Err("subscription cancelled".into());
        }
        if overflow.load(Ordering::Acquire) {
            return Err("v4 frame queue overflow; reconnect to recover topic cursors".into());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("initial v4 frame timed out".into());
        }
        let notification = match frame_rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(notification) => notification,
            Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => continue,
            Err(RecvTimeoutError::Timeout) => return Err("initial v4 frame timed out".into()),
            Err(RecvTimeoutError::Disconnected) => return Err("v4 frame reader stopped".into()),
        };
        if notification.get("method").and_then(Value::as_str) != Some("v4/conversation/frame") {
            continue;
        }
        let wire = notification
            .get("params")
            .ok_or_else(|| "v4 frame missing params".to_string())?;
        if let Some(frame) = assembler.push(wire).map_err(|err| err.to_string())? {
            if frame.get("topic").and_then(Value::as_str) == Some(topic)
                && frame.get("subscriptionId").and_then(Value::as_str) == Some(subscription_id)
            {
                return Ok(frame);
            }
            // Keep other already-owned topics for the agent-thread reducer;
            // initial conversation setup must not silently lose index deltas.
            if deferred.len() == DEFERRED_FRAME_CAPACITY {
                return Err(
                    "deferred v4 topic frame capacity exceeded; reconnect to recover".into(),
                );
            }
            deferred.push_back(frame);
        }
    }
}

fn session_entry(summary: &SessionSummary) -> SessionEntry {
    SessionEntry {
        session_id: summary.session_id.clone(),
        title: summary.title.clone(),
        status: summary.status.clone(),
    }
}

fn command_connection_epoch(command: &AgentCmd) -> u64 {
    match command {
        AgentCmd::SendText {
            connection_epoch, ..
        }
        | AgentCmd::SessionCommand {
            connection_epoch, ..
        }
        | AgentCmd::SwitchSession {
            connection_epoch, ..
        } => *connection_epoch,
    }
}

fn command_envelope(
    command_id: &str,
    client_id: &str,
    session_id: &str,
    revision: u64,
    command_type: &str,
    payload: Value,
) -> Value {
    json!({
        "inputOrigin": "desktop",
        "commandId": command_id,
        "clientId": client_id,
        "sessionId": session_id,
        "baseRevision": revision,
        "type": command_type,
        "payload": payload,
        "issuedAt": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    })
}

/// Spawn the CLI app-server and keep the view fed, reconnecting with
/// backoff until `stop`. Intended to run on its own thread.
pub fn spawn_agent(cli: &str, workspace: &Path, session: Option<String>) -> MonitorHandle {
    let handle = MonitorHandle::new();
    let cli = cli.to_string();
    let workspace = workspace.to_path_buf();
    let h = handle.clone();
    let worker = std::thread::spawn(move || loop {
        if h.stop.load(Ordering::Relaxed) {
            return;
        }
        // Wave 6: consult the pinned session on every iteration, so a
        // switch applies whether the connection is up, down or
        // reconnecting. `--session` seeds the initial pin.
        let pinned = h.pinned_session().or_else(|| session.clone());
        match AgentRun::spawn(&cli, &workspace, h.clone()) {
            Ok(run) => {
                if let Err(error) = run.run(pinned.clone()) {
                    if let Ok(mut view) = h.view.lock() {
                        view.link = LinkState::Down(error);
                    }
                    h.mark_dirty();
                }
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
            // Wave 6: a session switch skips the reconnect backoff.
            if h.switch_requested.swap(false, Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    *handle.worker.lock().expect("worker slot poisoned") = Some(worker);
    handle
}

/// Attach to the existing window-scoped Host described by `descriptor_path`.
/// The descriptor is re-read for each reconnect because the Host rotates its
/// single-use capability after a successful attachment. This path never
/// launches a CLI or creates a second runtime.
pub fn spawn_host_attached_agent(descriptor_path: &Path, session: Option<String>) -> MonitorHandle {
    let handle = MonitorHandle::new();
    let descriptor_path = descriptor_path.to_path_buf();
    let h = handle.clone();
    let worker = std::thread::spawn(move || loop {
        if h.stop.load(Ordering::Relaxed) {
            return;
        }
        let pinned = h.pinned_session().or_else(|| session.clone());
        match AgentRun::attach(&descriptor_path, h.clone()) {
            Ok(run) => {
                if let Err(error) = run.run(pinned.clone()) {
                    if let Ok(mut view) = h.view.lock() {
                        view.link = LinkState::Down(error);
                    }
                    h.mark_dirty();
                }
            }
            Err(error) => {
                if let Ok(mut view) = h.view.lock() {
                    view.link = LinkState::Down(error);
                }
                h.mark_dirty();
            }
        }
        for _ in 0..RECONNECT_DELAY_MS / 100 {
            if h.stop.load(Ordering::Relaxed) {
                return;
            }
            if h.switch_requested.swap(false, Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    *handle.worker.lock().expect("worker slot poisoned") = Some(worker);
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
            "second send before ACK is dropped"
        );
        // CLI ACK closes the transport slot; future inputs go to CLI admission.
        lifecycle.response(first.unwrap(), true);
        assert!(
            lifecycle.admit("sess_a").is_some(),
            "admission re-opens after ACK"
        );
    }

    #[test]
    fn command_queue_round_trips_and_survives_clone() {
        let handle = MonitorHandle::new();
        handle.set_active_session(Some("sess_a".to_string()));
        let clone = handle.clone();
        assert!(clone.send_text("hello".to_string(), true));
        let cmd = handle.try_take_cmd();
        assert!(
            matches!(
                &cmd,
                Some(AgentCmd::SendText {
                    line,
                    session_id,
                    ticket: Some(_),
                    command_id,
                    client_id,
                    ..
                }) if line == "hello" && session_id == "sess_a"
                    && !command_id.is_empty() && !client_id.is_empty()
            ),
            "admitted send carries session and ticket: {cmd:?}"
        );
        // Queue drained.
        assert_eq!(handle.try_take_cmd(), None);
    }

    #[test]
    fn send_disabled_or_unavailable_retains_command_as_draft() {
        let handle = MonitorHandle::new();
        handle.set_active_session(Some("sess_a".to_string()));
        assert!(!handle.send_text("nope".to_string(), false));
        assert_eq!(handle.try_take_cmd(), None);
        handle.set_active_session(None);
        assert!(!handle.send_text("also retained".to_string(), true));
        assert_eq!(handle.try_take_cmd(), None);
    }

    #[test]
    fn native_controls_capture_session_and_preserve_canonical_payloads() {
        let handle = MonitorHandle::new();
        handle.set_active_session(Some("sess_a".into()));
        handle.connection_epoch.store(19, Ordering::Release);
        if let Ok(mut view) = handle.view.lock() {
            view.projection_revision = 19;
        }
        assert!(handle.send_text_with_delivery("queued line".into(), SendDelivery::Queue));
        let send = handle.try_take_cmd().unwrap();
        assert!(matches!(
            &send,
            AgentCmd::SendText {
                session_id,
                delivery: SendDelivery::Queue,
                connection_epoch: 19,
                ..
            } if session_id == "sess_a"
        ));
        assert_ne!(
            command_connection_epoch(&send),
            20,
            "reconnected runs reject queued old intents"
        );

        assert!(handle.set_followup_mode(FollowupMode::Guide));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { session_id, command_type, payload, .. })
                if session_id == "sess_a" && command_type == "setFollowupMode" && payload["mode"] == "guide"
        ));
        assert!(handle.set_auto_drain(true));
        if let Ok(mut view) = handle.view.lock() {
            view.projection_revision = 20;
        }
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, base_revision: 19, .. })
                if command_type == "setAutoDrain" && payload["autoDrain"] == true
        ));
        assert!(handle.delete_queue_item("queue-1".into()));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "deleteQueueItem" && payload["queueItemId"] == "queue-1"
        ));
        assert!(handle.resolve_interaction(
            "interaction-1".into(),
            InteractionAnswer {
                option_id: Some("allow_once".into()),
                action: Some(InteractionAction::Accept),
                ..Default::default()
            }
        ));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "resolveInteraction"
                    && payload["interactionId"] == "interaction-1"
                    && payload["answer"]["optionId"] == "allow_once"
                    && payload["answer"]["action"] == "accept"
        ));
    }

    #[test]
    fn session_operation_commands_use_canonical_payloads_without_splicing_the_queue() {
        use crate::view::{WorkflowActiveWork, WorkflowBackgroundWork, WorkflowQueueItem};

        let handle = MonitorHandle::new();
        handle.set_active_session(Some("sess_a".into()));
        {
            let mut view = handle.view.lock().unwrap();
            view.projection_revision = 4;
            view.sessions.push(SessionEntry {
                session_id: "sess_a".into(),
                title: String::new(),
                status: String::new(),
            });
            view.workflow.control.can_stop = true;
            view.workflow.control.stop_state = "stoppable".into();
            view.workflow.control.active_works.push(WorkflowActiveWork {
                kind: "primaryTurn".into(),
                foreground_execution_id: Some("exec-1".into()),
            });
            view.workflow.queue_items = vec![
                WorkflowQueueItem {
                    queue_item_id: "queue-1".into(),
                    text: "next".into(),
                    delivery: "queue".into(),
                },
                WorkflowQueueItem {
                    queue_item_id: "queue-2".into(),
                    text: "later".into(),
                    delivery: "queue".into(),
                },
            ];
            view.workflow.background_works.push(WorkflowBackgroundWork {
                work_id: "work-1".into(),
                kind: "bash".into(),
                title: "tests".into(),
                status: "running".into(),
                cancellable: true,
            });
        }
        let queue_before = handle.view.lock().unwrap().workflow.queue_items.clone();
        assert!(handle.stop_session());
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, base_revision: 4, .. })
                if command_type == "stop" && payload["expectedForegroundExecutionId"] == "exec-1"
        ));
        assert!(handle
            .view
            .lock()
            .unwrap()
            .workflow_controls()
            .iter()
            .any(|control| control.id == "queue_remove_00" && control.enabled));
        assert!(handle.reorder_queue_item("queue-2".into(), "queue-1".into()));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "reorderQueueItem"
                    && payload["queueItemId"] == "queue-2"
                    && payload["beforeQueueItemId"] == "queue-1"
        ));
        assert!(handle.send_queued_now("queue-1".into()));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "sendQueuedNow" && payload["queueItemId"] == "queue-1"
        ));
        assert!(handle.cancel_background_work("work-1".into()));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "cancelBackgroundWork" && payload["workId"] == "work-1"
        ));
        assert!(handle.pause_goal());
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "pauseGoal" && payload.as_object().is_some_and(|map| map.is_empty())
        ));
        assert!(handle.resume_goal());
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "resumeGoal" && payload.as_object().is_some_and(|map| map.is_empty())
        ));
        assert!(handle.compact_session());
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "compact" && payload.as_object().is_some_and(|map| map.is_empty())
        ));
        assert_eq!(
            handle.view.lock().unwrap().workflow.queue_items,
            queue_before
        );
        {
            let mut view = handle.view.lock().unwrap();
            view.workflow.control.active_works.clear();
            let workflow = view.workflow.clone();
            view.replace_workflow(workflow);
        }
        assert!(handle.stop_session());
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SessionCommand { command_type, payload, .. })
                if command_type == "stop" && payload.as_object().is_some_and(|map| map.is_empty())
        ));
    }

    // --- Wave 6 (GY-ZCODE-006): session switching ---

    #[test]
    fn switch_session_abandons_in_flight_and_enqueues_switch() {
        let handle = MonitorHandle::new();
        assert_eq!(handle.pinned_session(), None, "no pin by default");
        handle.set_active_session(Some("sess_a".to_string()));
        // Admit an in-flight wire send first.
        assert!(handle.send_text("hello".to_string(), true));
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SendText { .. })
        ));
        assert!(!handle.send_text("still pending".to_string(), true));
        // Switch: the in-flight send is abandoned, the lifecycle resets,
        // the view resets + shows the switching hint, the pin moves, the
        // backoff is skipped, and a SwitchSession command is enqueued.
        handle.switch_session("sess_b".to_string(), ComposerSnapshot::default());
        assert_eq!(handle.pinned_session().as_deref(), Some("sess_b"));
        assert!(
            handle
                .send_lifecycle
                .lock()
                .map(|l| l.pending.is_none())
                .unwrap_or(false),
            "in-flight send abandoned on switch"
        );
        assert!(
            handle.switch_requested.load(Ordering::Relaxed),
            "reconnect backoff must be skipped"
        );
        assert_eq!(
            handle.view.lock().map(|v| v.status.clone()).ok(),
            Some("switching session…".to_string()),
            "empty sessions list falls back to the bare hint"
        );
        assert!(
            matches!(
                handle.try_take_cmd(),
                Some(AgentCmd::SwitchSession { ref session_id, .. }) if session_id == "sess_b"
            ),
            "SwitchSession command enqueued"
        );
    }

    #[test]
    fn switch_session_resets_transcript_and_shows_index_hint() {
        let handle = MonitorHandle::new();
        if let Ok(mut view) = handle.view.lock() {
            view.sessions = (0..3)
                .map(|i| SessionEntry {
                    session_id: format!("s{i}"),
                    title: String::new(),
                    status: String::new(),
                })
                .collect();
            view.selected = 1;
            view.push_line("old transcript line");
        }
        handle.switch_session("s2".to_string(), ComposerSnapshot::default());
        let (status, tail_empty) = handle
            .view
            .lock()
            .map(|v| (v.status.clone(), v.tail.is_empty()))
            .unwrap_or_default();
        assert!(
            tail_empty,
            "agent-side switch resets the transcript defensively"
        );
        assert_eq!(status, "2/3 switching session…");
    }

    // --- Wave 7 (GY-ZCODE-007): per-session transcript + composer retention ---

    fn composer_with(text: &str) -> ComposerSnapshot {
        let mut c = crate::composer::Composer::default();
        c.focused = true;
        for ch in text.chars() {
            c.handle_char(ch);
        }
        c.snapshot()
    }

    fn seeded_handle(active: &str) -> MonitorHandle {
        let handle = MonitorHandle::new();
        handle.set_active_session(Some(active.to_string()));
        if let Ok(mut view) = handle.view.lock() {
            view.push_line(&format!("transcript of {active}"));
            view.push_line("second line");
        }
        handle
    }

    #[test]
    fn switch_away_and_back_retains_transcript_and_composer() {
        let handle = seeded_handle("sess_a");
        let composer_snap = composer_with("draft for a");

        // Switch away: sess_a's state is retained, the view resets.
        handle.switch_session("sess_b".to_string(), composer_snap);
        let clean = handle
            .view
            .lock()
            .map(|v| v.tail.is_empty() && v.history.is_empty())
            .unwrap_or(false);
        assert!(clean, "switch resets the view before restore");
        assert_eq!(handle.take_composer_restore(), None, "nothing staged yet");

        // Run acquires sess_b (never seen): default empty transcript.
        handle.on_session_acquired("sess_b");
        let default_state = handle.view.lock().ok().map(|v| v.transcript_is_empty());
        assert_eq!(default_state, Some(true));
        assert_eq!(handle.take_composer_restore(), None);

        // Switch back to sess_a (the bin snapshots its now-cleared
        // composer), then the run re-acquires sess_a: full restore.
        handle.set_active_session(Some("sess_b".to_string()));
        handle.switch_session("sess_a".to_string(), composer_with(""));
        handle.on_session_acquired("sess_a");
        let restored = handle
            .view
            .lock()
            .map(|v| (v.history.clone(), v.scroll_from_bottom() == 0))
            .ok();
        assert_eq!(
            restored,
            Some((
                vec![
                    "transcript of sess_a".to_string(),
                    "second line".to_string()
                ],
                true
            )),
            "transcript restored on return"
        );
        let composer = handle.take_composer_restore();
        assert!(
            matches!(&composer, Some(s) if s == &composer_with("draft for a")),
            "retained composer snapshot staged on restore: {composer:?}"
        );
        // Drained.
        assert_eq!(handle.take_composer_restore(), None);
    }

    #[test]
    fn unknown_session_restore_leaves_default_empty_transcript() {
        let handle = seeded_handle("sess_a");
        handle.switch_session("sess_b".to_string(), ComposerSnapshot::default());
        handle.on_session_acquired("never-seen");
        let state = handle.view.lock().ok().map(|v| v.transcript_is_empty());
        assert_eq!(
            state,
            Some(true),
            "no retained snapshot: view keeps the default empty transcript"
        );
        assert_eq!(handle.take_composer_restore(), None);
    }

    #[test]
    fn retention_lru_evicts_oldest_at_cap() {
        let handle = MonitorHandle::new();
        for i in 0..RETENTION_CAP {
            handle.set_active_session(Some(format!("s{i}")));
            if let Ok(mut view) = handle.view.lock() {
                view.reset_transcript();
                view.push_line(&format!("line of s{i}"));
            }
            handle.switch_session(format!("next{i}"), ComposerSnapshot::default());
        }
        let len = handle
            .retention
            .lock()
            .map(|r| (r.map.len(), r.order.len()))
            .unwrap_or((0, 0));
        assert_eq!(len, (RETENTION_CAP, RETENTION_CAP), "bounded at the cap");
        // Saving again without eviction refreshes recency: re-save s0,
        // then a 9th save evicts s1 (now the oldest), not s0.
        handle.set_active_session(Some("s0".to_string()));
        if let Ok(mut view) = handle.view.lock() {
            view.reset_transcript();
            view.push_line("refreshed");
        }
        handle.switch_session("resaved".to_string(), ComposerSnapshot::default());
        handle.set_active_session(Some("resaved".to_string()));
        handle.switch_session("s9th".to_string(), ComposerSnapshot::default());
        let keys = handle
            .retention
            .lock()
            .map(|r| r.order.clone())
            .unwrap_or_default();
        assert_eq!(keys.len(), RETENTION_CAP);
        assert!(
            !keys.contains(&"s1".to_string()),
            "oldest-inserted session evicted: {keys:?}"
        );
        assert!(
            keys.contains(&"s0".to_string()),
            "re-saved session stays: {keys:?}"
        );
        // The evicted session restores nothing.
        handle.on_session_acquired("s1");
        assert_eq!(handle.take_composer_restore(), None);
    }

    #[test]
    fn switch_without_active_session_still_switches_but_stores_nothing() {
        let handle = MonitorHandle::new();
        handle.switch_session("sess_x".to_string(), composer_with("typed"));
        assert_eq!(handle.pinned_session().as_deref(), Some("sess_x"));
        let len = handle.retention.lock().map(|r| r.map.len()).unwrap_or(0);
        assert_eq!(len, 0, "no active session — nothing to key the snapshot on");
        assert!(matches!(
            handle.try_take_cmd(),
            Some(AgentCmd::SwitchSession { ref session_id, .. }) if session_id == "sess_x"
        ));
    }
}
