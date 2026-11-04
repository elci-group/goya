//! goya-hotswap — transactional hot-replacement supervisor for compiled
//! executable artifacts (GY-EDC-001 invariants #5–#7).
//!
//! Contract: [`HOTSWAP_CONTRACT`] (`GY-HOTSWAP/1`).
//!
//! Builds on [`goya_executable`] and [`goya_determinism`]:
//! - Invariant #5: identity. Every swap candidate carries a full
//!   [`ExecutionProof`] inside its [`goya_executable::Artifact`]; the
//!   supervisor keeps the proof-anchored artifact, never a bare hash.
//! - Invariant #6 companion: stale artifacts (schema or policy drift) are
//!   rejected before anything else, and [`units_compatible`] decides whether
//!   two units may substitute for each other.
//! - Invariant #7: capability non-escalation is re-validated at swap time
//!   against the candidate's origination capability set.
//!
//! `propose_swap` is transactional in the style of `goya_lang::SceneStore`:
//! on any rejection the supervisor is returned *completely untouched* —
//! neither the active unit nor the swap history is mutated, and the rejection
//! names the [`SwapStage`] that failed.

use goya_determinism::proofs::ExecutionProof;
use goya_executable::{is_stale, validate_capability_escalation, Artifact};

/// The hot-swap contract identifier this crate implements.
pub const HOTSWAP_CONTRACT: &str = "GY-HOTSWAP/1";

/// A compiled unit proposed for activation, with the compatibility metadata
/// the supervisor checks before swapping it in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SwapCandidate {
    /// The compiled artifact being proposed.
    pub artifact: Artifact,
    /// Version of the state snapshot contract the candidate reads/writes.
    /// Must be `<=` the active unit's version so the same app (or an older
    /// one) can still read persisted state.
    pub state_contract_version: u64,
    /// Minimum application version required to run this candidate.
    pub min_app_version: (u32, u32, u32),
    /// Version of the application proposing the swap.
    pub current_app_version: (u32, u32, u32),
}

impl SwapCandidate {
    /// Build a candidate whose app version floor is `(0, 1, 0)`.
    pub fn new(artifact: Artifact, state_contract_version: u64) -> Self {
        Self {
            artifact,
            state_contract_version,
            min_app_version: (0, 1, 0),
            current_app_version: (0, 1, 0),
        }
    }
}

/// The currently active unit, the revision at which it was activated, and
/// the state contract bound it established.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ActiveUnit {
    /// The live artifact.
    pub artifact: Artifact,
    /// Supervisor revision at which this artifact was activated.
    pub activated_at_rev: u64,
    /// The state contract version this unit bound the supervisor to.
    pub state_contract_version: u64,
}

/// An ordered record of one successful swap. `from` is `None` for the
/// initial activation (there was no previous artifact).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SwapRecord {
    /// The artifact that was active before the swap, if any.
    pub from: Option<Artifact>,
    /// The artifact activated by the swap.
    pub to: Artifact,
    /// The revision assigned to this swap.
    pub rev: u64,
    /// The state contract version bound by the `to` artifact at this swap
    /// (bookkeeping so rollback can restore the predecessor's bound).
    pub bound: u64,
}

/// Ordered stages of a swap proposal; the first failing stage aborts the
/// transaction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwapStage {
    /// Stage 1: artifact is stale (schema or policy drift, invariant #6).
    Staleness,
    /// Stage 2: capability escalation attempted (invariant #7).
    Capability,
    /// Stage 3: state contract version is not backwards-compatible.
    StateContract,
    /// Stage 4: application version below the candidate's minimum.
    AppVersion,
    /// The supervisor revision counter cannot advance without overflowing.
    Revision,
}

/// Why a swap proposal was rejected, and at which stage.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SwapRejection {
    /// The stage that rejected the candidate.
    pub stage: SwapStage,
    /// Human-readable explanation.
    pub reason: String,
}

