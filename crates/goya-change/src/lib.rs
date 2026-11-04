//! GY-CHANGE/1 — Localised Change Potential Efficiency (LCPE).
//!
//! GY-LCPE-001: every object declares what must persist, what may change,
//! at what resolution change is meaningful, and which events are authorised
//! to cause that change. The runtime resolves events into the smallest
//! correct set of state, computation and presentation updates.
//!
//! Core invariant: visual equality at the current resolution does NOT imply
//! semantic equality — authoritative state is always preserved even when
//! the display cannot reveal a difference. Quantisation may coarsen
//! *presentation*, never truth.

use std::collections::BTreeMap;
use std::fmt;

/// Contract identifier implemented by this crate.
pub const CHANGE_CONTRACT: &str = "GY-CHANGE/1";

/// Which presentation properties an object declares mutable/persistent.
/// (Millipoint scale matches GY-LAYOUT/1: 1 pt = 1000 mp, 4 px per pt.)
/// Bitflags as a plain u32 set — deterministic and serialisable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PropertyMask(pub u32);

impl PropertyMask {
    pub const GEOMETRY: u32 = 1 << 0;
    pub const COLOUR: u32 = 1 << 1;
    pub const TEXT: u32 = 1 << 2;
    pub const SONIC: u32 = 1 << 3;
    pub const OPACITY: u32 = 1 << 4;
    pub const ALL: u32 = Self::GEOMETRY | Self::COLOUR | Self::TEXT | Self::SONIC | Self::OPACITY;

    pub fn contains(self, other: u32) -> bool {
        self.0 & other == other
    }

    pub fn set(&mut self, bits: u32) {
        self.0 |= bits;
    }
}

/// Which event classes may cause which transitions (event causality).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRule {
    /// e.g. "pointer.press", "key.char", "data.update".
    pub event: String,
    /// Authorised resulting state, e.g. "hover", "pressed", "value".
    pub causes: Vec<String>,
}

/// A pixel rectangle in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl PixelRect {
    pub fn union(&self, other: &PixelRect) -> PixelRect {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.width).max(other.x + other.width);
        let y1 = (self.y + self.height).max(other.y + other.height);
        PixelRect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        }
    }

    pub fn area(&self) -> u64 {
        (self.width.max(0) as u64) * (self.height.max(0) as u64)
    }

    /// Quantise the rect to device-pixel tiles of `tile` px: the envelope
    /// grows conservatively (never shrinks below the true dirty area).
    pub fn quantise(&self, tile: u32) -> PixelRect {
        if tile == 0 {
            return *self;
        }
        let t = tile as i32;
        let x0 = (self.x / t) * t;
        let y0 = (self.y / t) * t;
        let x1 = ((self.x + self.width + t - 1) / t) * t;
        let y1 = ((self.y + self.height + t - 1) / t) * t;
        PixelRect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        }
    }
}

/// The compiled Local Change Potential Envelope (directive §5).
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeEnvelope {
    pub object: String,
    /// State/geometry revision the envelope was compiled against.
    pub revision: u64,
    /// Properties that must remain stable (semantic invariants).
    pub persistent: PropertyMask,
    /// Properties permitted to change.
    pub mutable: PropertyMask,
    /// Event → authorised transitions.
    pub event_rules: Vec<EventRule>,
    /// Static potential bounds in device pixels (conservative).
    pub spatial_bounds: PixelRect,
    /// Quantisation tile for resolution-aware invalidation (px).
    pub resolution_tile: u32,
}

impl ChangeEnvelope {
    /// Whether `event` is authorised to cause `transition` on this object.
    pub fn authorises(&self, event: &str, transition: &str) -> bool {
        self.event_rules
            .iter()
            .any(|r| r.event == event && r.causes.iter().any(|c| c == transition))
    }

    /// Conservative device-pixel dirty region for a property change within
    /// the current bounds: quantised to the resolution tile.
    pub fn dirty_region(&self, properties: u32) -> Option<PixelRect> {
        if !self.mutable.contains(properties) {
            return None; // change potential excludes these properties
        }
        Some(self.spatial_bounds.quantise(self.resolution_tile))
    }
}

