//! Bounded interactive window.
//!
//! `sash.win32.interactive/0.0.1` admits one heading, one numeric edit, one
//! button, one result label, and one D1 integer transition. The edit keeps
//! the system clipboard because the scene states no policy.
//! `sash.win32.interactive/0.0.2` is the same window when that edit states
//! copy and paste. Enter activates the button. Tab moves focus. Space goes to
//! the focused control. The result text is written before the value is stored.
//! A nested activation during that write is ignored. A release that starts
//! during that write drops the commit. The stored value changes only when the
//! result text reads back as that value. Close releases the window resources
//! and unregisters the class. A failed transition leaves the previous value and
//! revision in place. Windows launch of the generated executable is not claimed.

use crate::admit::{self, Disposition};
use crate::emit::Generated;
use crate::Diagnostic;
use goya_lang::gestures::{self, Decision, DeclarationError, Declarations, Operation, Target};
use goya_lang::{
    compile_d1_interactions, layout, CellKind, GestureDecision, InteractionClass, InteractionExpr,
    InteractionType, InteractionValue, ParsedScene, SceneCell, SourceRange,
};

mod canonical {
    include!("interactive_text.rs");
    include!("interactive_clipboard.rs");
    include!("interactive_keys.rs");
    include!("interactive_commit.rs");
}

pub use canonical::{
    activation, clipboard_blocked, command_activates, format_i64, hold_after_release, key_action,
    parse_canonical_i64, publish_after_text, shutdown_plan, visible_commit, Activation,
    ClipboardGate, KeyAction, Publish, PumpEnd, ShutdownPlan, VisibleCommit, CUT_ALLOW,
    CUT_DEFAULT, CUT_DENY,
};

pub const PROFILE: &str = "sash.win32.interactive/0.0.1";
pub const PROFILE_CLIPBOARD: &str = "sash.win32.interactive/0.0.2";

/// Interactive window emitted for the win8 API floor. The clipboard policy
/// distinction follows the scene exactly as on the win10 floor: 0.0.1 keeps
/// the system clipboard, 0.0.2 compiles an authored copy and paste policy.
pub const PROFILE_LEGACY: &str = "sash.win32.interactive-legacy/0.0.1";
pub const PROFILE_CLIPBOARD_LEGACY: &str = "sash.win32.interactive-legacy/0.0.2";

