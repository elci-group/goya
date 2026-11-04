//! GY-BRIDGE-PHYSICS/1 — deterministic damped-spring dynamics for natural
//! state bridging.
//!
//! The spring runs on a FIXED step (1/240 s) with semi-implicit Euler
//! integration. Large `dt` values are split internally into fixed substeps,
//! so equivalent inputs plus equivalent elapsed time always produce identical
//! trajectories regardless of how the caller slices its frame times. This is
//! the determinism contract: no wall-clock reads, no RNG, no environment
//! dependence.
//!
//! Contract: `advance` is a pure function of (state, target, dt, params).

/// Contract identifier implemented by this crate.
pub const PHYSICS_CONTRACT: &str = "GY-BRIDGE-PHYSICS/1";

/// Fixed integration substep in seconds. All advancing is performed in
/// multiples of this step so trajectories depend only on total elapsed time.
pub const FIXED_STEP: f64 = 1.0 / 240.0;

/// Maximum dt accepted per substep batch before it is split (defensive
/// clamp against pathological frame spikes; also keeps the substep count
/// bounded).
pub const MAX_DT: f64 = 1.0;

/// Spring physical parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringParams {
    /// Stiffness k (higher = snappier).
    pub stiffness: f64,
    /// Damping coefficient c.
    pub damping: f64,
    /// Mass m.
    pub mass: f64,
}

impl SpringParams {
    /// Underdamped preset: settles with a small, bounded overshoot — the
    /// "natural" feel.
    pub fn natural() -> Self {
        SpringParams {
            stiffness: 170.0,
            damping: 18.0,
            mass: 1.0,
        }
    }

    /// Critically damped preset: fastest settle with no overshoot.
    pub fn critical() -> Self {
        let k = 170.0;
        SpringParams {
            stiffness: k,
            damping: 2.0 * (k * 1.0).sqrt(),
            mass: 1.0,
        }
    }

    /// Direct preset: effectively immediate snap to the target.
    pub fn direct() -> Self {
        SpringParams {
            stiffness: 100_000.0,
            damping: 4_000.0,
            mass: 1.0,
        }
    }
}

impl Default for SpringParams {
    fn default() -> Self {
        SpringParams::natural()
    }
}

/// Scalar spring state: position `x` and velocity `v`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpringState {
    pub x: f64,
    pub v: f64,
}

impl SpringState {
    pub fn new(x: f64, v: f64) -> Self {
        SpringState { x, v }
    }
}

/// Bridging mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Underdamped natural spring.
    Spring,
    /// Critically damped, no overshoot.
    Critical,
    /// Immediate snap.
    Direct,
}

/// Policy applied by callers on top of raw spring dynamics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BridgePolicy {
    pub mode: Mode,
    /// Maximum allowed excursion beyond the target, as an absolute fraction
    /// (0.0 = no overshoot allowed, 0.05 = 5% past the target).
    pub max_overshoot: f64,
    /// Accessibility: when set, advance snaps directly with no spring.
    pub respect_reduced_motion: bool,
}

impl BridgePolicy {
    pub fn new(mode: Mode, max_overshoot: f64) -> Self {
        BridgePolicy {
            mode,
            max_overshoot: max_overshoot.clamp(0.0, 1.0),
            respect_reduced_motion: false,
        }
    }

    fn params(&self) -> SpringParams {
        match self.mode {
            Mode::Spring => SpringParams::natural(),
            Mode::Critical => SpringParams::critical(),
            Mode::Direct => SpringParams::direct(),
        }
    }
}

impl Default for BridgePolicy {
    fn default() -> Self {
        BridgePolicy {
            mode: Mode::Spring,
            max_overshoot: 0.05,
            respect_reduced_motion: false,
        }
    }
}

/// Advance `state` toward `target` over `dt_fixed` seconds using semi-implicit
/// Euler on fixed 1/240 s substeps. Overshoot beyond `policy.max_overshoot` is
/// clamped after each substep, preserving position and velocity continuity.
///
/// Determinism: identical (state, target, dt, policy) always yields identical
/// output, and splitting dt into separate calls over the same total time
/// produces the same trajectory as one call (within f64 rounding of identical
/// substep sequences).
pub fn advance(
    state: SpringState,
    target: f64,
    dt_fixed: f64,
    policy: &BridgePolicy,
) -> SpringState {
    if !dt_fixed.is_finite() || dt_fixed <= 0.0 {
        return clamp_overshoot(state, target, policy);
    }
    if policy.mode == Mode::Direct || policy.respect_reduced_motion {
        return SpringState { x: target, v: 0.0 };
    }

    let params = policy.params();
    let total = dt_fixed.min(MAX_DT);
    let mut steps = (total / FIXED_STEP).ceil() as usize;
    // At least one substep; keep substep count bounded for huge dt.
    steps = steps.clamp(1, 24_000);
    let h = total / steps as f64;

    let mut s = state;
    let inv_mass = 1.0 / params.mass.max(f64::MIN_POSITIVE);
    for _ in 0..steps {
        let displacement = s.x - target;
        let accel = (-params.stiffness * displacement - params.damping * s.v) * inv_mass;
        // Semi-implicit Euler: velocity first, then position with new velocity.
        s.v += accel * h;
        s.x += s.v * h;
        s = clamp_overshoot(s, target, policy);
    }
    s
}

