//! ie-gate — Goya neural feature declarations and runtime gate (WP-007
//! seed, S4). Compiles feature declarations to a target capability
//! manifest (IE-P02 "Capability-aware experience") and evaluates them at
//! runtime against an *authenticated* capability descriptor.
//!
//! Gate rules (frozen semantics, IE-P02):
//! - never advertise a feature as available before authenticated capability
//!   checks succeed — the gate only accepts post-authentication
//!   descriptors;
//! - runtime absence preserves UI state and explains remediation: the gate
//!   returns a typed unavailability with a reason, never a bare false;
//! - compile-time impossibility (an empty operator universe) is a build
//!   error, not a runtime surprise.

use ie_contract::messages::CapabilityDescriptor;
use ie_contract::PROTOCOL_VERSION;

/// What the application should do when the feature is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// Feature disabled; UI returns to baseline (PR-04).
    Disable,
    /// Deferred: retry when capability appears (e.g. peer reconnects).
    Defer,
    /// Substituted by a local non-neural path; never silent — the app must
    /// display the substitution (P02 "avoid silent substitution").
    Substitute,
    /// Required: fail explicitly before feature use (PR-04).
    Required,
}

/// Input sensitivity class (PR-05): drives the egress question at gate
/// evaluation time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensitivity {
    /// May leave the device if policy and consent allow.
    Shareable,
    /// Local-only; remote execution is a policy violation even if capable.
    LocalOnly,
}

/// Declarative description of one neural feature (compiled, not hand-run).
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureDecl {
    pub name: String,
    pub required_ops: Vec<String>,
    pub model_sha256: [u8; 32],
    /// Largest single input tensor the feature will submit, in bytes.
    pub max_input_bytes: u64,
    /// Maximum acceptable end-to-end latency in milliseconds.
    pub max_latency_ms: u64,
    pub sensitivity: Sensitivity,
    pub fallback: Fallback,
}

impl FeatureDecl {
    /// Compile-time validation. Empty operator requirements are impossible
    /// to gate — reject at compile time rather than advertising at runtime.
    pub fn compile(&self) -> Result<CompiledFeature, String> {
        if self.name.is_empty() {
            return Err("feature name must not be empty".into());
        }
        if self.required_ops.is_empty() {
            return Err(format!("feature '{}' declares no required ops", self.name));
        }
        if self.max_input_bytes == 0 {
            return Err(format!("feature '{}' declares zero-size input", self.name));
        }
        Ok(CompiledFeature { decl: self.clone() })
    }
}

/// A validated, compiled feature ready for gating.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFeature {
    decl: FeatureDecl,
}

impl CompiledFeature {
    pub fn name(&self) -> &str {
        &self.decl.name
    }

    pub fn fallback(&self) -> Fallback {
        self.decl.fallback
    }
}

/// Target-specific capability manifest: the compiled form of all features
/// for one build target. This is what a Goya application ships.
#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityManifest {
    pub protocol_version: u32,
    pub features: Vec<CompiledFeature>,
}

/// Compile a set of declarations into a target manifest.
pub fn compile_manifest(decls: &[FeatureDecl]) -> Result<CapabilityManifest, String> {
    let mut features = Vec::with_capacity(decls.len());
    for d in decls {
        features.push(d.compile()?);
    }
    Ok(CapabilityManifest { protocol_version: PROTOCOL_VERSION, features })
}

/// Gate verdict with an explicit, presentable reason when unavailable.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// All checks passed against an authenticated descriptor.
    Available { endpoint_id: String },
    /// Provider cannot run this feature's operators (T02-class).
    Unsupported { missing_ops: Vec<String> },
    /// Inputs exceed the provider's declared limits.
    ResourceLimited { reason: String },
    /// Deadline cannot be met: provider gives no latency guarantee below
    /// the feature's requirement — treated as unmet, never assumed.
    LatencyUnmet { required_ms: u64 },
    /// Local-only feature offered a remote endpoint (PR-05).
    PolicyBlocked,
    /// Provider is a different protocol generation.
    VersionMismatch { local: u32, remote: u32 },
}

impl Verdict {
    pub fn is_available(&self) -> bool {
        matches!(self, Verdict::Available { .. })
    }

    /// Human-presentable remediation text (P02: runtime absence explains
    /// remediation; never rely on colour/transient alone).
    pub fn remediation(&self) -> &'static str {
        match self {
            Verdict::Available { .. } => "",
            Verdict::Unsupported { .. } => "this feature needs operations the selected provider does not support; choose another provider or disable the feature",
            Verdict::ResourceLimited { .. } => "the provider's memory limits are too small for this feature; reduce input size or pick a local provider",
            Verdict::LatencyUnmet { .. } => "the provider cannot guarantee the required responsiveness; the feature will be deferred",
            Verdict::PolicyBlocked => "this feature's data is local-only and cannot be sent to a remote provider",
            Verdict::VersionMismatch { .. } => "the provider speaks a different protocol version; update the provider",
        }
    }
}

