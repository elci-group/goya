use goya_a11y::gtk_atspi::{AtAction, GtkAtspiBridge};
use goya_a11y::A11yTree;
use goya_change::PixelRect;
use goya_ddh::{DdhNode, Dig, InteractionState, SemanticRole};

fn node(id: &str, role: SemanticRole, label: &str, parent: Option<&str>) -> DdhNode {
    DdhNode {
        id: id.into(),
        revision: 1,
        role,
        label: label.into(),
        hit_region: PixelRect {
            width: 120,
            height: 36,
            ..Default::default()
        },
        state: InteractionState {
            enabled: true,
            visible: true,
            focused: false,
        },
        actions: vec!["press".into()],
        exposed: true,
        parent: parent.map(str::to_owned),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut dig = Dig::new();
    dig.put(
        node("window", SemanticRole::Window, "Goya AT-SPI demo", None),
        1,
    );
    dig.put(
        node("controls", SemanticRole::Panel, "Controls", Some("window")),
        1,
    );
    dig.put(
        node("status", SemanticRole::Label, "Ready", Some("controls")),
        1,
    );
    dig.put(
        node("submit", SemanticRole::Button, "Submit", Some("controls")),
        1,
    );
    dig.put(
        node("query", SemanticRole::Textbox, "Search", Some("controls")),
        1,
    );

    let bridge = GtkAtspiBridge::new(A11yTree::from_dig(&dig), "Goya AT-SPI demo");
    bridge.run(|id, action| match action {
        AtAction::Press => println!("activated {id}"),
        AtAction::SetValue(value) => println!("updated {id}: {value}"),
    })?;
    Ok(())
}
