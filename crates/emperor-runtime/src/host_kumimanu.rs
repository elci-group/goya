//! X11 host for the admitted counter.
//!
//! Emperor still lowers the scene. This module only presents that scene
//! through Kumimanu. IME, `PRIMARY`, and high-DPI scaling are not
//! implemented here. Copy and cut publish the same text as the Wayland host
//! by owning the X11 `CLIPBOARD` selection. Paste converts that selection to
//! UTF-8 text and applies it through the same `host::on_clipboard_text` gate,
//! so a denied paste still ignores every payload.

use std::time::Duration;

use goya_kumimanu::{logical_xy, Event, NamedKey, Session, WindowConfig};
use goya_parity::keys::KeyController;
use goya_wayland::{KeyModifiers, KeySpecial, PlatformEvent, Shortcut};

use crate::host::{self, WindowMode};
use crate::{NativeIr, OracleReport};

pub(crate) fn run_window(
    ir: &NativeIr,
    mut controller: KeyController,
    mode: WindowMode,
) -> Result<OracleReport, String> {
    let _ = host::scene_frame(ir, &controller)?;
    let hits = goya_parity::pointer::hit_map(&ir.scene).map_err(|err| err.to_string())?;
    let client_text = host::find_role(ir, "client")?.text.as_str();
    let title = if client_text.is_empty() {
        "Count"
    } else {
        client_text
    };
    let width = u16::try_from(ir.width_dip).map_err(|_| "kumimanu: width is out of range")?;
    let height = u16::try_from(ir.height_dip).map_err(|_| "kumimanu: height is out of range")?;
    let mut session = Session::connect().map_err(|err| format!("kumimanu: {err}"))?;
    let id = session
        .create_window(WindowConfig::new(title, width, height))
        .map_err(|err| format!("kumimanu: {err}"))?;
    session
        .show_window(id)
        .map_err(|err| format!("kumimanu: {err}"))?;
    let mut arm = host::PointerArm::default();
    if matches!(mode, WindowMode::Exercise) {
        crate::exercise::apply_visible_script(ir, &mut controller, &hits, &mut arm)?;
    }
    let mut dirty = true;
    let mut presented = 0u32;
    let mut pumps = 0u32;
    let mut quit = false;
    loop {
        if let WindowMode::Live {
            max_frames: Some(limit),
        } = mode
        {
            if pumps >= limit {
                break;
            }
        }
        let timeout = if dirty {
            Duration::ZERO
        } else {
            Duration::from_millis(16)
        };
        for event in session
            .poll_events(timeout)
            .map_err(|err| format!("kumimanu: {err}"))?
        {
            // A release may arrive after the pointer leaves this window. Do
            // not let a press armed before focus loss activate a control if
            // focus returns before that release is delivered.
            if matches!(&event, Event::Focus { focused: false }) {
                arm = host::PointerArm::default();
            }
            if matches!(event, Event::Exposed | Event::Mapped) {
                dirty = true;
            }
            if let Some(platform) = translate(event) {
                let mut published: Option<String> = None;
                let mut paste_requested = false;
                let step = host::on_event(
                    ir,
                    &mut controller,
                    &hits,
                    &mut arm,
                    &mut |text| published = Some(text.to_owned()),
                    &mut || paste_requested = true,
                    platform,
                );
                if let Some(text) = published {
                    // Ok(false) means another client kept the selection or
                    // the text is outside the limits. Nothing is owned then.
                    session
                        .set_clipboard_text(id, &text)
                        .map_err(|err| format!("kumimanu: {err}"))?;
                }
                if paste_requested {
                    session
                        .request_clipboard_text(id)
                        .map_err(|err| format!("kumimanu: {err}"))?;
                }
                match step {
                    host::Step::Changed => dirty = true,
                    host::Step::Quit => quit = true,
                    host::Step::Unchanged => {}
                }
            }
        }
        if dirty {
            let frame = host::scene_frame(ir, &controller)?;
            let (frame_w, frame_h) = session.size().unwrap_or((width, height));
            let rgba = crate::glyph::paint(&frame, frame_w, frame_h)?;
            session
                .present_rgba(id, frame_w, frame_h, &rgba)
                .map_err(|err| format!("kumimanu: {err}"))?;
            dirty = false;
            presented = presented.saturating_add(1);
        }
        if quit {
            break;
        }
        pumps = pumps.saturating_add(1);
        if matches!(mode, WindowMode::Smoke | WindowMode::Exercise) && presented >= 1 {
            break;
        }
    }
    session
        .close_window(id)
        .map_err(|err| format!("kumimanu: {err}"))?;
    Ok(OracleReport {
        count: controller.session.count,
        revision: controller.session.revision,
        copy_allowed: controller.session.copy_allowed,
        paste_allowed: controller.session.paste_allowed,
        presented,
        wayland: "not-used",
        window: "kumimanu",
        edit: controller.session.edit,
    })
}

