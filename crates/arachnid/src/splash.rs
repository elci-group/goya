//! The Arachnid launch splash. Contract `arachnid.splash/0.0.1`.
//!
//! **Enforced.** Every app `arachnid build` produces opens on the Arachnid
//! logo (`assets/arachnid-logo.png`, embedded in the compiler). The scene
//! cannot remove, replace or shorten it; there is no option for that. The
//! compilation manifest records the splash and its SHA-256, and
//! `scripts/check-arachnid.sh` fails a build without it.
//!
//! **Geometry** follows the parent-relative rule. The splash covers the
//! ontology limit, and the logo box is [`LOGO_HEIGHT_P`] of the limit's
//! height, keeps the logo's own aspect ratio and is centred. Outside the
//! logo's ring (about 0.89 of the image's half-width) the image is
//! feathered with a radial mask, so the square image edge never shows.
//!
//! **Behaviour.** The splash holds for [`HOLD_MS`], then fades out over
//! [`FADE_MS`] (opacity only, on the compositor). Under
//! `prefers-reduced-motion: reduce` it disappears without fading. While it
//! shows, the runtime takes the 5-way keys and pointer clicks and drops
//! them, so nobody activates a button they cannot see yet. Every other key,
//! including Back, still reaches the platform. When it ends, the runtime
//! removes it from the page and dispatches `arachnid:splash-end`.

use crate::ontology::{ONTOLOGY_HEIGHT_PT, ONTOLOGY_WIDTH_PT};

pub const CONTRACT: &str = "arachnid.splash/0.0.1";
/// The logo, embedded at compile time so no build can lack it.
pub const LOGO: &[u8] = include_bytes!("../../../assets/arachnid-logo.png");
pub const FILE: &str = "arachnid-splash.png";
pub const HOLD_MS: u32 = 1200;
pub const FADE_MS: u32 = 400;
/// Logo box height as a fraction of the ontology limit's height.
pub const LOGO_HEIGHT_P: f64 = 0.6;
/// The logo's own vignette, sampled from its corners and centre.
const EDGE: &str = "#dfd0b2";
const CENTRE: &str = "#ece0c7";

/// Logo box (x, y, w, h) in millipoints, centred in the ontology limit.
pub fn logo_box() -> (i64, i64, i64, i64) {
    let (lw, lh) = crate::archive::png_dimensions(LOGO).expect("the embedded logo is a PNG");
    let h = (ONTOLOGY_HEIGHT_PT as f64 * 1000.0 * LOGO_HEIGHT_P).round() as i64;
    let w = (h as f64 * lw as f64 / lh as f64).round() as i64;
    let x = (ONTOLOGY_WIDTH_PT as i64 * 1000 - w) / 2;
    let y = (ONTOLOGY_HEIGHT_PT as i64 * 1000 - h) / 2;
    (x, y, w, h)
}

pub fn markup() -> String {
    let px = crate::emit::px;
    let (x, y, w, h) = logo_box();
    format!(
        "<div id=\"arachnid-splash\" aria-label=\"Built with Arachnid\"><img src=\"{FILE}\" alt=\"Arachnid — webOS app compiler\" style=\"left:{};top:{};width:{};height:{}\"></div>\n",
        px(x),
        px(y),
        px(w),
        px(h)
    )
}

pub fn css() -> String {
    format!(
        "#arachnid-splash{{position:absolute;left:0;top:0;width:{ONTOLOGY_WIDTH_PT}px;height:{ONTOLOGY_HEIGHT_PT}px;z-index:2147483647;background:radial-gradient(ellipse at center,{CENTRE} 0%,{EDGE} 70%);will-change:opacity;animation:arachnid-splash-out {FADE_MS}ms ease-in {HOLD_MS}ms forwards}}\n\
         #arachnid-splash img{{position:absolute;object-fit:contain;-webkit-mask-image:radial-gradient(circle closest-side,#000 92%,transparent 100%)}}\n\
         @keyframes arachnid-splash-out{{to{{opacity:0;visibility:hidden}}}}\n\
         @media (prefers-reduced-motion:reduce){{#arachnid-splash{{animation-duration:1ms;animation-delay:{HOLD_MS}ms}}}}\n"
    )
}

/// Runtime declarations: whether the splash is up, and when it ends.
pub fn runtime_js() -> String {
    format!(
        "  var splashOn = !!document.getElementById(\"arachnid-splash\");\n  function endSplash() {{\n    var s = document.getElementById(\"arachnid-splash\");\n    if (s && s.parentNode) s.parentNode.removeChild(s);\n    splashOn = false;\n    var ev = document.createEvent(\"CustomEvent\");\n    ev.initCustomEvent(\"arachnid:splash-end\", false, false, null);\n    document.dispatchEvent(ev);\n  }}\n  setTimeout(endSplash, {});\n",
        HOLD_MS + FADE_MS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_box_is_parent_relative_and_centred() {
        let (x, y, w, h) = logo_box();
        assert_eq!(h, 648_000, "0.6 of 1080 pt");
        assert_eq!(w, (648_000.0_f64 * 723.0 / 729.0).round() as i64, "the logo keeps its aspect");
        // Centred to the millipoint (an odd width rounds down by one).
        assert!((x * 2 + w - 1_920_000).abs() <= 1);
        assert_eq!(y * 2 + h, 1_080_000);
    }
}
