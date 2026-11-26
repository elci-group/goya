//! Spatial Focus Engine — contract `arachnid.focus/0.0.1`.
//!
//! Copied verbatim from Nevada's `crates/nevada/src/focus.rs`
//! (`nevada.focus/0.0.1`, commit db00f6a on `nebula/v0.1`). Only this
//! header and the contract identifier differ, so the two copies can be
//! diffed. Arachnid-specific extensions live in `traps.rs` and
//! `ontology.rs`, not here.
//!
//!
//! Deterministic directional navigation over a two-dimensional element
//! layout (originally for Nevada, Goya's Android TV target). Geometry uses
//! millipoint integers (`i64`) like `goya-layout`; edges are evaluated in
//! `i128` so no arithmetic in this module can overflow or panic.
//!
//! # Navigation rule (fully deterministic, documented tie-break)
//!
//! For a direction `d` from the focused element `F`, a candidate element
//! `C` is eligible when:
//!
//! 1. `C` is `focusable`, is not `F` itself, and lies inside the active
//!    viewport (when one is supplied);
//! 2. `C`'s projection lies strictly forward on the primary axis, i.e. its
//!    leading edge is at or beyond `F`'s trailing edge (a shared edge has
//!    primary distance `0`);
//! 3. `C`'s projection *overlaps* `F`'s projection on the perpendicular
//!    axis (half-open interval intersection). Candidates without
//!    perpendicular overlap are not eligible; this is deliberately strict
//!    so that off-axis elements are reported as unreachable rather than
//!    silently reached.
//!
//! Eligible candidates are ranked by, in order:
//!
//! 1. primary-axis distance (smaller wins);
//! 2. perpendicular-axis distance between interval centres (smaller wins);
//! 3. lower element id (smaller wins).
//!
//! `current = None` selects the *initial focus*: the focusable element
//! with the smallest `y`, then smallest `x`, then smallest id
//! (topmost-then-leftmost).
//!
//! Because [`focus_graph`] is computed by repeatedly applying the same
//! rule, per-query navigation via [`next_focus`] and the precomputed
//! graph always agree. [`unreachable_essential`] performs a
//! deterministic BFS over that graph so the Conformance Validator can
//! fail closed when an essential interactive element cannot be reached by
//! directional navigation.

use std::collections::{BTreeMap, BTreeSet};

/// Arachnid spatial focus contract identifier.
pub const CONTRACT: &str = "arachnid.focus/0.0.1";

/// One navigable element in the layout. Geometry is millipoint-integer
/// (`i64`), mirroring `goya-layout` conventions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusElement {
    /// Stable element identifier. Callers must supply unique ids; when
    /// duplicated, the first occurrence in slice order wins.
    pub id: u32,
    /// Left edge, millipoints.
    pub x: i64,
    /// Top edge, millipoints.
    pub y: i64,
    /// Width, millipoints.
    pub w: i64,
    /// Height, millipoints.
    pub h: i64,
    /// Whether the element participates in directional focus at all.
    pub focusable: bool,
    /// Whether the element must be reachable (Conformance Validator).
    pub essential: bool,
}

/// A remote-control direction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// All directions in canonical slot order.
    pub const ALL: [Direction; 4] = [Self::Up, Self::Down, Self::Left, Self::Right];

    /// Slot index used by the successor arrays in [`FocusSuccessors`].
    pub const fn slot(self) -> usize {
        match self {
            Self::Up => 0,
            Self::Down => 1,
            Self::Left => 2,
            Self::Right => 3,
        }
    }
}

/// A bounded viewport; elements fully inside it are the only navigation
/// candidates. Edges are half-open: `[x, x + w) x [y, y + h)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Viewport {
    /// Left edge, millipoints.
    pub x: i64,
    /// Top edge, millipoints.
    pub y: i64,
    /// Width, millipoints.
    pub w: i64,
    /// Height, millipoints.
    pub h: i64,
}

impl Viewport {
    /// Whether `element` lies fully inside this viewport. Evaluated in
    /// `i128` so extreme `i64` geometry cannot overflow.
    pub fn contains(&self, element: &FocusElement) -> bool {
        let vx0 = self.x as i128;
        let vy0 = self.y as i128;
        let vx1 = vx0 + self.w as i128;
        let vy1 = vy0 + self.h as i128;
        let ex0 = element.x as i128;
        let ey0 = element.y as i128;
        let ex1 = ex0 + element.w as i128;
        let ey1 = ey0 + element.h as i128;
        ex0 >= vx0 && ey0 >= vy0 && ex1 <= vx1 && ey1 <= vy1
    }
}

