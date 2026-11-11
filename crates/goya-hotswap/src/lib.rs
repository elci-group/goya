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

/// Explicit persisted-state migrations used to prepare state before a swap.
pub mod state_migration;

use goya_determinism::proofs::ExecutionProof;
use goya_executable::{is_stale, validate_capability_escalation, Artifact};
use std::collections::{BTreeMap, BTreeSet};

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
    /// The candidate changes or removes an interface relied on by the active unit.
    Interface,
}

/// Why a swap proposal was rejected, and at which stage.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SwapRejection {
    /// The stage that rejected the candidate.
    pub stage: SwapStage,
    /// Human-readable explanation.
    pub reason: String,
}

/// Failure while migrating persisted state and then activating the candidate.
#[derive(Debug)]
pub enum MigratedSwapError<E> {
    /// The migration chain did not produce a snapshot for the candidate.
    Migration(state_migration::MigrationError),
    /// A swap check rejected the candidate before or instead of activation.
    Swap(SwapRejection),
    /// The caller refused to publish the migrated snapshot. The supervisor
    /// was left unchanged.
    Commit(E),
}

/// How [`SwapSupervisor::screen`] treats the state-contract version.
#[derive(Clone, Copy)]
enum ContractRule {
    /// Candidate version must be less than or equal to the active bound.
    Compatible,
    /// Candidate version must be strictly newer than the active bound.
    Upgrade,
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
    /// A higher state-contract version uses [`Self::propose_migrated_swap`].
    pub fn propose_swap(
        &mut self,
        candidate: SwapCandidate,
        origination: &[String],
        current_schema: u64,
        current_policy: u64,
    ) -> Result<u64, SwapRejection> {
        self.screen(
            &candidate,
            origination,
            current_schema,
            current_policy,
            ContractRule::Compatible,
        )?;
        Ok(self.commit_candidate(candidate))
    }

    /// Migrate `stored` from the active contract version up to the candidate,
    /// publish that snapshot, and only then activate the candidate.
    ///
    /// Swap checks run before `commit`. `commit` receives the migrated map and
    /// must publish it (for example with [`goya_state::Transaction::stage_snapshot`])
    /// before returning `Ok`. A migration error, a rejected swap check, or a
    /// failed `commit` leaves the supervisor untouched. An unmigrated higher
    /// version remains rejected by [`Self::propose_swap`].
    pub fn propose_migrated_swap<E>(
        &mut self,
        candidate: SwapCandidate,
        stored: &goya_state::Map,
        migrations: &[state_migration::StateMigration],
        origination: &[String],
        current_schema: u64,
        current_policy: u64,
        commit: impl FnOnce(&goya_state::Map) -> Result<(), E>,
    ) -> Result<u64, MigratedSwapError<E>> {
        let active_version = self
            .active
            .as_ref()
            .map(|unit| unit.state_contract_version)
            .ok_or_else(|| {
                MigratedSwapError::Swap(SwapRejection {
                    stage: SwapStage::StateContract,
                    reason: "state migration requires an active contract version".into(),
                })
            })?;
        self.screen(
            &candidate,
            origination,
            current_schema,
            current_policy,
            ContractRule::Upgrade,
        )
        .map_err(MigratedSwapError::Swap)?;
        let migrated = state_migration::migrate_state(
            stored,
            active_version,
            candidate.state_contract_version,
            migrations,
        )
        .map_err(MigratedSwapError::Migration)?;
        commit(&migrated).map_err(MigratedSwapError::Commit)?;
        Ok(self.commit_candidate(candidate))
    }

    fn screen(
        &self,
        candidate: &SwapCandidate,
        origination: &[String],
        current_schema: u64,
        current_policy: u64,
        contract: ContractRule,
    ) -> Result<(), SwapRejection> {
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

        if !validate_capability_escalation(origination, &candidate.artifact) {
            return Err(SwapRejection {
                stage: SwapStage::Capability,
                reason: "artifact acquires capabilities the origination set never held".into(),
            });
        }

        match contract {
            ContractRule::Compatible => {
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
            }
            ContractRule::Upgrade => match &self.active {
                Some(active)
                    if candidate.state_contract_version > active.state_contract_version => {}
                Some(active) => {
                    return Err(SwapRejection {
                        stage: SwapStage::StateContract,
                        reason: format!(
                            "candidate state contract v{} is not an upgrade from v{}",
                            candidate.state_contract_version, active.state_contract_version
                        ),
                    });
                }
                None => {
                    return Err(SwapRejection {
                        stage: SwapStage::StateContract,
                        reason: "state migration requires an active contract version".into(),
                    });
                }
            },
        }

        if candidate.current_app_version < candidate.min_app_version {
            return Err(SwapRejection {
                stage: SwapStage::AppVersion,
                reason: format!(
                    "app version {:?} is below candidate minimum {:?}",
                    candidate.current_app_version, candidate.min_app_version
                ),
            });
        }

        self.revision()
            .checked_add(1)
            .ok_or_else(|| SwapRejection {
                stage: SwapStage::Revision,
                reason: "supervisor revision counter is exhausted".into(),
            })?;
        Ok(())
    }

