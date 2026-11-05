//! Session projection — the single UI fact source for the monitor.
//!
//! The agent thread is the only writer; the render loop only reads
//! snapshots. All reduction logic is pure and unit-tested here.

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
    TurnStarted { input: String },
    PartDelta { delta: String },
    TurnCompleted { response: String, result: String },
    TurnFailed { message: String },
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
    pub scroll: usize,
    /// In-flight assistant streaming buffer, flushed to `tail` in chunks.
    pub streaming: String,
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
            scroll: 0,
            streaming: String::new(),
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
            self.tail.push_back(piece);
            if self.tail.len() > TAIL_LINES {
                self.tail.pop_front();
            }
            self.history.push(piece);
        }
        if self.history.len() > HISTORY_CAP {
            // Drop the oldest overflow so scrollback stays bounded.
            let drop = self.history.len() - HISTORY_CAP;
            self.history.drain(0..drop);
        }
    }

    /// Scroll back `n` lines (PageUp); clamped so the window stays inside
    /// the history.
    pub fn scroll_up(&mut self, n: usize) {
        let max = self.history.len();
        self.scroll = (self.scroll + n).min(max);
    }

    /// Scroll forward `n` lines (PageDown); `0` is live/bottom.
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    /// Jump back to the live bottom.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll = 0;
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
        }
    }

    /// The exactly-`TAIL_LINES` rows rendered into the scene, newest last.
    pub fn rows(&self) -> Vec<String> {
        let mut rows: Vec<String> = self
            .tail
            .iter()
            .map(|l| truncate_chars(l, LINE_WIDTH))
            .collect();
        // Live streaming preview on the last line while a turn runs.
        if !self.streaming.is_empty() && rows.len() < TAIL_LINES {
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
}
