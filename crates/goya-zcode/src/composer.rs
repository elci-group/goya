//! Composer — pure editable-input state machine (GY-ZCODE-002, J3).
//!
//! Owns the composed text, a char-offset cursor, focus, and a bounded
//! history of committed lines. The host binary maps Wayland events onto
//! [`Composer::handle_char`] / [`Composer::handle_special`]; this module
//! never touches goya-wayland types, so it compiles against the current
//! tree while J2 lands `KeySpecial` in parallel.
//!
//! Integration point (gate): `goya_wayland::KeySpecial` → `&str` mapping
//! is `ArrowLeft => "arrow_left"`, `ArrowRight => "arrow_right"`,
//! `ArrowUp => "arrow_up"`, `ArrowDown => "arrow_down"`, `Home => "home"`,
//! `End => "end"`, `Delete => "delete"`, `Escape => "escape"`,
//! `Tab => "tab"`. Enter stays `'\n'`, Backspace `'\u{8}'` in `KeyChar`.

/// History capacity for committed lines (recall via arrow up/down).
pub const HISTORY_CAP: usize = 16;

/// Visible-window width used by [`Composer::display`] to keep the
/// rendered composer text inside the input node.
pub const VIEW_WIDTH: usize = 30;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Composer {
    /// Shared platform-neutral editing state used by keyboard, IME, and
    /// accessibility input paths.
    pub buffer: goya_runtime::TextBuffer,
    /// IME composition is kept out of committed buffer text until the
    /// compositor sends commit_string. The byte cursor range is protocol
    /// relative to the preedit string.
    pub preedit: Option<(String, i32, i32)>,
    /// Whether the composer receives key input.
    pub focused: bool,
    /// Committed lines, newest last; arrow-up/down recall them.
    pub history: Vec<String>,
    /// History recall index; `None` = composing fresh text.
    pub history_pos: Option<usize>,
    /// In-progress draft stashed when the first recall starts, restored
    /// when recall walks past the newest entry.
    draft: Option<String>,
}

