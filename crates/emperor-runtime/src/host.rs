//! Wayland host loop for the admitted counter.
//!
//! Hit rectangles stay at the admitted DIP values. Resize marks the frame
//! dirty and does not move them. Preedit and commit events are ignored.

use goya_layout::Rect;
use goya_parity::keys::{Focus, KeyController};
use goya_parity::pointer::HitMap;
use goya_runtime::Primitive;
use goya_wayland::{GoyaWindow, KeyModifiers, KeySpecial, PlatformEvent, Shortcut};
use unicode_segmentation::UnicodeSegmentation;

use crate::{press_add, Element, NativeIr, OracleReport, VK_SPACE, VK_TAB, WM_KEYDOWN};

const FOCUS_FILL: u32 = 0xFFD6E4F5;
const FG: [u8; 3] = [20, 20, 28];
const BG: [u8; 3] = [235, 235, 240];
const GLYPH_PX: u32 = 8;

pub(crate) enum WindowMode {
    Smoke,
    /// Apply the scripted counter, then present that frame once.
    Exercise,
    Live {
        max_frames: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Unchanged,
    Changed,
    Quit,
}

pub(crate) struct PointerEffect {
    pub step: Step,
    pub hit: Option<String>,
}

/// Button id recorded by a press. Release inside that id activates.
///
/// Sash activates from `WM_COMMAND` note 0 (`BN_CLICKED`), which the system
/// button posts on release inside the control. This arm is that rule on the
/// hit map. It is not a seat grab and it does not call a pointer-capture API.
#[derive(Debug, Default)]
pub(crate) struct PointerArm {
    button_id: Option<String>,
}

pub(crate) struct SceneFrame {
    pub(crate) primitives: Vec<Primitive>,
    pub(crate) fills: Vec<Option<u32>>,
    pub(crate) foregrounds: Vec<Option<u32>>,
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
    let mut arm = PointerArm::default();
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
        if window.close_requested() {
            break;
        }
        for event in window.poll() {
            let mut published: Option<String> = None;
            let mut paste_requested = false;
            let step = on_event(
                ir,
                &mut controller,
                &hits,
                &mut arm,
                &mut |text| published = Some(text.to_owned()),
                &mut || paste_requested = true,
                event,
            );
            if let Some(text) = published {
                window.set_clipboard_text(&text);
            }
            if paste_requested {
                window.request_clipboard_text();
            }
            match step {
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
            if window.frame_committed() {
                dirty = false;
                presented = presented.saturating_add(1);
            }
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
        if matches!(mode, WindowMode::Smoke | WindowMode::Exercise) && presented >= 1 {
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
        window: "wayland",
        edit: controller.session.edit,
    })
}

pub(crate) fn on_event(
    ir: &NativeIr,
    controller: &mut KeyController,
    hits: &HitMap,
    arm: &mut PointerArm,
    publish: &mut dyn FnMut(&str),
    request_paste: &mut dyn FnMut(),
    event: PlatformEvent,
) -> Step {
    match event {
        PlatformEvent::PointerPress(x, y) => on_pointer_press(ir, controller, hits, arm, x, y).step,
        PlatformEvent::PointerRelease(x, y) => on_pointer_release(controller, hits, arm, x, y).step,
        PlatformEvent::KeyChar(ch) => on_char(controller, ch),
        PlatformEvent::KeySpecial(KeySpecial::Tab) => on_tab(controller, false),
        PlatformEvent::KeySpecialWithModifiers { key, modifiers } => {
            let effect = on_modified_special(ir, controller, key, modifiers);
            if let Some(text) = effect.clipboard.as_deref() {
                publish(text);
            }
            effect.step
        }
        PlatformEvent::Shortcut(Shortcut::Copy) => {
            if let Some(text) = copy_text(ir, controller) {
                publish(text);
            }
            Step::Unchanged
        }
        PlatformEvent::Shortcut(Shortcut::Cut) => {
            let effect = on_cut(ir, controller);
            if let Some(text) = effect.clipboard.as_deref() {
                publish(text);
            }
            effect.step
        }
        PlatformEvent::Shortcut(Shortcut::Paste) => {
            if paste_shortcut_requests_clipboard(ir, controller) {
                request_paste();
            }
            Step::Unchanged
        }
        PlatformEvent::Shortcut(Shortcut::Quit) => Step::Quit,
        PlatformEvent::ClipboardText(text) => on_clipboard_text(ir, controller, text.as_deref()),
        PlatformEvent::Resized(_, _) => on_resize(ir),
        PlatformEvent::CloseRequested => Step::Quit,
        // IME composition is not qualified. These events are ignored.
        PlatformEvent::Preedit(_)
        | PlatformEvent::PreeditWithCursor { .. }
        | PlatformEvent::CommitText(_)
        | PlatformEvent::DeleteSurroundingText { .. } => ignore_ime(controller),
        PlatformEvent::PointerMove(_, _)
        | PlatformEvent::PointerScroll(_)
        | PlatformEvent::PointerScrollDelta { .. }
        | PlatformEvent::KeySpecial(_)
        | PlatformEvent::Shortcut(_)
        | PlatformEvent::BrowserNavigation(_)
        | PlatformEvent::Configured => Step::Unchanged,
    }
}

pub(crate) fn on_pointer_press(
    ir: &NativeIr,
    controller: &mut KeyController,
    hits: &HitMap,
    arm: &mut PointerArm,
    x: f64,
    y: f64,
) -> PointerEffect {
    let (Some(x), Some(y)) = (dip_from_surface_px(x), dip_from_surface_px(y)) else {
        arm.button_id = None;
        return PointerEffect {
            step: Step::Unchanged,
            hit: None,
        };
    };
    let Some(control) = hits.at(x, y) else {
        arm.button_id = None;
        return PointerEffect {
            step: Step::Unchanged,
            hit: None,
        };
    };
    let hit = control.semantic_id.clone();
    let role = ir
        .elements
        .iter()
        .find(|element| element.id == hit)
        .map(|element| element.role.as_str());
    let is_button = role == Some("button") || hit == "window.add";
    let is_edit = role == Some("edit") || hit == "window.amount";
    let before = view(controller);
    if is_button {
        controller.focus = Focus::Button;
        arm.button_id = Some(hit.clone());
    } else if is_edit {
        controller.focus = Focus::Edit;
        arm.button_id = None;
    } else {
        arm.button_id = None;
        return PointerEffect {
            step: Step::Unchanged,
            hit: Some(hit),
        };
    }
    PointerEffect {
        step: step_if_changed(controller, &before),
        hit: Some(hit),
    }
}

pub(crate) fn on_pointer_release(
    controller: &mut KeyController,
    hits: &HitMap,
    arm: &mut PointerArm,
    x: f64,
    y: f64,
) -> PointerEffect {
    let hit = match (dip_from_surface_px(x), dip_from_surface_px(y)) {
        (Some(x), Some(y)) => hits
            .at(x, y)
            .map(|control| control.semantic_id.clone()),
        _ => None,
    };
    let armed = arm.button_id.take();
    let Some(armed_id) = armed else {
        return PointerEffect {
            step: Step::Unchanged,
            hit,
        };
    };
    if hit.as_deref() != Some(armed_id.as_str()) {
        return PointerEffect {
            step: Step::Unchanged,
            hit,
        };
    }
    let before = view(controller);
    press_add(controller);
    PointerEffect {
        step: step_if_changed(controller, &before),
        hit,
    }
}

fn step_if_changed(controller: &KeyController, before: &View) -> Step {
    if &view(controller) == before {
        Step::Unchanged
    } else {
        Step::Changed
    }
}

pub(crate) fn on_char(controller: &mut KeyController, ch: char) -> Step {
    let before = view(controller);
    match ch {
        '\n' => press_add(controller),
        // Space is Dispatch. The button activates. The edit does not change.
        ' ' => {
            controller.on_key(WM_KEYDOWN, VK_SPACE, false);
        }
        '\u{8}' => {
            if controller.focus == Focus::Edit {
                let next = delete_last_grapheme(&controller.session.edit);
                controller.session.set_edit(&next);
            }
        }
        _ => {
            if controller.focus == Focus::Edit && !ch.is_control() {
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

pub(crate) fn on_tab(controller: &mut KeyController, shift_down: bool) -> Step {
    let before = controller.focus;
    controller.on_key(WM_KEYDOWN, VK_TAB, shift_down);
    if controller.focus == before {
        Step::Unchanged
    } else {
        Step::Changed
    }
}

pub(crate) struct SpecialEffect {
    pub step: Step,
    pub clipboard: Option<String>,
}

/// Shift+Delete is a cut while the edit is focused.
///
/// `deny` publishes nothing and leaves the edit. `allow` and `default` publish
/// the whole edit and clear it. Button focus does not cut. Ctrl+Shift+Delete
/// is not this chord. Ctrl+X uses [`on_cut`] with the same rules.
pub(crate) fn on_modified_special(
    ir: &NativeIr,
    controller: &mut KeyController,
    key: KeySpecial,
    modifiers: KeyModifiers,
) -> SpecialEffect {
    if key == KeySpecial::Delete && modifiers.shift && !modifiers.ctrl {
        return on_cut(ir, controller);
    }
    if key == KeySpecial::Tab {
        return SpecialEffect {
            step: on_tab(controller, modifiers.shift),
            clipboard: None,
        };
    }
    SpecialEffect {
        step: Step::Unchanged,
        clipboard: None,
    }
}

/// Publish and clear the whole edit, or leave it untouched.
///
/// `deny`, non-edit focus, and an empty edit publish nothing. `allow` and
/// `default` publish the edit and clear it. There is no selection.
pub(crate) fn on_cut(ir: &NativeIr, controller: &mut KeyController) -> SpecialEffect {
    if ir.capabilities.cut == "deny"
        || controller.focus != Focus::Edit
        || controller.session.edit.is_empty()
    {
        return SpecialEffect {
            step: Step::Unchanged,
            clipboard: None,
        };
    }
    let text = controller.session.edit.clone();
    controller.session.set_edit("");
    SpecialEffect {
        step: Step::Changed,
        clipboard: Some(text),
    }
}

/// Clipboard text the window may publish.
///
/// `Some` only when copy is allowed and focus is the edit, including an
/// empty edit. Button focus returns `None` and does not change the edit.
pub(crate) fn copy_text<'a>(ir: &NativeIr, controller: &'a KeyController) -> Option<&'a str> {
    if ir.capabilities.copy && controller.focus == Focus::Edit {
        Some(controller.session.edit.as_str())
    } else {
        None
    }
}

/// Allowed paste on the edit asks the window for text.
///
/// Button focus does not request the clipboard. Denied paste does not.
pub(crate) fn paste_shortcut_requests_clipboard(ir: &NativeIr, controller: &KeyController) -> bool {
    ir.capabilities.paste && controller.focus == Focus::Edit
}

/// Apply a clipboard payload only while the edit still owns focus. Clipboard
/// delivery is asynchronous, so a response to an earlier request must not
/// replace text after focus has moved to another control.
pub(crate) fn on_clipboard_text(
    ir: &NativeIr,
    controller: &mut KeyController,
    text: Option<&str>,
) -> Step {
    match text {
        Some(text) if ir.capabilities.paste && controller.focus == Focus::Edit => {
            controller.session.set_edit(text);
            Step::Changed
        }
        _ => Step::Unchanged,
    }
}

/// Resize dirties the frame. It does not rewrite DIP rectangles.
pub(crate) fn on_resize(_ir: &NativeIr) -> Step {
    Step::Changed
}

/// Preedit, commit, and surrounding-delete are not qualified input.
pub(crate) fn ignore_ime(_controller: &KeyController) -> Step {
    Step::Unchanged
}

pub(crate) fn scene_frame(ir: &NativeIr, controller: &KeyController) -> Result<SceneFrame, String> {
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

pub(crate) fn find_role<'a>(ir: &'a NativeIr, role: &str) -> Result<&'a Element, String> {
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

/// Truncate a finite surface pixel toward zero into an i32 DIP.
/// Non-finite coordinates are invalid pointer input and must never alias the
/// origin, where a button may be hit.
fn dip_from_surface_px(value: f64) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let truncated = value.trunc();
    if truncated >= f64::from(i32::MAX) {
        Some(i32::MAX)
    } else if truncated <= f64::from(i32::MIN) {
        Some(i32::MIN)
    } else {
        Some(truncated as i32)
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
        assert_eq!(dip_from_surface_px(1.9), Some(1));
        assert_eq!(dip_from_surface_px(-1.9), Some(-1));
        assert_eq!(dip_from_surface_px(8.0), Some(8));
        assert_eq!(dip_from_surface_px(f64::NAN), None);
    }

    #[test]
    fn backspace_drops_one_grapheme() {
        assert_eq!(delete_last_grapheme("12"), "1");
        assert_eq!(delete_last_grapheme("e\u{0301}x"), "e\u{0301}");
        assert_eq!(delete_last_grapheme("a"), "");
        assert_eq!(delete_last_grapheme(""), "");
    }
}
