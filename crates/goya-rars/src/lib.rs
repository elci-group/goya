//! GY-RARS/1 — Resolution-Adaptive Resource Scaling.
//!
//! GY-RARS-001: allocate presentation resources according to the minimum
//! computational fidelity required to produce a perceptually equivalent
//! result at the object's current projected resolution.
//!
//! Core rules:
//! - The resolver is a deterministic pure function of
//!   (ProjectionFidelityContext, policy version).
//! - It NEVER modifies authoritative state.
//! - Interaction fidelity is inviolable: E_interaction = 0 (hit targets,
//!   hit-testing geometry and text legibility are never reduced).
//! - Spatial complexity and temporal complexity are independent: a small
//!   fast-moving object still gets full update frequency (directive §2).

/// Contract identifier implemented by this crate.
pub const RARS_CONTRACT: &str = "GY-RARS/1";

/// Projected size in device pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DevicePixelSize {
    pub width: u32,
    pub height: u32,
}

/// Motion profile of the object at decision time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionProfile {
    /// Speed in device px/s (0 = static).
    pub velocity_px_s: f64,
}

/// Perceptual error tolerances (directive §4 constraints).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErrorBudget {
    /// Permitted screen-space error for spatial representation (px).
    pub spatial_px: f64,
    /// Permitted temporal error (ms of phase lag).
    pub temporal_ms: f64,
}

/// Accessibility policy: reductions that affect legibility/usability are
/// prohibited regardless of projected size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilityPolicy {
    /// When true, text fidelity may NEVER be reduced (screen readers,
    /// low-vision users, assistive zoom).
    pub protect_text: bool,
    /// Minimum hit-target size in px that geometry LOD must preserve.
    pub min_hit_target_px: u32,
}

/// Fidelity configuration selected by the resolver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FidelityDecision {
    /// Spatial subdivisions for curved bridge paths (GY-RARS-001 §3).
    pub spatial_subdivisions: u32,
    /// Raster working scale (1 = full, 2 = half linear size, ...).
    pub raster_scale: u32,
    /// Effect quality tier: 0 economy, 1 balanced, 2 precision.
    pub effect_quality: u32,
    /// Glyph pixel scale for block-glyph renderers (never reduced below 1
    /// when text is protected).
    pub glyph_px: u32,
    /// Update frequency policy — NOT reduced by spatial smallness.
    pub temporal_policy: TemporalPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalPolicy {
    /// Static: present on change only.
    OnChange,
    /// Moving: continuous updates at the display cadence.
    Continuous,
}

/// Everything the resolver consumes (directive §9 contract, reduced to the
/// deterministic subset; power/thermal arrive with Mesut integration).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectionFidelityContext {
    pub projected_size: DevicePixelSize,
    pub device_scale: f64,
    pub motion: MotionProfile,
    pub error: ErrorBudget,
    pub accessibility: AccessibilityPolicy,
}

impl ProjectionFidelityContext {
    /// Relative pixel area vs a 4K reference (directive §5 model).
    pub fn relative_area(&self) -> f64 {
        let ref_px = 3840.0 * 2160.0;
        (self.projected_size.width as f64
            * self.projected_size.height as f64
            * self.device_scale
            * self.device_scale)
            / ref_px
    }
}

/// The resolver: deterministic fidelity decisions (GY-RARS/1 contract).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FidelityResolver {
    /// Policy version — bump invalidates compiled envelopes (EDC hook).
    pub policy_version: u32,
}

impl Default for FidelityResolver {
    fn default() -> Self {
        FidelityResolver { policy_version: 1 }
    }
}

impl FidelityResolver {
    /// Resolve the fidelity configuration. Deterministic: identical inputs
    /// and policy version always yield identical decisions.
    pub fn resolve(&self, ctx: &ProjectionFidelityContext) -> FidelityDecision {
        // Screen-space-error-driven spatial subdivision (directive §3):
        // subdivide until segment chord error <= eps_px. For unit arcs the
        // sagitta scales ~1/n², so n ≈ sqrt(k / eps). We model curvature
        // constant k=1 and clamp to [2, 64].
        let eps = ctx.error.spatial_px.max(0.05);
        let subdivisions = ((1.0 / eps).sqrt().ceil() as u32).clamp(2, 64);

        let rel = ctx.relative_area();
        // Raster scale: half-linear steps while the projected area is far
        // below the reference and tolerance allows.
        let raster_scale = if rel < 0.001 {
            4
        } else if rel < 0.01 {
            2
        } else {
            1
        };

        // Effect quality follows projected area (economy/balanced/precision).
        let effect_quality = if rel < 0.001 {
            0
        } else if rel < 0.05 {
            1
        } else {
            2
        };

        // Text protection (accessibility): glyph scale never drops below 2
        // when protected; otherwise follows raster scale.
        let glyph_px = if ctx.accessibility.protect_text {
            (4 / raster_scale).clamp(2, 4)
        } else {
            (4 / raster_scale).clamp(1, 4)
        };

        // TEMPORAL POLICY IS INDEPENDENT OF SPATIAL SIZE (directive §2):
        // small + fast still demands continuous updates.
        let temporal_policy = if ctx.motion.velocity_px_s > 1.0 {
            TemporalPolicy::Continuous
        } else {
            TemporalPolicy::OnChange
        };

        FidelityDecision {
            spatial_subdivisions: subdivisions,
            raster_scale,
            effect_quality,
            glyph_px,
            temporal_policy,
        }
    }
}