/// Why a rollback could not be performed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RollbackError {
    /// The history is empty: nothing to roll back to.
    NoHistory,
    /// Rolling back would go past the origin artifact (the initial
    /// activation has no predecessor).
    PastOrigin,
}

/// Hot-swap supervisor over registered executable artifacts.
///
/// State transitions are transactional: a rejected
/// [`SwapSupervisor::propose_swap`] leaves `active` and `history`
/// bit-for-bit unchanged.
#[derive(Clone, Default, Debug)]
pub struct SwapSupervisor {
    /// The currently active unit (none before the first swap).
    active: Option<ActiveUnit>,
    /// Ordered history of successful swaps.
    history: Vec<SwapRecord>,
}

impl SwapSupervisor {
    /// Create an empty supervisor with no active unit and no history.
    pub fn new() -> Self {
        Self::default()
    }

    /// The currently active unit, if any.
    pub fn active(&self) -> Option<&ActiveUnit> {
        self.active.as_ref()
    }

    /// The ordered swap history.
    pub fn history(&self) -> &[SwapRecord] {
        &self.history
    }

    /// The current revision: the revision of the most recent swap, or 0
    /// before any swap.
    pub fn revision(&self) -> u64 {
        self.history.last().map_or(0, |rec| rec.rev)
    }

    /// Propose a hot swap. All checks must pass, in order:
    ///
    /// 1. **Staleness** (invariant #6): the artifact must not be stale
    ///    against the current schema/policy.
    /// 2. **Capability non-escalation** (invariant #7): every artifact
    ///    capability must already be held by the origination set.
    /// 3. **State contract**: `candidate.state_contract_version` must be
    ///    `<=` the active unit's version; the first swap is free (no active
    ///    unit constrains it).
    /// 4. **App version**: `current_app_version >= min_app_version`.
    ///
    /// On success a [`SwapRecord`] is appended and the new revision is
    /// returned. On any failure the supervisor is left completely untouched.
    pub fn propose_swap(
        &mut self,
        candidate: SwapCandidate,
        origination: &[String],
        current_schema: u64,
        current_policy: u64,
    ) -> Result<u64, SwapRejection> {
        // Stage 1: staleness (invariant #6 companion).
        if is_stale(&candidate.artifact, current_schema, current_policy) {
            return Err(SwapRejection {
                stage: SwapStage::Staleness,
                reason: format!(
                    "artifact is stale: proof schema={} policy={} vs current schema={} policy={}",
                    candidate.artifact.proof.schema_version,
                    candidate.artifact.proof.policy_version,
                    current_schema,
                    current_policy
                ),
            });
        }

        // Stage 2: capability non-escalation (invariant #7).
        if !validate_capability_escalation(origination, &candidate.artifact) {
            return Err(SwapRejection {
                stage: SwapStage::Capability,
                reason: "artifact acquires capabilities the origination set never held".into(),
            });
        }

        // Stage 3: state contract compatibility. The first swap is free;
        // afterwards the candidate may not raise the contract version.
        if let Some(active) = &self.active {
            if candidate.state_contract_version > active.state_contract_version {
                return Err(SwapRejection {
                    stage: SwapStage::StateContract,
                    reason: format!(
                        "candidate state contract v{} exceeds active bound v{}",
                        candidate.state_contract_version, active.state_contract_version
                    ),
                });
            }
        }

        // Stage 4: application version floor.
        if candidate.current_app_version < candidate.min_app_version {
            return Err(SwapRejection {
                stage: SwapStage::AppVersion,
                reason: format!(
                    "app version {:?} is below candidate minimum {:?}",
                    candidate.current_app_version, candidate.min_app_version
                ),
            });
        }

        // Resolve the next revision before mutating anything. A long-lived
        // supervisor must fail closed instead of wrapping its identity counter.
        let rev = self.revision().checked_add(1).ok_or_else(|| SwapRejection {
            stage: SwapStage::Revision,
            reason: "supervisor revision counter is exhausted".into(),
        })?;

        // All checks passed: commit the transaction.
        let from = self.active.take().map(|unit| unit.artifact);
        self.history.push(SwapRecord {
            from,
            to: candidate.artifact.clone(),
            rev,
            bound: candidate.state_contract_version,
        });
        self.active = Some(ActiveUnit {
            artifact: candidate.artifact,
            activated_at_rev: rev,
            state_contract_version: candidate.state_contract_version,
        });
        Ok(rev)
    }

