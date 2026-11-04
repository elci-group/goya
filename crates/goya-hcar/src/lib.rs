//! GY-HCAR/1 — Heterogeneous Core-Aware Rendering classification.
//!
//! Extends CSR with P-core/E-core allocation: classify rendering work by
//! change potential, computational cost, perceptual importance and
//! presentation deadline, then preferentially allocate low-urgency work to
//! efficiency cores and latency-critical work to performance cores,
//! subject to measured scheduling and energy costs.
//!
//! Core rules:
//! - Classification is per work item, never permanently per quadrant.
//! - Energy-minimising allocation subject to deadline feasibility
//!   (min Σ E subject to all deadlines met).
//! - On homogeneous CPUs (no distinct classes) HCAR degrades to ordinary
//!   deadline-aware scheduling.
//! - Migration hysteresis: minimum residency before promotion/demotion.
//! - OS-managed is the default affinity mode; controlled affinity only
//!   when benchmarks justify it.

/// Contract identifier implemented by this crate.
pub const HCAR_CONTRACT: &str = "GY-HCAR/1";

/// CPU core classes (GY-HCAR-001 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreClass {
    Performance,
    Efficiency,
    /// Homogeneous CPU: no distinct classes.
    General,
}

/// Affinity policy modes (GY-HCAR-001 §7). Default: OsManaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AffinityMode {
    /// Linux schedules workers; Goya supplies priorities only.
    #[default]
    OsManaged,
    /// Goya prefers certain core classes but permits migration.
    Advisory,
    /// Explicit affinity, only when benchmarks justify it.
    Controlled,
}

/// Estimated cost of a work item on each core class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkCost {
    /// Estimated execution time on a P-core (ms).
    pub time_p: f64,
    /// Estimated execution time on an E-core (ms).
    pub time_e: f64,
    /// Estimated energy on a P-core (relative units).
    pub energy_p: f64,
    /// Estimated energy on an E-core (relative units).
    pub energy_e: f64,
}

/// Per-work-item scheduling inputs (directive §8 RenderWorkItem).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkItem {
    /// Milliseconds until this item's presentation deadline.
    pub deadline_ms: f64,
    /// Latency criticality: critical-path items may deserve P-cores even
    /// when cheap (directive §3).
    pub critical: bool,
    pub cost: WorkCost,
}

/// The classification decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreAssignment {
    Performance,
    Efficiency,
    /// Serial fallback / homogeneous CPU: caller treats as "any core".
    General,
}

/// Heterogeneous workload classifier (directive §2). Deterministic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Classifier {
    /// CPU exposes distinct P/E classes; false → degrade to General.
    pub heterogeneous: bool,
    pub affinity_mode: AffinityMode,
}

impl Default for Classifier {
    fn default() -> Self {
        Classifier {
            heterogeneous: true,
            affinity_mode: AffinityMode::OsManaged,
        }
    }
}

impl Classifier {
    /// Classify one work item: prefer the E-core when the deadline is
    /// comfortably met there AND the item is not critical; otherwise
    /// promote to the P-core. Energy-minimal subject to feasibility.
    pub fn classify(&self, item: &WorkItem) -> CoreAssignment {
        if !self.heterogeneous {
            return CoreAssignment::General;
        }
        // Feasibility first: if the E-core cannot meet the deadline, the
        // P-core is mandatory regardless of energy.
        let e_feasible = item.cost.time_e <= item.deadline_ms;
        if !e_feasible || item.critical {
            return CoreAssignment::Performance;
        }
        // Energy-minimising choice among feasible classes: E-cores are
        // preferred unless the P-core is both faster enough to matter and
        // strictly cheaper in energy (rare; models a turbo-efficient P).
        if item.cost.energy_p < item.cost.energy_e && item.cost.time_p < item.deadline_ms {
            CoreAssignment::Performance
        } else {
            CoreAssignment::Efficiency
        }
    }
}

/// Migration hysteresis (GY-HCAR-001 §6): a work region keeps its previous
/// core class unless the new classification persists for
/// `min_residency` consecutive frames.
#[derive(Debug)]
pub struct Residency {
    last_class: Option<CoreAssignment>,
    pending: Option<CoreAssignment>,
    pending_count: u32,
    min_residency: u32,
}

impl Residency {
    pub fn new(min_residency: u32) -> Self {
        Residency {
            last_class: None,
            pending: None,
            pending_count: 0,
            min_residency: min_residency.max(1),
        }
    }

    pub fn step(&mut self, class: CoreAssignment) -> CoreAssignment {
        // Seed residency with the first observed class: it is authoritative
        // immediately (there is nothing to be resident from).
        let Some(last) = self.last_class else {
            self.last_class = Some(class);
            self.pending = None;
            self.pending_count = 0;
            return class;
        };
        if class == last {
            self.pending = None;
            self.pending_count = 0;
            return class;
        }
        if Some(class) == self.pending {
            self.pending_count += 1;
        } else {
            self.pending = Some(class);
            self.pending_count = 1;
        }
        if self.pending_count >= self.min_residency {
            self.last_class = Some(class);
            self.pending_count = 0;
        }
        self.last_class.unwrap_or(class)
    }
}

