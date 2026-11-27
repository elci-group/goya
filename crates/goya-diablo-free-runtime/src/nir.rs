//! FreeBSD NIR (Native Intermediate Representation) schema.

use serde::{Deserialize, Serialize};

/// NIR schema identifier.
pub const NIR_SCHEMA: &str = "GY-DIA-FREE-NIR/1";

/// FreeBSD NIR (Native Intermediate Representation) - defined locally for runtime independence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirElement {
    pub id: u32,
    pub role: String,
    pub geometry: NirRect,
    pub text: Option<String>,
    pub source_span: Option<String>,
}

/// NIR rectangle (DIP coordinates).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// NIR window configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirWindow {
    pub title: String,
    pub width_dip: u32,
    pub height_dip: u32,
    pub min_width_dip: Option<u32>,
    pub min_height_dip: Option<u32>,
    pub resizable: bool,
}

/// NIR capabilities.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirCapabilities {
    pub filesystem: bool,
    pub network: bool,
    pub subprocess: bool,
    pub graphics: bool,
    pub neural: bool,
}

/// NIR security policy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirSecurity {
    pub sandbox_type: String,
    pub capsicum_rights: Option<CapsicumRightsNir>,
    pub jail_config: Option<JailConfigNir>,
}

/// Capsicum rights in NIR.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapsicumRightsNir {
    pub fd_rights: u64,
    pub cap_rights: u64,
}

/// Jail config in NIR.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JailConfigNir {
    pub name: String,
    pub path: String,
    pub hostname: String,
    pub ip4: Option<String>,
}

/// NIR graphics configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NirGraphics {
    pub backend: String,
    pub gvi_enabled: bool,
    pub gvi_format: Option<String>,
}

/// Validate NIR against schema.
pub fn validate_nir(nir: &FreeBsdNir) -> Result<(), String> {
    if nir.schema != NIR_SCHEMA {
        return Err(format!("Invalid NIR schema: expected {}, got {}", NIR_SCHEMA, nir.schema));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nir_schema_constant() {
        assert_eq!(NIR_SCHEMA, "GY-DIA-FREE-NIR/1");
    }
}