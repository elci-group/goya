// Announcement wording for the shown integer (decision D9).
//
// This file is included by the host compiler and pasted into the generated
// executable for GSSH/4-live scenes, whose result control is a UI Automation
// live region. It mirrors the integer case of
// `goya_lang::display_binding::describe_change`; the host tests compare them.

pub fn int_change_announcement(context: &str, old: i64, new: i64) -> Option<String> {
    if old == new {
        return None;
    }
    let difference = i128::from(new) - i128::from(old);
    let verb = if difference > 0 {
        "increased"
    } else {
        "decreased"
    };
    Some(format!(
        "{context} {verb} by {}, from {old} to {new}",
        difference.unsigned_abs()
    ))
}
