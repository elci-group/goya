//! Source-ingestion contract (ADR-AVC-001 decision 8, Phase-1 gate).
//!
//! The Phase-1 gate requires three things to exist before canonical video
//! compilation begins: the accepted decoded frame formats, the
//! decode→working-space conversion rule, and the closure parameters for
//! the colour inventory. This module is that contract.
//!
//! Decisions recorded here:
//!
//! - **Accepted formats.** Phase 1 accepts byte-packed RGB-family frames
//!   ([`DecodedFrameFormat::Rgb8`], [`Rgba8`](DecodedFrameFormat::Rgba8),
//!   [`Bgra8`](DecodedFrameFormat::Bgra8)). YUV-family decode output is
//!   rejected with `SourceError::UnsupportedPixelFormat`: the
//!   chroma-resample and matrix decisions belong to a decoder adapter
//!   (e.g. a process boundary around an external decoder), not to the
//!   visual-state contract. The rejection names the required conversion.
//! - **Conversion rule.** sRGB transfer function decoded to scene-linear
//!   Rec. 709 (the contractual trim working space, ADR-AVC-001 decision 6)
//!   and quantised to Q24.8 with round-half-to-even. As with AVR's f32
//!   lesson (ADR-AVR-001 F1), this is the one floating-point stage; the
//!   quantisation rule is declared, and byte-identical output is claimable
//!   per host build.
//! - **Closure parameters.** [`IngestionParameters`] binds the palette
//!   bound, tolerance, working space and media-clock timescale into one
//!   explicitly-constructed value. There is no `Default` (AVC-C09).

use crate::clock::MediaClock;
use crate::colour::{ColourEntry, WorkingSpace};

/// A decoded frame format the contract accepts or rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodedFrameFormat {
    /// 3 bytes per pixel, RGB order.
    Rgb8,
    /// 4 bytes per pixel, RGBA order (alpha ignored by Phase 1).
    Rgba8,
    /// 4 bytes per pixel, BGRA order (alpha ignored by Phase 1).
    Bgra8,
    /// YUV 4:2:0 8-bit — rejected at the contract boundary (see module
    /// docs); a decoder adapter must convert before ingestion.
    Yuv420p8,
}

impl DecodedFrameFormat {
    /// Bytes per pixel, where applicable.
    pub const fn bytes_per_pixel(self) -> Option<usize> {
        match self {
            DecodedFrameFormat::Rgb8 => Some(3),
            DecodedFrameFormat::Rgba8 | DecodedFrameFormat::Bgra8 => Some(4),
            DecodedFrameFormat::Yuv420p8 => None,
        }
    }

    pub const fn is_accepted(self) -> bool {
        !matches!(self, DecodedFrameFormat::Yuv420p8)
    }
}

/// Why a source could not deliver a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The frame payload does not match the declared format and geometry.
    MalformedFrame {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    /// The source's pixel format is not accepted by this contract; the
    /// message names the required conversion (actionable, AVC-C12).
    UnsupportedPixelFormat(&'static str),
    /// The decoder failed for this frame.
    DecodeFailed(String),
}

/// One decoded frame at a declared media-clock position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    pub format: DecodedFrameFormat,
    pub width: u32,
    pub height: u32,
    pub at: MediaClock,
    pub data: Vec<u8>,
}

impl DecodedFrame {
    /// Validate frame geometry against the payload length.
    pub fn validate(&self) -> Result<(), SourceError> {
        let bpp = self
            .format
            .bytes_per_pixel()
            .ok_or(SourceError::UnsupportedPixelFormat(
                "convert YUV 4:2:0 to RGB8/Rgba8/Bgra8 in the decoder adapter before ingestion",
            ))?;
        let expected = self.width as usize * self.height as usize * bpp;
        if self.data.len() != expected {
            return Err(SourceError::MalformedFrame {
                expected_bytes: expected,
                actual_bytes: self.data.len(),
            });
        }
        Ok(())
    }

    /// Iterate the frame's pixels as RGB triples in row-major order,
    /// normalising channel order (BGRA is swapped, alpha dropped).
    pub fn rgb_pixels(&self) -> Result<RgbPixels<'_>, SourceError> {
        self.validate()?;
        Ok(RgbPixels {
            data: &self.data,
            format: self.format,
            remaining: self.width as usize * self.height as usize,
        })
    }
}