const MAX_CONTROLS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRole {
    Static = 1,
    Edit = 2,
    Button = 3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntExpr {
    State,
    Input,
    Literal(i64),
    Add(Box<IntExpr>, Box<IntExpr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedControl {
    pub role: ControlRole,
    pub control_id: u16,
    pub name: String,
    pub semantic_id: String,
    pub text: String,
    pub x_dip: i32,
    pub y_dip: i32,
    pub width_dip: i32,
    pub height_dip: i32,
    pub text_range: SourceRange,
    pub id_range: SourceRange,
    pub extent_range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedInteractive {
    pub scene_name: String,
    pub semantic_id: String,
    pub action_id: String,
    pub action_name: String,
    pub state_key: String,
    pub input_key: String,
    pub initial_state: i64,
    pub initial_input: i64,
    pub profile: &'static str,
    pub copy_allowed: bool,
    pub paste_allowed: bool,
    pub cut_mode: u8,
    pub state_index: usize,
    pub input_index: usize,
    pub button_index: usize,
    pub expr: IntExpr,
    pub controls: Vec<PlacedControl>,
    pub width_dip: i32,
    pub height_dip: i32,
    pub source_digest: [u8; 32],
    pub dispositions: Vec<Disposition>,
    pub ontology_range: SourceRange,
    pub id_range: SourceRange,
    pub extent_range: SourceRange,
    pub action_range: SourceRange,
}

pub fn eval(expr: &IntExpr, state_value: i64, input_value: i64) -> Option<i64> {
    match expr {
        IntExpr::State => Some(state_value),
        IntExpr::Input => Some(input_value),
        IntExpr::Literal(value) => Some(*value),
        IntExpr::Add(left, right) => eval(left, state_value, input_value)?.checked_add(eval(
            right,
            state_value,
            input_value,
        )?),
    }
}

pub fn commit(
    expr: &IntExpr,
    state_value: i64,
    revision: u64,
    input_value: i64,
) -> Option<(i64, u64)> {
    let next = eval(expr, state_value, input_value)?;
    Some((next, revision.checked_add(1)?))
}

pub fn admit_parsed(parsed: &ParsedScene) -> Result<AdmittedInteractive, Diagnostic> {
    let scene = &parsed.scene;
    if scene.interactions.len() != 1 {
        return Err(admit::cap(
            format!(
                "this profile admits one D1 interaction and found {}",
                scene.interactions.len()
            ),
            "Author one interaction. The static-window profile still rejects every interaction.",
            scene
                .interactions
                .first()
                .and_then(|item| admit::entry_range(parsed, "interaction", &item.name))
                .or_else(|| admit::cell_range(parsed, &[])),
        ));
    }
    let action = &scene.interactions[0];
    let action_range =
        admit::entry_range(parsed, "interaction", &action.name).ok_or_else(|| {
            Diagnostic::new(
                "SASH-PARSE",
                "parse",
                "strict admission did not record an interaction origin",
                "Report this as a frontend defect.",
            )
        })?;
    if action.class != InteractionClass::D1
        || !action.declared_effects.is_empty()
        || !action.requests.is_empty()
    {
        return Err(admit::cap(
            format!(
                "interaction '{}' is outside the integer D1 subset",
                action.name
            ),
            "Use class D1 with no effects. D2 and host effects stay rejected.",
            Some(action_range),
        ));
    }
    let state_key = one_int(&action.state_reads, "state read")?;
    let written = one_int(&action.state_writes, "state write")?;
    let input_key = one_int(&action.inputs, "input")?;
    if state_key != written {
        return Err(admit::cap(
            format!("state read '{state_key}' and write '{written}' differ"),
            "Read and write the same integer key.",
            Some(action_range),
        ));
    }
    if action.assignments.len() != 1 || action.assignments[0].key != state_key {
        return Err(admit::cap(
            format!(
                "interaction '{}' must assign the state key once",
                action.name
            ),
            "Use one set statement that writes the declared integer.",
            Some(action_range),
        ));
    }
    let expr =
        lower_expr(&action.assignments[0].value, state_key, input_key).map_err(|detail| {
            admit::cap(
                format!("interaction '{}' {detail}", action.name),
                "Use integer literals, the state key, the input key, and addition.",
                Some(action_range),
            )
        })?;
    if !uses(&expr, true, false) || !uses(&expr, false, true) {
        return Err(admit::cap(
            format!(
                "interaction '{}' must read state.{state_key} and input.{input_key}",
                action.name
            ),
            "The counter transition reads both declared keys.",
            Some(action_range),
        ));
    }
    let compiled = compile_d1_interactions(scene).map_err(|error| {
        admit::cap(
            format!(
                "interaction '{}' was rejected by the canonical D1 compiler: {error}",
                action.name
            ),
            "Match the declared reads, writes, and inputs to the assignment.",
            Some(action_range),
        )
    })?;
    if compiled.len() != 1 || compiled[0].name() != action.name {
        return Err(admit::cap(
            "the canonical D1 compiler did not return this interaction",
            "Keep a single D1 interaction.",
            Some(action_range),
        ));
    }
    let action_id = action.authored_id.clone().ok_or_else(|| {
        admit::cap(
            "the interaction has no authored id",
            "Set id on the interaction.",
            Some(action_range),
        )
    })?;
    admit::check_label(
        "action id",
        &action_id,
        admit::MAX_ID_BYTES,
        Some(action_range),
    )?;

    let root = &scene.root;
    if root.kind != CellKind::Cell || root.is_input {
        return Err(admit::cap(
            "the interactive root must be a cell",
            "Put the heading, input, button, and result inside one root cell.",
            admit::cell_range(parsed, &[]),
        ));
    }
    admit::reject_position(root, parsed)?;
    let (extent_range, width_pt, height_pt) = admit::require_extent(root, parsed)?;
    let frame = layout(scene, width_pt, height_pt);
    let mut controls = Vec::new();
    let mut static_ids = 100u16;
    place_control(
        root,
        &[],
        true,
        &frame,
        parsed,
        &mut controls,
        &mut static_ids,
    )?;
    if root.children.len() != 3 {
        return Err(admit::cap(
            format!(
                "the root has {} children; this profile admits an input, a button, and a result",
                root.children.len()
            ),
            "Use one @input, one @button, and one result @cell.",
            admit::cell_range(parsed, &[]),
        ));
    }
    for (index, child) in root.children.iter().enumerate() {
        if !child.children.is_empty() {
            return Err(admit::cap(
                format!("element '{}' nests another element", child.name),
                "Keep the input, button, and result as direct children of the root.",
                admit::cell_range(parsed, &[index]),
            ));
        }
        place_control(
            child,
            &[index],
            false,
            &frame,
            parsed,
            &mut controls,
            &mut static_ids,
        )?;
    }
    if controls.len() != frame.nodes.len() {
        return Err(admit::layout_error(
            "layout node count does not match the scene tree",
            Some(extent_range),
        ));
    }
    let count_role = |role: ControlRole| controls.iter().filter(|item| item.role == role).count();
    if count_role(ControlRole::Button) != 1
        || count_role(ControlRole::Edit) != 1
        || count_role(ControlRole::Static) != 2
    {
        return Err(admit::cap(
            "this profile admits one heading, one result, one input, and one button",
            "Remove the extra control.",
            admit::cell_range(parsed, &[]),
        ));
    }
    let button_index = controls
        .iter()
        .position(|item| item.role == ControlRole::Button)
        .expect("button counted");
    let input_index = controls
        .iter()
        .position(|item| item.role == ControlRole::Edit)
        .expect("input counted");
    let state_index = controls
        .iter()
        .rposition(|item| item.role == ControlRole::Static)
        .expect("result counted");
    if controls[0].role != ControlRole::Static || state_index == 0 {
        return Err(admit::cap(
            "the root cell must be the heading and another cell must show the state",
            "Keep text on the root and put the result in a child cell.",
            admit::cell_range(parsed, &[]),
        ));
    }
    if controls[button_index].name != action.trigger_button {
        return Err(admit::cap(
            format!(
                "trigger button.{} does not name the admitted button '{}'",
                action.trigger_button, controls[button_index].name
            ),
            "Point the trigger at the button's structural name.",
            Some(action_range),
        ));
    }
    let display_id = format!("state.{state_key}");
    if controls[state_index].semantic_id != display_id {
        return Err(admit::cap(
            format!(
                "result id '{}' does not display state.{state_key}",
                controls[state_index].semantic_id
            ),
            "Set the result cell id to state.<key>.",
            Some(controls[state_index].id_range),
        ));
    }
    let initial_state = parse_canonical_i64(&controls[state_index].text).ok_or_else(|| {
        admit::cap(
            "the result text is not the canonical decimal of an i64",
            "Use the shortest decimal form, such as 0 or -1.",
            Some(controls[state_index].text_range),
        )
    })?;
    let initial_input = parse_canonical_i64(&controls[input_index].text).ok_or_else(|| {
        admit::cap(
            "the input text is not the canonical decimal of an i64",
            "Use the shortest decimal form. The edit starts with that value.",
            Some(controls[input_index].text_range),
        )
    })?;
    let (profile, copy_allowed, paste_allowed, cut_mode) =
        clipboard_profile(parsed, root, input_index)?;
    let (width_dip, height_dip) = admit::dips(
        frame.nodes[0].rect.width,
        frame.nodes[0].rect.height,
        extent_range,
    )?;
    let semantic_id = controls[0].semantic_id.clone();
    let ontology_range = admit::ontology_range(parsed).ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record an ontology origin",
            "Report this as a frontend defect.",
        )
    })?;
    let id_range = controls[0].id_range;
    admit::check_label("window title", &scene.name, 256, Some(ontology_range))?;
    let mut dispositions = vec![
        Disposition {
            subject: "interaction".into(),
            disposition: "preserved",
            detail: format!(
                "D1 action {action_id} reads and writes {state_key} and reads {input_key}. Addition is checked. The result text is written before the value and revision are stored. The stored value changes only when that text reads back as the new value. A different readback restores the previous result text while the window is live. A nested activation while that write is in progress is ignored. A release that starts during that write drops the commit. Overflow, a non-canonical edit, a failed text update, and revision overflow leave the previous value and revision unchanged."
            ),
        },
        Disposition {
            subject: "controls".into(),
            disposition: "equivalently-transformed",
            detail: "The heading and result are STATIC controls. The numeric field is an EDIT. The button is a default BUTTON. Enter activates it. Space is delivered to the focused control, so it activates the button only while that button has focus. Tab and Shift-Tab move between the edit and the button.".into(),
        },
        Disposition {
            subject: "clipboard".into(),
            disposition: if profile == PROFILE_CLIPBOARD {
                "preserved"
            } else {
                "rejected-by-omission"
            },
            detail: clipboard_detail(profile, copy_allowed, paste_allowed, cut_mode),
        },
        Disposition {
            subject: "runtime".into(),
            disposition: "unqualified",
            detail: "Emission of this profile is not a Windows launch. runtime_qualification stays BLOCKED. Close releases the font, semantic ids, and edit subclass, then unregisters the window class. A failed message pump destroys the window before that unregister.".into(),
        },
    ];
    dispositions.push(Disposition {
        subject: "extent".into(),
        disposition: "preserved",
        detail: format!(
            "The client area is {width_dip} by {height_dip} dip at {} millipoints per reference pixel. Resizing clips controls and does not reflow them.",
            admit::MP_PER_DIP
        ),
    });
    Ok(AdmittedInteractive {
        scene_name: scene.name.clone(),
        semantic_id,
        action_id,
        action_name: action.name.clone(),
        state_key: state_key.to_string(),
        input_key: input_key.to_string(),
        initial_state,
        initial_input,
        profile,
        copy_allowed,
        paste_allowed,
        cut_mode,
        state_index,
        input_index,
        button_index,
        expr,
        controls,
        width_dip,
        height_dip,
        source_digest: parsed.source_digest,
        dispositions,
        ontology_range,
        id_range,
        extent_range,
        action_range,
    })
}

fn clipboard_profile(
    parsed: &ParsedScene,
    root: &SceneCell,
    input_index: usize,
) -> Result<(&'static str, bool, bool, u8), Diagnostic> {
    if root.gestures.is_some() {
        return Err(admit::cap(
            "the heading cannot carry gesture policy",
            "Put the gestures block on the numeric edit.",
            admit::cell_range(parsed, &[]),
        ));
    }
    let child_index = input_index - 1;
    for (index, child) in root.children.iter().enumerate() {
        if index != child_index && child.gestures.is_some() {
            return Err(admit::cap(
                format!("element '{}' cannot carry gesture policy", child.name),
                "Put the gestures block on the numeric edit.",
                admit::cell_range(parsed, &[index]),
            ));
        }
    }
    let Some(policy) = root.children[child_index].gestures else {
        return Ok((PROFILE, true, true, CUT_DEFAULT));
    };
    let copy = policy.copy.ok_or_else(|| {
        admit::cap(
            "the edit gestures block does not state copy",
            "State copy: allow or copy: deny.",
            admit::cell_range(parsed, &[child_index]),
        )
    })?;
    let paste = policy.paste.ok_or_else(|| {
        admit::cap(
            "the edit gestures block does not state paste",
            "State paste: allow or paste: deny.",
            admit::cell_range(parsed, &[child_index]),
        )
    })?;
    let cut_mode = match policy.cut {
        None => CUT_DEFAULT,
        Some(GestureDecision::Allow) => CUT_ALLOW,
        Some(GestureDecision::Deny) => CUT_DENY,
    };
    let declarations = Declarations {
        select: Decision::Inherit,
        copy: decision(copy),
        cut: match policy.cut {
            Some(value) => decision(value),
            None => Decision::Inherit,
        },
        paste: decision(paste),
    };
    if let Err(error) = gestures::validate_declarations(Target::EditableText, declarations) {
        return Err(admit::cap(
            format!(
                "the edit gesture policy is contradictory: {}",
                gesture_error(error)
            ),
            "Allow copy before cut. This profile does not admit a protected editor.",
            admit::cell_range(parsed, &[child_index]),
        ));
    }
    Ok((
        PROFILE_CLIPBOARD,
        copy == GestureDecision::Allow,
        paste == GestureDecision::Allow,
        cut_mode,
    ))
}

fn decision(value: GestureDecision) -> Decision {
    match value {
        GestureDecision::Allow => Decision::Allow,
        GestureDecision::Deny => Decision::Deny,
    }
}

fn gesture_error(error: DeclarationError) -> &'static str {
    match error {
        DeclarationError::CopyRequiresSelection => "copy allow contradicts select deny",
        DeclarationError::CutRequiresSelection => "cut allow contradicts select deny",
        DeclarationError::CutRequiresCopy => "cut allow contradicts copy deny",
        DeclarationError::MutationRequiresEditableTarget(Operation::Cut) => {
            "cut is not valid on this target"
        }
        DeclarationError::MutationRequiresEditableTarget(Operation::Paste) => {
            "paste is not valid on this target"
        }
        DeclarationError::ProtectedExportForbidden(Operation::Copy) => {
            "a protected editor cannot allow copy"
        }
        DeclarationError::ProtectedExportForbidden(Operation::Cut) => {
            "a protected editor cannot allow cut"
        }
        DeclarationError::MutationRequiresEditableTarget(_)
        | DeclarationError::ProtectedExportForbidden(_) => "the gesture declarations contradict",
    }
}

