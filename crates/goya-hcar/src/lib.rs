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

/// Detected Linux CPU class counts. Detection is advisory metadata only;
/// callers must still choose an affinity policy separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTopology {
    pub cpus: Vec<CpuClassAssignment>,
    pub performance_cpus: usize,
    pub efficiency_cpus: usize,
    pub source: TopologySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuClassAssignment {
    pub cpu_id: u32,
    pub class: CoreClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologySource {
    CpuCapacity,
    AcpiHighestPerformance,
}

impl CpuTopology {
    /// Detect online Linux CPUs below `/sys/devices/system/cpu`.
    /// Returns `None` when the online set or all usable class signals are
    /// unavailable/incomplete. This does not set affinity or alter scheduling.
    #[cfg(target_os = "linux")]
    pub fn detect_linux() -> Option<Self> {
        Self::detect_linux_from("/sys/devices/system/cpu")
    }

    /// Testable Linux sysfs reader. `root` contains `online` and `cpuN/`
    /// directories with `cpu_capacity` or `acpi_cppc/highest_perf` files.
    #[cfg(target_os = "linux")]
    pub fn detect_linux_from(root: impl AsRef<std::path::Path>) -> Option<Self> {
        use std::fs;
        let root = root.as_ref();
        let online = fs::read_to_string(root.join("online")).ok()?;
        let ids = parse_cpu_list(&online)?;
        let read_signal = |relative: &str| {
            ids.iter()
                .map(|id| {
                    fs::read_to_string(root.join(format!("cpu{id}")).join(relative))
                        .ok()?
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
                .collect::<Vec<Option<u64>>>()
        };
        let capacity = read_signal("cpu_capacity");
        let highest_perf = read_signal("acpi_cppc/highest_perf");
        Self::from_cpu_ids_and_signals(&ids, &capacity, &highest_perf)
    }

    /// Classify complete per-CPU sysfs signal sets. Missing, malformed,
    /// non-positive, or uniform signals conservatively return `None`.
    /// `cpu_capacity` is preferred; ACPI highest_perf is fallback only.
    pub fn from_sysfs_signals(
        capacity: &[Option<u64>],
        highest_perf: &[Option<u64>],
    ) -> Option<Self> {
        fn split(values: &[Option<u64>]) -> Option<(usize, usize)> {
            if values.is_empty() {
                return None;
            }
            let parsed: Vec<u64> = values.iter().copied().collect::<Option<_>>()?;
            if parsed.contains(&0) {
                return None;
            }
            let min = *parsed.iter().min()?;
            let max = *parsed.iter().max()?;
            (min < max && parsed.iter().all(|v| *v == min || *v == max)).then(|| {
                (
                    parsed.iter().filter(|v| **v == max).count(),
                    parsed.iter().filter(|v| **v == min).count(),
                )
            })
        }
        if let Some((performance_cpus, efficiency_cpus)) = split(capacity) {
            return Self::from_values(
                capacity,
                TopologySource::CpuCapacity,
                performance_cpus,
                efficiency_cpus,
            );
        }
        let (performance_cpus, efficiency_cpus) = split(highest_perf)?;
        Self::from_values(
            highest_perf,
            TopologySource::AcpiHighestPerformance,
            performance_cpus,
            efficiency_cpus,
        )
    }

    fn from_values(
        values: &[Option<u64>],
        source: TopologySource,
        performance_cpus: usize,
        efficiency_cpus: usize,
    ) -> Option<Self> {
        let parsed: Vec<u64> = values.iter().copied().collect::<Option<_>>()?;
        let max = *parsed.iter().max()?;
        let cpus = parsed
            .iter()
            .enumerate()
            .map(|(cpu_id, value)| CpuClassAssignment {
                cpu_id: cpu_id as u32,
                class: if *value == max {
                    CoreClass::Performance
                } else {
                    CoreClass::Efficiency
                },
            })
            .collect();
        Some(Self {
            cpus,
            performance_cpus,
            efficiency_cpus,
            source,
        })
    }

    fn from_cpu_ids_and_signals(
        ids: &[u32],
        capacity: &[Option<u64>],
        highest_perf: &[Option<u64>],
    ) -> Option<Self> {
        if ids.is_empty() || ids.len() != capacity.len() || ids.len() != highest_perf.len() {
            return None;
        }
        let (source, values) = if Self::from_sysfs_signals(capacity, &[]).is_some() {
            (TopologySource::CpuCapacity, capacity)
        } else if Self::from_sysfs_signals(&[], highest_perf).is_some() {
            (TopologySource::AcpiHighestPerformance, highest_perf)
        } else {
            return None;
        };
        let parsed: Vec<u64> = values.iter().copied().collect::<Option<_>>()?;
        let max = *parsed.iter().max()?;
        let cpus: Vec<_> = ids
            .iter()
            .zip(parsed)
            .map(|(cpu_id, value)| CpuClassAssignment {
                cpu_id: *cpu_id,
                class: if value == max {
                    CoreClass::Performance
                } else {
                    CoreClass::Efficiency
                },
            })
            .collect();
        Some(Self {
            performance_cpus: cpus
                .iter()
                .filter(|c| c.class == CoreClass::Performance)
                .count(),
            efficiency_cpus: cpus
                .iter()
                .filter(|c| c.class == CoreClass::Efficiency)
                .count(),
            cpus,
            source,
        })
    }
}

#[cfg(target_os = "linux")]
fn parse_cpu_list(value: &str) -> Option<Vec<u32>> {
    let mut ids = Vec::new();
    for part in value.trim().split(',') {
        let mut ends = part.split('-');
        let start = ends.next()?.parse::<u32>().ok()?;
        let end = ends
            .next()
            .map(str::parse::<u32>)
            .transpose()
            .ok()?
            .unwrap_or(start);
        if ends.next().is_some() || end < start || end - start > 4096 {
            return None;
        }
        ids.extend(start..=end);
    }
    ids.sort_unstable();
    ids.dedup();
    (!ids.is_empty()).then_some(ids)
}

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

    #[test]
    fn topology_prefers_capacity_and_falls_back_conservatively() {
        let topo = CpuTopology::from_sysfs_signals(
            &[Some(1024), Some(1024), Some(512), Some(512)],
            &[Some(64), Some(64), Some(32), Some(32)],
        )
        .unwrap();
        assert_eq!(
            topo,
            CpuTopology {
                cpus: vec![
                    CpuClassAssignment {
                        cpu_id: 0,
                        class: CoreClass::Performance
                    },
                    CpuClassAssignment {
                        cpu_id: 1,
                        class: CoreClass::Performance
                    },
                    CpuClassAssignment {
                        cpu_id: 2,
                        class: CoreClass::Efficiency
                    },
                    CpuClassAssignment {
                        cpu_id: 3,
                        class: CoreClass::Efficiency
                    },
                ],
                performance_cpus: 2,
                efficiency_cpus: 2,
                source: TopologySource::CpuCapacity
            }
        );
        let fallback = CpuTopology::from_sysfs_signals(
            &[Some(1024), Some(1024), Some(1024)],
            &[Some(64), Some(64), Some(32)],
        )
        .unwrap();
        assert_eq!(fallback.source, TopologySource::AcpiHighestPerformance);
        assert_eq!(
            (fallback.performance_cpus, fallback.efficiency_cpus),
            (2, 1)
        );
        assert!(
            CpuTopology::from_sysfs_signals(&[Some(0), Some(512)], &[Some(0), Some(64)]).is_none()
        );
        assert!(
            CpuTopology::from_sysfs_signals(&[Some(512), Some(512)], &[Some(64), Some(64)])
                .is_none()
        );
        assert!(CpuTopology::from_sysfs_signals(&[None, Some(512)], &[Some(64), None]).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn injected_sysfs_root_uses_online_cpu_ids_and_fallback() {
        use std::{
            fs,
            sync::atomic::{AtomicU64, Ordering},
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "goya-hcar-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("online"), "2-3,7\n").unwrap();
        for (cpu, cap, perf) in [(2, "1024", "64"), (3, "512", "32"), (7, "1024", "64")] {
            let dir = root.join(format!("cpu{cpu}"));
            fs::create_dir_all(dir.join("acpi_cppc")).unwrap();
            fs::write(dir.join("cpu_capacity"), cap).unwrap();
            fs::write(dir.join("acpi_cppc/highest_perf"), perf).unwrap();
        }
        let detected = CpuTopology::detect_linux_from(&root).unwrap();
        assert_eq!(detected.source, TopologySource::CpuCapacity);
        assert_eq!(
            detected.cpus.iter().map(|c| c.cpu_id).collect::<Vec<_>>(),
            vec![2, 3, 7]
        );
        // Make capacity uniform so the complete ACPI signal is selected.
        for cpu in [2, 3, 7] {
            fs::write(root.join(format!("cpu{cpu}/cpu_capacity")), "512").unwrap();
        }
        let fallback = CpuTopology::detect_linux_from(&root).unwrap();
        assert_eq!(fallback.source, TopologySource::AcpiHighestPerformance);
        assert_eq!(fallback.cpus[1].class, CoreClass::Efficiency);
        fs::write(root.join("cpu3/acpi_cppc/highest_perf"), "broken").unwrap();
        assert!(CpuTopology::detect_linux_from(&root).is_none());
        let _ = fs::remove_dir_all(root);
    }

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
