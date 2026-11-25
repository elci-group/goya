//! Process-local diagnostic trace.
//!
//! The trace is not portable state. It is absent from the snapshot, the
//! journal, the fingerprint, and the wire. When it is full, the oldest event
//! is dropped. That drop does not drop a committed mutation.

use std::collections::VecDeque;

/// Retained diagnostic events. Older events are dropped, not the journal.
pub const MAX_TRACE: usize = 64;

/// One observed outcome. Names match the directive where the outcome exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diagnostic {
    SessionCreated,
    SessionJoined,
    MutationCommitted {
        revision: u64,
    },
    MutationAcknowledged {
        revision: u64,
    },
    MutationRejected {
        code: &'static str,
    },
    MutationDeduplicated {
        revision: u64,
    },
    /// `Apply::Stale`. The directive list has no name for this outcome.
    MutationStale {
        revision: u64,
    },
    AuthorityTransferStarted,
    AuthorityTransferCompleted {
        epoch: u64,
    },
    /// A prepared transfer was aborted, or activation was refused.
    AuthorityTransferFailed,
    SnapshotApplied {
        revision: u64,
    },
    SnapshotRejected {
        code: &'static str,
    },
}

impl Diagnostic {
    pub const fn name(self) -> &'static str {
        match self {
            Self::SessionCreated => "session.created",
            Self::SessionJoined => "session.joined",
            Self::MutationCommitted { .. } => "mutation.committed",
            Self::MutationAcknowledged { .. } => "mutation.acknowledged",
            Self::MutationRejected { .. } => "mutation.rejected",
            Self::MutationDeduplicated { .. } => "mutation.deduplicated",
            Self::MutationStale { .. } => "mutation.stale",
            Self::AuthorityTransferStarted => "authority.transfer.started",
            Self::AuthorityTransferCompleted { .. } => "authority.transfer.completed",
            Self::AuthorityTransferFailed => "authority.transfer.failed",
            Self::SnapshotApplied { .. } => "snapshot.applied",
            Self::SnapshotRejected { .. } => "snapshot.rejected",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Trace {
    events: VecDeque<Diagnostic>,
}

impl Trace {
    pub(crate) fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }

    pub(crate) fn record(&mut self, event: Diagnostic) {
        if self.events.len() == MAX_TRACE {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub(crate) fn as_slice(&self) -> Vec<Diagnostic> {
        self.events.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::Value;
    use crate::id_from_byte;
    use crate::mutation::{encode_envelope, Apply};
    use crate::pairs::{join, open, select, state_ok, F_SCORE};

    fn deck() -> [u32; 8] {
        [0, 1, 2, 3, 3, 2, 1, 0]
    }

    #[test]
    fn the_trace_drops_the_oldest_event() {
        let mut trace = Trace::new();
        for revision in 0..u64::try_from(MAX_TRACE).unwrap() + 1 {
            trace.record(Diagnostic::MutationCommitted { revision });
        }
        let events = trace.as_slice();
        assert_eq!(events.len(), MAX_TRACE);
        assert_eq!(events[0], Diagnostic::MutationCommitted { revision: 1 });
        assert_eq!(
            events[MAX_TRACE - 1],
            Diagnostic::MutationCommitted {
                revision: u64::try_from(MAX_TRACE).unwrap()
            }
        );
    }

    #[test]
    fn diagnostics_are_absent_from_the_snapshot() {
        let mut origin = open(id_from_byte(1), id_from_byte(9), &deck()).unwrap();
        let idle = open(id_from_byte(1), id_from_byte(9), &deck()).unwrap();
        assert_eq!(origin.diagnostics(), &[Diagnostic::SessionCreated]);
        assert_eq!(origin.diagnostics()[0].name(), "session.created");
        origin.prepare_handoff(id_from_byte(2)).unwrap();
        origin.abort_handoff().unwrap();
        assert_eq!(origin.fingerprint().unwrap(), idle.fingerprint().unwrap());
        assert_eq!(
            origin.encode_snapshot().unwrap(),
            idle.encode_snapshot().unwrap()
        );
        assert_eq!(
            origin.diagnostics().last().copied(),
            Some(Diagnostic::AuthorityTransferFailed)
        );
        select(&mut origin, 0).unwrap();
        assert_eq!(
            origin.diagnostics().last().copied(),
            Some(Diagnostic::MutationCommitted { revision: 1 })
        );
        assert_eq!(origin.values().get(&F_SCORE), Some(&Value::Int(0)));
    }

    #[test]
    fn outcomes_follow_the_commit_the_follower_and_the_grant() {
        let mut origin = open(id_from_byte(1), id_from_byte(9), &deck()).unwrap();
        let idle = origin.encode_snapshot().unwrap();
        let envelope = select(&mut origin, 0).unwrap();
        let mutation = encode_envelope(&envelope).unwrap();
        let matched = origin.encode_snapshot().unwrap();

        let mut phone = join(id_from_byte(2), &idle).unwrap();
        assert_eq!(phone.diagnostics(), [Diagnostic::SessionJoined]);
        assert_eq!(
            phone.apply_remote(&mutation, state_ok).unwrap(),
            Apply::Applied
        );
        assert_eq!(
            phone.diagnostics().last().copied(),
            Some(Diagnostic::MutationAcknowledged { revision: 1 })
        );
        assert_eq!(
            phone.apply_remote(&mutation, state_ok).unwrap(),
            Apply::Duplicate
        );
        assert_eq!(
            phone.diagnostics().last().copied(),
            Some(Diagnostic::MutationDeduplicated { revision: 1 })
        );
        let marked = phone.diagnostics().len();
        assert_eq!(
            phone
                .commit_authority(Vec::new(), state_ok)
                .unwrap_err()
                .code,
            "RMT-E5005"
        );
        assert_eq!(
            phone.diagnostics().last().copied(),
            Some(Diagnostic::MutationRejected { code: "RMT-E5005" })
        );
        assert_eq!(phone.diagnostics().len(), marked + 1);
        phone.install_snapshot(&matched).unwrap();
        assert_eq!(phone.diagnostics()[0], Diagnostic::SessionJoined);
        assert_eq!(
            phone.apply_remote(&mutation, state_ok).unwrap(),
            Apply::Stale
        );
        assert_eq!(
            phone.diagnostics().last().copied(),
            Some(Diagnostic::MutationStale { revision: 1 })
        );

        let before = origin.diagnostics().len();
        assert_eq!(
            origin.prepare_handoff(id_from_byte(1)).unwrap_err().code,
            "RMT-E5007"
        );
        assert_eq!(origin.diagnostics().len(), before);
        origin.prepare_handoff(id_from_byte(2)).unwrap();
        assert_eq!(
            origin.diagnostics().last().copied(),
            Some(Diagnostic::AuthorityTransferStarted)
        );
        let grant = origin.grant(id_from_byte(2)).unwrap();
        assert_eq!(
            origin.diagnostics().last().copied(),
            Some(Diagnostic::AuthorityTransferCompleted { epoch: 2 })
        );
        let after_grant = origin.diagnostics().len();
        assert_eq!(origin.abort_handoff().unwrap_err().code, "RMT-E5005");
        assert_eq!(origin.diagnostics().len(), after_grant);
        let post = origin.encode_snapshot().unwrap();

        let mut successor = join(id_from_byte(2), &idle).unwrap();
        successor.activate(&grant, &post).unwrap();
        assert!(successor.is_writer());
        let events = successor.diagnostics();
        assert_eq!(
            events[events.len() - 2],
            Diagnostic::SnapshotApplied { revision: 1 }
        );
        assert_eq!(
            events[events.len() - 1],
            Diagnostic::AuthorityTransferCompleted { epoch: 2 }
        );

        let mut stranger = join(id_from_byte(3), &idle).unwrap();
        let unchanged = stranger.encode_snapshot().unwrap();
        assert_eq!(
            stranger.activate(&grant, &post).unwrap_err().code,
            "RMT-E5007"
        );
        assert_eq!(stranger.encode_snapshot().unwrap(), unchanged);
        assert_eq!(
            stranger.diagnostics().last().copied(),
            Some(Diagnostic::AuthorityTransferFailed)
        );
    }
}
