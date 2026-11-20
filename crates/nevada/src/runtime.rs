//! Re-resolve an OEM input policy from observed device capabilities.
//!
//! The platform adapter takes a fresh snapshot before enabling interaction
//! and after input-device, service, or IME availability changes. Callers
//! stop dispatch when rechecking fails. Observations describe an input
//! path the adapter can actually receive or invoke, so an OS feature flag,
//! a peripheral transport, or a raw sensor inventory alone cannot qualify.

use std::fmt;

use crate::caprichio::{translation_capabilities, HardwareFactory, InputPath};
use crate::dcr::{self, DeviceProfile};
use crate::translate::{self, RemoteInteraction, TouchInteraction, TranslationOutcome};

pub const CONTRACT: &str = "nevada.runtime-capabilities/0.1.0";

/// Hardware census and usable input paths observed by the platform adapter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeObservations {
    pub census: DeviceProfile,
    pub usable_paths: Vec<InputPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInteraction {
    pub interaction: TouchInteraction,
    pub compiled_primary: RemoteInteraction,
    pub runtime_primary: RemoteInteraction,
    pub runtime_fallbacks: Vec<RemoteInteraction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeReport {
    pub active_paths: Vec<InputPath>,
    pub unavailable_paths: Vec<InputPath>,
    pub interactions: Vec<RuntimeInteraction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    Census(dcr::ResolveError),
    MissingRequiredPath(InputPath),
    UncompiledInteraction(TouchInteraction),
    NoUsableRoute(TouchInteraction),
    VoiceWithoutNonVoiceFallback,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Census(error) => error.fmt(f),
            Self::MissingRequiredPath(path) => {
                write!(f, "required input path '{}' is unusable", path.as_str())
            }
            Self::UncompiledInteraction(interaction) => {
                write!(f, "OEM policy cannot compile {interaction:?}")
            }
            Self::NoUsableRoute(interaction) => write!(
                f,
                "no compiled input route for {interaction:?} remains usable"
            ),
            Self::VoiceWithoutNonVoiceFallback => {
                f.write_str("voice text entry has no usable non-voice fallback")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Recheck the factory's declared paths before dispatch. The result follows
/// canonical path order and preserves the caller's interaction order. Extra
/// observed capabilities cannot enable paths outside the compiled policy.
/// A missing required route fails the whole recheck. Optional routes may
/// fall back only to alternatives already authorized at compile time.
pub fn recheck(
    factory: &HardwareFactory,
    observations: &RuntimeObservations,
    interactions: &[TouchInteraction],
) -> Result<RuntimeReport, RuntimeError> {
    let support = dcr::resolve(&observations.census).map_err(RuntimeError::Census)?;
    let (active_paths, unavailable_paths): (Vec<_>, Vec<_>) =
        factory.allowed_paths().iter().copied().partition(|path| {
            path.supported_by(&support) && observations.usable_paths.contains(path)
        });
    if let Some(path) = factory
        .required_paths()
        .iter()
        .find(|path| !active_paths.contains(path))
    {
        return Err(RuntimeError::MissingRequiredPath(*path));
    }

    let compiled = factory.translation_capabilities();
    let runtime = translation_capabilities(&active_paths);
    let mut rows = Vec::with_capacity(interactions.len());
    for &interaction in interactions {
        let TranslationOutcome::Translated {
            primary: compiled_primary,
            fallbacks: compiled_fallbacks,
        } = translate::translate(interaction, &compiled)
        else {
            return Err(RuntimeError::UncompiledInteraction(interaction));
        };

        let TranslationOutcome::Translated {
            primary: runtime_primary,
            fallbacks: runtime_fallbacks,
        } = translate::translate(interaction, &runtime)
        else {
            return Err(RuntimeError::NoUsableRoute(interaction));
        };

        let authorized = |route: &RemoteInteraction| {
            *route == compiled_primary || compiled_fallbacks.contains(route)
        };
        if !authorized(&runtime_primary) || !runtime_fallbacks.iter().all(authorized) {
            return Err(RuntimeError::NoUsableRoute(interaction));
        }
        if runtime_primary == RemoteInteraction::Voice && runtime_fallbacks.is_empty() {
            return Err(RuntimeError::VoiceWithoutNonVoiceFallback);
        }

        rows.push(RuntimeInteraction {
            interaction,
            compiled_primary,
            runtime_primary,
            runtime_fallbacks,
        });
    }

    Ok(RuntimeReport {
        active_paths,
        unavailable_paths,
        interactions: rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caprichio::{compile_registered, Declaration, Ir};
    use crate::dcr::{HardwareCapability, MotionOutput};

    fn observations(
        present: Vec<HardwareCapability>,
        motion_output: MotionOutput,
        usable_paths: Vec<InputPath>,
    ) -> RuntimeObservations {
        RuntimeObservations {
            census: DeviceProfile {
                present,
                motion_output,
            },
            usable_paths,
        }
    }

    #[test]
    fn dpad_requires_both_census_and_usable_path() {
        let factory = compile_registered("oem.generic.tv/1.0.0").unwrap();
        let mut device = observations(vec![], MotionOutput::None, vec![InputPath::Dpad]);
        assert_eq!(
            recheck(&factory, &device, &[TouchInteraction::TouchTap]),
            Err(RuntimeError::Census(
                dcr::ResolveError::MissingRequiredBaseline(HardwareCapability::Dpad)
            ))
        );
        device.census.present.push(HardwareCapability::Dpad);
        device.usable_paths.clear();
        assert_eq!(
            recheck(&factory, &device, &[TouchInteraction::TouchTap]),
            Err(RuntimeError::MissingRequiredPath(InputPath::Dpad))
        );
    }

    #[test]
    fn pointer_output_loss_uses_the_compiled_directional_fallback() {
        let factory = compile_registered("oem.acme.pointer-remote/1.0.0").unwrap();
        let mut device = observations(
            vec![HardwareCapability::Dpad, HardwareCapability::Touchpad],
            MotionOutput::NativePointerEvents,
            vec![InputPath::SpatialPointer, InputPath::Dpad],
        );
        let intents = &[TouchInteraction::MouseScroll, TouchInteraction::TouchTap];
        let first = recheck(&factory, &device, intents).unwrap();
        assert_eq!(
            first.interactions[0].runtime_primary,
            RemoteInteraction::PointerScroll
        );
        assert_eq!(
            first.interactions[0].runtime_fallbacks,
            [RemoteInteraction::DirectionalScroll]
        );
        device.usable_paths.reverse();
        assert_eq!(recheck(&factory, &device, intents).unwrap(), first);
        device.census.motion_output = MotionOutput::None;
        let fallback = recheck(&factory, &device, intents).unwrap();
        assert_eq!(
            fallback.interactions[0].compiled_primary,
            RemoteInteraction::PointerScroll
        );
        assert_eq!(
            fallback.interactions[0].runtime_primary,
            RemoteInteraction::DirectionalScroll
        );
        assert_eq!(
            fallback.interactions[1].runtime_primary,
            RemoteInteraction::FocusAndSelect
        );
        assert!(fallback
            .unavailable_paths
            .contains(&InputPath::SpatialPointer));
    }

    #[test]
    fn microphone_and_transport_presence_do_not_enable_text_routes() {
        let voice = compile_registered("oem.acme.voice-remote/1.0.0").unwrap();
        let mut device = observations(
            vec![
                HardwareCapability::Dpad,
                HardwareCapability::Microphone,
                HardwareCapability::Bluetooth,
            ],
            MotionOutput::None,
            vec![InputPath::Dpad, InputPath::PhysicalKeyboard],
        );
        let intents = &[TouchInteraction::TextInput];
        assert_eq!(
            recheck(&voice, &device, intents),
            Err(RuntimeError::NoUsableRoute(TouchInteraction::TextInput))
        );
        device.usable_paths.push(InputPath::Voice);
        assert_eq!(
            recheck(&voice, &device, intents),
            Err(RuntimeError::VoiceWithoutNonVoiceFallback)
        );
        device.usable_paths.push(InputPath::TvKeyboard);
        let report = recheck(&voice, &device, intents).unwrap();
        assert_eq!(
            report.interactions[0].runtime_primary,
            RemoteInteraction::Voice
        );
        assert_eq!(
            report.interactions[0].runtime_fallbacks,
            [RemoteInteraction::TvKeyboard]
        );
        assert!(!report.active_paths.contains(&InputPath::PhysicalKeyboard));
        device.usable_paths.retain(|path| *path != InputPath::Voice);
        assert_eq!(
            recheck(&voice, &device, intents).unwrap().interactions[0].runtime_primary,
            RemoteInteraction::TvKeyboard
        );
    }

    #[test]
    fn a_missing_oem_required_route_refuses_instead_of_falling_back() {
        let factory = Declaration {
            profile_id: "oem.acme.pointer-remote/1.0.0",
            required_paths: &[InputPath::Dpad, InputPath::SpatialPointer],
            optional_paths: &[],
        }
        .normalize()
        .map(Ir::lower)
        .unwrap();
        let device = observations(
            vec![HardwareCapability::Dpad],
            MotionOutput::None,
            vec![InputPath::Dpad],
        );
        assert_eq!(
            recheck(&factory, &device, &[TouchInteraction::MouseScroll]),
            Err(RuntimeError::MissingRequiredPath(InputPath::SpatialPointer))
        );
    }
}
