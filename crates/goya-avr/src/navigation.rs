//! Navigation-graph environments (directive §5.3, §6.2, §8.3 — Phase 6).
//!
//! An environmental viewport observes an *environment*: a bounded set of
//! discrete camera nodes connected by a navigation graph. Each node is an
//! observation site whose views share the node's spatial state
//! (`ObservationState::node`); navigation edges (§8.3) record the
//! transitions the developer authorises between nodes.
//!
//! Boundedness (§4.4): an environment is finite by construction — the
//! [`NavigationSpec`] declares a fixed node count and the compilation
//! renders exactly `nodes × views` observations under the budget. Nothing
//! here enumerates a continuous domain.
//!
//! Rugid currently has no environment/camera-node substrate (ADR-AVR-001
//! audit F1), so the environmental source contract is AVR-side; the Rugid
//! bridge simulates node movement with model offsets.

use crate::canonical::{CanonicalStore, CompilationBudget, CompileReport};
use crate::domain::{ObservationDomain, ObservationState};
use crate::graph::{NavigationGraph, ObservationId};
use crate::ingestion::{
    quantise_angle, quantise_distance, ObservationPayload, Observer, SourceError,
};
use crate::inventory::{FidelityClass, FidelityRecord, ObservationRecord, RepresentationId};
use crate::retrieval::{resolve_derived_state, DerivedResolution};
use std::collections::BTreeSet;

/// Navigation graph for one environment (§6.2 "Navigation graph" domain,
/// §8.3 navigation graph).
///
/// Rooms partition the nodes into contiguous ranges (multi-room
/// environments, §22 Phase 6); edges are undirected navigation
/// transitions. There is no `Default`: an environment's shape is a
/// developer decision, not an implementation convenience.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationSpec {
    /// Node count of each room, in order. Node ids are assigned
    /// contiguously: room 0 owns `0..rooms[0]`, room 1 the next range, …
    pub rooms: Vec<u32>,
    /// Undirected navigation edges `(a, b)`, `a != b`, over global node ids.
    pub edges: Vec<(u32, u32)>,
}

/// Why a navigation specification was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationSpecError {
    /// An environment must contain at least one room.
    NoRooms,
    /// A room declared zero nodes (§4.4: finite, non-empty domains).
    EmptyRoom { room: u32 },
    /// An edge referenced a node outside `0..node_count` (AVR-C03).
    EdgeOutOfRange { edge: (u32, u32), nodes: u32 },
    /// A navigation edge from a node to itself.
    SelfLoop { node: u32 },
    /// The same transition was declared twice.
    DuplicateEdge { edge: (u32, u32) },
}

impl NavigationSpec {
    pub fn node_count(&self) -> u32 {
        self.rooms.iter().sum()
    }

    /// Room index owning `node`, or `None` out of range.
    pub fn room_of(&self, node: u32) -> Option<u32> {
        let mut accumulated = 0u32;
        for (room, count) in self.rooms.iter().enumerate() {
            accumulated += count;
            if node < accumulated {
                return Some(room as u32);
            }
        }
        None
    }

    /// Edges crossing a room boundary (§22 Phase 6 "multi-room").
    pub fn door_edges(&self) -> Vec<(u32, u32)> {
        self.edges
            .iter()
            .filter(|(a, b)| self.room_of(*a) != self.room_of(*b))
            .copied()
            .collect()
    }

    /// Validates the structural contract before any rendering happens.
    pub fn validate(&self) -> Result<(), NavigationSpecError> {
        if self.rooms.is_empty() {
            return Err(NavigationSpecError::NoRooms);
        }
        for (room, count) in self.rooms.iter().enumerate() {
            if *count == 0 {
                return Err(NavigationSpecError::EmptyRoom { room: room as u32 });
            }
        }
        let nodes = self.node_count();
        let mut seen = BTreeSet::new();
        for &(a, b) in &self.edges {
            if a >= nodes || b >= nodes {
                return Err(NavigationSpecError::EdgeOutOfRange { edge: (a, b), nodes });
            }
            if a == b {
                return Err(NavigationSpecError::SelfLoop { node: a });
            }
            let key = (a.min(b), a.max(b));
            if !seen.insert(key) {
                return Err(NavigationSpecError::DuplicateEdge { edge: key });
            }
        }
        Ok(())
    }

