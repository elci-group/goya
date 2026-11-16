//! Whether one Win32 clipboard message is denied.
//!
//! The flags come from Sash admission. This module does not read a clipboard,
//! change edit text, launch Windows, open a counter session, or change a
//! stored count.

/// Clipboard flags read from one admitted interactive scene.
pub struct AdmittedGate {
    pub copy_allowed: bool,
    pub paste_allowed: bool,
    pub cut_mode: u8,
    pub profile: &'static str,
}

/// Admit `source` and return its clipboard flags.
///
/// A static scene is `SASH-CAP`. Any other admission error is returned unchanged.
pub fn admit_gate(source: &str) -> Result<AdmittedGate, sash::Diagnostic> {
    let admitted = match sash::admit::open(source)? {
        sash::admit::SceneTarget::Interactive(admitted) => admitted,
        sash::admit::SceneTarget::Static(_) => {
            return Err(cap("this run admits only the interactive counter"));
        }
    };
    Ok(AdmittedGate {
        copy_allowed: admitted.copy_allowed,
        paste_allowed: admitted.paste_allowed,
        cut_mode: admitted.cut_mode,
        profile: admitted.profile,
    })
}

/// Whether this message is a denied clipboard operation.
///
/// The decision is `clipboard_blocked`. There is no other policy.
pub fn blocked(
    gate: &AdmittedGate,
    message: u32,
    wparam: usize,
    ctrl_down: bool,
    shift_down: bool,
) -> bool {
    sash::interactive::clipboard_blocked(sash::interactive::ClipboardGate {
        copy_allowed: gate.copy_allowed,
        paste_allowed: gate.paste_allowed,
        cut_mode: gate.cut_mode,
        message,
        wparam,
        ctrl_down,
        shift_down,
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

    const WM_CUT: u32 = 0x0300;
    const WM_COPY: u32 = 0x0301;
    const WM_PASTE: u32 = 0x0302;
    const WM_KEYDOWN: u32 = 0x0100;
    const VK_DELETE: usize = 0x2E;

    const STATIC_SOURCE: &str = include_str!("../../sash/fixtures/static-window.gscene");

    #[test]
    fn counter_allows_copy_and_denies_paste() {
        let gate = admit_gate(crate::COUNTER_SOURCE).expect("counter admits");
        assert_eq!(gate.profile, sash::interactive::PROFILE_CLIPBOARD);
        assert!(gate.copy_allowed);
        assert!(!gate.paste_allowed);
        assert_eq!(gate.cut_mode, sash::interactive::CUT_DEFAULT);
        assert!(blocked(&gate, WM_PASTE, 0, false, false));
        assert!(!blocked(&gate, WM_COPY, 0, false, false));
        assert!(!blocked(&gate, WM_KEYDOWN, VK_DELETE, false, true));
        assert!(!blocked(&gate, WM_KEYDOWN, VK_DELETE, false, false));
        assert_eq!(
            crate::row("clipboard_gate").status,
            crate::Status::SharedSemantic
        );
    }

    #[test]
    fn denied_cut_drops_shift_delete_and_wm_cut() {
        let source = crate::COUNTER_SOURCE.replace(
            "gestures { copy: allow; paste: deny; }",
            "gestures { copy: allow; paste: deny; cut: deny; }",
        );
        let gate = admit_gate(&source).expect("cut deny admits");
        assert_eq!(gate.cut_mode, sash::interactive::CUT_DENY);
        assert!(blocked(&gate, WM_KEYDOWN, VK_DELETE, false, true));
        assert!(blocked(&gate, WM_CUT, 0, false, false));
        assert!(!blocked(&gate, WM_COPY, 0, false, false));
    }

    #[test]
    fn static_window_is_not_admitted() {
        match admit_gate(STATIC_SOURCE) {
            Err(error) => {
                assert_eq!(error.code, "SASH-CAP");
                assert_eq!(
                    error.message,
                    "this run admits only the interactive counter"
                );
            }
            Ok(_) => panic!("static scene admitted a clipboard gate"),
        }
    }
}
