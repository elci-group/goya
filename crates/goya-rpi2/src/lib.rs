//! GY-RPI/1 §6 Level 2 — Learned anticipation (opt-in).
//!
//! A lightweight local statistical model over interaction sequences:
//! a first-order Markov chain over [`ActionId`] transitions. Strictly
//! consent-gated: no training occurs without a valid [`ConsentRecord`],
//! retention is capped with FIFO eviction, and [`SequenceModel::export_summary`]
//! provides a transparent human-readable dump.
//!
//! Types [`ActionId`], [`Calibration`] and [`Level`] mirror the public
//! `goya-rpi` (GY-RPI/1) definitions so the two crates stay compatible;
//! they are re-declared locally to keep this crate std-only and
//! dependency-free.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Contract this module implements.
pub const RPI2_CONTRACT: &str = "GY-RPI/1 §6 Level 2";

/// Identifies an action within the sequence model.
/// Mirrors `goya_rpi::ActionId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActionId(pub u64);

/// Deterministic, explicit consent for learned anticipation.
///
/// Validity is checked against a caller-supplied `now_ns`; no wall clock
/// is consulted anywhere in this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentRecord {
    pub granted: bool,
    pub scope: String,
    /// Expiry in nanoseconds since the same epoch the caller's `now_ns` uses.
    pub expiry_ns: u64,
}

impl ConsentRecord {
    /// A granted, unexpired consent with the given scope.
    pub fn granted(scope: &str, expiry_ns: u64) -> Self {
        Self {
            granted: true,
            scope: scope.to_string(),
            expiry_ns,
        }
    }

    /// A refused/withdrawn consent.
    pub fn refused() -> Self {
        Self {
            granted: false,
            scope: String::new(),
            expiry_ns: 0,
        }
    }

    /// Valid only when granted, non-empty scope, and not expired at `now_ns`.
    pub fn is_valid_at(&self, now_ns: u64) -> bool {
        self.granted && !self.scope.is_empty() && now_ns <= self.expiry_ns
    }
}

impl Default for ConsentRecord {
    fn default() -> Self {
        Self::refused()
    }
}

/// Two-level anticipation policy.
/// Mirrors the GY-RPI level semantics: Level 1 is reactive/heuristic,
/// Level 2 adds learned anticipation and must be opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// No model or no consent: reactive only.
    Level1,
    /// Model present AND consent valid: learned anticipation active.
    Level2,
}

/// Two-level policy: Level 2 requires BOTH a model and valid consent.
pub fn anticipation_level(model_present: bool, consent: bool) -> Level {
    if model_present && consent {
        Level::Level2
    } else {
        Level::Level1
    }
}

/// Local mirror of `goya_rpi::Calibration` (Brier-score checker) so
/// predicted probabilities can be scored without a cross-crate dependency.
#[derive(Debug, Clone, Default)]
pub struct Calibration {
    pub predicted_sum: f64,
    pub hits: u64,
    pub n: u64,
    squared_error_sum: f64,
}

impl Calibration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a prediction and whether the predicted event occurred.
    pub fn record(&mut self, predicted: f64, occurred: bool) {
        // NaN poisons the accumulated score permanently. Treat any
        // non-finite probability as an invalid sample and leave the
        // calibration state untouched; finite out-of-range values remain
        // clamped below.
        if !predicted.is_finite() {
            return;
        }
        let p = predicted.clamp(0.0, 1.0);
        let y = if occurred { 1.0 } else { 0.0 };
        self.predicted_sum += p;
        if occurred {
            self.hits += 1;
        }
        self.squared_error_sum += (p - y) * (p - y);
        self.n += 1;
    }

    /// Brier score (mean squared error). 0.0 = perfect; 0.0 when empty.
    pub fn brier_score(&self) -> f64 {
        if self.n == 0 {
            return 0.0;
        }
        self.squared_error_sum / self.n as f64
    }
}

/// First-order Markov chain over [`ActionId`] transitions.
///
/// Counts live in a `BTreeMap<(ActionId, ActionId), u64>`; insertion order
/// is tracked separately so the `max_transitions` retention cap can evict
/// the oldest observed transition first (FIFO). All operations are
/// deterministic.
#[derive(Debug, Clone)]
pub struct SequenceModel {
    consent: ConsentRecord,
    max_transitions: usize,
    /// transition -> count.
    counts: BTreeMap<(ActionId, ActionId), u64>,
    /// FIFO insertion order of transition keys (oldest first).
    order: Vec<(ActionId, ActionId)>,
    /// Per-source outgoing totals, maintained for O(1) normalisation.
    outgoing: BTreeMap<ActionId, u64>,
    learned_transitions: u64,
}

