//! Wayland host loop for the admitted counter.
//!
//! Hit rectangles stay at the admitted DIP values. Resize marks the frame
//! dirty and does not move them. Preedit and commit events are ignored.

use goya_layout::Rect;
use goya_parity::keys::{Focus, KeyController};
use goya_parity::pointer::HitMap;
use goya_runtime::Primitive;
use goya_wayland::{GoyaWindow, KeySpecial, PlatformEvent, Shortcut};
use unicode_segmentation::UnicodeSegmentation;

use crate::{press_add, Element, NativeIr, OracleReport, VK_SPACE, VK_TAB, WM_KEYDOWN};

const FOCUS_FILL: u32 = 0xFFD6E4F5;
const FG: [u8; 3] = [20, 20, 28];
const BG: [u8; 3] = [235, 235, 240];
const GLYPH_PX: u32 = 8;

pub(crate) enum WindowMode {
    Smoke,
    Live { max_frames: Option<u32> },
}

enum Step {
    Unchanged,
    Changed,
    Quit,
}

struct SceneFrame {
    primitives: Vec<Primitive>,
    fills: Vec<Option<u32>>,
    foregrounds: Vec<Option<u32>>,
}

#[derive(PartialEq, Eq)]
struct View {
    count: i64,
    revision: u64,
    edit: String,
    result: String,
    focus: Focus,
}

pub(crate) fn run_window(
    ir: &NativeIr,
    mut controller: KeyController,
    mode: WindowMode,
) -> Result<OracleReport, String> {
    let _ = scene_frame(ir, &controller)?;
    let hits = goya_parity::pointer::hit_map(&ir.scene).map_err(|err| err.to_string())?;
    let client_text = find_role(ir, "client")?.text.as_str();
    let title = if client_text.is_empty() {
        "Count"
    } else {
        client_text
    };
    let mut window = GoyaWindow::create(title, ir.width_dip, ir.height_dip)
        .map_err(|err| format!("wayland: {err}"))?;

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
        if window.close_requested() {
            break;
        }
        for event in window.poll() {
            match on_event(ir, &mut controller, &hits, &mut window, event) {
                Step::Changed => dirty = true,
                Step::Quit => quit = true,
                Step::Unchanged => {}
            }
        }
        if dirty {
            let frame = scene_frame(ir, &controller)?;
            window
                .present_dirty(
                    &frame.primitives,
                    &frame.fills,
                    &frame.foregrounds,
                    FG,
                    BG,
                    GLYPH_PX,
                    None,
                )
                .map_err(|err| format!("wayland: {err}"))?;
            dirty = false;
            presented = presented.saturating_add(1);
        } else if matches!(mode, WindowMode::Live { .. }) && !quit {
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        if quit || window.close_requested() {
            break;
        }
        window.pump(1);
        pumps = pumps.saturating_add(1);
        if window.close_requested() {
            break;
        }
        if matches!(mode, WindowMode::Smoke) && presented >= 1 {
            break;
        }
    }
    Ok(OracleReport {
        count: controller.session.count,
        revision: controller.session.revision,
        copy_allowed: controller.session.copy_allowed,
        paste_allowed: controller.session.paste_allowed,
        presented,
        wayland: "opened",
    })
}

fn on_event(
    ir: &NativeIr,
    controller: &mut KeyController,
    hits: &HitMap,
    window: &mut GoyaWindow,
    event: PlatformEvent,
) -> Step {
    match event {
        PlatformEvent::PointerPress(x, y) => on_pointer(ir, controller, hits, x, y),
        PlatformEvent::KeyChar(ch) => on_char(controller, ch),
        PlatformEvent::KeySpecial(KeySpecial::Tab) => on_tab(controller, false),
        PlatformEvent::KeySpecialWithModifiers { key, modifiers } => match key {
            KeySpecial::Tab => on_tab(controller, modifiers.shift),
            // Denied Shift+Delete is a cut. It must not change the edit.
            KeySpecial::Delete if modifiers.shift && ir.capabilities.cut == "deny" => {
                Step::Unchanged
            }
            _ => Step::Unchanged,
        },
        PlatformEvent::Shortcut(Shortcut::Copy) => {
            if ir.capabilities.copy {
                window.set_clipboard_text(&controller.session.edit);
            }
            Step::Unchanged
        }
        PlatformEvent::Shortcut(Shortcut::Paste) => {
            if ir.capabilities.paste {
                window.request_clipboard_text();
            }
            Step::Unchanged
        }
        PlatformEvent::Shortcut(Shortcut::Quit) => Step::Quit,
        PlatformEvent::ClipboardText(text) => match text {
            Some(text) if ir.capabilities.paste => {
                controller.session.set_edit(&text);
                Step::Changed
            }
            // Denied paste ignores a clipboard payload, including an empty one.
            _ => Step::Unchanged,
        },
        PlatformEvent::Resized(_, _) => Step::Changed,
        PlatformEvent::CloseRequested => Step::Quit,
        // IME composition is not qualified. These events are ignored.
        PlatformEvent::Preedit(_)
        | PlatformEvent::PreeditWithCursor { .. }
        | PlatformEvent::CommitText(_)
        | PlatformEvent::DeleteSurroundingText { .. } => Step::Unchanged,
        PlatformEvent::PointerMove(_, _)
        | PlatformEvent::PointerRelease(_, _)
        | PlatformEvent::PointerScroll(_)
        | PlatformEvent::PointerScrollDelta { .. }
        | PlatformEvent::KeySpecial(_)
        | PlatformEvent::Shortcut(_)
        | PlatformEvent::BrowserNavigation(_)
        | PlatformEvent::Configured => Step::Unchanged,
    }
}

