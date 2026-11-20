//! Typed OEM input policies. The resolved policy is a capability ceiling.
//! An active input path also requires runtime evidence.

use std::fmt;

use crate::dcr::{self, DeviceProfile, InteractionSupport};
use crate::profiles::{self, OemProfile};
use crate::translate::DeviceCapabilities;

pub const CONTRACT: &str = "nevada.caprichio/0.1.0";

/// An input path that an OEM policy can authorize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputPath {
    Dpad,
    TvKeyboard,
    SpatialPointer,
    Voice,
    PhysicalKeyboard,
    TouchScrolling,
    Gamepad,
}

impl InputPath {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dpad => "dpad",
            Self::TvKeyboard => "tv-keyboard",
            Self::SpatialPointer => "spatial-pointer",
            Self::Voice => "voice",
            Self::PhysicalKeyboard => "physical-keyboard",
            Self::TouchScrolling => "touch-scrolling",
            Self::Gamepad => "gamepad",
        }
    }

    pub(crate) const fn supported_by(self, support: &InteractionSupport) -> bool {
        match self {
            Self::Dpad | Self::TvKeyboard => support.spatial_navigation,
            Self::SpatialPointer => support.spatial_pointer,
            Self::Voice => support.voice_input,
            Self::PhysicalKeyboard => support.keyboard_mouse,
            Self::TouchScrolling => support.touch_scrolling,
            Self::Gamepad => support.analogue_navigation,
        }
    }
}

/// Tie required and optional routes to an exact registered profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Declaration<'a> {
    pub profile_id: &'a str,
    pub required_paths: &'a [InputPath],
    pub optional_paths: &'a [InputPath],
}

/// Canonical, validated input policy. Its path lists are sorted and unique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ir {
    profile: &'static OemProfile,
    profile_id: String,
    required_paths: Vec<InputPath>,
    allowed_paths: Vec<InputPath>,
}

/// Factory lowered from an admitted input policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareFactory(Ir);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaprichioError {
    Profile(profiles::LookupError),
    Census(dcr::ResolveError),
    MissingRequiredDpad,
    DuplicatePath(InputPath),
    UnsupportedPath(InputPath),
    VoiceWithoutNonVoicePath,
}

impl fmt::Display for CaprichioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile(error) => error.fmt(f),
            Self::Census(error) => error.fmt(f),
            Self::MissingRequiredDpad => f.write_str("OEM input policy must require the D-pad"),
            Self::DuplicatePath(path) => {
                write!(f, "OEM input path '{}' is duplicated", path.as_str())
            }
            Self::UnsupportedPath(path) => {
                write!(f, "OEM census cannot authorize '{}'", path.as_str())
            }
            Self::VoiceWithoutNonVoicePath => {
                f.write_str("OEM voice input requires a non-voice text path")
            }
        }
    }
}

impl std::error::Error for CaprichioError {}

impl Declaration<'_> {
    pub fn normalize(self) -> Result<Ir, CaprichioError> {
        let profile = profiles::lookup(self.profile_id).map_err(CaprichioError::Profile)?;
        let support = dcr::resolve(&profile.census).map_err(CaprichioError::Census)?;
        if !self.required_paths.contains(&InputPath::Dpad) {
            return Err(CaprichioError::MissingRequiredDpad);
        }

        let mut allowed_paths = self.required_paths.to_vec();
        allowed_paths.extend_from_slice(self.optional_paths);
        allowed_paths.sort_unstable();
        if let Some(pair) = allowed_paths.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(CaprichioError::DuplicatePath(pair[0]));
        }
        if let Some(path) = allowed_paths
            .iter()
            .find(|path| !path.supported_by(&support))
        {
            return Err(CaprichioError::UnsupportedPath(*path));
        }
        if allowed_paths.contains(&InputPath::Voice)
            && !allowed_paths.contains(&InputPath::TvKeyboard)
            && !allowed_paths.contains(&InputPath::PhysicalKeyboard)
        {
            return Err(CaprichioError::VoiceWithoutNonVoicePath);
        }

        let mut required_paths = self.required_paths.to_vec();
        required_paths.sort_unstable();
        Ok(Ir {
            profile,
            profile_id: profile.id(),
            required_paths,
            allowed_paths,
        })
    }
}

impl Ir {
    pub fn lower(self) -> HardwareFactory {
        HardwareFactory(self)
    }
}

impl HardwareFactory {
    pub fn profile_id(&self) -> &str {
        &self.0.profile_id
    }

    pub fn census(&self) -> &DeviceProfile {
        &self.0.profile.census
    }

