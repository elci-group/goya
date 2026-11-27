//! Graph separation (directive §8.3) and reconstruction-graph acyclicity.
//!
//! Three logical graph types MUST be distinguished and MUST NOT be assumed
//! identical:
//!
//! - **Navigation** — permitted observation transitions; may contain cycles.
//! - **Reconstruction** — data dependencies; MUST be acyclic and depth-bounded.
//! - **Residency** — loading/eviction relationships; may contain predictive
//!   or probabilistic edges, which must not influence correctness.

/// Stable identifier of a canonical observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObservationId(pub u64);

/// Stable identifier of a primary anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnchorId(pub u32);

/// Stable identifier of a bounded neighbourhood.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NeighbourhoodId(pub u32);

/// Hard upper bound on reconstruction dependency depth (AVR-C05).
/// A derivation may lower this, never raise it.
pub const MAX_DEPENDENCY_DEPTH: u8 = 8;

/// Spatial coordinates of an observation within the domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationCoordinates {
    /// Fixed-point spherical or graph coordinates; interpretation is owned
    /// by the [`ObservationDomain`](crate::domain::ObservationDomain).
    pub spatial: [i64; 3],
}

/// Permitted observation transitions. Cycles are allowed by design.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NavigationGraph {
    /// Edges `(from, to)` over observation ids.
    pub edges: Vec<(ObservationId, ObservationId)>,
}

/// Reconstruction dependency of one observation on another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconstructionNode {
    /// The observation being resolved.
    pub observation: ObservationId,
    /// Direct dependencies, each at depth 1.
    pub dependencies: Vec<ObservationId>,
}

/// Data-dependency graph. Construction rejects cycles and enforces the
/// maximum dependency depth, so a graph instance is acyclic by construction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconstructionGraph {
    nodes: Vec<ReconstructionNode>,
    /// Observations stored directly; valid leaf dependencies.
    terminals: Vec<ObservationId>,
    max_depth: u8,
}

/// Why a reconstruction graph was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphTopologyError {
    /// A cycle exists in the dependency graph (AVR-C04).
    Cyclic,
    /// A dependency chain exceeds the maximum depth (AVR-C05).
    DepthExceeded { chain: Vec<ObservationId>, limit: u8 },
    /// A dependency references an observation with no record (referential
    /// integrity, directive §17).
    DanglingDependency { from: ObservationId, to: ObservationId },
}

impl ReconstructionGraph {
    /// Build a reconstruction graph, verifying acyclicity (AVR-C04),
    /// dependency depth (AVR-C05) and referential integrity.
    ///
    /// `terminals` are observation ids whose representations are stored
    /// directly (anchors and other independently reconstructable
    /// observations, §8.2): a dependency on a terminal is a valid leaf and
    /// contributes one level of depth. A dependency that references neither
    /// a node nor a terminal dangles.
    ///
    /// Cycles are detected by depth-bounded DFS; the work is O(V + E) over
    /// the node set, satisfying the bounded-computation rule.
    pub fn new(
        nodes: Vec<ReconstructionNode>,
        terminals: Vec<ObservationId>,
        max_depth: u8,
    ) -> Result<Self, GraphTopologyError> {
        let graph = Self { nodes, max_depth, terminals };
        graph.verify()?;
        Ok(graph)
    }

    /// The enforced maximum dependency depth.
    pub fn max_depth(&self) -> u8 {
        self.max_depth
    }

    pub fn nodes(&self) -> &[ReconstructionNode] {
        &self.nodes
    }

    /// Re-verify after external mutation is impossible by construction;
    /// Inquisition (AVR-C04/C05) calls this on any loaded inventory.
    ///
    /// Colour-marked DFS (white/gray/black) with memoised longest-chain
    /// depths: O(V + E), bounded by the node set, never by the domain.
    pub fn verify(&self) -> Result<(), GraphTopologyError> {
        let mut index: std::collections::HashMap<u64, usize> =
            std::collections::HashMap::with_capacity(self.nodes.len());
        for (i, node) in self.nodes.iter().enumerate() {
            index.insert(node.observation.0, i);
        }
        let terminals: std::collections::HashSet<u64> =
            self.terminals.iter().map(|t| t.0).collect();
        for node in &self.nodes {
            for dep in &node.dependencies {
                if !index.contains_key(&dep.0) && !terminals.contains(&dep.0) {
                    return Err(GraphTopologyError::DanglingDependency {
                        from: node.observation,
                        to: *dep,
                    });
                }
            }
        }

        // 0 = white, 1 = gray (on the DFS path), 2 = black (done).
        let mut colour = vec![0u8; self.nodes.len()];
        // Longest dependency chain starting at each node, memoised.
        let mut chain_len = vec![1u8; self.nodes.len()];

        for start in 0..self.nodes.len() {
            if colour[start] != 0 {
                continue;
            }
            colour[start] = 1;
            let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
            while let Some(&mut (idx, ref mut child)) = stack.last_mut() {
                let deps = &self.nodes[idx].dependencies;
                if *child < deps.len() {
                    let dep_id = deps[*child].0;
                    *child += 1;
                    match index.get(&dep_id) {
                        // Terminal: stored directly, contributes one level.
                        None => chain_len[idx] = chain_len[idx].max(2),
                        Some(&dep_idx) => match colour[dep_idx] {
                            0 => {
                                colour[dep_idx] = 1;
                                stack.push((dep_idx, 0));
                            }
                            // Gray: the dependency is still on the DFS path.
                            1 => return Err(GraphTopologyError::Cyclic),
                            _ => {
                                chain_len[idx] =
                                    chain_len[idx].max(chain_len[dep_idx].saturating_add(1));
                            }
                        },
                    }
                } else {
                    colour[idx] = 2;
                    stack.pop();
                    if let Some(&(parent, _)) = stack.last() {
                        chain_len[parent] = chain_len[parent].max(chain_len[idx].saturating_add(1));
                    }
                }
            }
            if chain_len[start] > self.max_depth {
                return Err(GraphTopologyError::DepthExceeded {
                    chain: vec![self.nodes[start].observation],
                    limit: self.max_depth,
                });
            }
        }
        Ok(())
    }
}

