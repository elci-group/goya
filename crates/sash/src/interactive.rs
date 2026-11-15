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
use crate::emit::{self, Generated};
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

pub(crate) fn emit_transition(expr: &IntExpr) -> String {
    format!(
        "fn transition(state_value: i64, input_value: i64) -> Option<i64> {{\n    {}\n}}\n",
        emit_expr(expr)
    )
}

fn emit_expr(expr: &IntExpr) -> String {
    match expr {
        IntExpr::State => "Some(state_value)".into(),
        IntExpr::Input => "Some(input_value)".into(),
        IntExpr::Literal(value) => {
            if *value == i64::MIN {
                "Some(i64::MIN)".into()
            } else {
                format!("Some({value}_i64)")
            }
        }
        IntExpr::Add(left, right) => format!(
            "({}).and_then(|left| ({}).and_then(|right| left.checked_add(right)))",
            emit_expr(left),
            emit_expr(right)
        ),
    }
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

pub fn generate(app: &AdmittedInteractive) -> Generated {
    let mut main = String::new();
    main.push_str("// Generated by Sash ");
    main.push_str(app.profile);
    main.push_str("\n// source-digest: ");
    main.push_str(&crate::hex_encode(&app.source_digest));
    main.push_str("\n// runtime_qualification: BLOCKED\n");
    main.push_str("#![windows_subsystem = \"windows\"]\n\n");
    main.push_str("use std::sync::atomic::{AtomicI64, AtomicU64};\n");
    let title = emit::array_line(&mut main, "TITLE_UTF8", app.scene_name.as_bytes());
    let identity = emit::array_line(&mut main, "ROOT_ID_UTF8", app.semantic_id.as_bytes());
    main.push_str(&format!(
        "const TITLE_LEN: usize = {};\n",
        app.scene_name.len()
    ));
    main.push_str(&format!(
        "const ROOT_ID_LEN: usize = {};\n",
        app.semantic_id.len()
    ));
    main.push_str(
        "const ROLE_STATIC: u8 = 1;\nconst ROLE_EDIT: u8 = 2;\nconst ROLE_BUTTON: u8 = 3;\n",
    );
    main.push_str("struct Placed { role: u8, control_id: u16, text: &'static [u8], text_len: usize, id: &'static [u8], id_len: usize, x: i32, y: i32, w: i32, h: i32 }\n");
    let mut text_ranges = Vec::new();
    for (index, control) in app.controls.iter().enumerate() {
        let text_name = format!("TEXT_{index}");
        let id_name = format!("ID_{index}");
        let text_range = emit::array_line(&mut main, &text_name, control.text.as_bytes());
        let id_range = emit::array_line(&mut main, &id_name, control.semantic_id.as_bytes());
        main.push_str(&format!(
            "const TEXT_{index}_LEN: usize = {};\nconst ID_{index}_LEN: usize = {};\n",
            control.text.len(),
            control.semantic_id.len()
        ));
        text_ranges.push((text_range, id_range));
    }
    main.push_str("const PLACED: &[Placed] = &[\n");
    let mut placed_ranges = Vec::new();
    for (index, control) in app.controls.iter().enumerate() {
        let start = main.len();
        main.push_str(&format!(
            "    Placed {{ role: {}, control_id: {}, text: TEXT_{index}, text_len: TEXT_{index}_LEN, id: ID_{index}, id_len: ID_{index}_LEN, x: {}, y: {}, w: {}, h: {} }},\n",
            control.role as u8,
            control.control_id,
            control.x_dip,
            control.y_dip,
            control.width_dip,
            control.height_dip
        ));
        placed_ranges.push((start, main.len()));
    }
    main.push_str("];\n");
    let width_at = main.len();
    main.push_str(&format!("const CLIENT_DIP_W: i32 = {};\n", app.width_dip));
    let width_end = main.len();
    let height_at = main.len();
    main.push_str(&format!("const CLIENT_DIP_H: i32 = {};\n", app.height_dip));
    let height_end = main.len();
    main.push_str(&format!(
        "const STATE_INDEX: usize = {};\nconst INPUT_INDEX: usize = {};\nconst BUTTON_INDEX: usize = {};\nconst BUTTON_ID: u16 = {};\n",
        app.state_index,
        app.input_index,
        app.button_index,
        app.controls[app.button_index].control_id
    ));
    main.push_str(&format!(
        "const INITIAL_STATE: i64 = {};\nconst INITIAL_INPUT: i64 = {};\nconst COPY_ALLOWED: bool = {};\nconst PASTE_ALLOWED: bool = {};\nconst CUT_MODE: u8 = {};\n",
        literal(app.initial_state),
        literal(app.initial_input),
        app.copy_allowed,
        app.paste_allowed,
        app.cut_mode
    ));
    main.push_str("static COUNT: AtomicI64 = AtomicI64::new(INITIAL_STATE);\n");
    main.push_str("static REVISION: AtomicU64 = AtomicU64::new(0);\n");
    main.push_str(include_str!("interactive_text.rs"));
    main.push('\n');
    main.push_str(include_str!("interactive_clipboard.rs"));
    main.push('\n');
    main.push_str(include_str!("interactive_keys.rs"));
    main.push('\n');
    main.push_str(include_str!("interactive_commit.rs"));
    main.push('\n');
    let transition_at = main.len();
    main.push_str(&emit_transition(&app.expr));
    let transition_end = main.len();
    main.push_str(
        "fn commit_transition(state_value: i64, revision: u64, input_value: i64) -> Option<(i64, u64)> {\n    let next = transition(state_value, input_value)?;\n    Some((next, revision.checked_add(1)?))\n}\n",
    );
    main.push_str(PROGRAM);
    let digest = crate::sha256_hex(main.as_bytes());
    let mut origins = vec![
        emit::origin("title", &title, &app.ontology_range, &app.semantic_id),
        emit::origin("id", &identity, &app.id_range, &app.semantic_id),
        emit::origin(
            "extent-width",
            &(width_at, width_end),
            &app.extent_range,
            &app.semantic_id,
        ),
        emit::origin(
            "extent-height",
            &(height_at, height_end),
            &app.extent_range,
            &app.semantic_id,
        ),
        emit::origin(
            "interaction",
            &(transition_at, transition_end),
            &app.action_range,
            &app.action_id,
        ),
    ];
    for (control, (text_range, id_range)) in app.controls.iter().zip(&text_ranges) {
        origins.push(emit::origin(
            "text",
            text_range,
            &control.text_range,
            &control.semantic_id,
        ));
        origins.push(emit::origin(
            "id",
            id_range,
            &control.id_range,
            &control.semantic_id,
        ));
    }
    for (control, placed) in app.controls.iter().zip(&placed_ranges) {
        origins.push(emit::origin(
            "extent",
            placed,
            &control.extent_range,
            &control.semantic_id,
        ));
    }
    Generated {
        main_rs: main,
        cargo_toml: emit::CARGO_TOML,
        config_toml: emit::CONFIG_TOML,
        build_rs: emit::BUILD_RS,
        manifest_xml: manifest_xml(app.profile),
        resource_script: resource_script(app.profile),
        source_map: serde_json::json!({
            "schema_version": "1",
            "generator": "sash",
            "profile": app.profile,
            "source_digest": crate::hex_encode(&app.source_digest),
            "generated_digest": digest,
            "files": [{
                "path": "evidence/generated-main.rs",
                "origins": origins,
            }]
        }),
    }
}

fn literal(value: i64) -> String {
    if value == i64::MIN {
        "i64::MIN".into()
    } else {
        format!("{value}")
    }
}

fn profile_version(profile: &str) -> &'static str {
    if profile == PROFILE_CLIPBOARD {
        "0.0.2"
    } else {
        "0.0.1"
    }
}

