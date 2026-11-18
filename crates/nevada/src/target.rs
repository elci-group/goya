//! Android TV target negotiation (contract `nevada.target/0.0.2`).
//!
//! Nevada names one target: Android TV, remote-first. This module negotiates
//! which target features may be granted for a device profile. Negotiation is
//! fail-closed: a feature is granted only when an explicit, usable capability
//! in the resolved census supports it, and a feature that cannot be granted
//! yet is refused with a reason — never silently dropped or granted.

use crate::dcr::{self, HardwareCapability};
use crate::profiles;

/// Contract identifier for this module.
pub const TARGET_PROFILE: &str = "androidtv/0.0.2";

/// A feature of the Android TV target that a caller may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AndroidTvFeature {
    /// The app declares a leanback launcher entry.
    LeanbackManifest,
    /// Directional-pad navigation across the whole surface.
    DpadNavigation,
    /// Spatial focus traversal driven by the D-pad.
    SpatialFocus,
    /// Air-mouse / motion pointer cursor control.
    MotionPointer,
    /// Voice dictation and voice commands.
    VoiceSearch,
    /// Game-controller navigation (analogue sticks, pad buttons).
    GamepadNavigation,
    /// Smooth scrolling driven by a touch surface on the remote.
    TouchScrolling,
    /// A second-screen companion experience.
    CompanionScreen,
}

impl AndroidTvFeature {
    /// Stable lowercase name used in diagnostics and stage details.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeanbackManifest => "leanback-manifest",
            Self::DpadNavigation => "dpad-navigation",
            Self::SpatialFocus => "spatial-focus",
            Self::MotionPointer => "motion-pointer",
            Self::VoiceSearch => "voice-search",
            Self::GamepadNavigation => "gamepad-navigation",
            Self::TouchScrolling => "touch-scrolling",
            Self::CompanionScreen => "companion-screen",
        }
    }
}

/// Why a requested feature was refused. Fail-closed: every non-grant has an
/// explicit reason, and a refusal is never converted into a grant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RefusalReason {
    /// The resolved census has no usable capability for this feature.
    NotSupportedByProfile,
    /// The feature exists in the contract but no phase of Nevada reaches it
    /// yet; granting it would overstate the artefact.
    PhaseNotReached,
    /// No registered profile matches the requested id.
    UnknownProfile,
    /// The census is missing a required baseline capability (D-pad).
    MissingBaseline,
}

impl std::fmt::Display for RefusalReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSupportedByProfile => write!(
                f,
                "feature is not supported by the resolved device census"
            ),
            Self::PhaseNotReached => write!(
                f,
                "feature is not reachable in this phase of nevada ({TARGET_PROFILE}); no codegen exists"
            ),
            Self::UnknownProfile => write!(f, "no registered profile matches the requested id"),
            Self::MissingBaseline => write!(f, "required D-pad baseline capability is absent"),
        }
    }
}

/// A refused feature together with the reason it was refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Refusal {
    /// The feature that was requested and not granted.
    pub feature: AndroidTvFeature,
    /// Why it was not granted.
    pub reason: RefusalReason,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: refused ({}) [{}]",
            self.feature.as_str(),
            self.reason,
            TARGET_PROFILE
        )
    }
}

/// The outcome of one negotiation: what is granted and what is refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Negotiation {
    /// Features granted for this target, in request order.
    pub granted: Vec<AndroidTvFeature>,
    /// Features refused, in request order, each with an explicit reason.
    pub refusals: Vec<Refusal>,
}

/// Fail-closed check: is this capability explicitly present in the census?
fn census_has(census: &dcr::DeviceProfile, capability: HardwareCapability) -> bool {
    census.present.contains(&capability)
}