    pub fn required_paths(&self) -> &[InputPath] {
        &self.0.required_paths
    }

    pub fn allowed_paths(&self) -> &[InputPath] {
        &self.0.allowed_paths
    }

    /// The translator capability ceiling declared by this policy.
    pub fn translation_capabilities(&self) -> DeviceCapabilities {
        translation_capabilities(self.allowed_paths())
    }

    /// Canonical serialization for evidence and reproducible generation.
    pub fn canonical_ir(&self) -> String {
        let list = |paths: &[InputPath]| {
            paths
                .iter()
                .map(|path| path.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "{CONTRACT}\nprofile={}\nrequired={}\nallowed={}\n",
            self.profile_id(),
            list(self.required_paths()),
            list(self.allowed_paths()),
        )
    }
}

/// Compile the reviewed route policy for an exact built-in profile id.
/// The gamepad profile's Bluetooth transport alone cannot authorize a
/// physical keyboard.
pub fn compile_registered(profile_id: &str) -> Result<HardwareFactory, CaprichioError> {
    let profile = profiles::lookup(profile_id).map_err(CaprichioError::Profile)?;
    let optional_paths: &[InputPath] = match (profile.family, profile.version) {
        ("oem.generic.tv", "1.0.0") => &[InputPath::TvKeyboard],
        ("oem.acme.pointer-remote", "1.0.0") => &[
            InputPath::TvKeyboard,
            InputPath::SpatialPointer,
            InputPath::TouchScrolling,
        ],
        ("oem.acme.voice-remote", "1.0.0") => &[InputPath::TvKeyboard, InputPath::Voice],
        ("oem.generic.gamepad", "1.0.0") => &[InputPath::TvKeyboard, InputPath::Gamepad],
        _ => {
            return Err(CaprichioError::Profile(
                profiles::LookupError::UnknownProfile(profile_id.to_owned()),
            ))
        }
    };
    Declaration {
        profile_id,
        required_paths: &[InputPath::Dpad],
        optional_paths,
    }
    .normalize()
    .map(Ir::lower)
}

pub(crate) fn translation_capabilities(paths: &[InputPath]) -> DeviceCapabilities {
    DeviceCapabilities {
        spatial_pointer: paths.contains(&InputPath::SpatialPointer),
        voice_input: paths.contains(&InputPath::Voice),
        keyboard_available: paths.contains(&InputPath::PhysicalKeyboard),
        tv_keyboard: paths.contains(&InputPath::TvKeyboard),
        long_press_supported: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_profile_has_a_reviewed_factory() {
        for id in profiles::registered_ids() {
            let factory = compile_registered(&id).unwrap();
            assert_eq!(factory.profile_id(), id);
            assert_eq!(factory.required_paths(), &[InputPath::Dpad]);
            assert!(!factory
                .allowed_paths()
                .contains(&InputPath::PhysicalKeyboard));
        }
        assert!(compile_registered("oem.generic.tv/1.0.1").is_err());
    }

    #[test]
    fn policy_is_canonical_and_refuses_duplicates() {
        let declaration = |optional_paths| Declaration {
            profile_id: "oem.acme.pointer-remote/1.0.0",
            required_paths: &[InputPath::Dpad],
            optional_paths,
        };
        let original = declaration(&[InputPath::SpatialPointer, InputPath::TvKeyboard]);
        let reversed = declaration(&[InputPath::TvKeyboard, InputPath::SpatialPointer]);
        assert_eq!(
            original.normalize().unwrap().lower().canonical_ir(),
            reversed.normalize().unwrap().lower().canonical_ir()
        );
        assert_eq!(
            declaration(&[InputPath::Dpad]).normalize(),
            Err(CaprichioError::DuplicatePath(InputPath::Dpad))
        );
    }

    #[test]
    fn policy_refuses_unsupported_paths_and_voice_alone() {
        let mut declaration = Declaration {
            profile_id: "oem.generic.tv/1.0.0",
            required_paths: &[InputPath::Dpad],
            optional_paths: &[InputPath::Voice],
        };
        assert_eq!(
            declaration.normalize(),
            Err(CaprichioError::UnsupportedPath(InputPath::Voice))
        );
        declaration.profile_id = "oem.acme.voice-remote/1.0.0";
        assert_eq!(
            declaration.normalize(),
            Err(CaprichioError::VoiceWithoutNonVoicePath)
        );
        declaration.required_paths = &[];
        assert_eq!(
            declaration.normalize(),
            Err(CaprichioError::MissingRequiredDpad)
        );
    }
}
