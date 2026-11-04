//! goya-executable — artifact registry, capability validation, staleness and
//! cost model for Enforced Deterministic Compilation (GY-EDC-001).
//!
//! Contract: [`EXECUTABLE_CONTRACT`] (`GY-EXECUTABLE/1`).
//!
//! Builds on [`goya_determinism`]:
//! - `Artifact` hashes the canonical [`ExecutionProof`] serialisation with a
//!   self-implemented, std-only FNV-1a.
//! - `validate_capability_escalation` enforces invariant #7: no privilege
//!   acquisition beyond the originating process's capability set.
//! - `is_stale` stales artifacts on schema or policy drift (invariant #6
//!   companion).
//! - `worthwhile` is the spec §5 compile-vs-interpret cost model.

use std::collections::HashMap;

use goya_determinism::proofs::ExecutionProof;

mod bytecode;
pub use bytecode::{
    compile_module, compile_verified, execute, Bytecode, CompileError, CompiledProgram,
    ExecutionNode, Instruction, VerifiedCompileError, VmError, BYTECODE_VERSION,
};

/// The executable contract identifier this crate implements.
pub const EXECUTABLE_CONTRACT: &str = "GY-EXECUTABLE/1";

/// FNV-1a (64-bit) over `bytes`, implemented in-crate, std-only.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// A compiled executable artifact.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Artifact {
    /// The execution proof the artifact was compiled from.
    pub proof: ExecutionProof,
    /// FNV-1a content hash over the canonical proof serialisation.
    pub content_hash: u64,
    /// Capabilities the artifact requires (e.g. "network", "filesystem").
    pub capabilities: Vec<String>,
}

impl Artifact {
    /// Build an artifact from a proof, hashing its canonical serialisation.
    pub fn from_proof(proof: ExecutionProof, capabilities: Vec<String>) -> Self {
        let content_hash = fnv1a(proof.canonical().as_bytes());
        Self {
            proof,
            content_hash,
            capabilities,
        }
    }

    /// Build a code-bound artifact whose identity covers both proof metadata
    /// and canonical executable bytes.
    pub fn from_proof_and_code(
        proof: ExecutionProof,
        capabilities: Vec<String>,
        canonical_code: &[u8],
    ) -> Self {
        let canonical_proof = proof.canonical();
        let mut identity = Vec::with_capacity(canonical_proof.len() + canonical_code.len() + 32);
        identity.extend_from_slice(b"GY-EDC-CODE-ARTIFACT/1\0");
        identity.extend_from_slice(&(canonical_proof.len() as u64).to_le_bytes());
        identity.extend_from_slice(canonical_proof.as_bytes());
        identity.extend_from_slice(&(canonical_code.len() as u64).to_le_bytes());
        identity.extend_from_slice(canonical_code);
        let content_hash = fnv1a(&identity);
        Self {
            proof,
            content_hash,
            capabilities,
        }
    }
}

/// Registry of compiled artifacts keyed by content hash. Registration is
/// idempotent: re-registering an identical hash is a no-op, not an error.
#[derive(Clone, Default, Debug)]
pub struct Registry {
    artifacts: HashMap<u64, Artifact>,
}

impl Registry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an artifact. Returns `false` if an artifact with the same
    /// content hash is already registered (idempotent duplicate rejection).
    pub fn register(&mut self, artifact: Artifact) -> bool {
        if self.artifacts.contains_key(&artifact.content_hash) {
            return false;
        }
        self.artifacts.insert(artifact.content_hash, artifact);
        true
    }

    /// Look up an artifact by its content hash.
    pub fn lookup_by_hash(&self, hash: u64) -> Option<&Artifact> {
        self.artifacts.get(&hash)
    }

    /// Number of registered artifacts.
    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }
}

/// Invariant #7: every artifact capability must already be present in the
/// originating process's capability set. Returns `true` when no escalation
/// occurs (all artifact capabilities are covered), `false` when the artifact
/// acquires a privilege the originator never held.
pub fn validate_capability_escalation(origination: &[String], artifact: &Artifact) -> bool {
    artifact
        .capabilities
        .iter()
        .all(|cap| origination.iter().any(|origin| origin == cap))
}

/// Invariant #6 companion: an artifact is stale when its schema or policy
/// version drifts from the current toolchain.
pub fn is_stale(artifact: &Artifact, current_schema: u64, current_policy: u64) -> bool {
    artifact.proof.schema_version != current_schema
        || artifact.proof.policy_version != current_policy
}

/// Spec §5 cost model: compilation is worthwhile when the total compiled cost
/// (one-time compile + per-run compiled cost) is strictly less than the
/// total interpreted cost.
pub fn worthwhile(
    expected_executions: u64,
    compile_cost: u64,
    interp_cost_per_run: u64,
    compiled_cost_per_run: u64,
) -> bool {
    compile_cost + expected_executions * compiled_cost_per_run
        < expected_executions * interp_cost_per_run
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::DeterminismClass;

    fn proof(schema: u64, policy: u64) -> ExecutionProof {
        ExecutionProof {
            process: "scheduler".into(),
            class: DeterminismClass::EffectDeterministic,
            compiler_version: "0.1.0".into(),
            schema_version: schema,
            policy_version: policy,
            target: "x86_64-unknown-linux-gnu".into(),
        }
    }

    fn artifact() -> Artifact {
        Artifact::from_proof(proof(1, 1), vec!["clock".into(), "filesystem".into()])
    }

    #[test]
    fn register_is_idempotent_on_duplicate_hash() {
        let mut registry = Registry::new();
        assert!(registry.register(artifact()));
        let before = registry.len();
        assert!(!registry.register(artifact()));
        assert_eq!(registry.len(), before);
    }

    #[test]
    fn unknown_hash_is_missing() {
        let mut registry = Registry::new();
        registry.register(artifact());
        assert!(registry.lookup_by_hash(0xDEAD_BEEF).is_none());
        assert!(registry
            .lookup_by_hash(fnv1a(proof(1, 1).canonical().as_bytes()))
            .is_some());
    }

    #[test]
    fn capability_escalation_is_detected() {
        let artifact = Artifact::from_proof(proof(1, 1), vec!["clock".into(), "network".into()]);
        // Originator only holds "clock": acquiring "network" is escalation.
        let origin = vec!["clock".to_string()];
        assert!(!validate_capability_escalation(&origin, &artifact));
        // Full coverage passes.
        let full = vec!["clock".to_string(), "network".to_string()];
        assert!(validate_capability_escalation(&full, &artifact));
    }

    #[test]
    fn stale_on_schema_drift() {
        let artifact = artifact();
        assert!(is_stale(&artifact, 2, 1)); // schema drifted
        assert!(!is_stale(&artifact, 1, 1));
    }

    #[test]
    fn stale_on_policy_drift() {
        let artifact = artifact();
        assert!(is_stale(&artifact, 1, 2)); // policy drifted
        assert!(!is_stale(&artifact, 1, 1));
    }

    #[test]
    fn cost_model_matches_spec_example() {
        // Spec §5: 1000 execs, compile cost 50 (scaled x2 → 100),
        // interpreted 1.0 vs compiled 0.08 per run (scaled x2 → 100 vs 8).
        assert!(worthwhile(1000, 100, 100, 8));
        // Interpreted total 100000 vs compiled total 100 + 8000 = 8100.
        // Counter-case: few executions do not pay off the compile cost.
        assert!(!worthwhile(1, 100, 100, 8));
    }
}
