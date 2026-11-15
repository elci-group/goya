// Keyboard and shutdown decisions for the interactive window.
//
// This file is included by the host compiler and pasted into the generated
// executable. Space is not an activation chord: it is delivered to whichever
// control has focus. The button's own handler activates on Space.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyAction {
    Activate,
    FocusNext,
    FocusPrev,
    Dispatch,
}

pub fn key_action(message: u32, wparam: usize, shift_down: bool) -> KeyAction {
    const WM_KEYDOWN: u32 = 0x0100;
    const VK_TAB: usize = 9;
    const VK_RETURN: usize = 13;
    if message != WM_KEYDOWN {
        return KeyAction::Dispatch;
    }
    match wparam {
        VK_TAB if shift_down => KeyAction::FocusPrev,
        VK_TAB => KeyAction::FocusNext,
        VK_RETURN => KeyAction::Activate,
        _ => KeyAction::Dispatch,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PumpEnd {
    NotEntered,
    Quit(i32),
    Failed,
}

pub struct ShutdownPlan {
    pub destroy_window: bool,
    pub unregister_class: bool,
    pub exit_code: i32,
}

pub fn shutdown_plan(class_registered: bool, window_live: bool, end: PumpEnd) -> ShutdownPlan {
    let exit_code = match end {
        PumpEnd::Quit(code) => code,
        PumpEnd::Failed | PumpEnd::NotEntered => 1,
    };
    ShutdownPlan {
        destroy_window: window_live && matches!(end, PumpEnd::Failed),
        unregister_class: class_registered,
        exit_code,
    }
}
