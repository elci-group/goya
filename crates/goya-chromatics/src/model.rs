//! Input model: slots, capability, candidates, policy and triggers.

use std::collections::BTreeSet;
use std::fmt;

/// Index of an abstract assignable input in a [`Capability`].
pub type SlotId = u8;

/// The most slots a capability may describe. A larger table is malformed.
pub const MAX_SLOTS: usize = 16;

/// Whether a physical input can carry a Chromatic binding (§3.3, §13.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotStatus {
    /// The input reaches the application and may be bound.
    Available,
    /// The platform or another function owns the input. Never bound.
    Reserved,
    /// The hardware does not have, or does not deliver, the input.
    Unsupported,
}

/// One abstract input slot. `name` is target-neutral (for example `red`);
/// the backend maps it to a key code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub name: String,
    pub status: SlotStatus,
}

/// What the actual remote and platform offer (Capability Registry, §4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capability {
    /// Slots in presentation order. At most [`MAX_SLOTS`].
    pub slots: Vec<Slot>,
    /// Native-equivalence keys a dedicated, operational input already
    /// covers (for example `volume`, `mute`, `playback.pause`).
    pub native: BTreeSet<String>,
}

impl Capability {
    /// Every slot available, named in order.
    pub fn available(names: &[&str]) -> Self {
        Capability {
            slots: names
                .iter()
                .map(|n| Slot {
                    name: (*n).to_string(),
                    status: SlotStatus::Available,
                })
                .collect(),
            native: BTreeSet::new(),
        }
    }

    /// The four webOS colour keys, all available.
    pub fn webos_colour_keys() -> Self {
        Self::available(&["red", "green", "yellow", "blue"])
    }

    /// Slots that may carry a binding, in order.
    pub fn available_slots(&self) -> Vec<SlotId> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.status == SlotStatus::Available)
            .map(|(i, _)| i as SlotId)
            .collect()
    }

    /// Negotiated capacity: the number of available slots.
    pub fn capacity(&self) -> usize {
        self.available_slots().len()
    }

    /// Slot index by name.
    pub fn slot(&self, name: &str) -> Option<SlotId> {
        self.slots
            .iter()
            .position(|s| s.name == name)
            .map(|i| i as SlotId)
    }
}

/// Consequence class of an action (§20).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Risk {
    #[default]
    Navigational,
    Reversible,
    Operational,
    Sensitive,
    Destructive,
    Privileged,
}

impl Risk {
    /// Whether a single press may only enter a confirmation workflow.
    pub fn needs_confirmation(self) -> bool {
        matches!(self, Risk::Sensitive | Risk::Destructive | Risk::Privileged)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Navigational => "navigational",
            Risk::Reversible => "reversible",
            Risk::Operational => "operational",
            Risk::Sensitive => "sensitive",
            Risk::Destructive => "destructive",
            Risk::Privileged => "privileged",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "navigational" => Risk::Navigational,
            "reversible" => Risk::Reversible,
            "operational" => Risk::Operational,
            "sensitive" => Risk::Sensitive,
            "destructive" => Risk::Destructive,
            "privileged" => Risk::Privileged,
            _ => return None,
        })
    }
}

/// One entity- or action-oriented candidate (§7).
///
/// `id` is the stable semantic identity. It never encodes a position. All
/// numeric fields are integers so allocation is exact and portable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    /// Associated entity, if entity-oriented.
    pub entity: Option<String>,
    /// Registered semantic intent to execute.
    pub intent: String,
    /// Text label. Required: colour alone never identifies a binding (§21).
    pub label: String,
    /// Semantic symbol or marker shape for non-colour identification
    /// (§21: symbols, distinct marker shapes). Optional; the presentation
    /// layer maps it to its own iconography.
    pub symbol: Option<String>,
    /// Accessible description of what the action is for (§21). Optional;
    /// supplementary to the required label.
    pub description: Option<String>,
    pub available: bool,
    pub authorised: bool,
    pub risk: Risk,
    /// The intent enters a confirmation workflow rather than acting.
    pub confirmation: bool,
    /// Native-equivalence key, if a dedicated input may already cover it.
    pub native: Option<String>,
    /// Application ranking, 0 best.
    pub rank: Option<u32>,
    /// Profile favourite order, 0 best.
    pub favourite: Option<u32>,
    /// Visible position, 0 first.
    pub position: Option<u32>,
    /// Last access tick.
    pub last_used: Option<u64>,
    /// Historic use count.
    pub uses: u32,
    /// Explicit manual assignment, by slot name.
    pub pinned: Option<String>,
    /// Conventional inputs needed to reach and activate it (≥ 1).
    pub cost: u32,
    /// Estimated likelihood of need, per mille (0..=1000).
    pub probability: u32,
    /// Utility terms in milli-inputs (§9).
    pub contextual: i64,
    pub urgency: i64,
    pub preference: i64,
    /// Tick at which the candidate stops being valid.
    pub expires: Option<u64>,
}

