use goya_sdk::{SceneProject, Viewport};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const TEMPLATE: &str = "@ontology my_app {\n    @cell root {\n        position: (0, 0, 0)\n        extent: (800, 600, 0)\n        fill: #20202a\n\n        @button hello {\n            position: (40, 40, 0)\n            extent: (160, 48, 0)\n            text: \"Hello, Goya\"\n            fill: #3267c8\n        }\n    }\n}\n";

fn usage() {
    println!(
        "goya-studio {}\n\nUsage:\n  goya-studio new <file.gscene>\n  goya-studio check <file.gscene>\n  goya-studio inspect <file.gscene>\n  goya-studio edit <file.gscene> [editor]\n  goya-studio preview <file.gscene> [--live] [scene-binary]\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn project(path: &Path) -> Result<SceneProject, String> {
    SceneProject::from_path(path, Viewport::default()).map_err(|error| error.to_string())
}

fn check(path: &Path) -> Result<(), String> {
    let project = project(path)?;
    println!(
        "valid: ontology={} scene_nodes={} ir_nodes={} viewport={}x{}pt",
        project.scene().name,
        project.frame().nodes.len(),
        project.module().nodes.len(),
        project.viewport().width_points,
        project.viewport().height_points
    );
    Ok(())
}

fn inspect(path: &Path) -> Result<(), String> {
    let project = project(path)?;
    println!("Scene: {}", project.scene().name);
    let frame = project.frame();
    for node in &frame.nodes {
        println!(
            "{} {} x={} y={} w={} h={} text={:?}",
            if node.is_button { "button" } else { "cell" },
            node.name,
            node.rect.x,
            node.rect.y,
            node.rect.width,
            node.rect.height,
            node.text
        );
    }
    Ok(())
}

fn edit(path: &Path, editor: Option<&str>) -> Result<(), String> {
    if !path.exists() {
        std::fs::write(path, TEMPLATE).map_err(|error| format!("cannot create scene: {error}"))?;
    }
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .ok_or_else(|| "set VISUAL/EDITOR or pass the editor executable".to_string())?;
    let status = Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|error| format!("cannot launch editor '{editor}': {error}"))?;
    if !status.success() {
        return Err(format!("editor exited with {status}"));
    }
    check(path)
}

fn preview(path: &Path, live: bool, scene_binary: Option<&str>) -> Result<(), String> {
    // Validate before preview launch so malformed edits never replace the
    // user's last known-good running scene.
    check(path)?;
    if live {
        let binary = scene_binary.unwrap_or("goya-scene");
        let status = Command::new(binary)
            .arg("--file")
            .arg(path)
            .arg("--live")
            .status()
            .map_err(|error| format!("cannot launch scene preview '{binary}': {error}"))?;
        if !status.success() {
            return Err(format!("scene preview exited with {status}"));
        }
    } else {
        inspect(path)?;
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return Ok(());
    };
    match command {
        "--help" | "-h" | "help" => usage(),
        "new" => {
            let path = args.get(1).ok_or_else(|| "new needs a path".to_string())?;
            let path = PathBuf::from(path);
            if path.exists() {
                return Err(format!("refusing to overwrite {}", path.display()));
            }
            std::fs::write(&path, TEMPLATE).map_err(|error| error.to_string())?;
            check(&path)?;
            println!("created {}", path.display());
        }
        "check" => {
            let path = args
                .get(1)
                .ok_or_else(|| "check needs a path".to_string())?;
            check(Path::new(path))?;
        }
        "inspect" => {
            let path = args
                .get(1)
                .ok_or_else(|| "inspect needs a path".to_string())?;
            inspect(Path::new(path))?;
        }
        "edit" => {
            let path = args.get(1).ok_or_else(|| "edit needs a path".to_string())?;
            edit(Path::new(path), args.get(2).map(String::as_str))?;
        }
        "preview" => {
            let path = args
                .get(1)
                .ok_or_else(|| "preview needs a path".to_string())?;
            let live = args.iter().any(|arg| arg == "--live");
            let binary = args
                .iter()
                .skip(2)
                .find(|arg| arg.as_str() != "--live")
                .map(String::as_str);
            preview(Path::new(path), live, binary)?;
        }
        other => return Err(format!("unknown command: {other}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("goya-studio: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_prints_usage_without_error() {
        assert!(run(&[]).is_ok());
    }

    #[test]
    fn invalid_command_is_rejected() {
        assert!(run(&["publish".into()]).is_err());
    }
}