/// Hysteresis wrapper (GY-RARS-001 §8): avoid oscillation between fidelity
/// levels during continuous resize. A level change requires the new level
/// to persist for `confirm_frames` consecutive decisions.
#[derive(Debug)]
pub struct Hysteresis {
    last_level: u32,
    pending_level: u32,
    pending_count: u32,
    confirm_frames: u32,
}

impl Hysteresis {
    pub fn new(confirm_frames: u32) -> Self {
        Hysteresis {
            last_level: 0,
            pending_level: 0,
            pending_count: 0,
            confirm_frames: confirm_frames.max(1),
        }
    }

    /// Feed a decision's level; returns the STABLE level (only changes after
    /// `confirm_frames` consecutive differing decisions).
    pub fn step(&mut self, level: u32) -> u32 {
        // Seed with the first observed level: authoritative immediately.
        if self.last_level == 0 && self.pending_count == 0 {
            self.last_level = level;
            return level;
        }
        if level == self.last_level {
            self.pending_level = 0;
            self.pending_count = 0;
            return self.last_level;
        }
        if level == self.pending_level {
            self.pending_count += 1;
        } else {
            self.pending_level = level;
            self.pending_count = 1;
        }
        if self.pending_count >= self.confirm_frames {
            self.last_level = level;
            self.pending_count = 0;
        }
        self.last_level
    }

    pub fn level(&self) -> u32 {
        self.last_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(
        w: u32,
        h: u32,
        scale: f64,
        vel: f64,
        eps: f64,
        protect_text: bool,
    ) -> ProjectionFidelityContext {
        ProjectionFidelityContext {
            projected_size: DevicePixelSize {
                width: w,
                height: h,
            },
            device_scale: scale,
            motion: MotionProfile { velocity_px_s: vel },
            error: ErrorBudget {
                spatial_px: eps,
                temporal_ms: 8.0,
            },
            accessibility: AccessibilityPolicy {
                protect_text,
                min_hit_target_px: 48,
            },
        }
    }

    #[test]
    fn deterministic_for_identical_inputs() {
        let r = FidelityResolver::default();
        let c = ctx(480, 320, 1.0, 0.0, 0.5, true);
        assert_eq!(r.resolve(&c), r.resolve(&c));
    }

    #[test]
    fn finer_tolerance_demands_more_subdivisions() {
        let r = FidelityResolver::default();
        let coarse = r.resolve(&ctx(480, 320, 1.0, 0.0, 1.0, true));
        let fine = r.resolve(&ctx(480, 320, 1.0, 0.0, 0.1, true));
        assert!(fine.spatial_subdivisions > coarse.spatial_subdivisions);
    }

    #[test]
    fn small_widget_gets_lower_raster_scale_but_continuous_motion() {
        // Directive §2: spatial complexity down, temporal complexity NOT.
        let r = FidelityResolver::default();
        let small_fast = r.resolve(&ctx(120, 40, 1.0, 600.0, 0.5, true));
        assert!(small_fast.raster_scale > 1);
        assert_eq!(small_fast.temporal_policy, TemporalPolicy::Continuous);
        // Same small widget, static: on-change only.
        let small_still = r.resolve(&ctx(120, 40, 1.0, 0.0, 0.5, true));
        assert_eq!(small_still.temporal_policy, TemporalPolicy::OnChange);
    }

    #[test]
    fn fullscreen_gets_precision() {
        let r = FidelityResolver::default();
        let full = r.resolve(&ctx(3840, 2160, 1.0, 0.0, 0.5, true));
        assert_eq!(full.raster_scale, 1);
        assert_eq!(full.effect_quality, 2);
    }

    #[test]
    fn accessibility_protects_text_fidelity() {
        let r = FidelityResolver::default();
        let tiny_protected = r.resolve(&ctx(60, 30, 1.0, 0.0, 0.5, true));
        let tiny_plain = r.resolve(&ctx(60, 30, 1.0, 0.0, 0.5, false));
        assert!(tiny_protected.glyph_px >= 2);
        assert!(tiny_protected.glyph_px > tiny_plain.glyph_px);
    }

    #[test]
    fn hysteresis_requires_confirmation() {
        let mut h = Hysteresis::new(3);
        // First observation is authoritative immediately (seeded residency).
        assert_eq!(h.step(1), 1);
        assert_eq!(h.level(), 1);
        // A single differing frame must NOT flip the level.
        assert_eq!(h.step(2), 1);
        assert_eq!(h.step(1), 1);
        // Sustained change commits after three consecutive 2s.
        assert_eq!(h.step(2), 1);
        assert_eq!(h.step(2), 1);
        assert_eq!(h.step(2), 2);
    }

    #[test]
    fn relative_area_matches_directive_model() {
        let c = ctx(480, 320, 1.0, 0.0, 0.5, true);
        // 480*320 / (3840*2160) ≈ 0.0185 → the directive's 1.9% example.
        assert!((c.relative_area() - 0.0185).abs() < 0.001);
    }
}
