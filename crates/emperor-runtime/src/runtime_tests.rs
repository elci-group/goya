// Unit tests for the parent module.
use super::*;

const SCENE: &str = include_str!("../../sash/fixtures/interactive-counter.gscene");

fn counter_ir() -> NativeIr {
    let mut ir = NativeIr {
        schema: SCHEMA.to_owned(),
        profile: PROFILE.to_owned(),
        source_digest_sha256: format!("{:x}", Sha256::digest(SCENE.as_bytes())),
        scene: SCENE.to_owned(),
        action_id: "action.add".to_owned(),
        state_id: "count".to_owned(),
        input_id: "amount".to_owned(),
        initial_state: 0,
        initial_input: 1,
        width_dip: 320,
        height_dip: 160,
        window_controls: vec![
            WindowControl::Minimize,
            WindowControl::Maximize,
            WindowControl::Restore,
            WindowControl::Close,
        ],
        elements: vec![
            Element {
                id: "window.root".to_owned(),
                role: "client".to_owned(),
                text: "Count".to_owned(),
                x_dip: 0,
                y_dip: 0,
                width_dip: 320,
                height_dip: 160,
                span_start: 0,
                span_end: 0,
            },
            Element {
                id: "window.amount".to_owned(),
                role: "edit".to_owned(),
                text: "1".to_owned(),
                x_dip: 8,
                y_dip: 64,
                width_dip: 120,
                height_dip: 32,
                span_start: 0,
                span_end: 0,
            },
            Element {
                id: "window.add".to_owned(),
                role: "button".to_owned(),
                text: "Add".to_owned(),
                x_dip: 136,
                y_dip: 64,
                width_dip: 80,
                height_dip: 32,
                span_start: 0,
                span_end: 0,
            },
            Element {
                id: "state.count".to_owned(),
                role: "result".to_owned(),
                text: "0".to_owned(),
                x_dip: 8,
                y_dip: 112,
                width_dip: 160,
                height_dip: 32,
                span_start: 0,
                span_end: 0,
            },
        ],
        capabilities: Capabilities {
            copy: true,
            paste: false,
            cut: "default".to_owned(),
            filesystem: "none".to_owned(),
            network: false,
            subprocess: false,
            web_content: false,
        },
    };
    let origins = goya_lang::parse_with_source_map(SCENE).expect("fixture origins");
    for element in &mut ir.elements {
        let origin = origins
            .entries
            .iter()
            .find(|entry| {
                entry.kind == "property"
                    && entry.name == "id"
                    && entry.semantic_id.as_deref() == Some(element.id.as_str())
            })
            .expect("fixture id origin");
        element.span_start = origin.range.start;
        element.span_end = origin.range.end;
    }
    ir
}

fn counter_json() -> String {
    serde_json::to_string(&counter_ir()).expect("counter serializes")
}

#[test]
fn backend_routing_prefers_an_explicit_env_var() {
    assert_eq!(backend_for(Some("wayland")), Backend::Wayland);
    assert_eq!(backend_for(Some("kumimanu")), Backend::Kumimanu);
}

// The feature decides only when `EMPEROR_WINDOW_BACKEND` does not name a
// known backend. Each `cfg` variant of `backend_for` compiles with one
// default; running the suite with and without `--features kumimanu` covers
// both.
#[test]
#[cfg(not(feature = "kumimanu"))]
fn backend_default_is_wayland_without_the_feature() {
    assert_eq!(backend_for(None), Backend::Wayland);
    assert_eq!(backend_for(Some("other")), Backend::Wayland);
}

#[test]
#[cfg(feature = "kumimanu")]
fn backend_default_is_kumimanu_with_the_feature() {
    assert_eq!(backend_for(None), Backend::Kumimanu);
    assert_eq!(backend_for(Some("other")), Backend::Kumimanu);
}

#[test]
fn oracle_stores_zero_plus_one() {
    let report = oracle(&counter_json()).expect("oracle");
    assert_eq!(report.count, 1);
    assert_eq!(report.revision, 1);
    assert!(report.copy_allowed);
    assert!(!report.paste_allowed);
    assert_eq!(report.presented, 0);
    assert_eq!(report.wayland, "not-requested");
}

#[test]
fn two_activations_store_count_two() {
    let report = activate_add(&counter_json(), 2).expect("two adds");
    assert_eq!(report.count, 2);
    assert_eq!(report.revision, 2);
    assert!(!report.paste_allowed);
    assert_eq!(report.presented, 0);
    assert_eq!(report.wayland, "not-requested");
}

#[test]
fn unknown_field_is_rejected() {
    let mut value = serde_json::to_value(counter_ir()).expect("value");
    value
        .as_object_mut()
        .expect("object")
        .insert("extra".to_owned(), serde_json::json!(true));
    let json = serde_json::to_string(&value).expect("json");
    assert!(oracle(&json).is_err());
}

