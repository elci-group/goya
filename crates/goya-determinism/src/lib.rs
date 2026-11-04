//! goya-determinism — classification and proof machinery for
//! Enforced Deterministic Compilation (GY-EDC-001).
//!
//! Contract: [`DETERMINISM_CONTRACT`] (`GY-DETERMINISM/1`).
//!
//! Spec §2 defines four determinism classes. The classifier in this crate
//! enforces the claim-vs-reality invariants:
//!
//! - Invariant #8: a `Pure` (D0) claim with any declared effect is rejected
//!   (`RejectReason::UnresolvedClaim`).
//! - D1 requires an explicit state snapshot contract flag.
//! - D2 must declare every effect it performs; undisclosed effects are
//!   rejected with [`RejectReason::UndisclosedEffect`].
//! - `Nondeterministic` processes are never compiled
//!   ([`RejectReason::Nondeterministic`]).
//!
//! `ExecutionProof` (in [`proofs`]) carries the compiler, schema, policy and
//! target identity required by invariant #5.

use std::fmt;

/// The determinism contract identifier this crate implements.
pub const DETERMINISM_CONTRACT: &str = "GY-DETERMINISM/1";

/// Spec §2 effect kinds a process may declare.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EffectKind {
    /// Outbound network I/O.
    Network,
    /// Filesystem reads/writes.
    FileSystem,
    /// Access to wall-clock or monotonic time.
    Clock,
    /// Entropy-backed randomness.
    Random,
    /// Unmanaged mutable global state.
    GlobalMutable,
}

impl EffectKind {
    /// Stable short name used in canonical serialisations.
    pub fn as_str(self) -> &'static str {
        match self {
            EffectKind::Network => "network",
            EffectKind::FileSystem => "filesystem",
            EffectKind::Clock => "clock",
            EffectKind::Random => "random",
            EffectKind::GlobalMutable => "global_mutable",
        }
    }
}

impl fmt::Display for EffectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Spec §2 determinism classes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeterminismClass {
    /// D0 — pure: no effects at all.
    Pure,
    /// D1 — state-deterministic: effects on private state replayed via a
    /// snapshot contract.
    StateDeterministic,
    /// D2 — effect-deterministic: declared effects are scheduled so the
    /// observable result is deterministic.
    EffectDeterministic,
    /// N — nondeterministic; never compiled.
    Nondeterministic,
}

impl DeterminismClass {
    /// Stable short name used in canonical serialisations.
    pub fn as_str(self) -> &'static str {
        match self {
            DeterminismClass::Pure => "D0",
            DeterminismClass::StateDeterministic => "D1",
            DeterminismClass::EffectDeterministic => "D2",
            DeterminismClass::Nondeterministic => "N",
        }
    }
}

impl fmt::Display for DeterminismClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A process declaration submitted for classification.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProcessDecl {
    /// Process name.
    pub name: String,
    /// The determinism class the author claims.
    pub claims: DeterminismClass,
    /// Effects the process declares it may perform.
    pub declared_effects: Vec<EffectKind>,
    /// Whether an explicit state snapshot contract is in force. Required for
    /// a D1 claim.
    pub state_contract: bool,
}

impl ProcessDecl {
    /// Convenience constructor for a decl with no state contract.
    pub fn new(
        name: impl Into<String>,
        claims: DeterminismClass,
        declared_effects: Vec<EffectKind>,
    ) -> Self {
        Self {
            name: name.into(),
            claims,
            declared_effects,
            state_contract: false,
        }
    }
}

/// Reason a declaration was rejected by the classifier.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RejectReason {
    /// Invariant #8: a Pure (D0) claim cannot carry any declared effect.
    UnresolvedClaim,
    /// A D1 claim without an explicit state snapshot contract flag.
    MissingStateContract,
    /// A D2 process performed an effect class it did not declare.
    UndisclosedEffect(EffectKind),
    /// Class N is never compiled.
    Nondeterministic,
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RejectReason::UnresolvedClaim => {
                write!(
                    f,
                    "unresolved claim: Pure (D0) processes must declare no effects"
                )
            }
            RejectReason::MissingStateContract => {
                write!(
                    f,
                    "StateDeterministic (D1) claim requires an explicit state snapshot contract"
                )
            }
            RejectReason::UndisclosedEffect(kind) => {
                write!(f, "undisclosed effect: {kind} was not declared")
            }
            RejectReason::Nondeterministic => {
                write!(f, "nondeterministic processes are never compiled")
            }
        }
    }
}

