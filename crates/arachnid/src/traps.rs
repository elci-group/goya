//! Focus-trap detection over the copied focus engine (`focus.rs`).
//!
//! `unreachable_essential` proves every essential element can be reached
//! *from the initial focus*. That does not prove the user can get anywhere
//! else afterwards. Directional edges are one-way: an element can be entered
//! by pressing Right and left with no direction leading back. This module
//! checks the stronger property: from **every** element reachable from the
//! initial focus, every essential element reachable from the initial focus
//! is still reachable.
//!
//! Each element costs one breadth-first search, which is bounded by
//! `ontology::MAX_ACTIONABLE`.

use crate::focus::{focus_graph, FocusElement};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// One element from which some essential elements can no longer be reached.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trap {
    pub from: u32,
    /// Essential ids reachable from the initial focus but not from `from`,
    /// ascending.
    pub unreachable: Vec<u32>,
}

fn reachable(graph: &BTreeMap<u32, [Option<u32>; 4]>, start: u32) -> BTreeSet<u32> {
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(id) = queue.pop_front() {
        if let Some(successors) = graph.get(&id) {
            for next in successors.iter().flatten() {
                if seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
    }
    seen
}

/// Every trap reachable from `initial`, ascending by `from`. An empty result
/// means navigation is strongly connected over the reachable essentials.
pub fn traps(elements: &[FocusElement], initial: u32) -> Vec<Trap> {
    let graph: BTreeMap<u32, [Option<u32>; 4]> = focus_graph(elements)
        .into_iter()
        .map(|row| (row.id, row.successors))
        .collect();
    let essential: BTreeSet<u32> = elements
        .iter()
        .filter(|e| e.essential)
        .map(|e| e.id)
        .collect();
    let from_initial = reachable(&graph, initial);
    let required: BTreeSet<u32> = essential.intersection(&from_initial).copied().collect();
    from_initial
        .iter()
        .filter_map(|&from| {
            let here = reachable(&graph, from);
            let unreachable: Vec<u32> = required.difference(&here).copied().collect();
            (!unreachable.is_empty()).then_some(Trap { from, unreachable })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(id: u32, x: i64, y: i64, w: i64, h: i64) -> FocusElement {
        FocusElement {
            id,
            x,
            y,
            w,
            h,
            focusable: true,
            essential: true,
        }
    }

    #[test]
    fn row_of_buttons_has_no_trap() {
        let row = [
            el(0, 0, 0, 10, 10),
            el(1, 20, 0, 10, 10),
            el(2, 40, 0, 10, 10),
        ];
        assert!(traps(&row, 0).is_empty());
    }

    #[test]
    fn grid_has_no_trap() {
        let mut grid = Vec::new();
        for r in 0..3 {
            for c in 0..4 {
                grid.push(el(r * 4 + c, c as i64 * 20, r as i64 * 20, 10, 10));
            }
        }
        assert!(traps(&grid, 0).is_empty());
    }

    /// Reachability from the initial focus is not enough. Here every
    /// element is reachable from 1, but once focus moves to the pillar (0),
    /// no direction leads back to 1.
    ///
    /// * From 1, Left → 0: the pillar overlaps 1 vertically.
    /// * From 0, Right → 2: 2 is nearer than 1, and both overlap the pillar.
    /// * From 2, Left → 0. Right and Up find nothing, because 1 shares no
    ///   span with 2 on either axis.
    #[test]
    fn one_way_entry_is_a_trap() {
        let elements = [
            el(0, 0, 0, 10, 100),
            el(2, 20, 50, 10, 10),
            el(1, 40, 0, 10, 10),
        ];
        assert!(crate::focus::unreachable_essential(&elements, Some(1)).is_empty());
        assert_eq!(
            traps(&elements, 1),
            vec![
                Trap {
                    from: 0,
                    unreachable: vec![1]
                },
                Trap {
                    from: 2,
                    unreachable: vec![1]
                },
            ]
        );
    }

    /// An element that is never reachable is a reachability failure, not a
    /// trap: traps only cover elements reachable from the initial focus.
    #[test]
    fn unreachable_is_not_a_trap() {
        let elements = [
            el(0, 0, 0, 100, 10),
            el(4, 0, 20, 10, 10),
            el(1, 60, 40, 10, 10),
        ];
        assert!(crate::focus::unreachable_essential(&elements, Some(0)).contains(&1));
        assert!(traps(&elements, 0).is_empty());
    }

    #[test]
    fn three_connected_elements_have_no_trap() {
        let elements = [
            el(0, 0, 0, 10, 10),
            el(1, 20, 5, 10, 10),
            el(2, 20, 30, 10, 10),
        ];
        assert!(traps(&elements, 0).is_empty());
    }

    #[test]
    fn single_element_has_no_trap() {
        assert!(traps(&[el(7, 0, 0, 10, 10)], 7).is_empty());
    }
}