fn translate(event: Event) -> Option<PlatformEvent> {
    match event {
        Event::Key { pressed: false, .. } => None,
        Event::Key {
            named: Some(NamedKey::Tab),
            modifiers,
            ..
        } => {
            if modifiers.shift || modifiers.control {
                Some(PlatformEvent::KeySpecialWithModifiers {
                    key: KeySpecial::Tab,
                    modifiers: host_modifiers(modifiers.shift, modifiers.control),
                })
            } else {
                Some(PlatformEvent::KeySpecial(KeySpecial::Tab))
            }
        }
        Event::Key {
            named: Some(NamedKey::Return),
            ..
        } => Some(PlatformEvent::KeyChar('\n')),
        Event::Key {
            named: Some(NamedKey::Backspace),
            ..
        } => Some(PlatformEvent::KeyChar('\u{8}')),
        Event::Key {
            named: Some(NamedKey::Delete),
            modifiers,
            ..
        } => Some(PlatformEvent::KeySpecialWithModifiers {
            key: KeySpecial::Delete,
            modifiers: host_modifiers(modifiers.shift, modifiers.control),
        }),
        Event::Key {
            keysym,
            text,
            modifiers,
            ..
        } => {
            if modifiers.control && !modifiers.shift {
                return match latin(keysym).map(|ch| ch.to_ascii_lowercase()) {
                    Some('c') => Some(PlatformEvent::Shortcut(Shortcut::Copy)),
                    Some('v') => Some(PlatformEvent::Shortcut(Shortcut::Paste)),
                    Some('x') => Some(PlatformEvent::Shortcut(Shortcut::Cut)),
                    Some('q') => Some(PlatformEvent::Shortcut(Shortcut::Quit)),
                    _ => None,
                };
            }
            if modifiers.control {
                return None;
            }
            text.map(PlatformEvent::KeyChar)
        }
        Event::PointerButton {
            x,
            y,
            button: 1,
            pressed,
        } => {
            let (x, y) = logical_xy(x, y);
            if pressed {
                Some(PlatformEvent::PointerPress(x, y))
            } else {
                Some(PlatformEvent::PointerRelease(x, y))
            }
        }
        Event::Resized { width, height } => {
            Some(PlatformEvent::Resized(i32::from(width), i32::from(height)))
        }
        Event::CloseRequested => Some(PlatformEvent::CloseRequested),
        Event::ClipboardText(text) => Some(PlatformEvent::ClipboardText(text)),
        // PRIMARY answers do not enter the paste gate. Paste is a CLIPBOARD
        // conversion; middle-click primary paste is not part of this profile.
        Event::PrimaryText(_) => None,
        Event::Mapped
        | Event::Exposed
        | Event::PointerMove { .. }
        | Event::PointerButton { .. }
        | Event::Scroll { .. }
        | Event::Focus { .. } => None,
    }
}

fn host_modifiers(shift: bool, ctrl: bool) -> KeyModifiers {
    KeyModifiers { shift, ctrl }
}