/// The successful outcome of classification: a verified declaration.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Verified {
    /// The process that was verified.
    pub process: String,
    /// The determinism class it was verified for.
    pub class: DeterminismClass,
    /// The declared effects admitted by the verifier (empty for D0/D1).
    pub declared_effects: Vec<EffectKind>,
}

/// Classify a process declaration per spec §2, enforcing the claim-vs-reality
/// invariants. Returns the [`Verified`] declaration or a [`RejectReason`].
pub fn classify(decl: &ProcessDecl) -> Result<Verified, RejectReason> {
    match decl.claims {
        DeterminismClass::Pure => {
            // Invariant #8: purity claim with any declared effect is unresolved.
            if decl.declared_effects.is_empty() {
                Ok(Verified {
                    process: decl.name.clone(),
                    class: DeterminismClass::Pure,
                    declared_effects: Vec::new(),
                })
            } else {
                Err(RejectReason::UnresolvedClaim)
            }
        }
        DeterminismClass::StateDeterministic => {
            if !decl.state_contract {
                return Err(RejectReason::MissingStateContract);
            }
            Ok(Verified {
                process: decl.name.clone(),
                class: DeterminismClass::StateDeterministic,
                declared_effects: Vec::new(),
            })
        }
        DeterminismClass::EffectDeterministic => {
            if decl.declared_effects.is_empty() {
                return Err(RejectReason::UndisclosedEffect(EffectKind::GlobalMutable));
            }
            Ok(Verified {
                process: decl.name.clone(),
                class: DeterminismClass::EffectDeterministic,
                declared_effects: decl.declared_effects.clone(),
            })
        }
        DeterminismClass::Nondeterministic => Err(RejectReason::Nondeterministic),
    }
}

pub mod proofs {
    //! Execution proofs: the identity record attached to every compiled
    //! artifact (invariant #5).

    use crate::{DeterminismClass, DETERMINISM_CONTRACT};

    /// Proof that a process was compiled under a specific compiler, schema,
    /// policy and target. Identity includes all four versions.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct ExecutionProof {
        /// Name of the verified process.
        pub process: String,
        /// Determinism class verified at compile time.
        pub class: DeterminismClass,
        /// Version of the compiler that produced the artifact.
        pub compiler_version: String,
        /// Version of the compilation schema.
        pub schema_version: u64,
        /// Version of the compilation policy.
        pub policy_version: u64,
        /// Compilation target triple.
        pub target: String,
    }

    impl ExecutionProof {
        /// Contract identifier string (`GY-DETERMINISM/1`).
        pub fn contract(&self) -> &'static str {
            DETERMINISM_CONTRACT
        }

        /// Canonical, line-based serialisation of the proof. Deterministic:
        /// fields are emitted in fixed order and effect-free. Consumers hash
        /// this to obtain artifact content hashes.
        pub fn canonical(&self) -> String {
            let mut s = String::new();
            s.push_str("contract=");
            s.push_str(DETERMINISM_CONTRACT);
            s.push_str("\nprocess=");
            s.push_str(&self.process);
            s.push_str("\nclass=");
            s.push_str(self.class.as_str());
            s.push_str("\ncompiler=");
            s.push_str(&self.compiler_version);
            s.push_str("\nschema=");
            s.push_str(&self.schema_version.to_string());
            s.push_str("\npolicy=");
            s.push_str(&self.policy_version.to_string());
            s.push_str("\ntarget=");
            s.push_str(&self.target);
            s.push('\n');
            s
        }
    }
}

pub use proofs::ExecutionProof;

/// A snapshot-to-snapshot transition in the limited D1 replay journal.
///
/// The caller supplies the state snapshots. This crate does not execute a
/// process or infer how one snapshot became another.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StateTransition {
    /// Zero-based position in the journal.
    pub sequence: u64,
    /// State that must be current before this transition is applied.
    pub before: Vec<u8>,
    /// State to install when `before` matches.
    pub after: Vec<u8>,
}

/// A deterministic journal of explicitly supplied state transitions.
///
/// The journal is intentionally a small D1 primitive: it checks and replays
/// byte snapshots, but does not provide a general bytecode or effect replay
/// engine.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ReplayJournal {
    initial_state: Vec<u8>,
    transitions: Vec<StateTransition>,
}

