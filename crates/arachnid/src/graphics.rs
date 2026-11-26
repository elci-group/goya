//! GVI lowering for webOS (contract `arachnid.graphics/0.0.1`).
//!
//! The lowering, encoding choice and verification are shared with Aragon in
//! `goya_scene_graphics`. Arachnid adds its diagnostics, packages each
//! inventory with the generated `gvi.js` runtime, and checks that runtime
//! against the reference decoder (tests below). The TV checks each file's
//! SHA-256 before decoding it.

use crate::diagnostics::{Code, Diagnostic};
pub use goya_scene_graphics::*;

pub const CONTRACT: &str = "arachnid.graphics/0.0.1";

/// The runtime decoder needs `BigInt` (Chromium 67) and `Path2D`.
pub const MIN_CHROMIUM: u16 = 67;

/// Generated runtime file name.
pub const FILE: &str = "gvi.js";

/// Compile every graphic a scene declares.
pub fn compile_all(parsed: &goya_lang::ParsedScene) -> Result<Vec<Compiled>, Vec<Diagnostic>> {
    let mut out = Vec::new();
    let mut diags = Vec::new();
    for g in &parsed.declarations.graphics {
        match compile(&parsed.scene.name, g) {
            Ok(c) => out.push(c),
            Err(why) => diags.push(
                Diagnostic::new(Code::GraphicNotCompiled, format!("@graphic {}: {why}", g.name))
                    .at(Some(g.range)),
            ),
        }
    }
    if diags.is_empty() {
        Ok(out)
    } else {
        Err(diags)
    }
}


#[cfg(test)]
mod tests {
    fn node() -> Option<std::path::PathBuf> {
        if std::env::var_os("ARACHNID_SKIP_NODE").is_some() {
            return None;
        }
        ["/usr/bin/node", "/usr/local/bin/node"].iter().map(std::path::PathBuf::from).find(|p| p.exists())
    }

    /// The generated `gvi.js` decodes every state of every shared
    /// conformance vector exactly as the reference decoder does.
    #[test]
    fn runtime_decoder_matches_the_reference_decoder() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let pairs = std::fs::read_to_string(manifest.join("../../apps/pairs.gscene")).unwrap();
        let Some(node) = node() else {
            eprintln!("node not found or ARACHNID_SKIP_NODE set: runtime conformance not run");
            return;
        };
        let dir = std::env::temp_dir().join(format!("arachnid-gvi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let js = dir.join("gvi.js");
        std::fs::write(&js, crate::gvi_js::GVI_JS).unwrap();
        let script = manifest.join("tests/runtime/gvi-conformance.js");
        for v in goya_scene_graphics::conformance_vectors(&[(&pairs, "card")]) {
            let file = dir.join("v.gvi");
            let refs = dir.join("v.json");
            std::fs::write(&file, &v.bytes).unwrap();
            std::fs::write(&refs, &v.reference).unwrap();
            let radices: Vec<String> = v.radices.iter().map(usize::to_string).collect();
            let out = std::process::Command::new(&node)
                .args([script.as_os_str(), js.as_os_str(), file.as_os_str()])
                .arg(radices.join(","))
                .arg(&v.sha256)
                .arg(&refs)
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            eprintln!("{} ({} bytes): {}", v.label, v.bytes.len(), text.trim());
            assert!(out.status.success() && text.starts_with("PASS"), "{}: {text}", v.label);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
