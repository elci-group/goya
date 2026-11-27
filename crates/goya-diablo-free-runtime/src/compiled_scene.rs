//! Compiled scene for Diablo Free runtime.

use crate::nir::{FreeBsdNir, NirWindow, NirCapabilities, NirSecurity, NirGraphics};
use goya_diablos::fault::Fault;

/// Compiled scene embedding the NIR and metadata.
#[derive(Debug, Clone)]
pub struct CompiledScene {
    pub nir: FreeBsdNir,
    pub metadata: CompiledMetadata,
}

/// Compiled scene metadata.
#[derive(Debug, Clone, Default)]
pub struct CompiledMetadata {
    pub compiler_version: String,
    pub target_triple: String,
    pub profile: String,
    pub source_digest: String,
}

/// Load a compiled scene from NIR file.
pub fn load_compiled_scene(nir_path: &str) -> Result<CompiledScene, Fault> {
    let nir = crate::load_nir(nir_path)?;
    Ok(CompiledScene {
        nir,
        metadata: CompiledMetadata::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_scene_creation() {
        let nir = FreeBsdNir {
            schema: "GY-DIA-FREE-NIR/1".to_string(),
            target_manifest_digest: "abc".to_string(),
            elements: vec![],
            window: NirWindow {
                title: "Test".to_string(),
                width_dip: 800,
                height_dip: 600,
                min_width_dip: None,
                min_height_dip: None,
                resizable: true,
            },
            capabilities: NirCapabilities {
                filesystem: false,
                network: false,
                subprocess: false,
                graphics: true,
                neural: false,
            },
            security: NirSecurity {
                sandbox_type: "capsicum".to_string(),
                capsicum_rights: None,
                jail_config: None,
            },
            graphics: NirGraphics {
                backend: "drm".to_string(),
                gvi_enabled: true,
                gvi_format: Some("binary".to_string()),
            },
        };

        let scene = CompiledScene {
            nir,
            metadata: CompiledMetadata::default(),
        };
        assert_eq!(scene.metadata.compiler_version, "");
    }
}