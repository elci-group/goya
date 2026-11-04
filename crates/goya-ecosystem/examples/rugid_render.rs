use goya_ecosystem::{export_rugid, Capability, CapabilityGrant, RugidRenderer};
use goya_lang::parse;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: rugid_render <RUGID-Cargo.toml>")?;
    let source = r#"
@ontology goya_demo {
    @cell root {
        extent: (100, 100, 0)
        fill: #202020
        @button hello {
            position: (10, 10, 0)
            extent: (80, 30, 0)
            text: "Hello from Goya"
            fill: #4080c0
        }
    }
}
"#;
    let scene = parse(source)?;
    let grant = CapabilityGrant::new([Capability::ExportScene]);
    let export = export_rugid(&scene, 100.0, 100.0, &grant)?;
    if !export.omitted_semantics.is_empty() {
        eprintln!(
            "visual export omits: {}",
            export.omitted_semantics.join(", ")
        );
    }
    let svg = RugidRenderer::new(manifest).render(&scene, 100.0, 100.0, &grant)?;
    println!("RUGID rendered {} SVG bytes", svg.len());
    Ok(())
}
