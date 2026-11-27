//! FreeBSD lowering: Goya IR → FreeBSD NIR.

use crate::Fault;
use goya_diablos::capabilities::manifest::TargetManifest;
use serde::{Deserialize, Serialize};

/// FreeBSD NIR (Native Intermediate Representation) schema: GY-DIA-FREE-NIR/1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeBsdNir {
    /// NIR schema version
    pub schema: String,
    /// Target manifest digest
    pub target_manifest_digest: String,
    /// Compiled scene elements
    pub elements: Vec<NirElement>,
    /// Window configuration
    pub window: NirWindow,
    /// Capabilities
    pub capabilities: NirCapabilities,
    /// Security policy
    pub security: NirSecurity,
    /// Graphics configuration
    pub graphics: NirGraphics,
}

/// NIR element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NirElement {
    pub id: u32,
    pub role: String, // "client", "edit", "button", "static", "result"
    pub geometry: NirRect,
    pub text: Option<String>,
    pub source_span: Option<String>,
}

/// NIR rectangle (DIP coordinates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NirRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// NIR window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NirWindow {
    pub title: String,
    pub width_dip: u32,
    pub height_dip: u32,
    pub min_width_dip: Option<u32>,
    pub min_height_dip: Option<u32>,
    pub resizable: bool,
}

/// NIR capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NirCapabilities {
    pub filesystem: bool,
    pub network: bool,
    pub subprocess: bool,
    pub graphics: bool,
    pub neural: bool,
}

/// NIR security policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NirSecurity {
    pub sandbox_type: String, // "capsicum", "jail", "none"
    pub capsicum_rights: Option<CapsicumRightsNir>,
    pub jail_config: Option<JailConfigNir>,
}

/// Capsicum rights in NIR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsicumRightsNir {
    pub fd_rights: u64,
    pub cap_rights: u64,
}

/// Jail config in NIR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JailConfigNir {
    pub name: String,
    pub path: String,
    pub hostname: String,
    pub ip4: Option<String>,
}

/// NIR graphics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NirGraphics {
    pub backend: String, // "drm", "wayland", "x11", "software"
    pub gvi_enabled: bool,
    pub gvi_format: Option<String>,
}

/// Lower Goya IR to FreeBSD NIR (stub for Phase 0).
pub fn lower_to_nir(
    _scene_path: &str,
    _target_manifest: &TargetManifest,
) -> Result<FreeBsdNir, Fault> {
    // In real implementation:
    // 1. Parse .gscene with goya-lang
    // 2. Admit with sash::admit
    // 3. Lower to Goya IR
    // 4. Transform to FreeBSD NIR

    Err(Fault::new(
        goya_diablos::fault::exit::VERIFICATION_FAILED,
        "LOWERING_NOT_IMPLEMENTED",
        "Lowering to FreeBSD NIR not implemented in this cycle",
    ))
}

/// Validate FreeBSD NIR.
pub fn validate_nir(_nir: &FreeBsdNir) -> Result<(), Fault> {
    // Would validate against schema GY-DIA-FREE-NIR/1
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nir_schema_version() {
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
        assert_eq!(nir.schema, "GY-DIA-FREE-NIR/1");
    }
}