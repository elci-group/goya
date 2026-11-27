//! Artefact provenance and SBOM for Diablos.

use crate::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Artefact provenance record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtefactProvenance {
    /// Schema version
    pub schema: String,
    /// Directive ID
    pub directive: String,
    /// Compiler identity
    pub compiler: CompilerIdentity,
    /// Source information
    pub source: SourceInfo,
    /// Target capability profile
    pub target_profile: TargetProfile,
    /// Dependency manifest (SBOM)
    pub dependencies: DependencyManifest,
    /// Build configuration digest
    pub build_config_digest: String,
    /// Security policy declaration
    pub security_policy: SecurityPolicyDecl,
    /// Correctness assessment reference
    pub correctness: CorrectnessRef,
    /// Artefact integrity digest
    pub artefact_integrity: ArtefactIntegrity,
    /// Timestamp
    pub timestamp: u64,
}

/// Compiler identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerIdentity {
    pub name: String,
    pub version: String,
    pub directive: String,
}

/// Source information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Source file digest (SHA-256)
    pub digest: String,
    /// Source file path
    pub path: String,
    /// Git revision
    pub revision: String,
}

/// Target capability profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetProfile {
    pub family: String,
    pub variant: String,
    pub architecture: String,
    pub abi: String,
    pub profile: String,
    pub manifest_digest: String,
}

/// Dependency manifest (SPDX-lite).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyManifest {
    /// Direct dependencies
    pub direct: Vec<Dependency>,
    /// Transitive dependencies (optional)
    pub transitive: Vec<Dependency>,
    /// Note about completeness
    pub note: String,
}

/// Single dependency entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub source: String, // crate.io, system, etc.
    pub license: String,
    pub sha256: Option<String>,
}

/// Build configuration digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub rustc_version: String,
    pub cargo_version: String,
    pub target: String,
    pub profile: String,
    pub features: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// Security policy declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicyDecl {
    pub sandbox_type: String,
    pub filesystem_policy: String,
    pub network_policy: String,
    pub enforceable: bool,
}

/// Correctness assessment reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectnessRef {
    pub inquisition_report_digest: Option<String>,
    pub oracle_passed: bool,
    pub smoke_passed: Option<bool>,
}

/// Artefact integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtefactIntegrity {
    pub executable_sha256: String,
    pub package_sha256: Option<String>,
    pub manifest_sha256: Option<String>,
}

/// Create provenance record for a build.
pub fn create_provenance(
    compiler_version: &str,
    source_path: &str,
    source_digest: &str,
    target_profile: TargetProfile,
    dependencies: DependencyManifest,
    build_config: BuildConfig,
    security_policy: SecurityPolicyDecl,
    correctness: CorrectnessRef,
    artefact_integrity: ArtefactIntegrity,
) -> ArtefactProvenance {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let build_config_digest = sha256_hex(&serde_json::to_vec(&build_config).unwrap_or_default());

    ArtefactProvenance {
        schema: "GY-DIA-PROVENANCE/1".to_string(),
        directive: crate::DIRECTIVE_ID.to_string(),
        compiler: CompilerIdentity {
            name: "diablos".to_string(),
            version: compiler_version.to_string(),
            directive: crate::DIRECTIVE_ID.to_string(),
        },
        source: SourceInfo {
            digest: source_digest.to_string(),
            path: source_path.to_string(),
            revision: source_revision(),
        },
        target_profile,
        dependencies,
        build_config_digest,
        security_policy,
        correctness,
        artefact_integrity,
        timestamp,
    }
}

fn source_revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Create a minimal SBOM for the workspace.
pub fn create_sbom() -> DependencyManifest {
    DependencyManifest {
        direct: vec![
            Dependency {
                name: "goya-diablos".to_string(),
                version: crate::COMPILER_VERSION.to_string(),
                source: "workspace".to_string(),
                license: "MIT OR Apache-2.0".to_string(),
                sha256: None,
            },
        ],
        transitive: vec![],
        note: "Direct dependencies only; no vulnerability review performed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_structure() {
        let prov = create_provenance(
            "0.0.1",
            "app.gscene",
            "abc123",
            TargetProfile {
                family: "bsd".to_string(),
                variant: "freebsd".to_string(),
                architecture: "x86_64".to_string(),
                abi: "native".to_string(),
                profile: "desktop".to_string(),
                manifest_digest: "def456".to_string(),
            },
            create_sbom(),
            BuildConfig {
                rustc_version: "1.95.0".to_string(),
                cargo_version: "1.95.0".to_string(),
                target: "x86_64-unknown-freebsd".to_string(),
                profile: "release".to_string(),
                features: vec![],
                env: BTreeMap::new(),
            },
            SecurityPolicyDecl {
                sandbox_type: "capsicum".to_string(),
                filesystem_policy: "least-privilege".to_string(),
                network_policy: "deny-all".to_string(),
                enforceable: true,
            },
            CorrectnessRef {
                inquisition_report_digest: None,
                oracle_passed: false,
                smoke_passed: None,
            },
            ArtefactIntegrity {
                executable_sha256: "exe123".to_string(),
                package_sha256: None,
                manifest_sha256: None,
            },
        );

        assert_eq!(prov.compiler.name, "diablos");
        assert_eq!(prov.directive, "GOYA-DIA-TD-001");
        assert!(prov.timestamp > 0);
    }
}