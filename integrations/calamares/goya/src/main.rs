use goya_lang::{layout, parse_strict_gssh4, SceneCell};
use serde_json::json;
use std::{env, fs, process};

fn visit<'a>(cell: &'a SceneCell, out: &mut Vec<&'a SceneCell>) {
    out.push(cell);
    for child in &cell.children {
        visit(child, out);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || !matches!(args[1].as_str(), "check" | "layout") {
        return Err("usage: calamares-gscene-shell <check|layout> <scene.gscene>".into());
    }
    let source = fs::read_to_string(&args[2]).map_err(|e| e.to_string())?;
    let parsed = parse_strict_gssh4(&source).map_err(|e| e.to_string())?;
    let mut cells = Vec::new();
    visit(&parsed.scene.root, &mut cells);
    for required in ["shell.root", "shell.content", "shell.back", "shell.next", "shell.quit"] {
        if !cells.iter().any(|cell| cell.authored_id.as_deref() == Some(required)) {
            return Err(format!("missing required scene element: {required}"));
        }
    }
    if args[1] == "check" {
        println!("GSSH/4: {} nodes admitted", cells.len());
        return Ok(());
    }
    let frame = layout(&parsed.scene, 960.0, 600.0);
    let nodes: Vec<_> = frame.nodes.iter().map(|n| {
        let id = cells.iter().find(|c| c.name == n.name).and_then(|c| c.authored_id.as_deref());
        json!({
            "id": id,
            "name": n.name,
            "parent": n.parent,
            "x": n.rect.x / 1000,
            "y": n.rect.y / 1000,
            "width": n.rect.width / 1000,
            "height": n.rect.height / 1000,
            "fill": n.fill,
            "text": n.text,
            "text_size": n.text_size_mp.map(|size| size / 1000),
            "button": n.is_button,
        })
    }).collect();
    println!("{}", serde_json::to_string_pretty(&json!({"contract": "GSSH/4", "viewport": [960, 600], "nodes": nodes})).map_err(|e| e.to_string())?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("calamares-gscene-shell: {error}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_and_embedded_layout_agree_on_navigation_and_content() {
        let scene = parse_strict_gssh4(include_str!("../shell.gscene")).unwrap().scene;
        let frame = layout(&scene, 960.0, 600.0);
        let embedded: serde_json::Value = serde_json::from_str(include_str!("../shell.layout.json")).unwrap();
        assert_eq!(embedded["contract"], "GSSH/4");
        assert_eq!(embedded["nodes"].as_array().unwrap().len(), frame.nodes.len());
        for id in ["shell.content", "shell.back", "shell.next", "shell.quit"] {
            let node = embedded["nodes"].as_array().unwrap().iter()
                .find(|node| node["id"] == id).unwrap();
            let resolved = frame.find(node["name"].as_str().unwrap()).unwrap();
            assert_eq!(node["x"], resolved.rect.x / 1000, "{id}");
            assert_eq!(node["y"], resolved.rect.y / 1000, "{id}");
            assert_eq!(node["width"], resolved.rect.width / 1000, "{id}");
            assert_eq!(node["height"], resolved.rect.height / 1000, "{id}");
            assert_eq!(node["button"], resolved.is_button, "{id}");
        }
    }
}