    fn commit_candidate(&mut self, candidate: SwapCandidate) -> u64 {
        let rev = self
            .revision()
            .checked_add(1)
            .expect("revision overflow is rejected before commit");
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
        rev
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

/// Stable signature for one exported operation. Type names are canonical
/// identifiers supplied by the producer (for example, `goya:text/v1`).
/// Capability requirements may shrink during an upgrade but may not grow for
/// an existing operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OperationInterface {
    /// Ordered input type identifiers.
    pub inputs: Vec<String>,
    /// Output type identifier.
    pub output: String,
    /// Capabilities required to invoke this operation.
    pub required_capabilities: BTreeSet<String>,
}

/// Public interface exported by a compiled unit. A `BTreeMap` makes
/// reconciliation output deterministic regardless of declaration order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceContract {
    /// Export name to its callable signature.
    pub exports: BTreeMap<String, OperationInterface>,
}

/// Deterministic summary of a successful interface reconciliation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceDiff {
    /// Exports present in the candidate and absent from the previous unit.
    pub added_exports: Vec<String>,
    /// Existing exports whose input/output types remain compatible.
    pub retained_exports: Vec<String>,
}

/// A breaking interface change. Ordering follows export-name order, then the
/// fixed field comparison order documented by [`reconcile_interfaces`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceIssue {
    /// Affected export name.
    pub export: String,
    /// Stable machine-readable issue kind.
    pub kind: InterfaceIssueKind,
}

/// Categories reported by interface reconciliation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceIssueKind {
    /// An export provided by the active unit is absent from the candidate.
    RemovedExport,
    /// The ordered input type list changed.
    InputsChanged,
    /// The output type changed.
    OutputChanged,
    /// An existing operation now requires additional authority.
    CapabilityAdded,
    /// A required type or capability identifier was empty.
    EmptyIdentifier,
}

/// Compare a replacement's public interface with the active unit.
///
/// Removing an export, changing its input/output types, or requiring a new
/// capability is rejected. New exports and reductions in required authority
/// are compatible. The function is pure and returns results in stable order.
pub fn reconcile_interfaces(
    previous: &InterfaceContract,
    candidate: &InterfaceContract,
) -> Result<InterfaceDiff, Vec<InterfaceIssue>> {
    let mut issues = Vec::new();

    for (name, operation) in previous.exports.iter().chain(candidate.exports.iter()) {
        let has_empty = name.trim().is_empty()
            || operation.output.trim().is_empty()
            || operation.inputs.iter().any(|ty| ty.trim().is_empty())
            || operation
                .required_capabilities
                .iter()
                .any(|capability| capability.trim().is_empty());
        if has_empty {
            issues.push(InterfaceIssue {
                export: name.clone(),
                kind: InterfaceIssueKind::EmptyIdentifier,
            });
        }
    }
    if !issues.is_empty() {
        issues.sort_by(|a, b| (&a.export, a.kind as u8).cmp(&(&b.export, b.kind as u8)));
        issues.dedup();
        return Err(issues);
    }

    let mut diff = InterfaceDiff::default();
    for (name, old) in &previous.exports {
        let Some(new) = candidate.exports.get(name) else {
            issues.push(InterfaceIssue {
                export: name.clone(),
                kind: InterfaceIssueKind::RemovedExport,
            });
            continue;
        };
        if old.inputs != new.inputs {
            issues.push(InterfaceIssue {
                export: name.clone(),
                kind: InterfaceIssueKind::InputsChanged,
            });
        }
        if old.output != new.output {
            issues.push(InterfaceIssue {
                export: name.clone(),
                kind: InterfaceIssueKind::OutputChanged,
            });
        }
        if !new
            .required_capabilities
            .is_subset(&old.required_capabilities)
        {
            issues.push(InterfaceIssue {
                export: name.clone(),
                kind: InterfaceIssueKind::CapabilityAdded,
            });
        }
        if old.inputs == new.inputs && old.output == new.output {
            diff.retained_exports.push(name.clone());
        }
    }
    diff.added_exports.extend(
        candidate
            .exports
            .keys()
            .filter(|name| !previous.exports.contains_key(*name))
            .cloned(),
    );
    if issues.is_empty() {
        Ok(diff)
    } else {
        Err(issues)
    }
}

