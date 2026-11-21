//! Reference behaviour of `text-from` display bindings (GSSH/3-live,
//! decision D9 in `docs/gssh/decisions.md`). Every target that admits
//! GSSH/3-live must produce exactly these strings; Maja's browser mirror is
//! differentially tested against them.
//!
//! * [`shown_text`]: the text a value is shown as.
//! * Wrapping (not a function): an element with `text-from` lays out its
//!   authored text followed by the shown value with CSS `white-space:
//!   pre-wrap` semantics, as Arachnid does: spaces and line breaks are
//!   kept, lines break at the element's edge, overflow is clipped.
//! * [`describe_change`] / [`announcement`]: what a target announces to
//!   assistive technology, by default, when a committed transition changes
//!   shown values. It says what changed (the element's context) and how
//!   (direction and amount, or old and new value), never how it looks.

use crate::{InteractionValue, SceneCell};
use std::collections::BTreeMap;

/// Visit live displays in authored depth-first order.
fn visit_displays(cell: &SceneCell, f: &mut impl FnMut(&SceneCell, &str)) {
    let mut pending = vec![cell];
    while let Some(cell) = pending.pop() {
        if let Some(key) = cell.text_from.as_deref() {
            f(cell, key);
        }
        // Reverse push preserves source-order preorder without recursive
        // calls, including for SceneCell values built outside the parser.
        pending.extend(cell.children.iter().rev());
    }
}

/// The text a state value is shown as: canonical decimal for `int`, the
/// text itself for `text`, `true` or `false` for `bool`.
pub fn shown_text(value: &InteractionValue) -> String {
    match value {
        InteractionValue::Int(n) => n.to_string(),
        InteractionValue::Text(t) => t.clone(),
        InteractionValue::Bool(b) => b.to_string(),
    }
}

/// One live display projected from a state snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneDisplayValue {
    /// Authored element identity, absent only for non-strict parsed scenes.
    pub element_id: Option<String>,
    /// State key referenced by `text-from`.
    pub state_key: String,
    /// Context label used by canonical announcements.
    pub context: String,
    /// Canonical text shown for the current value.
    pub text: String,
}

/// Project a state snapshot onto the scene's live displays in document
/// order. Results carry authored identity, bound key, announcement context
/// and canonical value; repeated bindings are preserved because each display
/// is independently rendered.
pub fn scene_display_values(
    root: &SceneCell,
    state: &BTreeMap<String, InteractionValue>,
) -> Vec<SceneDisplayValue> {
    let mut values = Vec::new();
    visit_displays(root, &mut |cell, key| {
        if let Some(value) = state.get(key) {
            values.push(SceneDisplayValue {
                element_id: cell.authored_id.clone(),
                state_key: key.to_owned(),
                context: context(cell.text.as_deref(), key),
                text: shown_text(value),
            });
        }
    });
    values
}

/// What names a display in an announcement: the element's authored text
/// without trailing spaces, tabs, line breaks or one trailing colon
/// (`"Count: "` → `"Count"`), or the state key when that leaves nothing.
pub fn context(authored_text: Option<&str>, key: &str) -> String {
    // Authored labels can contain Unicode spacing characters (for example a
    // non-breaking space before a colon); treat all Unicode whitespace as a
    // trailing blank when deriving the spoken context.
    let text = authored_text.unwrap_or("").trim_end_matches(char::is_whitespace);
    let text = text
        .strip_suffix(':')
        .unwrap_or(text)
        .trim_end_matches(char::is_whitespace);
    if text.is_empty() {
        key.to_owned()
    } else {
        text.to_owned()
    }
}

