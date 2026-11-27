//! Target capability manifest for Diablos.

use crate::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Graphics backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GraphicsConfig {
    /// Rendering backend: auto, drm, wayland, x11, software
    pub backend: String,
    /// Whether software fallback is enabled
    pub software_fallback: bool,
    /// Whether GVI (Graphical Vector Inventory) is enabled
    pub gvi: bool,
}

/// Security policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SecurityConfig {
    /// Security policy: least-privilege, restricted, unrestricted
    pub policy: String,
    /// Native sandbox: auto, capsicum, pledge, kauth, none
    pub native_sandbox: String,
}

/// Execution environment configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExecutionConfig {
    /// Event backend: kqueue, epoll, poll
    pub event_backend: String,
    /// Native threads support
    pub native_threads: bool,
}

/// Resource optimisation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResourcesConfig {
    /// Optimisation strategy: adaptive, minimal, maximum
    pub optimisation: String,
    /// Residency policy: auto, packed, expanded
    pub residency: String,
}

/// Distribution/packaging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DistributionConfig {
    /// Package format: native, pkg, pkg_add, pkgsrc, archive
    pub format: String,
}

/// Complete target capability manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetManifest {
    /// Target metadata
    pub target: TargetInfo,
    /// Graphics configuration
    pub graphics: GraphicsConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Execution configuration
    pub execution: ExecutionConfig,
    /// Resources configuration
    pub resources: ResourcesConfig,
    /// Distribution configuration
    pub distribution: DistributionConfig,
    /// Caprichio optimisations applicable to this target
    #[serde(default)]
    pub caprichios: Vec<String>,
    /// Additional target-specific properties
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Target identifying information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    /// OS family: "bsd"
    pub family: String,
    /// OS variant: "freebsd", "openbsd", "netbsd", "dragonfly", "ghostbsd", "midnightbsd", "hardenedbsd"
    pub variant: String,
    /// Processor architecture: "x86_64", "aarch64", "i386", "armv7", "riscv64", "powerpc", "powerpc64", "mips", "sparc64"
    pub architecture: String,
    /// ABI: "native", "gnu", "musl"
    pub abi: String,
    /// Deployment profile: "desktop", "server", "embedded", "headless", "constrained"
    pub profile: String,
    /// OS version (e.g., "14.2", "7.6", "10.1")
    pub version: String,
}

impl TargetManifest {
    /// Create a new TargetManifest with defaults for a FreeBSD desktop target.
    pub fn freebsd_desktop(arch: &str, version: &str) -> Self {
        Self {
            target: TargetInfo {
                family: "bsd".to_string(),
                variant: "freebsd".to_string(),
                architecture: arch.to_string(),
                abi: "native".to_string(),
                profile: "desktop".to_string(),
                version: version.to_string(),
            },
            graphics: GraphicsConfig {
                backend: "auto".to_string(),
                software_fallback: true,
                gvi: true,
            },
            security: SecurityConfig {
                policy: "least-privilege".to_string(),
                native_sandbox: "capsicum".to_string(),
            },
            execution: ExecutionConfig {
                event_backend: "kqueue".to_string(),
                native_threads: true,
            },
            resources: ResourcesConfig {
                optimisation: "adaptive".to_string(),
                residency: "auto".to_string(),
            },
            distribution: DistributionConfig {
                format: "pkg".to_string(),
            },
            caprichios: vec![
                "bsd-kqueue".to_string(),
                "bsd-capsicum".to_string(),
                "freebsd-jail".to_string(),
                "bsd-drm-kms".to_string(),
            ],
            extra: BTreeMap::new(),
        }
    }

    /// Create a new TargetManifest with defaults for an OpenBSD target.
    pub fn openbsd(arch: &str, version: &str) -> Self {
        Self {
            target: TargetInfo {
                family: "bsd".to_string(),
                variant: "openbsd".to_string(),
                architecture: arch.to_string(),
                abi: "native".to_string(),
                profile: "desktop".to_string(),
                version: version.to_string(),
            },
            graphics: GraphicsConfig {
                backend: "auto".to_string(),
                software_fallback: true,
                gvi: true,
            },
            security: SecurityConfig {
                policy: "least-privilege".to_string(),
                native_sandbox: "pledge".to_string(),
            },
            execution: ExecutionConfig {
                event_backend: "kqueue".to_string(),
                native_threads: true,
            },
            resources: ResourcesConfig {
                optimisation: "adaptive".to_string(),
                residency: "auto".to_string(),
            },
            distribution: DistributionConfig {
                format: "pkg_add".to_string(),
            },
            caprichios: vec![
                "bsd-kqueue".to_string(),
                "openbsd-pledge".to_string(),
                "openbsd-unveil".to_string(),
                "openbsd-w-x".to_string(),
            ],
            extra: BTreeMap::new(),
        }
    }