/// Queue assignment for a frame's work items: classify all, split into
/// P/E queues (directive §2). General-classified items land in the E queue
/// (any core) with the caller free to run them anywhere.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameQueues {
    pub performance: Vec<WorkItem>,
    pub efficiency: Vec<WorkItem>,
}

pub fn assign_frame(items: &[WorkItem], classifier: &Classifier) -> FrameQueues {
    let mut queues = FrameQueues {
        performance: Vec::new(),
        efficiency: Vec::new(),
    };
    for item in items {
        match classifier.classify(item) {
            CoreAssignment::Performance => queues.performance.push(*item),
            CoreAssignment::Efficiency | CoreAssignment::General => queues.efficiency.push(*item),
        }
    }
    queues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cost(tp: f64, te: f64, ep: f64, ee: f64) -> WorkCost {
        WorkCost {
            time_p: tp,
            time_e: te,
            energy_p: ep,
            energy_e: ee,
        }
    }

    #[test]
    fn e_core_deadline_violation_forces_p_core() {
        let c = Classifier::default();
        let item = WorkItem {
            deadline_ms: 4.0,
            critical: false,
            cost: cost(1.0, 8.0, 5.0, 3.0), // E-core too slow for 4ms deadline
        };
        assert_eq!(c.classify(&item), CoreAssignment::Performance);
    }

    #[test]
    fn critical_items_get_p_cores_even_when_cheap() {
        let c = Classifier::default();
        let item = WorkItem {
            deadline_ms: 16.0,
            critical: true,
            cost: cost(0.5, 2.0, 4.0, 1.0),
        };
        assert_eq!(c.classify(&item), CoreAssignment::Performance);
    }

    #[test]
    fn comfortable_low_energy_work_goes_e_core() {
        let c = Classifier::default();
        let item = WorkItem {
            deadline_ms: 16.0,
            critical: false,
            cost: cost(1.0, 3.0, 5.0, 1.0),
        };
        assert_eq!(c.classify(&item), CoreAssignment::Efficiency);
    }

    #[test]
    fn homogeneous_cpu_degrades_to_general() {
        let c = Classifier {
            heterogeneous: false,
            ..Classifier::default()
        };
        let item = WorkItem {
            deadline_ms: 2.0,
            critical: true,
            cost: cost(1.0, 8.0, 5.0, 3.0),
        };
        assert_eq!(c.classify(&item), CoreAssignment::General);
    }

    #[test]
    fn energy_minimum_respected_within_deadline() {
        // P-core strictly cheaper in energy AND feasible → exception rule.
        let c = Classifier::default();
        let item = WorkItem {
            deadline_ms: 8.0,
            critical: false,
            cost: cost(1.0, 4.0, 0.5, 1.0), // P cheaper in energy here
        };
        assert_eq!(c.classify(&item), CoreAssignment::Performance);
    }

    #[test]
    fn queues_split_by_classification() {
        let c = Classifier::default();
        let items = vec![
            WorkItem {
                deadline_ms: 16.0,
                critical: false,
                cost: cost(1.0, 3.0, 5.0, 1.0),
            },
            WorkItem {
                deadline_ms: 3.0,
                critical: false,
                cost: cost(1.0, 8.0, 5.0, 3.0),
            },
        ];
        let q = assign_frame(&items, &c);
        assert_eq!(q.efficiency.len(), 1);
        assert_eq!(q.performance.len(), 1);
    }

    #[test]
    fn residency_prevents_flip_flopping() {
        let mut r = Residency::new(3);
        let e = CoreAssignment::Efficiency;
        let p = CoreAssignment::Performance;
        assert_eq!(r.step(e), e);
        assert_eq!(r.step(p), e); // 1 frame of P: no migration
        assert_eq!(r.step(p), e); // 2 frames: still no
        assert_eq!(r.step(p), p); // 3 consecutive: migrate
        assert_eq!(r.step(e), p); // single E: no immediate flip
        assert_eq!(r.step(e), p); // two: still resident on P
        assert_eq!(r.step(e), e); // three consecutive: migrate
    }

    #[test]
    fn temporal_persistence_table() {
        // Directive §4's temporal-persistence intuition expressed as
        // classification over identical costs but different urgency.
        let c = Classifier::default();
        // "Scrolling document": cheap per tile but deadline-critical → P.
        let scroll_tile = WorkItem {
            deadline_ms: 6.0,
            critical: true,
            cost: cost(1.0, 3.0, 4.0, 2.0),
        };
        assert_eq!(c.classify(&scroll_tile), CoreAssignment::Performance);
        // "Status indicator": occasional, relaxed → E.
        let status = WorkItem {
            deadline_ms: 200.0,
            critical: false,
            cost: cost(1.0, 3.0, 4.0, 2.0),
        };
        assert_eq!(c.classify(&status), CoreAssignment::Efficiency);
    }
}
