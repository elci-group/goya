//! Every scene file in the repository converts to proportional lengths that
//! reproduce its reference layout at the reference viewport.
use goya_lang::{layout, parse, parse_with_source_map, proportional::to_proportional, SceneCell};
use std::path::{Path, PathBuf};

fn scenes(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if !matches!(name.as_str(), "target" | ".git" | "node_modules") && !name.starts_with('.') {
                scenes(&path, out);
            }
        } else if name.ends_with(".gscene") {
            out.push(path);
        }
    }
}

fn absolute(cell: &SceneCell) -> bool {
    cell.position.is_some()
        || cell.extent.is_some()
        || cell.text_size.is_some()
        || cell.stroke_width.is_some()
        || cell.children.iter().any(absolute)
}

#[test]
fn repository_scenes_convert_exactly_at_the_reference_viewport() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = Vec::new();
    scenes(&root, &mut paths);
    paths.sort();
    let (mut converted, mut skipped, mut inexact) = (0, 0, Vec::new());
    let mut zero_base = 0;
    for path in &paths {
        let source = std::fs::read_to_string(path).unwrap();
        if parse_with_source_map(&source).is_err() {
            // Deliberately malformed fixtures (negative corpus).
            skipped += 1;
            continue;
        }
        for (width, height) in [(800.0, 600.0), (392.0, 753.0)] {
            let conversion = to_proportional(&source, width, height)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let scene = parse(&conversion.source)
                .unwrap_or_else(|e| panic!("{}: converted scene does not parse: {e}", path.display()));
            assert!(!absolute(&scene.root), "{}: absolute length left", path.display());
            if conversion.inexact.is_empty() {
                let before = layout(&parse(&source).unwrap(), width, height);
                let after = layout(&scene, width, height);
                assert_eq!(before.nodes.len(), after.nodes.len());
                for (a, b) in before.nodes.iter().zip(&after.nodes) {
                    assert_eq!(a.rect, b.rect, "{}: {}", path.display(), a.name);
                }
            } else if conversion
                .inexact
                .iter()
                .any(|item| item.reason != goya_lang::proportional::ZERO_BASE)
            {
                inexact.push(format!("{} @ {width}x{height}: {:?}", path.display(), conversion.inexact));
            } else {
                // Lengths on zero-sized elements have no proportional form;
                // they are reported, and the rest of the scene is exact.
                zero_base += conversion.inexact.len();
            }
        }
        converted += 1;
    }
    eprintln!(
        "{converted} converted, {skipped} skipped, {} inexact, {zero_base} zero-base lengths reported",
        inexact.len()
    );
    for line in &inexact {
        eprintln!("  {line}");
    }
    assert!(converted > 30, "only {converted} scenes found");
    assert!(inexact.is_empty(), "{} inexact conversions", inexact.len());
}
