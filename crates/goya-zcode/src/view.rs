//! Session projection — the single UI fact source for the monitor.
//!
//! The agent thread is the only writer; the render loop only reads
//! snapshots. All reduction logic is pure and unit-tested here.

use goya_runtime::ScrollState;
use std::collections::VecDeque;

/// Transcript tail capacity (matches `row_00`..`row_13` in the scene).
pub const TAIL_LINES: usize = 14;
/// Fixed wrap width in characters. Goya has no text wrapping (GY-ZCODE/1),
/// so lines are wrapped here before they reach the scene.
pub const LINE_WIDTH: usize = 56;
/// Scrollback capacity for committed lines (`history`); the oldest lines
/// are evicted once the cap is reached.
pub const HISTORY_CAP: usize = 2000;
/// Lines scrolled per PageUp/PageDown / PointerScroll step.
pub const SCROLL_STEP: usize = TAIL_LINES;
/// Wave 5 (GY-ZCODE-005): capacity of the in-flight / terminal tool-id
/// trackers used for `tool.updated` dedup (see [`SessionView`]).
pub const TOOL_TRACK_CAP: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Up,
    Down(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionEntry {
    pub session_id: String,
    pub title: String,
    pub status: String,
}

/// Neutral projection input; `agent` maps wire events onto these variants
/// so this module never touches JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjEvent {
    TurnStarted {
        input: String,
    },
    PartDelta {
        delta: String,
    },
    TurnCompleted {
        response: String,
        result: String,
    },
    TurnFailed {
        message: String,
    },
    /// Wave 5 (GY-ZCODE-005): one `tool.updated` wire event. `id` is the
    /// raw `toolCallId` and is the dedup key; `tool` is the display name
    /// (toolName, else description, else a short form of the id) chosen by
    /// `agent::map_event`. `progress` wire kinds are folded into
    /// [`ToolState::Started`] by the mapper — the monitor's line model is
    /// append-only and cannot update rows in place, so progress deltas are
    /// deliberately not rendered (frozen Wave 5 contract).
    ///
    /// Wave 10 (GY-ZCODE-010): optional `stats` — duration/output size as
    /// seen on the wire (`progress` payloads carry `elapsedMs`/`outputBytes`;
    /// terminal payloads may carry their own, else the view folds the
    /// last-seen progress stats into the terminal summary row).
    ToolUpdate {
        id: String,
        tool: String,
        state: ToolState,
        stats: Option<ToolStats>,
    },
}

/// Wave 10 (GY-ZCODE-010): execution stats for one tool call, extracted by
/// `agent::map_event` from the `tool.updated` payload (`elapsedMs`,
/// `outputBytes`). Both optional — omit what the wire did not provide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolStats {
    pub duration_ms: Option<u64>,
    pub output_bytes: Option<u64>,
}

/// Lifecycle state of a tool call in [`ProjEvent::ToolUpdate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolState {
    Scheduled,
    Started,
    Completed,
    Failed,
}

/// Wave 7 (GY-ZCODE-007): session-scoped transcript state, saved by
/// [`SessionView::snapshot_transcript`] on switch and restored by
/// [`SessionView::restore_transcript`] on return. Everything the switch
/// previously dropped via `reset_transcript` travels here; link state,
/// the sessions list, the selection index and the `markdown` toggle stay
/// with the view (list/global state, not transcript).
/// One in-flight tool in [`SessionView`]'s tracker: the dedup id plus the
/// last-seen [`ToolStats`] (Wave 10 — progress stats fold here so the
/// terminal row can summarise them even when the terminal payload itself
/// carries no stats).
#[derive(Clone, Debug, Default)]
struct TrackedTool {
    id: String,
    stats: Option<ToolStats>,
}

#[derive(Clone, Debug, Default)]
pub struct SessionTranscript {
    tail: VecDeque<String>,
    history: Vec<String>,
    streaming: String,
    scroll: ScrollState,
    in_flight_tools: VecDeque<TrackedTool>,
    finished_tools: VecDeque<String>,
}

#[derive(Clone, Debug)]
pub struct SessionView {
    pub link: LinkState,
    pub sessions: Vec<SessionEntry>,
    pub title: String,
    pub status: String,
    /// Committed transcript lines (already wrapped).
    pub tail: VecDeque<String>,
    /// Scrollback of committed lines, oldest first, capped at
    /// [`HISTORY_CAP`] (oldest evicted). `tail` is the live last-14
    /// window of this; `history` keeps the rest for scrolling.
    pub history: Vec<String>,
    /// Lines back from the newest committed line; `0` = live/bottom.
    /// New events never hijack the scroll position (see `apply`).
    scroll: ScrollState,
    /// In-flight assistant streaming buffer, flushed to `tail` in chunks.
    pub streaming: String,
    /// Wave 4 (GY-ZCODE-004): when on (default), turn-completed response
    /// lines route through the markdown renderer (see [`markdown`]).
    pub markdown: bool,
    /// Wave 5 (GY-ZCODE-005): tool ids with an open `⚙ …` row, oldest
    /// first; capped at [`TOOL_TRACK_CAP`] (oldest evicted). `progress`
    /// updates fold onto these instead of emitting duplicate lines.
    /// Wave 10: each entry carries the last-seen [`ToolStats`].
    in_flight_tools: VecDeque<TrackedTool>,
    /// Wave 5: tool ids that already emitted a terminal (`✓`/`✗`) line;
    /// anything after that is inert. Same cap/eviction as above.
    finished_tools: VecDeque<String>,
    /// Wave 6 (GY-ZCODE-006): index into `sessions` of the currently
    /// selected entry; moved by `cycle_selection` and kept across
    /// `reset_transcript` (the selection is list state, not transcript).
    pub selected: usize,
}

