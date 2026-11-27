//! Versioned capability discovery for Diablos.

use crate::capabilities::manifest::TargetManifest;
use crate::Fault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Discovered capability information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredCapability {
    /// Capability identifier
    pub id: String,
    /// Capability version
    pub version: String,
    /// Whether capability is available
    pub available: bool,
    /// Source of discovery
    pub source: CapabilitySource,
    /// Additional metadata
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Source of capability discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilitySource {
    /// Discovered via system interrogation
    System,
    /// Declared in target manifest
    Manifest,
    /// Negotiated via Caprichios
    Caprichio,
    /// Inferred from hardware
    Hardware,
    /// Configured by deployment profile
    Profile,
}

/// Discover capabilities for a target.
pub fn discover_capabilities(manifest: &TargetManifest) -> Result<Vec<DiscoveredCapability>, Fault> {
    let mut capabilities = Vec::new();

    // 1. Core BSD capabilities (always available on BSD)
    capabilities.push(DiscoveredCapability {
        id: "bsd.kqueue".to_string(),
        version: "1.0".to_string(),
        available: true,
        source: CapabilitySource::Manifest,
        metadata: BTreeMap::new(),
    });

    // 2. Variant-specific capabilities
    match manifest.target.variant.as_str() {
        "freebsd" | "ghostbsd" | "midnightbsd" | "hardenedbsd" => {
            capabilities.push(DiscoveredCapability {
                id: "freebsd.capsicum".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
            capabilities.push(DiscoveredCapability {
                id: "freebsd.jail".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
        }
        "openbsd" => {
            capabilities.push(DiscoveredCapability {
                id: "openbsd.pledge".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
            capabilities.push(DiscoveredCapability {
                id: "openbsd.unveil".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
            capabilities.push(DiscoveredCapability {
                id: "openbsd.w-x".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
        }
        "netbsd" => {
            capabilities.push(DiscoveredCapability {
                id: "netbsd.kauth".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
            capabilities.push(DiscoveredCapability {
                id: "netbsd.veriexec".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
        }
        "dragonfly" => {
            capabilities.push(DiscoveredCapability {
                id: "dragonfly.lwkt".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
            capabilities.push(DiscoveredCapability {
                id: "dragonfly.hammer2".to_string(),
                version: "1.0".to_string(),
                available: true,
                source: CapabilitySource::Manifest,
                metadata: BTreeMap::new(),
            });
        }
        _ => {}
    }

    // 3. Graphics capabilities
    if manifest.graphics.gvi {
        capabilities.push(DiscoveredCapability {
            id: "graphics.gvi".to_string(),
            version: "1.0".to_string(),
            available: true,
            source: CapabilitySource::Manifest,
            metadata: BTreeMap::new(),
        });
    }

    if manifest.graphics.backend != "software" {
        capabilities.push(DiscoveredCapability {
            id: format!("graphics.{}", manifest.graphics.backend),
            version: "1.0".to_string(),
            available: true,
            source: CapabilitySource::Manifest,
            metadata: BTreeMap::new(),
        });
    }

    // 4. Execution capabilities
    if manifest.execution.native_threads {
        capabilities.push(DiscoveredCapability {
            id: "execution.native_threads".to_string(),
            version: "1.0".to_string(),
            available: true,
            source: CapabilitySource::Manifest,
            metadata: BTreeMap::new(),
        });
    }

    // 5. Architecture capabilities
    capabilities.push(DiscoveredCapability {
        id: format!("arch.{}", manifest.target.architecture),
        version: "1.0".to_string(),
        available: true,
        source: CapabilitySource::Hardware,
        metadata: BTreeMap::new(),
    });

    // 6. Caprichios from manifest
    for cap in &manifest.caprichios {
        capabilities.push(DiscoveredCapability {
            id: format!("caprichio.{}", cap),
            version: "1.0".to_string(),
            available: true,
            source: CapabilitySource::Caprichio,
            metadata: BTreeMap::new(),
        });
    }

    Ok(capabilities)
}

/// Check if a specific capability is available.
pub fn has_capability(manifest: &TargetManifest, capability_id: &str) -> bool {
    discover_capabilities(manifest)
        .map(|caps| caps.iter().any(|c| c.id == capability_id && c.available))
        .unwrap_or(false)
}

/// Get capability version if available.
pub fn capability_version(manifest: &TargetManifest, capability_id: &str) -> Option<String> {
    discover_capabilities(manifest)
        .ok()
        .and_then(|caps| {
            caps.iter()
                .find(|c| c.id == capability_id && c.available)
                .map(|c| c.version.clone())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::manifest::TargetManifest;

    #[test]
    fn discover_freebsd_capabilities() {
        let manifest = TargetManifest::freebsd_desktop("x86_64", "14.2");
        let caps = discover_capabilities(&manifest).unwrap();

        assert!(caps.iter().any(|c| c.id == "bsd.kqueue"));
        assert!(caps.iter().any(|c| c.id == "freebsd.capsicum"));
        assert!(caps.iter().any(|c| c.id == "freebsd.jail"));
        assert!(caps.iter().any(|c| c.id == "graphics.gvi"));
    }

    #[test]
    fn discover_openbsd_capabilities() {
        let manifest = TargetManifest::openbsd("x86_64", "7.6");
        let caps = discover_capabilities(&manifest).unwrap();

        assert!(caps.iter().any(|c| c.id == "bsd.kqueue"));
        assert!(caps.iter().any(|c| c.id == "openbsd.pledge"));
        assert!(caps.iter().any(|c| c.id == "openbsd.unveil"));
        assert!(caps.iter().any(|c| c.id == "openbsd.w-x"));
    }

    #[test]
    fn has_capability_check() {
        let manifest = TargetManifest::freebsd_desktop("x86_64", "14.2");
        assert!(has_capability(&manifest, "freebsd.capsicum"));
        assert!(!has_capability(&manifest, "openbsd.pledge"));
    }
}