#[test]
fn wrong_schema_is_rejected() {
    let mut ir = counter_ir();
    ir.schema = "nope".to_owned();
    let json = serde_json::to_string(&ir).expect("json");
    let err = oracle(&json).expect_err("schema");
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn element_origin_must_be_its_authored_id_property() {
    let mut ir = counter_ir();
    ir.elements[0].span_start = ir.elements[1].span_start;
    ir.elements[0].span_end = ir.elements[1].span_end;
    let json = serde_json::to_string(&ir).expect("json");
    let error = oracle(&json).expect_err("another element's origin is invalid");
    assert!(error.contains("window.root source span"), "{error}");
}

#[test]
fn denied_paste_leaves_the_edit_at_one() {
    assert!(paste_is_ignored(&counter_json()).expect("paste"));
}

#[test]
fn privileged_filesystem_is_rejected() {
    let mut ir = counter_ir();
    ir.capabilities.filesystem = "deny".to_owned();
    let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("filesystem");
    assert!(err.contains("filesystem"), "{err}");
}

#[test]
fn negative_extent_is_rejected() {
    let mut ir = counter_ir();
    ir.elements[1].width_dip = -120;
    let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("width");
    assert!(err.contains("window.amount"), "{err}");
}

#[test]
fn oversized_geometry_is_rejected() {
    let mut ir = counter_ir();
    ir.elements[2].x_dip = MAX_DIP + 1;
    let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("position");
    assert!(err.contains("window.add"), "{err}");
    let mut ir = counter_ir();
    ir.elements[2].height_dip = MAX_DIP + 1;
    let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("extent");
    assert!(err.contains("window.add"), "{err}");
}

#[test]
fn oversized_window_is_rejected() {
    let mut ir = counter_ir();
    ir.width_dip = MAX_DIP + 1;
    let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("window");
    assert!(err.contains("window size"), "{err}");
}

#[test]
fn more_elements_than_sash_cells_is_rejected() {
    let mut ir = counter_ir();
    ir.elements.resize(MAX_ELEMENTS + 1, ir.elements[0].clone());
    let err = oracle(&serde_json::to_string(&ir).expect("json")).expect_err("count");
    assert!(err.contains("33"), "{err}");
}

#[test]
fn canonical_json_keeps_field_order() {
    let json = counter_json();
    assert!(json.starts_with(
        "{\"schema\":\"GY-EMP-NIR/1\",\"profile\":\"emperor.linux.interactive/0.0.1\","
    ));
}

const ELEMENTS: &[CompiledElement] = &[
    CompiledElement {
        id: "window.root",
        role: "client",
        text: "Count",
        x_dip: 0,
        y_dip: 0,
        width_dip: 320,
        height_dip: 160,
        span_start: 0,
        span_end: 0,
    },
    CompiledElement {
        id: "window.amount",
        role: "edit",
        text: "1",
        x_dip: 8,
        y_dip: 64,
        width_dip: 120,
        height_dip: 32,
        span_start: 0,
        span_end: 0,
    },
    CompiledElement {
        id: "window.add",
        role: "button",
        text: "Add",
        x_dip: 136,
        y_dip: 64,
        width_dip: 80,
        height_dip: 32,
        span_start: 0,
        span_end: 0,
    },
    CompiledElement {
        id: "state.count",
        role: "result",
        text: "0",
        x_dip: 8,
        y_dip: 112,
        width_dip: 160,
        height_dip: 32,
        span_start: 0,
        span_end: 0,
    },
];

#[test]
fn compiled_scene_matches_its_json() {
    let ir = counter_ir();
    let json = Box::leak(
        serde_json::to_string(&ir)
            .expect("fixture JSON")
            .into_boxed_str(),
    );
    let mut elements: [CompiledElement; 4] = ELEMENTS.try_into().expect("four fixture elements");
    for (compiled, element) in elements.iter_mut().zip(ir.elements) {
        compiled.span_start = element.span_start;
        compiled.span_end = element.span_end;
    }
    let scene = CompiledScene {
        nir_json: json,
        schema: SCHEMA,
        profile: PROFILE,
        source_digest_sha256: Box::leak(ir.source_digest_sha256.into_boxed_str()),
        scene: SCENE,
        action_id: "action.add",
        state_id: "count",
        input_id: "amount",
        initial_state: 0,
        initial_input: 1,
        width_dip: 320,
        height_dip: 160,
        copy: true,
        paste: false,
        cut: "default",
        filesystem: "none",
        network: false,
        subprocess: false,
        web_content: false,
        elements: Box::leak(Box::new(elements)),
        window_controls: Box::leak(Box::new([
            WindowControl::Minimize,
            WindowControl::Maximize,
            WindowControl::Restore,
            WindowControl::Close,
        ])),
    };
    assert_scene_matches(&scene).expect("match");
    let mut mismatched = scene.clone();
    mismatched.copy = false;
    assert!(assert_scene_matches(&mismatched).is_err());
}

#[test]
fn interaction_script_matches_the_admitted_counter() {
    let ir = counter_ir();
    assert_eq!(ir.schema, SCHEMA);
    assert_eq!(ir.capabilities.filesystem, "none");
    assert!(ir.capabilities.copy);
    assert!(!ir.capabilities.paste);
    assert_eq!(ir.state_id, "count");
    assert_eq!(ir.input_id, "amount");
    let add = ir
        .elements
        .iter()
        .find(|element| element.id == "window.add")
        .expect("add");
    assert_eq!(
        (add.x_dip, add.y_dip, add.width_dip, add.height_dip),
        (136, 64, 80, 32)
    );
    let report = exercise_interaction(&counter_json()).expect("interaction");
    assert_eq!(
        report,
        InteractionReport {
            count: 3,
            revision: 2,
            edit: "2".to_owned(),
            focus: "button".to_owned(),
            paste_ignored: true,
            copy_allowed: true,
            paste_allowed: false,
            add_hit: true,
            rects_stable: true,
            space_activates: true,
        }
    );
}

#[test]
fn denied_paste_ignores_shortcut_empty_and_payload() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert!(!host::paste_shortcut_requests_clipboard(&ir, &controller));
    assert_eq!(controller.session.edit, "1");
    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("9")),
        host::Step::Unchanged
    );
    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("")),
        host::Step::Unchanged
    );
    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, None),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");

    let (mut allowed, mut controller) = open_admitted(&counter_json()).expect("admit");
    allowed.capabilities.paste = true;
    assert!(host::paste_shortcut_requests_clipboard(
        &allowed,
        &controller
    ));
    assert_eq!(
        host::on_clipboard_text(&allowed, &mut controller, None),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(
        host::on_clipboard_text(&allowed, &mut controller, Some("")),
        host::Step::Changed
    );
    assert_eq!(controller.session.edit, "");
    assert_eq!(
        host::on_clipboard_text(&allowed, &mut controller, Some("9")),
        host::Step::Changed
    );
    assert_eq!(controller.session.edit, "9");

    let (mut button_focus, mut controller) = open_admitted(&counter_json()).expect("admit");
    button_focus.capabilities.paste = true;
    controller.focus = goya_parity::keys::Focus::Button;
    assert!(!host::paste_shortcut_requests_clipboard(
        &button_focus,
        &controller
    ));
    assert_eq!(controller.session.edit, "1");
}