fn latin(keysym: u32) -> Option<char> {
    if (0x20..0x7f).contains(&keysym) {
        char::from_u32(keysym)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_kumimanu::Modifiers;

    #[test]
    fn control_c_is_copy_and_a_plain_digit_is_text() {
        let copy = translate(Event::Key {
            keycode: 1,
            keysym: u32::from(b'c'),
            text: None,
            named: None,
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
            pressed: true,
            repeat: false,
        });
        assert_eq!(copy, Some(PlatformEvent::Shortcut(Shortcut::Copy)));
        let digit = translate(Event::Key {
            keycode: 2,
            keysym: u32::from(b'2'),
            text: Some('2'),
            named: None,
            modifiers: Modifiers::default(),
            pressed: true,
            repeat: false,
        });
        assert_eq!(digit, Some(PlatformEvent::KeyChar('2')));
    }

    #[test]
    fn clipboard_answers_reach_the_shared_paste_gate() {
        assert_eq!(
            translate(Event::ClipboardText(Some("7".into()))),
            Some(PlatformEvent::ClipboardText(Some("7".into())))
        );
        assert_eq!(
            translate(Event::ClipboardText(None)),
            Some(PlatformEvent::ClipboardText(None))
        );
    }

    #[test]
    fn remaining_control_shortcuts_are_copy_paste_cut_quit() {
        let cases = [
            (b'v', Shortcut::Paste),
            (b'x', Shortcut::Cut),
            (b'q', Shortcut::Quit),
        ];
        for (keysym, expected) in cases {
            let event = translate(Event::Key {
                keycode: 3,
                keysym: u32::from(keysym),
                text: None,
                named: None,
                modifiers: Modifiers {
                    control: true,
                    ..Modifiers::default()
                },
                pressed: true,
                repeat: false,
            });
            assert_eq!(event, Some(PlatformEvent::Shortcut(expected)));
        }
    }

    #[test]
    fn named_keys_translate_to_specials_or_characters() {
        let named = |name, modifiers| {
            translate(Event::Key {
                keycode: 4,
                keysym: 0,
                text: None,
                named: Some(name),
                modifiers,
                pressed: true,
                repeat: false,
            })
        };
        assert_eq!(
            named(NamedKey::Tab, Modifiers::default()),
            Some(PlatformEvent::KeySpecial(KeySpecial::Tab))
        );
        assert_eq!(
            named(NamedKey::Return, Modifiers::default()),
            Some(PlatformEvent::KeyChar('\n'))
        );
        assert_eq!(
            named(NamedKey::Backspace, Modifiers::default()),
            Some(PlatformEvent::KeyChar('\u{8}'))
        );
        assert_eq!(
            named(NamedKey::Delete, Modifiers::default()),
            Some(PlatformEvent::KeySpecialWithModifiers {
                key: KeySpecial::Delete,
                modifiers: KeyModifiers {
                    shift: false,
                    ctrl: false
                },
            })
        );
    }

    #[test]
    fn modifiered_tab_and_delete_carry_the_modifiers() {
        let shift_ctrl = Modifiers {
            shift: true,
            control: true,
            ..Modifiers::default()
        };
        assert_eq!(
            translate(Event::Key {
                keycode: 5,
                keysym: 0,
                text: None,
                named: Some(NamedKey::Tab),
                modifiers: shift_ctrl,
                pressed: true,
                repeat: false,
            }),
            Some(PlatformEvent::KeySpecialWithModifiers {
                key: KeySpecial::Tab,
                modifiers: KeyModifiers {
                    shift: true,
                    ctrl: true
                },
            })
        );
        // shift-only tab behaves the same way.
        assert_eq!(
            translate(Event::Key {
                keycode: 6,
                keysym: 0,
                text: None,
                named: Some(NamedKey::Tab),
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
                pressed: true,
                repeat: false,
            }),
            Some(PlatformEvent::KeySpecialWithModifiers {
                key: KeySpecial::Tab,
                modifiers: KeyModifiers {
                    shift: true,
                    ctrl: false
                },
            })
        );
        assert_eq!(
            translate(Event::Key {
                keycode: 7,
                keysym: 0,
                text: None,
                named: Some(NamedKey::Delete),
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
                pressed: true,
                repeat: false,
            }),
            Some(PlatformEvent::KeySpecialWithModifiers {
                key: KeySpecial::Delete,
                modifiers: KeyModifiers {
                    shift: true,
                    ctrl: false
                },
            })
        );
    }

    #[test]
    fn released_keys_and_control_with_shift_are_ignored() {
        assert_eq!(
            translate(Event::Key {
                keycode: 8,
                keysym: 0,
                text: None,
                named: None,
                modifiers: Modifiers::default(),
                pressed: false,
                repeat: false,
            }),
            None
        );
        // ctrl+shift swallows ordinary keys, matching the shortcut gate.
        assert_eq!(
            translate(Event::Key {
                keycode: 9,
                keysym: u32::from(b'a'),
                text: Some('a'),
                named: None,
                modifiers: Modifiers {
                    control: true,
                    shift: true,
                    ..Modifiers::default()
                },
                pressed: true,
                repeat: false,
            }),
            None
        );
    }
}