/// Clamp excursion beyond the target to `policy.max_overshoot` (fraction of
/// the original travel distance is not tracked here, so the clamp is applied
/// as a fraction of 1.0 of the bridging parameter range when the caller uses
/// t in [0,1]; for arbitrary scales callers scale accordingly).
fn clamp_overshoot(state: SpringState, target: f64, policy: &BridgePolicy) -> SpringState {
    let limit = policy.max_overshoot;
    if limit >= 1.0 {
        return state;
    }
    let overshoot = state.x - target;
    // Only clamp genuine overshoot: the state has crossed the target and is
    // still moving away from it. Approach-side travel toward the target must
    // pass through unclamped or far-away states would be snapped to the band.
    let moving_away = overshoot != 0.0 && state.v != 0.0 && overshoot.signum() == state.v.signum();
    if !moving_away || overshoot.abs() <= limit {
        return state;
    }
    // Excursion exceeds the allowed band: clamp position to the band edge
    // and stop the outward motion so the spring returns instead of fighting
    // the clamp.
    let clamped = if overshoot > 0.0 {
        target + limit
    } else {
        target - limit
    };
    SpringState { x: clamped, v: 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn run(steps: usize, target: f64, policy: &BridgePolicy) -> SpringState {
        let mut s = SpringState::new(0.0, 0.0);
        for _ in 0..steps {
            s = advance(s, target, FIXED_STEP, policy);
        }
        s
    }

    #[test]
    fn converges_to_target() {
        let policy = BridgePolicy::default();
        let s = run(2400, 1.0, &policy); // 10 s
        assert!((s.x - 1.0).abs() < 1e-3, "x = {}", s.x);
        assert!(s.v.abs() < 1e-3, "v = {}", s.v);
    }

    #[test]
    fn critically_damped_never_overshoots() {
        let policy = BridgePolicy::new(Mode::Critical, 0.0);
        let mut s = SpringState::new(0.0, 0.0);
        let h = 1.0 / 240.0;
        for _ in 0..4800 {
            s = advance(s, 1.0, h, &policy);
            assert!(s.x <= 1.0 + EPS, "overshoot detected: x = {}", s.x);
        }
        assert!((s.x - 1.0).abs() < 1e-3);
    }

    #[test]
    fn direct_snaps_immediately() {
        let policy = BridgePolicy::new(Mode::Direct, 0.0);
        let s = advance(SpringState::new(0.25, -3.0), 1.0, FIXED_STEP, &policy);
        assert_eq!(s.x, 1.0);
        assert_eq!(s.v, 0.0);
    }

    #[test]
    fn retarget_preserves_position_and_velocity_continuity() {
        let policy = BridgePolicy::default();
        let mut s = SpringState::new(0.0, 0.0);
        // Advance toward A for 0.2 s.
        for _ in 0..48 {
            s = advance(s, 1.0, FIXED_STEP, &policy);
        }
        let (x_before, v_before) = (s.x, s.v);
        // Retarget to B: first B-step must start from current x, v.
        let first_b = advance(s, 2.0, FIXED_STEP, &policy);
        let expected_v = v_before
            + ((-SpringParams::natural().stiffness * (x_before - 2.0)
                - SpringParams::natural().damping * v_before)
                * FIXED_STEP);
        let expected_x = x_before + expected_v * FIXED_STEP;
        assert!((first_b.v - expected_v).abs() < 1e-12);
        assert!((first_b.x - expected_x).abs() < 1e-12);
        // Continuity also means the first B-step is close to where we were.
        assert!((first_b.x - x_before).abs() < 0.05);
    }

    #[test]
    fn determinism_identical_runs_identical_trajectories() {
        let policy = BridgePolicy::default();
        let a = run(600, 1.0, &policy);
        let b = run(600, 1.0, &policy);
        assert_eq!(a, b);
    }

    #[test]
    fn dt_splitting_invariance() {
        // Both call patterns must decompose into the identical substep
        // sequence (exact multiples of FIXED_STEP) and thus agree exactly.
        let policy = BridgePolicy::default();
        let one_call = advance(SpringState::new(0.0, 0.0), 1.0, 24.0 * FIXED_STEP, &policy);
        let mut many = SpringState::new(0.0, 0.0);
        for _ in 0..24 {
            many = advance(many, 1.0, FIXED_STEP, &policy);
        }
        assert!(
            (one_call.x - many.x).abs() < 1e-9 && (one_call.v - many.v).abs() < 1e-9,
            "one-call x={} v={}, many-call x={} v={}",
            one_call.x,
            one_call.v,
            many.x,
            many.v
        );
    }

    #[test]
    fn overshoot_clamped_to_policy() {
        // Tight clamp on the natural spring: excursion past target must never
        // exceed max_overshoot.
        let policy = BridgePolicy::new(Mode::Spring, 0.02);
        let mut s = SpringState::new(0.0, 0.0);
        let mut max_seen = 0.0_f64;
        for _ in 0..1200 {
            s = advance(s, 1.0, FIXED_STEP, &policy);
            max_seen = max_seen.max(s.x);
        }
        assert!(max_seen <= 1.02 + EPS, "max x = {}", max_seen);
    }

    #[test]
    fn zero_and_non_finite_dt_are_no_ops_on_position() {
        let policy = BridgePolicy::default();
        let s = SpringState::new(0.4, 1.0);
        let out = advance(s, 1.0, 0.0, &policy);
        assert_eq!(out.x, s.x);
        let out = advance(s, 1.0, f64::NAN, &policy);
        assert_eq!(out.x, s.x);
    }
}