fn on_pointer(
    ir: &NativeIr,
    controller: &mut KeyController,
    hits: &HitMap,
    x: f64,
    y: f64,
) -> Step {
    let Some(control) = hits.at(dip_from_surface_px(x), dip_from_surface_px(y)) else {
        return Step::Unchanged;
    };
    let role = ir
        .elements
        .iter()
        .find(|element| element.id == control.semantic_id)
        .map(|element| element.role.as_str());
    let is_button = role == Some("button") || control.semantic_id == "window.add";
    let is_edit = role == Some("edit") || control.semantic_id == "window.amount";
    let before = view(controller);
    if is_button {
        controller.focus = Focus::Button;
        press_add(controller);
    } else if is_edit {
        controller.focus = Focus::Edit;
    } else {
        return Step::Unchanged;
    }
    if view(controller) == before {
        Step::Unchanged
    } else {
        Step::Changed
    }
}

fn on_char(controller: &mut KeyController, ch: char) -> Step {
    let before = view(controller);
    match ch {
        '\n' => press_add(controller),
        ' ' if controller.focus == Focus::Button => {
            controller.on_key(WM_KEYDOWN, VK_SPACE, false);
        }
        ' ' => {
            let mut edit = controller.session.edit.clone();
            edit.push(' ');
            controller.session.set_edit(&edit);
        }
        '\u{8}' => {
            if controller.focus == Focus::Edit {
                let next = delete_last_grapheme(&controller.session.edit);
                controller.session.set_edit(&next);
            }
        }
        _ => {
            if controller.focus == Focus::Edit {
                let mut edit = controller.session.edit.clone();
                edit.push(ch);
                controller.session.set_edit(&edit);
            }
        }
    }
    if view(controller) == before {
        Step::Unchanged
    } else {
        Step::Changed
    }
}

fn on_tab(controller: &mut KeyController, shift_down: bool) -> Step {
    let before = controller.focus;
    controller.on_key(WM_KEYDOWN, VK_TAB, shift_down);
    if controller.focus == before {
        Step::Unchanged
    } else {
        Step::Changed
    }
}

fn scene_frame(ir: &NativeIr, controller: &KeyController) -> Result<SceneFrame, String> {
    let client = find_role(ir, "client")?;
    let edit = find_role(ir, "edit")?;
    let button = find_role(ir, "button")?;
    let result = find_role(ir, "result")?;
    let mut primitives = Vec::with_capacity(5);
    let mut fills = Vec::with_capacity(5);
    primitives.push(Primitive::Panel(dip_rect(client)));
    fills.push(None);
    primitives.push(Primitive::Text(dip_rect(client), client.text.clone()));
    fills.push(None);
    primitives.push(Primitive::Text(
        dip_rect(edit),
        controller.session.edit.clone(),
    ));
    fills.push((controller.focus == Focus::Edit).then_some(FOCUS_FILL));
    primitives.push(Primitive::Button(dip_rect(button), button.text.clone()));
    fills.push((controller.focus == Focus::Button).then_some(FOCUS_FILL));
    primitives.push(Primitive::Text(
        dip_rect(result),
        controller.session.result.clone(),
    ));
    fills.push(None);
    let foregrounds = vec![None; primitives.len()];
    debug_assert_eq!(fills.len(), primitives.len());
    Ok(SceneFrame {
        primitives,
        fills,
        foregrounds,
    })
}

fn find_role<'a>(ir: &'a NativeIr, role: &str) -> Result<&'a Element, String> {
    ir.elements
        .iter()
        .find(|element| element.role == role)
        .ok_or_else(|| format!("lowering bug: missing {role} element"))
}

fn dip_rect(element: &Element) -> Rect {
    Rect {
        x: i64::from(element.x_dip) * goya_wayland::MP_PER_PX,
        y: i64::from(element.y_dip) * goya_wayland::MP_PER_PX,
        width: i64::from(element.width_dip) * goya_wayland::MP_PER_PX,
        height: i64::from(element.height_dip) * goya_wayland::MP_PER_PX,
    }
}

fn view(controller: &KeyController) -> View {
    View {
        count: controller.session.count,
        revision: controller.session.revision,
        edit: controller.session.edit.clone(),
        result: controller.session.result.clone(),
        focus: controller.focus,
    }
}

/// Truncate a surface pixel toward zero into an i32 DIP.
fn dip_from_surface_px(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let truncated = value.trunc();
    if truncated >= f64::from(i32::MAX) {
        i32::MAX
    } else if truncated <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        truncated as i32
    }
}

fn delete_last_grapheme(text: &str) -> String {
    let mut cut = text.len();
    for (index, _) in text.grapheme_indices(true) {
        cut = index;
    }
    if cut == text.len() {
        String::new()
    } else {
        text[..cut].to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_px_truncates_toward_zero() {
        assert_eq!(dip_from_surface_px(1.9), 1);
        assert_eq!(dip_from_surface_px(-1.9), -1);
        assert_eq!(dip_from_surface_px(8.0), 8);
        assert_eq!(dip_from_surface_px(f64::NAN), 0);
    }

    #[test]
    fn backspace_drops_one_grapheme() {
        assert_eq!(delete_last_grapheme("12"), "1");
        assert_eq!(delete_last_grapheme("e\u{0301}x"), "e\u{0301}");
        assert_eq!(delete_last_grapheme("a"), "");
        assert_eq!(delete_last_grapheme(""), "");
    }
}
