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

use crate::InteractionValue;

/// The text a state value is shown as: canonical decimal for `int`, the
/// text itself for `text`, `true` or `false` for `bool`.
pub fn shown_text(value: &InteractionValue) -> String {
    match value {
        InteractionValue::Int(n) => n.to_string(),
        InteractionValue::Text(t) => t.clone(),
        InteractionValue::Bool(b) => b.to_string(),
    }
}

/// What names a display in an announcement: the element's authored text
/// without trailing spaces, tabs, line breaks or one trailing colon
/// (`"Count: "` → `"Count"`), or the state key when that leaves nothing.
pub fn context(authored_text: Option<&str>, key: &str) -> String {
    let blank = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    let text = authored_text.unwrap_or("").trim_end_matches(blank);
    let text = text
        .strip_suffix(':')
        .unwrap_or(text)
        .trim_end_matches(blank);
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
