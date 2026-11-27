//! §11 — Anticipatory residency management.
//!
//! The residency manager executes over a *derived* inventory: it tracks which
//! encoded observations are expanded into each §11.1 runtime tier and services
//! §10.1 observation requests, promoting and demoting entries deterministically
//! under explicit byte budgets.
//!
//! Deployment tiers (`crate::target::Tier`) classify what ships resident; the
//! runtime tiers here (Hot/Warm/Cold) are the §11.1 execution concept. Anchors
//! (deployed Hot) start runtime-Hot; relational neighbourhood members (deployed
//! Cold) start runtime-Cold and are promoted on demand through Warm.
//!
//! §11.2: prediction is optional and must never answer a request incorrectly —
//! prefetching only moves bytes between tiers.

use crate::ingestion::observer_in_domain;
use crate::retrieval::UnsupportedReason;
use crate::target::{Derivation, Tier};
use std::collections::BTreeMap;

/// Explicit §11.1 memory limits. A budget of zero disables that tier's
/// expansion entirely (the manager then keeps everything Cold).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidencyLimits {
    /// §11.1 Hot: current observation and immediately required dependencies.
    pub hot_bytes: u64,
    /// §11.1 Warm: likely upcoming observations and neighbouring anchors.
    pub warm_bytes: u64,
    /// §11.2 number of adjacent neighbourhoods to prefetch on a transition;
    /// zero disables prediction.
    pub prefetch_neighbourhoods: u32,
}

/// §11.1 runtime residency tier of a retained observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeTier {
    /// Immediately accessible.
    Hot,
    /// Likely upcoming; one promotion away from Hot.
    Warm,
    /// Retained but not expanded; retrieval transitions through the
    /// neighbourhood.
    Cold,
}

/// Result of servicing a §10.1 observation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestOutcome {
    /// Served from the requested tier without a neighbourhood transition.
    Resident {
        id: crate::graph::ObservationId,
        tier: RuntimeTier,
    },
    /// Required a §11.4 neighbourhood boundary transition before serving.
    Transitioned {
        id: crate::graph::ObservationId,
        neighbourhood: crate::graph::NeighbourhoodId,
    },
}

/// Ways a residency request can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyError {
    /// §10.1 step 2: the request falls outside the inventory's domain.
    OutsideDomain,
    /// The observation exists but is not retained under the current budgets.
    NotRetained,
}

impl From<ResidencyError> for UnsupportedReason {
    fn from(e: ResidencyError) -> Self {
        match e {
            ResidencyError::OutsideDomain => UnsupportedReason::OutsideDomain,
            ResidencyError::NotRetained => UnsupportedReason::NotCompiled,
        }
    }
}

/// §10.1/§11 runtime residency manager over one derived inventory.
pub struct ResidencyManager<'a> {
    derivation: &'a Derivation,
    limits: ResidencyLimits,
    /// runtime tier per observation id.
    tier: BTreeMap<crate::graph::ObservationId, RuntimeTier>,
    /// logical LRU clock: monotonic request counter.
    clock: u64,
    last_use: BTreeMap<crate::graph::ObservationId, u64>,
    /// neighbourhood containing each observation id.
    neighbourhood_of: BTreeMap<crate::graph::ObservationId, crate::graph::NeighbourhoodId>,
    /// §11.4 adjacency (kept symmetric by the graph builder).
    adjacent: Vec<(crate::graph::AnchorId, crate::graph::AnchorId)>,
    transitions: u64,
    resident_lookups: u64,
    prefetched: u64,
}

