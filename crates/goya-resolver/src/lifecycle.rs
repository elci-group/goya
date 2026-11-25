//! Instance lifecycle after a scene has been admitted.
//!
//! `UNLOADED`, `VALIDATING`, and `RESOLVING` happen before an [`crate::Instance`]
//! exists: a failed load returns a diagnostic and does not produce an instance.
//! Suspension, resumption, and termination are applied synchronously, so a
//! caller observes the stable phase at the end of each call.

/// Stable phase of one application instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Initialising,
    Running,
    Suspended,
    Terminated,
    Failed,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initialising => "initialising",
            Self::Running => "running",
            Self::Suspended => "suspended",
            Self::Terminated => "terminated",
            Self::Failed => "failed",
        }
    }
}
