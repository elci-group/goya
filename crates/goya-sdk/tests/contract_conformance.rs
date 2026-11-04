use goya_ir::{ActionId, Node};
use goya_runtime::Event;
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