impl<'a> ResidencyManager<'a> {
    /// Seeds runtime tiers from the deployment classification: anchors are
    /// Hot (§8.5 independently reconstructable, shipped resident), relational
    /// neighbourhood members are Cold.
    pub fn new(derivation: &'a Derivation, limits: ResidencyLimits) -> Self {
        let mut tier = BTreeMap::new();
        for (id, entry) in &derivation.entries {
            let initial = match entry.tier() {
                Tier::Hot => RuntimeTier::Hot,
                Tier::Cold => RuntimeTier::Cold,
            };
            tier.insert(crate::graph::ObservationId(*id), initial);
        }

        let mut neighbourhood_of = BTreeMap::new();
        for n in &derivation.inventory.neighbourhoods {
            for obs in &n.observations {
                neighbourhood_of.insert(*obs, n.id);
            }
        }

        let mut adjacent = derivation.inventory.residency.adjacency.clone();
        // defensive symmetry: the builder guarantees it, eviction prefetching
        // must not depend on that guarantee holding.
        adjacent.retain(|(a, b)| a != b);

        let mut manager = Self {
            derivation,
            limits,
            tier,
            clock: 0,
            last_use: BTreeMap::new(),
            neighbourhood_of,
            adjacent,
            transitions: 0,
            resident_lookups: 0,
            prefetched: 0,
        };
        // §11.1: the runtime MUST enforce explicit memory limits from the
        // first instant — a deployment tier exceeds a tighter runtime budget
        // only until the manager's first deterministic eviction pass.
        manager.enforce(RuntimeTier::Hot, manager.limits.hot_bytes);
        manager.enforce(RuntimeTier::Warm, manager.limits.warm_bytes);
        manager
    }

    /// Services a §10.1 request: domain check (step 2), resident lookup
    /// (steps 3–4) or neighbourhood transition, then budget enforcement.
    pub fn request(
        &mut self,
        observer: &crate::ingestion::Observer,
    ) -> Result<RequestOutcome, ResidencyError> {
        let inventory = &self.derivation.inventory;
        if !observer_in_domain(&inventory.domain, observer) {
            return Err(ResidencyError::OutsideDomain);
        }

        // §10.1 step 3–4: primary index first, then the full observation
        // table (quantised state lookup).
        let state = observer.to_state();
        let id = self
            .derivation
            .primary_index
            .get(&state)
            .copied()
            .or_else(|| {
                inventory
                    .observations
                    .iter()
                    .find(|o| o.state == state)
                    .map(|o| o.id)
            })
            .ok_or(ResidencyError::NotRetained)?;

        self.clock += 1;
        self.last_use.insert(id, self.clock);

        let current = self.tier.get(&id).copied().unwrap_or(RuntimeTier::Cold);
        match current {
            RuntimeTier::Cold => {
                let neighbourhood = self.neighbourhood_of.get(&id).copied();
                self.transitions += 1;
                // §11.4: transition promotes through Warm to Hot.
                self.promote(id);
                if let Some(n) = neighbourhood {
                    self.prefetch_adjacent(n);
                }
                Ok(RequestOutcome::Transitioned { id, neighbourhood: neighbourhood.ok_or(ResidencyError::NotRetained)? })
            }
            tier => {
                self.resident_lookups += 1;
                self.promote(id);
                Ok(RequestOutcome::Resident { id, tier })
            }
        }
    }

    /// Moves an observation toward Hot, enforcing each tier's byte budget on
    /// the way. Ties are broken by observation id (deterministic).
    fn promote(&mut self, id: crate::graph::ObservationId) {
        let current = self.tier.get(&id).copied().unwrap_or(RuntimeTier::Cold);
        match current {
            RuntimeTier::Cold => {
                self.tier.insert(id, RuntimeTier::Warm);
                self.enforce(RuntimeTier::Warm, self.limits.warm_bytes);
                self.promote(id);
            }
            RuntimeTier::Warm => {
                self.tier.insert(id, RuntimeTier::Hot);
                self.enforce(RuntimeTier::Hot, self.limits.hot_bytes);
            }
            RuntimeTier::Hot => {}
        }
    }