    /// Node-level adjacency, both directions, deterministic order.
    pub fn adjacency(&self) -> BTreeSet<(u32, u32)> {
        let mut out = BTreeSet::new();
        for &(a, b) in &self.edges {
            out.insert((a.min(b), a.max(b)));
            out.insert((a.max(b), a.min(b)));
        }
        out
    }
}

/// An environment AVR can observe: discrete camera nodes, each rendering
/// views of the local spatial content (§5.3).
///
/// The Rugid bridge implements this over model offsets; a future Rugid
/// environment substrate implements it natively. Rendering must be a pure
/// function of `(node, view)` — the canonical payload identity depends on
/// it (§7.3).
pub trait EnvironmentalSource {
    /// Digest of the whole environment definition: room layout, node
    /// placement and spatial content (§7.3 source asset digest).
    fn source_digest(&self) -> String;

    /// Render one view from camera node `node`.
    fn render_node(&self, node: u32, view: &Observer) -> Result<ObservationPayload, SourceError>;
}

/// A requested environmental observation: camera node plus the view taken
/// from it (§10.1 "receive requested observation").
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationRequest {
    pub node: u32,
    pub view: Observer,
}

impl NavigationRequest {
    /// Quantised observation state, including the navigation node
    /// (`ObservationState::node`) — the axis conventional orbit requests
    /// leave `None`.
    pub fn to_state(&self) -> ObservationState {
        let v = self.view.normalised();
        ObservationState {
            camera: [quantise_distance(v.distance), 0, 0],
            orientation: [quantise_angle(v.azimuth_deg), quantise_angle(v.elevation_deg), 0],
            zoom: None,
            node: Some(self.node),
            material_revision: 0,
            environment_revision: 0,
        }
    }
}

/// Why a navigation compilation failed outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationCompileError {
    /// The specification violated the structural contract.
    Spec(NavigationSpecError),
    /// The domain bound rejected the observation count (§6.4 step 8).
    BudgetExhausted { requested: u32, allowed: u32 },
    /// The source failed to render a node view.
    Source(SourceError),
}

/// Views per node: the orbital ring azimuths at the policy's distance,
/// deterministic ascending order. Environments are observed orbitally at
/// each node in Phase 6; elevation variation arrives with a Rugid
/// environment substrate.
fn node_views(policy: &crate::sampling::SamplingPolicy) -> Vec<Observer> {
    let n = policy.initial_samples;
    (0..n)
        .map(|i| Observer {
            azimuth_deg: (i as f32) * 360.0 / n as f32,
            elevation_deg: 0.0,
            distance: policy.distance,
            fov_deg: policy.fov_deg,
        })
        .collect()
}

/// Observation count of one node inside the compiled inventory.
pub fn observations_per_node(report: &CompileReport) -> u32 {
    match report.inventory.domain {
        ObservationDomain::NavigationGraph { nodes } if nodes > 0 => {
            report.inventory.observations.len() as u32 / nodes
        }
        _ => 0,
    }
}