#[test]
fn paste_payload_is_ignored_after_focus_moves() {
    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    ir.capabilities.paste = true;
    assert!(host::paste_shortcut_requests_clipboard(&ir, &controller));

    controller.focus = goya_parity::keys::Focus::Button;
    assert!(!host::paste_shortcut_requests_clipboard(&ir, &controller));
    assert_eq!(controller.session.edit, "1");

    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("9")),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("")),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);

    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, None),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert!(!ir.capabilities.paste);
    controller.focus = goya_parity::keys::Focus::Button;
    assert!(!host::paste_shortcut_requests_clipboard(&ir, &controller));
    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("9")),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
}

#[test]
fn paste_payload_is_not_rewritten() {
    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    ir.capabilities.paste = true;
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("00")),
        host::Step::Changed
    );
    assert_eq!(controller.session.edit, "00");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_ne!(controller.session.edit, controller.session.result);

    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("\n")),
        host::Step::Changed
    );
    assert_eq!(controller.session.edit, "\n");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("1\u{200D}")),
        host::Step::Changed
    );
    assert_eq!(controller.session.edit, "1\u{200D}");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert!(!ir.capabilities.paste);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("00")),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(
        host::on_clipboard_text(&ir, &mut controller, Some("\n")),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
}

#[test]
fn shift_delete_deny_does_not_change_the_edit() {
    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    ir.capabilities.cut = "deny".to_owned();
    controller.session.set_edit("12");
    let step = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: false,
        },
    );
    assert_eq!(step.step, host::Step::Unchanged);
    assert!(step.clipboard.is_none());
    assert_eq!(controller.session.edit, "12");
    assert_eq!(controller.session.count, 0);
}

#[test]
fn button_release_inside_activates_and_outside_does_not() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    let outside = host::on_pointer_release(&mut controller, &hits, &mut arm, 8.0, 8.0);
    assert_eq!(outside.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.step, host::Step::Unchanged);
    let inside = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(inside.hit.as_deref(), Some("window.add"));
    assert_eq!(inside.step, host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.edit, "1");
}

#[test]
fn generated_window_controls_render_and_dispatch_on_matching_release() {
    use goya_wayland::{window_management::WindowAction, PlatformEvent};

    let ir = counter_ir();
    let frame = host::scene_frame(&ir, &KeyController::open(&ir.scene).expect("controller"))
        .expect("frame");
    assert_eq!(frame.primitives.len(), 9);
    let controls = host::window_control_rects(&ir);
    assert_eq!(controls.len(), 4);
    let (close, close_rect) = controls[3];
    assert_eq!(close, WindowControl::Close);
    let x = (close_rect.x + close_rect.width / 2) as f64 / goya_wayland::MP_PER_PX as f64;
    let y = (close_rect.y + close_rect.height / 2) as f64 / goya_wayland::MP_PER_PX as f64;
    let mut arm = host::PointerArm::default();
    let mut actions = Vec::new();
    let press = host::on_window_pointer(
        &ir,
        &mut arm,
        &PlatformEvent::PointerPress(x, y),
        |action| actions.push(action),
    );
    assert_eq!(press, Some(false));
    let release = host::on_window_pointer(
        &ir,
        &mut arm,
        &PlatformEvent::PointerRelease(x, y),
        |action| actions.push(action),
    );
    assert_eq!(release, Some(true));
    assert_eq!(actions, vec![WindowAction::Close]);

    let mut arm = host::PointerArm::default();
    let _ = host::on_window_pointer(&ir, &mut arm, &PlatformEvent::PointerPress(x, y), |_| {
        panic!("press must not dispatch")
    });
    let _ = host::on_window_pointer(
        &ir,
        &mut arm,
        &PlatformEvent::PointerRelease(10.0, 10.0),
        |_| panic!("release outside must not dispatch"),
    );
}