/// One display's change, or `None` when the shown value did not change.
///
/// * `int`: `"<context> increased by <d>, from <old> to <new>"` (or
///   `decreased`), where `d` is the exact difference.
/// * `text`: `<context> changed from "<old>" to "<new>"` (quoted, so an
///   empty value is audible).
/// * `bool`: `"<context> changed from <old> to <new>"`.
pub fn describe_change(
    context: &str,
    old: &InteractionValue,
    new: &InteractionValue,
) -> Option<String> {
    match (old, new) {
        (InteractionValue::Int(a), InteractionValue::Int(b)) if a != b => {
            let d = i128::from(*b) - i128::from(*a);
            let (verb, amount) = if d > 0 {
                ("increased", d)
            } else {
                ("decreased", -d)
            };
            Some(format!("{context} {verb} by {amount}, from {a} to {b}"))
        }
        (InteractionValue::Text(a), InteractionValue::Text(b)) if a != b => {
            Some(format!("{context} changed from \"{a}\" to \"{b}\""))
        }
        (InteractionValue::Bool(a), InteractionValue::Bool(b)) if a != b => {
            Some(format!("{context} changed from {a} to {b}"))
        }
        _ => None,
    }
}

/// The single announcement for one committed transition: every display
/// whose shown value changed, in document (depth-first) order, joined with
/// `"; "`. `None` when nothing shown changed; a failed or refused
/// transition is never announced here (targets report those separately).
pub fn announcement<'a>(
    changes: impl IntoIterator<Item = (&'a str, &'a InteractionValue, &'a InteractionValue)>,
) -> Option<String> {
    let parts: Vec<String> = changes
        .into_iter()
        .filter_map(|(context, old, new)| describe_change(context, old, new))
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// Build the canonical announcement for a scene after a committed state
/// transition. Displays are visited in document (depth-first) order, and a
/// display is included only when its bound value exists in both snapshots
/// and changed. Missing keys are ignored so a partial host snapshot cannot
/// produce a fabricated change; admitted scenes guarantee that each binding
/// names a declared key.
pub fn scene_announcement(
    root: &SceneCell,
    before: &BTreeMap<String, InteractionValue>,
    after: &BTreeMap<String, InteractionValue>,
) -> Option<String> {
    let mut changes = Vec::new();
    visit_displays(root, &mut |cell, key| {
        if let (Some(old), Some(new)) = (before.get(key), after.get(key)) {
            changes.push((context(cell.text.as_deref(), key), old, new));
        }
    });
    announcement(
        changes
            .iter()
            .map(|(label, old, new)| (label.as_str(), *old, *new)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use InteractionValue::{Bool, Int, Text};

    #[test]
    fn values_are_shown_canonically() {
        assert_eq!(shown_text(&Int(-12)), "-12");
        assert_eq!(shown_text(&Int(i64::MIN)), "-9223372036854775808");
        assert_eq!(shown_text(&Text("a  b".into())), "a  b");
        assert_eq!(shown_text(&Bool(true)), "true");
    }

    #[test]
    fn context_comes_from_authored_text_or_the_key() {
        assert_eq!(context(Some("Count: "), "count"), "Count");
        assert_eq!(context(Some("Total (€):\t\n"), "t"), "Total (€)");
        assert_eq!(context(Some("Ratio::"), "r"), "Ratio:");
        assert_eq!(context(Some(" : "), "label"), "label");
        assert_eq!(context(None, "label"), "label");
    }

    #[test]
    fn changes_say_what_and_how() {
        assert_eq!(
            describe_change("Count", &Int(-12), &Int(-7)).unwrap(),
            "Count increased by 5, from -12 to -7"
        );
        assert_eq!(
            describe_change("Count", &Int(i64::MAX), &Int(i64::MIN)).unwrap(),
            "Count decreased by 18446744073709551615, from 9223372036854775807 to -9223372036854775808"
        );
        assert_eq!(
            describe_change("label", &Text("none".into()), &Text(String::new())).unwrap(),
            "label changed from \"none\" to \"\""
        );
        assert_eq!(
            describe_change("Raised", &Bool(false), &Bool(true)).unwrap(),
            "Raised changed from false to true"
        );
        assert_eq!(describe_change("Count", &Int(3), &Int(3)), None);
        assert_eq!(
            announcement([
                ("Count", &Int(0), &Int(2)),
                ("label", &Text("a".into()), &Text("a".into())),
                ("Raised", &Bool(false), &Bool(true)),
            ])
            .unwrap(),
            "Count increased by 2, from 0 to 2; Raised changed from false to true"
        );
        assert_eq!(announcement([("Count", &Int(1), &Int(1))]), None);
    }
}
