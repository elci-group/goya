//! Sash Win32 key policy on an admitted counter session.
//!
//! Enter and Space on the button publish through `CounterSession`. Space on
//! the edit does not. This module does not launch Windows or bind Wayland.

const WM_KEYDOWN: u32 = 0x0100;
const VK_SPACE: usize = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Edit,
    Button,
}

pub struct KeyController {
    pub session: crate::session::CounterSession,
    pub focus: Focus,
    /// `commit` needs the expression. `CounterSession` does not expose it.
    expr: sash::interactive::IntExpr,
}

impl KeyController {
    /// Admit `source` and start focus on the edit.
    pub fn open(source: &str) -> Result<Self, sash::Diagnostic> {
        let session = crate::session::CounterSession::open(source)?;
        let expr = match sash::admit::open(source)? {
            sash::admit::SceneTarget::Interactive(admitted) => admitted.expr,
            sash::admit::SceneTarget::Static(_) => {
                return Err(sash::Diagnostic::new(
                    "SASH-CAP",
                    "admit",
                    "this run admits only the interactive counter",
                    "Author the interactive counter. This run uses the integers Sash parsed and does not infer a missing value.",
                ));
            }
        };
        Ok(Self {
            session,
            focus: Focus::Edit,
            expr,
        })
    }

    /// Apply one Win32 key message. The returned action is Sash's decision.
    pub fn on_key(
        &mut self,
        message: u32,
        wparam: usize,
        shift_down: bool,
    ) -> sash::interactive::KeyAction {
        let action = sash::interactive::key_action(message, wparam, shift_down);
        match action {
            sash::interactive::KeyAction::FocusNext => {
                self.focus = match self.focus {
                    Focus::Edit => Focus::Button,
                    Focus::Button => Focus::Edit,
                };
            }
            sash::interactive::KeyAction::FocusPrev => {
                self.focus = match self.focus {
                    Focus::Button => Focus::Edit,
                    Focus::Edit => Focus::Button,
                };
            }
            sash::interactive::KeyAction::Activate => self.activate(),
            sash::interactive::KeyAction::Dispatch => {
                if self.focus == Focus::Button && wparam == VK_SPACE && message == WM_KEYDOWN {
                    self.activate();
                }
            }
        }
        action
    }

    /// Publish when the edit parses. A failed commit still asks the session.
    fn activate(&mut self) {
        let visible =
            sash::interactive::parse_canonical_i64(&self.session.edit).and_then(|input| {
                sash::interactive::commit(
                    &self.expr,
                    self.session.count,
                    self.session.revision,
                    input,
                )
                .map(|(next, _)| next)
            });
        self.session.activate(false, true, false, visible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sash::interactive::KeyAction;

    const VK_TAB: usize = 9;
    const VK_RETURN: usize = 13;

    #[test]
    fn open_starts_on_the_edit() {
        let controller = KeyController::open(crate::COUNTER_SOURCE).expect("counter opens");
        assert_eq!(controller.focus, Focus::Edit);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn tab_then_shift_tab_returns_to_the_edit() {
        let mut controller = KeyController::open(crate::COUNTER_SOURCE).expect("counter opens");
        let next = controller.on_key(WM_KEYDOWN, VK_TAB, false);
        assert_eq!(next, KeyAction::FocusNext);
        assert_eq!(controller.focus, Focus::Button);
        let prev = controller.on_key(WM_KEYDOWN, VK_TAB, true);
        assert_eq!(prev, KeyAction::FocusPrev);
        assert_eq!(controller.focus, Focus::Edit);
        assert_eq!(controller.session.count, 0);
    }

    #[test]
    fn enter_and_space_follow_button_focus() {
        let mut controller = KeyController::open(crate::COUNTER_SOURCE).expect("counter opens");

        let first = controller.on_key(WM_KEYDOWN, VK_RETURN, false);
        assert_eq!(first, KeyAction::Activate);
        assert_eq!(controller.session.count, 1);
        assert_eq!(controller.session.revision, 1);

        let second = controller.on_key(WM_KEYDOWN, VK_RETURN, false);
        assert_eq!(second, KeyAction::Activate);
        assert_eq!(controller.session.count, 2);
        assert_eq!(controller.session.revision, 2);

        let space_on_edit = controller.on_key(WM_KEYDOWN, VK_SPACE, false);
        assert_eq!(space_on_edit, KeyAction::Dispatch);
        assert_eq!(controller.focus, Focus::Edit);
        assert_eq!(controller.session.count, 2);
        assert_eq!(controller.session.edit, "1");

        let tab = controller.on_key(WM_KEYDOWN, VK_TAB, false);
        assert_eq!(tab, KeyAction::FocusNext);
        assert_eq!(controller.focus, Focus::Button);
        let space_on_button = controller.on_key(WM_KEYDOWN, VK_SPACE, false);
        assert_eq!(space_on_button, KeyAction::Dispatch);
        assert_eq!(controller.session.count, 3);
        assert_eq!(controller.session.revision, 3);
    }

    #[test]
    fn non_keydown_return_is_dispatch() {
        let mut controller = KeyController::open(crate::COUNTER_SOURCE).expect("counter opens");
        controller.on_key(WM_KEYDOWN, VK_RETURN, false);
        controller.on_key(WM_KEYDOWN, VK_RETURN, false);
        assert_eq!(controller.session.count, 2);
        let action = controller.on_key(0, VK_RETURN, false);
        assert_eq!(action, KeyAction::Dispatch);
        assert_eq!(controller.session.count, 2);
        assert_eq!(controller.session.revision, 2);
    }

    #[test]
    fn win32_keys_row_is_admitted_and_unlaunched() {
        let row = crate::row("win32_keys");
        assert_eq!(row.class, crate::Class::PlatformNative);
        assert_eq!(row.status, crate::Status::WindowsAdmittedUnlaunched);
    }
}