fn manifest_xml(profile: &str) -> String {
    let version = profile_version(profile);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="ELCI.Goya.Sash.InteractiveWindow" version="{version}.0" processorArchitecture="amd64"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#
    )
}

fn resource_script(profile: &str) -> String {
    let version = profile_version(profile);
    let commas = version.replace('.', ",");
    format!(
        r#"1 24 "app.manifest"

1 VERSIONINFO
FILEVERSION {commas},0
PRODUCTVERSION {commas},0
FILEFLAGSMASK 0x3f
FILEFLAGS 0
FILEOS 0x00040004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Goya Sash interactive window"
      VALUE "ProductName", "Goya Sash"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "App.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}

const PROGRAM: &str = r#"
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, Ordering};

const MAX_PLACED: usize = 8;
static HWNDS: [AtomicPtr<c_void>; MAX_PLACED] = [const { AtomicPtr::new(std::ptr::null_mut()) }; MAX_PLACED];
static ID_PTRS: [AtomicPtr<u16>; MAX_PLACED] = [const { AtomicPtr::new(std::ptr::null_mut()) }; MAX_PLACED];
static ID_LENS: [AtomicPtr<usize>; MAX_PLACED] = [const { AtomicPtr::new(std::ptr::null_mut()) }; MAX_PLACED];
static MESSAGE_FONT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ROOT_ID_PTR: AtomicPtr<u16> = AtomicPtr::new(std::ptr::null_mut());
static ROOT_ID_UNITS: AtomicPtr<usize> = AtomicPtr::new(std::ptr::null_mut());
static EDIT_PROC: AtomicIsize = AtomicIsize::new(0);
static ACTIVATING: AtomicBool = AtomicBool::new(false);
static RELEASED: AtomicBool = AtomicBool::new(false);

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const SS_NOPREFIX: u32 = 0x0000_0080;
const ES_AUTOHSCROLL: u32 = 0x0000_0080;
const BS_DEFPUSHBUTTON: u32 = 0x0000_0001;
const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
const SW_SHOWNORMAL: i32 = 1;
const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_SETFONT: u32 = 0x0030;
const WM_COMMAND: u32 = 0x0111;
const WM_DPICHANGED: u32 = 0x02E0;
const BM_CLICK: u32 = 0x00F5;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SPI_GETNONCLIENTMETRICS: u32 = 0x0029;
const GWLP_WNDPROC: i32 = -4;

const PROP_SEMANTIC: &[u16] = &[
    71, 111, 121, 97, 46, 83, 101, 109, 97, 110, 116, 105, 99, 73, 100, 0,
];

#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(*mut c_void, u32, usize, isize) -> isize>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: *mut c_void,
    h_icon: *mut c_void,
    h_cursor: *mut c_void,
    hbr_background: *mut c_void,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: *mut c_void,
}