/// Failure while constructing or replaying a [`ReplayJournal`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReplayError {
    /// A transition did not use the next zero-based sequence number.
    Sequence { expected: u64, actual: u64 },
    /// A transition's `before` snapshot did not match the preceding state.
    DiscontinuousState { sequence: u64 },
    /// The replay input differs from the journal's declared initial state.
    InitialStateMismatch,
    /// The replayed state differs from the caller's expected final state.
    FinalStateMismatch,
}

impl ReplayJournal {
    /// Start a journal from an explicit initial state snapshot.
    pub fn new(initial_state: impl Into<Vec<u8>>) -> Self {
        Self {
            initial_state: initial_state.into(),
            transitions: Vec::new(),
        }
    }

    /// Append a transition at the next sequence, requiring its before-state
    /// to match the current recorded state.
    pub fn record(
        &mut self,
        sequence: u64,
        before: impl Into<Vec<u8>>,
        after: impl Into<Vec<u8>>,
    ) -> Result<(), ReplayError> {
        let expected = self.transitions.len() as u64;
        if sequence != expected {
            return Err(ReplayError::Sequence {
                expected,
                actual: sequence,
            });
        }
        let before = before.into();
        if before != self.current_state() {
            return Err(ReplayError::DiscontinuousState { sequence });
        }
        self.transitions.push(StateTransition {
            sequence,
            before,
            after: after.into(),
        });
        Ok(())
    }

    /// The initial snapshot declared by this journal.
    pub fn initial_state(&self) -> &[u8] {
        &self.initial_state
    }

    /// Recorded transitions in sequence order.
    pub fn transitions(&self) -> &[StateTransition] {
        &self.transitions
    }

    /// The last recorded state (or the initial snapshot for an empty journal).
    pub fn current_state(&self) -> &[u8] {
        self.transitions
            .last()
            .map(|transition| transition.after.as_slice())
            .unwrap_or(&self.initial_state)
    }

    /// Replay transitions against an explicit initial snapshot.
    pub fn replay(&self, initial_state: &[u8]) -> Result<Vec<u8>, ReplayError> {
        if initial_state != self.initial_state {
            return Err(ReplayError::InitialStateMismatch);
        }
        let mut state = initial_state.to_vec();
        for (index, transition) in self.transitions.iter().enumerate() {
            let expected = index as u64;
            if transition.sequence != expected {
                return Err(ReplayError::Sequence {
                    expected,
                    actual: transition.sequence,
                });
            }
            if transition.before != state {
                return Err(ReplayError::DiscontinuousState {
                    sequence: transition.sequence,
                });
            }
            state.clone_from(&transition.after);
        }
        Ok(state)
    }

    /// Replay and compare with an expected final snapshot.
    pub fn replay_equal(
        &self,
        initial_state: &[u8],
        expected_final_state: &[u8],
    ) -> Result<(), ReplayError> {
        if self.replay(initial_state)? == expected_final_state {
            Ok(())
        } else {
            Err(ReplayError::FinalStateMismatch)
        }
    }

    /// Stable, versioned, length-prefixed encoding. Lengths and sequence
    /// numbers are little-endian u64 values; state bytes are opaque.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        const DOMAIN: &[u8] = b"GY-EDC-D1-REPLAY/1\0";
        let mut bytes = Vec::new();
        bytes.extend_from_slice(DOMAIN);
        push_blob(&mut bytes, &self.initial_state);
        bytes.extend_from_slice(&(self.transitions.len() as u64).to_le_bytes());
        for transition in &self.transitions {
            bytes.extend_from_slice(&transition.sequence.to_le_bytes());
            push_blob(&mut bytes, &transition.before);
            push_blob(&mut bytes, &transition.after);
        }
        bytes
    }

    /// FNV-1a 64-bit fingerprint of [`Self::canonical_bytes`]. This is a
    /// deterministic identity/checksum, not a cryptographic commitment.
    pub fn hash(&self) -> u64 {
        self.canonical_bytes()
            .iter()
            .fold(0xcbf29ce484222325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            })
    }
}