/// An observed input mapped through causality to an authorised transition.
#[derive(Debug, Clone, PartialEq)]
pub struct CausalTransition {
    pub object: String,
    pub transition: String,
    pub dirty: Option<PixelRect>,
}

/// Causal resolver: input event → authorised transitions → dirty regions.
/// A rendered pixel changing NEVER causes a state transition through this
/// path (no feedback loops); pixel-observation inputs must be declared as
/// explicit external events with their own rules.
#[derive(Debug, Default)]
pub struct CausalResolver {
    envelopes: BTreeMap<String, ChangeEnvelope>,
}

impl CausalResolver {
    pub fn register(&mut self, envelope: ChangeEnvelope) {
        self.envelopes.insert(envelope.object.clone(), envelope);
    }

    pub fn envelope(&self, object: &str) -> Option<&ChangeEnvelope> {
        self.envelopes.get(object)
    }

    /// Resolve an input event into the minimal authorised transition set.
    /// Returns an empty vec when no envelope authorises the pair — events
    /// are rejected by default (capability-style causality).
    pub fn resolve(&self, object: &str, event: &str, transition: &str) -> Vec<CausalTransition> {
        match self.envelopes.get(object) {
            Some(env) if env.authorises(event, transition) => {
                let properties = transition_properties(transition);
                vec![CausalTransition {
                    object: object.to_string(),
                    transition: transition.to_string(),
                    dirty: env.dirty_region(properties),
                }]
            }
            _ => Vec::new(),
        }
    }
}

/// Map a transition name to the property bits it may affect. Deterministic
/// vocabulary: colour-ish transitions touch COLOUR, geometry-ish GEOMETRY,
/// text-ish TEXT. Unknown transitions affect nothing (conservative reject).
fn transition_properties(transition: &str) -> u32 {
    match transition {
        "hover" | "pressed" | "accent" | "flash" => PropertyMask::COLOUR | PropertyMask::SONIC,
        "move" | "resize" | "expand" | "collapse" => PropertyMask::GEOMETRY | PropertyMask::SONIC,
        "relabel" | "value" => PropertyMask::TEXT | PropertyMask::COLOUR,
        _ => 0,
    }
}

/// Local vs full-frame cost decision (directive §8). Deterministic: choose
/// local invalidation only when its modelled cost is below a full redraw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostModel {
    /// Fixed cost of resolving dependencies per local invalidation.
    pub resolve: f64,
    /// Fixed scheduling/submission overhead for a commit.
    pub commit: f64,
    /// Cost per 1000 dirty pixels of raster work.
    pub per_kpixel: f64,
    /// Cost of a full-frame redraw.
    pub full: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        // Units are relative; defaults express the common case where small
        // local updates win and large dirty regions do not.
        CostModel {
            resolve: 0.4,
            commit: 0.6,
            per_kpixel: 0.5,
            full: 30.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalidation {
    /// Commit only the quantised dirty region.
    Local(PixelRect),
    /// Full-frame redraw is cheaper or required for correctness.
    FullFrame,
}

impl fmt::Display for Invalidation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Invalidation::Local(r) => write!(f, "local {}x{} @ {},{}", r.width, r.height, r.x, r.y),
            Invalidation::FullFrame => write!(f, "full-frame"),
        }
    }
}