#[repr(C)]
struct Msg {
    hwnd: *mut c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct LogFontW {
    height: i32,
    width: i32,
    escapement: i32,
    orientation: i32,
    weight: i32,
    italic: u8,
    underline: u8,
    strike_out: u8,
    char_set: u8,
    out_precision: u8,
    clip_precision: u8,
    quality: u8,
    pitch_and_family: u8,
    face_name: [u16; 32],
}

#[repr(C)]
struct NonClientMetricsW {
    cb_size: u32,
    border_width: i32,
    scroll_width: i32,
    scroll_height: i32,
    caption_width: i32,
    caption_height: i32,
    caption_font: LogFontW,
    sm_caption_width: i32,
    sm_caption_height: i32,
    sm_caption_font: LogFontW,
    menu_width: i32,
    menu_height: i32,
    menu_font: LogFontW,
    status_font: LogFontW,
    message_font: LogFontW,
    padded_border_width: i32,
}

const _: () = assert!(std::mem::size_of::<WndClassExW>() == 80);
const _: () = assert!(std::mem::size_of::<Msg>() == 48);
const _: () = assert!(std::mem::size_of::<Rect>() == 16);
const _: () = assert!(std::mem::size_of::<LogFontW>() == 92);
const _: () = assert!(std::mem::size_of::<NonClientMetricsW>() == 504);

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

#[link(name = "user32")]
extern "system" {
    fn SetProcessDpiAwarenessContext(value: *mut c_void) -> i32;
    fn GetThreadDpiAwarenessContext() -> *mut c_void;
    fn AreDpiAwarenessContextsEqual(left: *mut c_void, right: *mut c_void) -> i32;
    fn GetDpiForSystem() -> u32;
    fn GetDpiForWindow(hwnd: *mut c_void) -> u32;
    fn AdjustWindowRectExForDpi(rect: *mut Rect, style: u32, menu: i32, ex_style: u32, dpi: u32) -> i32;
    fn RegisterClassExW(class: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: *mut c_void,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> *mut c_void;
    fn SetPropW(hwnd: *mut c_void, name: *const u16, data: *mut c_void) -> i32;
    fn RemovePropW(hwnd: *mut c_void, name: *const u16) -> *mut c_void;
    fn DefWindowProcW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: *mut c_void, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(hwnd: *mut c_void) -> i32;
    fn UnregisterClassW(class_name: *const u16, instance: *mut c_void) -> i32;
    fn ShowWindow(hwnd: *mut c_void, cmd: i32) -> i32;
    fn UpdateWindow(hwnd: *mut c_void) -> i32;
    fn LoadCursorW(instance: *mut c_void, name: *const u16) -> *mut c_void;
    fn MoveWindow(hwnd: *mut c_void, x: i32, y: i32, width: i32, height: i32, repaint: i32) -> i32;
    fn GetClientRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
    fn SetWindowPos(hwnd: *mut c_void, insert_after: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn SendMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    fn SystemParametersInfoForDpi(action: u32, param: u32, out: *mut c_void, win_ini: u32, dpi: u32) -> i32;
    fn GetWindowTextW(hwnd: *mut c_void, buffer: *mut u16, max: i32) -> i32;
    fn SetWindowTextW(hwnd: *mut c_void, text: *const u16) -> i32;
    fn SetFocus(hwnd: *mut c_void) -> *mut c_void;
    fn GetKeyState(key: i32) -> i16;
    fn SetWindowLongPtrW(hwnd: *mut c_void, index: i32, new_long: isize) -> isize;
    fn CallWindowProcW(prev: isize, hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateFontIndirectW(font: *const LogFontW) -> *mut c_void;
    fn DeleteObject(object: *mut c_void) -> i32;
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    if TITLE_UTF8.len() != TITLE_LEN || ROOT_ID_UTF8.len() != ROOT_ID_LEN || PLACED.len() != 4 || PLACED.len() > MAX_PLACED {
        return fail("generated lengths do not match");
    }
    if PLACED[STATE_INDEX].role != ROLE_STATIC || PLACED[INPUT_INDEX].role != ROLE_EDIT || PLACED[BUTTON_INDEX].role != ROLE_BUTTON || PLACED[BUTTON_INDEX].control_id != BUTTON_ID {
        return fail("generated control roles do not match");
    }
    for cell in PLACED {
        if cell.text.len() != cell.text_len || cell.id.len() != cell.id_len {
            return fail("generated lengths do not match");
        }
    }
    let title = match wide_utf8(TITLE_UTF8) {
        Some(value) => value,
        None => return fail("window title is not valid UTF-8"),
    };
    unsafe { create_and_run(title.as_ptr()) }
}

fn wide_utf8(bytes: &[u8]) -> Option<Vec<u16>> {
    let text = std::str::from_utf8(bytes).ok()?;
    if text.chars().any(|unit| unit == '\0') {
        return None;
    }
    let mut encoded: Vec<u16> = text.encode_utf16().collect();
    encoded.push(0);
    Some(encoded)
}

fn wide_ascii(text: &str) -> Vec<u16> {
    let mut encoded: Vec<u16> = text.encode_utf16().collect();
    encoded.push(0);
    encoded
}

fn fail(text: &str) -> i32 {
    let body = wide_ascii(text);
    let caption = wide_ascii("Sash");
    unsafe {
        MessageBoxW(std::ptr::null_mut(), body.as_ptr(), caption.as_ptr(), 0x10);
    }
    1
}

fn parse_edit(units: &[u16]) -> Option<i64> {
    if units.is_empty() || units.len() > 20 {
        return None;
    }
    let mut bytes = Vec::with_capacity(units.len());
    for unit in units {
        if *unit > 0x7f {
            return None;
        }
        bytes.push(*unit as u8);
    }
    parse_canonical_i64(std::str::from_utf8(&bytes).ok()?)
}

unsafe fn create_and_run(title: *const u16) -> i32 {
    let per_monitor_v2 = (-4isize) as usize as *mut c_void;
    if SetProcessDpiAwarenessContext(per_monitor_v2) == 0
        && AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), per_monitor_v2) == 0
    {
        return fail("per-monitor DPI awareness is not active");
    }
    let class_name = wide_ascii("SashInteractiveWindow");
    let instance = GetModuleHandleW(std::ptr::null());
    if instance.is_null() {
        return fail("the process module handle is missing");
    }
    let cursor = LoadCursorW(std::ptr::null_mut(), 32512usize as *const u16);
    let class = WndClassExW {
        cb_size: std::mem::size_of::<WndClassExW>() as u32,
        style: 0x0001 | 0x0002,
        lpfn_wnd_proc: Some(wnd_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: instance,
        h_icon: std::ptr::null_mut(),
        h_cursor: cursor,
        hbr_background: (5usize + 1) as *mut c_void,
        lpsz_menu_name: std::ptr::null(),
        lpsz_class_name: class_name.as_ptr(),
        h_icon_sm: std::ptr::null_mut(),
    };
    if RegisterClassExW(&class) == 0 {
        return fail("the window class was not registered");
    }
    let code = run_registered(class_name.as_ptr(), instance, title);
    UnregisterClassW(class_name.as_ptr(), instance);
    code
}

unsafe fn run_registered(class_name: *const u16, instance: *mut c_void, title: *const u16) -> i32 {
    let mut dpi = GetDpiForSystem();
    if dpi == 0 || dpi > 960 {
        dpi = 96;
    }
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: scale(CLIENT_DIP_W, dpi),
        bottom: scale(CLIENT_DIP_H, dpi),
    };
    if AdjustWindowRectExForDpi(&mut rect, WS_OVERLAPPEDWINDOW, 0, 0, dpi) == 0 {
        return fail("the window frame size was not calculated");
    }
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width <= 0 || height <= 0 {
        return fail("the window frame size is empty");
    }
    let hwnd = CreateWindowExW(
        0,
        class_name,
        title,
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        width,
        height,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        return fail("the window was not created");
    }
    ShowWindow(hwnd, SW_SHOWNORMAL);
    UpdateWindow(hwnd);
    let mut message = std::mem::zeroed::<Msg>();
    let mut end = PumpEnd::NotEntered;
    let mut window_live = true;
    loop {
        let status = GetMessageW(&mut message, std::ptr::null_mut(), 0, 0);
        if status == 0 {
            end = PumpEnd::Quit(message.wparam as i32);
            window_live = false;
            break;
        }
        if status < 0 {
            end = PumpEnd::Failed;
            break;
        }
        match key_action(message.message, message.wparam, GetKeyState(0x10) < 0) {
            KeyAction::Activate => {
                let button = HWNDS[BUTTON_INDEX].load(Ordering::Relaxed);
                if !button.is_null() {
                    SendMessageW(button, BM_CLICK, 0, 0);
                }
            }
            KeyAction::FocusNext => focus_cycle(message.hwnd, false),
            KeyAction::FocusPrev => focus_cycle(message.hwnd, true),
            KeyAction::Dispatch => {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    let plan = shutdown_plan(true, window_live, end);
    if plan.destroy_window {
        DestroyWindow(hwnd);
    }
    plan.exit_code
}

fn scale(dip: i32, dpi: u32) -> i32 {
    ((dip as u32).saturating_mul(dpi) / 96) as i32
}

unsafe extern "system" fn wnd_proc(hwnd: *mut c_void, message: u32, wparam: usize, lparam: isize) -> isize {
    match message {
        WM_CREATE => {
            if setup(hwnd) != 0 {
                release(hwnd);
                return -1;
            }
            0
        }
        WM_SIZE => {
            place_all(hwnd);
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xffff) as u16;
            let note = ((wparam >> 16) & 0xffff) as u16;
            if command_activates(id, note, BUTTON_ID) {
                activate();
            }
            0
        }
        WM_DPICHANGED => {
            let rect = lparam as *const Rect;
            if !rect.is_null() {
                let rect = &*rect;
                let width = rect.right.saturating_sub(rect.left);
                let height = rect.bottom.saturating_sub(rect.top);
                SetWindowPos(hwnd, std::ptr::null_mut(), rect.left, rect.top, width, height, SWP_NOZORDER | SWP_NOACTIVATE);
            }
            let _ = apply_font(clamp_dpi((wparam & 0xffff) as u32));
            place_all(hwnd);
            0
        }
        WM_DESTROY => {
            release(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn edit_proc(hwnd: *mut c_void, message: u32, wparam: usize, lparam: isize) -> isize {
    let previous = EDIT_PROC.load(Ordering::Relaxed);
    if previous == 0 {
        return 0;
    }
    let ctrl = GetKeyState(0x11) < 0;
    let shift = GetKeyState(0x10) < 0;
    if clipboard_blocked(ClipboardGate {
        copy_allowed: COPY_ALLOWED,
        paste_allowed: PASTE_ALLOWED,
        cut_mode: CUT_MODE,
        message,
        wparam,
        ctrl_down: ctrl,
        shift_down: shift,
    }) {
        return 0;
    }
    CallWindowProcW(previous, hwnd, message, wparam, lparam)
}

unsafe fn release(hwnd: *mut c_void) {
    RELEASED.store(true, Ordering::Relaxed);
    let previous = EDIT_PROC.swap(0, Ordering::Relaxed);
    if previous != 0 {
        let edit = HWNDS[INPUT_INDEX].load(Ordering::Relaxed);
        if !edit.is_null() {
            SetWindowLongPtrW(edit, GWLP_WNDPROC, previous);
        }
    }
    RemovePropW(hwnd, PROP_SEMANTIC.as_ptr());
    let root_ptr = ROOT_ID_PTR.swap(std::ptr::null_mut(), Ordering::Relaxed);
    let root_len = ROOT_ID_UNITS.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if !root_ptr.is_null() && !root_len.is_null() {
        let len = *root_len;
        drop(Box::from_raw(root_len));
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(root_ptr, len)));
    }
    for index in 0..PLACED.len() {
        let child = HWNDS[index].swap(std::ptr::null_mut(), Ordering::Relaxed);
        if !child.is_null() {
            RemovePropW(child, PROP_SEMANTIC.as_ptr());
        }
        let id_ptr = ID_PTRS[index].swap(std::ptr::null_mut(), Ordering::Relaxed);
        let len_ptr = ID_LENS[index].swap(std::ptr::null_mut(), Ordering::Relaxed);
        if !id_ptr.is_null() && !len_ptr.is_null() {
            let len = *len_ptr;
            drop(Box::from_raw(len_ptr));
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(id_ptr, len)));
        }
    }
    let font = MESSAGE_FONT.swap(std::ptr::null_mut(), Ordering::Relaxed);
    if !font.is_null() {
        DeleteObject(font);
    }
}

unsafe fn setup(hwnd: *mut c_void) -> i32 {
    let root_id = match wide_utf8(ROOT_ID_UTF8) {
        Some(value) => value,
        None => return 1,
    };
    let root_box = root_id.into_boxed_slice();
    let root_len = root_box.len();
    let root_ptr = Box::into_raw(root_box) as *mut u16;
    if SetPropW(hwnd, PROP_SEMANTIC.as_ptr(), root_ptr as *mut c_void) == 0 {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(root_ptr, root_len)));
        return 1;
    }
    ROOT_ID_PTR.store(root_ptr, Ordering::Relaxed);
    ROOT_ID_UNITS.store(Box::into_raw(Box::new(root_len)), Ordering::Relaxed);
    let instance = GetModuleHandleW(std::ptr::null());
    for (index, cell) in PLACED.iter().enumerate() {
        let text = match wide_utf8(cell.text) {
            Some(value) => value,
            None => return 1,
        };
        let identity = match wide_utf8(cell.id) {
            Some(value) => value,
            None => return 1,
        };
        let class_name = match cell.role {
            ROLE_EDIT => wide_ascii("EDIT"),
            ROLE_BUTTON => wide_ascii("BUTTON"),
            _ => wide_ascii("STATIC"),
        };
        let style = match cell.role {
            ROLE_EDIT => WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_CLIPSIBLINGS | ES_AUTOHSCROLL,
            ROLE_BUTTON => WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_CLIPSIBLINGS | BS_DEFPUSHBUTTON,
            _ => WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | SS_NOPREFIX,
        };
        let child = CreateWindowExW(
            0,
            class_name.as_ptr(),
            text.as_ptr(),
            style,
            0,
            0,
            0,
            0,
            hwnd,
            cell.control_id as usize as *mut c_void,
            instance,
            std::ptr::null_mut(),
        );
        if child.is_null() {
            return 1;
        }
        let boxed = identity.into_boxed_slice();
        let id_len = boxed.len();
        let id_ptr = Box::into_raw(boxed) as *mut u16;
        if SetPropW(child, PROP_SEMANTIC.as_ptr(), id_ptr as *mut c_void) == 0 {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(id_ptr, id_len)));
            return 1;
        }
        ID_PTRS[index].store(id_ptr, Ordering::Relaxed);
        ID_LENS[index].store(Box::into_raw(Box::new(id_len)), Ordering::Relaxed);
        HWNDS[index].store(child, Ordering::Relaxed);
        if cell.role == ROLE_EDIT {
            let previous = SetWindowLongPtrW(child, GWLP_WNDPROC, edit_proc as usize as isize);
            if previous == 0 {
                return 1;
            }
            EDIT_PROC.store(previous, Ordering::Relaxed);
        }
    }
    if apply_font(clamp_dpi(GetDpiForWindow(hwnd))) != 0 {
        return 1;
    }
    place_all(hwnd);
    let edit = HWNDS[INPUT_INDEX].load(Ordering::Relaxed);
    if !edit.is_null() {
        SetFocus(edit);
    }
    0
}

