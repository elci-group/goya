//! Run the admitted interactive counter on the shared D1 owner.
//!
//! The integers come from Sash. This module does not invent a missing value,
//! and it does not open a window.

use std::collections::BTreeMap;

use goya_executable::D1Value;

/// Result of one checked add on the admitted interactive counter.
pub struct CounterRun {
    pub count: i64,
    pub revision: u64,
    pub profile: &'static str,
    pub copy_allowed: bool,
    pub paste_allowed: bool,
    pub width_dip: i32,
    pub height_dip: i32,
}

/// Admit `source` and add its authored integers on `goya_sdk::D1Application`.
///
/// A scene that is not the interactive counter, and any D1 failure, is `SASH-CAP`.
pub fn run_authored_add(source: &str) -> Result<CounterRun, sash::Diagnostic> {
    let admitted = match sash::admit::open(source)? {
        sash::admit::SceneTarget::Interactive(admitted) => admitted,
        sash::admit::SceneTarget::Static(_) => {
            return Err(cap("this run admits only the interactive counter"));
        }
    };
    let handoff = goya_lang::handoff::admit(source).map_err(|error| {
        cap(format!(
            "D1 error: shared scene was not available after admission: {error}"
        ))
    })?;
    let mut owner = goya_sdk::D1Application::new(
        &handoff.parsed.scene,
        BTreeMap::from([(
            admitted.state_key.clone(),
            D1Value::Int(admitted.initial_state),
        )]),
    )
    .map_err(|error| cap(format!("D1 error: {error}")))?;
    owner
        .dispatch(
            &admitted.action_id,
            BTreeMap::from([(
                admitted.input_key.clone(),
                D1Value::Int(admitted.initial_input),
            )]),
        )
        .map_err(|error| cap(format!("D1 error: {error}")))?;
    let count = match owner.values().get(&admitted.state_key) {
        Some(D1Value::Int(count)) => *count,
        other => {
            return Err(cap(format!(
                "D1 error: {} resolved to {other:?}, not an integer",
                admitted.state_key
            )));
        }
    };
    Ok(CounterRun {
        count,
        revision: owner.revision(),
        profile: admitted.profile,
        copy_allowed: admitted.copy_allowed,
        paste_allowed: admitted.paste_allowed,
        width_dip: admitted.width_dip,
        height_dip: admitted.height_dip,
    })
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

    const STATIC_SOURCE: &str = include_str!("../../sash/fixtures/static-window.gscene");

    #[test]
    fn counter_source_adds_the_authored_integers() {
        let run = run_authored_add(crate::COUNTER_SOURCE).expect("counter runs");
        assert_eq!(run.count, 1);
        assert_eq!(run.revision, 1);
        assert_eq!(run.profile, sash::interactive::PROFILE_CLIPBOARD);
        assert!(run.copy_allowed);
        assert!(!run.paste_allowed);
        assert_eq!((run.width_dip, run.height_dip), (320, 160));
    }

    #[test]
    fn undo_in_the_gesture_block_is_rejected() {
        let source = crate::COUNTER_SOURCE.replace(
            "gestures { copy: allow; paste: deny; }",
            "gestures { copy: allow; paste: deny; undo: deny; }",
        );
        assert!(run_authored_add(&source).is_err());
    }

    #[test]
    fn static_window_does_not_produce_a_count() {
        match run_authored_add(STATIC_SOURCE) {
            Err(error) => assert_eq!(error.code, "SASH-CAP"),
            Ok(run) => panic!("static scene produced count {}", run.count),
        }
    }
}
