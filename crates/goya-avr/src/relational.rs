//! Relational layout construction (directive §8.2–§8.4, WS-06).
//!
//! Phase 2 anchor selection optimises **spatial coverage** by greedy
//! farthest-point sampling over the quantised observation states — a
//! deterministic approximation to the §8.4 objective J. The full weighted
//! objective (storage, residency, latency, error, transition overhead) is
//! deferred until the §18 benchmark programme produces evidence; the
//! directive's hard constraints (fidelity, memory, latency) are enforced
//! as budget/capacity errors by the derivation engine (`target.rs`), never
//! traded away silently.
//!
//! All construction is deterministic (AVR-G04): greedy choices break ties
//! by lowest observation id, so two compilations with identical canonical
//! identity produce identical layouts.

use crate::domain::ObservationState;
use crate::graph::{
    AnchorId, NavigationGraph, NeighbourhoodId, ObservationCoordinates, ObservationId,
    ResidencyGraph,
};
use crate::inventory::{Neighbourhood, RelationalAnchor};

/// Why a relational layout could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// The neighbourhood capacity across all anchors cannot cover every
    /// canonical observation (§9.2: a relational inventory MAY retain
    /// every canonical observation; this profile cannot).
    CoverageImpossible { uncovered: u32, primary_nodes: u32, max_neighbourhood_size: u32 },
}

/// Unit-sphere position of a quantised observation state.
///
/// The derivation metric is the chord distance between unit-sphere
/// positions of (azimuth, elevation); distance/zoom axes do not
/// participate in Phase 2 because the initial mesh is single-distance.
fn unit_position(state: &ObservationState) -> [f64; 3] {
    let azimuth = (state.orientation[0] as f64 / 100.0).to_radians();
    let elevation = (state.orientation[1] as f64 / 100.0).to_radians();
    [
        elevation.cos() * azimuth.cos(),
        elevation.cos() * azimuth.sin(),
        elevation.sin(),
    ]
}

fn chord_sq(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

/// Select `k` primary anchors by greedy farthest-point sampling.
///
/// The first anchor is the observation nearest the reference direction
/// (azimuth 0°, elevation 0°) — the "front" of the model; each subsequent
/// anchor maximises the minimum distance to the selected set, ties broken
/// by lowest observation id. Returns selected indices in selection order.
pub fn select_anchors(states: &[ObservationState], k: usize) -> Vec<usize> {
    if k == 0 || states.is_empty() {
        return Vec::new();
    }
    let positions: Vec<[f64; 3]> = states.iter().map(unit_position).collect();
    let front = [1.0f64, 0.0, 0.0];

    let mut selected: Vec<usize> = Vec::with_capacity(k);
    // Start: nearest the front, ties by lowest id (canonical order).
    let (mut start, mut start_d) = (0usize, f64::INFINITY);
    for (i, pos) in positions.iter().enumerate() {
        let d = chord_sq(pos, &front);
        if d < start_d {
            start_d = d;
            start = i;
        }
    }
    selected.push(start);

    // min-distance to the selected set, maintained incrementally.
    let mut min_dist: Vec<f64> = positions
        .iter()
        .map(|p| chord_sq(p, &positions[start]))
        .collect();
    while selected.len() < k && selected.len() < states.len() {
        let mut best = usize::MAX;
        let mut best_d = -1.0f64;
        for (i, d) in min_dist.iter().enumerate() {
            if *d > best_d {
                best_d = *d;
                best = i;
            }
        }
        if best == usize::MAX {
            break;
        }
        selected.push(best);
        let bp = &positions[best];
        for (i, pos) in positions.iter().enumerate() {
            let d = chord_sq(pos, bp);
            if d < min_dist[i] {
                min_dist[i] = d;
            }
        }
    }
    selected
}

/// Construct the bounded neighbourhood of one anchor: the
/// `max_size` observations nearest to it (the anchor itself included),
/// ties broken by lowest id.
fn neighbourhood_members(
    anchor_index: usize,
    positions: &[[f64; 3]],
    max_size: u32,
) -> Vec<usize> {
    let anchor_pos = &positions[anchor_index];
    let mut ranked: Vec<(u64, f64)> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u64, chord_sq(p, anchor_pos)))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    ranked.truncate(max_size as usize);
    ranked.into_iter().map(|(id, _)| id as usize).collect()
}

