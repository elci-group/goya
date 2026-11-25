//! Stock-remote support: the key contract and the press-count analysis.
//!
//! A stock (infrared) remote has no pointer. Everything has to work with
//! the arrow keys and OK alone, so 5-way is the baseline every build must
//! meet, and the Magic Remote pointer is an optional extra.
//!
//! Observed on the owner's TV (XR55N21WEB24, webOS TV Lite 7.5.8,
//! Chromium 87, stock remote, `examples/input_probe.rs`, 20 November 2026):
//! * The arrow keys arrive as `keydown`/`keyup` with keyCodes 37–40 and
//!   `key` set to `Arrow*`.
//! * OK arrives as `keydown 13` → `keyup 13`, with no `mousedown`,
//!   `mouseup` or `click`.
//! * No `mousemove` or `cursorStateChange` was ever delivered.
//! * Back never reached the page. The platform closed the app, and the
//!   next launch was a fresh load.
//!
//! The compile-time half is [`press_counts`]: for every actionable element,
//! how many arrow presses it takes to reach it from the initial focus, and
//! how many to get back. These are reported, not judged, because there is
//! no sourced threshold for an acceptable press count.

use crate::focus::FocusSuccessors;
use std::collections::{BTreeMap, VecDeque};

pub const CONTRACT: &str = "arachnid.remote/0.0.1";

/// What the generated runtime does with a key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Handling {
    /// Moves focus along the compiled graph; default action prevented.
    Navigate,
    /// Activates the focused element; default action prevented.
    Activate,
    /// Not handled; the default action is left to the platform.
    Platform,
}

/// Where a key fact comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Evidence {
    /// Seen with the input probe on the owner's TV.
    Observed,
    /// From LG's developer documentation; not yet seen on a device.
    Documented,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Key {
    pub code: u16,
    pub name: &'static str,
    pub handling: Handling,
    pub evidence: Evidence,
}

/// The stock-remote key contract. The runtime takes only the five 5-way
/// keys. Every other key keeps its platform behaviour.
pub const KEYS: &[Key] = &[
    Key {
        code: 37,
        name: "Left",
        handling: Handling::Navigate,
        evidence: Evidence::Observed,
    },
    Key {
        code: 38,
        name: "Up",
        handling: Handling::Navigate,
        evidence: Evidence::Observed,
    },
    Key {
        code: 39,
        name: "Right",
        handling: Handling::Navigate,
        evidence: Evidence::Observed,
    },
    Key {
        code: 40,
        name: "Down",
        handling: Handling::Navigate,
        evidence: Evidence::Observed,
    },
    Key {
        code: 13,
        name: "OK",
        handling: Handling::Activate,
        evidence: Evidence::Observed,
    },
    Key {
        code: 461,
        name: "Back",
        handling: Handling::Platform,
        evidence: Evidence::Observed,
    },
    Key {
        code: 403,
        name: "Red",
        handling: Handling::Platform,
        evidence: Evidence::Documented,
    },
    Key {
        code: 404,
        name: "Green",
        handling: Handling::Platform,
        evidence: Evidence::Documented,
    },
    Key {
        code: 405,
        name: "Yellow",
        handling: Handling::Platform,
        evidence: Evidence::Documented,
    },
    Key {
        code: 406,
        name: "Blue",
        handling: Handling::Platform,
        evidence: Evidence::Documented,
    },
];

/// Codes the runtime consumes, in table order.
pub fn consumed_codes() -> Vec<u16> {
    KEYS.iter()
        .filter(|k| k.handling != Handling::Platform)
        .map(|k| k.code)
        .collect()
}

/// Arrow-press distances over the compiled focus graph.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PressCounts {
    /// Fewest presses from the initial focus to each reachable element.
    pub from_initial: BTreeMap<u32, u32>,
    /// Fewest presses from each element back to the initial focus.
    pub to_initial: BTreeMap<u32, u32>,
}

impl PressCounts {
    pub fn max_from_initial(&self) -> u32 {
        self.from_initial.values().copied().max().unwrap_or(0)
    }
    pub fn max_to_initial(&self) -> u32 {
        self.to_initial.values().copied().max().unwrap_or(0)
    }
}

fn bfs(adjacent: &BTreeMap<u32, Vec<u32>>, start: u32) -> BTreeMap<u32, u32> {
    let mut dist = BTreeMap::from([(start, 0)]);
    let mut queue = VecDeque::from([start]);
    while let Some(id) = queue.pop_front() {
        let d = dist[&id];
        for &next in adjacent.get(&id).into_iter().flatten() {
            if let std::collections::btree_map::Entry::Vacant(slot) = dist.entry(next) {
                slot.insert(d + 1);
                queue.push_back(next);
            }
        }
    }
    dist
}

/// Breadth-first press counts from and back to `initial`. One press is one
/// edge of the graph.
pub fn press_counts(graph: &[FocusSuccessors], initial: Option<u32>) -> PressCounts {
    let Some(initial) = initial else {
        return PressCounts::default();
    };
    let mut forward: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut reverse: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for row in graph {
        for &next in row.successors.iter().flatten() {
            forward.entry(row.id).or_default().push(next);
            reverse.entry(next).or_default().push(row.id);
        }
    }
    PressCounts {
        from_initial: bfs(&forward, initial),
        to_initial: bfs(&reverse, initial),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u32, successors: [Option<u32>; 4]) -> FocusSuccessors {
        FocusSuccessors { id, successors }
    }

    #[test]
    fn counts_a_row_of_three() {
        // Slots: up, down, left, right.
        let g = [
            row(0, [None, None, None, Some(1)]),
            row(1, [None, None, Some(0), Some(2)]),
            row(2, [None, None, Some(1), None]),
        ];
        let p = press_counts(&g, Some(0));
        assert_eq!(p.from_initial, BTreeMap::from([(0, 0), (1, 1), (2, 2)]));
        assert_eq!(p.to_initial, BTreeMap::from([(0, 0), (1, 1), (2, 2)]));
        assert_eq!((p.max_from_initial(), p.max_to_initial()), (2, 2));
    }

    #[test]
    fn one_way_edges_count_differently_each_way() {
        // 0 → 1 → 2 → 0 is a one-way ring: reaching 2 takes two presses,
        // coming back from 1 also takes two.
        let g = [
            row(0, [None, None, None, Some(1)]),
            row(1, [None, None, None, Some(2)]),
            row(2, [None, None, None, Some(0)]),
        ];
        let p = press_counts(&g, Some(0));
        assert_eq!(p.from_initial[&2], 2);
        assert_eq!(p.to_initial[&1], 2);
        assert_eq!(p.to_initial[&2], 1);
    }

    #[test]
    fn no_initial_means_no_counts() {
        assert_eq!(press_counts(&[], None), PressCounts::default());
    }

    #[test]
    fn runtime_consumes_only_five_way() {
        assert_eq!(consumed_codes(), vec![37, 38, 39, 40, 13]);
    }
}