fn push_blob(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    output.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_with_declared_effect_is_rejected() {
        // Invariant #8.
        let decl = ProcessDecl::new(
            "pure_liar",
            DeterminismClass::Pure,
            vec![EffectKind::Network],
        );
        assert_eq!(classify(&decl), Err(RejectReason::UnresolvedClaim));
    }

    #[test]
    fn d1_without_state_contract_is_rejected() {
        let mut decl = ProcessDecl::new("cached", DeterminismClass::StateDeterministic, vec![]);
        assert_eq!(classify(&decl), Err(RejectReason::MissingStateContract));
        decl.state_contract = true;
        let v = classify(&decl).expect("D1 with contract must pass");
        assert_eq!(v.class, DeterminismClass::StateDeterministic);
    }

    #[test]
    fn d2_with_undeclared_effect_is_rejected() {
        let decl = ProcessDecl::new("fetcher", DeterminismClass::EffectDeterministic, vec![]);
        // A D2 claim with no declared effects admits nothing.
        assert!(matches!(
            classify(&decl),
            Err(RejectReason::UndisclosedEffect(_))
        ));
    }

    #[test]
    fn valid_d0_is_accepted() {
        let decl = ProcessDecl::new("arithmetic", DeterminismClass::Pure, vec![]);
        let v = classify(&decl).expect("valid D0 must pass");
        assert_eq!(v.class, DeterminismClass::Pure);
        assert!(v.declared_effects.is_empty());
    }

    #[test]
    fn valid_d2_with_declared_effects_is_accepted() {
        let decl = ProcessDecl::new(
            "scheduler",
            DeterminismClass::EffectDeterministic,
            vec![EffectKind::Clock, EffectKind::FileSystem],
        );
        let v = classify(&decl).expect("valid D2 must pass");
        assert_eq!(v.class, DeterminismClass::EffectDeterministic);
        assert_eq!(v.declared_effects.len(), 2);
    }

    #[test]
    fn nondeterministic_is_never_compiled() {
        let decl = ProcessDecl::new("gambler", DeterminismClass::Nondeterministic, vec![]);
        assert_eq!(classify(&decl), Err(RejectReason::Nondeterministic));
    }

    #[test]
    fn proof_identity_includes_all_versions() {
        let proof = ExecutionProof {
            process: "scheduler".into(),
            class: DeterminismClass::EffectDeterministic,
            compiler_version: "0.1.0".into(),
            schema_version: 1,
            policy_version: 2,
            target: "x86_64-unknown-linux-gnu".into(),
        };
        let canonical = proof.canonical();
        for needle in [
            "compiler=0.1.0",
            "schema=1",
            "policy=2",
            "target=x86_64-unknown-linux-gnu",
            DETERMINISM_CONTRACT,
            "class=D2",
        ] {
            assert!(
                canonical.contains(needle),
                "canonical proof missing {needle}"
            );
        }
        // Determinism: same proof, same serialisation.
        assert_eq!(canonical, proof.canonical());
    }

    #[test]
    fn state_journal_replays_and_checks_final_equality() {
        let mut journal = ReplayJournal::new(b"cold".to_vec());
        journal
            .record(0, b"cold".to_vec(), b"warm".to_vec())
            .unwrap();
        journal
            .record(1, b"warm".to_vec(), b"ready".to_vec())
            .unwrap();

        assert_eq!(journal.replay(b"cold"), Ok(b"ready".to_vec()));
        assert_eq!(journal.replay_equal(b"cold", b"ready"), Ok(()));
        assert_eq!(
            journal.replay_equal(b"cold", b"other"),
            Err(ReplayError::FinalStateMismatch)
        );
        assert_eq!(
            journal.replay(b"warm"),
            Err(ReplayError::InitialStateMismatch)
        );
    }

    #[test]
    fn state_journal_rejects_sequence_gaps_and_discontinuities() {
        let mut journal = ReplayJournal::new(vec![0]);
        assert_eq!(
            journal.record(1, vec![0], vec![1]),
            Err(ReplayError::Sequence {
                expected: 0,
                actual: 1
            })
        );
        assert_eq!(
            journal.record(0, vec![9], vec![1]),
            Err(ReplayError::DiscontinuousState { sequence: 0 })
        );
    }

    #[test]
    fn state_journal_encoding_and_hash_are_stable_and_framed() {
        let mut first = ReplayJournal::new(vec![1, 2]);
        first.record(0, vec![1, 2], vec![3]).unwrap();
        let mut same = ReplayJournal::new(vec![1, 2]);
        same.record(0, vec![1, 2], vec![3]).unwrap();
        let mut different = ReplayJournal::new(vec![1, 2, 3]);
        different.record(0, vec![1, 2, 3], vec![]).unwrap();

        assert_eq!(first.canonical_bytes(), same.canonical_bytes());
        assert_eq!(first.hash(), same.hash());
        assert_ne!(first.canonical_bytes(), different.canonical_bytes());
        assert_ne!(first.hash(), different.hash());
        assert!(first.canonical_bytes().starts_with(b"GY-EDC-D1-REPLAY/1\0"));
    }
}
