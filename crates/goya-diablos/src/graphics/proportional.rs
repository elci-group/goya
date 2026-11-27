//! Proportional geometry resolution for Diablos.

use crate::Fault;
use goya_layout::Rect;
use serde::{Deserialize, Serialize};

/// Millipoints (1 point = 1000 mp) - integer geometry unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Millipoints(pub u32);

impl Millipoints {
    pub fn new(mp: u32) -> Self {
        Self(mp)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Display characteristics for proportional geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    /// Physical width in pixels
    pub width_px: u32,
    /// Physical height in pixels
    pub height_px: u32,
    /// Pixel density (DPI)
    pub dpi: u32,
    /// Scale factor (1.0 = 1 pixel = 1 DIP at 250 MP/DIP)
    pub scale_factor: f32,
    /// Whether display supports fractional scaling
    pub fractional_scaling: bool,
    /// Refresh rate (Hz)
    pub refresh_rate: u32,
    /// Colour depth (bits per pixel)
    pub colour_depth: u8,
}

impl Default for DisplayInfo {
    fn default() -> Self {
        Self {
            width_px: 1920,
            height_px: 1080,
            dpi: 96,
            scale_factor: 1.0,
            fractional_scaling: false,
            refresh_rate: 60,
            colour_depth: 24,
        }
    }
}

/// Proportional geometry resolver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProportionalGeometry {
    display: DisplayInfo,
    /// Base DIP size (1 DIP = 250 millipoints at scale 1.0)
    base_dip_size: u32,
}

impl ProportionalGeometry {
    /// Create a new proportional geometry resolver.
    pub fn new(display: DisplayInfo) -> Self {
        Self {
            display,
            base_dip_size: 250, // 1 DIP = 250 millipoints
        }
    }

    /// Convert millipoints to physical pixels.
    pub fn millipoints_to_pixels(&self, mp: Millipoints) -> u32 {
        let dip = mp.as_u32() as f32 / self.base_dip_size as f32;
        let scaled = dip * self.display.scale_factor;
        (scaled.round() as u32).max(1)
    }

    /// Convert physical pixels to millipoints.
    pub fn pixels_to_millipoints(&self, px: u32) -> Millipoints {
        let dip = px as f32 / self.display.scale_factor;
        Millipoints::new((dip * self.base_dip_size as f32).round() as u32)
    }

    /// Convert a rectangle from millipoints to pixels.
    pub fn rect_mp_to_px(&self, rect: Rect) -> RectPx {
        RectPx {
            x: self.millipoints_to_pixels_i64(rect.x),
            y: self.millipoints_to_pixels_i64(rect.y),
            width: self.millipoints_to_pixels_i64(rect.width),
            height: self.millipoints_to_pixels_i64(rect.height),
        }
    }

    /// Convert a rectangle from pixels to millipoints.
    pub fn rect_px_to_mp(&self, rect: RectPx) -> Rect {
        Rect {
            x: self.pixels_to_millipoints_i64(rect.x),
            y: self.pixels_to_millipoints_i64(rect.y),
            width: self.pixels_to_millipoints_i64(rect.width),
            height: self.pixels_to_millipoints_i64(rect.height),
        }
    }

    /// Convert i64 millipoints to physical pixels.
    pub fn millipoints_to_pixels_i64(&self, mp: i64) -> u32 {
        if mp < 0 {
            return 0;
        }
        let dip = mp as f32 / self.base_dip_size as f32;
        let scaled = dip * self.display.scale_factor;
        (scaled.round() as u32).max(1)
    }

    /// Convert physical pixels to i64 millipoints.
    pub fn pixels_to_millipoints_i64(&self, px: u32) -> i64 {
        let dip = px as f32 / self.display.scale_factor;
        (dip * self.base_dip_size as f32).round() as i64
    }

    /// Get the display info.
    pub fn display(&self) -> &DisplayInfo {
        &self.display
    }

    /// Get the scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.display.scale_factor
    }

    /// Get base DIP size in millipoints.
    pub fn base_dip_size(&self) -> u32 {
        self.base_dip_size
    }
}

/// Rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectPx {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Resolve application geometry against target display.
pub fn resolve_geometry(app_rect: Rect, display: &DisplayInfo) -> Result<RectPx, Fault> {
    let geometry = ProportionalGeometry::new(display.clone());
    Ok(geometry.rect_mp_to_px(app_rect))
}

/// Compute optimal scale factor for a display.
pub fn compute_scale_factor(display: &DisplayInfo, prefer_integer: bool) -> f32 {
    // Base scale from DPI (96 DPI = 1.0 scale)
    let base_scale = display.dpi as f32 / 96.0;

    if prefer_integer || !display.fractional_scaling {
        // Round to nearest integer
        base_scale.round().max(1.0)
    } else {
        // Allow fractional, but clamp to reasonable range
        base_scale.clamp(0.5, 4.0)
    }
}

/// Compute display info from system (stub for Phase 0).
pub fn detect_display_info() -> DisplayInfo {
    DisplayInfo::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proportional_geometry_basic() {
        let display = DisplayInfo {
            width_px: 1920,
            height_px: 1080,
            dpi: 96,
            scale_factor: 1.0,
            fractional_scaling: false,
            refresh_rate: 60,
            colour_depth: 24,
        };

        let geometry = ProportionalGeometry::new(display);
        let rect = Rect { x: 25000, y: 25000, width: 25000, height: 25000 }; // 100x100 DIP

        let px = geometry.rect_mp_to_px(rect);
        assert_eq!(px.width, 100);
        assert_eq!(px.height, 100);
    }

    #[test]
    fn proportional_geometry_scaled() {
        let display = DisplayInfo {
            width_px: 3840,
            height_px: 2160,
            dpi: 192,
            scale_factor: 2.0,
            fractional_scaling: false,
            refresh_rate: 60,
            colour_depth: 24,
        };

        let geometry = ProportionalGeometry::new(display);
        let rect = Rect { x: 25000, y: 25000, width: 25000, height: 25000 }; // 100x100 DIP

        let px = geometry.rect_mp_to_px(rect);
        assert_eq!(px.width, 200);
        assert_eq!(px.height, 200);
    }

    #[test]
    fn compute_scale_factor_integer() {
        let display = DisplayInfo { dpi: 144, fractional_scaling: false, ..Default::default() };
        assert_eq!(compute_scale_factor(&display, true), 2.0);
    }

    #[test]
    fn compute_scale_factor_fractional() {
        let display = DisplayInfo { dpi: 144, fractional_scaling: true, ..Default::default() };
        assert_eq!(compute_scale_factor(&display, false), 1.5);
    }

    #[test]
    fn pixels_to_millipoints_roundtrip() {
        let display = DisplayInfo { scale_factor: 1.0, ..Default::default() };
        let geometry = ProportionalGeometry::new(display);

        let mp = Millipoints::new(25000); // 100 DIP
        let px = geometry.millipoints_to_pixels(mp);
        let mp_back = geometry.pixels_to_millipoints(px);

        assert_eq!(mp, mp_back);
    }
}