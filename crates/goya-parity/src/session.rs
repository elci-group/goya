//! Repeated activation of the admitted interactive counter.
//!
//! The integers and the publication rule come from Sash. This module does not
//! invent a missing value, build a D1 owner, or open a window.

/// Admitted counter kept across activations.
///
/// `count` changes only when Sash publishes a visible commit.
pub struct CounterSession {
    pub count: i64,
    pub revision: u64,
    pub edit: String,
    pub result: String,
    pub profile: &'static str,
    pub copy_allowed: bool,
    pub paste_allowed: bool,
    expr: sash::interactive::IntExpr,
}

/// One activation and the publication that followed it.
pub struct Attempt {
    pub activation: sash::interactive::Activation,
    pub outcome: sash::interactive::VisibleCommit,
}

impl CounterSession {
    /// Admit `source` as the interactive counter.
    ///
    /// A static scene is `SASH-CAP`. Any other admission error is returned unchanged.
    pub fn open(source: &str) -> Result<Self, sash::Diagnostic> {
        let admitted = match sash::admit::open(source)? {
            sash::admit::SceneTarget::Interactive(admitted) => admitted,
            sash::admit::SceneTarget::Static(_) => {
                return Err(cap("this run admits only the interactive counter"));
            }
        };
        Ok(Self {
            count: admitted.initial_state,
            revision: 0,
            edit: sash::interactive::format_i64(admitted.initial_input),
            result: sash::interactive::format_i64(admitted.initial_state),
            profile: admitted.profile,
            copy_allowed: admitted.copy_allowed,
            paste_allowed: admitted.paste_allowed,
            expr: admitted.expr,
        })
    }

    /// Replace the edit text. The stored count, revision, and result stay put.
    pub fn set_edit(&mut self, text: &str) {
        self.edit = text.to_owned();
    }

    /// Apply one Sash activation. The stored count changes only on `VisibleCommit::Store`.
    pub fn activate(
        &mut self,
        busy: bool,
        text_written: bool,
        released: bool,
        visible: Option<i64>,
    ) -> Attempt {
        let parsed = sash::interactive::parse_canonical_i64(&self.edit);
        let committed = parsed.and_then(|input| {
            sash::interactive::commit(&self.expr, self.count, self.revision, input)
        });
        let length = edit_len(&self.edit);
        let step = sash::interactive::activation(busy, length, parsed, committed);
        let decision = sash::interactive::publish_after_text(step, text_written);
        let outcome = sash::interactive::visible_commit(decision, released, visible);
        match outcome {
            sash::interactive::VisibleCommit::Store { next, revision } => {
                self.count = next;
                self.revision = revision;
                self.result = sash::interactive::format_i64(next);
            }
            sash::interactive::VisibleCommit::Revert => {
                self.result = sash::interactive::format_i64(self.count);
            }
            sash::interactive::VisibleCommit::Leave => {}
        }
        Attempt {
            activation: step,
            outcome,
        }
    }
}

/// UTF-16 length, or `-1` when that length does not fit in `i32`.
fn edit_len(text: &str) -> i32 {
    i32::try_from(text.encode_utf16().count()).unwrap_or(-1)
}