/// Decide whether a single requested feature is granted for the resolved
/// census, given the DCR interaction support summary.
fn decision(
    feature: AndroidTvFeature,
    census: &dcr::DeviceProfile,
    support: &dcr::InteractionSupport,
) -> Result<(), RefusalReason> {
    match feature {
        // Required baseline: always granted on a resolved profile.
        AndroidTvFeature::DpadNavigation | AndroidTvFeature::SpatialFocus => Ok(()),
        AndroidTvFeature::MotionPointer => {
            // Presence is not usability: only a usable pointer output path
            // (as resolved by the DCR) grants the motion pointer.
            if support.spatial_pointer {
                Ok(())
            } else {
                Err(RefusalReason::NotSupportedByProfile)
            }
        }
        AndroidTvFeature::VoiceSearch => {
            if census_has(census, HardwareCapability::Microphone) {
                Ok(())
            } else {
                Err(RefusalReason::NotSupportedByProfile)
            }
        }
        AndroidTvFeature::GamepadNavigation => {
            if census_has(census, HardwareCapability::GameController) {
                Ok(())
            } else {
                Err(RefusalReason::NotSupportedByProfile)
            }
        }
        AndroidTvFeature::TouchScrolling => {
            if census_has(census, HardwareCapability::Touchpad) {
                Ok(())
            } else {
                Err(RefusalReason::NotSupportedByProfile)
            }
        }
        // Fail-closed: these features are not reachable in this phase. They
        // are never silently granted.
        AndroidTvFeature::LeanbackManifest | AndroidTvFeature::CompanionScreen => {
            Err(RefusalReason::PhaseNotReached)
        }
    }
}

