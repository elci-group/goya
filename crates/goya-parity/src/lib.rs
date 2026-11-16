//! Shared conformance gate for the Linux native pipeline and Sash.
//!
//! The gate records observable behaviour for one shared source. It does not
//! launch a window, compare pixels, or admit a release.

#![forbid(unsafe_code)]

pub mod clipboard_gate;
pub mod keys;
pub mod pointer;
pub mod run;
pub mod session;

/// Programme that owns this gate.
pub const PROGRAMME_ID: &str = "GOYA-PARITY-LAB/0";

/// The only shared scene this gate scores.
pub const COUNTER_SOURCE: &str = include_str!("../../sash/fixtures/interactive-counter.gscene");

/// Which kind of equivalence a row belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Functional,
    Visual,
    PlatformNative,
}

/// Measured disposition. A passing test records the disposition. It does not
/// promote an unlaunched or absent row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The shared D1 owner and the Windows admission agree on the result.
    SharedSemantic,
    /// Sash admits the behaviour. No Windows process has run it.
    WindowsAdmittedUnlaunched,
    /// The scene syntax or the executable path does not carry it.
    Absent,
    /// No measurement and no agreed tolerance.
    NotCompared,
    /// A headless Linux process ran the admitted action. It did not open a window.
    HeadlessLinux,
}

/// One required observable of the parity programme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub id: &'static str,
    pub class: Class,
    pub status: Status,
    pub evidence: &'static str,
}

/// Current ledger. Phase exits stay unmet while any required row is
/// `Absent`, `WindowsAdmittedUnlaunched`, or `NotCompared`.
pub const ROWS: &[Row] = &[
    Row {
        id: "d1_add",
        class: Class::Functional,
        status: Status::SharedSemantic,
        evidence: "run_authored_add reads the integers Sash parsed from authored text. The shared D1 owner stores the checked sum.",
    },
    Row {
        id: "authored_initial_not_inferred_by_d1_owner",
        class: Class::Functional,
        status: Status::SharedSemantic,
        evidence: "D1Application rejects an empty initial map. run_authored_add supplies only the integers Sash parsed from authored text.",
    },
    Row {
        id: "repeated_activation",
        class: Class::Functional,
        status: Status::SharedSemantic,
        evidence: "CounterSession uses Sash commit, activation, and visible_commit. A rejected edit, overflow, busy activation, release, or mismatched readback leaves the stored count unchanged. This row is not a Windows launch.",
    },
    Row {
        id: "copy_paste_policy",
        class: Class::PlatformNative,
        status: Status::WindowsAdmittedUnlaunched,
        evidence: "sash.win32.interactive/0.0.2 admits copy allow and paste deny. The Linux host has no executable for this scene.",
    },
    Row {
        id: "undo_redo",
        class: Class::Functional,
        status: Status::Absent,
        evidence: "The gesture resolver names Undo and Redo. The scene parser accepts only copy, cut, and paste.",
    },
    Row {
        id: "document_save_and_restore",
        class: Class::Functional,
        status: Status::Absent,
        evidence: "The admitted counter has one D1 action. It has no document path and no restore file.",
    },
    Row {
        id: "theme",
        class: Class::Visual,
        status: Status::Absent,
        evidence: "The counter scene authors no theme. goya-chroma is not applied to this source.",
    },
    Row {
        id: "layout_dip",
        class: Class::Visual,
        status: Status::WindowsAdmittedUnlaunched,
        evidence: "80 by 40 points become 320 by 160 DIP at MP_PER_DIP 250. Native font rasterisation has not been compared.",
    },
    Row {
        id: "linux_executable",
        class: Class::PlatformNative,
        status: Status::HeadlessLinux,
        evidence: "parity-counter runs the admitted add and prints the count, revision, profile, and copy/paste flags. It does not open a Wayland window.",
    },
    Row {
        id: "windows_launch",
        class: Class::PlatformNative,
        status: Status::WindowsAdmittedUnlaunched,
        evidence: "Sash cross-links the counter. verify does not launch it. Windows execution remains BLOCKED.",
    },
    Row {
        id: "win32_keys",
        class: Class::PlatformNative,
        status: Status::WindowsAdmittedUnlaunched,
        evidence: "KeyController uses sash key_action. Enter and Space on the button store through CounterSession. Space on the edit does not. This has not been launched on Windows.",
    },
    Row {
        id: "visual_tolerance",
        class: Class::Visual,
        status: Status::NotCompared,
        evidence: "No pixel or geometry baseline has been collected, so no tolerance is declared.",
    },
    Row {
        id: "electron_comparison",
        class: Class::Functional,
        status: Status::NotCompared,
        evidence: "Goya Linux versus Goya Windows has not passed. No Electron workload is scored.",
    },
    Row {
        id: "clipboard_gate",
        class: Class::PlatformNative,
        status: Status::SharedSemantic,
        evidence: "admit_gate reads the Sash clipboard flags and blocked calls clipboard_blocked, so paste is denied and copy is allowed for the counter. A denied cut drops Shift+Delete. The default cut does not. The Win32 edit has not been launched.",
    },
    Row {
        id: "pointer_hit",
        class: Class::Visual,
        status: Status::SharedSemantic,
        evidence: "Hit-testing walks the admitted DIP rectangles from front to back. This is not a pixel comparison, and no window was opened.",
    },
];