/// The complete relational layout derived from canonical observations.
#[derive(Debug, Clone, PartialEq)]
pub struct RelationalLayout {
    pub anchors: Vec<RelationalAnchor>,
    pub neighbourhoods: Vec<Neighbourhood>,
    pub navigation: NavigationGraph,
    pub residency: ResidencyGraph,
    /// For each canonical observation index: the neighbourhood ids that
    /// contain it (overlap is permitted; payloads are never duplicated).
    pub membership: Vec<Vec<u32>>,
}

/// Build anchors, bounded neighbourhoods and the three separated graphs.
///
/// Every canonical observation must land in its nearest anchor's
/// neighbourhood; if the capacity bound makes that impossible the profile
/// is rejected with [`LayoutError::CoverageImpossible`] rather than
/// silently dropping observations (AVR-C02/C07).
pub fn build_layout(
    states: &[ObservationState],
    anchor_count: u32,
    max_neighbourhood_size: u32,
    max_dependency_depth: u8,
) -> Result<RelationalLayout, LayoutError> {
    if anchor_count == 0 || max_neighbourhood_size == 0 {
        return Err(LayoutError::CoverageImpossible {
            uncovered: states.len() as u32,
            primary_nodes: anchor_count,
            max_neighbourhood_size,
        });
    }
    let positions: Vec<[f64; 3]> = states.iter().map(unit_position).collect();
    let selected = select_anchors(states, anchor_count.min(states.len() as u32) as usize);

    // Materialise anchors and neighbourhoods. Coverage is defined over
    // neighbourhood *references*: an observation is covered when some
    // anchor's nearest-member list contains it (§8.2). Ownership is not
    // modelled separately in Phase 2 — payloads live once and are shared.
    let mut anchors = Vec::with_capacity(selected.len());
    let mut neighbourhoods = Vec::with_capacity(selected.len());
    let mut membership: Vec<Vec<u32>> = vec![Vec::new(); states.len()];
    for (rank, &anchor_index) in selected.iter().enumerate() {
        let members = neighbourhood_members(anchor_index, &positions, max_neighbourhood_size);
        for &member in &members {
            membership[member].push(rank as u32);
        }
        neighbourhoods.push(Neighbourhood {
            id: NeighbourhoodId(rank as u32),
            anchor: AnchorId(rank as u32),
            observations: members.iter().map(|i| ObservationId(*i as u64)).collect(),
            max_observations: max_neighbourhood_size,
        });
        anchors.push(RelationalAnchor {
            id: AnchorId(rank as u32),
            observation: ObservationId(anchor_index as u64),
            neighbourhood: NeighbourhoodId(rank as u32),
            adjacent: Vec::new(),
            bounds: ObservationCoordinates {
                spatial: [
                    states[anchor_index].orientation[0],
                    states[anchor_index].orientation[1],
                    states[anchor_index].camera[0],
                ],
            },
        });
    }

    // Coverage check: every canonical observation must be referenced by at
    // least one neighbourhood, else this profile cannot retain the full
    // observational granularity (§9.2, AVR-C02/C07).
    let uncovered = membership.iter().filter(|refs| refs.is_empty()).count() as u32;
    if uncovered > 0 {
        return Err(LayoutError::CoverageImpossible {
            uncovered,
            primary_nodes: anchor_count,
            max_neighbourhood_size,
        });
    }

    // Anchor adjacency: the `2` nearest other anchors, ties by anchor id —
    // a deterministic sparse navigation net over the anchors.
    let anchor_positions: Vec<[f64; 3]> =
        selected.iter().map(|i| positions[*i]).collect();
    for (rank, pos) in anchor_positions.iter().enumerate() {
        let mut ranked: Vec<(usize, f64)> = anchor_positions
            .iter()
            .enumerate()
            .filter(|(other, _)| other != &rank)
            .map(|(other, other_pos)| (other, chord_sq(pos, other_pos)))
            .collect();
        ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        ranked.truncate(2);
        anchors[rank].adjacent = ranked.iter().map(|(o, _)| AnchorId(*o as u32)).collect();
    }

    // Navigation graph: permitted observation transitions — anchor ring
    // plus anchor↔member edges. Cycles are permitted here by design (§8.3).
    let mut edges: Vec<(ObservationId, ObservationId)> = Vec::new();
    for anchor in &anchors {
        for adj in &anchor.adjacent {
            edges.push((anchor.observation, anchors[adj.0 as usize].observation));
        }
        let neighbourhood = &neighbourhoods[anchor.neighbourhood.0 as usize];
        for member in &neighbourhood.observations {
            if *member != anchor.observation {
                edges.push((anchor.observation, *member));
            }
        }
    }
    edges.sort();
    edges.dedup();

    // Residency graph: adjacency drives prefetch/eviction; the predictive
    // set stays empty in Phase 2 (no runtime history yet, §11.2).
    let adjacency: Vec<(AnchorId, AnchorId)> = anchors
        .iter()
        .flat_map(|a| a.adjacent.iter().map(move |adj| (a.id, *adj)))
        .collect();

    let _ = max_dependency_depth; // reconstruction is Phase 3; see target.rs

    Ok(RelationalLayout {
        anchors,
        neighbourhoods,
        navigation: NavigationGraph { edges },
        residency: ResidencyGraph { adjacency, predicted: Vec::new() },
        membership,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(azimuth_deg: f32, elevation_deg: f32) -> ObservationState {
        crate::ingestion::Observer {
            azimuth_deg,
            elevation_deg,
            distance: 2.0,
            fov_deg: 60.0,
        }
        .to_state()
    }

    #[test]
    fn anchor_selection_is_deterministic_and_spread() {
        let states: Vec<ObservationState> = (0..64)
            .map(|i| state(i as f32 * 5.625, ((i % 8) as f32 - 3.5) * 20.0))
            .collect();
        let a = select_anchors(&states, 8);
        let b = select_anchors(&states, 8);
        assert_eq!(a, b, "AVR-G04: deterministic anchor selection");
        assert_eq!(a.len(), 8);
        // The first anchor is the observation nearest the front direction:
        // here index 3 (az 16.875°, el −10°) beats index 0 (el −70°).
        assert_eq!(a[0], 3);
        // Selected anchors are distinct.
        let unique: std::collections::HashSet<usize> = a.iter().copied().collect();
        assert_eq!(unique.len(), 8);
    }

    #[test]
    fn layout_covers_every_observation_and_respects_bounds() {
        let states: Vec<ObservationState> = (0..128)
            .map(|i| state(i as f32 * 2.8125, 0.0))
            .collect();
        let layout = build_layout(&states, 16, 32, crate::graph::MAX_DEPENDENCY_DEPTH)
            .expect("layout covers");
        assert_eq!(layout.anchors.len(), 16);
        assert_eq!(layout.neighbourhoods.len(), 16);
        for neighbourhood in &layout.neighbourhoods {
            assert!(
                neighbourhood.observations.len() as u32 <= neighbourhood.max_observations,
                "AVR-C07"
            );
        }
        // Every observation is referenced by at least one neighbourhood.
        for (i, member_of) in layout.membership.iter().enumerate() {
            assert!(!member_of.is_empty(), "observation {i} uncovered");
        }
    }

    #[test]
    fn insufficient_capacity_is_an_explicit_error() {
        let states: Vec<ObservationState> = (0..100).map(|i| state(i as f32 * 3.6, 0.0)).collect();
        let err = build_layout(&states, 2, 10, crate::graph::MAX_DEPENDENCY_DEPTH).unwrap_err();
        // 2 anchors × nearest-10 = at most 20 referenced observations.
        assert_eq!(
            err,
            LayoutError::CoverageImpossible { uncovered: 80, primary_nodes: 2, max_neighbourhood_size: 10 }
        );
    }

    #[test]
    fn navigation_graph_is_separated_and_deterministic() {
        let states: Vec<ObservationState> = (0..64).map(|i| state(i as f32 * 5.625, 0.0)).collect();
        let a = build_layout(&states, 8, 32, 4).expect("layout");
        let b = build_layout(&states, 8, 32, 4).expect("layout");
        assert_eq!(a.navigation, b.navigation);
        assert_eq!(a.residency, b.residency);
        // Navigation may contain cycles; residency adjacency mirrors
        // anchor adjacency; predicted set is empty in Phase 2.
        assert!(!a.navigation.edges.is_empty());
        assert!(a.residency.predicted.is_empty());
    }
}