    /// Roll back to the previous artifact by popping the most recent swap
    /// record. Rolling back the initial activation (whose `from` is `None`)
    /// is refused with [`RollbackError::PastOrigin`] and leaves the
    /// supervisor untouched. On success the restored revision is returned.
    pub fn rollback(&mut self) -> Result<u64, RollbackError> {
        let popped = match self.history.last() {
            Some(record) => record.clone(),
            None => return Err(RollbackError::NoHistory),
        };
        let previous = match popped.from {
            Some(artifact) => artifact,
            None => return Err(RollbackError::PastOrigin),
        };
        self.history.pop();
        let rev = self.revision();
        // The restored predecessor keeps the state contract bound it had
        // when it was last active: the bound recorded by the swap that
        // activated it (the new top of history). If the history is now
        // empty the predecessor was the origin unit, whose bound was the
        // first swap's own contract version.
        let bound = self.history.last().map_or(popped.bound, |rec| rec.bound);
        self.active = Some(ActiveUnit {
            artifact: previous,
            activated_at_rev: rev,
            state_contract_version: bound,
        });
        Ok(rev)
    }
}

/// EDC invariant #6: two units are compatible when they were produced under
/// the same schema, policy and target (and thus neither is stale relative to
/// the other's toolchain identity).
pub fn units_compatible(a: &ExecutionProof, b: &ExecutionProof) -> bool {
    a.schema_version == b.schema_version
        && a.policy_version == b.policy_version
        && a.target == b.target
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_determinism::DeterminismClass;

    const SCHEMA: u64 = 1;
    const POLICY: u64 = 1;
    const ORIGIN: &[&str] = &["clock", "filesystem"];

    fn proof(name: &str, schema: u64, policy: u64) -> ExecutionProof {
        ExecutionProof {
            process: name.into(),
            class: DeterminismClass::EffectDeterministic,
            compiler_version: "0.1.0".into(),
            schema_version: schema,
            policy_version: policy,
            target: "x86_64-unknown-linux-gnu".into(),
        }
    }

    fn candidate(name: &str, contract: u64) -> SwapCandidate {
        let mut c = SwapCandidate::new(
            Artifact::from_proof(
                proof(name, SCHEMA, POLICY),
                vec!["clock".into(), "filesystem".into()],
            ),
            contract,
        );
        c.current_app_version = (0, 2, 0);
        c
    }

    fn origination() -> Vec<String> {
        ORIGIN.iter().map(|s| (*s).to_string()).collect()
    }

    fn supervisor_with_first_swap() -> SwapSupervisor {
        let mut sup = SwapSupervisor::new();
        sup.propose_swap(candidate("v1", 1), &origination(), SCHEMA, POLICY)
            .expect("first swap must succeed");
        sup
    }

    #[test]
    fn happy_swap_bumps_revision() {
        let mut sup = SwapSupervisor::new();
        assert_eq!(sup.revision(), 0);
        assert!(sup.active().is_none());

        let rev1 = sup
            .propose_swap(candidate("v1", 1), &origination(), SCHEMA, POLICY)
            .expect("first swap");
        assert_eq!(rev1, 1);
        assert_eq!(sup.revision(), 1);
        let active = sup.active().expect("active");
        assert_eq!(active.artifact.proof.process, "v1");
        assert_eq!(active.activated_at_rev, 1);

        let rev2 = sup
            .propose_swap(candidate("v2", 1), &origination(), SCHEMA, POLICY)
            .expect("second swap");
        assert_eq!(rev2, 2);
        assert_eq!(sup.active().expect("active").artifact.proof.process, "v2");
        assert_eq!(sup.history().len(), 2);
    }

    #[test]
    fn stale_rejection_is_transactional() {
        let mut sup = supervisor_with_first_swap();
        let before = sup.clone();

        let stale = SwapCandidate::new(
            Artifact::from_proof(proof("stale", SCHEMA + 1, POLICY), vec![]),
            1,
        );
        let err = sup
            .propose_swap(stale, &origination(), SCHEMA, POLICY)
            .expect_err("stale artifact must be rejected");
        assert_eq!(err.stage, SwapStage::Staleness);

        assert_eq!(sup.active, before.active, "active untouched");
        assert_eq!(sup.history, before.history, "history untouched");
    }

    #[test]
    fn capability_escalation_rejection_is_transactional() {
        let mut sup = supervisor_with_first_swap();
        let before = sup.clone();

        let escalator = SwapCandidate::new(
            Artifact::from_proof(
                proof("escalator", SCHEMA, POLICY),
                vec!["clock".into(), "network".into()],
            ),
            1,
        );
        let err = sup
            .propose_swap(escalator, &origination(), SCHEMA, POLICY)
            .expect_err("escalation must be rejected");
        assert_eq!(err.stage, SwapStage::Capability);

        assert_eq!(sup.active, before.active);
        assert_eq!(sup.history, before.history);
    }

    #[test]
    fn state_contract_regression_rejected_but_first_swap_free() {
        // First swap is free: no active unit constrains the contract version.
        let mut sup = SwapSupervisor::new();
        let rev = sup
            .propose_swap(candidate("v1", 7), &origination(), SCHEMA, POLICY)
            .expect("first swap sets the bound freely");
        assert_eq!(rev, 1);
        let before = sup.clone();

        // Raising the contract version above the active bound is rejected.
        let raising = candidate("v2", 8);
        let err = sup
            .propose_swap(raising, &origination(), SCHEMA, POLICY)
            .expect_err("raising contract version must be rejected");
        assert_eq!(err.stage, SwapStage::StateContract);
        assert_eq!(sup.active, before.active);
        assert_eq!(sup.history, before.history);

        // Equal or lower is accepted.
        let rev2 = sup
            .propose_swap(candidate("v3", 7), &origination(), SCHEMA, POLICY)
            .expect("equal contract version is compatible");
        assert_eq!(rev2, 2);
        let rev3 = sup
            .propose_swap(candidate("v4", 3), &origination(), SCHEMA, POLICY)
            .expect("lower contract version is backwards-compatible");
        assert_eq!(rev3, 3);
    }

    #[test]
    fn app_version_rejection_is_transactional() {
        let mut sup = supervisor_with_first_swap();
        let before = sup.clone();

        let mut too_old = candidate("v2", 1);
        too_old.min_app_version = (1, 0, 0);
        too_old.current_app_version = (0, 2, 0); // below (1,0,0)
        let err = sup
            .propose_swap(too_old, &origination(), SCHEMA, POLICY)
            .expect_err("app below candidate minimum must be rejected");
        assert_eq!(err.stage, SwapStage::AppVersion);

        assert_eq!(sup.active, before.active);
        assert_eq!(sup.history, before.history);
    }

    #[test]
    fn rollback_restores_previous_artifact() {
        let mut sup = supervisor_with_first_swap();
        let v1 = sup.active().expect("active").artifact.clone();

        sup.propose_swap(candidate("v2", 1), &origination(), SCHEMA, POLICY)
            .expect("second swap");
        assert_eq!(sup.active().expect("active").artifact.proof.process, "v2");

        let rev = sup.rollback().expect("rollback");
        assert_eq!(rev, 1);
        assert_eq!(sup.active().expect("active").artifact, v1);
        assert_eq!(sup.history().len(), 1);
    }

    #[test]
    fn rollback_past_origin_errors() {
        let mut sup = SwapSupervisor::new();
        // No history at all.
        assert_eq!(sup.rollback(), Err(RollbackError::NoHistory));

        sup.propose_swap(candidate("v1", 1), &origination(), SCHEMA, POLICY)
            .expect("initial activation");
        // The initial activation has no predecessor: rolling back would go
        // past the origin, and must not mutate the supervisor.
        let before = sup.clone();
        assert_eq!(sup.rollback(), Err(RollbackError::PastOrigin));
        assert_eq!(sup.active, before.active);
        assert_eq!(sup.history, before.history);

        // Double rollback: swap, roll back to origin, then try again.
        sup.propose_swap(candidate("v2", 1), &origination(), SCHEMA, POLICY)
            .expect("second swap");
        sup.rollback().expect("first rollback to origin");
        assert_eq!(sup.rollback(), Err(RollbackError::PastOrigin));
    }

    #[test]
    fn sequential_swaps_chain() {
        let mut sup = SwapSupervisor::new();
        let v1 = candidate("v1", 1).artifact;
        let v2 = candidate("v2", 1).artifact;
        let v3 = candidate("v3", 1).artifact;

        sup.propose_swap(candidate("v1", 1), &origination(), SCHEMA, POLICY)
            .expect("swap 1");
        sup.propose_swap(candidate("v2", 1), &origination(), SCHEMA, POLICY)
            .expect("swap 2");
        sup.propose_swap(candidate("v3", 1), &origination(), SCHEMA, POLICY)
            .expect("swap 3");

        assert_eq!(sup.revision(), 3);
        assert_eq!(sup.history().len(), 3);
        assert_eq!(sup.history()[0].from, None);
        assert_eq!(sup.history()[1].from, Some(v1.clone()));
        assert_eq!(sup.history()[2].from, Some(v2.clone()));
        assert_eq!(sup.active().expect("active").artifact, v3);

        // Rolling back twice walks the chain backwards.
        sup.rollback().expect("rollback 3");
        assert_eq!(sup.active().expect("active").artifact, v2);
        sup.rollback().expect("rollback 2");
        assert_eq!(sup.active().expect("active").artifact, v1);
    }

    #[test]
    fn same_proposals_same_outcomes() {
        // Determinism: two supervisors fed identical proposal sequences must
        // reach identical state, revision by revision.
        let mut a = SwapSupervisor::new();
        let mut b = SwapSupervisor::new();

        for name in ["v1", "v2", "v3"] {
            let ra = a.propose_swap(candidate(name, 1), &origination(), SCHEMA, POLICY);
            let rb = b.propose_swap(candidate(name, 1), &origination(), SCHEMA, POLICY);
            assert_eq!(ra, rb);
        }
        // A rejected proposal is also deterministic.
        let bad = SwapCandidate::new(
            Artifact::from_proof(proof("stale", SCHEMA + 5, POLICY), vec![]),
            1,
        );
        assert_eq!(
            a.propose_swap(bad.clone(), &origination(), SCHEMA, POLICY),
            b.propose_swap(bad, &origination(), SCHEMA, POLICY)
        );
        assert_eq!(a.revision(), b.revision());
        assert_eq!(a.active, b.active);
        assert_eq!(a.history, b.history);
    }

    #[test]
    fn units_compatible_per_invariant_six() {
        let same = proof("a", 1, 1);
        let same_other_name = proof("b", 1, 1);
        let schema_drift = proof("a", 2, 1);
        let policy_drift = proof("a", 1, 2);
        let target_drift = ExecutionProof {
            process: "a".into(),
            class: DeterminismClass::EffectDeterministic,
            compiler_version: "0.1.0".into(),
            schema_version: 1,
            policy_version: 1,
            target: "aarch64-unknown-linux-gnu".into(),
        };

        assert!(units_compatible(&same, &same));
        // Different process names under the same toolchain are compatible.
        assert!(units_compatible(&same, &same_other_name));
        assert!(!units_compatible(&same, &schema_drift));
        assert!(!units_compatible(&same, &policy_drift));
        assert!(!units_compatible(&same, &target_drift));
    }
}