impl Default for SessionView {
    fn default() -> Self {
        Self {
            link: LinkState::Connecting,
            sessions: Vec::new(),
            title: String::from("zcode monitor"),
            status: String::from("connecting"),
            tail: VecDeque::new(),
            history: Vec::new(),
            scroll: ScrollState::default(),
            streaming: String::new(),
            markdown: true,
            in_flight_tools: VecDeque::new(),
            finished_tools: VecDeque::new(),
            selected: 0,
        }
    }
}

impl SessionView {
    /// Wrap and append a line to the tail and the scrollback, keeping
    /// both bounded (tail: `TAIL_LINES`, history: `HISTORY_CAP`).
    pub fn push_line(&mut self, line: &str) {
        let wrapped = wrap_line(line, LINE_WIDTH);
        if wrapped.is_empty() {
            return;
        }
        for piece in wrapped {
            self.history.push(piece.clone());
            self.tail.push_back(piece);
            if self.tail.len() > TAIL_LINES {
                self.tail.pop_front();
            }
        }
        if self.history.len() > HISTORY_CAP {
            // Drop the oldest overflow so scrollback stays bounded.
            let drop = self.history.len() - HISTORY_CAP;
            self.history.drain(0..drop);
        }
        self.update_scroll_geometry();
    }

    /// Append assistant markdown text: rendered to styled plain lines by
    /// [`crate::markdown::render_markdown`], then pushed line by line
    /// (blank separators are absorbed by `push_line`).
    pub fn push_markdown(&mut self, text: &str) {
        for line in crate::markdown::render_markdown(text, LINE_WIDTH) {
            self.push_line(&line);
        }
    }

