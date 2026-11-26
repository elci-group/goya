//! Stable diagnostic codes, rendered as `GY-AR-<n>: <message>`.
//!
//! Contract `arachnid.diag/0.0.1`. Codes are reserved in the 6xx range so
//! that they never collide with Nevada (`GY-NV-5xx`). A code's number never
//! changes once assigned; retired codes are not reused.

use goya_lang::SourceRange;
use std::fmt;

pub const CONTRACT: &str = "arachnid.diag/0.0.1";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum Code {
    /// The source failed strict GSSH/3 admission.
    SourceRejected = 601,
    /// The target was not a recognised model, platform, version or agent.
    TargetUnresolved = 602,
    /// `@interaction` is not lowered to webOS yet.
    InteractionNotLowered = 610,
    /// `@state` and input bindings are not lowered to webOS yet.
    StateNotLowered = 611,
    /// `@input` is not lowered to webOS yet.
    InputNotLowered = 612,
    /// `@intent` (voice) has no webOS lowering.
    IntentNotLowered = 613,
    /// Gesture policies have no webOS lowering yet.
    GesturesNotLowered = 614,
    /// Scrollable cells are not lowered to webOS yet.
    ScrollNotLowered = 615,
    /// An integer literal lies outside the runtime's exact range (±2^53−1).
    IntegerOutOfRange = 616,
    /// A node falls outside the 1920×1080 ontology limit.
    OutsideOntologyLimit = 620,
    /// A drawn node leaves the ontology limit while it drifts.
    MotionOutsideLimit = 621,
    /// Text over a breathing or moving background falls below WCAG AA
    /// contrast, or cannot be verified.
    TextContrast = 622,
    /// An actionable element has no area at the ontology limit.
    ActionableWithoutArea = 630,
    /// Two actionable elements overlap, so a pointer click is ambiguous.
    ActionableOverlap = 631,
    /// More actionable elements than the bounded focus resolver accepts.
    TooManyActionable = 632,
    /// A drift carries some buttons but not all of them.
    IndependentMotion = 633,
    /// An actionable element cannot be reached by directional navigation.
    FocusUnreachable = 640,
    /// Directional navigation can enter a region it cannot leave.
    FocusTrap = 641,
    /// Presence conditions produce more configurations than the bound.
    TooManyConfigurations = 642,
    /// Two variants stacked on one focus slot can be present together.
    SlotConflict = 643,
    /// A `@graphic` failed GVI compilation or verification.
    GraphicNotCompiled = 645,
    /// A durable store, or an accessibility mapping, is not one Arachnid
    /// implements.
    PlatformStoreUnsupported = 651,
    /// Host state or a D2 request has no platform service to serve it, or
    /// a `@link` is not one Arachnid lowers.
    HostContract = 650,
    /// A launch request names an application its `@apps` does not declare,
    /// or an `@apps` declaration is not one Arachnid lowers.
    LaunchContract = 652,
    /// Generated code needs a web feature the target engine lacks.
    FeatureUnsupported = 660,
    /// Package metadata violates LG's `appinfo.json` rules.
    InvalidAppInfo = 670,
    /// The supplied icon is not an 80×80 PNG.
    InvalidIcon = 671,
    /// Output could not be written.
    OutputFailed = 680,
}

impl Code {
    pub const fn number(self) -> u16 {
        self as u16
    }
}

/// One diagnostic. `range` is a byte range into the source when one applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: Code,
    pub message: String,
    pub range: Option<SourceRange>,
}

impl Diagnostic {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            range: None,
        }
    }

    pub fn at(mut self, range: Option<SourceRange>) -> Self {
        self.range = range;
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GY-AR-{}: {}", self.code.number(), self.message)?;
        if let Some(range) = self.range {
            write!(f, " (bytes {}..{})", range.start, range.end)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_stable_codes() {
        let d =
            Diagnostic::new(Code::FocusTrap, "trapped").at(Some(SourceRange { start: 3, end: 9 }));
        assert_eq!(d.to_string(), "GY-AR-641: trapped (bytes 3..9)");
        assert_eq!(Code::SourceRejected.number(), 601);
        assert_eq!(Code::OutputFailed.number(), 680);
    }
}