impl SequenceModel {
    /// Default retention cap.
    pub const DEFAULT_MAX_TRANSITIONS: usize = 1024;

    /// A model that will never learn (no consent).
    pub fn without_consent() -> Self {
        Self::with_consent(ConsentRecord::refused())
    }

    /// A model gated on the given consent record.
    pub fn with_consent(consent: ConsentRecord) -> Self {
        Self::with_consent_and_cap(consent, Self::DEFAULT_MAX_TRANSITIONS)
    }

    /// A model gated on `consent` with an explicit retention cap.
    pub fn with_consent_and_cap(consent: ConsentRecord, max_transitions: usize) -> Self {
        Self {
            consent,
            max_transitions: max_transitions.max(1),
            counts: BTreeMap::new(),
            order: Vec::new(),
            outgoing: BTreeMap::new(),
            learned_transitions: 0,
        }
    }

    /// Whether training is currently permitted: consent granted, non-empty
    /// scope, and unexpired at `now_ns`.
    pub fn can_learn_at(&self, now_ns: u64) -> bool {
        self.consent.is_valid_at(now_ns)
    }

    /// Number of distinct retained transitions.
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    /// True when no transitions are retained.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Retention cap.
    pub fn max_transitions(&self) -> usize {
        self.max_transitions
    }

    /// Train on one interaction sequence. Refused (no-op) unless consent
    /// is valid at `now_ns`. Consecutive equal actions produce self-
    /// transitions and are counted like any other.
    pub fn observe_sequence(&mut self, sequence: &[ActionId], now_ns: u64) {
        if !self.can_learn_at(now_ns) {
            return;
        }
        for pair in sequence.windows(2) {
            self.record_transition((pair[0], pair[1]));
        }
    }

    fn record_transition(&mut self, key: (ActionId, ActionId)) {
        if !self.counts.contains_key(&key) {
            self.order.push(key);
        }
        *self.counts.entry(key).or_insert(0) += 1;
        *self.outgoing.entry(key.0).or_insert(0) += 1;
        self.learned_transitions += 1;
        self.evict_to_cap();
    }

    /// FIFO eviction: drop oldest-inserted transitions until within cap.
    fn evict_to_cap(&mut self) {
        while self.counts.len() > self.max_transitions {
            let oldest = self.order.remove(0);
            if let Some(c) = self.counts.remove(&oldest) {
                if let Some(total) = self.outgoing.get_mut(&oldest.0) {
                    *total = total.saturating_sub(c);
                }
            }
        }
    }

    /// Most likely successor of `last`, with its normalised probability.
    /// `None` when nothing was ever observed after `last`.
    pub fn predict_next(&self, last: ActionId) -> Option<(ActionId, f64)> {
        let total = *self.outgoing.get(&last)?;
        if total == 0 {
            return None;
        }
        // Deterministic tie-break: highest count, then lowest ActionId.
        let best = self
            .counts
            .iter()
            .filter(|((from, _), _)| *from == last)
            .max_by_key(|((_, to), &c)| (c, std::cmp::Reverse(*to)))?;
        Some((best.0 .1, *best.1 as f64 / total as f64))
    }

    /// Normalised probability of a specific successor. 0.0 when unknown.
    pub fn probability_of(&self, from: ActionId, to: ActionId) -> f64 {
        let total = match self.outgoing.get(&from) {
            Some(t) if *t > 0 => *t,
            _ => return 0.0,
        };
        self.counts
            .get(&(from, to))
            .map(|&c| c as f64 / total as f64)
            .unwrap_or(0.0)
    }

    /// Transparent human-readable dump: consent state, retention, and the
    /// top transitions by count (descending; count then source then target
    /// for determinism).
    pub fn export_summary(&self) -> String {
        let mut summary = String::new();
        summary.push_str("GY-RPI/1 Level 2 sequence model summary\n");
        summary.push_str(&format!(
            "consent: {} (scope: '{}', expires_ns: {})\n",
            if self.consent.granted {
                "granted"
            } else {
                "refused"
            },
            self.consent.scope,
            self.consent.expiry_ns
        ));
        summary.push_str(&format!(
            "retention: {}/{} transitions, {} total observations\n",
            self.counts.len(),
            self.max_transitions,
            self.learned_transitions
        ));
        let mut ranked: Vec<(&(ActionId, ActionId), &u64)> = self.counts.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        summary.push_str("top transitions:\n");
        for (&(from, to), &count) in ranked.iter().take(10) {
            let total = self.outgoing.get(&from).copied().unwrap_or(1).max(1);
            summary.push_str(&format!(
                "  {} -> {} count={} p={:.3}\n",
                from.0,
                to.0,
                count,
                count as f64 / total as f64
            ));
        }
        summary
    }