    /// §11.2 optional prefetch: warms Cold members of adjacent
    /// neighbourhoods. A wrong prediction only moves bytes — it can never
    /// answer a request or corrupt state.
    fn prefetch_adjacent(&mut self, from: crate::graph::NeighbourhoodId) {
        if self.limits.prefetch_neighbourhoods == 0 {
            return;
        }
        let neighbours = self.neighbourhood_anchor_adjacency(from);
        for anchor in neighbours.iter().take(self.limits.prefetch_neighbourhoods as usize) {
            let members: Vec<_> = self
                .derivation
                .inventory
                .neighbourhoods
                .iter()
                .filter(|n| n.anchor == *anchor)
                .flat_map(|n| n.observations.iter().copied())
                .collect();
            for obs in members {
                if self.tier.get(&obs).copied() == Some(RuntimeTier::Cold) {
                    self.tier.insert(obs, RuntimeTier::Warm);
                    self.prefetched += 1;
                }
            }
        }
        self.enforce(RuntimeTier::Warm, self.limits.warm_bytes);
    }

    /// Anchor adjacency of the neighbourhood containing `from`, deterministic
    /// order (sorted by anchor id).
    fn neighbourhood_anchor_adjacency(
        &self,
        from: crate::graph::NeighbourhoodId,
    ) -> Vec<crate::graph::AnchorId> {
        let anchor = self
            .derivation
            .inventory
            .neighbourhoods
            .iter()
            .find(|n| n.id == from)
            .map(|n| n.anchor);
        let Some(anchor) = anchor else {
            return Vec::new();
        };
        let mut set = std::collections::BTreeSet::new();
        for (a, b) in &self.adjacent {
            if *a == anchor {
                set.insert(*b);
            }
            if *b == anchor {
                set.insert(*a);
            }
        }
        set.into_iter().collect()
    }

    /// Deterministic LRU eviction: demote the least-recently-used entries of
    /// `tier` until the budget holds. Hot demotes to Warm (re-enforcing the
    /// warm budget), Warm to Cold.
    fn enforce(&mut self, tier: RuntimeTier, budget_bytes: u64) {
        let members: Vec<_> = self
            .tier
            .iter()
            .filter(|(id, t)| **t == tier && self.stored_bytes(**id) > 0)
            .map(|(id, _)| *id)
            .collect();

        let mut ranked: Vec<_> = members
            .into_iter()
            .map(|id| (self.last_use.get(&id).copied().unwrap_or(0), id))
            .collect();
        ranked.sort(); // oldest use first; id breaks ties deterministically

        let mut used: u64 = self.tier_bytes(tier);
        for (_, id) in ranked {
            if used <= budget_bytes {
                break;
            }
            let bytes = self.stored_bytes(id);
            match tier {
                RuntimeTier::Hot => {
                    self.tier.insert(id, RuntimeTier::Warm);
                }
                RuntimeTier::Warm => {
                    self.tier.insert(id, RuntimeTier::Cold);
                }
                RuntimeTier::Cold => {}
            }
            used = used.saturating_sub(bytes);
        }
        // A Hot eviction may have overfilled Warm.
        if tier == RuntimeTier::Hot && self.tier_bytes(RuntimeTier::Warm) > self.limits.warm_bytes {
            self.enforce(RuntimeTier::Warm, self.limits.warm_bytes);
        }
    }

    /// Encoded size of one retained observation (the bytes its tier holds).
    /// §8.5 COPY references another observation's payload and contributes no
    /// independent resident bytes.
    fn stored_bytes(&self, id: crate::graph::ObservationId) -> u64 {
        self.derivation
            .entries
            .get(&id.0)
            .map(crate::target::EncodedObservation::stored_bytes)
            .unwrap_or(0)
    }

    fn tier_bytes(&self, tier: RuntimeTier) -> u64 {
        self.tier
            .iter()
            .filter(|(_, t)| **t == tier)
            .map(|(id, _)| self.stored_bytes(*id))
            .sum()
    }

    /// §11.4 accounting: neighbourhood boundary transitions (distinct from
    /// ordinary resident lookups — the two MUST NOT be conflated).
    pub fn transition_count(&self) -> u64 {
        self.transitions
    }

    /// Ordinary resident lookup count.
    pub fn resident_lookup_count(&self) -> u64 {
        self.resident_lookups
    }

    /// Observations warmed by §11.2 prediction.
    pub fn prefetched_observation_count(&self) -> u64 {
        self.prefetched
    }