#[test]
fn release_without_a_position_does_not_activate() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    let outside = host::on_pointer_release(&mut controller, &hits, &mut arm, -1.0, -1.0);
    assert_eq!(outside.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn edit_press_focuses_without_activating() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 68.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.amount"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 68.0, 80.0);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn result_press_does_not_move_focus() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 88.0, 128.0);
    assert_eq!(press.hit.as_deref(), Some("state.count"));
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 88.0, 128.0);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 88.0, 128.0);
    assert_eq!(press.hit.as_deref(), Some("state.count"));
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 8.0, 8.0);
    assert_eq!(press.hit.as_deref(), Some("window.root"));
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn outside_client_press_does_not_move_focus() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, -1.0, 0.0);
    assert_eq!(press.hit.as_deref(), None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 320.0, 80.0);
    assert_eq!(press.hit.as_deref(), None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 8.0, 160.0);
    assert_eq!(press.hit.as_deref(), None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn non_finite_press_is_a_miss() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, f64::NAN, f64::NAN);
    assert_eq!(press.hit, None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, f64::INFINITY, 80.0);
    assert_eq!(press.hit, None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);

    let press = host::on_pointer_press(
        &ir,
        &mut controller,
        &hits,
        &mut arm,
        80.0,
        f64::NEG_INFINITY,
    );
    assert_eq!(press.hit, None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn finite_press_truncates_toward_zero() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 215.9, 95.9);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(press.step, host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, -0.9, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.root"));
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(
        &ir,
        &mut controller,
        &hits,
        &mut arm,
        f64::from(i32::MAX) + 1.0,
        80.0,
    );
    assert_eq!(press.hit.as_deref(), None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);

    let press = host::on_pointer_press(
        &ir,
        &mut controller,
        &hits,
        &mut arm,
        f64::from(i32::MIN) - 1.0,
        80.0,
    );
    assert_eq!(press.hit.as_deref(), None);
    assert_eq!(press.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn release_truncates_toward_zero() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
    assert_eq!(release.hit.as_deref(), Some("window.add"));
    assert_eq!(release.step, host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 135.9, 80.0);
    assert_eq!(release.hit.as_deref(), Some("window.root"));
    assert_ne!(release.hit.as_deref(), None);
    assert_ne!(release.hit.as_deref(), Some("window.add"));
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 63.9);
    assert_eq!(release.hit.as_deref(), Some("window.root"));
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
}

#[test]
fn non_finite_release_is_a_miss() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, f64::NAN, f64::NAN);
    assert_eq!(release.hit, None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, f64::NAN, f64::NAN);
    assert_eq!(release.hit, None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, f64::INFINITY, 80.0);
    assert_eq!(release.hit, None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release =
        host::on_pointer_release(&mut controller, &hits, &mut arm, 80.0, f64::NEG_INFINITY);
    assert_eq!(release.hit, None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn release_truncates_to_a_miss() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, -1.9, 80.0);
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 320.9, 80.0);
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 80.0, 160.9);
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, -1.9, 80.0);
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 319.9, 159.9);
    assert_eq!(release.hit.as_deref(), Some("window.root"));
    assert_ne!(release.hit.as_deref(), None);
    assert_ne!(release.hit.as_deref(), Some("window.add"));
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn saturated_release_is_a_miss() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let release = host::on_pointer_release(
        &mut controller,
        &hits,
        &mut arm,
        f64::from(i32::MAX) + 1.0,
        80.0,
    );
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(
        &mut controller,
        &hits,
        &mut arm,
        f64::from(i32::MAX) + 1.0,
        80.0,
    );
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(
        &mut controller,
        &hits,
        &mut arm,
        f64::from(i32::MIN) - 1.0,
        80.0,
    );
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(
        &mut controller,
        &hits,
        &mut arm,
        80.0,
        f64::from(i32::MAX) + 1.0,
    );
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(
        &mut controller,
        &hits,
        &mut arm,
        80.0,
        f64::from(i32::MIN) - 1.0,
    );
    assert_eq!(release.hit.as_deref(), None);
    assert_eq!(release.step, host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
}

#[test]
fn second_truncated_release_does_not_store() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    let hits = goya_parity::pointer::hit_map(&ir.scene).expect("hits");
    let mut arm = host::PointerArm::default();
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let press = host::on_pointer_press(&ir, &mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(press.hit.as_deref(), Some("window.add"));
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, 0);

    let release = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
    assert_eq!(release.hit.as_deref(), Some("window.add"));
    assert_eq!(release.step, host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

    let second = host::on_pointer_release(&mut controller, &hits, &mut arm, 215.9, 95.9);
    assert_eq!(second.hit.as_deref(), Some("window.add"));
    assert_ne!(second.hit.as_deref(), None);
    assert_eq!(second.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);

    let late = host::on_pointer_release(&mut controller, &hits, &mut arm, 176.0, 80.0);
    assert_eq!(late.hit.as_deref(), Some("window.add"));
    assert_eq!(late.step, host::Step::Unchanged);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
}