/// The four directional successors of one element, in
/// [`Direction::ALL`] slot order (`Up`, `Down`, `Left`, `Right`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusSuccessors {
    /// Id of the element this row describes.
    pub id: u32,
    /// Successor per direction slot; `None` when no candidate exists.
    pub successors: [Option<u32>; 4],
}

/// Half-open rectangle in `i128` millipoints. Internal helper; all edge
/// arithmetic is overflow-free by construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rect {
    x0: i128,
    y0: i128,
    x1: i128,
    y1: i128,
}

impl Rect {
    fn of(element: &FocusElement) -> Self {
        let x0 = element.x as i128;
        let y0 = element.y as i128;
        Self {
            x0,
            y0,
            x1: x0 + element.w as i128,
            y1: y0 + element.h as i128,
        }
    }

    fn center_x(&self) -> i128 {
        (self.x0 + self.x1) / 2
    }

    fn center_y(&self) -> i128 {
        (self.y0 + self.y1) / 2
    }
}

fn spans_overlap(a0: i128, a1: i128, b0: i128, b1: i128) -> bool {
    a0 < b1 && b0 < a1
}

/// Find the deterministic successor of `from` in direction `dir`.
///
/// `viewport` is `None` for the unbounded layout; otherwise only elements
/// fully inside the viewport are eligible.
fn successor_in_viewport(
    elements: &[FocusElement],
    from: &FocusElement,
    dir: Direction,
    viewport: Option<&Viewport>,
) -> Option<u32> {
    let from_rect = Rect::of(from);
    let mut best: Option<(i128, i128, u32)> = None;
    for candidate in elements {
        if !candidate.focusable || candidate.id == from.id {
            continue;
        }
        if let Some(viewport) = viewport {
            if !viewport.contains(candidate) {
                continue;
            }
        }
        let rect = Rect::of(candidate);
        // (primary_ok, primary_distance, perpendicular_overlap, perpendicular_distance)
        let (forward, primary, overlap, perpendicular) = match dir {
            Direction::Down => (
                rect.y0 >= from_rect.y1,
                rect.y0 - from_rect.y1,
                spans_overlap(rect.x0, rect.x1, from_rect.x0, from_rect.x1),
                (rect.center_x() - from_rect.center_x()).abs(),
            ),
            Direction::Up => (
                rect.y1 <= from_rect.y0,
                from_rect.y0 - rect.y1,
                spans_overlap(rect.x0, rect.x1, from_rect.x0, from_rect.x1),
                (rect.center_x() - from_rect.center_x()).abs(),
            ),
            Direction::Right => (
                rect.x0 >= from_rect.x1,
                rect.x0 - from_rect.x1,
                spans_overlap(rect.y0, rect.y1, from_rect.y0, from_rect.y1),
                (rect.center_y() - from_rect.center_y()).abs(),
            ),
            Direction::Left => (
                rect.x1 <= from_rect.x0,
                from_rect.x0 - rect.x1,
                spans_overlap(rect.y0, rect.y1, from_rect.y0, from_rect.y1),
                (rect.center_y() - from_rect.center_y()).abs(),
            ),
        };
        if !forward || !overlap {
            continue;
        }
        // Rank: primary distance, then perpendicular distance, then lower id.
        let key = (primary, perpendicular, candidate.id);
        if best.is_none_or(|current| key < current) {
            best = Some(key);
        }
    }
    best.map(|(_, _, id)| id)
}

/// Look up the first element with `id` that is focusable (and inside the
/// viewport when one is supplied).
fn find_focusable<'a>(
    elements: &'a [FocusElement],
    id: u32,
    viewport: Option<&'a Viewport>,
) -> Option<&'a FocusElement> {
    elements.iter().find(|element| {
        element.id == id
            && element.focusable
            && viewport.is_none_or(|viewport| viewport.contains(element))
    })
}

/// Deterministic initial focus: the focusable element with the smallest
/// `y`, then smallest `x`, then smallest id, optionally restricted to the
/// viewport.
fn initial_focus_in_viewport(
    elements: &[FocusElement],
    viewport: Option<&Viewport>,
) -> Option<u32> {
    let mut best: Option<(i64, i64, u32)> = None;
    for element in elements {
        if !element.focusable {
            continue;
        }
        if let Some(viewport) = viewport {
            if !viewport.contains(element) {
                continue;
            }
        }
        let key = (element.y, element.x, element.id);
        if best.is_none_or(|current| key < current) {
            best = Some(key);
        }
    }
    best.map(|(_, _, id)| id)
}

