// Commit publication for the interactive window.
//
// This file is included by the host compiler and pasted into the generated
// executable. The result text is written before the value is stored. A nested
// activation during that write does not publish. A release that starts during
// that write drops the commit. The stored value changes only when the result
// text reads back as that value. A different readback restores the previous
// result text while the window is still live. A failed text update leaves the
// previous value and revision unchanged.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Activation {
    Busy,
    Rejected,
    Publish { next: i64, revision: u64 },
}

pub fn activation(
    busy: bool,
    edit_len: i32,
    parsed: Option<i64>,
    committed: Option<(i64, u64)>,
) -> Activation {
    if busy {
        return Activation::Busy;
    }
    if !(1..31).contains(&edit_len) {
        return Activation::Rejected;
    }
    if parsed.is_none() {
        return Activation::Rejected;
    }
    match committed {
        Some((next, revision)) => Activation::Publish { next, revision },
        None => Activation::Rejected,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Publish {
    Hold,
    Store { next: i64, revision: u64 },
}

pub fn publish_after_text(step: Activation, text_written: bool) -> Publish {
    match step {
        Activation::Publish { next, revision } if text_written => Publish::Store { next, revision },
        Activation::Publish { .. } | Activation::Busy | Activation::Rejected => Publish::Hold,
    }
}

pub fn command_activates(control_id: u16, note: u16, button_id: u16) -> bool {
    control_id == button_id && note == 0
}

pub fn hold_after_release(decision: Publish, released: bool) -> Publish {
    if released {
        Publish::Hold
    } else {
        decision
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisibleCommit {
    Store { next: i64, revision: u64 },
    Revert,
    Leave,
}

pub fn visible_commit(decision: Publish, released: bool, visible: Option<i64>) -> VisibleCommit {
    match hold_after_release(decision, released) {
        Publish::Store { next, revision } if visible == Some(next) => {
            VisibleCommit::Store { next, revision }
        }
        Publish::Store { .. } => VisibleCommit::Revert,
        Publish::Hold => VisibleCommit::Leave,
    }
}
