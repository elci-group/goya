/// Modifier bits copied from a core X11 key or button event.
///
/// `alt` is Mod1. This is not an XKB group, and it is not a composed character.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub lock: bool,
    pub control: bool,
    pub alt: bool,
}

impl Modifiers {
    pub fn from_state(state: u16) -> Self {
        Self {
            shift: state & 0x1 != 0,
            lock: state & 0x2 != 0,
            control: state & 0x4 != 0,
            alt: state & 0x8 != 0,
        }
    }
}

/// Named keys the core keymap can identify without producing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Backspace,
    Tab,
    Return,
    Escape,
    Delete,
    Home,
    End,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
}

/// Result of one keycode lookup.
///
/// `text` is empty when Control is held, and for non-character keysyms.
/// The keysym stays the shift-level symbol so a host can tell Control+C from
/// the letter C without Kumimanu inserting a control character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterpretedKey {
    pub keysym: u32,
    pub text: Option<char>,
    pub named: Option<NamedKey>,
}

/// Core `GetKeyboardMapping` table. Group switches from the XKB extension are
/// not applied. `MappingNotify` replaces the whole table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    pub min_keycode: u8,
    pub per_keycode: u8,
    pub keysyms: Vec<u32>,
}

impl Keymap {
    pub fn interpret(&self, keycode: u8, modifiers: Modifiers) -> InterpretedKey {
        let keysym = self.lookup(keycode, modifiers.shift);
        let named = named_key(keysym);
        let mut text = if named.is_some() {
            None
        } else {
            latin1(keysym)
        };
        if modifiers.control {
            text = None;
        } else if modifiers.lock {
            if let Some(ch) = text {
                if ch.is_ascii_lowercase() {
                    text = Some(ch.to_ascii_uppercase());
                }
            }
        }
        InterpretedKey {
            keysym,
            text,
            named,
        }
    }

    fn lookup(&self, keycode: u8, shift: bool) -> u32 {
        let Some(index) = keycode.checked_sub(self.min_keycode) else {
            return 0;
        };
        let per = usize::from(self.per_keycode);
        if per == 0 {
            return 0;
        }
        let base = usize::from(index).saturating_mul(per);
        let column = if shift && per > 1 { 1 } else { 0 };
        let shifted = self.keysyms.get(base + column).copied().unwrap_or(0);
        if shifted == 0 && column == 1 {
            self.keysyms.get(base).copied().unwrap_or(0)
        } else {
            shifted
        }
    }
}

fn latin1(keysym: u32) -> Option<char> {
    if (0x20..0x7f).contains(&keysym) || (0xa0..=0xff).contains(&keysym) {
        char::from_u32(keysym)
    } else {
        None
    }
}

fn named_key(keysym: u32) -> Option<NamedKey> {
    Some(match keysym {
        0xff08 => NamedKey::Backspace,
        0xff09 => NamedKey::Tab,
        0xff0d | 0xff8d => NamedKey::Return,
        0xff1b => NamedKey::Escape,
        0xffff => NamedKey::Delete,
        0xff50 => NamedKey::Home,
        0xff51 => NamedKey::Left,
        0xff52 => NamedKey::Up,
        0xff53 => NamedKey::Right,
        0xff54 => NamedKey::Down,
        0xff55 => NamedKey::PageUp,
        0xff56 => NamedKey::PageDown,
        0xff57 => NamedKey::End,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Keymap {
        Keymap {
            min_keycode: 10,
            per_keycode: 2,
            // keycode 10 is 'a' / 'A'. keycode 11 is BackSpace with no shift symbol.
            keysyms: vec![0x61, 0x41, 0xff08, 0],
        }
    }

    #[test]
    fn shift_selects_the_second_column() {
        let map = sample();
        let plain = map.interpret(10, Modifiers::default());
        assert_eq!(plain.text, Some('a'));
        assert_eq!(plain.keysym, 0x61);
        let shifted = map.interpret(
            10,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        );
        assert_eq!(shifted.text, Some('A'));
    }

    #[test]
    fn control_keeps_the_keysym_and_drops_text() {
        let map = sample();
        let chord = map.interpret(
            10,
            Modifiers {
                control: true,
                ..Modifiers::default()
            },
        );
        assert_eq!(chord.keysym, 0x61);
        assert_eq!(chord.text, None);
    }

    #[test]
    fn lock_uppercases_a_letter_without_changing_the_keysym() {
        let map = sample();
        let locked = map.interpret(
            10,
            Modifiers {
                lock: true,
                ..Modifiers::default()
            },
        );
        assert_eq!(locked.keysym, 0x61);
        assert_eq!(locked.text, Some('A'));
    }

    #[test]
    fn backspace_is_named_and_has_no_text() {
        let key = sample().interpret(11, Modifiers::default());
        assert_eq!(key.named, Some(NamedKey::Backspace));
        assert_eq!(key.text, None);
    }

    #[test]
    fn a_keycode_outside_the_table_is_empty() {
        let key = sample().interpret(9, Modifiers::default());
        assert_eq!(key.keysym, 0);
        assert_eq!(key.text, None);
        assert_eq!(key.named, None);
    }
}