/// Compile the canonical observation inventory of a navigation environment
/// (§6.4, §7): every node is rendered over its orbital view ring, in
/// deterministic node-major order, with navigation edges materialised into
/// the inventory's navigation graph (§8.3) as transitions between the
/// nodes' representative observations.
pub fn compile_navigation(
    source: &dyn EnvironmentalSource,
    spec: &NavigationSpec,
    policy: &crate::sampling::SamplingPolicy,
    budget: &CompilationBudget,
) -> Result<CompileReport, NavigationCompileError> {
    spec.validate().map_err(NavigationCompileError::Spec)?;

    let views = node_views(policy);
    let nodes = spec.node_count();
    let requested = nodes * views.len() as u32;
    if requested > budget.max_observations {
        return Err(NavigationCompileError::BudgetExhausted {
            requested,
            allowed: budget.max_observations,
        });
    }

    let domain = ObservationDomain::NavigationGraph { nodes };
    let mut records = Vec::with_capacity(requested as usize);
    let mut store = CanonicalStore::default();
    let mut payload_digests = Vec::with_capacity(requested as usize);
    let mut total_bytes: u64 = 0;

    let mut index = 0u64;
    for node in 0..nodes {
        for view in &views {
            let payload = source.render_node(node, view).map_err(NavigationCompileError::Source)?;
            total_bytes += payload.bytes.len() as u64;
            if total_bytes > budget.max_payload_bytes {
                return Err(NavigationCompileError::BudgetExhausted {
                    requested,
                    allowed: budget.max_observations,
                });
            }
            payload_digests.push(payload.digest());
            store.insert(index, payload);
            records.push(ObservationRecord {
                id: ObservationId(index),
                state: NavigationRequest { node, view: *view }.to_state(),
                representation: RepresentationId(0),
                provenance: None,
                fidelity: FidelityRecord {
                    classification: FidelityClass::Exact,
                    error_bounds: [0; 4],
                    verification_method: "direct canonical render",
                    coverage: crate::graph::ObservationCoordinates {
                        spatial: [
                            i64::from(node),
                            view.azimuth_deg.to_bits() as i64,
                            view.distance.to_bits() as i64,
                        ],
                    },
                },
            });
            index += 1;
        }
    }

    // §8.3: navigation edges are authorised transitions between nodes,
    // materialised as edges between each node's representative (first)
    // observation. Navigation cycles are permitted; reconstruction
    // acyclicity is a different graph (AVR-C04) and is untouched here.
    let mut edges = Vec::new();
    for &(a, b) in &spec.edges {
        if let (Some(fa), Some(fb)) =
            (node_first_observation_of(&records, a), node_first_observation_of(&records, b))
        {
            edges.push((fa, fb));
            edges.push((fb, fa));
        }
    }

    // §7.3 canonical identity: environment digest + spec + policy + every
    // payload digest.
    let digest =
        crate::canonical::navigation_digest(&source.source_digest(), spec, policy, &payload_digests);

    let inventory = crate::inventory::ObservationInventory::new(
        digest.clone(),
        domain,
        records,
        NavigationGraph { edges },
        Vec::new(),
        crate::graph::ResidencyGraph::default(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        crate::graph::MAX_DEPENDENCY_DEPTH,
    )
    .expect("navigation compilation constructs a well-formed inventory");

    Ok(CompileReport {
        inventory,
        store,
        complete: true,
        requested,
        generated: requested,
        canonical_digest: digest,
    })
}

fn node_first_observation_of(records: &[ObservationRecord], node: u32) -> Option<ObservationId> {
    records
        .iter()
        .find(|r| r.state.node == Some(node))
        .map(|r| r.id)
}

/// Resolve a navigation request against a derived inventory (§10.1).
/// The node must exist in the environment's domain (AVR-C14); the view
/// resolves through the same hierarchy as any other request.
pub fn resolve_navigation(
    derivation: &crate::target::Derivation,
    request: &NavigationRequest,
    engine: Option<&dyn crate::engine::ReconstructionEngine>,
) -> DerivedResolution {
    let nodes = match derivation.inventory.domain {
        ObservationDomain::NavigationGraph { nodes } => nodes,
        _ => {
            return DerivedResolution::Unsupported(crate::retrieval::UnsupportedReason::OutsideDomain)
        }
    };
    if request.node >= nodes {
        return DerivedResolution::Unsupported(crate::retrieval::UnsupportedReason::OutsideDomain);
    }
    if request.view.normalised().elevation_deg != 0.0 {
        // Phase 6 environments observe orbitally per node; a pitched view
        // is outside the compiled domain, never silently flattened.
        return DerivedResolution::Unsupported(crate::retrieval::UnsupportedReason::OutsideDomain);
    }
    resolve_derived_state(derivation, &request.to_state(), engine)
}

/// Node-level adjacency reachable from a compiled or derived inventory's
/// navigation graph (§8.3: navigation transitions; §10.1 runtime can move
/// along them). Returns the global node ids adjacent to `node`.
pub fn adjacent_nodes(edges: &[(ObservationId, ObservationId)], node: u32, per_node: u32) -> BTreeSet<u32> {
    if per_node == 0 {
        return BTreeSet::new();
    }
    let first = u64::from(node) * u64::from(per_node);
    edges
        .iter()
        .filter_map(|(a, b)| {
            if a.0 == first {
                Some((b.0 / u64::from(per_node)) as u32)
            } else if b.0 == first {
                Some((a.0 / u64::from(per_node)) as u32)
            } else {
                None
            }
        })
        .collect()
}

/// §7.4 incremental invalidation for navigation environments: which
/// compiled observations a modification forces to regenerate. Unchanged
/// observations stay reusable (§7.4's reuse table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentModification {
    /// A node's spatial content changed: that node's observations.
    NodeGeometryChanged { node: u32 },
    /// Material/appearance change at a revision: observations rendered
    /// against that material revision.
    MaterialChanged { revision: u64 },
    /// Lighting/simulation change at a revision: observations rendered
    /// against that environment revision.
    EnvironmentChanged { revision: u64 },
}

