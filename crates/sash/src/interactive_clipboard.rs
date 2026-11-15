// Edit clipboard gate for sash.win32.interactive/0.0.2.
//
// This file is included by the host compiler and pasted into the generated
// executable. It decides whether one Win32 message is a denied clipboard
// operation. It does not read the clipboard.

pub const CUT_DEFAULT: u8 = 0;
pub const CUT_ALLOW: u8 = 1;
pub const CUT_DENY: u8 = 2;

const OP_COPY: u8 = 1;
const OP_PASTE: u8 = 2;
const OP_CUT: u8 = 3;

pub struct ClipboardGate {
    pub copy_allowed: bool,
    pub paste_allowed: bool,
    pub cut_mode: u8,
    pub message: u32,
    pub wparam: usize,
    pub ctrl_down: bool,
    pub shift_down: bool,
}

pub fn clipboard_blocked(gate: ClipboardGate) -> bool {
    match operation(gate.message, gate.wparam, gate.ctrl_down, gate.shift_down) {
        Some(OP_COPY) => !gate.copy_allowed,
        Some(OP_PASTE) => !gate.paste_allowed,
        Some(OP_CUT) => gate.cut_mode == CUT_DENY,
        _ => false,
    }
}

fn operation(message: u32, wparam: usize, ctrl_down: bool, shift_down: bool) -> Option<u8> {
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_CHAR: u32 = 0x0102;
    const WM_CUT: u32 = 0x0300;
    const WM_COPY: u32 = 0x0301;
    const WM_PASTE: u32 = 0x0302;
    match message {
        WM_COPY => Some(OP_COPY),
        WM_PASTE => Some(OP_PASTE),
        WM_CUT => Some(OP_CUT),
        WM_CHAR => match wparam {
            0x03 => Some(OP_COPY),
            0x16 => Some(OP_PASTE),
            0x18 => Some(OP_CUT),
            _ => None,
        },
        WM_KEYDOWN => key_operation(wparam, ctrl_down, shift_down),
        _ => None,
    }
}

fn key_operation(wparam: usize, ctrl_down: bool, shift_down: bool) -> Option<u8> {
    const VK_C: usize = 0x43;
    const VK_V: usize = 0x56;
    const VK_X: usize = 0x58;
    const VK_INSERT: usize = 0x2D;
    const VK_DELETE: usize = 0x2E;
    if ctrl_down && !shift_down && (wparam == VK_C || wparam == VK_INSERT) {
        Some(OP_COPY)
    } else if (ctrl_down && !shift_down && wparam == VK_V)
        || (!ctrl_down && shift_down && wparam == VK_INSERT)
    {
        Some(OP_PASTE)
    } else if ctrl_down && !shift_down && wparam == VK_X {
        Some(OP_CUT)
    } else if !ctrl_down && shift_down && wparam == VK_DELETE {
        // Delete alone removes text without copying it. Shift+Delete is the edit control's cut chord.
        Some(OP_CUT)
    } else {
        None
    }
}