pub fn row(id: &str) -> &'static Row {
    ROWS.iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("unknown parity row {id}"))
}

/// Phase 1 through 6. None have reached their exit condition.
pub fn phase_exit_met(phase: u8) -> bool {
    let _ = phase;
    false
}

/// Headless Linux process for the admitted add. It is not a Wayland window.
pub fn linux_executable_for_counter() -> Option<&'static str> {
    Some("parity-counter")
}

/// Windows execution of the linked image.
pub fn windows_execution() -> &'static str {
    "BLOCKED"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use goya_executable::D1Value;

    #[test]
    fn counter_add_matches_when_sash_initials_are_supplied() {
        let opened = sash::admit::open(COUNTER_SOURCE).expect("counter admits");
        let sash::admit::SceneTarget::Interactive(admitted) = opened else {
            panic!("counter is the interactive profile");
        };
        assert_eq!(admitted.profile, sash::interactive::PROFILE_CLIPBOARD);
        assert_eq!(admitted.initial_state, 0);
        assert_eq!(admitted.initial_input, 1);
        assert_eq!(admitted.action_id, "action.add");
        assert_eq!(admitted.width_dip, 320);
        assert_eq!(admitted.height_dip, 160);
        assert!(admitted.copy_allowed);
        assert!(!admitted.paste_allowed);

        let handoff = goya_lang::handoff::admit(COUNTER_SOURCE).expect("shared admission");
        assert_eq!(handoff.contract, goya_lang::handoff::CONTRACT);
        let scene = &handoff.parsed.scene;
        assert!(matches!(
            goya_sdk::D1Application::new(scene, BTreeMap::new()),
            Err(goya_sdk::D1ApplicationError::StateSchemaMismatch)
        ));
        let mut owner = goya_sdk::D1Application::new(
            scene,
            BTreeMap::from([(
                admitted.state_key.clone(),
                D1Value::Int(admitted.initial_state),
            )]),
        )
        .expect("explicit initial state");
        owner
            .dispatch(
                &admitted.action_id,
                BTreeMap::from([(
                    admitted.input_key.clone(),
                    D1Value::Int(admitted.initial_input),
                )]),
            )
            .expect("checked add");
        assert_eq!(owner.revision(), 1);
        assert_eq!(owner.values()[&admitted.state_key], D1Value::Int(1));
        assert_eq!(row("d1_add").status, Status::SharedSemantic);
        assert_eq!(
            row("authored_initial_not_inferred_by_d1_owner").status,
            Status::SharedSemantic
        );
        assert_eq!(row("layout_dip").status, Status::WindowsAdmittedUnlaunched);
        assert_eq!(
            row("copy_paste_policy").status,
            Status::WindowsAdmittedUnlaunched
        );
    }

    #[test]
    fn undo_save_and_theme_are_not_admitted() {
        let undo = COUNTER_SOURCE.replace(
            "gestures { copy: allow; paste: deny; }",
            "gestures { copy: allow; paste: deny; undo: deny; }",
        );
        assert!(goya_lang::handoff::admit(&undo).is_err());
        assert!(sash::admit::open(&undo).is_err());
        assert_eq!(row("undo_redo").status, Status::Absent);
        assert_eq!(row("document_save_and_restore").status, Status::Absent);
        assert_eq!(row("theme").status, Status::Absent);
        assert_eq!(linux_executable_for_counter(), Some("parity-counter"));
        assert_eq!(windows_execution(), "BLOCKED");
        assert_eq!(row("linux_executable").status, Status::HeadlessLinux);
        assert_eq!(
            row("windows_launch").status,
            Status::WindowsAdmittedUnlaunched
        );
        assert_eq!(row("visual_tolerance").status, Status::NotCompared);
        assert_eq!(row("electron_comparison").status, Status::NotCompared);
    }

    #[test]
    fn phase_exits_remain_unmet() {
        assert_eq!(PROGRAMME_ID, "GOYA-PARITY-LAB/0");
        for phase in 1..=6 {
            assert!(!phase_exit_met(phase), "phase {phase}");
        }
        assert!(ROWS.iter().any(|row| matches!(
            row.status,
            Status::Absent | Status::WindowsAdmittedUnlaunched | Status::NotCompared
        )));
    }
}