#[test]
fn space_on_the_edit_does_not_insert() {
    let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(host::on_char(&mut controller, ' '), host::Step::Unchanged);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
}

#[test]
fn space_on_the_button_activates() {
    let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_char(&mut controller, ' '), host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
}

#[test]
fn admitted_cut_publishes_the_edit_and_clears_it() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(ir.capabilities.cut, "default");
    let effect = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: false,
        },
    );
    assert_eq!(effect.step, host::Step::Changed);
    assert_eq!(effect.clipboard.as_deref(), Some("1"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.focus = goya_parity::keys::Focus::Button;
    let effect = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: false,
        },
    );
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "1");
}

#[test]
fn delete_chords_that_are_not_cuts() {
    // Cut is Shift+Delete without ctrl. `allow` matches `default`.
    // Plain Delete, Ctrl+Delete, and Ctrl+Shift+Delete copy nothing.
    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    ir.capabilities.cut = "allow".to_owned();
    controller.session.set_edit("12");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    let count = controller.session.count;
    let revision = controller.session.revision;

    let shift_delete = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: false,
        },
    );
    assert_eq!(shift_delete.step, host::Step::Changed);
    assert_eq!(shift_delete.clipboard.as_deref(), Some("12"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, count);
    assert_eq!(controller.session.revision, revision);

    controller.session.set_edit("12");
    let ctrl_shift_delete = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: true,
        },
    );
    assert_eq!(ctrl_shift_delete.step, host::Step::Unchanged);
    assert!(ctrl_shift_delete.clipboard.is_none());
    assert_eq!(controller.session.edit, "12");
    assert_eq!(controller.session.count, count);
    assert_eq!(controller.session.revision, revision);

    let ctrl_delete = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: false,
            ctrl: true,
        },
    );
    assert_eq!(ctrl_delete.step, host::Step::Unchanged);
    assert!(ctrl_delete.clipboard.is_none());
    assert_eq!(controller.session.edit, "12");
    assert_eq!(controller.session.count, count);
    assert_eq!(controller.session.revision, revision);

    let plain_delete = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: false,
            ctrl: false,
        },
    );
    assert_eq!(plain_delete.step, host::Step::Unchanged);
    assert!(plain_delete.clipboard.is_none());
    assert_eq!(controller.session.edit, "12");
    assert_eq!(controller.session.count, count);
    assert_eq!(controller.session.revision, revision);

    controller.focus = goya_parity::keys::Focus::Button;
    let button_shift_delete = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Delete,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: false,
        },
    );
    assert_eq!(button_shift_delete.step, host::Step::Unchanged);
    assert!(button_shift_delete.clipboard.is_none());
    assert_eq!(controller.session.edit, "12");
    assert_eq!(controller.session.count, count);
    assert_eq!(controller.session.revision, revision);
}

#[test]
fn resize_dirties_without_moving_dip_rects() {
    let (ir, _controller) = open_admitted(&counter_json()).expect("admit");
    let before = ir.elements.clone();
    assert_eq!(host::on_resize(&ir), host::Step::Changed);
    assert_eq!(ir.elements, before);
}

#[test]
fn copy_text_follows_the_capability() {
    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(
        host::copy_text(&ir, &controller),
        Some(controller.session.edit.as_str())
    );
    ir.capabilities.copy = false;
    assert_eq!(host::copy_text(&ir, &controller), None);
    assert_eq!(controller.session.edit, "1");
    ir.capabilities.copy = true;
    controller.focus = goya_parity::keys::Focus::Button;
    assert_eq!(host::copy_text(&ir, &controller), None);
    assert_eq!(controller.session.edit, "1");
}

#[test]
fn empty_edit_copy_offers_an_empty_string() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert!(ir.capabilities.copy);
    assert_eq!(ir.capabilities.cut, "default");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    controller.session.set_edit("");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "");

    assert_eq!(host::copy_text(&ir, &controller), Some(""));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.session.set_edit("");
    controller.focus = goya_parity::keys::Focus::Button;
    assert_eq!(host::copy_text(&ir, &controller), None);
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);

    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.session.set_edit("");
    ir.capabilities.copy = false;
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(host::copy_text(&ir, &controller), None);
    assert_eq!(controller.session.edit, "");
}

#[test]
fn noncanonical_edit_is_the_clipboard_text() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert!(ir.capabilities.copy);
    assert_eq!(ir.capabilities.cut, "default");

    controller.session.set_edit("00");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "00");

    assert_eq!(host::copy_text(&ir, &controller), Some("00"));
    assert_eq!(controller.session.edit, "00");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Changed);
    assert_eq!(effect.clipboard.as_deref(), Some("00"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.session.set_edit("-0");
    assert_eq!(host::copy_text(&ir, &controller), Some("-0"));
    assert_eq!(controller.session.result, "0");
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Changed);
    assert_eq!(effect.clipboard.as_deref(), Some("-0"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.session.set_edit("00");
    controller.focus = goya_parity::keys::Focus::Button;
    assert_eq!(host::copy_text(&ir, &controller), None);
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "00");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.session.set_edit("00");
    ir.capabilities.copy = false;
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(host::copy_text(&ir, &controller), None);
    assert_eq!(controller.session.edit, "00");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
}

#[test]
fn on_cut_clears_a_focused_edit_unless_blocked() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(ir.capabilities.cut, "default");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    let count = controller.session.count;
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Changed);
    assert_eq!(effect.clipboard.as_deref(), Some("1"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, count);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.focus = goya_parity::keys::Focus::Button;
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);

    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    ir.capabilities.cut = "deny".to_owned();
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);

    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.session.set_edit("");
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
}

