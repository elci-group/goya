//! Capability registry with precedence resolution for Diablos.

use crate::capabilities::manifest::TargetManifest;
use crate::Fault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Application requirements extracted from .gscene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationRequirements {
    /// Required filesystem access paths
    pub filesystem: Vec<String>,
    /// Required network access (hosts, ports)
    pub network: Vec<String>,
    /// Required subprocess execution
    pub subprocess: bool,
    /// Required device access
    pub devices: Vec<String>,
    /// Required interprocess communication
    pub ipc: bool,
    /// Required graphics capabilities
    pub graphics: GraphicsRequirements,
    /// Required neural capabilities
    pub neural: NeuralRequirements,
    /// Explicit Caprichio requests
    pub caprichios: Vec<String>,
}

/// Graphics requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphicsRequirements {
    /// Requires hardware acceleration
    pub hardware_acceleration: bool,
    /// Requires specific GPU vendor features
    pub gpu_features: Vec<String>,
    /// Maximum frame latency budget (ms)
    pub max_frame_latency_ms: Option<u32>,
}

/// Neural requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NeuralRequirements {
    /// Requires local neural execution (Iberia)
    pub local: bool,
    /// Requires distributed neural execution (España)
    pub distributed: bool,
    /// Required model capabilities
    pub models: Vec<String>,
}

/// A negotiated capability (simplified for Phase 0).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiatedCapability {
    pub id: String,
    pub recipe: String,
    pub artifact: serde_json::Value,
}

/// Resolved capabilities after precedence resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedCapabilities {
    /// Final target manifest with all resolved values
    pub manifest: TargetManifest,
    /// Negotiated Caprichios
    pub caprichios: Vec<NegotiatedCapability>,
    /// Security policy synthesised from requirements
    pub security_policy: SecurityPolicy,
    /// Graphics backend selected
    pub graphics_backend: String,
    /// Whether each requirement was satisfied
    pub satisfaction: RequirementSatisfaction,
}

/// Security policy derived from requirements and target capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Filesystem access rules
    pub filesystem: FilesystemPolicy,
    /// Network access rules
    pub network: NetworkPolicy,
    /// Process execution rules
    pub subprocess: SubprocessPolicy,
    /// Device access rules
    pub devices: DevicePolicy,
    /// IPC rules
    pub ipc: IpcPolicy,
    /// Native sandbox configuration
    pub native_sandbox: NativeSandboxConfig,
}

/// Filesystem access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemPolicy {
    /// Allowed read paths
    pub read: Vec<String>,
    /// Allowed write paths
    pub write: Vec<String>,
    /// Denied paths (override allows)
    pub deny: Vec<String>,
}

/// Network access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Allowed outbound hosts
    pub outbound: Vec<String>,
    /// Allowed inbound ports
    pub inbound: Vec<u16>,
    /// Deny all network if true
    pub deny_all: bool,
}

/// Subprocess execution policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessPolicy {
    /// Allowed executable paths
    pub allowed: Vec<String>,
    /// Deny all subprocess if true
    pub deny_all: bool,
}

/// Device access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePolicy {
    /// Allowed device paths
    pub allowed: Vec<String>,
    /// Deny all devices if true
    pub deny_all: bool,
}

/// IPC policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcPolicy {
    /// Allowed socket paths
    pub sockets: Vec<String>,
    /// Allow shared memory
    pub shared_memory: bool,
    /// Deny all IPC if true
    pub deny_all: bool,
}

/// Native sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeSandboxConfig {
    /// Sandbox type: capsicum, pledge, kauth, none
    pub sandbox_type: String,
    /// Capsicum rights (FreeBSD)
    pub capsicum_rights: Option<CapsicumRights>,
    /// Pledge promises (OpenBSD)
    pub pledge_promises: Option<Vec<String>>,
    /// Unveil paths (OpenBSD)
    pub unveil_paths: Option<Vec<UnveilPath>>,
    /// Kauth listeners (NetBSD)
    pub kauth_listeners: Option<Vec<String>>,
}

/// Capsicum capability rights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsicumRights {
    /// File descriptor rights
    pub fd_rights: u64,
    /// Capability rights
    pub cap_rights: u64,
}

/// Unveil path with permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnveilPath {
    pub path: String,
    pub permissions: String, // "r", "w", "x", "rw", "rx", "rwx"
}

/// Requirement satisfaction tracking.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequirementSatisfaction {
    /// Filesystem requirements satisfied
    pub filesystem: bool,
    /// Network requirements satisfied
    pub network: bool,
    /// Subprocess requirements satisfied
    pub subprocess: bool,
    /// Device requirements satisfied
    pub devices: bool,
    /// IPC requirements satisfied
    pub ipc: bool,
    /// Graphics requirements satisfied
    pub graphics: bool,
    /// Neural requirements satisfied
    pub neural: bool,
    /// Unsatisfied requirements with reasons
    pub unsatisfied: Vec<UnsatisfiedRequirement>,
}

