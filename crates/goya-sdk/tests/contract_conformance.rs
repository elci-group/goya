use goya_ir::{ActionId, Node};
use goya_lang::SceneStore;
use goya_runtime::{Event, Primitive};
use goya_sdk::{SceneProject, Viewport};
use goya_state::{Store, Value};

const SOURCE: &str = r#"
@ontology conformance {
    @cell root {
        position: (0, 0, 0)
        extent: (240, 120, 0)

        @button first {
            position: (10, 20, 0)
            extent: (80, 30, 0)
            text: "First"
        }

        @button increment {
            position: (110, 20, 0)
            extent: (80, 30, 0)
            text: "Increment"
        }
    }
}
"#;

#[test]
fn public_sdk_scene_conforms_across_parse_lower_layout_and_runtime() {
    let viewport = Viewport::new(240.0, 120.0).unwrap();
    let project = SceneProject::from_source(SOURCE, viewport).unwrap();

    assert_eq!(project.scene().name, "conformance");
    assert_eq!(project.module().root.unwrap().0, 2);
    let first = project.lowered().table.get("first").unwrap();
    let increment = project.lowered().table.get("increment").unwrap();
    assert_eq!(first.role, goya_lang::Role::Button);
    assert_eq!(increment.role, goya_lang::Role::Button);
    assert_eq!(
        project.lowered().rect_of("increment"),
        Some((110_000, 20_000, 80_000, 30_000))
    );

    let first_action = match project.module().node(first.ir_node).unwrap() {
        Node::Button { on_activate, .. } => *on_activate,
        other => panic!("button did not lower to IR button: {other:?}"),
    };
    let increment_action = match project.module().node(increment.ir_node).unwrap() {
        Node::Button { on_activate, .. } => *on_activate,
        other => panic!("button did not lower to IR button: {other:?}"),
    };
    assert_eq!(first_action, ActionId(0));
    assert_eq!(increment_action, ActionId(1));

    let scene_frame = project.frame();
    assert_eq!(
        scene_frame.find("increment").unwrap().rect,
        goya_lang::RectMp {
            x: 110_000,
            y: 20_000,
            width: 80_000,
            height: 30_000,
        }
    );

    let store = Store::new();
    let tx = store.begin().unwrap();
    tx.set("count", Value::Int(7));
    tx.commit(|state| matches!(state.get("count"), Some(Value::Int(value)) if *value >= 0))
        .unwrap();
    let mut runtime = project.runtime(store);
    let rect = runtime.frame().rect(increment.ir_node).unwrap();
    let hit_action = runtime
        .hit_test(rect.x + rect.width / 2, rect.y + rect.height / 2)
        .unwrap();
    assert_eq!(hit_action, increment_action);
    assert!(runtime.handle(Event::Activate(hit_action)));
    assert_eq!(runtime.store().get("count"), Some(Value::Int(8)));
}

const RESPONSIVE_SOURCE: &str = r#"
@ontology responsive {
    @cell root {
        @button launch {
            position_p: (0.25, 0.25, 0)
            extent_p: (0.5, 0.5, 0)
            text: "Launch"
        }
    }
}
"#;

const UPDATED_SOURCE: &str = r#"
@ontology updated {
    @cell root {
        @button next {
            position_p: (0.5, 0.25, 0)
            extent_p: (0.25, 0.5, 0)
            text: "Next"
        }
    }
}
"#;

const INVALID_SOURCE: &str = "@ontology broken { @button missing {";

fn button_primitive(runtime: &goya_runtime::Runtime, label: &str) -> goya_layout::Rect {
    runtime
        .render_list()
        .into_iter()
        .find_map(|primitive| match primitive {
            Primitive::Button(rect, actual) if actual == label => Some(rect),
            _ => None,
        })
        .unwrap_or_else(|| panic!("render list did not contain button {label:?}"))
}

