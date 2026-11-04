//! GY-BRIDGE/1 — natural state bridging between authoritative and
//! presentation state.
//!
//! Three-layer model:
//!
//! * `Authoritative<T>` — what the application says is true, right now.
//! * `Presentation<T>`  — what is actually shown, animated toward truth.
//! * `Bridge<T>`        — the machinery keeping the two in step.
//!
//! Contracts:
//!
//! * Interruption: retargeting never snaps; current value and velocity are
//!   preserved so mid-flight changes blend naturally.
//! * Exit: `deactivate()` flips authoritative state to Exited immediately.
//!   The presentation may keep animating out, but `interactive()` is false
//!   from that instant (the disappearing panel must not stay interactive).
//! * Reduced motion: when the policy asks for it, advance snaps directly.
//! * Determinism: all time goes through `goya_bridge_physics::advance`.

use goya_bridge_physics::{advance, BridgePolicy, SpringState};
use goya_chroma::Oklch;

/// Contract identifier implemented by this crate.
pub const BRIDGE_CONTRACT: &str = "GY-BRIDGE/1";

/// What the application says is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authoritative<T> {
    /// The thing is live and its authoritative value is `T`.
    Active(T),
    /// The thing has exited. Not interactive, regardless of presentation.
    Exited,
}

/// What is actually shown to the user.
#[derive(Debug, Clone, PartialEq)]
pub enum Presentation<T> {
    /// Shown steady-state at `T`.
    Shown(T),
    /// Animating (in or out) toward a target.
    Animating(T),
}

/// Full bridging state for introspection.
#[derive(Debug, Clone, PartialEq)]
pub struct BridgeState<T> {
    pub current: T,
    pub target: T,
    pub velocity: f64,
    pub revision: u64,
}

/// Values that can be bridged: they support perceptual (or plain) linear
/// interpolation.
pub trait Bridgeable {
    fn lerp(a: &Self, b: &Self, t: f64) -> Self;
}

impl Bridgeable for f64 {
    fn lerp(a: &Self, b: &Self, t: f64) -> Self {
        a + (b - a) * t
    }
}

impl Bridgeable for Oklch {
    /// Perceptual interpolation in OKLCH: lightness and chroma lerp linearly;
    /// hue travels the shortest angular path (350° -> 10° goes through 0°,
    /// not 180°).
    fn lerp(a: &Self, b: &Self, t: f64) -> Self {
        let dh = hue_delta(a.h, b.h);
        Oklch {
            l: a.l + (b.l - a.l) * t,
            c: a.c + (b.c - a.c) * t,
            h: normalize_hue(a.h + dh * t),
        }
    }
}