/// Observation ids invalidated by `modification` (deterministic order).
pub fn invalidated_observations(
    report: &CompileReport,
    modification: &EnvironmentModification,
) -> Vec<ObservationId> {
    report
        .inventory
        .observations
        .iter()
        .filter(|record| match modification {
            EnvironmentModification::NodeGeometryChanged { node } => {
                record.state.node == Some(*node)
            }
            EnvironmentModification::MaterialChanged { revision } => {
                record.state.material_revision == *revision
            }
            EnvironmentModification::EnvironmentChanged { revision } => {
                record.state.environment_revision == *revision
            }
        })
        .map(|record| record.id)
        .collect()
}

/// Certification failures for navigation inventories (WS-10 slice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationCertificationError {
    /// AVR-C02: a declared node compiled no observations.
    NodeNotCompiled { node: u32 },
    /// AVR-C03: a navigation edge references an unknown observation.
    EdgeReferencesUnknownObservation { edge: (ObservationId, ObservationId) },
    /// AVR-C06: a room is not internally connected.
    RoomDisconnected { room: u32 },
    /// AVR-C06: a multi-room environment's rooms are not all reachable
    /// through door edges.
    EnvironmentDisconnected,
}

/// Certify a compiled navigation environment (§15, WS-10 slice):
/// AVR-C02 reachable-node completeness, AVR-C03 valid navigation
/// references, AVR-C06 room and environment connectivity. Structural
/// validation never executes source code; it only reads the compiled
/// inventory and the declared spec.
pub fn certify_navigation(
    report: &CompileReport,
    spec: &NavigationSpec,
) -> Result<(), NavigationCertificationError> {
    let nodes = match report.inventory.domain {
        ObservationDomain::NavigationGraph { nodes } => nodes,
        _ => {
            return Err(NavigationCertificationError::NodeNotCompiled { node: 0 });
        }
    };

    // AVR-C02: every declared node compiled at least one observation.
    let mut compiled = vec![false; nodes as usize];
    for record in &report.inventory.observations {
        if let Some(node) = record.state.node {
            if (node as usize) < compiled.len() {
                compiled[node as usize] = true;
            }
        }
    }
    let uncompiled = compiled.iter().position(|c| !c);
    if let Some(node) = uncompiled {
        return Err(NavigationCertificationError::NodeNotCompiled { node: node as u32 });
    }

    // AVR-C03: navigation edges reference existing observation ids.
    let known: BTreeSet<u64> =
        report.inventory.observations.iter().map(|r| r.id.0).collect();
    for &(a, b) in &report.inventory.navigation.edges {
        if !known.contains(&a.0) || !known.contains(&b.0) {
            return Err(NavigationCertificationError::EdgeReferencesUnknownObservation { edge: (a, b) });
        }
    }

    // AVR-C06: connectivity, room-locally and environment-wide, computed
    // over the declared spec (the authoritative transition intent).
    let adjacency = spec.adjacency();
    let per_room: Vec<(u32, u32)> = {
        let mut ranges = Vec::with_capacity(spec.rooms.len());
        let mut start = 0u32;
        for count in &spec.rooms {
            ranges.push((start, start + count));
            start += count;
        }
        ranges
    };
    for (room, (start, end)) in per_room.iter().enumerate() {
        if !connected((*start..*end).collect(), &adjacency, *start) {
            return Err(NavigationCertificationError::RoomDisconnected { room: room as u32 });
        }
    }
    // Environment-wide: BFS from node 0 must reach every room's first node.
    let all: Vec<u32> = (0..nodes).collect();
    if !connected(all, &adjacency, 0) {
        return Err(NavigationCertificationError::EnvironmentDisconnected);
    }

    Ok(())
}

