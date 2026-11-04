//! OKLCH colour space and sRGB conversion with deterministic gamut clamping.

use std::fmt;

/// Colour in OKLCH: lightness 0..1, chroma >= 0, hue in degrees 0..360.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// sRGB 8-bit colour. Display target of the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb8 { r, g, b }
    }

    /// WCAG relative luminance of the composited colour.
    pub fn relative_luminance(self) -> f64 {
        fn channel(v: u8) -> f64 {
            let s = v as f64 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }
}

impl fmt::Display for Rgb8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// Convert sRGB to OKLCH (Björn Ottosson's OKLab, sRGB D65).
pub fn srgb_to_oklch(rgb: Rgb8) -> Oklch {
    let lin = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (lin(rgb.r), lin(rgb.g), lin(rgb.b));

    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;

    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();

    let lab_l = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
    let lab_a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
    let lab_b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;

    let chroma = lab_a.hypot(lab_b);
    let mut hue = lab_b.atan2(lab_a).to_degrees();
    if hue < 0.0 {
        hue += 360.0;
    }
    Oklch {
        l: lab_l,
        c: chroma,
        h: hue,
    }
}

/// Convert OKLCH to sRGB, deterministically clamping out-of-gamut colours by
/// reducing chroma (binary search on chroma) until the result is in gamut.
pub fn oklch_to_srgb(color: Oklch) -> Rgb8 {
    let attempt = |chroma: f64| try_oklch_to_srgb_unclamped(Oklch { c: chroma, ..color });
    if let Some(rgb) = attempt(color.c) {
        return rgb;
    }
    // Invariant-preserving gamut mapping: keep lightness and hue, shrink chroma.
    let (mut lo, mut hi) = (0.0_f64, color.c);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if attempt(mid).is_some() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    attempt(lo).unwrap_or(Rgb8::new(0, 0, 0))
}

/// Returns `None` when the converted colour falls outside sRGB [0,1]³.
fn try_oklch_to_srgb_unclamped(color: Oklch) -> Option<Rgb8> {
    let rad = color.h.to_radians();
    let lab_a = color.c * rad.cos();
    let lab_b = color.c * rad.sin();

    let l_ = color.l + 0.3963377774 * lab_a + 0.2158037573 * lab_b;
    let m_ = color.l - 0.1055613458 * lab_a - 0.0638541728 * lab_b;
    let s_ = color.l - 0.0894841775 * lab_a - 1.2914855480 * lab_b;

    let l = l_.powi(3);
    let m = m_.powi(3);
    let s = s_.powi(3);

    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

    let delin = |v: f64| -> Option<f64> {
        if v <= 0.0031308 {
            Some(12.92 * v)
        } else {
            Some(1.055 * v.powf(1.0 / 2.4) - 0.055)
        }
    };
    let byte = |v: f64| -> Option<u8> {
        let srgb = delin(v)?;
        (0.0..=1.0)
            .contains(&srgb)
            .then(|| (srgb * 255.0).round() as u8)
    };
    Some(Rgb8::new(byte(r)?, byte(g)?, byte(b)?))
}
