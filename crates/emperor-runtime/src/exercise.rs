//! Scripted host dispatch for one admitted counter.
//!
//! Scale is 1, so surface pixels are DIP. This is not a seat event and it
//! does not open Wayland.

use goya_parity::keys::Focus;

use crate::host;
use crate::{open_admitted, NativeIr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionReport {
    pub count: i64,
    pub revision: u64,
    pub edit: String,
    pub focus: String, // "edit" or "button"
    pub paste_ignored: bool,
    pub copy_allowed: bool,
    pub paste_allowed: bool,
    pub add_hit: bool,
    pub rects_stable: bool,
    pub space_activates: bool,
}

/// Drive the admitted counter through the same host functions the window calls.
pub fn exercise_interaction(nir_json: &str) -> Result<InteractionReport, String> {
    let (ir, mut controller) = open_admitted(nir_json)?;
    let hits = goya_parity::pointer::hit_map(&ir.scene).map_err(|err| err.to_string())?;
    let mut arm = host::PointerArm::default();
    let outcome = apply_visible_script(&ir, &mut controller, &hits, &mut arm)?;
    let focus = match controller.focus {
        Focus::Edit => "edit",
        Focus::Button => "button",
    };
    let space_activates = space_on_fresh_button(nir_json)?;

    Ok(InteractionReport {
        count: controller.session.count,
        revision: controller.session.revision,
        edit: controller.session.edit,
        focus: focus.to_owned(),
        paste_ignored: outcome.paste_ignored,
        copy_allowed: controller.session.copy_allowed,
        paste_allowed: controller.session.paste_allowed,
        add_hit: outcome.add_hit,
        rects_stable: outcome.rects_stable,
        space_activates,
    })
}

pub(crate) struct ScriptOutcome {
    pub add_hit: bool,
    pub paste_ignored: bool,
    pub rects_stable: bool,
}

/// Pointer, denied paste, edit, activation, and tab. The caller presents.
pub(crate) fn apply_visible_script(
    ir: &NativeIr,
    controller: &mut goya_parity::keys::KeyController,
    hits: &goya_parity::pointer::HitMap,
    arm: &mut host::PointerArm,
) -> Result<ScriptOutcome, String> {
    let rects = dip_rects(ir);
    let pressed = host::on_pointer_press(ir, controller, hits, arm, 176.0, 80.0);
    let released = host::on_pointer_release(controller, hits, arm, 176.0, 80.0);
    let add_hit = pressed.hit.as_deref() == Some("window.add")
        && released.hit.as_deref() == Some("window.add");
    let edit_before_paste = controller.session.edit.clone();
    host::on_clipboard_text(ir, controller, Some("9"));
    let paste_ignored =
        !ir.capabilities.paste && edit_before_paste == "1" && controller.session.edit == "1";
    host::on_pointer_press(ir, controller, hits, arm, 68.0, 80.0);
    host::on_char(controller, '\u{8}');
    host::on_char(controller, '2');
    host::on_char(controller, '\n');
    host::on_tab(controller, false);
    let _ = host::on_resize(ir);
    Ok(ScriptOutcome {
        add_hit,
        paste_ignored,
        rects_stable: dip_rects(ir) == rects,
    })
}

fn space_on_fresh_button(nir_json: &str) -> Result<bool, String> {
    let (_ir, mut controller) = open_admitted(nir_json)?;
    host::on_tab(&mut controller, false);
    host::on_char(&mut controller, ' ');
    Ok(controller.session.count == 1 && controller.session.revision == 1)
}

fn dip_rects(ir: &NativeIr) -> Vec<(i32, i32, i32, i32)> {
    ir.elements
        .iter()
        .map(|element| {
            (
                element.x_dip,
                element.y_dip,
                element.width_dip,
                element.height_dip,
            )
        })
        .collect()
}