impl Composer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of extended grapheme clusters in the composed text.
    pub fn len_chars(&self) -> usize {
        self.buffer.grapheme_len()
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }
    pub fn cursor_grapheme(&self) -> usize {
        self.buffer.cursor_grapheme()
    }

    /// Feed one printable char, or `'\u{8}'` (backspace) / `'\n'` / `'\r'`
    /// (commit). Returns `Some(line)` exactly when a line is committed —
    /// the commit clears the field and pushes to history.
    pub fn handle_char(&mut self, c: char) -> Option<String> {
        if !self.focused {
            return None;
        }
        match c {
            '\n' | '\r' => {
                let line = self.buffer.text().to_owned();
                self.buffer.set_text("");
                self.history_pos = None;
                if line.is_empty() {
                    return None;
                }
                self.history.push(line.clone());
                if self.history.len() > HISTORY_CAP {
                    self.history.remove(0);
                }
                Some(line)
            }
            '\u{8}' => {
                self.buffer.delete_backward();
                None
            }
            c if !c.is_control() => {
                self.buffer.insert(&c.to_string());
                None
            }
            // Other control chars are inert.
            _ => None,
        }
    }

    /// Handle a named special key. Names are host-mapped from
    /// `goya_wayland::KeySpecial` (see module docs); unknown names are
    /// inert. Returns the new focused state for escape handling.
    pub fn handle_special(&mut self, special: &str) {
        if !self.focused {
            return;
        }
        match special {
            "arrow_left" => self.buffer.move_left(false),
            "arrow_right" => self.buffer.move_right(false),
            "home" => self.buffer.move_home(false),
            "end" => self.buffer.move_end(false),
            "delete" => self.buffer.delete_forward(),
            "escape" => self.focused = false,
            "tab" => {}
            "arrow_up" => self.recall_history(-1),
            "arrow_down" => self.recall_history(1),
            _ => {}
        }
    }

    /// Apply one text-input-v3 done batch in protocol order. Preedit remains
    /// an overlay; delete and commit edits go through the shared TextBuffer.
    pub fn handle_ime_batch(
        &mut self,
        preedit: Option<(String, i32, i32)>,
        commit: Option<&str>,
        delete: Option<(u32, u32)>,
    ) {
        if !self.focused { return; }
        if delete.is_some() { self.preedit = None; }
        if let Some((before, after)) = delete {
            self.buffer.delete_surrounding(before, after);
        }
        if let Some(text) = commit {
            self.buffer.insert(text);
            self.preedit = None;
        }
        if let Some(preedit) = preedit {
            self.preedit = (!preedit.0.is_empty()).then_some(preedit);
        }
    }

    /// Recall a committed line from history (`-1` = previous, `+1` = next).
    /// `history_pos` walks `Some(last)` … `Some(0)` upward, then `None`
    /// restores any stashed draft.
    fn recall_history(&mut self, dir: i32) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        match (dir, self.history_pos) {
            (-1, None) => {
                // First recall: stash any non-empty, non-committed draft.
                let draft = match self.text().is_empty()
                    || self.history.last().map(String::as_str) == Some(self.text())
                {
                    true => None,
                    false => {
                        let draft = self.text().to_owned();
                        self.buffer.set_text("");
                        Some(draft)
                    }
                };
                self.draft = draft;
                self.history_pos = Some(last);
                self.buffer.set_text(self.history[last].clone());
            }
            (-1, Some(0)) => {} // already at the oldest entry
            (-1, Some(p)) => {
                self.history_pos = Some(p - 1);
                self.buffer.set_text(self.history[p - 1].clone());
            }
            (1, None) => {} // already at the newest draft
            (1, Some(p)) if p >= last => {
                // Past the newest entry: restore the stashed draft.
                self.history_pos = None;
                self.buffer.set_text(self.draft.take().unwrap_or_default());
            }
            (1, Some(p)) => {
                self.history_pos = Some(p + 1);
                self.buffer.set_text(self.history[p + 1].clone());
            }
            _ => {}
        }
        self.buffer.move_end(false);
    }

    /// Rendered text for the input node: a `VIEW_WIDTH`-char window around
    /// the cursor with a `│` cursor marker inserted at the cursor position.
    pub fn display(&self) -> String {
        let mut chars = self.buffer.graphemes();
        let mut combined = Vec::new();
        let cursor = self.cursor_grapheme();
        if let Some((preedit, _, _)) = &self.preedit {
            let mut before = chars[..cursor.min(chars.len())].to_vec();
            combined.append(&mut before);
            combined.extend(preedit.graphemes(true));
            combined.extend(chars[cursor.min(chars.len())..].iter().copied());
            chars = combined;
        }
        let len = chars.len();
        let cursor = cursor.min(len);
        // Keep ~2/3 of the window before the cursor when possible.
        let half_before = VIEW_WIDTH * 2 / 3;
        let start = cursor
            .saturating_sub(half_before)
            .min(len.saturating_sub(1));
        let end = (start + VIEW_WIDTH).min(len);
        let start = end.saturating_sub(VIEW_WIDTH);
        let mut out = String::new();
        out.push_str(&chars[start..cursor].concat());
        out.push('│');
        out.push_str(&chars[cursor..end].concat());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focused() -> Composer {
        Composer {
            focused: true,
            ..Composer::default()
        }
    }

    #[test]
    fn unfocused_composer_is_inert() {
        let mut c = Composer::default();
        assert_eq!(c.handle_char('a'), None);
        c.handle_special("arrow_left");
        c.handle_special("escape");
        assert_eq!(c.text(), "");
        assert!(!c.focused);
    }

    #[test]
    fn printable_chars_insert_at_cursor() {
        let mut c = focused();
        for ch in "abc".chars() {
            c.handle_char(ch);
        }
        assert_eq!(c.text(), "abc");
        assert_eq!(c.cursor_grapheme(), 3);
    }

    #[test]
    fn utf8_multibyte_cursor_math_is_char_safe() {
        let mut c = focused();
        for ch in "héllo日本".chars() {
            c.handle_char(ch);
        }
        // Move left twice (over 本 and 日), insert.
        c.handle_special("arrow_left");
        c.handle_special("arrow_left");
        assert_eq!(c.cursor_grapheme(), 5);
        assert_eq!(c.handle_char('x'), None);
        assert_eq!(c.text(), "héllox日本");
        // Backspace removes the 'x' again.
        c.handle_char('\u{8}');
        assert_eq!(c.text(), "héllo日本");
        assert_eq!(c.cursor_grapheme(), 5);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut c = focused();
        assert_eq!(c.handle_char('\u{8}'), None);
        assert_eq!(c.text(), "");
        assert_eq!(c.cursor_grapheme(), 0);
    }

    #[test]
    fn enter_commits_and_clears() {
        let mut c = focused();
        for ch in "hello\n".chars() {
            if let Some(line) = c.handle_char(ch) {
                assert_eq!(line, "hello");
            }
        }
        assert_eq!(c.text(), "");
        assert_eq!(c.cursor_grapheme(), 0);
        assert_eq!(c.history, vec!["hello"]);
    }

    #[test]
    fn carriage_return_commits_like_newline() {
        let mut c = focused();
        for ch in "hi\r".chars() {
            if let Some(line) = c.handle_char(ch) {
                assert_eq!(line, "hi");
            }
        }
        assert!(c.text().is_empty());
    }

    #[test]
    fn empty_enter_commits_nothing() {
        let mut c = focused();
        assert_eq!(c.handle_char('\n'), None);
        assert!(c.history.is_empty());
    }

    #[test]
    fn history_is_bounded() {
        let mut c = focused();
        for i in 0..(HISTORY_CAP + 5) {
            for ch in format!("line{i}").chars() {
                c.handle_char(ch);
            }
            assert_eq!(
                c.handle_char('\n').as_deref(),
                Some(format!("line{i}").as_str())
            );
        }
        assert_eq!(c.history.len(), HISTORY_CAP);
        assert_eq!(c.history[0], "line5");
        assert_eq!(c.history[HISTORY_CAP - 1], "line20");
    }

    #[test]
    fn arrows_move_cursor_within_bounds() {
        let mut c = focused();
        for ch in "ab".chars() {
            c.handle_char(ch);
        }
        c.handle_special("arrow_left");
        c.handle_special("arrow_left");
        assert_eq!(c.cursor_grapheme(), 0);
        // Clamp at start.
        c.handle_special("arrow_left");
        assert_eq!(c.cursor_grapheme(), 0);
        c.handle_special("arrow_right");
        assert_eq!(c.cursor_grapheme(), 1);
        c.handle_special("end");
        assert_eq!(c.cursor_grapheme(), 2);
        // Clamp at end.
        c.handle_special("arrow_right");
        assert_eq!(c.cursor_grapheme(), 2);
    }

    #[test]
    fn home_end_jump() {
        let mut c = focused();
        for ch in "abc".chars() {
            c.handle_char(ch);
        }
        c.handle_special("home");
        assert_eq!(c.cursor_grapheme(), 0);
        c.handle_special("end");
        assert_eq!(c.cursor_grapheme(), 3);
    }

    #[test]
    fn delete_removes_after_cursor() {
        let mut c = focused();
        for ch in "abc".chars() {
            c.handle_char(ch);
        }
        c.handle_special("home");
        c.handle_special("delete");
        assert_eq!(c.text(), "bc");
        assert_eq!(c.cursor_grapheme(), 0);
        // Delete at end is a noop.
        c.handle_special("end");
        c.handle_special("delete");
        assert_eq!(c.text(), "bc");
    }

    #[test]
    fn arrow_up_recalls_then_arrow_down_returns_to_draft() {
        let mut c = focused();
        for ch in "first\n".chars() {
            c.handle_char(ch);
        }
        assert_eq!(c.handle_char('d'), None);
        // Draft is "d"; recall previous commit.
        c.handle_special("arrow_up");
        assert_eq!(c.text(), "first");
        // Past the oldest entry: still "first".
        c.handle_special("arrow_up");
        assert_eq!(c.text(), "first");
        // Next past newest restores the draft.
        c.handle_special("arrow_down");
        assert_eq!(c.text(), "d");
        assert_eq!(c.cursor_grapheme(), 1);
    }

    #[test]
    fn arrow_down_without_recall_is_noop() {
        let mut c = focused();
        for ch in "x\n".chars() {
            c.handle_char(ch);
        }
        c.handle_char('y');
        c.handle_special("arrow_down");
        assert_eq!(c.text(), "y");
    }

    #[test]
    fn escape_clears_focus() {
        let mut c = focused();
        c.handle_special("escape");
        assert!(!c.focused);
        // And input is ignored while unfocused.
        assert_eq!(c.handle_char('a'), None);
    }

    #[test]
    fn display_inserts_cursor_marker_and_windows() {
        let mut c = focused();
        for ch in "hello".chars() {
            c.handle_char(ch);
        }
        assert_eq!(c.display(), "hello│");
        c.handle_special("home");
        assert_eq!(c.display(), "│hello");

        // Long text: window stays at VIEW_WIDTH visible chars around cursor.
        let mut long = focused();
        for i in 0..40 {
            long.handle_char(char::from(b'a' + (i % 26)));
        }
        long.handle_special("arrow_left");
        let visible = long.display();
        let count = visible.chars().count();
        assert!(count <= VIEW_WIDTH + 1, "window too wide: {count}");
        assert!(visible.contains('│'));
    }
}