/// Decide local vs full invalidation for a set of dirty regions.
/// Union first (one commit), then compare against the full-frame cost.
pub fn decide(dirty: &[PixelRect], model: &CostModel) -> Invalidation {
    if dirty.is_empty() {
        return Invalidation::FullFrame; // nothing to do; caller skips entirely
    }
    let mut total = dirty[0];
    for r in &dirty[1..] {
        total = total.union(r);
    }
    let local_cost =
        model.resolve + model.commit + (total.area() as f64 / 1000.0) * model.per_kpixel;
    if local_cost < model.full {
        Invalidation::Local(total)
    } else {
        Invalidation::FullFrame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button_envelope() -> ChangeEnvelope {
        ChangeEnvelope {
            object: "btn_go".into(),
            revision: 7,
            persistent: PropertyMask(PropertyMask::TEXT | PropertyMask::GEOMETRY),
            mutable: PropertyMask(
                PropertyMask::COLOUR | PropertyMask::SONIC | PropertyMask::OPACITY,
            ),
            event_rules: vec![EventRule {
                event: "pointer.press".into(),
                causes: vec!["hover".into(), "pressed".into()],
            }],
            spatial_bounds: PixelRect {
                x: 300,
                y: 300,
                width: 200,
                height: 60,
            },
            resolution_tile: 16,
        }
    }

    #[test]
    fn causality_authorises_only_declared_pairs() {
        let mut r = CausalResolver::default();
        r.register(button_envelope());
        assert_eq!(r.resolve("btn_go", "pointer.press", "hover").len(), 1);
        // Unauthorised: no rule for this event, no envelope, no object.
        assert!(r.resolve("btn_go", "pointer.press", "move").is_empty());
        assert!(r.resolve("btn_go", "key.char", "hover").is_empty());
        assert!(r.resolve("other", "pointer.press", "hover").is_empty());
    }

    #[test]
    fn persistence_mask_blocks_forbidden_dirty_regions() {
        let env = button_envelope();
        // GEOMETRY is persistent (not mutable): a move transition yields no
        // dirty region — presentation may not express it via this envelope.
        assert_eq!(env.dirty_region(PropertyMask::GEOMETRY), None);
        assert!(env.dirty_region(PropertyMask::COLOUR).is_some());
    }

    #[test]
    fn quantisation_grows_conservatively() {
        let r = PixelRect {
            x: 305,
            y: 309,
            width: 197,
            height: 55,
        };
        let q = r.quantise(16);
        assert!(q.x <= r.x && q.y <= r.y);
        assert!(q.x + q.width >= r.x + r.width);
        assert!(q.y + q.height >= r.y + r.height);
        // Tile-aligned.
        assert_eq!(q.x % 16, 0);
        assert_eq!(q.y % 16, 0);
    }

    #[test]
    fn cost_model_prefers_local_for_small_dirty() {
        let small = [PixelRect {
            x: 300,
            y: 300,
            width: 200,
            height: 60,
        }];
        assert!(matches!(
            decide(&small, &CostModel::default()),
            Invalidation::Local(_)
        ));
    }

    #[test]
    fn cost_model_prefers_full_frame_for_large_dirty() {
        let big = [PixelRect {
            x: 0,
            y: 0,
            width: 4000,
            height: 2000,
        }];
        assert!(matches!(
            decide(&big, &CostModel::default()),
            Invalidation::FullFrame
        ));
    }

    #[test]
    fn cost_model_union_before_decision() {
        let a = PixelRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let b = PixelRect {
            x: 150,
            y: 0,
            width: 100,
            height: 100,
        };
        // Two small regions: local (union with gap — conservative envelope).
        assert!(matches!(
            decide(&[a, b], &CostModel::default()),
            Invalidation::Local(_)
        ));
        // Many spread regions exceed the full-frame cost.
        let spread: Vec<PixelRect> = (0..10)
            .map(|i| PixelRect {
                x: i * 400,
                y: 0,
                width: 400,
                height: 1500,
            })
            .collect();
        assert!(matches!(
            decide(&spread, &CostModel::default()),
            Invalidation::FullFrame
        ));
    }

    #[test]
    fn resolution_changes_meaningful_change_surface() {
        let mut env = button_envelope();
        // At a coarse tile the envelope covers nearly the full bounds; at a
        // fine tile it stays tight — same change, different meaningful res.
        env.resolution_tile = 64;
        let coarse = env.dirty_region(PropertyMask::COLOUR).unwrap();
        env.resolution_tile = 4;
        let fine = env.dirty_region(PropertyMask::COLOUR).unwrap();
        assert!(coarse.area() >= fine.area());
    }

    #[test]
    fn semantic_state_preserved_beyond_display_resolution() {
        // The directive's critical qualification: quantisation must never
        // touch authoritative state. The resolver returns transitions with
        // dirty regions; state values live in goya-state, untouched here.
        let mut r = CausalResolver::default();
        r.register(button_envelope());
        let t = &r.resolve("btn_go", "pointer.press", "pressed")[0];
        assert_eq!(t.transition, "pressed");
        // The envelope revision is the truth anchor, independent of pixels.
        assert_eq!(r.envelope("btn_go").unwrap().revision, 7);
    }
}