/// Hot-swap supervisor that requires interface reconciliation on every
/// replacement. The legacy [`SwapSupervisor`] remains available for clients
/// that do not yet publish interface manifests.
#[derive(Clone, Debug, Default)]
pub struct ReconciledSupervisor {
    swaps: SwapSupervisor,
    interfaces: Vec<InterfaceContract>,
}

impl ReconciledSupervisor {
    /// Create an empty reconciled supervisor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Active executable state.
    pub fn active(&self) -> Option<&ActiveUnit> {
        self.swaps.active()
    }

    /// Active public interface.
    pub fn active_interface(&self) -> Option<&InterfaceContract> {
        self.interfaces.last()
    }

    /// Current successful swap revision.
    pub fn revision(&self) -> u64 {
        self.swaps.revision()
    }

    /// Swap only when the candidate's interface remains compatible with the
    /// active unit. Both interface validation and executable validation occur
    /// before committing either supervisor state or interface history.
    pub fn propose_swap(
        &mut self,
        candidate: SwapCandidate,
        candidate_interface: InterfaceContract,
        origination: &[String],
        current_schema: u64,
        current_policy: u64,
    ) -> Result<(u64, InterfaceDiff), SwapRejection> {
        let diff = self.preview_interface(&candidate_interface)?;

        let mut staged = self.swaps.clone();
        let revision =
            staged.propose_swap(candidate, origination, current_schema, current_policy)?;
        self.swaps = staged;
        self.interfaces.push(candidate_interface);
        Ok((revision, diff))
    }

    /// Reconcile `candidate_interface`, migrate persisted state, and activate
    /// only after `commit` publishes the migrated snapshot.
    ///
    /// An interface rejection or a failed migration/commit leaves both the
    /// executable supervisor and the interface history unchanged.
    pub fn propose_migrated_swap<E>(
        &mut self,
        candidate: SwapCandidate,
        candidate_interface: InterfaceContract,
        stored: &goya_state::Map,
        migrations: &[state_migration::StateMigration],
        origination: &[String],
        current_schema: u64,
        current_policy: u64,
        commit: impl FnOnce(&goya_state::Map) -> Result<(), E>,
    ) -> Result<(u64, InterfaceDiff), MigratedSwapError<E>> {
        let diff = self
            .preview_interface(&candidate_interface)
            .map_err(MigratedSwapError::Swap)?;
        let revision = self.swaps.propose_migrated_swap(
            candidate,
            stored,
            migrations,
            origination,
            current_schema,
            current_policy,
            commit,
        )?;
        self.interfaces.push(candidate_interface);
        Ok((revision, diff))
    }

    fn preview_interface(
        &self,
        candidate_interface: &InterfaceContract,
    ) -> Result<InterfaceDiff, SwapRejection> {
        if let Some(active) = self.active_interface() {
            reconcile_interfaces(active, candidate_interface).map_err(|issues| SwapRejection {
                stage: SwapStage::Interface,
                reason: format!("interface incompatibility: {issues:?}"),
            })
        } else {
            let mut diff = InterfaceDiff::default();
            diff.added_exports
                .extend(candidate_interface.exports.keys().cloned());
            reconcile_interfaces(&InterfaceContract::default(), candidate_interface).map_err(
                |issues| SwapRejection {
                    stage: SwapStage::Interface,
                    reason: format!("invalid candidate interface: {issues:?}"),
                },
            )?;
            Ok(diff)
        }
    }

    /// Roll back executable and interface state atomically.
    pub fn rollback(&mut self) -> Result<u64, RollbackError> {
        let mut staged = self.swaps.clone();
        let revision = staged.rollback()?;
        self.swaps = staged;
        self.interfaces.pop();
        Ok(revision)
    }
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

