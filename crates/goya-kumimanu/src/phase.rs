/// Lifecycle of one Kumimanu session.
///
/// `Failed` is the error sink. `Terminated` is the only clean end.
/// A session does not return to `Connected` after a window has been created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Connecting,
    Connected,
    WindowCreated,
    Mapped,
    Closing,
    Terminated,
    Failed,
}

/// One legal edge in the session lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Connected,
    Create,
    Map,
    Close,
    Destroyed,
    Fault,
}

/// Advance `phase` by `step`.
///
/// An illegal pair returns the unchanged phase so the caller can refuse it.
pub fn advance(phase: Phase, step: Step) -> Result<Phase, Phase> {
    let next = match (phase, step) {
        (Phase::Connecting, Step::Connected) => Phase::Connected,
        (Phase::Connecting, Step::Fault) => Phase::Failed,
        (Phase::Connected, Step::Create) => Phase::WindowCreated,
        (Phase::Connected, Step::Close) => Phase::Terminated,
        (Phase::Connected, Step::Fault) => Phase::Failed,
        (Phase::WindowCreated, Step::Map) => Phase::Mapped,
        (Phase::WindowCreated, Step::Close) => Phase::Closing,
        (Phase::WindowCreated, Step::Fault) => Phase::Failed,
        (Phase::Mapped, Step::Map) => Phase::Mapped,
        (Phase::Mapped, Step::Close) => Phase::Closing,
        (Phase::Mapped, Step::Fault) => Phase::Failed,
        (Phase::Closing, Step::Destroyed) => Phase::Terminated,
        (Phase::Closing, Step::Fault) => Phase::Failed,
        (Phase::Failed, Step::Destroyed) => Phase::Terminated,
        (Phase::Terminated, Step::Destroyed) => Phase::Terminated,
        _ => return Err(phase),
    };
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_reaches_terminated() {
        let mut phase = Phase::Connecting;
        for step in [
            Step::Connected,
            Step::Create,
            Step::Map,
            Step::Close,
            Step::Destroyed,
        ] {
            phase = advance(phase, step).expect("legal");
        }
        assert_eq!(phase, Phase::Terminated);
    }

    #[test]
    fn map_before_create_stays_put() {
        let err = advance(Phase::Connected, Step::Map).unwrap_err();
        assert_eq!(err, Phase::Connected);
    }

    #[test]
    fn fault_from_mapped_is_failed() {
        assert_eq!(advance(Phase::Mapped, Step::Fault).unwrap(), Phase::Failed);
    }

    #[test]
    fn repeated_map_notify_stays_mapped() {
        assert_eq!(advance(Phase::Mapped, Step::Map).unwrap(), Phase::Mapped);
    }

    #[test]
    fn close_without_a_window_terminates() {
        assert_eq!(
            advance(Phase::Connected, Step::Close).unwrap(),
            Phase::Terminated
        );
    }
}