#[test]
fn cut_ignores_the_copy_flag() {
    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert!(ir.capabilities.copy);
    assert_eq!(ir.capabilities.cut, "default");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    ir.capabilities.copy = false;
    assert_eq!(host::copy_text(&ir, &controller), None);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Changed);
    assert_eq!(effect.clipboard.as_deref(), Some("1"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    controller.session.set_edit("00");
    ir.capabilities.copy = false;
    assert_eq!(ir.capabilities.cut, "default");
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Changed);
    assert_eq!(effect.clipboard.as_deref(), Some("00"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    ir.capabilities.copy = false;
    ir.capabilities.cut = "allow".to_owned();
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Changed);
    assert_eq!(effect.clipboard.as_deref(), Some("1"));
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    ir.capabilities.copy = false;
    ir.capabilities.cut = "deny".to_owned();
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let (mut ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    ir.capabilities.copy = false;
    assert_eq!(ir.capabilities.cut, "default");
    controller.focus = goya_parity::keys::Focus::Button;
    let effect = host::on_cut(&ir, &mut controller);
    assert_eq!(effect.step, host::Step::Unchanged);
    assert!(effect.clipboard.is_none());
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
}

#[test]
fn ime_events_do_not_change_the_edit() {
    let (_ir, controller) = open_admitted(&counter_json()).expect("admit");
    let edit = controller.session.edit.clone();
    let count = controller.session.count;
    assert_eq!(host::ignore_ime(&controller), host::Step::Unchanged);
    assert_eq!(controller.session.edit, edit);
    assert_eq!(controller.session.count, count);
}

#[test]
fn focused_frame_uses_the_live_session() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(goya_wayland::MP_PER_PX, 250);
    let scale = goya_wayland::MP_PER_PX;
    let rect = |x: i32, y: i32, width: i32, height: i32| goya_layout::Rect {
        x: i64::from(x) * scale,
        y: i64::from(y) * scale,
        width: i64::from(width) * scale,
        height: i64::from(height) * scale,
    };
    let client = rect(0, 0, 320, 160);
    let edit = rect(8, 64, 120, 32);
    let button = rect(136, 64, 80, 32);
    let result = rect(8, 112, 160, 32);
    let focus_fill = Some(0xFFD6E4F5);

    let expect_frame = |controller: &goya_parity::keys::KeyController, fills: [Option<u32>; 5]| {
        let frame = host::scene_frame(&ir, controller).expect("frame");
        assert_eq!(
            &frame.primitives[..5],
            &vec![
                goya_runtime::Primitive::Panel(client),
                goya_runtime::Primitive::Text(client, "Count".to_owned()),
                goya_runtime::Primitive::Text(edit, controller.session.edit.clone()),
                goya_runtime::Primitive::Button(button, "Add".to_owned()),
                goya_runtime::Primitive::Text(result, controller.session.result.clone()),
            ]
        );
        assert_eq!(&frame.fills[..5], &fills);
        assert_eq!(frame.primitives.len(), 9);
        assert_eq!(frame.foregrounds, vec![None; 9]);
        assert_eq!(frame.fills.iter().filter(|fill| fill.is_some()).count(), 5);
        assert!(frame.fills[0].is_none());
        assert!(frame.fills[1].is_none());
        assert!(frame.fills[4].is_none());
        frame
    };

    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    expect_frame(&controller, [None, None, focus_fill, None, None]);

    assert_eq!(host::on_char(&mut controller, ' '), host::Step::Unchanged);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    expect_frame(&controller, [None, None, focus_fill, None, None]);

    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    expect_frame(&controller, [None, None, None, focus_fill, None]);

    assert_eq!(host::on_tab(&mut controller, true), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.result, "1");
    let frame = expect_frame(&controller, [None, None, focus_fill, None, None]);
    match &frame.primitives[2] {
        goya_runtime::Primitive::Text(rect, text) => {
            assert_eq!(rect, &edit);
            assert_eq!(text, "1");
        }
        other => panic!("edit primitive was {other:?}"),
    }
    match &frame.primitives[4] {
        goya_runtime::Primitive::Text(rect, text) => {
            assert_eq!(rect, &result);
            assert_eq!(text, "1");
        }
        other => panic!("result primitive was {other:?}"),
    }

    controller.session.set_edit("e\u{0301}x");
    let count = controller.session.count;
    let revision = controller.session.revision;
    let result_text = controller.session.result.clone();
    assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
    assert_eq!(controller.session.edit, "e\u{0301}");
    assert_eq!(controller.session.count, count);
    assert_eq!(controller.session.revision, revision);
    assert_eq!(controller.session.result, result_text);
    let frame = expect_frame(&controller, [None, None, focus_fill, None, None]);
    match &frame.primitives[2] {
        goya_runtime::Primitive::Text(_, text) => assert_eq!(text, &controller.session.edit),
        other => panic!("edit primitive was {other:?}"),
    }

    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    let edit_text = controller.session.edit.clone();
    assert_eq!(
        host::on_char(&mut controller, '\u{8}'),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, edit_text);
    assert_eq!(controller.session.count, count);
    assert_eq!(controller.session.revision, revision);
    assert_eq!(controller.session.result, result_text);
    let frame = expect_frame(&controller, [None, None, None, focus_fill, None]);
    match &frame.primitives[2] {
        goya_runtime::Primitive::Text(_, text) => assert_eq!(text, &controller.session.edit),
        other => panic!("edit primitive was {other:?}"),
    }
}

#[test]
fn digits_append_only_on_the_edit() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");

    assert_eq!(host::on_char(&mut controller, '2'), host::Step::Changed);
    assert_eq!(controller.session.edit, "12");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_char(&mut controller, 'é'), host::Step::Changed);
    assert!(controller.session.edit.ends_with('é'));
    assert_eq!(controller.session.edit, "12é");
    assert_eq!(controller.session.count, 0);

    let edit = controller.session.edit.clone();
    assert_eq!(host::on_char(&mut controller, '\t'), host::Step::Unchanged);
    assert_eq!(host::on_char(&mut controller, '\r'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, edit);

    let to_button = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Tab,
        goya_wayland::KeyModifiers {
            shift: false,
            ctrl: true,
        },
    );
    assert_eq!(to_button.step, host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.edit, edit);
    assert_eq!(controller.session.count, 0);

    assert_eq!(host::on_char(&mut controller, '3'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, edit);

    let to_edit = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Tab,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: true,
        },
    );
    assert_eq!(to_edit.step, host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, edit);
    assert_eq!(controller.session.count, 0);

    controller.session.set_edit("1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");
}