fn clipboard_detail(
    profile: &str,
    copy_allowed: bool,
    paste_allowed: bool,
    cut_mode: u8,
) -> String {
    if profile != PROFILE_CLIPBOARD {
        return "The edit authors no gesture policy. Copy, cut, and paste reach the system EDIT. Select, undo, and redo are not policy operations. A gestures property with a scalar value remains a frontend error.".into();
    }
    let cut = match cut_mode {
        CUT_ALLOW => "Cut is allowed.",
        CUT_DENY => "Cut is denied.",
        _ => "Cut was not authored, so the system cut command remains.",
    };
    format!(
        "The edit is subclassed. Copy is {}. Paste is {}. {cut} A denied operation drops WM_COPY, WM_CUT, WM_PASTE, the matching Ctrl chord, Shift+Delete, and the matching control character before the text changes. Delete without Shift is not a cut. Select, undo, and redo are not policy operations. This has not been launched on Windows.",
        if copy_allowed { "allowed" } else { "denied" },
        if paste_allowed { "allowed" } else { "denied" }
    )
}

fn one_int<'a>(keys: &'a [goya_lang::TypedKey], what: &str) -> Result<&'a str, Diagnostic> {
    match keys {
        [key] if key.value_type == InteractionType::Int => Ok(key.name.as_str()),
        _ => Err(admit::cap(
            format!("this profile admits one int {what}"),
            "Declare a single int field. Text, bool, and extra keys stay rejected.",
            None,
        )),
    }
}