/// Evaluate one compiled feature against an authenticated descriptor.
/// The descriptor must already be authenticated (loopback models
/// authentication as the Describe step); the gate performs no I/O.
pub fn evaluate(feature: &CompiledFeature, cap: &CapabilityDescriptor) -> Verdict {
    if cap.protocol_version != PROTOCOL_VERSION {
        return Verdict::VersionMismatch { local: PROTOCOL_VERSION, remote: cap.protocol_version };
    }
    let missing: Vec<String> = feature
        .decl
        .required_ops
        .iter()
        .filter(|op| !cap.supported_ops.contains(op))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Verdict::Unsupported { missing_ops: missing };
    }
    if feature.decl.max_input_bytes > cap.max_tensor_bytes {
        return Verdict::ResourceLimited {
            reason: format!(
                "input {} bytes exceeds provider limit {} bytes",
                feature.decl.max_input_bytes, cap.max_tensor_bytes
            ),
        };
    }
    // Latency: a provider that publishes no budget (0) cannot promise the
    // feature's requirement; this is honest absence of evidence, not
    // optimism.
    if cap.expiry_ms > 0 && cap.expiry_ms < feature.decl.max_latency_ms {
        return Verdict::LatencyUnmet { required_ms: feature.decl.max_latency_ms };
    }
    if feature.decl.sensitivity == Sensitivity::LocalOnly && cap.endpoint_id != "ie://local/cpu0"
    {
        return Verdict::PolicyBlocked;
    }
    Verdict::Available { endpoint_id: cap.endpoint_id.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(endpoint: &str, ops: &[&str], max_tensor: u64, expiry_ms: u64) -> CapabilityDescriptor {
        CapabilityDescriptor {
            protocol_version: PROTOCOL_VERSION,
            endpoint_id: endpoint.into(),
            epoch: 1,
            supported_ops: ops.iter().map(|s| s.to_string()).collect(),
            max_tensor_bytes: max_tensor,
            queue_limit: 4,
            expiry_ms,
        }
    }

    fn feature() -> FeatureDecl {
        FeatureDecl {
            name: "photo-classify".into(),
            required_ops: vec!["add".into(), "matmul".into()],
            model_sha256: [7; 32],
            max_input_bytes: 1024,
            max_latency_ms: 200,
            sensitivity: Sensitivity::Shareable,
            fallback: Fallback::Disable,
        }
    }

    /// T11 seed — conditional UI: feature available on a capable peer,
    /// typed unavailability elsewhere.
    #[test]
    fn gate_advertises_only_after_capable_authenticated_peer() {
        let f = feature().compile().unwrap();
        assert!(evaluate(&f, &cap("ie://peer/esp0/0", &["add", "matmul"], 1 << 20, 10_000))
            .is_available());
        let v = evaluate(&f, &cap("ie://peer/esp0/0", &["add"], 1 << 20, 10_000));
        assert_eq!(
            v,
            Verdict::Unsupported { missing_ops: vec!["matmul".into()] }
        );
        assert!(!v.remediation().is_empty());
    }

    #[test]
    fn version_mismatch_is_typed() {
        let f = feature().compile().unwrap();
        let mut c = cap("ie://local/cpu0", &["add", "matmul"], 1 << 20, 10_000);
        c.protocol_version = PROTOCOL_VERSION + 1;
        assert_eq!(
            evaluate(&f, &c),
            Verdict::VersionMismatch { local: PROTOCOL_VERSION, remote: PROTOCOL_VERSION + 1 }
        );
    }

    #[test]
    fn local_only_feature_blocked_on_remote() {
        let mut d = feature();
        d.sensitivity = Sensitivity::LocalOnly;
        let f = d.compile().unwrap();
        assert_eq!(
            evaluate(&f, &cap("ie://peer/esp0/0", &["add", "matmul"], 1 << 20, 10_000)),
            Verdict::PolicyBlocked
        );
        assert!(evaluate(&f, &cap("ie://local/cpu0", &["add", "matmul"], 1 << 20, 10_000))
            .is_available());
    }

    #[test]
    fn resource_and_latency_limits_enforced() {
        let f = feature().compile().unwrap();
        assert!(matches!(
            evaluate(&f, &cap("ie://local/cpu0", &["add", "matmul"], 512, 10_000)),
            Verdict::ResourceLimited { .. }
        ));
        assert!(matches!(
            evaluate(&f, &cap("ie://local/cpu0", &["add", "matmul"], 1 << 20, 100)),
            Verdict::LatencyUnmet { required_ms: 200 }
        ));
    }

    #[test]
    fn compile_time_impossibility_rejected() {
        let mut d = feature();
        d.required_ops.clear();
        assert!(d.compile().is_err());
        let mut d = feature();
        d.name.clear();
        assert!(d.compile().is_err());
    }

    #[test]
    fn manifest_compiles_all_or_errors() {
        let ok = compile_manifest(&[feature(), feature()]).unwrap();
        assert_eq!(ok.features.len(), 2);
        assert_eq!(ok.protocol_version, PROTOCOL_VERSION);
        let mut bad = feature();
        bad.name = String::new();
        assert!(compile_manifest(&[feature(), bad]).is_err());
    }
}