#[test]
fn delete_does_not_append_and_backspace_drops_one_cluster() {
    let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(
        host::on_char(&mut controller, '\u{7f}'),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(
        host::on_char(&mut controller, '\u{200D}'),
        host::Step::Changed
    );
    assert_eq!(controller.session.edit, "1\u{200D}");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    controller.session.set_edit("e\u{0301}");
    assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

    controller.session.set_edit("1");
    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(
        host::on_char(&mut controller, '\u{7f}'),
        host::Step::Unchanged
    );
    assert_eq!(
        host::on_char(&mut controller, '\u{200D}'),
        host::Step::Unchanged
    );
    assert_eq!(
        host::on_char(&mut controller, '\u{8}'),
        host::Step::Unchanged
    );
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
}

#[test]
fn shift_tab_on_the_edit_wraps_to_the_button() {
    let (ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let to_button = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Tab,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: false,
        },
    );
    assert_eq!(to_button.step, host::Step::Changed);
    assert!(to_button.clipboard.is_none());
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    let back_to_edit = host::on_modified_special(
        &ir,
        &mut controller,
        goya_wayland::KeySpecial::Tab,
        goya_wayland::KeyModifiers {
            shift: true,
            ctrl: false,
        },
    );
    assert_eq!(back_to_edit.step, host::Step::Changed);
    assert!(back_to_edit.clipboard.is_none());
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);

    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
}

#[test]
fn rejected_edit_does_not_store() {
    let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

    assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
    assert_eq!(controller.session.edit, "");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, "");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_char(&mut controller, '0'), host::Step::Changed);
    assert_eq!(host::on_char(&mut controller, '0'), host::Step::Changed);
    assert_eq!(controller.session.edit, "00");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, "00");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    controller.session.set_edit("1");
    assert_eq!(host::on_char(&mut controller, ','), host::Step::Changed);
    assert_eq!(controller.session.edit, "1,");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, "1,");
    assert_eq!(controller.session.count, 0);

    controller.session.set_edit("");
    assert_eq!(host::on_char(&mut controller, '-'), host::Step::Changed);
    assert_eq!(host::on_char(&mut controller, '0'), host::Step::Changed);
    assert_eq!(controller.session.edit, "-0");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, "-0");
    assert_eq!(controller.session.count, 0);

    let overflow = "9223372036854775808";
    controller.session.set_edit(overflow);
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, overflow);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    controller.session.set_edit("1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");

    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(host::on_char(&mut controller, '0'), host::Step::Unchanged);
    assert_eq!(controller.session.edit, "1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, 2);
    assert_eq!(controller.session.revision, 2);
    assert_eq!(controller.session.result, "2");
    assert_eq!(controller.session.edit, "1");
}