fn lower_expr(
    expr: &InteractionExpr,
    state_key: &str,
    input_key: &str,
) -> Result<IntExpr, &'static str> {
    match expr {
        InteractionExpr::Literal(InteractionValue::Int(value)) => Ok(IntExpr::Literal(*value)),
        InteractionExpr::Literal(_) => Err("uses a non-integer literal"),
        InteractionExpr::State(key) if key == state_key => Ok(IntExpr::State),
        InteractionExpr::Input(key) if key == input_key => Ok(IntExpr::Input),
        InteractionExpr::State(_) | InteractionExpr::Input(_) => {
            Err("reads a key that is not the single state or input")
        }
        InteractionExpr::Effect(_) => Err("reads an effect"),
        InteractionExpr::Add(left, right) => Ok(IntExpr::Add(
            Box::new(lower_expr(left, state_key, input_key)?),
            Box::new(lower_expr(right, state_key, input_key)?),
        )),
    }
}

fn uses(expr: &IntExpr, state: bool, input: bool) -> bool {
    match expr {
        IntExpr::State => state,
        IntExpr::Input => input,
        IntExpr::Literal(_) => false,
        IntExpr::Add(left, right) => uses(left, state, input) || uses(right, state, input),
    }
}

fn place_control(
    cell: &SceneCell,
    path: &[usize],
    is_root: bool,
    frame: &goya_lang::Frame,
    parsed: &ParsedScene,
    controls: &mut Vec<PlacedControl>,
    static_ids: &mut u16,
) -> Result<(), Diagnostic> {
    if controls.len() >= MAX_CONTROLS {
        return Err(admit::cap(
            format!("this profile admits at most {MAX_CONTROLS} controls"),
            "Remove a control.",
            admit::cell_range(parsed, path),
        ));
    }
    admit::reject_visuals(cell, parsed, path)?;
    let node_index = controls.len();
    let Some(node) = frame.nodes.get(node_index) else {
        return Err(admit::layout_error(
            "layout produced fewer nodes than the scene tree",
            admit::cell_range(parsed, path),
        ));
    };
    if !is_root {
        let parent = frame.nodes[0].rect;
        admit::expect_child_rect(cell, parent, node.rect, parsed, path)?;
    }
    let role = match cell.kind {
        CellKind::Button => ControlRole::Button,
        CellKind::Input => ControlRole::Edit,
        CellKind::Cell if cell.is_input => ControlRole::Edit,
        CellKind::Cell => ControlRole::Static,
    };
    if node.is_button != (role == ControlRole::Button)
        || node.is_input != (role == ControlRole::Edit)
    {
        return Err(admit::layout_error(
            "layout changed the authored control kind",
            admit::cell_range(parsed, path),
        ));
    }
    let Some(text) = admit::validate_text(cell, parsed, path)? else {
        return Err(admit::cap(
            format!("element '{}' has no text", cell.name),
            "Set text. This profile does not invent a label.",
            admit::cell_range(parsed, path),
        ));
    };
    let semantic_id = cell.authored_id.clone().ok_or_else(|| {
        admit::cap(
            format!("element '{}' has no id", cell.name),
            "Set id on every element.",
            admit::cell_range(parsed, path),
        )
    })?;
    let id_range = admit::id_range_at(parsed, path).ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record an id origin",
            "Report this as a frontend defect.",
        )
    })?;
    admit::check_label(
        "semantic id",
        &semantic_id,
        admit::MAX_ID_BYTES,
        Some(id_range),
    )?;
    let text_range = admit::property_range_at(parsed, path, "text").ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record a text origin",
            "Report this as a frontend defect.",
        )
    })?;
    let extent_range = admit::property_range_at(parsed, path, "extent")
        .or(admit::cell_range(parsed, path))
        .unwrap_or(SourceRange { start: 0, end: 0 });
    let (x_dip, y_dip) = admit::dips_pair(node.rect.x, node.rect.y, extent_range, "position")?;
    let (width_dip, height_dip) = admit::dips(node.rect.width, node.rect.height, extent_range)?;
    let control_id = match role {
        ControlRole::Static => {
            *static_ids = static_ids.saturating_add(1);
            *static_ids
        }
        ControlRole::Edit => 201,
        ControlRole::Button => 301,
    };
    controls.push(PlacedControl {
        role,
        control_id,
        name: cell.name.clone(),
        semantic_id,
        text,
        x_dip,
        y_dip,
        width_dip,
        height_dip,
        text_range,
        id_range,
        extent_range,
    });
    Ok(())
}