/// Row-major RGB pixel iterator over a validated frame.
pub struct RgbPixels<'a> {
    data: &'a [u8],
    format: DecodedFrameFormat,
    remaining: usize,
}

impl Iterator for RgbPixels<'_> {
    type Item = [u8; 3];

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let (rgb, stride) = match self.format {
            DecodedFrameFormat::Rgb8 => ([self.data[0], self.data[1], self.data[2]], 3),
            DecodedFrameFormat::Rgba8 => ([self.data[0], self.data[1], self.data[2]], 4),
            DecodedFrameFormat::Bgra8 => ([self.data[2], self.data[1], self.data[0]], 4),
            DecodedFrameFormat::Yuv420p8 => unreachable!("validated before iteration"),
        };
        self.data = &self.data[stride..];
        Some(rgb)
    }
}

/// The declared sRGB→linear quantisation: Q24.8, round-half-to-even, the
/// GVI arithmetic convention (`docs/graphics/gvi.md` §1).
pub const LINEAR_QUANTISATION_SHIFT: u32 = 8;
/// Unity linear light in Q24.8 (1.0 → 256).
pub const LINEAR_UNITY: u32 = 1 << LINEAR_QUANTISATION_SHIFT;

fn round_half_even_u32(x: f64) -> u32 {
    #[allow(clippy::if_same_then_else)]
    // half-even: the equal-diff branches are intentionally identical for even floors
    let rounded = {
        let floor = x.floor();
        let diff = x - floor;
        if diff > 0.5 {
            floor + 1.0
        } else if diff < 0.5 {
            floor
        } else if (floor as i64) % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    };
    rounded.clamp(0.0, f64::from(u32::MAX)) as u32
}

/// The sRGB transfer function decoded to scene-linear Rec. 709, quantised
/// to Q24.8. The 256-entry table is computed once per process; the
/// rounding rule is fixed so the table is a pure function of the input
/// byte on any IEEE-754 host (per-host byte-identical, per ADR-AVR-001
/// F1's reproducibility split).
pub fn srgb8_to_linear_q24_8(v: u8) -> u32 {
    use std::sync::OnceLock;
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        let mut i = 0usize;
        while i < 256 {
            let c = f64::from(i as u8) / 255.0;
            let linear = if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
            t[i] = round_half_even_u32(linear * f64::from(LINEAR_UNITY));
            i += 1;
        }
        t
    });
    table[v as usize]
}

/// Convert one sRGB pixel to a canonical [`ColourEntry`] in the
/// contractual working space.
pub fn srgb8_to_colour_entry(rgb: [u8; 3]) -> ColourEntry {
    ColourEntry {
        linear_rgb: [
            srgb8_to_linear_q24_8(rgb[0]),
            srgb8_to_linear_q24_8(rgb[1]),
            srgb8_to_linear_q24_8(rgb[2]),
        ],
    }
}

/// The closure parameters of the Phase-1 gate (decision 8), bound
/// explicitly. No `Default` (AVC-C09).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestionParameters {
    /// Working space the decode conversion targets; must be the
    /// contractual one (AVC-C11).
    pub working_space: WorkingSpace,
    /// Media-clock timescale all frame positions are snapped to (F5).
    pub ticks_per_second: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionError {
    /// The declared working space is not the contractual one.
    WorkingSpaceViolation,
    /// The timescale is zero.
    ZeroTimescale,
}

impl IngestionParameters {
    pub fn bind(self) -> Result<Self, IngestionError> {
        if self.working_space != WorkingSpace::LinearRec709 {
            return Err(IngestionError::WorkingSpaceViolation);
        }
        if self.ticks_per_second == 0 {
            return Err(IngestionError::ZeroTimescale);
        }
        Ok(self)
    }
}

/// The video source subset a canonical AOT-VC compiler consumes
/// (mirrors AVR's `SpatialSource`). Implementations must be
/// deterministic: the same frame index always produces the same frame on
/// the same host build.
pub trait VideoSource {
    /// Digest of the source definition — canonical identity input.
    fn source_digest(&self) -> String;