/// Nearest focusable element in direction `dir` from `current`, over the
/// unbounded layout. See the module documentation for the exact rule and
/// tie-break. Returns `None` for an empty layout, an unknown or
/// non-focusable `current` id, or when no candidate exists.
pub fn next_focus(elements: &[FocusElement], current: Option<u32>, dir: Direction) -> Option<u32> {
    match current {
        None => initial_focus_in_viewport(elements, None),
        Some(id) => {
            let from = find_focusable(elements, id, None)?;
            successor_in_viewport(elements, from, dir, None)
        }
    }
}

/// Same rule as [`next_focus`] but restricted to a bounded viewport:
/// both the current element and every candidate must lie fully inside
/// `viewport`.
pub fn next_focus_in_viewport(
    elements: &[FocusElement],
    current: Option<u32>,
    dir: Direction,
    viewport: &Viewport,
) -> Option<u32> {
    match current {
        None => initial_focus_in_viewport(elements, Some(viewport)),
        Some(id) => {
            let from = find_focusable(elements, id, Some(viewport))?;
            successor_in_viewport(elements, from, dir, Some(viewport))
        }
    }
}

/// Precompute the four directional successors of every focusable element
/// over the unbounded layout by repeatedly applying the same rule as
/// [`next_focus`], so the graph and per-query navigation always agree.
/// Rows are sorted by id; non-focusable elements are omitted.
pub fn focus_graph(elements: &[FocusElement]) -> Vec<FocusSuccessors> {
    focus_graph_in_viewport(elements, None)
}

/// [`focus_graph`] restricted to a bounded viewport.
pub fn focus_graph_in_viewport(
    elements: &[FocusElement],
    viewport: Option<&Viewport>,
) -> Vec<FocusSuccessors> {
    let mut rows: Vec<FocusSuccessors> = elements
        .iter()
        .filter(|element| {
            element.focusable && viewport.is_none_or(|viewport| viewport.contains(element))
        })
        .map(|element| FocusSuccessors {
            id: element.id,
            successors: Direction::ALL
                .map(|dir| successor_in_viewport(elements, element, dir, viewport)),
        })
        .collect();
    rows.sort_by_key(|row| row.id);
    rows
}

/// Essential elements that cannot be reached from `initial` by any
/// sequence of directional moves, ascending by id. Deterministic BFS over
/// [`focus_graph`].
///
/// - `initial = None` starts from the deterministic initial focus;
/// - an unknown or non-focusable `initial` id fails closed: every
///   essential element is reported;
/// - essential elements that are not focusable can never be reached and
///   are always reported.
pub fn unreachable_essential(elements: &[FocusElement], initial: Option<u32>) -> Vec<u32> {
    unreachable_essential_in_viewport(elements, initial, None)
}

