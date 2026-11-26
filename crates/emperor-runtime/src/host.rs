//! Wayland host loop for the admitted counter.
//!
//! Hit rectangles stay at the admitted DIP values. Resize marks the frame
//! dirty and does not move them. Under the generic profile (EMP-M1) the
//! frame is drawn at those rectangles. Under the opt-in COSMIC profile the
//! frame is placed proportionally and pointer positions are mapped back to
//! design space through the same placement before the hit test. Preedit and
//! commit events are ignored.

use goya_layout::proportional::{self, ResizePolicy, Size, Transform};
use goya_layout::Rect;
use goya_parity::keys::{Focus, KeyController};
use goya_parity::pointer::HitMap;
use goya_runtime::Primitive;
use goya_wayland::{GoyaWindow, KeyModifiers, KeySpecial, PlatformEvent, Shortcut};
use unicode_segmentation::UnicodeSegmentation;

use crate::presentation::{self, Paint, PointerMap, Presentation, WmObservation, GLYPH_PX};
use crate::{press_add, Element, NativeIr, OracleReport, VK_SPACE, VK_TAB, WM_KEYDOWN};

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
    look: &Presentation,
) -> Result<OracleReport, String> {
    let _ = scene_frame_with(ir, &controller, &look.paint)?;
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
    // The pointer is mapped through the placement of the frame on screen.
    // Fixed never changes it; Proportional updates it when a frame commits.
    let mut shown = PointerMap::Fixed;
    let mut shown_transform = Transform::IDENTITY;
    let mut shown_hint = None;
    let mut seen_generation = window.toplevel_state_generation();
    let mut resizes = 0u32;
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
            if matches!(event, PlatformEvent::Resized(_, _)) {
                resizes = resizes.saturating_add(1);
            }
            let step = on_event_mapped(
                ir,
                &mut controller,
                &hits,
                &mut arm,
                shown,
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
        // A tiled-state change can move the anchor of a proportional frame.
        let generation = window.toplevel_state_generation();
        if generation != seen_generation {
            seen_generation = generation;
            if look.resize != ResizePolicy::Fixed {
                dirty = true;
            }
        }
        if dirty {
            let mut frame = scene_frame_with(ir, &controller, &look.paint)?;
            let hint = presentation::tiled_hint(&window.toplevel_state());
            let transform = place_frame(ir, look.resize, window.size(), hint, &mut frame);
            let glyph_px = match look.resize {
                ResizePolicy::Fixed => GLYPH_PX,
                ResizePolicy::Proportional { .. } => presentation::glyph_px(&transform),
            };
            window
                .present_dirty(
                    &frame.primitives,
                    &frame.fills,
                    &frame.foregrounds,
                    look.paint.fg,
                    look.paint.bg,
                    glyph_px,
                    None,
                )
                .map_err(|err| format!("wayland: {err}"))?;
            if look.resize != ResizePolicy::Fixed && window.frame_committed() {
                shown = PointerMap::Placed(transform);
                shown_transform = transform;
                shown_hint = hint;
            }
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
        if matches!(mode, WindowMode::Smoke | WindowMode::Exercise) && presented >= 1 {
            break;
        }
    }
    let wm = WmObservation {
        xdg_wm_base_version: window.xdg_wm_base_version(),
        state: window.toplevel_state(),
        capabilities: window.wm_capabilities(),
        state_generation: window.toplevel_state_generation(),
        size: window.size(),
        desktop: look.desktop,
        resize: look.resize_name(),
        tiled_hint: shown_hint,
        transform: shown_transform,
        resizes,
    };
    Ok(OracleReport {
        count: controller.session.count,
        revision: controller.session.revision,
        copy_allowed: controller.session.copy_allowed,
        paste_allowed: controller.session.paste_allowed,
        presented,
        wayland: "opened",
        window: "wayland",
        edit: controller.session.edit,
        wm: Some(wm),
    })
}

/// Place a frame on a surface of `surface_px` under `policy`.
///
/// `Fixed` leaves the frame untouched and returns the identity: EMP-M1 draws
/// at the admitted DIP rectangles. `Proportional` replaces every primitive
/// rectangle with the [`proportional::place`] output and returns the
/// transform, which the caller also uses for the pointer.
pub(crate) fn place_frame(
    ir: &NativeIr,
    policy: ResizePolicy,
    surface_px: (i32, i32),
    tiled: Option<proportional::TiledEdges>,
    frame: &mut SceneFrame,
) -> Transform {
    if policy == ResizePolicy::Fixed {
        return Transform::IDENTITY;
    }
    let mp = goya_wayland::MP_PER_PX;
    let design = Size::new(i64::from(ir.width_dip) * mp, i64::from(ir.height_dip) * mp);
    let configured = Size::new(i64::from(surface_px.0) * mp, i64::from(surface_px.1) * mp);
    let rects: Vec<Rect> = frame.primitives.iter().map(primitive_rect).collect();
    let placement = proportional::place(policy, design, configured, tiled, &rects);
    for (primitive, rect) in frame.primitives.iter_mut().zip(placement.rects.iter()) {
        set_primitive_rect(primitive, *rect);
    }
    placement.transform
}

fn primitive_rect(primitive: &Primitive) -> Rect {
    match primitive {
        Primitive::Panel(rect) | Primitive::Text(rect, _) | Primitive::Button(rect, _) => *rect,
    }
}

fn set_primitive_rect(primitive: &mut Primitive, rect: Rect) {
    match primitive {
        Primitive::Panel(slot) | Primitive::Text(slot, _) | Primitive::Button(slot, _) => {
            *slot = rect
        }
    }
}

/// EMP-M1 dispatch: pointer positions are surface pixels at scale 1.
#[cfg_attr(not(feature = "kumimanu"), allow(dead_code))]
pub(crate) fn on_event(
    ir: &NativeIr,
    controller: &mut KeyController,
    hits: &HitMap,
    arm: &mut PointerArm,
    publish: &mut dyn FnMut(&str),
    request_paste: &mut dyn FnMut(),
    event: PlatformEvent,
) -> Step {
    on_event_mapped(
        ir,
        controller,
        hits,
        arm,
        PointerMap::Fixed,
        publish,
        request_paste,
        event,
    )
}

/// Dispatch with pointer positions mapped by `map` before the hit test.
#[allow(clippy::too_many_arguments)]
pub(crate) fn on_event_mapped(
    ir: &NativeIr,
    controller: &mut KeyController,
    hits: &HitMap,
    arm: &mut PointerArm,
    map: PointerMap,
    publish: &mut dyn FnMut(&str),
    request_paste: &mut dyn FnMut(),
    event: PlatformEvent,
) -> Step {
    match event {
        PlatformEvent::PointerPress(x, y) => {
            on_pointer_press_mapped(ir, controller, hits, arm, map, x, y).step
        }
        PlatformEvent::PointerRelease(x, y) => {
            on_pointer_release_mapped(controller, hits, arm, map, x, y).step
        }
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
    on_pointer_press_mapped(ir, controller, hits, arm, PointerMap::Fixed, x, y)
}

pub(crate) fn on_pointer_press_mapped(
    ir: &NativeIr,
    controller: &mut KeyController,
    hits: &HitMap,
    arm: &mut PointerArm,
    map: PointerMap,
    x: f64,
    y: f64,
) -> PointerEffect {
    let (dip_x, dip_y) = map.dip(x, y);
    let Some(control) = hits.at(dip_x, dip_y) else {
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
    on_pointer_release_mapped(controller, hits, arm, PointerMap::Fixed, x, y)
}

pub(crate) fn on_pointer_release_mapped(
    controller: &mut KeyController,
    hits: &HitMap,
    arm: &mut PointerArm,
    map: PointerMap,
    x: f64,
    y: f64,
) -> PointerEffect {
    let (dip_x, dip_y) = map.dip(x, y);
    let hit = hits
        .at(dip_x, dip_y)
        .map(|control| control.semantic_id.clone());
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
/// A payload that arrives after focus moves is not decided here.
pub(crate) fn paste_shortcut_requests_clipboard(ir: &NativeIr, controller: &KeyController) -> bool {
    ir.capabilities.paste && controller.focus == Focus::Edit
}

/// Apply a clipboard payload. Denied paste ignores every payload, including empty text.
pub(crate) fn on_clipboard_text(
    ir: &NativeIr,
    controller: &mut KeyController,
    text: Option<&str>,
) -> Step {
    match text {
        Some(text) if ir.capabilities.paste => {
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

/// EMP-M1 frame with the generic paint.
#[cfg_attr(not(any(test, feature = "kumimanu")), allow(dead_code))]
pub(crate) fn scene_frame(ir: &NativeIr, controller: &KeyController) -> Result<SceneFrame, String> {
    scene_frame_with(ir, controller, &Paint::GENERIC)
}

/// Frame at the admitted DIP rectangles with `paint`.
///
/// The generic paint fills only the focused control and leaves every glyph
/// colour to the host default, exactly as EMP-M1 did.
pub(crate) fn scene_frame_with(
    ir: &NativeIr,
    controller: &KeyController,
    paint: &Paint,
) -> Result<SceneFrame, String> {
    let client = find_role(ir, "client")?;
    let edit = find_role(ir, "edit")?;
    let button = find_role(ir, "button")?;
    let result = find_role(ir, "result")?;
    let mut primitives = Vec::with_capacity(5);
    let mut fills = Vec::with_capacity(5);
    let mut foregrounds = Vec::with_capacity(5);
    let edit_focused = controller.focus == Focus::Edit;
    let button_focused = controller.focus == Focus::Button;
    let focus_or_field = |focused: bool| {
        if focused {
            Some(paint.focus_fill)
        } else {
            paint.field_fill
        }
    };
    let focus_fg = |focused: bool| if focused { paint.focus_fg } else { None };
    primitives.push(Primitive::Panel(dip_rect(client)));
    fills.push(None);
    foregrounds.push(None);
    primitives.push(Primitive::Text(dip_rect(client), client.text.clone()));
    fills.push(paint.client_fill);
    foregrounds.push(None);
    primitives.push(Primitive::Text(
        dip_rect(edit),
        controller.session.edit.clone(),
    ));
    fills.push(focus_or_field(edit_focused));
    foregrounds.push(focus_fg(edit_focused));
    primitives.push(Primitive::Button(dip_rect(button), button.text.clone()));
    fills.push(focus_or_field(button_focused));
    foregrounds.push(focus_fg(button_focused));
    primitives.push(Primitive::Text(
        dip_rect(result),
        controller.session.result.clone(),
    ));
    fills.push(paint.field_fill);
    foregrounds.push(None);
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
    use crate::presentation::dip_from_surface_px;

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
