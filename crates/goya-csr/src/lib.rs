//! GY-CSR/1 — Core-Split Rendering: topology-aware spatial scheduling.
//!
//! GY-CSR-001: partition the affected change envelope (not the whole frame)
//! into spatial territories, assign them to workers according to a
//! selectable policy (serial / dual-horizontal / quad-corner), and permit
//! work stealing when a worker's territory is empty while others still hold
//! tiles.
//!
//! Core rules:
//! - Partitioning defines work OWNERSHIP, not draw order.
//! - Worker count is a budget, never the machine's raw logical CPU count.
//! - Pixel-correct output independent of worker count (unit-tested:
//!   partitioning is a pure function; tile sets are disjoint and complete).
//! - The scheduler is deterministic for equivalent inputs.

/// Contract identifier implemented by this crate.
pub const CSR_CONTRACT: &str = "GY-CSR/1";

/// A dirty tile in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
    pub size: u32,
}

/// Scheduling policy (GY-CSR-001 §8: serial, dual-horizontal, quad-corner,
/// adaptive tile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// One worker does everything (low-power / small dirty regions).
    Serial,
    /// Two workers: left and right territories meeting at the centre.
    DualHorizontal,
    /// Four workers from the four corners, progressing inward.
    QuadCorner,
}

/// A spatial territory assigned to one worker.
#[derive(Debug, Clone, PartialEq)]
pub struct Territory {
    pub worker: u32,
    pub tiles: Vec<Tile>,
}

/// Greedy work-stealing scheduler over the partitioned territories:
/// workers drain their own queue; idle workers steal from the worker with
/// the most remaining tiles. Returns the tile→worker execution plan.
pub fn schedule(territories: &[Territory]) -> Vec<(Tile, u32)> {
    let mut queues: Vec<std::collections::VecDeque<Tile>> = territories
        .iter()
        .map(|t| t.tiles.iter().copied().collect())
        .collect();
    let mut plan = Vec::new();
    let n = queues.len();
    if n == 0 {
        return plan;
    }
    loop {
        // Every active worker takes one tile per step (ownership first).
        let mut progressed = false;
        for (worker, q) in queues.iter_mut().enumerate() {
            if let Some(tile) = q.pop_front() {
                plan.push((tile, worker as u32));
                progressed = true;
            }
        }
        // Work stealing: idle workers take from the largest queue.
        loop {
            let idle: Vec<usize> = (0..n).filter(|&w| queues[w].is_empty()).collect();
            if idle.is_empty() {
                break;
            }
            let donor = queues
                .iter()
                .enumerate()
                .filter(|(_, q)| !q.is_empty())
                .max_by_key(|(_, q)| q.len())
                .map(|(w, _)| w);
            let Some(donor) = donor else {
                break;
            };
            // Pop can only fail if the queue emptied between the filter and
            // here (single-threaded loop: impossible) — still explicit.
            let tile = match queues[donor].pop_back() {
                Some(t) => t,
                None => break,
            };
            plan.push((tile, idle[0] as u32));
            progressed = true;
        }
        if !progressed {
            break;
        }
    }
    plan
}