impl Candidate {
    /// A navigational, available, authorised candidate with no ranking.
    pub fn new(id: &str, intent: &str, label: &str) -> Self {
        Candidate {
            id: id.to_string(),
            entity: None,
            intent: intent.to_string(),
            label: label.to_string(),
            symbol: None,
            description: None,
            available: true,
            authorised: true,
            risk: Risk::Navigational,
            confirmation: false,
            native: None,
            rank: None,
            favourite: None,
            position: None,
            last_used: None,
            uses: 0,
            pinned: None,
            cost: 1,
            probability: 0,
            contextual: 0,
            urgency: 0,
            preference: 0,
            expires: None,
        }
    }

    /// Expected utility in milli-inputs, without the reassignment term (§9):
    /// `P × savings + contextual + urgency + preference`. Savings are the
    /// conventional cost minus the one Chromatic press.
    pub fn utility(&self) -> i64 {
        let p = i64::from(self.probability.min(1000));
        let savings = i64::from(self.cost.saturating_sub(1));
        p.saturating_mul(savings)
            .saturating_add(self.contextual)
            .saturating_add(self.urgency)
            .saturating_add(self.preference)
    }
}

/// Deterministic allocation strategies (§8).
///
/// Operational, Severity, Predictive and Hybrid are the `Utility` strategy
/// with the corresponding terms supplied (contextual, urgency, probability,
/// or several). Predictive inputs are the caller's local statistics; this
/// crate never infers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Only pinned candidates are bound.
    Manual,
    /// Favourite order.
    Preferential,
    /// Application ranking.
    Ranked,
    /// Most recently used first.
    Temporal,
    /// Most used first (uses > 0).
    Frequent,
    /// Visible position.
    Positional,
    /// Highest expected utility first.
    Utility,
}

impl Strategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Strategy::Manual => "manual",
            Strategy::Preferential => "preferential",
            Strategy::Ranked => "ranked",
            Strategy::Temporal => "temporal",
            Strategy::Frequent => "frequent",
            Strategy::Positional => "positional",
            Strategy::Utility => "utility",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "manual" => Strategy::Manual,
            "preferential" => Strategy::Preferential,
            "ranked" => Strategy::Ranked,
            "temporal" => Strategy::Temporal,
            "frequent" => Strategy::Frequent,
            "positional" => Strategy::Positional,
            "utility" | "operational" | "severity" | "predictive" | "hybrid" => Strategy::Utility,
            _ => return None,
        })
    }

    /// Ordering key, smaller first, or `None` if this strategy does not
    /// supply the candidate.
    pub(crate) fn key(self, c: &Candidate) -> Option<i128> {
        match self {
            Strategy::Manual => None,
            Strategy::Preferential => c.favourite.map(i128::from),
            Strategy::Ranked => c.rank.map(i128::from),
            Strategy::Temporal => c.last_used.map(|t| -i128::from(t)),
            Strategy::Frequent => (c.uses > 0).then(|| -i128::from(c.uses)),
            Strategy::Positional => c.position.map(i128::from),
            Strategy::Utility => Some(-i128::from(c.utility())),
        }
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Chromatic Inertia settings (§10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stability {
    /// Score a challenger must exceed an incumbent by before displacing it
    /// on a stabilised trigger. This is the §9 ReassignmentCost. Units are
    /// milli-inputs for `Utility`, places in the order otherwise.
    pub min_improvement: i64,
    /// Ticks an incumbent holds its slot before it may be displaced.
    pub min_residency: u64,
    /// Ticks a better challenger may be held back before it wins anyway.
    pub max_deferral: u64,
}

impl Default for Stability {
    fn default() -> Self {
        Stability {
            min_improvement: 1,
            min_residency: 0,
            max_deferral: u64::MAX,
        }
    }
}

/// Allocation policy for one scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub primary: Strategy,
    /// Fills slots the primary leaves empty, never with a duplicate.
    pub fallback: Option<Strategy>,
    /// Keep a still-selected candidate on its previous slot (§10.4).
    pub preserve_slots: bool,
    /// Assign slots strictly in selection order (overrides preservation).
    pub ordered_slots: bool,
    /// Drop candidates a native input already covers (§3.3). Turning this
    /// off is the explicit accessibility accommodation.
    pub exclude_native: bool,
    /// At most one binding per entity.
    pub exclude_duplicates: bool,
    pub stability: Stability,
}

impl Policy {
    pub fn new(primary: Strategy) -> Self {
        Policy {
            primary,
            fallback: None,
            preserve_slots: true,
            ordered_slots: false,
            exclude_native: true,
            exclude_duplicates: true,
            stability: Stability::default(),
        }
    }

    pub fn with_fallback(mut self, fallback: Strategy) -> Self {
        self.fallback = Some(fallback);
        self
    }
}

/// Why allocation runs (§10.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Scene, view, profile or modal transition: immediate resolution.
    Context,
    /// Explicit sorting or preference change: immediate resolution.
    Policy,
    /// Hardware capability change: re-evaluate.
    Capability,
    /// Background ranking update or new candidate: stabilised.
    Background,
    /// A candidate became unavailable, expired or lost permission:
    /// invalid bindings go, the others stay where they are.
    Invalidation,
}

impl Trigger {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "context" => Trigger::Context,
            "policy" => Trigger::Policy,
            "capability" => Trigger::Capability,
            "background" | "arrival" => Trigger::Background,
            "invalidation" => Trigger::Invalidation,
            _ => return None,
        })
    }
}
