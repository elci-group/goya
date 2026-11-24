//! Wayland host loop for the admitted counter.
//!
//! Default hit rectangles stay at admitted DIP values when resized. A baked
//! desktop style may opt into shared proportional placement and inverse input
//! coordinates. Preedit and commit events are ignored.

use goya_layout::proportional::{self, Anchor, Placement, Point, ResizePolicy, Size, TiledEdges};
use goya_layout::Rect;
use goya_parity::keys::{Focus, KeyController};
use goya_parity::pointer::HitMap;
use goya_runtime::Primitive;
use goya_wayland::{GoyaWindow, KeyModifiers, KeySpecial, PlatformEvent, Shortcut};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    press_add, DesktopStyle, Element, NativeIr, OracleReport, WindowControl, VK_SPACE, VK_TAB,
    WM_KEYDOWN,
};

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
    window_control: Option<WindowControl>,
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

pub(crate) fn run_window_with_style(
    ir: &NativeIr,
    mut controller: KeyController,
    mode: WindowMode,
    style: Option<DesktopStyle>,
) -> Result<OracleReport, String> {
    let _ = scene_frame(ir, &controller)?;
    let hits = goya_parity::pointer::hit_map(&ir.scene).map_err(|err| err.to_string())?;
    let client_text = find_role(ir, "client")?.text.as_str();
    let title = if client_text.is_empty() {
        "Count"
    } else {
        client_text
    };
    let (initial_width, initial_height) = initial_window_size(ir.width_dip, ir.height_dip, style);
    let mut window = GoyaWindow::create(title, initial_width, initial_height)
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
        let events = window.poll();
        let placement = style
            .filter(|style| style.proportional)
            .map(|_| placement_for(ir, &window, &[]));
        for event in events {
            let event = if let Some(placement) = &placement {
                map_pointer_event(event, placement)
            } else {
                event
            };
            let mut published: Option<String> = None;
            let mut paste_requested = false;
            if let Some(action) = on_window_pointer(ir, &mut arm, &event, |action| {
                let _ = window.manage_window(action);
            }) {
                if action {
                    dirty = true;
                }
                continue;
            }
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
            let frame = styled_frame(ir, &controller, &window, style)?;
            let fg = style.map_or(FG, |style| style.fg);
            let bg = style.map_or(BG, |style| style.bg);
            let glyph_px = if style.is_some_and(|style| style.proportional) {
                let transform = placement_for(ir, &window, &[]).transform;
                scaled_glyph_px(transform.scale_x_milli.min(transform.scale_y_milli))
            } else {
                GLYPH_PX
            };
            window
                .present_dirty(
                    &frame.primitives,
                    &frame.fills,
                    &frame.foregrounds,
                    fg,
                    bg,
                    glyph_px,
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
        xdg_wm_base_version: style.map(|_| window.xdg_wm_base_version()),
        toplevel_state: style.map(|_| window.toplevel_state()),
    })
}

/// Handle generated chrome before authored scene controls. `Some(true)` means
/// an action was activated; `Some(false)` means the pointer event belongs to
/// generated chrome; `None` lets the ordinary scene hit map handle it.
pub(crate) fn on_window_pointer(
    ir: &NativeIr,
    arm: &mut PointerArm,
    event: &PlatformEvent,
    request: impl FnOnce(goya_wayland::window_management::WindowAction),
) -> Option<bool> {
    use goya_wayland::window_management::WindowAction;
    match *event {
        PlatformEvent::PointerPress(x, y) => {
            if let Some(control) = window_control_at(ir, x, y) {
                arm.button_id = None;
                arm.window_control = Some(control);
                Some(false)
            } else {
                arm.window_control = None;
                None
            }
        }
        PlatformEvent::PointerRelease(x, y) => {
            let Some(armed) = arm.window_control.take() else {
                return None;
            };
            if window_control_at(ir, x, y) == Some(armed) {
                request(match armed {
                    WindowControl::Minimize => WindowAction::Minimize,
                    WindowControl::Maximize => WindowAction::Maximize,
                    WindowControl::Restore => WindowAction::Restore,
                    WindowControl::Close => WindowAction::Close,
                });
                Some(true)
            } else {
                Some(false)
            }
        }
        _ => None,
    }
}