pub fn generate(app: &AdmittedInteractive, floor: crate::api_floor::ApiFloor) -> Generated {
    let window_ir = crate::ir_lower::lower_interactive_window(app, floor);
    crate::ir_codegen::codegen_from_ir(&window_ir, floor)
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit::{self, SceneTarget};
    use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
    use std::collections::BTreeMap;

    fn fixture() -> AdmittedInteractive {
        match admit::open(include_str!("../fixtures/interactive-counter.gscene")).unwrap() {
            SceneTarget::Interactive(app) => app,
            SceneTarget::Static(_) => panic!("counter was admitted as a static window"),
        }
    }

    #[test]
    fn counter_fixture_places_native_controls() {
        let app = fixture();
        assert_eq!(app.semantic_id, "window.root");
        assert_eq!((app.width_dip, app.height_dip), (320, 160));
        assert_eq!(app.initial_state, 0);
        assert_eq!(app.initial_input, 1);
        assert_eq!(app.state_key, "count");
        assert_eq!(app.input_key, "amount");
        assert_eq!(app.profile, PROFILE_CLIPBOARD);
        assert!(app.copy_allowed);
        assert!(!app.paste_allowed);
        assert_eq!(app.cut_mode, CUT_DEFAULT);
        assert_eq!(app.controls[app.input_index].x_dip, 8);
        assert_eq!(app.controls[app.input_index].y_dip, 64);
        assert_eq!(app.controls[app.input_index].width_dip, 120);
        assert_eq!(app.controls[app.button_index].x_dip, 136);
        assert_eq!(app.controls[app.state_index].semantic_id, "state.count");
        assert!(admit::admit(include_str!("../fixtures/interactive-counter.gscene")).is_err());
    }

    #[test]
    fn commit_matches_the_canonical_d1_executor() {
        let app = fixture();
        let source = include_str!("../fixtures/interactive-counter.gscene");
        let handoff = goya_lang::handoff::admit(source).unwrap();
        let compiled = compile_d1_interactions(&handoff.parsed.scene).unwrap();
        let run = |state: i64, input: i64| {
            let mut values = BTreeMap::new();
            values.insert("count".into(), D1Value::Int(state));
            let snapshot = D1Snapshot::new(0, values).unwrap();
            let mut inputs = BTreeMap::new();
            inputs.insert("amount".into(), D1Value::Int(input));
            let inputs = InputSnapshot::new(inputs).unwrap();
            compiled[0].execute(&snapshot, &inputs)
        };
        let outcome = run(1, 2).unwrap();
        assert_eq!(outcome.revision_before, 0);
        assert_eq!(outcome.revision_after, 1);
        assert_eq!(outcome.next_state["count"], D1Value::Int(3));
        assert_eq!(commit(&app.expr, 1, 0, 2), Some((3, 1)));
        assert!(run(i64::MAX, 1).is_err());
        assert_eq!(commit(&app.expr, i64::MAX, 4, 1), None);
        assert_eq!(commit(&app.expr, 5, u64::MAX, 1), None);
    }

    #[test]
    fn non_canonical_integers_do_not_commit() {
        assert_eq!(parse_canonical_i64("0"), Some(0));
        assert_eq!(parse_canonical_i64("-3"), Some(-3));
        assert_eq!(parse_canonical_i64("00"), None);
        assert_eq!(parse_canonical_i64("-0"), None);
        assert_eq!(parse_canonical_i64("+1"), None);
        assert_eq!(parse_canonical_i64("1.0"), None);
        assert_eq!(parse_canonical_i64(""), None);
    }

    #[test]
    fn emitted_transition_agrees_with_eval() {
        let app = fixture();
        let generated = generate(&app, crate::api_floor::ApiFloor::Win10);
        assert!(generated.main_rs.contains("SashInteractiveWindow"));
        assert!(generated.main_rs.contains("BUTTON"));
        assert!(generated.main_rs.contains("EDIT"));
        assert!(generated.main_rs.contains("checked_add"));
        assert!(generated.main_rs.contains("WS_CLIPSIBLINGS"));
        assert!(generated.main_rs.contains("WM_PASTE"));
        assert!(generated
            .main_rs
            .contains("const COPY_ALLOWED: bool = true;"));
        assert!(generated
            .main_rs
            .contains("const PASTE_ALLOWED: bool = false;"));
        assert!(generated.main_rs.contains("ROOT_ID_UNITS"));
        assert!(generated.main_rs.contains("UnregisterClassW"));
        assert!(generated.main_rs.contains("SetWindowLongW"));
        assert!(generated.main_rs.contains("SetWindowLongPtrW"));
        assert!(generated.main_rs.contains("target_pointer_width = \"32\""));
        assert!(generated.main_rs.contains("KeyAction::Activate"));
        assert!(generated.main_rs.contains("ACTIVATING"));
        assert!(generated.main_rs.contains("RELEASED"));
        assert!(generated.main_rs.contains("command_activates"));
        assert!(generated.main_rs.contains("publish_after_text"));
        assert!(generated.main_rs.contains("hold_after_release"));
        assert!(generated.main_rs.contains("visible_commit"));
        assert!(generated.main_rs.contains("VK_DELETE"));
        assert!(!generated.main_rs.contains("\"Add\""));
        assert!(!generated.main_rs.contains("\"Count\""));
        assert!(generated.manifest_xml.contains("InteractiveWindow"));
        assert!(generated.manifest_xml.contains("0.0.2.0"));
        let dir = std::env::temp_dir().join(format!("sash-interactive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let source = dir.join("main.rs");
        std::fs::write(&source, &generated.main_rs).unwrap();
        let metadata = dir.join("main.rmeta");
        let checked = std::process::Command::new("rustc")
            .arg("--edition")
            .arg("2021")
            .arg("--emit=metadata")
            .arg("-o")
            .arg(&metadata)
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            checked.status.success(),
            "generated window failed to typecheck: {}",
            String::from_utf8_lossy(&checked.stderr)
        );
        let _ = std::fs::remove_dir_all(&dir);
        let body = crate::ir_codegen::emit_transition_from_ir(&crate::ir_lower::lower_int_expr(
        &app.expr,
    ));
        let program = format!(
            "{body}fn main() {{ assert_eq!(transition(1, 2), Some(3)); assert_eq!(transition(i64::MAX, 1), None); assert_eq!(transition(4, -4), Some(0)); }}\n"
        );
        let dir = std::env::temp_dir().join(format!("sash-d1-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let source = dir.join("main.rs");
        std::fs::write(&source, program).unwrap();
        let output = dir.join("check");
        let compiled = std::process::Command::new("rustc")
            .arg("--edition")
            .arg("2021")
            .arg("-o")
            .arg(&output)
            .arg(&source)
            .status()
            .unwrap();
        assert!(compiled.success());
        let ran = std::process::Command::new(&output).status().unwrap();
        assert!(ran.success());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(eval(&app.expr, 1, 2), Some(3));
    }

    #[test]
    fn unsupported_interactive_forms_fail() {
        let d2 = "@ontology app { @cell root { id: \"window.root\"; text: \"Hi\"; extent: (80, 40, 0); @button go { id: \"window.go\"; text: \"Go\"; position: (2, 2, 0); extent: (20, 8, 0); } } @interaction fetch { id: \"action.fetch\"; trigger: button.go.activate; class: D2; effects { network; } requests { lookup: effect network capability \"network\" request \"static\" result int64; } state { write count: int; } input { } set count = effect.lookup; } }\n";
        let error = match admit::open(d2) {
            Err(error) => error,
            Ok(_) => panic!("D2 was admitted"),
        };
        assert_eq!(error.code, "SASH-CAP");
        let scalar = "@ontology app { @cell root { id: \"window.root\"; text: \"Hi\"; extent: (80, 40, 0); gestures: allow } }\n";
        let error = match admit::open(scalar) {
            Err(error) => error,
            Ok(_) => panic!("a scalar gestures property was admitted"),
        };
        assert!(
            error.code.starts_with("GY-E"),
            "scalar gesture syntax must fail in the frontend, got {}",
            error.code
        );
        let static_policy = "@ontology app { @cell root { id: \"window.root\"; text: \"Hi\"; extent: (80, 40, 0); gestures { copy: deny; paste: deny; } } }\n";
        let error = match admit::open(static_policy) {
            Err(error) => error,
            Ok(_) => panic!("a static window compiled gesture policy"),
        };
        assert_eq!(error.code, "SASH-CAP");
        let incomplete = "@ontology counter { @cell root { id: \"window.root\"; text: \"Count\"; extent: (80, 40, 0); @input amount { id: \"window.amount\"; text: \"1\"; position: (2, 16, 0); extent: (30, 8, 0); gestures { copy: allow; } } @button add { id: \"window.add\"; text: \"Add\"; position: (34, 16, 0); extent: (20, 8, 0); } @cell total { id: \"state.count\"; text: \"0\"; position: (2, 28, 0); extent: (40, 8, 0); } } @interaction add { id: \"action.add\"; trigger: button.add.activate; class: D1; state { read count: int; write count: int; } input { amount: int; } set count = state.count + input.amount; } }\n";
        let error = match admit::open(incomplete) {
            Err(error) => error,
            Ok(_) => panic!("a gestures block without paste was admitted"),
        };
        assert_eq!(error.code, "SASH-CAP");
        let contradictory = incomplete.replace(
            "gestures { copy: allow; }",
            "gestures { copy: deny; cut: allow; paste: deny; }",
        );
        let error = match admit::open(&contradictory) {
            Err(error) => error,
            Ok(_) => panic!("cut allow with copy deny was admitted"),
        };
        assert_eq!(error.code, "SASH-CAP");
    }

    #[test]
    fn edit_without_gestures_stays_on_0_0_1() {
        let source = include_str!("../fixtures/interactive-counter.gscene")
            .replace("      gestures { copy: allow; paste: deny; }\n", "");
        match admit::open(&source).unwrap() {
            SceneTarget::Interactive(app) => {
                assert_eq!(app.profile, PROFILE);
                assert!(app.paste_allowed);
                let generated = generate(&app, crate::api_floor::ApiFloor::Win10);
                assert!(generated
                    .main_rs
                    .contains("const PASTE_ALLOWED: bool = true;"));
                assert!(generated.manifest_xml.contains("0.0.1.0"));
            }
            SceneTarget::Static(_) => panic!("counter without gestures became static"),
        }
    }

    #[test]
    fn denied_paste_does_not_commit_text() {
        let gate = |message, wparam, ctrl, shift| {
            clipboard_blocked(ClipboardGate {
                copy_allowed: true,
                paste_allowed: false,
                cut_mode: CUT_DEFAULT,
                message,
                wparam,
                ctrl_down: ctrl,
                shift_down: shift,
            })
        };
        assert!(gate(0x0302, 0, false, false));
        assert!(gate(0x0102, 0x16, false, false));
        assert!(gate(0x0100, 0x56, true, false));
        assert!(gate(0x0100, 0x2D, false, true));
        assert!(!gate(0x0301, 0, false, false));
        assert!(!gate(0x0300, 0, false, false));
        assert!(!gate(0x0100, 0x43, true, false));
        assert!(!gate(0x0100, 0x2E, false, true));
        let denied_cut = |message, wparam, ctrl, shift| {
            clipboard_blocked(ClipboardGate {
                copy_allowed: false,
                paste_allowed: false,
                cut_mode: CUT_DENY,
                message,
                wparam,
                ctrl_down: ctrl,
                shift_down: shift,
            })
        };
        assert!(denied_cut(0x0300, 0, false, false));
        assert!(denied_cut(0x0100, 0x58, true, false));
        assert!(denied_cut(0x0100, 0x2E, false, true));
        assert!(!denied_cut(0x0100, 0x2E, false, false));
        assert!(!denied_cut(0x0100, 0x2E, true, false));
    }

    #[test]
    fn keyboard_and_shutdown_follow_the_compiled_plan() {
        assert_eq!(key_action(0x0100, 9, false), KeyAction::FocusNext);
        assert_eq!(key_action(0x0100, 9, true), KeyAction::FocusPrev);
        assert_eq!(key_action(0x0100, 13, false), KeyAction::Activate);
        assert_eq!(key_action(0x0100, 0x20, false), KeyAction::Dispatch);
        assert_eq!(key_action(0x0102, 0x16, false), KeyAction::Dispatch);
        let closed = shutdown_plan(true, false, PumpEnd::Quit(0));
        assert_eq!(closed.exit_code, 0);
        assert!(closed.unregister_class);
        assert!(!closed.destroy_window);
        let failed = shutdown_plan(true, true, PumpEnd::Failed);
        assert!(failed.destroy_window);
        assert_eq!(failed.exit_code, 1);
        let startup = shutdown_plan(true, false, PumpEnd::NotEntered);
        assert!(!startup.destroy_window);
        assert!(startup.unregister_class);
        let unregistered = shutdown_plan(false, false, PumpEnd::NotEntered);
        assert!(!unregistered.unregister_class);
    }

    #[test]
    fn commit_publishes_only_after_the_result_text() {
        assert_eq!(activation(true, 1, Some(1), Some((1, 1))), Activation::Busy);
        assert_eq!(
            activation(false, 0, Some(1), Some((1, 1))),
            Activation::Rejected
        );
        assert_eq!(
            activation(false, 31, Some(1), Some((1, 1))),
            Activation::Rejected
        );
        assert_eq!(activation(false, -1, None, None), Activation::Rejected);
        assert_eq!(
            activation(false, 1, None, Some((1, 1))),
            Activation::Rejected
        );
        assert_eq!(activation(false, 1, Some(1), None), Activation::Rejected);
        let published = activation(false, 1, Some(1), Some((1, 1)));
        assert_eq!(
            published,
            Activation::Publish {
                next: 1,
                revision: 1
            }
        );
        assert_eq!(publish_after_text(published, false), Publish::Hold);
        assert_eq!(
            publish_after_text(published, true),
            Publish::Store {
                next: 1,
                revision: 1
            }
        );
        assert_eq!(
            publish_after_text(Activation::Rejected, true),
            Publish::Hold
        );
        assert_eq!(publish_after_text(Activation::Busy, true), Publish::Hold);
        assert!(!command_activates(301, 6, 301));
        assert!(command_activates(301, 0, 301));
        assert!(!command_activates(201, 0, 301));

        let app = fixture();
        let source = include_str!("../fixtures/interactive-counter.gscene");
        let handoff = goya_lang::handoff::admit(source).unwrap();
        let compiled = compile_d1_interactions(&handoff.parsed.scene).unwrap();
        let mut state = 0i64;
        let mut revision = 0u64;
        for _ in 0..3 {
            let mut values = BTreeMap::new();
            values.insert("count".into(), D1Value::Int(state));
            let snapshot = D1Snapshot::new(revision, values).unwrap();
            let mut inputs = BTreeMap::new();
            inputs.insert("amount".into(), D1Value::Int(1));
            let inputs = InputSnapshot::new(inputs).unwrap();
            let outcome = compiled[0].execute(&snapshot, &inputs).unwrap();
            let parsed = parse_canonical_i64("1");
            let committed = parsed.and_then(|input| commit(&app.expr, state, revision, input));
            let step = activation(false, 1, parsed, committed);
            match publish_after_text(step, true) {
                Publish::Store {
                    next,
                    revision: next_revision,
                } => {
                    assert_eq!(outcome.next_state["count"], D1Value::Int(next));
                    assert_eq!(outcome.revision_after, next_revision);
                    state = next;
                    revision = next_revision;
                }
                Publish::Hold => panic!("a valid addition did not publish"),
            }
        }
        assert_eq!((state, revision), (3, 3));
        let nested = activation(true, 1, Some(1), Some((4, 4)));
        assert_eq!(publish_after_text(nested, true), Publish::Hold);
        let pending = activation(false, 1, Some(1), commit(&app.expr, state, revision, 1));
        assert_eq!(
            pending,
            Activation::Publish {
                next: 4,
                revision: 4
            }
        );
        assert_eq!(publish_after_text(pending, false), Publish::Hold);
        let written = publish_after_text(pending, true);
        assert_eq!(hold_after_release(written, false), written);
        assert_eq!(hold_after_release(written, true), Publish::Hold);
        assert_eq!(hold_after_release(Publish::Hold, true), Publish::Hold);
        let mut values = BTreeMap::new();
        values.insert("count".into(), D1Value::Int(state));
        let snapshot = D1Snapshot::new(revision, values).unwrap();
        let mut inputs = BTreeMap::new();
        inputs.insert("amount".into(), D1Value::Int(1));
        let inputs = InputSnapshot::new(inputs).unwrap();
        let outcome = compiled[0].execute(&snapshot, &inputs).unwrap();
        assert_eq!(outcome.next_state["count"], D1Value::Int(4));
        assert_eq!(outcome.revision_after, 4);
        assert_eq!((state, revision), (3, 3));
        assert_eq!(
            activation(false, 1, Some(1), commit(&app.expr, i64::MAX, 9, 1)),
            Activation::Rejected
        );
        let mut values = BTreeMap::new();
        values.insert("count".into(), D1Value::Int(i64::MAX));
        let snapshot = D1Snapshot::new(9, values).unwrap();
        let mut inputs = BTreeMap::new();
        inputs.insert("amount".into(), D1Value::Int(1));
        let inputs = InputSnapshot::new(inputs).unwrap();
        assert!(compiled[0].execute(&snapshot, &inputs).is_err());
        assert_eq!(
            activation(false, 2, parse_canonical_i64("00"), None),
            Activation::Rejected
        );
    }

    #[test]
    fn visible_result_must_match_before_store() {
        let stored = Publish::Store {
            next: 4,
            revision: 4,
        };
        assert_eq!(
            visible_commit(stored, false, Some(4)),
            VisibleCommit::Store {
                next: 4,
                revision: 4
            }
        );
        assert_eq!(
            visible_commit(stored, false, Some(3)),
            VisibleCommit::Revert
        );
        assert_eq!(visible_commit(stored, false, None), VisibleCommit::Revert);
        assert_eq!(visible_commit(stored, true, Some(4)), VisibleCommit::Leave);
        assert_eq!(
            visible_commit(Publish::Hold, false, Some(4)),
            VisibleCommit::Leave
        );
        let app = fixture();
        let mut state = 0i64;
        let mut revision = 0u64;
        for _ in 0..3 {
            let step = activation(false, 1, Some(1), commit(&app.expr, state, revision, 1));
            let decision = publish_after_text(step, true);
            let next = match decision {
                Publish::Store { next, .. } => next,
                Publish::Hold => panic!("a valid addition did not publish"),
            };
            let visible = parse_canonical_i64(&format_i64(next));
            match visible_commit(decision, false, visible) {
                VisibleCommit::Store {
                    next,
                    revision: next_revision,
                } => {
                    state = next;
                    revision = next_revision;
                }
                VisibleCommit::Revert | VisibleCommit::Leave => {
                    panic!("a matching result did not store")
                }
            }
        }
        assert_eq!((state, revision), (3, 3));
        let step = activation(false, 1, Some(1), commit(&app.expr, state, revision, 1));
        let decision = publish_after_text(step, true);
        assert_eq!(
            visible_commit(decision, false, Some(state)),
            VisibleCommit::Revert
        );
        assert_eq!((state, revision), (3, 3));
    }

    #[test]
    fn static_fixture_stays_on_the_static_profile() {
        match admit::open(include_str!("../fixtures/static-window.gscene")).unwrap() {
            SceneTarget::Static(window) => assert_eq!(window.semantic_id, "window.root"),
            SceneTarget::Interactive(_) => panic!("static fixture changed profile"),
        }
    }
}