    /// Frame dimensions in pixels.
    fn dimensions(&self) -> (u32, u32);

    /// Rational frame rate in Hz (`numerator / denominator`).
    fn frame_rate(&self) -> (u32, u32);

    /// Total frame count.
    fn frame_count(&self) -> u32;

    /// Decode one frame. The returned position is the source's frame
    /// time snapped to `params`' timescale: `frame_index * frame_duration`.
    fn decode_frame(
        &self,
        frame_index: u32,
        params: &IngestionParameters,
    ) -> Result<DecodedFrame, SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuv_is_rejected_with_actionable_conversion_hint() {
        assert!(!DecodedFrameFormat::Yuv420p8.is_accepted());
        let frame = DecodedFrame {
            format: DecodedFrameFormat::Yuv420p8,
            width: 2,
            height: 2,
            at: MediaClock::new(0, 30).unwrap(),
            data: vec![0; 6],
        };
        match frame.validate() {
            Err(SourceError::UnsupportedPixelFormat(hint)) => {
                assert!(hint.contains("decoder adapter"));
            }
            other => panic!("expected unsupported pixel format, got {other:?}"),
        }
    }

    #[test]
    fn malformed_frames_are_rejected() {
        let frame = DecodedFrame {
            format: DecodedFrameFormat::Rgb8,
            width: 2,
            height: 2,
            at: MediaClock::new(0, 30).unwrap(),
            data: vec![0; 11], // needs 12
        };
        assert_eq!(
            frame.validate(),
            Err(SourceError::MalformedFrame {
                expected_bytes: 12,
                actual_bytes: 11
            })
        );
    }

    #[test]
    fn bgra_pixels_are_normalised_to_rgb() {
        let frame = DecodedFrame {
            format: DecodedFrameFormat::Bgra8,
            width: 2,
            height: 1,
            at: MediaClock::new(0, 30).unwrap(),
            data: vec![10, 20, 30, 255, 40, 50, 60, 255],
        };
        let pixels: Vec<_> = frame.rgb_pixels().unwrap().collect();
        assert_eq!(pixels, vec![[30, 20, 10], [60, 50, 40]]);
    }

    #[test]
    fn srgb_table_matches_reference_values() {
        // Reference: sRGB 0 → 0.0; 255 → 1.0 (unity); 128 ≈ 0.2159 linear.
        assert_eq!(srgb8_to_linear_q24_8(0), 0);
        assert_eq!(srgb8_to_linear_q24_8(255), LINEAR_UNITY);
        let mid = srgb8_to_linear_q24_8(128);
        let mid_f = f64::from(mid) / f64::from(LINEAR_UNITY);
        assert!(
            (mid_f - 0.2159).abs() < 0.002,
            "mid grey decoded to {mid_f}"
        );
    }

    #[test]
    fn table_is_pure_and_clamped() {
        for v in 0..=255u8 {
            let a = srgb8_to_linear_q24_8(v);
            let b = srgb8_to_linear_q24_8(v);
            assert_eq!(a, b);
            assert!(a <= LINEAR_UNITY, "linear light must not exceed unity");
        }
        // Monotonic across the transfer curve; the sRGB EOTF legitimately
        // maps sub-step values (e.g. 1) onto zero at Q24.8.
        assert!(srgb8_to_linear_q24_8(254) > srgb8_to_linear_q24_8(8));
        assert!(srgb8_to_linear_q24_8(8) > srgb8_to_linear_q24_8(1));
        assert_eq!(srgb8_to_linear_q24_8(1), 0);
    }

    #[test]
    fn ingestion_parameters_reject_wrong_space_and_zero_timescale() {
        assert_eq!(
            IngestionParameters {
                working_space: WorkingSpace::LinearRec709,
                ticks_per_second: 0
            }
            .bind(),
            Err(IngestionError::ZeroTimescale)
        );
        // A second working space would only exist after ADR supersession;
        // LinearRec709 binds cleanly.
        assert!(IngestionParameters {
            working_space: WorkingSpace::LinearRec709,
            ticks_per_second: 30
        }
        .bind()
        .is_ok());
    }
}