/// An unsatisfied requirement with reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsatisfiedRequirement {
    pub requirement: String,
    pub reason: String,
    /// Whether this is a mandatory correctness requirement (fail-closed)
    pub mandatory: bool,
}

/// Capability registry for resolving target capabilities with precedence.
pub struct CapabilityRegistry {
    /// Base target manifest
    manifest: TargetManifest,
    /// Application requirements
    requirements: ApplicationRequirements,
    /// Host profile for Caprichio negotiation
    host_profile: HostProfile,
}

impl CapabilityRegistry {
    /// Create a new capability registry.
    pub fn new(manifest: TargetManifest, requirements: ApplicationRequirements, host_profile: HostProfile) -> Self {
        Self {
            manifest,
            requirements,
            host_profile,
        }
    }

    /// Resolve capabilities according to precedence order (per GOYA-DIA-TD-001 §6.2):
    /// 1. Explicit application requirements
    /// 2. Mandatory target correctness constraints
    /// 3. Target operating system capabilities
    /// 4. Hardware capabilities
    /// 5. Applicable Caprichios
    /// 6. Deployment profile
    /// 7. Optional optimisation preferences
    pub fn resolve(mut self) -> Result<ResolvedCapabilities, Fault> {
        let mut manifest = self.manifest.clone();
        let mut satisfaction = RequirementSatisfaction::default();
        let mut unsatisfied = Vec::new();

        // 1. Apply explicit application requirements
        self.apply_application_requirements(&mut manifest, &mut satisfaction, &mut unsatisfied)?;

        // 2. Apply mandatory target correctness constraints
        self.apply_correctness_constraints(&mut manifest, &mut satisfaction, &mut unsatisfied)?;

        // 3. Apply target OS capabilities (already in manifest defaults)

        // 4. Apply hardware capabilities (from host profile)

        // 5. Negotiate Caprichios
        let caprichios = self.negotiate_caprichios(&manifest)?;

        // 6. Apply deployment profile adjustments
        self.apply_profile(&mut manifest);

        // 7. Apply optional optimisation preferences
        self.apply_optimisations(&mut manifest);

        // Synthesise security policy
        let security_policy = self.synthesise_security_policy(&manifest, &satisfaction)?;

        // Select graphics backend
        let graphics_backend = self.select_graphics_backend(&manifest, &caprichios);

        // Check for mandatory unsatisfied requirements (fail-closed)
        for unsat in &unsatisfied {
            if unsat.mandatory {
                return Err(Fault::new(
                    crate::fault::exit::ADMISSION_FAILED,
                    "MANDATORY_REQUIREMENT_UNSATISFIED",
                    format!("Mandatory requirement '{}' cannot be satisfied: {}", unsat.requirement, unsat.reason),
                ));
            }
        }

        satisfaction.unsatisfied = unsatisfied;

        Ok(ResolvedCapabilities {
            manifest,
            caprichios,
            security_policy,
            graphics_backend,
            satisfaction,
        })
    }

    /// Apply explicit application requirements to manifest.
    fn apply_application_requirements(
        &self,
        manifest: &mut TargetManifest,
        satisfaction: &mut RequirementSatisfaction,
        unsatisfied: &mut Vec<UnsatisfiedRequirement>,
    ) -> Result<(), Fault> {
        let req = &self.requirements;

        // Filesystem
        if !req.filesystem.is_empty() {
            manifest.extra.insert("app_filesystem_read".to_string(), serde_json::to_value(&req.filesystem).unwrap());
            satisfaction.filesystem = true;
        } else {
            satisfaction.filesystem = true; // No requirement = satisfied
        }

        // Network
        if !req.network.is_empty() || req.subprocess {
            manifest.extra.insert("app_network".to_string(), serde_json::to_value(&req.network).unwrap());
            manifest.security.native_sandbox = "restricted".to_string(); // Require sandbox if network needed
            satisfaction.network = true;
        } else {
            satisfaction.network = true;
        }

        // Subprocess
        if req.subprocess {
            manifest.security.policy = "restricted".to_string();
            satisfaction.subprocess = true;
        } else {
            satisfaction.subprocess = true;
        }

        // Devices
        if !req.devices.is_empty() {
            manifest.extra.insert("app_devices".to_string(), serde_json::to_value(&req.devices).unwrap());
            satisfaction.devices = true;
        } else {
            satisfaction.devices = true;
        }

        // IPC
        if req.ipc {
            manifest.extra.insert("app_ipc".to_string(), serde_json::Value::Bool(true));
            satisfaction.ipc = true;
        } else {
            satisfaction.ipc = true;
        }

        // Graphics
        if req.graphics.hardware_acceleration {
            manifest.graphics.backend = "drm".to_string();
            satisfaction.graphics = true;
        } else {
            satisfaction.graphics = true;
        }

        // Neural
        if req.neural.local || req.neural.distributed {
            manifest.extra.insert("app_neural_local".to_string(), serde_json::Value::Bool(req.neural.local));
            manifest.extra.insert("app_neural_distributed".to_string(), serde_json::Value::Bool(req.neural.distributed));
            satisfaction.neural = true;
        } else {
            satisfaction.neural = true;
        }

        // Explicit Caprichios
        if !req.caprichios.is_empty() {
            manifest.caprichios.extend(req.caprichios.clone());
        }

        Ok(())
    }

