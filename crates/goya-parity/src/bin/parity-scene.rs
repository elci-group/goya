//! Canonical GSSH/3 geometry and scroll reference for platform comparisons.

use goya_lang::{handoff::admit_v3, layout, CellKind, SceneCell};
use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::{self, Read, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("parity-scene: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let usage = "usage: parity-scene <scene.gscene> <width-points>x<height-points> [...]";
    let source_path = args.next().ok_or(usage)?;
    let viewports = args
        .map(|arg| {
            let (width, height) = arg
                .to_str()
                .and_then(|arg| arg.split_once('x'))
                .ok_or(usage)?;
            Ok::<_, Box<dyn Error>>((width.parse::<i32>()?, height.parse::<i32>()?))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if viewports.is_empty() {
        return Err(usage.into());
    }

    const SOURCE_LIMIT: u64 = 1024 * 1024;
    let mut source = String::new();
    File::open(source_path)?
        .take(SOURCE_LIMIT + 1)
        .read_to_string(&mut source)?;
    if source.len() as u64 > SOURCE_LIMIT {
        return Err("scene exceeds the 1 MiB reference input limit".into());
    }
    let admitted = admit_v3(&source).map_err(|error| {
        format!(
            "{} at bytes {}..{}: {}",
            error.code, error.range.start, error.range.end, error.name
        )
    })?;
    let scene = &admitted.parsed().scene;
    let mut cells = Vec::new();
    collect(&scene.root, None, &mut cells)?;
    let mut name_counts = HashMap::new();
    for (cell, _, _) in &cells {
        *name_counts.entry(cell.name.as_str()).or_insert(0) += 1;
    }
    for (cell, id, _) in &cells {
        if cell.scroll && name_counts[cell.name.as_str()] != 1 {
            return Err(format!("ambiguous scroll element {id}").into());
        }
    }

    let mut stdout = io::BufWriter::new(io::stdout().lock());
    writeln!(
        stdout,
        "viewport_width_pt\tviewport_height_pt\tid\tparent_id\tkind\tx_mp\ty_mp\twidth_mp\theight_mp\tscroll_x_mp\tscroll_y_mp"
    )?;
    for (width, height) in viewports {
        let frame = layout(scene, f64::from(width), f64::from(height));
        if frame.nodes.len() != cells.len() {
            return Err("scene element count differs from reference layout node count".into());
        }
        for ((cell, id, parent), node) in cells.iter().zip(&frame.nodes) {
            if cell.name != node.name {
                return Err(format!("reference layout node order differs at {id}").into());
            }
            let (scroll_x, scroll_y) = if cell.scroll {
                frame.scroll_limits(&cell.name)
            } else {
                (0, 0)
            };
            let kind = match cell.kind {
                CellKind::Cell => "cell",
                CellKind::Button => "button",
                CellKind::Input => "input",
            };
            writeln!(
                stdout,
                "{width}\t{height}\t{}\t{}\t{kind}\t{}\t{}\t{}\t{}\t{scroll_x}\t{scroll_y}",
                escape_id(id),
                escape_id(parent.unwrap_or("")),
                node.rect.x,
                node.rect.y,
                node.rect.width,
                node.rect.height
            )?;
        }
    }
    stdout.flush()?;
    Ok(())
}

fn escape_id(id: &str) -> String {
    id.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

type Element<'a> = (&'a SceneCell, &'a str, Option<&'a str>);

fn collect<'a>(
    cell: &'a SceneCell,
    parent: Option<&'a str>,
    out: &mut Vec<Element<'a>>,
) -> Result<(), Box<dyn Error>> {
    let id = cell.authored_id.as_deref().ok_or("missing authored ID")?;
    out.push((cell, id, parent));
    for child in &cell.children {
        collect(child, Some(id), out)?;
    }
    Ok(())
}