    fn interface(capability: &str) -> InterfaceContract {
        InterfaceContract {
            exports: BTreeMap::from([(
                "compute".into(),
                OperationInterface {
                    inputs: vec!["goya:number/v1".into()],
                    output: "goya:number/v1".into(),
                    required_capabilities: BTreeSet::from([capability.into()]),
                },
            )]),
        }
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

    #[test]
    fn interface_reconciliation_allows_additive_exports_and_less_authority() {
        let previous = interface("clock");
        let mut candidate = interface("clock");
        candidate
            .exports
            .get_mut("compute")
            .expect("compute")
            .required_capabilities
            .clear();
        candidate.exports.insert(
            "format".into(),
            OperationInterface {
                inputs: vec!["goya:number/v1".into()],
                output: "goya:text/v1".into(),
                required_capabilities: BTreeSet::new(),
            },
        );

        let diff = reconcile_interfaces(&previous, &candidate).expect("compatible additive API");
        assert_eq!(diff.retained_exports, vec!["compute"]);
        assert_eq!(diff.added_exports, vec!["format"]);
    }

    #[test]
    fn interface_reconciliation_rejects_removed_changed_and_privileged_exports() {
        let previous = interface("clock");
        let mut candidate = InterfaceContract::default();
        candidate.exports.insert(
            "compute".into(),
            OperationInterface {
                inputs: vec!["goya:text/v1".into()],
                output: "goya:text/v1".into(),
                required_capabilities: BTreeSet::from(["clock".into(), "network".into()]),
            },
        );
        candidate.exports.insert(
            "new_api".into(),
            OperationInterface {
                inputs: vec![],
                output: "goya:void/v1".into(),
                required_capabilities: BTreeSet::new(),
            },
        );
        let errors = reconcile_interfaces(&previous, &candidate).expect_err("breaking API");
        assert!(errors
            .iter()
            .any(|e| e.kind == InterfaceIssueKind::InputsChanged));
        assert!(errors
            .iter()
            .any(|e| e.kind == InterfaceIssueKind::OutputChanged));
        assert!(errors
            .iter()
            .any(|e| e.kind == InterfaceIssueKind::CapabilityAdded));

        let mut removed = InterfaceContract::default();
        removed.exports.insert(
            "new_api".into(),
            OperationInterface {
                inputs: vec![],
                output: "goya:void/v1".into(),
                required_capabilities: BTreeSet::new(),
            },
        );
        let errors = reconcile_interfaces(&previous, &removed).expect_err("removed API");
        assert_eq!(errors[0].kind, InterfaceIssueKind::RemovedExport);
    }

    #[test]
    fn reconciled_swap_rejection_preserves_active_artifact_and_interface() {
        let mut supervisor = ReconciledSupervisor::new();
        supervisor
            .propose_swap(
                candidate("v1", 1),
                interface("clock"),
                &origination(),
                SCHEMA,
                POLICY,
            )
            .expect("initial activation");
        let active = supervisor.active().cloned();
        let active_interface = supervisor.active_interface().cloned();
        let revision = supervisor.revision();

        let mut breaking = interface("clock");
        breaking
            .exports
            .get_mut("compute")
            .expect("compute")
            .required_capabilities
            .insert("network".into());
        let error = supervisor
            .propose_swap(candidate("v2", 1), breaking, &origination(), SCHEMA, POLICY)
            .expect_err("new authority must be rejected");
        assert_eq!(error.stage, SwapStage::Interface);
        assert_eq!(supervisor.active(), active.as_ref());
        assert_eq!(supervisor.active_interface(), active_interface.as_ref());
        assert_eq!(supervisor.revision(), revision);
    }

    #[test]
    fn reconciled_rollback_restores_matching_interface() {
        let mut supervisor = ReconciledSupervisor::new();
        let first = interface("clock");
        let mut second = interface("clock");
        second.exports.insert(
            "format".into(),
            OperationInterface {
                inputs: vec![],
                output: "goya:text/v1".into(),
                required_capabilities: BTreeSet::new(),
            },
        );
        supervisor
            .propose_swap(
                candidate("v1", 1),
                first.clone(),
                &origination(),
                SCHEMA,
                POLICY,
            )
            .expect("initial activation");
        supervisor
            .propose_swap(candidate("v2", 1), second, &origination(), SCHEMA, POLICY)
            .expect("additive update");
        supervisor.rollback().expect("rollback");
        assert_eq!(supervisor.active_interface(), Some(&first));
        assert_eq!(supervisor.active().unwrap().artifact.proof.process, "v1");
    }

    #[test]
    fn migrated_upgrade_activates_only_after_state_commit() {
        use goya_state::{Map, Store, Value};
        use state_migration::{StateMigration, StateMigrationOp};

        let mut supervisor = SwapSupervisor::new();
        supervisor
            .propose_swap(candidate("v1", 1), &origination(), SCHEMA, POLICY)
            .unwrap();
        let stored = Map::from([("count".into(), Value::Int(4))]);
        let store = Store::new();
        {
            let tx = store.begin().unwrap();
            assert!(tx.stage_snapshot(&stored));
            tx.commit(|_| true).unwrap();
        }
        let migrations = [StateMigration {
            from_version: 1,
            to_version: 2,
            operations: vec![StateMigrationOp::Rename {
                from: "count".into(),
                to: "total".into(),
            }],
        }];

        let error = supervisor
            .propose_migrated_swap(
                candidate("v2", 2),
                &stored,
                &migrations,
                &origination(),
                SCHEMA,
                POLICY,
                |_| Err("rejected"),
            )
            .expect_err("failed publish");
        assert!(matches!(error, MigratedSwapError::Commit("rejected")));
        assert_eq!(supervisor.active().unwrap().state_contract_version, 1);
        assert_eq!(store.get("count"), Some(Value::Int(4)));
        assert_eq!(store.revision(), 1);

        let mut commit_called = false;
        let error = supervisor
            .propose_migrated_swap(
                candidate("v2", 2),
                &stored,
                &[],
                &origination(),
                SCHEMA,
                POLICY,
                |_| {
                    commit_called = true;
                    Ok::<(), &str>(())
                },
            )
            .expect_err("missing migration");
        assert!(matches!(error, MigratedSwapError::Migration(_)));
        assert!(!commit_called);
        assert_eq!(supervisor.revision(), 1);

        let rev = supervisor
            .propose_migrated_swap(
                candidate("v2", 2),
                &stored,
                &migrations,
                &origination(),
                SCHEMA,
                POLICY,
                |migrated| {
                    let tx = store.begin().unwrap();
                    assert!(tx.stage_snapshot(migrated));
                    tx.commit(|_| true).map_err(|_| "commit")
                },
            )
            .expect("upgrade");
        assert_eq!(rev, 2);
        assert_eq!(supervisor.active().unwrap().state_contract_version, 2);
        assert_eq!(supervisor.active().unwrap().artifact.proof.process, "v2");
        assert_eq!(store.get("total"), Some(Value::Int(4)));
        assert_eq!(store.get("count"), None);
        assert_eq!(store.revision(), 2);
    }

    #[test]
    fn reconciled_migration_rejects_breaking_interface_before_commit() {
        use goya_state::{Map, Value};
        use state_migration::{StateMigration, StateMigrationOp};

        let mut supervisor = ReconciledSupervisor::new();
        supervisor
            .propose_swap(
                candidate("v1", 1),
                interface("clock"),
                &origination(),
                SCHEMA,
                POLICY,
            )
            .unwrap();
        let stored = Map::from([("count".into(), Value::Int(1))]);
        let migrations = [StateMigration {
            from_version: 1,
            to_version: 2,
            operations: vec![StateMigrationOp::SetDefault {
                key: "extra".into(),
                value: Value::Bool(true),
            }],
        }];
        let mut breaking = interface("clock");
        breaking.exports.remove("compute");
        let mut committed = false;
        let error = supervisor
            .propose_migrated_swap(
                candidate("v2", 2),
                breaking,
                &stored,
                &migrations,
                &origination(),
                SCHEMA,
                POLICY,
                |_| {
                    committed = true;
                    Ok::<(), &str>(())
                },
            )
            .expect_err("removed export");
        assert!(matches!(error, MigratedSwapError::Swap(_)));
        assert!(!committed);
        assert_eq!(supervisor.revision(), 1);

        let (rev, diff) = supervisor
            .propose_migrated_swap(
                candidate("v2", 2),
                interface("clock"),
                &stored,
                &migrations,
                &origination(),
                SCHEMA,
                POLICY,
                |_| Ok::<(), &str>(()),
            )
            .unwrap();
        assert_eq!(rev, 2);
        assert!(diff.retained_exports.iter().any(|name| name == "compute"));
        assert_eq!(supervisor.active().unwrap().state_contract_version, 2);
    }
}
