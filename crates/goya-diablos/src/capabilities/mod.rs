//! Capability discovery and resolution for Diablos.

pub mod manifest;
pub mod registry;
pub mod discovery;

pub use manifest::{TargetManifest, TargetInfo, GraphicsConfig, SecurityConfig, ExecutionConfig, ResourcesConfig, DistributionConfig};
pub use registry::{ApplicationRequirements, ResolvedCapabilities, SecurityPolicy, CapabilityRegistry, detect_host_profile};
pub use discovery::{DiscoveredCapability, CapabilitySource, discover_capabilities, has_capability, capability_version};