    /// Apply mandatory target correctness constraints.
    fn apply_correctness_constraints(
        &self,
        manifest: &mut TargetManifest,
        satisfaction: &mut RequirementSatisfaction,
        unsatisfied: &mut Vec<UnsatisfiedRequirement>,
    ) -> Result<(), Fault> {
        // Security: If target requires sandbox but none available, fail
        if manifest.security.native_sandbox == "auto" {
            // Auto-select based on variant
            manifest.security.native_sandbox = match manifest.target.variant.as_str() {
                "freebsd" | "ghostbsd" | "midnightbsd" | "hardenedbsd" => "capsicum".to_string(),
                "openbsd" => "pledge".to_string(),
                "netbsd" => "kauth".to_string(),
                "dragonfly" => "native".to_string(),
                _ => "none".to_string(),
            };
        }

        // Graphics: If hardware acceleration required but not available, fail-closed
        if self.requirements.graphics.hardware_acceleration && !self.host_has_gpu() {
            unsatisfied.push(UnsatisfiedRequirement {
                requirement: "hardware_acceleration".to_string(),
                reason: "No GPU available on target".to_string(),
                mandatory: true, // Fail-closed per directive §9.4
            });
            satisfaction.graphics = false;
        }

        // Neural: Must have Iberia/España available
        if self.requirements.neural.local && !self.host_has_iberia() {
            unsatisfied.push(UnsatisfiedRequirement {
                requirement: "neural_local".to_string(),
                reason: "Iberia not available on target".to_string(),
                mandatory: true,
            });
            satisfaction.neural = false;
        }
        if self.requirements.neural.distributed && !self.host_has_espana() {
            unsatisfied.push(UnsatisfiedRequirement {
                requirement: "neural_distributed".to_string(),
                reason: "España not available on target".to_string(),
                mandatory: true,
            });
            satisfaction.neural = false;
        }

        Ok(())
    }

    /// Negotiate Caprichios using goya-caprichios registry (stubbed for Phase 0).
    fn negotiate_caprichios(&self, _manifest: &TargetManifest) -> Result<Vec<NegotiatedCapability>, Fault> {
        // Caprichio negotiation not implemented in this cycle
        Ok(Vec::new())
    }

    /// Apply deployment profile adjustments.
    fn apply_profile(&self, manifest: &mut TargetManifest) {
        match manifest.target.profile.as_str() {
            "constrained" | "embedded" => {
                manifest.resources.optimisation = "minimal".to_string();
                manifest.graphics.software_fallback = true;
                manifest.graphics.backend = "software".to_string();
            }
            "server" | "headless" => {
                manifest.graphics.backend = "software".to_string();
                manifest.graphics.gvi = false;
            }
            "desktop" => {
                // Keep defaults
            }
            _ => {}
        }
    }

    /// Apply optional optimisation preferences.
    fn apply_optimisations(&self, manifest: &mut TargetManifest) {
        // Optimisation preferences never override correctness/security (per directive §6.2)
        // Only apply if not already set by higher precedence
        if manifest.resources.optimisation == "adaptive" {
            // Could adjust based on host profile hardware info
        }
    }