    /// Erase all learned transitions (transparent right to erasure).
    /// Consent is retained as-is; learning resumes only if still valid.
    pub fn forget_all(&mut self) {
        self.counts.clear();
        self.order.clear();
        self.outgoing.clear();
        self.learned_transitions = 0;
    }
}

impl Default for SequenceModel {
    fn default() -> Self {
        Self::without_consent()
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Level::Level1 => write!(f, "Level1"),
            Level::Level2 => write!(f, "Level2"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000_000;

    fn consent_valid_until(expiry_ns: u64) -> ConsentRecord {
        ConsentRecord::granted("interaction-sequences", expiry_ns)
    }

    fn model_consented() -> SequenceModel {
        SequenceModel::with_consent(consent_valid_until(T0 + 10_000))
    }

    #[test]
    fn predicts_most_likely_successor_with_normalised_probability() {
        let mut model = model_consented();
        // After 1, action 2 dominates (3 vs 1).
        let seq: Vec<ActionId> = [1, 2, 1, 2, 1, 2, 1, 3]
            .iter()
            .map(|&a| ActionId(a))
            .collect();
        model.observe_sequence(&seq, T0);
        let (next, p) = model.predict_next(ActionId(1)).expect("trained");
        assert_eq!(next, ActionId(2));
        assert!((p - 0.75).abs() < 1e-9, "p = {p}");
        // Probabilities out of a source sum to 1.
        let sum = model.probability_of(ActionId(1), ActionId(2))
            + model.probability_of(ActionId(1), ActionId(3));
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn unseen_last_action_yields_none() {
        let mut model = model_consented();
        model.observe_sequence(&[ActionId(1), ActionId(2)], T0);
        assert_eq!(model.predict_next(ActionId(99)), None);
        // Empty model too.
        assert_eq!(
            SequenceModel::without_consent().predict_next(ActionId(1)),
            None
        );
    }

    #[test]
    fn refused_consent_means_no_learning() {
        let mut model = SequenceModel::with_consent(ConsentRecord::refused());
        model.observe_sequence(&[ActionId(1), ActionId(2), ActionId(1), ActionId(2)], T0);
        assert!(model.is_empty(), "must not learn without consent");
        assert_eq!(model.predict_next(ActionId(1)), None);
    }

    #[test]
    fn expired_consent_is_refused() {
        let mut model = SequenceModel::with_consent(consent_valid_until(T0 + 100));
        assert!(model.can_learn_at(T0 + 100), "valid up to and incl. expiry");
        model.observe_sequence(&[ActionId(1), ActionId(2)], T0);
        assert_eq!(model.len(), 1);
        // After expiry: refused.
        assert!(!model.can_learn_at(T0 + 101));
        let before = model.export_summary();
        model.observe_sequence(&[ActionId(2), ActionId(3)], T0 + 101);
        assert_eq!(model.len(), 1, "no post-expiry learning");
        assert_eq!(model.export_summary(), before);
        assert_eq!(model.predict_next(ActionId(2)), None);
    }

    #[test]
    fn retention_cap_evicts_oldest_transitions_fifo() {
        let mut model = SequenceModel::with_consent_and_cap(consent_valid_until(T0 + 1_000_000), 2);
        model.observe_sequence(&[ActionId(1), ActionId(2), ActionId(3), ActionId(4)], T0);
        // Transitions observed in order: (1,2), (2,3), (3,4). Cap 2 keeps the
        // two newest.
        assert_eq!(model.len(), 2);
        assert_eq!(model.predict_next(ActionId(1)), None, "(1,2) evicted first");
        assert_eq!(
            model.predict_next(ActionId(2)).map(|(a, _)| a),
            Some(ActionId(3))
        );
        assert_eq!(
            model.predict_next(ActionId(3)).map(|(a, _)| a),
            Some(ActionId(4))
        );
    }

    #[test]
    fn summary_contains_top_transition_and_is_transparent() {
        let mut model = model_consented();
        let seq: Vec<ActionId> = [5, 6, 5, 6, 5, 6, 5, 7]
            .iter()
            .map(|&a| ActionId(a))
            .collect();
        model.observe_sequence(&seq, T0);
        let summary = model.export_summary();
        assert!(
            summary.contains("5 -> 6"),
            "top transition present: {summary}"
        );
        assert!(summary.contains("count=3"));
        assert!(summary.contains("granted"));
        assert!(summary.contains("interaction-sequences"));
        // forget_all clears and the summary reflects it.
        model.forget_all();
        assert!(model.is_empty());
        assert!(!model.export_summary().contains("5 -> 6"));
        // Consent survives forget_all; learning can resume.
        model.observe_sequence(&[ActionId(5), ActionId(6)], T0 + 1);
        assert_eq!(
            model.predict_next(ActionId(5)).map(|(a, _)| a),
            Some(ActionId(6))
        );
    }

    #[test]
    fn two_level_policy_matrix() {
        assert_eq!(anticipation_level(false, false), Level::Level1);
        assert_eq!(anticipation_level(true, false), Level::Level1);
        assert_eq!(anticipation_level(false, true), Level::Level1);
        assert_eq!(anticipation_level(true, true), Level::Level2);
    }

    #[test]
    fn calibrated_predictions_beat_random_half_baseline() {
        // Synthetic stream with strong structure: mostly 1 -> 2, occasionally
        // 1 -> 3. Train, then score the model's own predictions on the same
        // deterministic stream via the local Brier checker.
        let mut model = model_consented();
        let mut stream = Vec::new();
        for _ in 0..40 {
            stream.push(ActionId(1));
            stream.push(ActionId(2));
        }
        for _ in 0..10 {
            stream.push(ActionId(1));
            stream.push(ActionId(3));
        }
        model.observe_sequence(&stream, T0);
        assert!((model.probability_of(ActionId(1), ActionId(2)) - 0.8).abs() < 1e-9);

        let mut calibrated = Calibration::new();
        let mut random = Calibration::new();
        for pair in stream.windows(2) {
            if pair[0] != ActionId(1) {
                continue;
            }
            let occurred = pair[1] == ActionId(2);
            let (predicted_action, p) = model.predict_next(ActionId(1)).expect("trained");
            assert_eq!(predicted_action, ActionId(2));
            calibrated.record(p, occurred);
            random.record(0.5, occurred);
        }
        assert!(
            calibrated.brier_score() < random.brier_score(),
            "calibrated {} must beat random 0.5 baseline {}",
            calibrated.brier_score(),
            random.brier_score()
        );
        // And against a constant 0.8 guess on the same events, sharpened
        // prediction is still no worse (0.8 is optimal here, tie allowed).
        assert!(calibrated.brier_score() <= 0.2 + 1e-9);
    }

    #[test]
    fn consent_scope_must_be_non_empty() {
        let mut model = SequenceModel::with_consent(ConsentRecord {
            granted: true,
            scope: String::new(),
            expiry_ns: u64::MAX,
        });
        assert!(!model.can_learn_at(T0), "blank scope is not valid consent");
        model.observe_sequence(&[ActionId(1), ActionId(2)], T0);
        assert!(model.is_empty());
    }

    #[test]
    fn contract_constant_matches_spec() {
        assert_eq!(RPI2_CONTRACT, "GY-RPI/1 §6 Level 2");
    }
}

/// GY-HCAR hardware dispatch — Linux implementation (directive §7).
///
/// Despite the crate name (a historical pre-allocation placeholder), this
/// section implements the Linux side of GY-HCAR dispatch:
///
/// 1. **Topology detection** — parse `/proc/cpuinfo` (std-only, never
///    panics when the file is missing, e.g. on non-Linux CI) and classify
///    logical CPUs into [`CoreClass`]s. Per directive §7, logical CPU
///    numbering is *never* trusted alone: homogeneity is decided by
///    comparing `model name` strings, and a P/E split is attempted only
///    through a frequency proxy (max `cpu MHz` per distinct model name;
///    the faster family is Performance, the slower Efficiency).
/// 2. **Work dispatch** — run [`DispatchJob`]s on plain `std::thread`
///    workers. Affinity mode is OS-managed by default: the Linux scheduler
///    decides physical placement; Goya supplies *ordering* only —
///    performance-classified items are dispatched first.
/// 3. **Verification** — measure per-item wall time and report whether
///    each item met its presentation deadline.
pub mod hcar {
    use super::{thread, Duration, Instant, Mutex};
    use std::collections::BTreeMap;

    /// Contract identifier this module implements.
    pub const HCAR_DISPATCH_CONTRACT: &str = "GY-HCAR/1";

    /// CPU core classes (GY-HCAR-001 §8), mirroring `goya_hcar::CoreClass`.
    /// Re-declared locally: goya-hcar is not a dependency of this crate.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CoreClass {
        Performance,
        Efficiency,
        /// Homogeneous CPU: no distinct classes.
        General,
    }

    /// Per-work-item scheduling inputs (directive §8 RenderWorkItem),
    /// field-compatible with `goya_hcar::WorkItem`.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct WorkItem {
        /// Milliseconds until this item's presentation deadline.
        pub deadline_ms: f64,
        /// Latency criticality: critical-path items deserve P-cores.
        pub critical: bool,
        /// Estimated execution time on a P-core (ms).
        pub time_p: f64,
        /// Estimated execution time on an E-core (ms).
        pub time_e: f64,
    }

    impl Default for WorkItem {
        fn default() -> Self {
            WorkItem {
                deadline_ms: f64::INFINITY,
                critical: false,
                time_p: 0.0,
                time_e: 0.0,
            }
        }
    }

    /// One dispatchable unit of work. `us_work` is spun for deterministically
    /// so tests and deadline verification are repeatable.
    #[derive(Debug, Clone)]
    pub struct DispatchJob {
        /// Scheduling description (deadline, criticality, costs).
        pub desc: WorkItem,
        /// Microseconds of deterministic spin work to execute.
        pub us_work: u64,
    }

    impl DispatchJob {
        /// Classify this job against a topology, mirroring goya-hcar's
        /// energy-minimal-subject-to-feasibility rule.
        pub fn classify(&self, topo: &DetectedTopology) -> CoreClass {
            if topo.homogeneous {
                return CoreClass::General;
            }
            let e_feasible = self.desc.time_e <= self.desc.deadline_ms;
            if !e_feasible || self.desc.critical {
                return CoreClass::Performance;
            }
            CoreClass::Efficiency
        }
    }

    /// Detected CPU topology.
    #[derive(Debug, Clone, PartialEq)]
    pub struct DetectedTopology {
        /// One entry per `processor` found in cpuinfo.
        pub classes: Vec<(u32, CoreClass)>,
        /// True when all model names are identical (degrade to General).
        pub homogeneous: bool,
        /// Where the topology came from (e.g. "/proc/cpuinfo", "synthetic").
        pub source: &'static str,
    }

    impl DetectedTopology {
        /// Detect the topology of the running machine. Never panics; on any
        /// I/O failure (non-Linux, no /proc) returns an empty, homogeneous
        /// fallback topology.
        pub fn detect() -> DetectedTopology {
            match std::fs::read_to_string("/proc/cpuinfo") {
                Ok(text) => Topology::from_text(&text),
                Err(_) => DetectedTopology {
                    classes: Vec::new(),
                    homogeneous: true,
                    source: "/proc/cpuinfo",
                },
            }
        }

        /// Highest class observed (Performance > Efficiency > General).
        pub fn peak_class(&self) -> CoreClass {
            if self.homogeneous {
                return CoreClass::General;
            }
            let has = |c: CoreClass| self.classes.iter().any(|(_, k)| *k == c);
            if has(CoreClass::Performance) {
                CoreClass::Performance
            } else if has(CoreClass::Efficiency) {
                CoreClass::Efficiency
            } else {
                CoreClass::General
            }
        }
    }

    /// Topology analysis entry point.
    pub struct Topology;

    /// One raw `processor` entry from cpuinfo.
    #[derive(Debug, Clone, PartialEq)]
    struct CpuEntry {
        logical_id: u32,
        model_name: String,
        mhz: f64,
    }

    impl Topology {
        /// Parse cpuinfo text into a classified topology. Injectable so
        /// tests can pass synthetic cpuinfo text.
        pub fn from_text(text: &str) -> DetectedTopology {
            let entries = parse_cpuinfo(text);
            classify_entries(entries, "synthetic")
        }
    }

    /// Parse `/proc/cpuinfo`-style text into per-processor entries.
    /// Tolerant of missing fields and unrelated blocks; never panics.
    fn parse_cpuinfo(text: &str) -> Vec<CpuEntry> {
        let mut entries: Vec<CpuEntry> = Vec::new();
        let mut cur_id: Option<u32> = None;
        let mut cur_name: Option<String> = None;
        let mut cur_mhz: Option<f64> = None;

        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                // Blank line terminates a processor block on real cpuinfo.
                if line.trim().is_empty() {
                    flush(&mut entries, &mut cur_id, &mut cur_name, &mut cur_mhz);
                }
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "processor" => {
                    // A new "processor" key also ends any unterminated block.
                    flush(&mut entries, &mut cur_id, &mut cur_name, &mut cur_mhz);
                    cur_id = value
                        .trim_start_matches(|c: char| !c.is_ascii_digit())
                        .split(|c: char| !c.is_ascii_digit())
                        .next()
                        .and_then(|d| d.parse::<u32>().ok());
                }
                "model name" => cur_name = Some(value.to_string()),
                "cpu MHz" => cur_mhz = value.parse::<f64>().ok(),
                _ => {}
            }
        }
        flush(&mut entries, &mut cur_id, &mut cur_name, &mut cur_mhz);
        entries
    }

    /// Push a pending processor block (if any) into `entries`.
    fn flush(
        entries: &mut Vec<CpuEntry>,
        id: &mut Option<u32>,
        name: &mut Option<String>,
        mhz: &mut Option<f64>,
    ) {
        if let Some(logical_id) = id.take() {
            entries.push(CpuEntry {
                logical_id,
                model_name: name.take().unwrap_or_default(),
                mhz: mhz.take().unwrap_or(0.0),
            });
        } else {
            name.take();
            mhz.take();
        }
    }

    /// Classify parsed entries: identical model names → homogeneous
    /// General; distinct names → P/E split by the frequency proxy
    /// (per-model max MHz; faster family = Performance, slower =
    /// Efficiency).
    fn classify_entries(entries: Vec<CpuEntry>, source: &'static str) -> DetectedTopology {
        if entries.is_empty() {
            return DetectedTopology {
                classes: Vec::new(),
                homogeneous: true,
                source,
            };
        }
        let first_name = &entries[0].model_name;
        let homogeneous = entries.iter().all(|e| &e.model_name == first_name);
        if homogeneous {
            let classes = entries
                .into_iter()
                .map(|e| (e.logical_id, CoreClass::General))
                .collect();
            return DetectedTopology {
                classes,
                homogeneous: true,
                source,
            };
        }

        // Frequency proxy: max MHz observed per distinct model name.
        let mut max_mhz: BTreeMap<String, f64> = BTreeMap::new();
        for e in &entries {
            let slot = max_mhz.entry(e.model_name.clone()).or_insert(0.0);
            if e.mhz > *slot {
                *slot = e.mhz;
            }
        }
        let fastest = max_mhz.values().copied().fold(f64::NEG_INFINITY, f64::max);
        let classes = entries
            .into_iter()
            .map(|e| {
                let mhz = max_mhz.get(&e.model_name).copied().unwrap_or(0.0);
                // Distinct names already proven; the fastest family is the
                // Performance class, every other family Efficiency.
                let class = if mhz >= fastest {
                    CoreClass::Performance
                } else {
                    CoreClass::Efficiency
                };
                (e.logical_id, class)
            })
            .collect();
        DetectedTopology {
            classes,
            homogeneous: false,
            source,
        }
    }

    /// Per-item result of a dispatch run.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ItemUsage {
        /// Index of the job in the input slice.
        pub item_idx: usize,
        /// Measured wall time of the item's execution, microseconds.
        pub us: u64,
        /// Class the item was dispatched as.
        pub class: CoreClass,
        /// Whether the item finished within its presentation deadline.
        pub deadline_met: bool,
    }

    /// Report of a full dispatch run.
    #[derive(Debug, Clone, PartialEq)]
    pub struct DispatchReport {
        pub per_item_usage: Vec<ItemUsage>,
        /// Total wall time of the whole dispatch, microseconds.
        pub total_us: u64,
        /// The order in which items were handed to workers.
        pub dispatch_order: Vec<usize>,
    }

    /// Work dispatch entry point (OS-managed affinity, directive §7).
    pub struct WorkDispatch;

    impl WorkDispatch {
        /// Dispatch jobs across a worker pool, OS-managed affinity mode.
        ///
        /// Ordering guarantee: performance-classified items are enqueued and
        /// claimed before efficiency items, which precede General items;
        /// input order is preserved within a class. The Linux scheduler
        /// decides physical core placement; we supply ordering only.
        ///
        /// Workers = `min(available_parallelism, jobs)`, at least 1.
        pub fn dispatch(jobs: &[DispatchJob], topo: &DetectedTopology) -> DispatchReport {
            let total_start = Instant::now();

            // Classify once, then order: Performance, Efficiency, General;
            // stable within a class.
            let mut class_of = vec![CoreClass::General; jobs.len()];
            let mut queue: Vec<(usize, u8)> = jobs
                .iter()
                .enumerate()
                .map(|(i, j)| {
                    let class = j.classify(topo);
                    class_of[i] = class;
                    let prio = match class {
                        CoreClass::Performance => 0u8,
                        CoreClass::Efficiency => 1,
                        CoreClass::General => 2,
                    };
                    (i, prio)
                })
                .collect();
            queue.sort_by_key(|&(_, prio)| prio);
            // Workers claim via Vec::pop (from the tail); reversing the
            // ascending order makes the tail the highest-priority item
            // while preserving input order within each class.
            queue.reverse();

            let queue = Mutex::new(queue);
            let dispatch_order_log: Mutex<Vec<usize>> = Mutex::new(Vec::new());
            let usage: Mutex<Vec<ItemUsage>> = Mutex::new(Vec::new());

            let workers = thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
                .max(1)
                .min(jobs.len().max(1));

            thread::scope(|scope| {
                for _ in 0..workers {
                    scope.spawn(|| loop {
                        // Claim the next item under the queue lock, so the
                        // dispatch order is exactly the priority order.
                        let next = queue.lock().expect("queue mutex poisoned").pop();
                        let Some((idx, _prio)) = next else { break };
                        dispatch_order_log
                            .lock()
                            .expect("order mutex poisoned")
                            .push(idx);
                        let start = Instant::now();
                        spin_for(Duration::from_micros(jobs[idx].us_work));
                        let us = start.elapsed().as_micros() as u64;
                        let deadline_us = (jobs[idx].desc.deadline_ms * 1000.0).max(0.0);
                        usage.lock().expect("usage mutex poisoned").push(ItemUsage {
                            item_idx: idx,
                            us,
                            class: class_of[idx],
                            deadline_met: us as f64 <= deadline_us,
                        });
                    });
                }
            });

            let mut per_item_usage = usage.into_inner().expect("usage mutex poisoned");
            per_item_usage.sort_by_key(|u| u.item_idx);
            DispatchReport {
                per_item_usage,
                total_us: total_start.elapsed().as_micros() as u64,
                dispatch_order: dispatch_order_log
                    .into_inner()
                    .expect("order mutex poisoned"),
            }
        }
    }

    /// Busy-spin for `d`. Deterministic placeholder for real rendering work
    /// so tests can verify deadlines without external dependencies.
    fn spin_for(d: Duration) {
        let start = Instant::now();
        while start.elapsed() < d {
            std::hint::spin_loop();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const SYNTHETIC_PE: &str = "\
processor\t: 0
model name\t: Cortex-A78
cpu MHz\t\t: 2400.000

processor\t: 1
model name\t: Cortex-A78
cpu MHz\t\t: 2400.000

processor\t: 2
model name\t: Cortex-A55
cpu MHz\t\t: 1800.000

processor\t: 3
model name\t: Cortex-A55
cpu MHz\t\t: 1800.000
";

        const SYNTHETIC_HOMO: &str = "\
processor\t: 0
model name\t: Cortex-A53
cpu MHz\t\t: 1200.000

processor\t: 1
model name\t: Cortex-A53
cpu MHz\t\t: 1200.000
";

        fn cheap_job(deadline_ms: f64, critical: bool, us_work: u64) -> DispatchJob {
            DispatchJob {
                desc: WorkItem {
                    deadline_ms,
                    critical,
                    time_p: 1.0,
                    time_e: 2.0,
                },
                us_work,
            }
        }

        #[test]
        fn detect_real_cpuinfo_never_panics() {
            let topo = DetectedTopology::detect();
            assert!(!topo.source.is_empty());
            assert!(topo.homogeneous || topo.classes.len() >= 2);
            if !topo.classes.is_empty() {
                assert!(topo.classes.iter().all(|(id, _)| *id < 4096));
            }
        }

        #[test]
        fn homogeneous_cpu_degrades_to_general() {
            let topo = Topology::from_text(SYNTHETIC_HOMO);
            assert!(topo.homogeneous);
            assert_eq!(topo.classes.len(), 2);
            assert!(topo.classes.iter().all(|(_, c)| *c == CoreClass::General));
            assert_eq!(topo.peak_class(), CoreClass::General);
        }

        #[test]
        fn pe_split_by_frequency_proxy() {
            let topo = Topology::from_text(SYNTHETIC_PE);
            assert!(!topo.homogeneous);
            let class_of = |id: u32| {
                topo.classes
                    .iter()
                    .find(|&&(i, _)| i == id)
                    .map(|&(_, c)| c)
                    .expect("id present")
            };
            assert_eq!(class_of(0), CoreClass::Performance);
            assert_eq!(class_of(1), CoreClass::Performance);
            assert_eq!(class_of(2), CoreClass::Efficiency);
            assert_eq!(class_of(3), CoreClass::Efficiency);
            assert_eq!(topo.peak_class(), CoreClass::Performance);
        }

        #[test]
        fn missing_or_malformed_cpuinfo_yields_empty_homogeneous_fallback() {
            for text in ["", "not a cpuinfo\nno colons here\n"] {
                let topo = Topology::from_text(text);
                assert!(topo.classes.is_empty());
                assert!(topo.homogeneous);
                assert_eq!(topo.peak_class(), CoreClass::General);
            }
        }

        #[test]
        fn dispatch_executes_every_job_exactly_once() {
            let topo = Topology::from_text(SYNTHETIC_PE);
            let jobs: Vec<DispatchJob> = (0..8)
                .map(|i| cheap_job(1000.0, false, 500 + i as u64))
                .collect();
            let report = WorkDispatch::dispatch(&jobs, &topo);
            let mut seen = report
                .per_item_usage
                .iter()
                .map(|u| u.item_idx)
                .collect::<Vec<_>>();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), 8);
            assert_eq!(report.per_item_usage.len(), 8);
            // The dispatch-order log also contains each index exactly once.
            let mut order_seen = report.dispatch_order.clone();
            order_seen.sort_unstable();
            order_seen.dedup();
            assert_eq!(order_seen.len(), 8);
        }

        #[test]
        fn performance_items_dispatched_before_efficiency_items() {
            let topo = Topology::from_text(SYNTHETIC_PE);
            // idx 0: efficiency (non-critical, E-feasible)
            // idx 1: performance (critical)
            // idx 2: efficiency
            let jobs = vec![
                cheap_job(100.0, false, 200),
                cheap_job(100.0, true, 200),
                cheap_job(100.0, false, 200),
            ];
            let report = WorkDispatch::dispatch(&jobs, &topo);
            // Confirm the classifier split as intended.
            assert_eq!(report.per_item_usage[0].class, CoreClass::Efficiency);
            assert_eq!(report.per_item_usage[1].class, CoreClass::Performance);
            assert_eq!(report.per_item_usage[2].class, CoreClass::Efficiency);
            // Dispatch order: Performance (idx 1) strictly before both
            // Efficiency items, even with multiple workers claiming under
            // one queue lock.
            let pos = |i: usize| {
                report
                    .dispatch_order
                    .iter()
                    .position(|&x| x == i)
                    .expect("index in dispatch order")
            };
            assert!(pos(1) < pos(0));
            assert!(pos(1) < pos(2));
        }

        #[test]
        fn report_timing_monotonic_and_nonzero() {
            let topo = Topology::from_text(SYNTHETIC_HOMO);
            let jobs = vec![cheap_job(500.0, false, 1500), cheap_job(500.0, false, 1000)];
            let report = WorkDispatch::dispatch(&jobs, &topo);
            assert_eq!(report.per_item_usage.len(), 2);
            for u in &report.per_item_usage {
                assert!(u.us > 0, "per-item timing must be non-zero");
                // An item can never have run longer than the whole dispatch.
                assert!(u.us <= report.total_us, "item exceeds total wall time");
            }
            assert!(report.total_us > 0);
            // Deadlines comfortably met (500 ms >> ~1.5 ms of spin).
            assert!(report.per_item_usage.iter().all(|u| u.deadline_met));
        }

        #[test]
        fn deadline_miss_is_reported() {
            let topo = Topology::from_text(SYNTHETIC_HOMO);
            let jobs = vec![DispatchJob {
                desc: WorkItem {
                    deadline_ms: 0.0005, // 500 ns: the 1 ms spin cannot meet it
                    critical: false,
                    time_p: 0.001,
                    time_e: 0.002,
                },
                us_work: 1000,
            }];
            let report = WorkDispatch::dispatch(&jobs, &topo);
            assert!(!report.per_item_usage[0].deadline_met);
        }

        #[test]
        fn empty_dispatch_is_graceful() {
            let topo = Topology::from_text(SYNTHETIC_PE);
            let report = WorkDispatch::dispatch(&[], &topo);
            assert!(report.per_item_usage.is_empty());
            assert!(report.dispatch_order.is_empty());
            assert!(report.total_us > 0);
        }

        #[test]
        fn cpuinfo_parse_extracts_fields() {
            let entries = parse_cpuinfo(SYNTHETIC_PE);
            assert_eq!(entries.len(), 4);
            assert_eq!(entries[0].logical_id, 0);
            assert_eq!(entries[0].model_name, "Cortex-A78");
            assert_eq!(entries[0].mhz, 2400.0);
            assert_eq!(entries[3].logical_id, 3);
            assert_eq!(entries[3].mhz, 1800.0);
        }

        #[test]
        fn dispatch_contract_constant_matches_spec() {
            assert_eq!(HCAR_DISPATCH_CONTRACT, "GY-HCAR/1");
        }
    }
}