/// [`unreachable_essential`] restricted to a bounded viewport.
pub fn unreachable_essential_in_viewport(
    elements: &[FocusElement],
    initial: Option<u32>,
    viewport: Option<&Viewport>,
) -> Vec<u32> {
    let start = match initial {
        Some(id) => find_focusable(elements, id, viewport).map(|element| element.id),
        None => initial_focus_in_viewport(elements, viewport),
    };
    let essential: BTreeSet<u32> = elements
        .iter()
        .filter(|element| element.essential)
        .map(|element| element.id)
        .collect();
    let Some(start) = start else {
        return essential.into_iter().collect();
    };

    let graph: BTreeMap<u32, [Option<u32>; 4]> = focus_graph_in_viewport(elements, viewport)
        .into_iter()
        .map(|row| (row.id, row.successors))
        .collect();

    let mut visited: BTreeSet<u32> = BTreeSet::new();
    let mut queue: Vec<u32> = vec![start];
    visited.insert(start);
    while let Some(id) = queue.pop() {
        if let Some(successors) = graph.get(&id) {
            for successor in successors.iter().flatten() {
                if visited.insert(*successor) {
                    queue.push(*successor);
                }
            }
        }
    }

    essential
        .iter()
        .copied()
        .filter(|id| !visited.contains(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(id: u32, x: i64, y: i64, w: i64, h: i64) -> FocusElement {
        FocusElement {
            id,
            x,
            y,
            w,
            h,
            focusable: true,
            essential: false,
        }
    }

    fn essential(id: u32, x: i64, y: i64, w: i64, h: i64) -> FocusElement {
        FocusElement {
            essential: true,
            ..element(id, x, y, w, h)
        }
    }

    #[test]
    fn straight_line_navigation() {
        let elements = [
            element(1, 0, 0, 100, 50),
            element(2, 0, 100, 100, 50),
            element(3, 0, 200, 100, 50),
        ];
        assert_eq!(next_focus(&elements, None, Direction::Down), Some(1));
        assert_eq!(next_focus(&elements, Some(1), Direction::Down), Some(2));
        assert_eq!(next_focus(&elements, Some(2), Direction::Down), Some(3));
        assert_eq!(next_focus(&elements, Some(3), Direction::Down), None);
        assert_eq!(next_focus(&elements, Some(3), Direction::Up), Some(2));
        assert_eq!(next_focus(&elements, Some(2), Direction::Up), Some(1));
        assert_eq!(next_focus(&elements, Some(1), Direction::Up), None);
        assert_eq!(next_focus(&elements, Some(1), Direction::Left), None);
        assert_eq!(next_focus(&elements, Some(1), Direction::Right), None);
    }

    #[test]
    fn perpendicular_distance_breaks_primary_ties() {
        // Two candidates the same primary distance below; the one whose
        // horizontal centre is closest to the current centre wins.
        let elements = [
            element(1, 0, 0, 100, 50),
            element(2, 0, 100, 100, 50),   // centre x = 50, distance 50
            element(3, 400, 100, 100, 50), // centre x = 450, distance 400
        ];
        assert_eq!(next_focus(&elements, Some(1), Direction::Down), Some(2));
    }

    #[test]
    fn id_breaks_remaining_ties() {
        // Identical geometry: only the id tie-break can decide.
        let elements = [
            element(7, 0, 0, 100, 50),
            element(9, 0, 100, 100, 50),
            element(4, 0, 100, 100, 50),
        ];
        assert_eq!(next_focus(&elements, Some(7), Direction::Down), Some(4));
    }

    #[test]
    fn non_overlapping_candidates_are_not_eligible() {
        // Candidate below but with no horizontal overlap: unreachable
        // downward even though it is the only element in that direction.
        let elements = [element(1, 0, 0, 100, 50), element(2, 500, 100, 100, 50)];
        assert_eq!(next_focus(&elements, Some(1), Direction::Down), None);
    }

    #[test]
    fn shared_edge_has_zero_primary_distance() {
        let elements = [element(1, 0, 0, 100, 100), element(2, 0, 100, 100, 100)];
        assert_eq!(next_focus(&elements, Some(1), Direction::Down), Some(2));
        assert_eq!(next_focus(&elements, Some(2), Direction::Up), Some(1));
    }

    #[test]
    fn initial_focus_is_topmost_then_leftmost() {
        let elements = [
            element(3, 50, 0, 10, 10),
            element(2, 0, 0, 10, 10),
            element(1, 0, 100, 10, 10),
        ];
        assert_eq!(next_focus(&elements, None, Direction::Down), Some(2));
        // Non-focusable elements never win initial focus.
        let mut hidden = element(0, 0, 0, 10, 10);
        hidden.focusable = false;
        let with_hidden = [hidden, element(5, 10, 0, 10, 10)];
        assert_eq!(next_focus(&with_hidden, None, Direction::Down), Some(5));
    }

    #[test]
    fn unknown_or_non_focusable_current_fails_closed() {
        let elements = [element(1, 0, 0, 100, 50)];
        assert_eq!(next_focus(&elements, Some(42), Direction::Down), None);
        let mut locked = element(1, 0, 0, 100, 50);
        locked.focusable = false;
        assert_eq!(next_focus(&[locked], Some(1), Direction::Down), None);
    }

    #[test]
    fn empty_layout_returns_none_and_no_unreachable() {
        let elements: [FocusElement; 0] = [];
        assert_eq!(next_focus(&elements, None, Direction::Down), None);
        assert!(focus_graph(&elements).is_empty());
        assert!(unreachable_essential(&elements, None).is_empty());
    }

    #[test]
    fn graph_agrees_with_next_focus() {
        let elements = [
            element(1, 0, 0, 100, 50),
            essential(2, 0, 100, 100, 50),
            element(3, 200, 50, 100, 50),
            element(4, 200, 150, 100, 50),
        ];
        let graph = focus_graph(&elements);
        assert_eq!(graph.len(), 4);
        assert_eq!(graph[0].id, 1);
        assert_eq!(graph[1].id, 2);
        assert_eq!(graph[2].id, 3);
        assert_eq!(graph[3].id, 4);
        for row in &graph {
            for dir in Direction::ALL {
                assert_eq!(
                    row.successors[dir.slot()],
                    next_focus(&elements, Some(row.id), dir),
                    "graph mismatch for element {} direction {dir:?}",
                    row.id
                );
            }
        }
    }

    #[test]
    fn unreachable_essential_is_reported() {
        // Element 2 lies fully off-axis from everything else: no
        // perpendicular overlap in any direction, so it can never be
        // reached by directional navigation.
        let elements = [
            essential(1, 0, 0, 100, 50),
            essential(2, 500, 1_000, 50, 25),
            essential(3, 150, 0, 100, 50),
        ];
        // Elements 1 and 3 overlap horizontally and sit side by side, so
        // each is reachable from the other; 2 is isolated.
        assert_eq!(unreachable_essential(&elements, None), vec![2]);
        assert_eq!(unreachable_essential(&elements, Some(3)), vec![2]);
        assert_eq!(unreachable_essential(&elements, Some(1)), vec![2]);
    }

    #[test]
    fn everything_reachable_reports_nothing() {
        let elements = [
            essential(1, 0, 0, 100, 50),
            essential(2, 0, 100, 100, 50),
            essential(3, 0, 200, 100, 50),
        ];
        assert!(unreachable_essential(&elements, None).is_empty());
    }

    #[test]
    fn invalid_initial_fails_closed_to_all_essential() {
        let elements = [essential(1, 0, 0, 100, 50), essential(2, 0, 100, 100, 50)];
        assert_eq!(unreachable_essential(&elements, Some(99)), vec![1, 2]);
        let mut locked = essential(1, 0, 0, 100, 50);
        locked.focusable = false;
        let elements = [locked, essential(2, 0, 100, 100, 50)];
        // Initial focus skips the locked element and lands on 2; only the
        // non-focusable essential 1 is unreachable.
        assert_eq!(unreachable_essential(&elements, None), vec![1]);
    }

    #[test]
    fn viewport_excludes_out_of_bounds_elements() {
        let viewport = Viewport {
            x: 0,
            y: 0,
            w: 1_000,
            h: 1_000,
        };
        let elements = [
            element(1, 0, 0, 100, 50),
            element(2, 0, 900, 100, 50),
            element(3, 0, 1_500, 100, 50),
        ];
        assert_eq!(
            next_focus_in_viewport(&elements, Some(1), Direction::Down, &viewport),
            Some(2)
        );
        assert_eq!(
            next_focus_in_viewport(&elements, Some(2), Direction::Down, &viewport),
            None
        );
        // Without the viewport the same query reaches element 3.
        assert_eq!(next_focus(&elements, Some(2), Direction::Down), Some(3));
        // A current element outside the viewport fails closed.
        assert_eq!(
            next_focus_in_viewport(&elements, Some(3), Direction::Up, &viewport),
            None
        );
    }

    #[test]
    fn extreme_geometry_does_not_overflow() {
        let elements = [
            element(1, i64::MIN / 2, i64::MIN / 2, i64::MAX, i64::MAX),
            element(2, 0, 0, i64::MAX / 4, i64::MAX / 4),
        ];
        // Any result is fine; the requirement is that nothing overflows
        // or panics in debug builds.
        let _ = next_focus(&elements, Some(1), Direction::Down);
        let _ = focus_graph(&elements);
        let viewport = Viewport {
            x: i64::MIN,
            y: i64::MIN,
            w: i64::MAX,
            h: i64::MAX,
        };
        let _ = next_focus_in_viewport(&elements, None, Direction::Down, &viewport);
    }

    #[test]
    fn duplicate_ids_take_first_occurrence() {
        // Element 1 appears twice; lookups resolve to the first
        // occurrence, so the successor below is the one under (0, 0).
        let elements = [
            element(1, 0, 0, 100, 50),
            element(1, 500, 100, 100, 50),
            element(2, 0, 100, 100, 50),
        ];
        assert_eq!(next_focus(&elements, None, Direction::Down), Some(1));
        assert_eq!(next_focus(&elements, Some(1), Direction::Down), Some(2));
    }
}