fn cap(message: impl Into<String>) -> sash::Diagnostic {
    sash::Diagnostic::new(
        "SASH-CAP",
        "admit",
        message,
        "Author the interactive counter. This run uses the integers Sash parsed and does not infer a missing value.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use sash::interactive::{Activation, VisibleCommit};

    const STATIC_SOURCE: &str = include_str!("../../sash/fixtures/static-window.gscene");

    #[test]
    fn open_reads_the_authored_counter() {
        let session = CounterSession::open(crate::COUNTER_SOURCE).expect("counter opens");
        assert_eq!(session.count, 0);
        assert_eq!(session.revision, 0);
        assert_eq!(session.edit, "1");
        assert_eq!(session.result, "0");
        assert_eq!(session.profile, sash::interactive::PROFILE_CLIPBOARD);
        assert!(session.copy_allowed);
        assert!(!session.paste_allowed);
    }

    #[test]
    fn repeated_activation_stores_then_rejects_a_bad_edit() {
        let mut session = CounterSession::open(crate::COUNTER_SOURCE).expect("counter opens");

        let first = session.activate(false, true, false, Some(1));
        assert_eq!(
            first.activation,
            Activation::Publish {
                next: 1,
                revision: 1
            }
        );
        assert_eq!(
            first.outcome,
            VisibleCommit::Store {
                next: 1,
                revision: 1
            }
        );
        assert_eq!(session.count, 1);
        assert_eq!(session.revision, 1);
        assert_eq!(session.result, "1");

        let second = session.activate(false, true, false, Some(2));
        assert_eq!(
            second.activation,
            Activation::Publish {
                next: 2,
                revision: 2
            }
        );
        assert_eq!(
            second.outcome,
            VisibleCommit::Store {
                next: 2,
                revision: 2
            }
        );
        assert_eq!(session.count, 2);
        assert_eq!(session.revision, 2);
        assert_eq!(session.result, "2");

        session.set_edit("00");
        assert_eq!(session.edit, "00");
        assert_eq!(session.count, 2);
        assert_eq!(session.revision, 2);
        assert_eq!(session.result, "2");
        let rejected = session.activate(false, true, false, Some(1));
        assert_eq!(rejected.activation, Activation::Rejected);
        assert_eq!(rejected.outcome, VisibleCommit::Leave);
        assert_eq!(session.count, 2);
        assert_eq!(session.revision, 2);
        assert_eq!(session.result, "2");
    }

    #[test]
    fn overflow_does_not_change_count_or_revision() {
        let mut session = CounterSession::open(crate::COUNTER_SOURCE).expect("counter opens");
        session.set_edit("1");
        session.count = i64::MAX;
        let revision = session.revision;
        let attempt = session.activate(false, true, false, None);
        assert_eq!(attempt.activation, Activation::Rejected);
        assert_eq!(attempt.outcome, VisibleCommit::Leave);
        assert_eq!(session.count, i64::MAX);
        assert_eq!(session.revision, revision);
    }

    #[test]
    fn mismatched_readback_reverts_without_storing() {
        let mut session = CounterSession::open(crate::COUNTER_SOURCE).expect("counter opens");
        session.activate(false, true, false, Some(1));
        session.activate(false, true, false, Some(2));
        assert_eq!(
            (session.count, session.revision, session.result.as_str()),
            (2, 2, "2")
        );
        session.result = "stale".to_owned();
        let attempt = session.activate(false, true, false, Some(0));
        assert_eq!(
            attempt.activation,
            Activation::Publish {
                next: 3,
                revision: 3
            }
        );
        assert_eq!(attempt.outcome, VisibleCommit::Revert);
        assert_eq!(session.count, 2);
        assert_eq!(session.revision, 2);
        assert_eq!(session.result, "2");
    }

    #[test]
    fn busy_activation_leaves_the_count() {
        let mut session = CounterSession::open(crate::COUNTER_SOURCE).expect("counter opens");
        let next = 1;
        let attempt = session.activate(true, true, false, Some(next));
        assert_eq!(attempt.activation, Activation::Busy);
        assert_eq!(attempt.outcome, VisibleCommit::Leave);
        assert_eq!(session.count, 0);
        assert_eq!(session.revision, 0);
        assert_eq!(session.result, "0");
    }

    #[test]
    fn release_during_publish_leaves_the_count() {
        let mut session = CounterSession::open(crate::COUNTER_SOURCE).expect("counter opens");
        let next = 1;
        let attempt = session.activate(false, true, true, Some(next));
        assert_eq!(
            attempt.activation,
            Activation::Publish {
                next: 1,
                revision: 1
            }
        );
        assert_eq!(attempt.outcome, VisibleCommit::Leave);
        assert_eq!(session.count, 0);
        assert_eq!(session.revision, 0);
        assert_eq!(session.result, "0");
    }

    #[test]
    fn static_window_is_not_a_counter_session() {
        match CounterSession::open(STATIC_SOURCE) {
            Err(error) => {
                assert_eq!(error.code, "SASH-CAP");
                assert_eq!(
                    error.message,
                    "this run admits only the interactive counter"
                );
            }
            Ok(_) => panic!("static scene opened a counter session"),
        }
    }
}