unsafe fn activate() {
    if ACTIVATING.swap(true, Ordering::Relaxed) {
        return;
    }
    finish_activation();
    ACTIVATING.store(false, Ordering::Relaxed);
}

unsafe fn finish_activation() {
    let edit = HWNDS[INPUT_INDEX].load(Ordering::Relaxed);
    let result = HWNDS[STATE_INDEX].load(Ordering::Relaxed);
    let mut buffer = [0u16; 32];
    let length = if edit.is_null() || result.is_null() {
        -1
    } else {
        GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32)
    };
    let parsed = if (1..31).contains(&length) {
        parse_edit(&buffer[..length as usize])
    } else {
        None
    };
    let state_value = COUNT.load(Ordering::Relaxed);
    let revision_now = REVISION.load(Ordering::Relaxed);
    let committed = match parsed {
        Some(input_value) => commit_transition(state_value, revision_now, input_value),
        None => None,
    };
    let step = activation(false, length, parsed, committed);
    let written = match step {
        Activation::Publish { next, .. } if !RELEASED.load(Ordering::Relaxed) => {
            let rendered = format_i64(next);
            match wide_utf8(rendered.as_bytes()) {
                Some(wide) => !result.is_null() && SetWindowTextW(result, wide.as_ptr()) != 0,
                None => false,
            }
        }
        Activation::Publish { .. } | Activation::Busy | Activation::Rejected => false,
    };
    let visible = if written && !RELEASED.load(Ordering::Relaxed) {
        let current = HWNDS[STATE_INDEX].load(Ordering::Relaxed);
        if current == result && !result.is_null() {
            let mut seen = [0u16; 32];
            let seen_len = GetWindowTextW(result, seen.as_mut_ptr(), seen.len() as i32);
            if (1..31).contains(&seen_len) {
                parse_edit(&seen[..seen_len as usize])
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let decision = publish_after_text(step, written);
    match visible_commit(decision, RELEASED.load(Ordering::Relaxed), visible) {
        VisibleCommit::Store { next, revision } => {
            COUNT.store(next, Ordering::Relaxed);
            REVISION.store(revision, Ordering::Relaxed);
        }
        VisibleCommit::Revert => {
            let current = HWNDS[STATE_INDEX].load(Ordering::Relaxed);
            if current == result && !result.is_null() && !RELEASED.load(Ordering::Relaxed) {
                let rendered = format_i64(state_value);
                if let Some(wide) = wide_utf8(rendered.as_bytes()) {
                    let _ = SetWindowTextW(result, wide.as_ptr());
                }
            }
        }
        VisibleCommit::Leave => {}
    }
}

unsafe fn focus_cycle(current: *mut c_void, reverse: bool) {
    let order = [INPUT_INDEX, BUTTON_INDEX];
    let position = order.iter().position(|index| HWNDS[*index].load(Ordering::Relaxed) == current);
    let next = match position {
        Some(index) if reverse => order[(index + order.len() - 1) % order.len()],
        Some(index) => order[(index + 1) % order.len()],
        None => order[0],
    };
    let hwnd = HWNDS[next].load(Ordering::Relaxed);
    if !hwnd.is_null() {
        SetFocus(hwnd);
    }
}

fn clamp_dpi(dpi: u32) -> u32 {
    if dpi == 0 || dpi > 960 {
        96
    } else {
        dpi
    }
}

unsafe fn message_font(dpi: u32) -> *mut c_void {
    let mut metrics = std::mem::zeroed::<NonClientMetricsW>();
    metrics.cb_size = std::mem::size_of::<NonClientMetricsW>() as u32;
    if SystemParametersInfoForDpi(
        SPI_GETNONCLIENTMETRICS,
        metrics.cb_size,
        &mut metrics as *mut NonClientMetricsW as *mut c_void,
        0,
        dpi,
    ) == 0
    {
        return std::ptr::null_mut();
    }
    CreateFontIndirectW(&metrics.message_font)
}

unsafe fn apply_font(dpi: u32) -> i32 {
    let font = message_font(dpi);
    if font.is_null() {
        return 1;
    }
    let previous = MESSAGE_FONT.swap(font, Ordering::Relaxed);
    for index in 0..PLACED.len() {
        let child = HWNDS[index].load(Ordering::Relaxed);
        if !child.is_null() {
            SendMessageW(child, WM_SETFONT, font as usize, 1);
        }
    }
    if !previous.is_null() {
        DeleteObject(previous);
    }
    0
}

unsafe fn place_all(parent: *mut c_void) {
    let dpi = clamp_dpi(GetDpiForWindow(parent));
    let mut rect = Rect { left: 0, top: 0, right: 0, bottom: 0 };
    if GetClientRect(parent, &mut rect) == 0 {
        return;
    }
    let client_w = rect.right.saturating_sub(rect.left).max(0);
    let client_h = rect.bottom.saturating_sub(rect.top).max(0);
    for (index, cell) in PLACED.iter().enumerate() {
        let child = HWNDS[index].load(Ordering::Relaxed);
        if child.is_null() {
            continue;
        }
        let x = scale(cell.x, dpi);
        let y = scale(cell.y, dpi);
        let mut width = scale(cell.w, dpi);
        let mut height = scale(cell.h, dpi);
        if x >= client_w || y >= client_h {
            width = 0;
            height = 0;
        } else {
            width = width.min(client_w - x);
            height = height.min(client_h - y);
        }
        MoveWindow(child, x.max(0), y.max(0), width.max(0), height.max(0), 1);
    }
}
"#;

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
        let generated = generate(&app);
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
        let body = emit_transition(&app.expr);
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
                let generated = generate(&app);
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