    /// Create a new TargetManifest with defaults for a NetBSD target.
    pub fn netbsd(arch: &str, version: &str) -> Self {
        Self {
            target: TargetInfo {
                family: "bsd".to_string(),
                variant: "netbsd".to_string(),
                architecture: arch.to_string(),
                abi: "native".to_string(),
                profile: "desktop".to_string(),
                version: version.to_string(),
            },
            graphics: GraphicsConfig {
                backend: "auto".to_string(),
                software_fallback: true,
                gvi: true,
            },
            security: SecurityConfig {
                policy: "least-privilege".to_string(),
                native_sandbox: "kauth".to_string(),
            },
            execution: ExecutionConfig {
                event_backend: "kqueue".to_string(),
                native_threads: true,
            },
            resources: ResourcesConfig {
                optimisation: "adaptive".to_string(),
                residency: "auto".to_string(),
            },
            distribution: DistributionConfig {
                format: "pkgsrc".to_string(),
            },
            caprichios: vec![
                "bsd-kqueue".to_string(),
                "netbsd-kauth".to_string(),
                "netbsd-veriexec".to_string(),
            ],
            extra: BTreeMap::new(),
        }
    }

    /// Create a new TargetManifest with defaults for a DragonFly BSD target.
    pub fn dragonfly(arch: &str, version: &str) -> Self {
        Self {
            target: TargetInfo {
                family: "bsd".to_string(),
                variant: "dragonfly".to_string(),
                architecture: arch.to_string(),
                abi: "native".to_string(),
                profile: "desktop".to_string(),
                version: version.to_string(),
            },
            graphics: GraphicsConfig {
                backend: "auto".to_string(),
                software_fallback: true,
                gvi: true,
            },
            security: SecurityConfig {
                policy: "least-privilege".to_string(),
                native_sandbox: "native".to_string(),
            },
            execution: ExecutionConfig {
                event_backend: "kqueue".to_string(),
                native_threads: true,
            },
            resources: ResourcesConfig {
                optimisation: "adaptive".to_string(),
                residency: "auto".to_string(),
            },
            distribution: DistributionConfig {
                format: "native".to_string(),
            },
            caprichios: vec![
                "bsd-kqueue".to_string(),
                "dragonfly-lwkt".to_string(),
                "dragonfly-hammer2".to_string(),
            ],
            extra: BTreeMap::new(),
        }
    }

    /// Compute a SHA-256 digest of the manifest for reproducibility.
    pub fn digest(&self) -> String {
        let json = serde_json::to_vec(self).expect("TargetManifest serializes");
        sha256_hex(&json)
    }

    /// Get the target triple string (e.g., "x86_64-unknown-freebsd").
    pub fn triple(&self) -> String {
        format!("{}-unknown-{}", self.target.architecture, self.target.variant)
    }

    /// Get the profile string (e.g., "diablo.free.interactive/0.0.1").
    pub fn profile(&self) -> String {
        format!("diablo.{}.{}/0.0.1", self.target.variant, self.target.profile)
    }
}

impl Default for TargetManifest {
    fn default() -> Self {
        Self::freebsd_desktop("x86_64", "14.2")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freebsd_manifest_defaults() {
        let m = TargetManifest::freebsd_desktop("x86_64", "14.2");
        assert_eq!(m.target.variant, "freebsd");
        assert_eq!(m.target.architecture, "x86_64");
        assert_eq!(m.target.profile, "desktop");
        assert_eq!(m.security.native_sandbox, "capsicum");
        assert_eq!(m.distribution.format, "pkg");
        assert!(m.caprichios.contains(&"bsd-capsicum".to_string()));
    }

    #[test]
    fn openbsd_manifest_defaults() {
        let m = TargetManifest::openbsd("x86_64", "7.6");
        assert_eq!(m.target.variant, "openbsd");
        assert_eq!(m.security.native_sandbox, "pledge");
        assert_eq!(m.distribution.format, "pkg_add");
        assert!(m.caprichios.contains(&"openbsd-pledge".to_string()));
    }

    #[test]
    fn netbsd_manifest_defaults() {
        let m = TargetManifest::netbsd("aarch64", "10.1");
        assert_eq!(m.target.variant, "netbsd");
        assert_eq!(m.target.architecture, "aarch64");
        assert_eq!(m.security.native_sandbox, "kauth");
        assert_eq!(m.distribution.format, "pkgsrc");
    }

    #[test]
    fn dragonfly_manifest_defaults() {
        let m = TargetManifest::dragonfly("x86_64", "6.4");
        assert_eq!(m.target.variant, "dragonfly");
        assert!(m.caprichios.contains(&"dragonfly-lwkt".to_string()));
    }

    #[test]
    fn manifest_digest_deterministic() {
        let m1 = TargetManifest::freebsd_desktop("x86_64", "14.2");
        let m2 = TargetManifest::freebsd_desktop("x86_64", "14.2");
        assert_eq!(m1.digest(), m2.digest());
    }

    #[test]
    fn triple_format() {
        let m = TargetManifest::freebsd_desktop("x86_64", "14.2");
        assert_eq!(m.triple(), "x86_64-unknown-freebsd");
    }

    #[test]
    fn profile_format() {
        let m = TargetManifest::freebsd_desktop("x86_64", "14.2");
        assert_eq!(m.profile(), "diablo.freebsd.desktop/0.0.1");
    }
}