#[test]
fn public_sdk_conforms_across_resize_swap_rollback_and_rendering() {
    let small = Viewport::new(240.0, 120.0).unwrap();
    let large = Viewport::new(480.0, 240.0).unwrap();
    let small_project = SceneProject::from_source(RESPONSIVE_SOURCE, small).unwrap();
    let large_project = SceneProject::from_source(RESPONSIVE_SOURCE, large).unwrap();

    // Rebuilding a project for a resized viewport updates proportional scene
    // geometry at the public SDK boundary.
    assert_eq!(
        small_project.frame().find("launch").unwrap().rect,
        goya_lang::RectMp {
            x: 60_000,
            y: 30_000,
            width: 120_000,
            height: 60_000,
        }
    );
    assert_eq!(
        large_project.frame().find("launch").unwrap().rect,
        goya_lang::RectMp {
            x: 120_000,
            y: 60_000,
            width: 240_000,
            height: 120_000,
        }
    );

    // A host retains interaction state while replacing its active source only
    // after SceneStore accepts the candidate. A rejected parse must preserve
    // generation, active scene, state, and the currently rendered primitives.
    let mut scenes = SceneStore::new(RESPONSIVE_SOURCE).unwrap();
    let active_source = RESPONSIVE_SOURCE;
    let state = Store::new();
    let tx = state.begin().unwrap();
    tx.set("count", Value::Int(7));
    tx.commit(|_| true).unwrap();
    let active_project = SceneProject::from_source(active_source, small).unwrap();
    let active_runtime = active_project.runtime(state.clone());
    let rendered_before = active_runtime.render_list();
    let launch_record = active_project.lowered().table.get("launch").unwrap();
    let flow_rect = active_runtime.frame().rect(launch_record.ir_node).unwrap();
    assert_eq!(button_primitive(&active_runtime, "Launch"), flow_rect);

    assert!(scenes.swap(INVALID_SOURCE).is_err());
    assert_eq!(scenes.generation(), 1);
    assert_eq!(scenes.get().name, "responsive");
    assert_eq!(state.get("count"), Some(Value::Int(7)));
    assert_eq!(active_project.scene().name, "responsive");
    assert_eq!(active_runtime.render_list(), rendered_before);

    // A valid replacement advances the scene generation; the application can
    // then build the new project while reusing its state handle.
    assert_eq!(scenes.swap(UPDATED_SOURCE).unwrap(), 2);
    assert_eq!(scenes.get().name, "updated");
    let updated_project = SceneProject::from_source(UPDATED_SOURCE, small).unwrap();
    let updated_runtime = updated_project.runtime(state.clone());
    assert_eq!(state.get("count"), Some(Value::Int(7)));
    let next_record = updated_project.lowered().table.get("next").unwrap();
    let next_rect = updated_runtime.frame().rect(next_record.ir_node).unwrap();
    assert_eq!(button_primitive(&updated_runtime, "Next"), next_rect);
}

#[test]
fn public_sdk_preserves_input_node_semantics_through_rendering() {
    let source = r#"
@ontology composer_contract {
    @cell root {
        position: (0, 0, 0)
        extent: (240, 80, 0)

        @input composer {
            position: (12, 20, 0)
            extent: (180, 32, 0)
            text: "Type here"
        }
    }
}
"#;
    let viewport = Viewport::new(240.0, 80.0).unwrap();
    let project = SceneProject::from_source(source, viewport).unwrap();
    let input = project.lowered().table.get("composer").unwrap();

    assert_eq!(input.role, goya_lang::Role::Input);
    assert_eq!(
        project.module().node(input.ir_node),
        Some(&Node::Input {
            placeholder: "Type here".to_string()
        })
    );
    assert!(project.frame().find("composer").unwrap().is_input);
    assert_eq!(
        project.lowered().rect_of("composer"),
        Some((12_000, 20_000, 180_000, 32_000))
    );

    let runtime = project.runtime(Store::new());
    let rendered = runtime.render_list();
    assert!(rendered.iter().any(|primitive| matches!(
        primitive,
        Primitive::Text(rect, text)
            if *rect == runtime.frame().rect(input.ir_node).unwrap()
                && text == "Type here"
    )));
}