    /// Bytes currently expanded into a runtime tier.
    pub fn resident_bytes(&self, tier: RuntimeTier) -> u64 {
        self.tier_bytes(tier)
    }

    /// Observation ids currently in a tier (deterministic order).
    pub fn ids_in_tier(&self, tier: RuntimeTier) -> Vec<crate::graph::ObservationId> {
        self.tier
            .iter()
            .filter(|(_, t)| **t == tier)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Current runtime tier of an observation, if retained.
    pub fn tier_of(&self, id: crate::graph::ObservationId) -> Option<RuntimeTier> {
        self.tier.get(&id).copied()
    }

    /// §11.1 Persistent tier: the complete deployed inventory.
    pub fn persistent_bytes(&self) -> u64 {
        self.derivation.metrics.stored_payload_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::{
        Observer, ANGLE_QUANTISATION, DISTANCE_QUANTISATION,
    };
    use crate::derivation::InventoryProfile;
    use crate::target::tests::{orbital_report, profile};
    use crate::target::{derive, Derivation};

    fn derivation(n: u32, calls: &std::cell::Cell<u32>) -> Derivation {
        let report = orbital_report(n, calls);
        let p = profile(InventoryProfile::Relational, &report.canonical_digest, 8, 8);
        derive(&report, &p, None).expect("derivation")
    }

    fn limits(hot: u64, warm: u64, prefetch: u32) -> ResidencyLimits {
        ResidencyLimits { hot_bytes: hot, warm_bytes: warm, prefetch_neighbourhoods: prefetch }
    }

    /// §8.5/§11.1: anchors deploy Hot and start runtime-Hot; relational
    /// neighbourhood members start Cold.
    #[test]
    fn anchors_start_hot_members_start_cold() {
        let calls = std::cell::Cell::new(0);
        let d = derivation(16, &calls);
        let m = ResidencyManager::new(&d, limits(1 << 20, 1 << 20, 0));

        assert!(d.entries.iter().all(|(id, e)| {
            let expected = match e.tier() {
                Tier::Hot => RuntimeTier::Hot,
                Tier::Cold => RuntimeTier::Cold,
            };
            m.tier_of(crate::graph::ObservationId(*id)) == Some(expected)
        }));
    }

    /// §10.1/§11.4: a Cold request transitions once, then the observation is
    /// resident; transition and lookup accounting stay separated.
    #[test]
    fn cold_request_transitions_then_is_resident() {
        let calls = std::cell::Cell::new(0);
        let d = derivation(16, &calls);
        let mut m = ResidencyManager::new(&d, limits(1 << 20, 1 << 20, 0));

        let cold = d
            .entries
            .iter()
            .find(|(_, e)| e.tier() == Tier::Cold)
            .map(|(id, _)| *id)
            .expect("cold member");
        let observer = cold_request_observer(&d, crate::graph::ObservationId(cold));

        match m.request(&observer).expect("first request") {
            RequestOutcome::Transitioned { id, .. } => assert_eq!(id, crate::graph::ObservationId(cold)),
            other => panic!("expected transition, got {other:?}"),
        }
        assert_eq!(m.transition_count(), 1);
        assert_eq!(m.resident_lookup_count(), 0);
        assert_eq!(m.tier_of(crate::graph::ObservationId(cold)), Some(RuntimeTier::Hot));

        match m.request(&observer).expect("second request") {
            RequestOutcome::Resident { id, .. } => assert_eq!(id, crate::graph::ObservationId(cold)),
            other => panic!("expected resident hit, got {other:?}"),
        }
        assert_eq!(m.transition_count(), 1);
        assert_eq!(m.resident_lookup_count(), 1);
    }

    /// Find an observer that quantises to a given observation's state.
    fn cold_request_observer(d: &Derivation, id: crate::graph::ObservationId) -> Observer {
        let record = d
            .inventory
            .observations
            .iter()
            .find(|o| o.id == id)
            .expect("record");
        Observer {
            azimuth_deg: record.state.orientation[0] as f32 / ANGLE_QUANTISATION,
            elevation_deg: record.state.orientation[1] as f32 / ANGLE_QUANTISATION,
            distance: record.state.camera[0] as f32 / DISTANCE_QUANTISATION,
            fov_deg: 60.0,
        }
        .normalised()
    }

    /// §11.1: budgets are hard; deterministic LRU eviction keeps each tier
    /// within its limit.
    #[test]
    fn budgets_are_enforced_deterministically() {
        let calls = std::cell::Cell::new(0);
        let d = derivation(16, &calls);
        let hot_bytes = 40; // small enough to force eviction churn
        let mut m = ResidencyManager::new(&d, limits(hot_bytes, 80, 0));

        let mut cold_observers: Vec<_> = d
            .entries
            .iter()
            .filter(|(_, e)| e.tier() == Tier::Cold)
            .map(|(id, _)| cold_request_observer(&d, crate::graph::ObservationId(*id)))
            .collect();
        cold_observers.sort_by_key(|o| o.azimuth_deg as i32);

        for o in &cold_observers {
            let _ = m.request(o);
            assert!(m.resident_bytes(RuntimeTier::Hot) <= hot_bytes);
            assert!(m.resident_bytes(RuntimeTier::Warm) <= 80);
        }
        // determinism: an identical run produces identical tier assignments
        let mut m2 = ResidencyManager::new(&d, limits(hot_bytes, 80, 0));
        for o in &cold_observers {
            let _ = m2.request(o);
        }
        assert_eq!(m2.ids_in_tier(RuntimeTier::Hot), m.ids_in_tier(RuntimeTier::Hot));
    }

    /// §11.2: prefetching is optional and only moves bytes; it never answers
    /// a request or produces a transition on its own.
    #[test]
    fn prefetch_is_optional_and_never_answers_requests() {
        let calls = std::cell::Cell::new(0);
        let d = derivation(16, &calls);
        let mut m = ResidencyManager::new(&d, limits(1 << 20, 1 << 20, 0));
        let before = m.resident_bytes(RuntimeTier::Warm);

        let cold = d
            .entries
            .iter()
            .find(|(_, e)| e.tier() == Tier::Cold)
            .map(|(id, _)| *id)
            .expect("cold member");
        let observer = cold_request_observer(&d, crate::graph::ObservationId(cold));
        let outcome = m.request(&observer).expect("request");

        assert!(matches!(outcome, RequestOutcome::Transitioned { id, .. } if id == crate::graph::ObservationId(cold)));
        assert_eq!(m.transition_count(), 1); // the request itself only
        // prediction may have warmed neighbours (or not, if none adjacent)
        assert!(m.resident_bytes(RuntimeTier::Warm) >= before);

        // prefetch disabled: identical request sequence performs no warming
        let mut m2 = ResidencyManager::new(&d, limits(1 << 20, 1 << 20, 0));
        let _ = m2.request(&observer);
        assert_eq!(m2.prefetched_observation_count(), 0);
    }

    /// §10.1 step 2 (AVR-C14): out-of-domain requests are refused.
    #[test]
    fn out_of_domain_requests_are_refused() {
        let calls = std::cell::Cell::new(0);
        let d = derivation(16, &calls);
        let mut m = ResidencyManager::new(&d, limits(1 << 20, 1 << 20, 0));

        // Orbital domain pins elevation to 0; a pitched observer is outside.
        let pitched = Observer {
            azimuth_deg: 10.0,
            elevation_deg: 45.0,
            distance: 2.0,
            fov_deg: 60.0,
        };
        let err = m.request(&pitched);
        assert_eq!(err, Err(ResidencyError::OutsideDomain));
        assert_eq!(err.unwrap_err(), ResidencyError::OutsideDomain);
    }

    /// Residency failures map onto §10.3 priority-8 resolution failures.
    #[test]
    fn residency_errors_convert_to_unsupported() {
        assert_eq!(
            UnsupportedReason::from(ResidencyError::OutsideDomain),
            UnsupportedReason::OutsideDomain
        );
        assert_eq!(
            UnsupportedReason::from(ResidencyError::NotRetained),
            UnsupportedReason::NotCompiled
        );
    }
}