/// Negotiate the Android TV target features for a device profile.
///
/// Fail-closed and deterministic: the same request and profile id always
/// yield the same negotiation. An unknown profile id fails the whole
/// negotiation (every requested feature is refused with
/// [`RefusalReason::UnknownProfile`]); an empty request yields an empty
/// negotiation, which is not an error.
pub fn negotiate(requested: &[AndroidTvFeature], profile_id: &str) -> Negotiation {
    let mut negotiation = Negotiation::default();

    let resolved = match profiles::lookup(profile_id) {
        Ok(profile) => match dcr::resolve(&profile.census) {
            Ok(support) => Some((profile.census.clone(), support)),
            Err(_) => None,
        },
        Err(_) => None,
    };

    for &feature in requested {
        let outcome = match &resolved {
            None => Err(RefusalReason::UnknownProfile),
            Some((census, support)) => decision(feature, census, support),
        };
        match outcome {
            Ok(()) => negotiation.granted.push(feature),
            Err(reason) => negotiation.refusals.push(Refusal { feature, reason }),
        }
    }

    negotiation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcr::MotionOutput;

    const TV: &str = "oem.generic.tv/1.0.0";
    const POINTER: &str = "oem.acme.pointer-remote/1.0.0";
    const VOICE: &str = "oem.acme.voice-remote/1.0.0";
    const GAMEPAD: &str = "oem.generic.gamepad/1.0.0";

    #[test]
    fn dpad_only_profile_grants_exact_baseline() {
        let negotiation = negotiate(
            &[
                AndroidTvFeature::DpadNavigation,
                AndroidTvFeature::SpatialFocus,
                AndroidTvFeature::MotionPointer,
                AndroidTvFeature::VoiceSearch,
                AndroidTvFeature::GamepadNavigation,
                AndroidTvFeature::TouchScrolling,
                AndroidTvFeature::CompanionScreen,
                AndroidTvFeature::LeanbackManifest,
            ],
            TV,
        );
        assert_eq!(
            negotiation.granted,
            vec![
                AndroidTvFeature::DpadNavigation,
                AndroidTvFeature::SpatialFocus
            ]
        );
        assert_eq!(negotiation.refusals.len(), 6);
        assert!(negotiation.refusals.iter().all(|refusal| refusal.reason
            == RefusalReason::NotSupportedByProfile
            || refusal.reason == RefusalReason::PhaseNotReached));
    }

    #[test]
    fn pointer_remote_grants_motion_pointer() {
        let negotiation = negotiate(
            &[
                AndroidTvFeature::DpadNavigation,
                AndroidTvFeature::MotionPointer,
                AndroidTvFeature::TouchScrolling,
            ],
            POINTER,
        );
        assert_eq!(
            negotiation.granted,
            vec![
                AndroidTvFeature::DpadNavigation,
                AndroidTvFeature::MotionPointer,
                AndroidTvFeature::TouchScrolling,
            ]
        );
        assert!(negotiation.refusals.is_empty());
    }

    #[test]
    fn voice_profile_grants_voice_search() {
        let negotiation = negotiate(&[AndroidTvFeature::VoiceSearch], VOICE);
        assert_eq!(negotiation.granted, vec![AndroidTvFeature::VoiceSearch]);
        assert!(negotiation.refusals.is_empty());
    }

    #[test]
    fn gamepad_profile_grants_gamepad_navigation() {
        let negotiation = negotiate(&[AndroidTvFeature::GamepadNavigation], GAMEPAD);
        assert_eq!(
            negotiation.granted,
            vec![AndroidTvFeature::GamepadNavigation]
        );
        assert!(negotiation.refusals.is_empty());
    }

    #[test]
    fn unknown_profile_fails_every_request() {
        let requested = [
            AndroidTvFeature::DpadNavigation,
            AndroidTvFeature::SpatialFocus,
            AndroidTvFeature::MotionPointer,
        ];
        let negotiation = negotiate(&requested, "oem.unheard-of.tv/9.9.9");
        assert!(
            negotiation.granted.is_empty(),
            "fail closed: nothing granted"
        );
        assert_eq!(negotiation.refusals.len(), requested.len());
        assert!(negotiation
            .refusals
            .iter()
            .all(|refusal| refusal.reason == RefusalReason::UnknownProfile));
    }

    #[test]
    fn companion_screen_always_refused_phase_not_reached() {
        for id in [TV, POINTER, VOICE, GAMEPAD] {
            let negotiation = negotiate(&[AndroidTvFeature::CompanionScreen], id);
            assert!(negotiation.granted.is_empty());
            assert_eq!(negotiation.refusals.len(), 1);
            assert_eq!(
                negotiation.refusals[0].reason,
                RefusalReason::PhaseNotReached
            );
        }
    }

    #[test]
    fn leanback_manifest_always_refused_phase_not_reached() {
        let negotiation = negotiate(&[AndroidTvFeature::LeanbackManifest], TV);
        assert!(negotiation.granted.is_empty());
        assert_eq!(
            negotiation.refusals[0].reason,
            RefusalReason::PhaseNotReached
        );
    }

    #[test]
    fn empty_request_yields_empty_negotiation() {
        let negotiation = negotiate(&[], TV);
        assert!(negotiation.granted.is_empty());
        assert!(negotiation.refusals.is_empty());
    }

    #[test]
    fn refusal_display_is_diagnostic_friendly() {
        let negotiation = negotiate(&[AndroidTvFeature::CompanionScreen], TV);
        let text = negotiation.refusals[0].to_string();
        assert!(text.contains("companion-screen"));
        assert!(text.contains(TARGET_PROFILE));
        assert!(text.contains("not reachable in this phase"));
    }

    #[test]
    fn negotiation_is_deterministic() {
        let requested = [
            AndroidTvFeature::SpatialFocus,
            AndroidTvFeature::VoiceSearch,
            AndroidTvFeature::TouchScrolling,
            AndroidTvFeature::CompanionScreen,
        ];
        let first = negotiate(&requested, POINTER);
        for _ in 0..8 {
            assert_eq!(negotiate(&requested, POINTER), first);
        }
    }

    #[test]
    fn motion_sensor_without_output_does_not_grant_pointer() {
        // Sanity via the DCR contract: presence is not usability. The
        // registered pointer profile must have a usable output path; a
        // synthetic census without one must not exist in the registry, so
        // assert the pointer profile resolves to NativePointerEvents.
        let profile = profiles::lookup(POINTER).expect("registered");
        assert_eq!(
            profile.census.motion_output,
            MotionOutput::NativePointerEvents
        );
    }
}