    /// Scroll back `n` lines (PageUp); clamped so the topmost view still
    /// shows a full `TAIL_LINES` window (short histories cannot scroll).
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll.scroll_by(0.0, -(n as f64));
    }

    /// Scroll forward `n` lines (PageDown); `0` is live/bottom.
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll.scroll_by(0.0, n as f64);
    }

    /// Jump back to the live bottom.
    pub fn scroll_to_bottom(&mut self) {
        let (_, max) = self.scroll.max_offset();
        self.scroll.scroll_to(0.0, max);
    }

    /// Lines back from the newest committed line; zero means the live bottom.
    pub fn scroll_from_bottom(&self) -> usize {
        let (_, max) = self.scroll.max_offset();
        let (_, offset) = self.scroll.offset();
        (max - offset).round().max(0.0) as usize
    }

    fn update_scroll_geometry(&mut self) {
        let from_bottom = self.scroll_from_bottom();
        let max = self.history.len().saturating_sub(TAIL_LINES);
        self.scroll
            .set_geometry(1.0, self.history.len() as f64, 1.0, TAIL_LINES as f64);
        self.scroll
            .scroll_to(0.0, max.saturating_sub(from_bottom) as f64);
    }

    /// Flush the streaming buffer into the tail.
    pub fn flush_streaming(&mut self) {
        if self.streaming.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.streaming);
        self.push_line(&text);
    }

    /// Reduce one projection event. Unknown events are inert (never panic).
    pub fn apply(&mut self, event: &ProjEvent) {
        match event {
            ProjEvent::TurnStarted { input } => {
                // The wire-accepted "sending" status is confirmed here:
                // setting "running" clears any pending "sending" hint.
                self.flush_streaming();
                self.push_line(&format!("> {input}"));
                self.status = "running".to_string();
            }
            ProjEvent::PartDelta { delta } => {
                self.streaming.push_str(delta);
                // Chunk the buffer so long responses stream into the tail
                // instead of growing without bound.
                while self.streaming.chars().count() >= LINE_WIDTH * 3 {
                    let split_at = char_boundary_floor(&self.streaming, LINE_WIDTH * 2);
                    let chunk = self.streaming.split_off(split_at);
                    let drained = std::mem::replace(&mut self.streaming, chunk);
                    self.push_line(&drained);
                }
                self.status = "running".to_string();
            }
            ProjEvent::TurnCompleted { response, result } => {
                self.flush_streaming();
                if response.is_empty() {
                    self.push_line(&format!("[done: {result}]"));
                } else if self.markdown {
                    self.push_markdown(response);
                } else {
                    self.push_line(response);
                }
                self.status = if result == "success" || result == "cancelled" {
                    "idle".to_string()
                } else {
                    "error".to_string()
                };
            }
            ProjEvent::TurnFailed { message } => {
                self.flush_streaming();
                self.push_line(&format!("[error] {message}"));
                self.status = "error".to_string();
            }
            ProjEvent::ToolUpdate {
                id,
                tool,
                state,
                stats,
            } => {
                // Dedup: once a terminal line exists for this id, nothing
                // further is emitted; progress on an in-flight tool folds
                // into its existing row (append-only line model).
                if self.finished_tools.contains(id) {
                    return;
                }
                match state {
                    ToolState::Scheduled => {} // silent — avoid noise
                    ToolState::Started => {
                        if let Some(open) = self.in_flight_tools.iter_mut().find(|t| t.id == *id) {
                            // Wave 10: progress fold — refresh the
                            // last-seen stats, emit nothing new.
                            if stats.is_some() {
                                open.stats = *stats;
                            }
                            return; // progress fold / duplicate start
                        }
                        self.push_line(&format!("⚙ {tool} …"));
                        track_tool(
                            &mut self.in_flight_tools,
                            TrackedTool {
                                id: id.to_string(),
                                stats: *stats,
                            },
                        );
                    }
                    ToolState::Completed | ToolState::Failed => {
                        // Terminal lines only close tools we saw start;
                        // unsolicited completions render nothing.
                        let open = self.in_flight_tools.iter().position(|t| t.id == *id);
                        if let Some(pos) = open {
                            let tracked = self.in_flight_tools.remove(pos);
                            let mark = match state {
                                ToolState::Failed => "✗",
                                _ => "✓",
                            };
                            // Wave 10: terminal summary — the event's own
                            // stats win, else the last-seen progress stats
                            // carried by the tracker. Missing fields (or a
                            // fully missing summary) are omitted.
                            let summary = stats
                                .or(tracked.as_ref().and_then(|t| t.stats))
                                .and_then(|s| format_tool_stats(&s));
                            let line = match summary {
                                Some(s) => format!("⚙ {tool} {mark} ({s})"),
                                None => format!("⚙ {tool} {mark}"),
                            };
                            // The summary must never widen the row past
                            // the fixed line width (no wrapping on tool
                            // rows — clamp instead).
                            self.push_line(&truncate_chars(&line, LINE_WIDTH));
                            track_tool(&mut self.finished_tools, id.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Wave 6 (GY-ZCODE-006): move the selection by `delta` entries,
    /// wrapping in both directions. Returns the newly selected entry, or
    /// `None` when the sessions list is empty (selection unchanged).
    pub fn cycle_selection(&mut self, delta: isize) -> Option<&SessionEntry> {
        let len = self.sessions.len();
        if len == 0 {
            return None;
        }
        let idx = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
        self.selected = idx;
        self.sessions.get(idx)
    }

    /// Wave 7 (GY-ZCODE-007): capture the full session-scoped transcript
    /// state (tail, history, streaming buffer, scroll offset and the
    /// in-flight/finished tool trackers). Called by
    /// `MonitorHandle::switch_session` before `reset_transcript`.
    pub fn snapshot_transcript(&self) -> SessionTranscript {
        SessionTranscript {
            tail: self.tail.clone(),
            history: self.history.clone(),
            streaming: self.streaming.clone(),
            scroll: self.scroll,
            in_flight_tools: self.in_flight_tools.clone(),
            finished_tools: self.finished_tools.clone(),
        }
    }

    /// Wave 7: replace the transcript state with `t`. Link state, the
    /// sessions list, the selection index and the `markdown` toggle are
    /// deliberately kept (list/global state, not transcript).
    pub fn restore_transcript(&mut self, t: SessionTranscript) {
        self.tail = t.tail;
        self.history = t.history;
        self.streaming = t.streaming;
        self.scroll = t.scroll;
        self.in_flight_tools = t.in_flight_tools;
        self.finished_tools = t.finished_tools;
    }

    /// Wave 6: clear all session-scoped transcript state — tail, history,
    /// streaming buffer, scroll position and the in-flight/finished tool
    /// trackers. Called on session switch; Wave 7 snapshots the transcript
    /// BEFORE this reset so the state survives the switch in the
    /// per-session LRU. Link state, the sessions list, the selection
    /// index and the `markdown` toggle are deliberately kept.
    pub fn reset_transcript(&mut self) {
        self.tail.clear();
        self.history.clear();
        self.streaming.clear();
        self.scroll = ScrollState::default();
        self.in_flight_tools.clear();
        self.finished_tools.clear();
    }

    /// Wave 7: true when no transcript state is held (the post-
    /// `reset_transcript` / default shape). Used by the agent retention
    /// tests; the trackers are private to this module.
    #[cfg(test)]
    pub(crate) fn transcript_is_empty(&self) -> bool {
        self.tail.is_empty()
            && self.history.is_empty()
            && self.streaming.is_empty()
            && self.in_flight_tools.is_empty()
            && self.finished_tools.is_empty()
            && self.scroll_from_bottom() == 0
    }

    /// Wave 6: the status text shown while a switch is in flight —
    /// `"<n>/<m> switching session…"` (1-based index into the sessions
    /// list). Falls back to the bare hint when the list is empty.
    pub fn switching_status(&self) -> String {
        if self.sessions.is_empty() {
            return String::from("switching session…");
        }
        format!(
            "{}/{} switching session…",
            self.selected + 1,
            self.sessions.len()
        )
    }

    /// The exactly-`TAIL_LINES` rows rendered into the scene, newest last.
    /// At `scroll == 0` this is the live tail (plus streaming preview);
    /// when scrolled back it is the `TAIL_LINES` window ending at
    /// `history.len() - scroll` (clamped to the available lines).
    pub fn rows(&self) -> Vec<String> {
        let scroll = self.scroll_from_bottom();
        let mut rows: Vec<String> = if scroll == 0 {
            self.tail
                .iter()
                .map(|l| truncate_chars(l, LINE_WIDTH))
                .collect()
        } else {
            // Window ends at `len - scroll` (clamped so it never goes
            // past the oldest retained line).
            let end = self.history.len().saturating_sub(scroll);
            let start = end.saturating_sub(TAIL_LINES);
            self.history[start..end]
                .iter()
                .map(|l| truncate_chars(l, LINE_WIDTH))
                .collect()
        };
        // Live streaming preview on the last line while a turn runs
        // (only at the bottom — never while scrolled back).
        if !self.streaming.is_empty() && scroll == 0 && rows.len() < TAIL_LINES {
            let preview = self
                .streaming
                .chars()
                .rev()
                .take(LINE_WIDTH)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            rows.push(format!("...{}", preview));
        }
        while rows.len() < TAIL_LINES {
            rows.insert(0, String::new());
        }
        let start = rows.len().saturating_sub(TAIL_LINES);
        rows.split_off(start)
    }
}

/// Record a tool in a bounded tracker, evicting the oldest entry once
/// [`TOOL_TRACK_CAP`] is reached (duplicate ids are the caller's concern).
fn track_tool<T>(set: &mut VecDeque<T>, tool: T) {
    if set.len() >= TOOL_TRACK_CAP {
        set.pop_front();
    }
    set.push_back(tool);
}

/// Wave 10 (GY-ZCODE-010): render a tool summary — `(3.4s, 12.3KB)`.
/// Fields the wire did not provide are omitted; `None` overall when
/// nothing was provided (no summary at all).
fn format_tool_stats(stats: &ToolStats) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(ms) = stats.duration_ms {
        parts.push(format!("{:.1}s", ms as f64 / 1000.0));
    }
    if let Some(bytes) = stats.output_bytes {
        parts.push(format_bytes(bytes));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// Byte size with 1 decimal above the byte range: `0B`, `1023B`,
/// `1.0KB`, `1.0MB` (MB is the top unit — monitor rows never need more).
fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= MIB {
        format!("{:.1}MB", b / MIB)
    } else if b >= KIB {
        format!("{:.1}KB", b / KIB)
    } else {
        format!("{bytes}B")
    }
}

/// Greedy wrap at `width` characters, breaking mid-word when a single
/// token exceeds the width. ASCII-oriented; non-ASCII chars count as 1.
pub fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![line.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for word in line.split(' ') {
        let mut word = word;
        loop {
            let word_len = word.chars().count();
            let sep = usize::from(!current.is_empty());
            if current_len + sep + word_len <= width {
                if sep == 1 {
                    current.push(' ');
                }
                current.push_str(word);
                current_len += sep + word_len;
                break;
            }
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
                current_len = 0;
                continue;
            }
            // The word alone exceeds the width: hard-split it.
            let split_at = char_boundary_floor(word, width);
            let head = &word[..split_at];
            word = &word[split_at..];
            out.push(head.to_string());
            if word.is_empty() {
                break;
            }
        }
    }
    if !current.is_empty() || out.is_empty() {
        out.push(current);
    }
    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('~');
    out
}

/// Largest char boundary <= `max_chars`.
fn char_boundary_floor(s: &str, max_chars: usize) -> usize {
    s.char_indices()
        .nth(max_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_respects_width() {
        let lines = wrap_line("the quick brown fox jumps over the lazy dog", 10);
        assert!(lines.iter().all(|l| l.chars().count() <= 10));
        assert_eq!(
            lines.join(" "),
            "the quick brown fox jumps over the lazy dog"
        );
    }

    #[test]
    fn wrap_hard_splits_oversized_tokens() {
        let lines = wrap_line(&"x".repeat(25), 10);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| l.chars().count() <= 10));
    }

    #[test]
    fn tail_is_bounded_and_ordered() {
        let mut view = SessionView::default();
        for i in 0..40 {
            view.push_line(&format!("line {i:03}"));
        }
        let rows = view.rows();
        assert_eq!(rows.len(), TAIL_LINES);
        assert_eq!(rows[TAIL_LINES - 1], "line 039");
    }

    #[test]
    fn turn_lifecycle_updates_status_and_tail() {
        let mut view = SessionView::default();
        view.apply(&ProjEvent::TurnStarted {
            input: "fix the bug".into(),
        });
        assert_eq!(view.status, "running");
        view.apply(&ProjEvent::PartDelta {
            delta: "partial answer text".into(),
        });
        assert_eq!(view.streaming, "partial answer text");
        view.apply(&ProjEvent::TurnCompleted {
            response: "done".into(),
            result: "success".into(),
        });
        assert_eq!(view.status, "idle");
        assert!(view.streaming.is_empty());
        let rows = view.rows();
        assert_eq!(rows[TAIL_LINES - 1], "done");
        assert!(rows.contains(&"> fix the bug".to_string()));
    }

    #[test]
    fn failed_turn_marks_error() {
        let mut view = SessionView::default();
        view.apply(&ProjEvent::TurnFailed {
            message: "boom".into(),
        });
        assert_eq!(view.status, "error");
    }

    #[test]
    fn rows_always_fill_to_capacity() {
        let view = SessionView::default();
        assert_eq!(view.rows().len(), TAIL_LINES);
    }

    #[test]
    fn stick_to_bottom_at_live_scroll() {
        let mut view = SessionView::default();
        for i in 0..30 {
            view.push_line(&format!("line {i:02}"));
        }
        assert_eq!(view.scroll_from_bottom(), 0);
        let rows = view.rows();
        assert_eq!(rows[TAIL_LINES - 1], "line 29");
        assert_eq!(rows[TAIL_LINES - 2], "line 28");
        // New events keep the view pinned to the bottom.
        view.push_line("line 30");
        assert_eq!(view.scroll_from_bottom(), 0);
        assert_eq!(view.rows()[TAIL_LINES - 1], "line 30");
    }

    #[test]
    fn scroll_window_clamps_at_top() {
        let mut view = SessionView::default();
        for i in 0..30 {
            view.push_line(&format!("line {i:02}"));
        }
        // Way past the top: clamp so the window shows the oldest lines.
        view.scroll_up(10_000);
        let rows = view.rows();
        assert_eq!(rows[0], "line 00");
        assert_eq!(rows[TAIL_LINES - 1], "line 13");
        // Scrolled view comes from history, not the 14-line tail.
        assert_eq!(view.history.len(), 30);
    }

    #[test]
    fn scroll_down_returns_to_live() {
        let mut view = SessionView::default();
        for i in 0..30 {
            view.push_line(&format!("line {i:02}"));
        }
        view.scroll_up(SCROLL_STEP);
        assert_eq!(view.scroll_from_bottom(), SCROLL_STEP);
        assert_eq!(view.rows()[TAIL_LINES - 1], "line 15");
        view.scroll_down(SCROLL_STEP);
        assert_eq!(view.scroll_from_bottom(), 0);
        assert_eq!(view.rows()[TAIL_LINES - 1], "line 29");
        // scroll_to_bottom is a no-op at 0 and resets from anywhere.
        view.scroll_up(5);
        view.scroll_to_bottom();
        assert_eq!(view.scroll_from_bottom(), 0);
    }

    #[test]
    fn history_cap_evicts_oldest() {
        let mut view = SessionView::default();
        for i in 0..(HISTORY_CAP + 10) {
            view.push_line(&format!("line {i}"));
        }
        assert_eq!(view.history.len(), HISTORY_CAP);
        assert_eq!(view.history[0], format!("line 10"));
        assert_eq!(
            view.history[HISTORY_CAP - 1],
            format!("line {}", HISTORY_CAP + 9)
        );
    }

    #[test]
    fn scroll_preserved_across_delta_when_scrolled_back() {
        let mut view = SessionView::default();
        for i in 0..30 {
            view.push_line(&format!("line {i:02}"));
        }
        view.scroll_up(3);
        // Incoming deltas while scrolled back must not hijack the view.
        view.apply(&ProjEvent::PartDelta {
            delta: "streaming in the background".into(),
        });
        assert_eq!(view.scroll_from_bottom(), 3);
        let rows = view.rows();
        assert_eq!(rows[TAIL_LINES - 1], "line 26");
        assert!(!rows[TAIL_LINES - 1].contains("streaming"));
        // And at the bottom, the streaming preview still shows.
        view.scroll_to_bottom();
        view.push_line("settled");
        assert!(view.rows()[TAIL_LINES - 1] == "settled");
    }

    #[test]
    fn markdown_on_routes_turn_completed_through_renderer() {
        let mut view = SessionView::default();
        assert!(view.markdown);
        view.apply(&ProjEvent::TurnCompleted {
            response: "# heading\n- bullet".into(),
            result: "success".into(),
        });
        let rows = view.rows();
        assert!(rows.contains(&"■ HEADING".to_string()));
        assert!(rows.contains(&"• bullet".to_string()));
        // Markdown off falls back to raw push_line.
        let mut raw = SessionView {
            markdown: false,
            ..Default::default()
        };
        raw.apply(&ProjEvent::TurnCompleted {
            response: "# heading".into(),
            result: "success".into(),
        });
        assert!(raw.history.last().map(String::as_str) == Some("# heading"));
    }

    #[test]
    fn turn_started_confirms_sending_status() {
        let mut view = SessionView {
            status: "sending".to_string(),
            ..Default::default()
        };
        view.apply(&ProjEvent::TurnStarted { input: "hi".into() });
        assert_eq!(view.status, "running");
    }

    // --- Wave 5 (GY-ZCODE-005): tool-row lifecycle ---

    fn tool(id: &str, tool: &str, state: ToolState) -> ProjEvent {
        ProjEvent::ToolUpdate {
            id: id.to_string(),
            tool: tool.to_string(),
            state,
            stats: None,
        }
    }

    fn tool_with_stats(id: &str, tool: &str, state: ToolState, stats: ToolStats) -> ProjEvent {
        ProjEvent::ToolUpdate {
            id: id.to_string(),
            tool: tool.to_string(),
            state,
            stats: Some(stats),
        }
    }

    #[test]
    fn tool_started_then_completed_emits_exactly_two_lines() {
        let mut view = SessionView::default();
        view.apply(&tool("t1", "read_file", ToolState::Started));
        view.apply(&tool("t1", "read_file", ToolState::Completed));
        let gear: Vec<&String> = view.history.iter().filter(|l| l.contains('⚙')).collect();
        assert_eq!(gear, vec!["⚙ read_file …", "⚙ read_file ✓"]);
        assert!(view.in_flight_tools.is_empty());
        assert!(view.finished_tools.contains(&"t1".to_string()));
    }

    #[test]
    fn tool_failed_emits_cross_mark() {
        let mut view = SessionView::default();
        view.apply(&tool("t1", "write_file", ToolState::Started));
        view.apply(&tool("t1", "write_file", ToolState::Failed));
        assert!(view.history.contains(&"⚙ write_file ✗".to_string()));
        assert!(view.finished_tools.contains(&"t1".to_string()));
    }

    #[test]
    fn progress_folds_into_started_row() {
        let mut view = SessionView::default();
        view.apply(&tool("t1", "read_file", ToolState::Started));
        // Progress arrives as repeated Started events (agent.rs folding).
        view.apply(&tool("t1", "read_file", ToolState::Started));
        view.apply(&tool("t1", "read_file", ToolState::Started));
        view.apply(&tool("t1", "read_file", ToolState::Completed));
        let gear: Vec<&String> = view.history.iter().filter(|l| l.contains('⚙')).collect();
        assert_eq!(gear, vec!["⚙ read_file …", "⚙ read_file ✓"]);
    }

    #[test]
    fn scheduled_is_silent_and_terminal_only_closes_in_flight() {
        let mut view = SessionView::default();
        // Scheduled renders nothing.
        view.apply(&tool("t0", "bash", ToolState::Scheduled));
        assert!(view.history.is_empty());
        // Unsolicited completion without a start row: nothing.
        view.apply(&tool("t0", "bash", ToolState::Completed));
        assert!(view.history.is_empty());
        // Started after a (never-rendered) completion still opens a row.
        view.apply(&tool("t0", "bash", ToolState::Started));
        assert!(view.history.contains(&"⚙ bash …".to_string()));
    }

    #[test]
    fn events_after_terminal_line_are_inert() {
        let mut view = SessionView::default();
        view.apply(&tool("t1", "read_file", ToolState::Started));
        view.apply(&tool("t1", "read_file", ToolState::Completed));
        let before = view.history.clone();
        // Anything after the terminal line: nothing further.
        view.apply(&tool("t1", "read_file", ToolState::Started));
        view.apply(&tool("t1", "read_file", ToolState::Completed));
        view.apply(&tool("t1", "read_file", ToolState::Failed));
        assert_eq!(view.history, before);
    }

    #[test]
    fn tool_trackers_evict_oldest_at_cap() {
        let mut view = SessionView::default();
        let n = TOOL_TRACK_CAP as u32 + 5;
        for i in 0..n {
            view.apply(&tool(&format!("t{i}"), "bash", ToolState::Started));
        }
        assert_eq!(view.in_flight_tools.len(), TOOL_TRACK_CAP);
        assert_eq!(
            view.in_flight_tools.front().map(|t| t.id.as_str()),
            Some("t5")
        );
        assert_eq!(
            view.in_flight_tools.back().map(|t| t.id.as_str()),
            Some(format!("t{}", n - 1).as_str())
        );
        // The evicted oldest id can start a fresh row again (the other
        // starts were never dedup'd — distinct ids all rendered).
        view.apply(&tool("t0", "bash", ToolState::Started));
        let starts = view.history.iter().filter(|l| *l == "⚙ bash …").count();
        assert_eq!(starts, n as usize + 1);
    }

    // --- Wave 10 (GY-ZCODE-010): tool-row stats summary ---

    #[test]
    fn progress_stats_carry_into_terminal_summary() {
        let mut view = SessionView::default();
        view.apply(&tool("t1", "bash", ToolState::Started));
        // Progress arrives as a stats-bearing Started (agent.rs folding).
        view.apply(&tool_with_stats(
            "t1",
            "bash",
            ToolState::Started,
            ToolStats {
                duration_ms: Some(3_400),
                output_bytes: Some(12_600),
            },
        ));
        // Still exactly one open row; the progress emitted nothing.
        let gear: Vec<&String> = view.history.iter().filter(|l| l.contains('⚙')).collect();
        assert_eq!(gear, vec!["⚙ bash …"]);
        // Terminal payload without its own stats falls back to the
        // last-seen progress stats carried by the tracker.
        view.apply(&tool("t1", "bash", ToolState::Completed));
        let gear: Vec<&String> = view.history.iter().filter(|l| l.contains('⚙')).collect();
        assert_eq!(gear, vec!["⚙ bash …", "⚙ bash ✓ (3.4s, 12.3KB)"]);
    }

    #[test]
    fn terminal_stats_win_over_last_seen_progress() {
        let mut view = SessionView::default();
        view.apply(&tool_with_stats(
            "t1",
            "bash",
            ToolState::Started,
            ToolStats {
                duration_ms: Some(1_000),
                output_bytes: None,
            },
        ));
        view.apply(&tool_with_stats(
            "t1",
            "bash",
            ToolState::Completed,
            ToolStats {
                duration_ms: Some(2_500),
                output_bytes: None,
            },
        ));
        assert!(view.history.contains(&"⚙ bash ✓ (2.5s)".to_string()));
    }

    #[test]
    fn missing_stats_omits_summary_entirely() {
        let mut view = SessionView::default();
        // No progress stats, terminal without stats: plain rows.
        view.apply(&tool("t1", "read_file", ToolState::Started));
        view.apply(&tool("t1", "read_file", ToolState::Completed));
        assert!(view.history.contains(&"⚙ read_file ✓".to_string()));
        // Stats with both fields None also renders plain.
        view.apply(&tool("t2", "write_file", ToolState::Started));
        view.apply(&tool_with_stats(
            "t2",
            "write_file",
            ToolState::Completed,
            ToolStats::default(),
        ));
        assert!(view.history.contains(&"⚙ write_file ✓".to_string()));
    }

    #[test]
    fn stats_formatting_boundaries() {
        let f = |s: ToolStats| format_tool_stats(&s);
        // Duration: sub-second, exact, fractional.
        assert_eq!(
            f(ToolStats {
                duration_ms: Some(400),
                output_bytes: None
            }),
            Some("0.4s".into())
        );
        assert_eq!(
            f(ToolStats {
                duration_ms: Some(3_400),
                output_bytes: None
            }),
            Some("3.4s".into())
        );
        assert_eq!(
            f(ToolStats {
                duration_ms: Some(0),
                output_bytes: None
            }),
            Some("0.0s".into())
        );
        // Bytes: 0, just under/over the KiB boundary, MiB top unit.
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(1023), "1023B");
        assert_eq!(format_bytes(1024), "1.0KB");
        assert_eq!(format_bytes(12_600), "12.3KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0MB");
        assert_eq!(format_bytes(5 * 1024 * 1024 + 600_000), "5.6MB");
        // Combined order: duration first, then bytes.
        assert_eq!(
            f(ToolStats {
                duration_ms: Some(1_050),
                output_bytes: Some(1024)
            }),
            Some("1.1s, 1.0KB".into())
        );
    }

    #[test]
    fn stats_line_is_clamped_to_line_width() {
        let mut view = SessionView::default();
        view.apply(&tool("t1", "bash", ToolState::Started));
        view.apply(&tool_with_stats(
            "t1",
            "bash",
            ToolState::Completed,
            ToolStats {
                duration_ms: Some(3_400),
                output_bytes: Some(12_600),
            },
        ));
        // Every committed line (the summary included) fits LINE_WIDTH.
        for line in &view.history {
            assert!(
                line.chars().count() <= LINE_WIDTH,
                "line exceeds LINE_WIDTH: {line}"
            );
        }
        // An oversized tool name still clamps (truncate_chars marks it).
        let mut view = SessionView::default();
        let huge_tool = "t".repeat(LINE_WIDTH);
        view.apply(&tool("t1", &huge_tool, ToolState::Started));
        view.apply(&tool_with_stats(
            "t1",
            &huge_tool,
            ToolState::Completed,
            ToolStats {
                duration_ms: Some(3_400),
                output_bytes: Some(12_600),
            },
        ));
        for line in &view.history {
            assert!(line.chars().count() <= LINE_WIDTH);
        }
        // Exactly one terminal line came out (clamped, not wrapped — the
        // summary pushes past the width, so truncate_chars cut the row
        // and marked it with `~`).
        let done = view.history.iter().filter(|l| l.ends_with('~')).count();
        assert_eq!(done, 1);
    }

    // --- Wave 6 (GY-ZCODE-006): multi-session switching ---

    fn entry(id: &str) -> SessionEntry {
        SessionEntry {
            session_id: id.to_string(),
            title: format!("title {id}"),
            status: "idle".to_string(),
        }
    }

    fn view_with_sessions() -> SessionView {
        SessionView {
            sessions: vec![entry("a"), entry("b"), entry("c")],
            ..Default::default()
        }
    }

    #[test]
    fn cycle_selection_wraps_forward_and_backward() {
        let mut view = view_with_sessions();
        assert_eq!(view.selected, 0);
        assert_eq!(
            view.cycle_selection(1).map(|e| e.session_id.as_str()),
            Some("b")
        );
        assert_eq!(view.selected, 1);
        // Backward from the first entry wraps to the last.
        assert_eq!(
            view.cycle_selection(-1).map(|e| e.session_id.as_str()),
            Some("a")
        );
        assert_eq!(
            view.cycle_selection(-1).map(|e| e.session_id.as_str()),
            Some("c")
        );
        assert_eq!(view.selected, 2);
        // Forward from the last entry wraps to the first.
        assert_eq!(
            view.cycle_selection(1).map(|e| e.session_id.as_str()),
            Some("a")
        );
        // Large deltas keep wrapping.
        assert_eq!(
            view.cycle_selection(7).map(|e| e.session_id.as_str()),
            Some("b")
        );
    }

    #[test]
    fn cycle_selection_on_empty_list_is_none() {
        let mut view = SessionView::default();
        assert!(view.cycle_selection(1).is_none());
        assert!(view.cycle_selection(-1).is_none());
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn reset_transcript_clears_session_scoped_state_only() {
        let mut view = view_with_sessions();
        view.link = LinkState::Up;
        view.markdown = false;
        view.apply(&ProjEvent::TurnStarted { input: "hi".into() });
        view.apply(&ProjEvent::PartDelta {
            delta: "stream".into(),
        });
        view.apply(&tool("t1", "read_file", ToolState::Started));
        // Enough history for a non-zero scroll offset.
        for i in 0..30 {
            view.push_line(&format!("line {i:02}"));
        }
        view.scroll_up(5);
        assert!(!view.history.is_empty());
        assert!(!view.tail.is_empty());
        assert_eq!(view.scroll_from_bottom(), 5);

        view.reset_transcript();
        assert!(view.tail.is_empty());
        assert!(view.history.is_empty());
        assert!(view.streaming.is_empty());
        assert!(view.in_flight_tools.is_empty());
        assert!(view.finished_tools.is_empty());
        assert_eq!(view.scroll_from_bottom(), 0);
        assert!(view.rows().iter().all(|r| r.is_empty()));
        // Link, markdown toggle, sessions list and selection are kept.
        assert_eq!(view.link, LinkState::Up);
        assert!(!view.markdown);
        assert_eq!(view.sessions.len(), 3);
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn switching_status_shows_one_based_index() {
        let mut view = view_with_sessions();
        view.selected = 1;
        assert_eq!(view.switching_status(), "2/3 switching session…");
        assert_eq!(
            SessionView::default().switching_status(),
            "switching session…",
            "empty list falls back to the bare hint"
        );
    }

    // --- Wave 7 (GY-ZCODE-007): per-session transcript snapshot/restore ---

    #[test]
    fn transcript_snapshot_restore_round_trips() {
        let mut view = view_with_sessions();
        view.link = LinkState::Up;
        view.markdown = false;
        view.apply(&ProjEvent::TurnStarted { input: "q".into() });
        view.apply(&ProjEvent::PartDelta {
            delta: "streaming mid-turn".into(),
        });
        view.apply(&tool("t1", "read_file", ToolState::Started));
        for i in 0..30 {
            view.push_line(&format!("line {i:02}"));
        }
        view.scroll_up(4);
        let before_rows = view.rows();
        let before_from_bottom = view.scroll_from_bottom();

        let snap = view.snapshot_transcript();
        view.reset_transcript();
        assert!(view.tail.is_empty() && view.history.is_empty());
        assert!(view.in_flight_tools.is_empty());
        view.restore_transcript(snap);

        assert_eq!(view.rows(), before_rows, "rows restored exactly");
        assert_eq!(view.scroll_from_bottom(), before_from_bottom);
        assert_eq!(view.streaming, "streaming mid-turn");
        assert!(view.in_flight_tools.iter().any(|t| t.id == "t1"));
        // List/global state is untouched by restore.
        assert_eq!(view.link, LinkState::Up);
        assert!(!view.markdown);
        assert_eq!(view.sessions.len(), 3);
    }

    #[test]
    fn restore_into_default_leaves_global_state_alone() {
        let mut saved = view_with_sessions();
        saved.apply(&tool("t9", "bash", ToolState::Started));
        saved.push_line("kept");
        let snap = saved.snapshot_transcript();

        let mut fresh = SessionView {
            sessions: vec![entry("other")],
            selected: 0,
            markdown: false,
            ..Default::default()
        };
        fresh.restore_transcript(snap);

        assert_eq!(
            fresh.history,
            vec!["⚙ bash …".to_string(), "kept".to_string()]
        );
        assert!(fresh.in_flight_tools.iter().any(|t| t.id == "t9"));
        // Only transcript fields moved; the fresh view's own state stays.
        assert_eq!(fresh.sessions.len(), 1);
        assert_eq!(fresh.sessions[0].session_id, "other");
        assert!(!fresh.markdown);
    }

    #[test]
    fn default_transcript_restores_to_empty() {
        let mut view = view_with_sessions();
        view.push_line("stale");
        view.restore_transcript(SessionTranscript::default());
        assert!(view.tail.is_empty());
        assert!(view.history.is_empty());
        assert!(view.streaming.is_empty());
        assert!(view.in_flight_tools.is_empty());
        assert!(view.finished_tools.is_empty());
        assert_eq!(view.scroll_from_bottom(), 0);
        assert!(view.rows().iter().all(|r| r.is_empty()));
    }
}