fn window_control_at(ir: &NativeIr, x: f64, y: f64) -> Option<WindowControl> {
    let (Some(x), Some(y)) = (dip_from_surface_px(x), dip_from_surface_px(y)) else {
        return None;
    };
    window_control_rects(ir)
        .into_iter()
        .find_map(|(control, rect)| {
            let left = rect.x.div_euclid(goya_wayland::MP_PER_PX) as i32;
            let top = rect.y.div_euclid(goya_wayland::MP_PER_PX) as i32;
            let width = rect.width.div_euclid(goya_wayland::MP_PER_PX) as i32;
            let height = rect.height.div_euclid(goya_wayland::MP_PER_PX) as i32;
            (x >= left && y >= top && x < left + width && y < top + height).then_some(control)
        })
}

pub(crate) fn window_control_rects(ir: &NativeIr) -> Vec<(WindowControl, Rect)> {
    let count = ir.window_controls.len();
    if count == 0 {
        return Vec::new();
    }
    let gap = 4_i32;
    let margin = 4_i32;
    let width =
        ((ir.width_dip - margin * 2 - gap * (count as i32 - 1)) / count as i32).clamp(1, 28);
    let height = ir.height_dip.min(24).max(1);
    let top = margin.min(ir.height_dip.saturating_sub(height));
    let start = ir.width_dip - margin - count as i32 * width - (count as i32 - 1) * gap;
    ir.window_controls
        .iter()
        .copied()
        .enumerate()
        .map(|(index, control)| {
            (
                control,
                Rect {
                    x: i64::from(start + index as i32 * (width + gap)) * goya_wayland::MP_PER_PX,
                    y: i64::from(top) * goya_wayland::MP_PER_PX,
                    width: i64::from(width) * goya_wayland::MP_PER_PX,
                    height: i64::from(height) * goya_wayland::MP_PER_PX,
                },
            )
        })
        .collect()
}

fn window_control_label(control: WindowControl) -> &'static str {
    match control {
        WindowControl::Minimize => "−",
        WindowControl::Maximize => "□",
        WindowControl::Restore => "▣",
        WindowControl::Close => "×",
    }
}

fn rgb(color: [u8; 3]) -> u32 {
    (u32::from(color[0]) << 16) | (u32::from(color[1]) << 8) | u32::from(color[2])
}

fn initial_window_size(width: i32, height: i32, style: Option<DesktopStyle>) -> (i32, i32) {
    if style.is_some_and(|style| style.proportional) {
        // The admitted 80x40 counter is a design grid. Give its proportional
        // presentation a usable initial surface while retaining that grid as
        // the single source of element geometry and hit testing.
        (width.max(640), height.max(320))
    } else {
        (width, height)
    }
}

fn scaled_glyph_px(scale_milli: u32) -> u32 {
    // The bitmap font uses pixels per 5x7 cell. One pixel per cell matches
    // the admitted design grid; scale that cell with the scene rectangles.
    ((scale_milli.saturating_add(500) / 1000).max(1)).min(64)
}

fn placement_for(ir: &NativeIr, window: &GoyaWindow, rects: &[Rect]) -> Placement {
    let (width, height) = window.size();
    let tiled = window.toplevel_state().tiled;
    proportional::place(
        ResizePolicy::Proportional {
            min_scale_milli: 1,
            max_scale_milli: 100_000,
            preserve_aspect: true,
            anchor: Anchor::Center,
        },
        Size::new(
            i64::from(ir.width_dip) * goya_wayland::MP_PER_PX,
            i64::from(ir.height_dip) * goya_wayland::MP_PER_PX,
        ),
        Size::new(
            i64::from(width) * goya_wayland::MP_PER_PX,
            i64::from(height) * goya_wayland::MP_PER_PX,
        ),
        Some(TiledEdges {
            left: tiled.left,
            right: tiled.right,
            top: tiled.top,
            bottom: tiled.bottom,
        }),
        rects,
    )
}

