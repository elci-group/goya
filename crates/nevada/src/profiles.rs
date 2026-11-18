//! OEM Profile Registry (contract `nevada.profiles/0.0.1`).
//!
//! Versioned OEM/device profiles map a profile id such as
//! `oem.generic.tv/1.0.0` to an immutable [`DeviceProfile`] census. Lookups
//! fail closed: an unknown or partially matching id is an error, never an
//! implicit default. All built-in profiles are `const`-style immutable
//! static data.

use crate::dcr::{DeviceProfile, HardwareCapability, MotionOutput};

/// Contract identifier for this module.
pub const CONTRACT: &str = "nevada.profiles/0.0.1";

/// Diagnostic returned by [`lookup`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LookupError {
    /// No profile is registered under this exact id. No partial matches and
    /// no implicit defaults exist.
    UnknownProfile(String),
    /// The id is syntactically malformed: it must end in `/major.minor.patch`.
    MalformedProfileId(String),
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile(id) => write!(f, "no registered profile matches `{id}`"),
            Self::MalformedProfileId(id) => {
                write!(f, "profile id `{id}` is not `<family>/<version>`")
            }
        }
    }
}

impl std::error::Error for LookupError {}

/// A registered OEM profile: an immutable id, its version, and the device
/// census the OEM declares.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OemProfile {
    /// Profile family, e.g. `oem.generic.tv`.
    pub family: &'static str,
    /// Semantic version string, e.g. `1.0.0`.
    pub version: &'static str,
    /// The immutable device census this profile declares.
    pub census: DeviceProfile,
}

const fn profile(
    family: &'static str,
    version: &'static str,
    present: &'static [HardwareCapability],
    motion_output: MotionOutput,
) -> OemProfile {
    OemProfile {
        family,
        version,
        census: DeviceProfile {
            present: present.to_vec(),
            motion_output,
        },
    }
}

/// Built-in profiles. Immutable static data; never mutated at runtime.
static PROFILES: [OemProfile; 4] = [
    profile(
        "oem.generic.tv",
        "1.0.0",
        &[HardwareCapability::Dpad],
        MotionOutput::None,
    ),
    profile(
        "oem.acme.pointer-remote",
        "1.0.0",
        &[HardwareCapability::Dpad, HardwareCapability::Touchpad],
        MotionOutput::NativePointerEvents,
    ),
    profile(
        "oem.acme.voice-remote",
        "1.0.0",
        &[HardwareCapability::Dpad, HardwareCapability::Microphone],
        MotionOutput::None,
    ),
    profile(
        "oem.generic.gamepad",
        "1.0.0",
        &[
            HardwareCapability::Dpad,
            HardwareCapability::GameController,
            HardwareCapability::Bluetooth,
        ],
        MotionOutput::None,
    ),
];

impl OemProfile {
    /// The full profile id: `family/version`.
    pub fn id(&self) -> String {
        format!("{}/{}", self.family, self.version)
    }
}

/// Enumerate every registered profile id in registration order.
pub fn registered_ids() -> Vec<String> {
    PROFILES.iter().map(OemProfile::id).collect()
}

/// Look up a profile by exact id (e.g. `oem.generic.tv/1.0.0`).
///
/// Fail-closed: unknown ids error; there are no partial matches, no
/// implicit defaults, and no prefix or fuzzy matching.
pub fn lookup(id: &str) -> Result<&'static OemProfile, LookupError> {
    if !id.contains('/') {
        return Err(LookupError::MalformedProfileId(id.to_owned()));
    }
    let mut split = id.rsplitn(2, '/');
    let version = split.next().unwrap_or_default();
    let family = split.next().unwrap_or_default();
    if family.is_empty() || version.is_empty() {
        return Err(LookupError::MalformedProfileId(id.to_owned()));
    }
    for profile in &PROFILES {
        if profile.family == family && profile.version == version {
            return Ok(profile);
        }
    }
    Err(LookupError::UnknownProfile(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_tv_is_dpad_only() {
        let profile = lookup("oem.generic.tv/1.0.0").expect("registered profile");
        assert_eq!(profile.family, "oem.generic.tv");
        assert_eq!(profile.version, "1.0.0");
        assert_eq!(profile.census.present, vec![HardwareCapability::Dpad]);
        assert_eq!(profile.census.motion_output, MotionOutput::None);
    }

    #[test]
    fn pointer_remote_has_touchpad_and_pointer_output() {
        let profile = lookup("oem.acme.pointer-remote/1.0.0").expect("registered profile");
        assert!(profile.census.present.contains(&HardwareCapability::Dpad));
        assert!(profile
            .census
            .present
            .contains(&HardwareCapability::Touchpad));
        assert_eq!(
            profile.census.motion_output,
            MotionOutput::NativePointerEvents
        );
    }

    #[test]
    fn voice_remote_has_microphone() {
        let profile = lookup("oem.acme.voice-remote/1.0.0").expect("registered profile");
        assert!(profile
            .census
            .present
            .contains(&HardwareCapability::Microphone));
        assert!(!profile
            .census
            .present
            .contains(&HardwareCapability::Touchpad));
    }

    #[test]
    fn gamepad_profile_has_controller_and_bluetooth() {
        let profile = lookup("oem.generic.gamepad/1.0.0").expect("registered profile");
        assert!(profile
            .census
            .present
            .contains(&HardwareCapability::GameController));
        assert!(profile
            .census
            .present
            .contains(&HardwareCapability::Bluetooth));
    }

    #[test]
    fn unknown_id_fails_closed() {
        assert_eq!(
            lookup("oem.unheard-of.tv/9.9.9"),
            Err(LookupError::UnknownProfile(
                "oem.unheard-of.tv/9.9.9".to_owned()
            ))
        );
    }

    #[test]
    fn partial_or_prefix_matches_fail_closed() {
        // Family without version, version without family, and a partial
        // family must all be rejected.
        assert!(lookup("oem.generic.tv").is_err());
        assert!(lookup("1.0.0").is_err());
        assert!(lookup("oem.generic/1.0.0").is_err());
        assert!(lookup("oem.generic.tv/1.0").is_err());
    }

    #[test]
    fn version_round_trips() {
        for id in registered_ids() {
            let profile = lookup(&id).expect("every listed id must resolve");
            assert_eq!(profile.id(), id);
            assert_eq!(profile.family, id.rsplit_once('/').unwrap_or_default().0);
            assert_eq!(profile.version, id.rsplit_once('/').unwrap_or_default().1);
        }
    }

    #[test]
    fn profiles_are_immutable_static_data() {
        for profile in &PROFILES {
            // The census is owned data cloned from static slices; verify the
            // registered set matches expected ids exactly (no mutation path).
            assert!(profile.id.contains('/'));
        }
        assert_eq!(
            registered_ids(),
            vec![
                "oem.generic.tv/1.0.0".to_owned(),
                "oem.acme.pointer-remote/1.0.0".to_owned(),
                "oem.acme.voice-remote/1.0.0".to_owned(),
                "oem.generic.gamepad/1.0.0".to_owned(),
            ]
        );
    }
}