/// Residency relationships used for loading and eviction. Predictive or
/// probabilistic edges live here and never influence correctness
/// (directive §8.3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResidencyGraph {
    /// `(anchor, neighbour)` adjacency used for prefetch and eviction.
    pub adjacency: Vec<(AnchorId, AnchorId)>,
    /// Predicted next anchors; advisory only.
    pub predicted: Vec<(AnchorId, AnchorId)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(obs: u64, deps: &[u64]) -> ReconstructionNode {
        ReconstructionNode {
            observation: ObservationId(obs),
            dependencies: deps.iter().map(|d| ObservationId(*d)).collect(),
        }
    }

    #[test]
    fn acyclic_graph_is_accepted() {
        let graph = ReconstructionGraph::new(
            vec![node(1, &[2]), node(2, &[3]), node(3, &[])],
            vec![],
            MAX_DEPENDENCY_DEPTH,
        )
        .expect("chain 1→2→3 is acyclic");
        assert_eq!(graph.max_depth(), MAX_DEPENDENCY_DEPTH);
        assert_eq!(graph.nodes().len(), 3);
    }

    #[test]
    fn cyclic_graph_is_rejected() {
        let err = ReconstructionGraph::new(vec![node(1, &[2]), node(2, &[1])], vec![], MAX_DEPENDENCY_DEPTH)
            .expect_err("cycle must be rejected");
        assert_eq!(err, GraphTopologyError::Cyclic);
    }

    #[test]
    fn depth_beyond_limit_is_rejected() {
        let mut nodes = Vec::new();
        for i in 0..=8u64 {
            let deps = if i < 8 { vec![i + 1] } else { vec![] };
            nodes.push(node(i, &deps));
        }
        // Chain of length 9 exceeds depth limit 4.
        let err = ReconstructionGraph::new(nodes, vec![], 4).expect_err("depth must be rejected");
        match err {
            GraphTopologyError::DepthExceeded { limit, .. } => assert_eq!(limit, 4),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn dangling_dependency_is_rejected() {
        let err = ReconstructionGraph::new(vec![node(1, &[99])], vec![], MAX_DEPENDENCY_DEPTH)
            .expect_err("dangling dependency must be rejected");
        assert_eq!(
            err,
            GraphTopologyError::DanglingDependency {
                from: ObservationId(1),
                to: ObservationId(99),
            }
        );
    }

    #[test]
    fn diamond_dependency_graph_is_not_mistaken_for_a_cycle() {
        // 1 depends on 2 and 3, both depend on 4: a shared dependency, not
        // a cycle. This guards the DFS colour marking.
        let graph = ReconstructionGraph::new(
            vec![node(1, &[2, 3]), node(2, &[4]), node(3, &[4]), node(4, &[])],
            vec![],
            MAX_DEPENDENCY_DEPTH,
        )
        .expect("diamond DAG is acyclic");
        assert_eq!(graph.nodes().len(), 4);
    }

    #[test]
    fn dependency_on_a_stored_terminal_is_a_valid_leaf() {
        // Observation 2 depends on anchor observation 1, which is stored
        // directly and has no reconstruction node (§8.2).
        let graph = ReconstructionGraph::new(vec![node(2, &[1])], vec![ObservationId(1)], 4)
            .expect("anchor dependency is a valid leaf");
        assert_eq!(graph.nodes().len(), 1);
    }

    #[test]
    fn navigation_graph_may_contain_cycles() {
        let nav = NavigationGraph {
            edges: vec![(ObservationId(1), ObservationId(2)), (ObservationId(2), ObservationId(1))],
        };
        // No construction-time check: navigation cycles are permitted.
        assert_eq!(nav.edges.len(), 2);
    }
}