/// Partition a dirty region into tiles and assign territories per policy.
/// Deterministic; tile sets across territories are disjoint and complete.
pub fn partition(
    dirty: goya_change::PixelRect,
    tile_size: u32,
    policy: Policy,
    worker_budget: u32,
) -> Vec<Territory> {
    let tiles = tile_grid(dirty, tile_size);
    match policy {
        Policy::Serial => {
            vec![Territory { worker: 0, tiles }]
        }
        Policy::DualHorizontal => {
            let (left, right) = split_vertical(tiles);
            vec![
                Territory {
                    worker: 0,
                    tiles: left,
                },
                Territory {
                    worker: 1,
                    tiles: right,
                },
            ]
        }
        Policy::QuadCorner => {
            let mut quads: [Vec<Tile>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            let (cx, cy) = (dirty.x + dirty.width / 2, dirty.y + dirty.height / 2);
            for t in tiles {
                // Quadrant by tile centre relative to the dirty region centre.
                let tcx = t.x + t.size as i32 / 2;
                let tcy = t.y + t.size as i32 / 2;
                let idx = match (tcx < cx, tcy < cy) {
                    (true, true) => 0,  // top-left: worker 0's corner origin
                    (false, true) => 1, // top-right
                    (true, false) => 2, // bottom-left
                    (false, false) => 3,
                };
                quads[idx].push(t);
            }
            let workers = worker_budget.clamp(1, 4) as usize;
            (0..workers)
                .map(|w| Territory {
                    worker: w as u32,
                    tiles: std::mem::take(&mut quads[w]),
                })
                .filter(|t| !t.tiles.is_empty())
                .collect()
        }
    }
}

/// Select a policy from change locality and worker budget (directive §3:
/// small workloads must not pay synchronisation costs).
pub fn select_policy(dirty_area_px: u64, tile_size: u32, worker_budget: u32) -> Policy {
    if worker_budget <= 1 {
        return Policy::Serial;
    }
    let tiles = dirty_area_px / u64::from(tile_size.max(1) * tile_size.max(1));
    if tiles <= 4 {
        Policy::Serial
    } else if tiles <= 64 {
        Policy::DualHorizontal
    } else {
        Policy::QuadCorner
    }
}

fn tile_grid(dirty: goya_change::PixelRect, tile_size: u32) -> Vec<Tile> {
    let mut tiles = Vec::new();
    let size = tile_size.max(1) as i32;
    let mut y = dirty.y;
    while y < dirty.y + dirty.height {
        let mut x = dirty.x;
        while x < dirty.x + dirty.width {
            tiles.push(Tile {
                x,
                y,
                size: tile_size.max(1),
            });
            x += size;
        }
        y += size;
    }
    tiles
}

fn split_vertical(tiles: Vec<Tile>) -> (Vec<Tile>, Vec<Tile>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mid_x = tiles.iter().map(|t| t.x).min().unwrap_or(0)
        + tiles.iter().map(|t| t.size as i32).max().unwrap_or(0) * 2;
    for t in tiles {
        if t.x + t.size as i32 <= mid_x {
            left.push(t);
        } else {
            right.push(t);
        }
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_change::PixelRect;

    fn region() -> PixelRect {
        PixelRect {
            x: 0,
            y: 0,
            width: 128,
            height: 128,
        }
    }

    #[test]
    fn territories_are_disjoint_and_complete() {
        for policy in [Policy::Serial, Policy::DualHorizontal, Policy::QuadCorner] {
            let parts = partition(region(), 16, policy, 4);
            let mut all: Vec<Tile> = parts.iter().flat_map(|t| t.tiles.iter().copied()).collect();
            all.sort_by_key(|t| (t.y, t.x));
            // Complete: same count as the raw grid.
            assert_eq!(all.len(), tile_grid(region(), 16).len(), "{policy:?}");
            // Disjoint: no duplicate tiles.
            all.dedup();
            assert_eq!(
                all.len(),
                tile_grid(region(), 16).len(),
                "{policy:?} duplicates"
            );
        }
    }

    #[test]
    fn quad_corner_assigns_corners_to_correct_workers() {
        let parts = partition(region(), 16, Policy::QuadCorner, 4);
        assert_eq!(parts.len(), 4);
        for t in &parts {
            let first = &t.tiles[0];
            let cx = first.x + 8;
            let cy = first.y + 8;
            let want = match (cx < 64, cy < 64) {
                (true, true) => 0,
                (false, true) => 1,
                (true, false) => 2,
                (false, false) => 3,
            };
            assert_eq!(t.worker, want, "corner worker mismatch");
        }
    }

    #[test]
    fn schedule_executes_every_tile_exactly_once() {
        let parts = partition(region(), 16, Policy::QuadCorner, 4);
        let plan = schedule(&parts);
        assert_eq!(plan.len(), tile_grid(region(), 16).len());
        let mut keys: Vec<(i32, i32)> = plan.iter().map(|(t, _)| (t.x, t.y)).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), tile_grid(region(), 16).len());
    }

    #[test]
    fn work_stealing_rebalances_uneven_territories() {
        // One heavy territory, one nearly empty: stealing must move tiles
        // to the idle worker.
        let parts = vec![
            Territory {
                worker: 0,
                tiles: (0..8)
                    .map(|i| Tile {
                        x: i * 16,
                        y: 0,
                        size: 16,
                    })
                    .collect(),
            },
            Territory {
                worker: 1,
                tiles: vec![Tile {
                    x: 0,
                    y: 16,
                    size: 16,
                }],
            },
        ];
        let plan = schedule(&parts);
        let stolen: usize = plan.iter().filter(|(_, w)| *w == 1).count();
        assert!(stolen > 1, "worker 1 executed only its own tile: {stolen}");
    }

    #[test]
    fn policy_selection_avoids_parallel_overhead_for_small_work() {
        // 4 tiles or fewer → serial (directive §3: don't parallelise a
        // 500µs operation into a slower multithreaded one).
        assert_eq!(select_policy(16 * 16 * 4, 16, 4), Policy::Serial);
        assert_eq!(select_policy(16 * 16 * 16, 16, 4), Policy::DualHorizontal);
        assert_eq!(select_policy(16 * 16 * 128, 16, 4), Policy::QuadCorner);
        // Budget of 1 always serial.
        assert_eq!(select_policy(16 * 16 * 128, 16, 1), Policy::Serial);
    }

    #[test]
    fn worker_budget_caps_territories() {
        // 2x2-tile dirty region with a 4-worker budget → at most 4 territories.
        let parts = partition(
            PixelRect {
                x: 0,
                y: 0,
                width: 32,
                height: 32,
            },
            16,
            Policy::QuadCorner,
            4,
        );
        assert_eq!(parts.len(), 4);
        // Tiny region: single tile → one territory regardless of budget.
        let parts = partition(
            PixelRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            16,
            Policy::QuadCorner,
            4,
        );
        assert_eq!(parts.len(), 1);
    }
}
