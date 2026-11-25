//! Grapheme caret for one focused field.
//!
//! Byte offsets follow the same boundaries as `goya_runtime::TextBuffer`:
//! UTF-8 offsets snap to whole graphemes. This module does not draw a caret
//! and does not keep an undo stack.

use unicode_segmentation::UnicodeSegmentation;

/// Where [`crate::Instance::edit`] puts the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretMotion {
    Left,
    Right,
    Home,
    End,
}

/// One caret edit. `Insert` writes at the caret. The others move or delete
/// one grapheme. None of them publishes D1 state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Insert(String),
    Backspace,
    Delete,
    Move(CaretMotion),
}

/// Focused field and the caret's UTF-8 byte offset after an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaretPlace {
    pub id: String,
    pub byte: usize,
}

/// Apply `edit` to `text` at `caret`. `Some` text replaces the field.
/// The byte is the caret after the edit, before a rejected write.
pub(crate) fn apply(text: &str, caret: usize, edit: Edit) -> (Option<String>, usize) {
    let caret = floor(text, caret.min(text.len()));
    match edit {
        Edit::Move(motion) => (None, move_to(text, caret, motion)),
        Edit::Insert(extra) => {
            let mut next = String::new();
            next.push_str(&text[..caret]);
            next.push_str(&extra);
            next.push_str(&text[caret..]);
            let byte = caret + extra.len();
            (Some(next), byte)
        }
        Edit::Backspace => {
            let start = previous(text, caret);
            if start == caret {
                (None, caret)
            } else {
                let mut next = String::new();
                next.push_str(&text[..start]);
                next.push_str(&text[caret..]);
                (Some(next), start)
            }
        }
        Edit::Delete => {
            let end = following(text, caret);
            if end == caret {
                (None, caret)
            } else {
                let mut next = String::new();
                next.push_str(&text[..caret]);
                next.push_str(&text[end..]);
                (Some(next), caret)
            }
        }
    }
}

pub(crate) fn clamp(text: &str, caret: usize) -> usize {
    floor(text, caret.min(text.len()))
}

fn move_to(text: &str, caret: usize, motion: CaretMotion) -> usize {
    match motion {
        CaretMotion::Left => previous(text, caret),
        CaretMotion::Right => following(text, caret),
        CaretMotion::Home => 0,
        CaretMotion::End => text.len(),
    }
}

fn floor(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .take_while(|index| *index <= byte)
        .last()
        .unwrap_or(0)
}

fn previous(text: &str, caret: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .take_while(|index| *index < caret)
        .last()
        .unwrap_or(0)
}

fn following(text: &str, caret: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .find(|index| *index > caret)
        .unwrap_or(text.len())
}