fn map_pointer_event(event: PlatformEvent, placement: &Placement) -> PlatformEvent {
    let point = |x: f64, y: f64| {
        if !x.is_finite() || !y.is_finite() {
            return (x, y);
        }
        let unit = goya_wayland::MP_PER_PX;
        let design = placement.to_design(Point::new(
            (x * unit as f64).floor() as i64,
            (y * unit as f64).floor() as i64,
        ));
        (
            design.x.div_euclid(unit) as f64,
            design.y.div_euclid(unit) as f64,
        )
    };
    match event {
        PlatformEvent::PointerPress(x, y) => {
            let (x, y) = point(x, y);
            PlatformEvent::PointerPress(x, y)
        }
        PlatformEvent::PointerRelease(x, y) => {
            let (x, y) = point(x, y);
            PlatformEvent::PointerRelease(x, y)
        }
        other => other,
    }
}

fn styled_frame(
    ir: &NativeIr,
    controller: &KeyController,
    window: &GoyaWindow,
    style: Option<DesktopStyle>,
) -> Result<SceneFrame, String> {
    let mut frame = scene_frame(ir, controller)?;
    let Some(style) = style else { return Ok(frame) };
    let accent = rgb(style.accent);
    frame.fills[0] = Some(rgb(style.bg));
    for fill in frame.fills.iter_mut().skip(1).flatten() {
        *fill = accent;
    }
    frame.foregrounds.fill(Some(rgb(style.fg)));
    if style.proportional {
        let rects: Vec<_> = frame
            .primitives
            .iter()
            .map(|primitive| match primitive {
                Primitive::Panel(rect) | Primitive::Text(rect, _) | Primitive::Button(rect, _) => {
                    *rect
                }
            })
            .collect();
        let placement = placement_for(ir, window, &rects);
        for (primitive, rect) in frame.primitives.iter_mut().zip(placement.rects) {
            match primitive {
                Primitive::Panel(current)
                | Primitive::Text(current, _)
                | Primitive::Button(current, _) => *current = rect,
            }
        }
    }
    Ok(frame)
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
        (Some(x), Some(y)) => hits.at(x, y).map(|control| control.semantic_id.clone()),
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
    for (control, rect) in window_control_rects(ir) {
        primitives.push(Primitive::Button(
            rect,
            window_control_label(control).to_owned(),
        ));
        fills.push(Some(FOCUS_FILL));
    }
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
    fn cosmic_counter_uses_design_proportions_on_a_readable_surface() {
        let style = Some(DesktopStyle {
            bg: [0; 3],
            fg: [255; 3],
            accent: [100; 3],
            proportional: true,
        });
        assert_eq!(initial_window_size(80, 40, style), (640, 320));
        assert_eq!(initial_window_size(80, 40, None), (80, 40));
        let placement = proportional::place(
            ResizePolicy::Proportional {
                min_scale_milli: 1,
                max_scale_milli: 100_000,
                preserve_aspect: true,
                anchor: Anchor::Center,
            },
            Size::new(80_000, 40_000),
            Size::new(640_000, 320_000),
            None,
            &[Rect {
                x: 34_000,
                y: 16_000,
                width: 20_000,
                height: 8_000,
            }],
        );
        assert_eq!(placement.rects[0].x, 272_000);
        assert_eq!(placement.rects[0].width, 160_000);
        assert_eq!(
            placement.to_design(Point::new(300_000, 150_000)),
            Point::new(37_500, 18_750)
        );
        assert_eq!(scaled_glyph_px(placement.transform.scale_x_milli), 8);
    }

    #[test]
    fn proportional_pointer_uses_inverse_design_coordinates() {
        let placement = proportional::place(
            ResizePolicy::Proportional {
                min_scale_milli: 1,
                max_scale_milli: 100_000,
                preserve_aspect: false,
                anchor: Anchor::TopLeft,
            },
            Size::new(320 * goya_wayland::MP_PER_PX, 160 * goya_wayland::MP_PER_PX),
            Size::new(640 * goya_wayland::MP_PER_PX, 320 * goya_wayland::MP_PER_PX),
            None,
            &[],
        );
        assert_eq!(
            map_pointer_event(PlatformEvent::PointerPress(160.0, 80.0), &placement),
            PlatformEvent::PointerPress(80.0, 40.0)
        );
        assert_eq!(
            map_pointer_event(PlatformEvent::PointerRelease(-0.1, 80.0), &placement),
            PlatformEvent::PointerRelease(-1.0, 40.0)
        );
    }

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