/// Signed shortest angular distance from `from` to `to` in degrees,
/// in (-180, 180].
fn hue_delta(from: f64, to: f64) -> f64 {
    let mut d = (to - from) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// Normalize a hue into [0, 360).
fn normalize_hue(h: f64) -> f64 {
    let mut h = h % 360.0;
    if h < 0.0 {
        h += 360.0;
    }
    h
}

/// Bridge between an authoritative value and its animated presentation.
///
/// The spring drives the scalar progress parameter `t` in [0, 1] from the
/// frozen `current` value toward the `target`; the presentation value is
/// `Bridgeable::lerp(current, target, t)`.
#[derive(Debug, Clone)]
pub struct Bridge<T: Bridgeable + Clone + PartialEq> {
    current: T,
    target: T,
    progress: SpringState,
    authoritative: Authoritative<T>,
    revision: u64,
    /// Overshoot allowance from the most recent policy (presentation may
    /// extrapolate up to this far beyond the target during settling).
    overshoot_band: f64,
}

impl<T: Bridgeable + Clone + PartialEq> Bridge<T> {
    /// New bridge, settled at `value` and authoritative-active.
    pub fn new(value: T) -> Self {
        Bridge {
            current: value.clone(),
            target: value.clone(),
            progress: SpringState::new(1.0, 0.0),
            authoritative: Authoritative::Active(value),
            revision: 0,
            overshoot_band: 0.0,
        }
    }

    /// Retarget WITHOUT resetting current/velocity: the spring keeps its
    /// momentum and the presentation value freezes into `current`, from which
    /// a fresh [0,1] progress begins. This is the interruption contract.
    pub fn set_target(&mut self, new: T) {
        if self.authoritative == Authoritative::Exited || new == self.target {
            self.revision += 1;
            return;
        }
        let shown = self.current();
        self.current = shown;
        self.target = new.clone();
        self.progress = SpringState::new(0.0, self.progress.v);
        self.authoritative = Authoritative::Active(new);
        self.revision += 1;
    }

    /// Advance the spring by `elapsed` seconds under `policy`. If the policy
    /// asks to respect reduced motion, snap directly instead.
    pub fn advance_dt(&mut self, elapsed: f64, policy: &BridgePolicy) {
        if policy.respect_reduced_motion {
            self.progress = SpringState::new(1.0, 0.0);
            self.current = self.target.clone();
            return;
        }
        if self.progress.x >= 1.0 && self.progress.v.abs() < 1e-3 {
            return; // already settled; nothing to advance
        }
        self.progress = advance(self.progress, 1.0, elapsed, policy);
        self.overshoot_band = policy.max_overshoot;
        if self.progress.x >= 1.0 && self.progress.v.abs() < 1e-3 {
            // Lock the endpoints so equality checks succeed.
            self.progress = SpringState::new(1.0, 0.0);
            self.current = self.target.clone();
        }
    }

    /// The presentation value: what should be shown right now. May
    /// extrapolate up to the policy's overshoot band beyond the target
    /// while the spring settles (bounded, never unbounded).
    pub fn current(&self) -> T {
        if self.progress.x >= 1.0 + self.overshoot_band {
            self.target.clone()
        } else {
            let t = self.progress.x.clamp(0.0, 1.0 + self.overshoot_band);
            T::lerp(&self.current, &self.target, t)
        }
    }

    /// True once the presentation has visually reached the target.
    pub fn settled(&self) -> bool {
        (self.progress.x - 1.0).abs() < 1e-3 && self.progress.v.abs() < 1e-3
    }

    /// True only while the authoritative state is Active. A deactivated
    /// bridge is not interactive even while its presentation animates out.
    pub fn interactive(&self) -> bool {
        self.authoritative != Authoritative::Exited
    }

    /// Deactivate immediately: authoritative state becomes Exited at once.
    /// The presentation is left to animate out via subsequent `advance_dt`
    /// calls, but the bridge can no longer receive interaction.
    pub fn deactivate(&mut self) {
        self.authoritative = Authoritative::Exited;
        self.revision += 1;
    }

    /// Authoritative value, if active.
    pub fn authoritative(&self) -> &Authoritative<T> {
        &self.authoritative
    }

    /// Full bridging state snapshot.
    pub fn state(&self) -> BridgeState<T> {
        BridgeState {
            current: self.current(),
            target: self.target.clone(),
            velocity: self.progress.v,
            revision: self.revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_bridge_physics::{Mode, PHYSICS_CONTRACT};

    fn policy() -> BridgePolicy {
        BridgePolicy::new(Mode::Spring, 0.05)
    }

    #[test]
    fn retarget_continuity_velocity_not_reset() {
        let mut b = Bridge::new(0.0_f64);
        b.set_target(1.0);
        let p = policy();
        b.advance_dt(0.2, &p); // mid-flight, moving with real velocity
        let state = b.state();
        assert!(
            state.velocity > 0.0,
            "expected positive velocity, got {}",
            state.velocity
        );
        let x_before = state.velocity;
        let shown_before = b.current();

        // Interrupt mid-flight.
        b.set_target(2.0);
        let after = b.state();
        // Velocity carried over, not reset.
        assert!(
            (after.velocity - x_before).abs() < 1e-9,
            "velocity was reset: {} -> {}",
            x_before,
            after.velocity
        );
        // Presentation continues from where it was (no snap).
        assert!((b.current() - shown_before).abs() < 1e-9);
    }

    #[test]
    fn oklch_lerp_takes_shortest_hue_path() {
        let a = Oklch {
            l: 0.6,
            c: 0.1,
            h: 350.0,
        };
        let b = Oklch {
            l: 0.6,
            c: 0.1,
            h: 10.0,
        };
        let mid = Oklch::lerp(&a, &b, 0.5);
        // 350 -> 10 must pass through 0 (i.e. ~360), not through 180.
        assert!(
            (mid.h - 0.0).abs() < 1.0,
            "mid hue {} should be near 0°, not near 180°",
            mid.h
        );
        // Reverse direction also shortest-path.
        let mid_rev = Oklch::lerp(&b, &a, 0.5);
        assert!(
            (mid_rev.h - 0.0).abs() < 1.0,
            "reverse mid hue {}",
            mid_rev.h
        );
        // A long way around must take the short cut: 10 -> 350 goes through 0.
        let wrap = Oklch {
            l: 0.5,
            c: 0.05,
            h: 10.0,
        };
        let other = Oklch {
            l: 0.5,
            c: 0.05,
            h: 350.0,
        };
        let quarter = Oklch::lerp(&wrap, &other, 0.25);
        assert!(
            quarter.h > 350.0 || quarter.h < 20.0,
            "quarter hue {}",
            quarter.h
        );
    }

    #[test]
    fn settled_detection() {
        let mut b = Bridge::new(0.0_f64);
        let p = policy();
        assert!(!b.settled() || b.current() == 0.0);
        // Fresh bridge is settled at its own value.
        assert!(b.settled());
        b.set_target(1.0);
        assert!(!b.settled());
        for _ in 0..90 {
            b.advance_dt(1.0 / 60.0, &p);
        }
        assert!(b.settled(), "should settle after 1.5 s");
        assert!((b.current() - 1.0).abs() < 1e-2);
    }

    #[test]
    fn exit_disables_interactivity_instantly() {
        let mut b = Bridge::new(Oklch {
            l: 0.7,
            c: 0.12,
            h: 30.0,
        });
        let p = policy();
        assert!(b.interactive());
        // Start animating toward a new colour so presentation is mid-flight.
        b.set_target(Oklch {
            l: 0.3,
            c: 0.05,
            h: 200.0,
        });
        b.advance_dt(1.0 / 60.0, &p);
        assert!(!b.settled(), "presentation should still be animating");

        b.deactivate();
        // Instant: no advance_dt needed.
        assert!(!b.interactive());
        assert_eq!(b.authoritative(), &Authoritative::Exited);
        // Presentation keeps animating out, still not interactive.
        b.advance_dt(1.0 / 60.0, &p);
        assert!(!b.interactive());
        assert!(!b.settled());
    }

    #[test]
    fn reduced_motion_snaps_directly() {
        let mut b = Bridge::new(0.0_f64);
        b.set_target(1.0);
        let mut p = policy();
        p.respect_reduced_motion = true;
        b.advance_dt(1.0 / 240.0, &p);
        assert!(b.settled());
        assert_eq!(b.current(), 1.0);
    }

    #[test]
    fn deterministic_advance() {
        let mk = || {
            let mut b = Bridge::new(0.0_f64);
            b.set_target(1.0);
            b
        };
        let p = policy();
        let mut a = mk();
        let mut b = mk();
        for _ in 0..30 {
            a.advance_dt(1.0 / 60.0, &p);
            b.advance_dt(1.0 / 60.0, &p);
        }
        assert_eq!(a.state(), b.state());
        assert_eq!(a.current(), b.current());
    }

    #[test]
    fn no_unbounded_overshoot() {
        let mut b = Bridge::new(0.0_f64);
        b.set_target(1.0);
        let p = policy(); // max_overshoot = 0.05
        let mut max_shown = f64::MIN;
        for _ in 0..120 {
            b.advance_dt(1.0 / 60.0, &p);
            let shown = b.current();
            max_shown = max_shown.max(shown);
        }
        assert!(
            max_shown <= 1.05 + 1e-6,
            "presentation overshot the clamp: {}",
            max_shown
        );
        assert!(max_shown > 1.0, "expected some (bounded) overshoot");
    }

    #[test]
    fn hue_helpers_edge_cases() {
        assert!((hue_delta(350.0, 10.0)).abs() - 20.0 < 1e-9);
        assert!((hue_delta(10.0, 350.0) + 20.0).abs() < 1e-9);
        assert!((hue_delta(0.0, 180.0) - 180.0).abs() < 1e-9);
        assert_eq!(normalize_hue(-10.0), 350.0);
        assert_eq!(normalize_hue(370.0), 10.0);
    }

    #[test]
    fn contract_constants_present() {
        assert_eq!(PHYSICS_CONTRACT, "GY-BRIDGE-PHYSICS/1");
        assert_eq!(BRIDGE_CONTRACT, "GY-BRIDGE/1");
    }
}