    /// Synthesise native security policy from requirements and target capabilities.
    fn synthesise_security_policy(
        &self,
        manifest: &TargetManifest,
        satisfaction: &RequirementSatisfaction,
    ) -> Result<SecurityPolicy, Fault> {
        let req = &self.requirements;

        let filesystem = FilesystemPolicy {
            read: req.filesystem.clone(),
            write: req.filesystem.iter().filter(|p| !p.starts_with("/etc") && !p.starts_with("/usr")).cloned().collect(),
            deny: vec!["/etc".to_string(), "/usr".to_string(), "/boot".to_string()],
        };

        let network = NetworkPolicy {
            outbound: req.network.clone(),
            inbound: Vec::new(),
            deny_all: req.network.is_empty() && !req.subprocess,
        };

        let subprocess = SubprocessPolicy {
            allowed: Vec::new(),
            deny_all: !req.subprocess,
        };

        let devices = DevicePolicy {
            allowed: req.devices.clone(),
            deny_all: req.devices.is_empty(),
        };

        let ipc = IpcPolicy {
            sockets: Vec::new(),
            shared_memory: req.ipc,
            deny_all: !req.ipc,
        };

        let native_sandbox = NativeSandboxConfig {
            sandbox_type: manifest.security.native_sandbox.clone(),
            capsicum_rights: if manifest.security.native_sandbox == "capsicum" {
                Some(CapsicumRights {
                    fd_rights: 0, // Would be populated from requirements
                    cap_rights: 0,
                })
            } else {
                None
            },
            pledge_promises: if manifest.security.native_sandbox == "pledge" {
                Some(vec!["stdio".to_string(), "rpath".to_string(), "wpath".to_string()])
            } else {
                None
            },
            unveil_paths: if manifest.security.native_sandbox == "pledge" {
                Some(req.filesystem.iter().map(|p| UnveilPath { path: p.clone(), permissions: "rw".to_string() }).collect())
            } else {
                None
            },
            kauth_listeners: if manifest.security.native_sandbox == "kauth" {
                Some(vec!["filesystem".to_string(), "network".to_string()])
            } else {
                None
            },
        };

        Ok(SecurityPolicy {
            filesystem,
            network,
            subprocess,
            devices,
            ipc,
            native_sandbox,
        })
    }

    /// Select graphics backend based on capabilities and Caprichios.
    fn select_graphics_backend(&self, manifest: &TargetManifest, caprichios: &[NegotiatedCapability]) -> String {
        // Check for DRM Caprichio
        for cap in caprichios {
            if cap.recipe.contains("drm") || cap.recipe.contains("kms") {
                return "drm".to_string();
            }
        }

        // Fall back to manifest config
        match manifest.graphics.backend.as_str() {
            "auto" => {
                if self.host_has_gpu() {
                    "drm".to_string()
                } else {
                    "software".to_string()
                }
            }
            b => b.to_string(),
        }
    }

    /// Check if host has GPU (stub - would query actual hardware).
    fn host_has_gpu(&self) -> bool {
        // In real implementation, would query DRM devices, PCI, etc.
        self.host_profile.graphics_runtime == GraphicsRuntime::Accelerated
    }

    /// Check if host has Iberia (stub).
    fn host_has_iberia(&self) -> bool {
        self.host_profile.neural_runtime == NeuralRuntime::Local
    }

    /// Check if host has España (stub).
    fn host_has_espana(&self) -> bool {
        self.host_profile.neural_runtime == NeuralRuntime::Distributed
    }
}

/// Host profile for capability detection (simplified for Phase 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostProfile {
    pub graphics_runtime: GraphicsRuntime,
    pub neural_runtime: NeuralRuntime,
}

/// Graphics runtime capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsRuntime {
    Software,
    Accelerated,
}

/// Neural runtime capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeuralRuntime {
    None,
    Local,
    Distributed,
}

/// Create a HostProfile for the current machine (stub for Phase 0).
pub fn detect_host_profile() -> HostProfile {
    HostProfile {
        graphics_runtime: GraphicsRuntime::Software,
        neural_runtime: NeuralRuntime::None,
    }
}

impl Default for ApplicationRequirements {
    fn default() -> Self {
        Self {
            filesystem: Vec::new(),
            network: Vec::new(),
            subprocess: false,
            devices: Vec::new(),
            ipc: false,
            graphics: GraphicsRequirements::default(),
            neural: NeuralRequirements::default(),
            caprichios: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_basic() {
        let manifest = TargetManifest::freebsd_desktop("x86_64", "14.2");
        let requirements = ApplicationRequirements::default();
        let host_profile = detect_host_profile();

        let registry = CapabilityRegistry::new(manifest, requirements, host_profile);
        let resolved = registry.resolve().expect("resolve should succeed");

        assert_eq!(resolved.manifest.target.variant, "freebsd");
        assert!(resolved.satisfaction.filesystem);
        assert!(resolved.satisfaction.network);
    }

    #[test]
    fn registry_fail_closed_on_mandatory_gpu() {
        let manifest = TargetManifest::freebsd_desktop("x86_64", "14.2");
        let mut requirements = ApplicationRequirements::default();
        requirements.graphics.hardware_acceleration = true;
        // host_profile with no GPU
        let mut host_profile = detect_host_profile();
        host_profile.graphics_runtime = GraphicsRuntime::Software;

        let registry = CapabilityRegistry::new(manifest, requirements, host_profile);
        let result = registry.resolve();

        assert!(result.is_err());
        let fault = result.unwrap_err();
        assert_eq!(fault.code, "MANDATORY_REQUIREMENT_UNSATISFIED");
    }
}