#[test]
fn checked_add_overflow_does_not_store() {
    let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);

    controller.session.count = i64::MAX;
    assert_eq!(controller.session.edit, "1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.count, i64::MAX);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "1");

    // Enter is not focus-gated. This failure is the addition.
    assert_eq!(host::on_tab(&mut controller, false), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Button);
    assert_eq!(controller.session.count, i64::MAX);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "1");
    assert_eq!(host::on_tab(&mut controller, true), host::Step::Changed);
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, i64::MAX);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "1");

    controller.session.count = i64::MIN;
    controller.session.set_edit("-1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.count, i64::MIN);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "-1");

    controller.session.count = 0;
    controller.session.revision = u64::MAX;
    controller.session.set_edit("1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, u64::MAX);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "1");

    // i64::MAX plus 0 stores. The count is not rejected by itself.
    controller.session.count = i64::MAX;
    controller.session.revision = 0;
    controller.session.set_edit("0");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, i64::MAX);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "9223372036854775807");
    assert_eq!(controller.session.edit, "0");

    controller.session.count = 0;
    controller.session.revision = 0;
    controller.session.set_edit("1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(controller.session.edit, "1");

    assert_eq!(host::on_char(&mut controller, '\u{8}'), host::Step::Changed);
    assert_eq!(controller.session.edit, "");
    assert_eq!(host::on_char(&mut controller, '-'), host::Step::Changed);
    assert_eq!(host::on_char(&mut controller, '1'), host::Step::Changed);
    assert_eq!(controller.session.edit, "-1");
    assert_eq!(controller.session.count, 1);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, "1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 2);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "-1");
}

#[test]
fn canonical_i64_min_stores() {
    let (_ir, mut controller) = open_admitted(&counter_json()).expect("admit");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");
    assert_eq!(controller.session.edit, "1");

    let min_text = i64::MIN.to_string();
    assert_eq!(min_text, "-9223372036854775808");
    assert_eq!(min_text.len(), 20);

    controller.session.set_edit(&min_text);
    assert_eq!(controller.session.count, 0);
    assert_eq!(controller.session.revision, 0);
    assert_eq!(controller.session.result, "0");

    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Changed);
    assert_eq!(controller.session.count, i64::MIN);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, min_text);
    assert_eq!(controller.session.edit, min_text);

    controller.session.set_edit("-1");
    assert_eq!(host::on_char(&mut controller, '\n'), host::Step::Unchanged);
    assert_eq!(controller.session.count, i64::MIN);
    assert_eq!(controller.session.revision, 1);
    assert_eq!(controller.session.result, min_text);
    assert_eq!(controller.session.edit, "-1");
    assert_eq!(controller.focus, goya_parity::keys::Focus::Edit);
}

#[test]
#[ignore = "opens one Wayland window and presents the scripted counter"]
#[cfg(not(feature = "kumimanu"))]
fn exercise_presents_the_scripted_counter() {
    // With the kumimanu feature, routing selects Kumimanu and this test
    // would exercise the wrong backend; the variant below covers it.
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    assert!(!display.is_empty(), "WAYLAND_DISPLAY is unset");
    let report = run(&counter_json(), RunMode::Exercise).expect("window");
    assert_eq!(report.wayland, "opened");
    assert!(report.presented >= 1, "{report:?}");
    assert_eq!(report.count, 3);
    assert_eq!(report.revision, 2);
    assert!(report.copy_allowed);
    assert!(!report.paste_allowed);
}

#[cfg(feature = "kumimanu")]
#[test]
#[ignore = "opens one X11 window; run with --ignored when DISPLAY is set"]
fn exercise_presents_the_scripted_counter_on_kumimanu() {
    // With the kumimanu feature compiled in, routing selects Kumimanu
    // unless EMPEROR_WINDOW_BACKEND forces Wayland at build time.
    let display = std::env::var("DISPLAY").unwrap_or_default();
    assert!(!display.is_empty(), "DISPLAY is unset");
    let report = run(&counter_json(), RunMode::Exercise).expect("window");
    assert_eq!(report.window, "kumimanu");
    assert_eq!(report.wayland, "not-used");
    assert!(report.presented >= 1, "{report:?}");
    assert_eq!(report.count, 3);
    assert_eq!(report.revision, 2);
    assert!(report.copy_allowed);
    assert!(!report.paste_allowed);
}

#[cfg(feature = "kumimanu")]
#[test]
#[ignore = "opens one X11 window; run with --ignored when DISPLAY is set"]
fn kumimanu_counter_stores_twelve_from_sent_input() {
    let display = std::env::var("DISPLAY").unwrap_or_default();
    assert!(!display.is_empty(), "DISPLAY is unset");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(super::inject_count_window());
    });
    if let Ok(elf) = std::env::var("KUMIMANU_ELF") {
        let output = std::process::Command::new(&elf)
            .args(["--live", "--frames", "700"])
            .output()
            .unwrap_or_else(|err| panic!("could not start {elf}: {err}"));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "elf exited {:?}\n{stdout}\n{stderr}",
            output.status.code()
        );
        assert!(stdout.contains("window=kumimanu"), "{stdout}");
        assert!(stdout.contains("count=12 "), "{stdout}");
        assert!(stdout.contains("revision=1 "), "{stdout}");
        assert!(stdout.contains("edit=12"), "{stdout}");
        assert!(!stdout.contains("presented=0 "), "{stdout}");
    } else {
        let (ir, controller) = open_admitted(&counter_json()).expect("admit");
        let report = host_kumimanu::run_window(
            &ir,
            controller,
            host::WindowMode::Live {
                max_frames: Some(500),
            },
        )
        .expect("window");
        assert_eq!(report.window, "kumimanu");
        assert_eq!(report.wayland, "not-used");
        assert_eq!(report.edit, "12");
        assert_eq!(report.count, 12);
        assert_eq!(report.revision, 1);
        assert!(report.presented >= 1, "{report:?}");
    }
    let injected = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("injector finished");
    injected.expect("sent key and button");
}