/// Reachability over the undirected adjacency from `start`.
fn connected(nodes: Vec<u32>, adjacency: &BTreeSet<(u32, u32)>, start: u32) -> bool {
    if nodes.is_empty() {
        return true;
    }
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut queue = vec![start];
    seen.insert(start);
    while let Some(current) = queue.pop() {
        for (a, b) in adjacency.iter() {
            let next = if *a == current {
                Some(*b)
            } else if *b == current {
                Some(*a)
            } else {
                None
            };
            if let Some(next) = next {
                if nodes.contains(&next) && seen.insert(next) {
                    queue.push(next);
                }
            }
        }
    }
    seen.len() == nodes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::CompilationBudget;
    use crate::representation::RepresentationClass;
    use crate::sampling::SamplingPolicy;
    use crate::target::derive;
    use std::cell::Cell;

    /// Environment standing in for the Rugid bridge: renders per-node
    /// payloads and counts render calls (§4.1 single-compilation checks).
    struct FakeEnvironment {
        nodes: u32,
        render_calls: Cell<u32>,
    }

    impl EnvironmentalSource for FakeEnvironment {
        fn source_digest(&self) -> String {
            format!("fake-env:{:02x}", self.nodes)
        }
        fn render_node(&self, node: u32, view: &Observer) -> Result<ObservationPayload, SourceError> {
            self.render_calls.set(self.render_calls.get() + 1);
            let v = view.normalised();
            Ok(ObservationPayload {
                representation: RepresentationClass::Vec,
                bytes: format!("NODE {node} AZ {:.0} D {:.0}", v.azimuth_deg, v.distance).into_bytes(),
            })
        }
    }

    fn policy(views: u32) -> SamplingPolicy {
        SamplingPolicy { initial_samples: views, distance: 2.0, fov_deg: 60.0 }
    }

    fn budget(obs: u32) -> CompilationBudget {
        CompilationBudget { max_observations: obs, max_payload_bytes: u64::MAX }
    }

    fn corridor_spec() -> NavigationSpec {
        // Two rooms of four nodes, joined by a door edge 3–4.
        NavigationSpec {
            rooms: vec![4, 4],
            edges: vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7)],
        }
    }

    #[test]
    fn spec_validation_rejects_structural_violations() {
        assert_eq!(NavigationSpec { rooms: vec![], edges: vec![] }.validate(),
            Err(NavigationSpecError::NoRooms));
        assert_eq!(NavigationSpec { rooms: vec![2, 0], edges: vec![] }.validate(),
            Err(NavigationSpecError::EmptyRoom { room: 1 }));
        assert_eq!(NavigationSpec { rooms: vec![2], edges: vec![(0, 5)] }.validate(),
            Err(NavigationSpecError::EdgeOutOfRange { edge: (0, 5), nodes: 2 }));
        assert_eq!(NavigationSpec { rooms: vec![2], edges: vec![(1, 1)] }.validate(),
            Err(NavigationSpecError::SelfLoop { node: 1 }));
        assert_eq!(NavigationSpec { rooms: vec![2], edges: vec![(0, 1), (1, 0)] }.validate(),
            Err(NavigationSpecError::DuplicateEdge { edge: (0, 1) }));
    }

    #[test]
    fn rooms_partition_nodes_and_doors_cross_them() {
        let spec = corridor_spec();
        assert_eq!(spec.node_count(), 8);
        assert_eq!(spec.room_of(0), Some(0));
        assert_eq!(spec.room_of(3), Some(0));
        assert_eq!(spec.room_of(4), Some(1));
        assert_eq!(spec.room_of(7), Some(1));
        assert_eq!(spec.room_of(8), None);
        assert_eq!(spec.door_edges(), vec![(3, 4)]);
    }

    #[test]
    fn navigation_compilation_covers_every_node_and_materialises_edges() {
        let env = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let report =
            compile_navigation(&env, &corridor_spec(), &policy(4), &budget(64)).expect("compiles");
        assert_eq!(report.inventory.observations.len(), 32);
        assert!(report.complete);
        // §4.1: the environment was rendered exactly once per observation.
        assert_eq!(env.render_calls.get(), 32);
        // Every node's state carries the node axis (§6.1 independently
        // variable spatial state).
        for node in 0..8u32 {
            assert!(report.inventory.observations.iter().any(|o| o.state.node == Some(node)));
        }
        // §8.3: authorised transitions exist between each node pair's
        // representative observations, both directions.
        let per_node = 4;
        for &(a, b) in &corridor_spec().edges {
            let fa = u64::from(a) * per_node;
            let fb = u64::from(b) * per_node;
            assert!(report.inventory.navigation.edges.contains(&(ObservationId(fa), ObservationId(fb))));
            assert!(report.inventory.navigation.edges.contains(&(ObservationId(fb), ObservationId(fa))));
        }
        // The navigation graph may contain cycles (§8.3) — construct one
        // deliberately and confirm the inventory accepts it.
        let cyclic = NavigationSpec { rooms: vec![3], edges: vec![(0, 1), (1, 2), (2, 0)] };
        let cyclic_report = compile_navigation(&env, &cyclic, &policy(2), &budget(16)).expect("cyclic");
        assert_eq!(cyclic_report.inventory.navigation.edges.len(), 6);
    }

    #[test]
    fn navigation_compilation_is_deterministic() {
        let env = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let a = compile_navigation(&env, &corridor_spec(), &policy(4), &budget(64)).expect("a");
        let env2 = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let b = compile_navigation(&env2, &corridor_spec(), &policy(4), &budget(64)).expect("b");
        assert_eq!(a.canonical_digest, b.canonical_digest);
        assert_eq!(a.store.payload_bytes(), b.store.payload_bytes());
        assert_eq!(a.store.len(), b.store.len());
        for i in 0..a.store.len() as u64 {
            assert_eq!(a.store.get(ObservationId(i)), b.store.get(ObservationId(i)));
        }
    }

    #[test]
    fn budget_exhaustion_is_explicit() {
        let env = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let err = compile_navigation(&env, &corridor_spec(), &policy(4), &budget(31))
            .expect_err("over budget");
        assert_eq!(err, NavigationCompileError::BudgetExhausted { requested: 32, allowed: 31 });
        assert_eq!(env.render_calls.get(), 0, "budget checked before any rendering");
    }

    #[test]
    fn navigation_requests_resolve_exactly_and_refuse_outside_the_domain() {
        let env = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let report =
            compile_navigation(&env, &corridor_spec(), &policy(4), &budget(64)).expect("compiles");
        let p = crate::derivation::DerivationProfile {
            profile: crate::derivation::InventoryProfile::Dense,
            canonical_digest: report.canonical_digest.clone(),
            index: crate::derivation::IndexPolicy {
                primary_nodes: 32,
                max_neighbourhood_size: 8,
                preserve_canonical_granularity: true,
                allow_pruning: false,
            },
            reconstruction: crate::derivation::ReconstructionPolicy {
                max_dependency_depth: 4,
                allowed_encodings: vec![crate::encoding::EncodingClass::Anchor],
            },
            residency: crate::derivation::ResidencyPolicy {
                asset_budget_bytes: u64::MAX,
                prefetch_neighbourhoods: 0,
            },
            max_silhouette_error_px: 1,
            allow_live_fallback: false,
        };
        let derivation = derive(&report, &p, None).expect("derivation");

        let view = Observer { azimuth_deg: 0.0, elevation_deg: 0.0, distance: 2.0, fov_deg: 60.0 };
        let direct = env.render_node(5, &view).expect("direct");
        match resolve_navigation(&derivation, &NavigationRequest { node: 5, view }, None) {
            DerivedResolution::Exact { payload, fidelity, .. } => {
                assert_eq!(payload.bytes, direct.bytes);
                assert_eq!(fidelity, FidelityClass::Exact);
            }
            other => panic!("expected exact resolution, got {other:?}"),
        }

        // AVR-C14: an unknown node is refused, never flattened to a
        // neighbouring node's view.
        let outside = NavigationRequest { node: 8, view };
        assert_eq!(
            resolve_navigation(&derivation, &outside, None),
            DerivedResolution::Unsupported(crate::retrieval::UnsupportedReason::OutsideDomain)
        );
        // Phase 6 environments observe orbitally per node; a pitched view
        // is outside the compiled domain.
        let pitched = NavigationRequest {
            node: 0,
            view: Observer { azimuth_deg: 0.0, elevation_deg: 30.0, distance: 2.0, fov_deg: 60.0 },
        };
        assert_eq!(
            resolve_navigation(&derivation, &pitched, None),
            DerivedResolution::Unsupported(crate::retrieval::UnsupportedReason::OutsideDomain)
        );
    }

    #[test]
    fn adjacency_decodes_back_to_node_level() {
        let env = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let report =
            compile_navigation(&env, &corridor_spec(), &policy(4), &budget(64)).expect("compiles");
        let per_node = observations_per_node(&report);
        assert_eq!(per_node, 4);
        assert_eq!(adjacent_nodes(&report.inventory.navigation.edges, 3, per_node),
            BTreeSet::from([2, 4]));
        assert_eq!(adjacent_nodes(&report.inventory.navigation.edges, 0, per_node),
            BTreeSet::from([1]));
        assert!(adjacent_nodes(&report.inventory.navigation.edges, 7, per_node).contains(&6));
    }

    #[test]
    fn invalidation_follows_the_section_7_4_table() {
        let env = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let report =
            compile_navigation(&env, &corridor_spec(), &policy(4), &budget(64)).expect("compiles");

        // Geometry modification: affected node's observations only.
        let geo = invalidated_observations(&report, &EnvironmentModification::NodeGeometryChanged { node: 3 });
        assert_eq!(geo.len(), 4);
        assert!(geo.iter().all(|id| (id.0 / 4) == 3));

        // Material/environment revisions default to 0; a revision change
        // invalidates exactly the observations rendered against it.
        let mat = invalidated_observations(&report, &EnvironmentModification::MaterialChanged { revision: 0 });
        assert_eq!(mat.len(), 32);
        let other = invalidated_observations(&report, &EnvironmentModification::MaterialChanged { revision: 1 });
        assert!(other.is_empty());
        let env_rev = invalidated_observations(&report, &EnvironmentModification::EnvironmentChanged { revision: 0 });
        assert_eq!(env_rev.len(), 32);
    }

    #[test]
    fn certification_accepts_connected_and_rejects_broken_environments() {
        let env = FakeEnvironment { nodes: 8, render_calls: Cell::new(0) };
        let report =
            compile_navigation(&env, &corridor_spec(), &policy(4), &budget(64)).expect("compiles");
        assert_eq!(certify_navigation(&report, &corridor_spec()), Ok(()));

        // AVR-C06: a room without internal connections is disconnected.
        let disconnected_room =
            NavigationSpec { rooms: vec![4, 4], edges: vec![(0, 1), (4, 5), (3, 4)] };
        assert_eq!(
            certify_navigation(&report, &disconnected_room),
            Err(NavigationCertificationError::RoomDisconnected { room: 0 })
        );

        // AVR-C06: rooms without a door edge between them leave the
        // environment disconnected.
        let no_door = NavigationSpec {
            rooms: vec![4, 4],
            edges: vec![(0, 1), (1, 2), (2, 3), (4, 5), (5, 6), (6, 7)],
        };
        assert_eq!(
            certify_navigation(&report, &no_door),
            Err(NavigationCertificationError::EnvironmentDisconnected)
        );

        // AVR-C03: an edge referencing an unknown observation is rejected.
        let mut forged = report.clone();
        forged.inventory.navigation.edges.push((ObservationId(9999), ObservationId(0)));
        assert!(matches!(
            certify_navigation(&forged, &corridor_spec()),
            Err(NavigationCertificationError::EdgeReferencesUnknownObservation { .. })
        ));
    }
}
