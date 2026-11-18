//! Device Capability Resolver (contract `nevada.dcr/0.0.1`).
//!
//! The DCR turns a raw hardware capability census into a resolved
//! [`InteractionSupport`] summary. Semantics are fail-closed and
//! deterministic: a capability that is absent, or present without a usable
//! output path (e.g. a motion sensor that emits no pointer events or motion
//! data stream), is reported as unsupported. The same census in always
//! yields the same result out.

/// A hardware capability the device may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HardwareCapability {
    /// D-pad directional pad. Required baseline for a TV target.
    Dpad,
    /// Accelerometer motion sensor.
    Accelerometer,
    /// Gyroscope motion sensor.
    Gyroscope,
    /// Touchpad / pointer surface on the remote.
    Touchpad,
    /// Microphone for voice input.
    Microphone,
    /// Bluetooth radio connectivity.
    Bluetooth,
    /// USB host/device connectivity.
    Usb,
    /// Dedicated game controller profile.
    GameController,
}

/// Whether a capability is mandatory baseline or nice-to-have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Requirement {
    /// The TV target cannot operate without this.
    Required,
    /// Absence degrades gracefully.
    Optional,
}

impl HardwareCapability {
    /// Baseline requirement of this capability for the Nevada TV target.
    /// D-pad is the only `Required` baseline; everything else is optional.
    pub const fn requirement(self) -> Requirement {
        match self {
            Self::Dpad => Requirement::Required,
            Self::Accelerometer
            | Self::Gyroscope
            | Self::Touchpad
            | Self::Microphone
            | Self::Bluetooth
            | Self::Usb
            | Self::GameController => Requirement::Optional,
        }
    }
}

/// How motion sensor data can actually reach the application.
///
/// Sensor *presence* is not sensor *usability*: the enhanced
/// motion-driven interaction may only be enabled when the device exposes a
/// usable output path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum MotionOutput {
    /// The sensor (if any) emits nothing the application can consume.
    /// Presence without usable output must not enable the interaction.
    #[default]
    None,
    /// The platform surfaces motion as native pointer events.
    NativePointerEvents,
    /// The platform exposes a raw motion data stream.
    MotionDataStream,
}

/// A raw device capability census, supplied by the caller. The resolver does
/// not infer hardware facts from OS labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DeviceProfile {
    /// Capabilities physically present on the device.
    pub present: Vec<HardwareCapability>,
    /// Usable output path for motion sensor data, if any.
    pub motion_output: MotionOutput,
}

impl DeviceProfile {
    /// Fail-closed convenience: is this capability present in the census?
    fn has(&self, capability: HardwareCapability) -> bool {
        self.present.contains(&capability)
    }
}

/// A resolved, fail-closed summary of the interactions the device supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct InteractionSupport {
    /// D-pad style directional focus traversal.
    pub spatial_navigation: bool,
    /// Motion-gesture input (shake, tilt commands) via a usable output path.
    pub motion_gestures: bool,
    /// Spatial pointer (air mouse) cursor control.
    pub spatial_pointer: bool,
    /// Smooth touch scrolling on a pointer surface.
    pub touch_scrolling: bool,
    /// Voice dictation and commands.
    pub voice_input: bool,
    /// Peripheral attachment (Bluetooth or USB) for keyboards, mice, pads.
    pub peripheral_connectivity: bool,
    /// Physical keyboard / mouse text-and-pointer interaction.
    pub keyboard_mouse: bool,
    /// Analogue stick navigation from a game controller.
    pub analogue_navigation: bool,
}

/// Diagnostic returned when a census violates a baseline invariant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResolveError {
    /// The census is missing the required D-pad baseline.
    MissingRequiredBaseline(HardwareCapability),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequiredBaseline(capability) => {
                write!(f, "required baseline capability `{capability:?}` is absent")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Contract identifier for this module.
pub const CONTRACT: &str = "nevada.dcr/0.0.1";

/// Resolve a raw census into an [`InteractionSupport`].
///
/// Fail-closed: every enhanced interaction requires an explicit, usable
/// capability in the census; ambiguous or absent capabilities resolve to
/// `false`. Deterministic: the same census always produces the same result.
pub fn resolve(census: &DeviceProfile) -> Result<InteractionSupport, ResolveError> {
    if !census.has(HardwareCapability::Dpad) {
        return Err(ResolveError::MissingRequiredBaseline(
            HardwareCapability::Dpad,
        ));
    }

    // A motion sensor without a usable output path must NOT enable the
    // enhanced interaction (fail-closed).
    let motion_sensor_present =
        census.has(HardwareCapability::Accelerometer) || census.has(HardwareCapability::Gyroscope);
    let motion_usable = motion_sensor_present && census.motion_output != MotionOutput::None;

    let peripheral =
        census.has(HardwareCapability::Bluetooth) || census.has(HardwareCapability::Usb);

    Ok(InteractionSupport {
        spatial_navigation: true,
        motion_gestures: motion_usable,
        spatial_pointer: motion_usable && census.motion_output == MotionOutput::NativePointerEvents,
        touch_scrolling: census.has(HardwareCapability::Touchpad),
        voice_input: census.has(HardwareCapability::Microphone),
        peripheral_connectivity: peripheral,
        keyboard_mouse: peripheral,
        analogue_navigation: census.has(HardwareCapability::GameController),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn census(present: &[HardwareCapability], motion_output: MotionOutput) -> DeviceProfile {
        DeviceProfile {
            present: present.to_vec(),
            motion_output,
        }
    }

    #[test]
    fn dpad_only_baseline() {
        let support = resolve(&census(&[HardwareCapability::Dpad], MotionOutput::None))
            .expect("dpad satisfies the baseline");
        assert!(support.spatial_navigation);
        assert!(!support.motion_gestures);
        assert!(!support.spatial_pointer);
        assert!(!support.touch_scrolling);
        assert!(!support.voice_input);
        assert!(!support.peripheral_connectivity);
        assert!(!support.keyboard_mouse);
        assert!(!support.analogue_navigation);
    }

    #[test]
    fn missing_dpad_fails_closed() {
        let error = resolve(&census(&[HardwareCapability::Touchpad], MotionOutput::None))
            .expect_err("absent D-pad must fail");
        assert_eq!(
            error,
            ResolveError::MissingRequiredBaseline(HardwareCapability::Dpad)
        );
    }

    #[test]
    fn motion_sensor_without_output_is_not_usable() {
        let support = resolve(&census(
            &[
                HardwareCapability::Dpad,
                HardwareCapability::Accelerometer,
                HardwareCapability::Gyroscope,
            ],
            MotionOutput::None,
        ))
        .expect("dpad satisfies the baseline");
        assert!(!support.spatial_pointer, "sensor present but no output");
        assert!(!support.motion_gestures, "presence != usability");
    }

    #[test]
    fn motion_data_stream_enables_gestures_but_not_pointer() {
        let support = resolve(&census(
            &[HardwareCapability::Dpad, HardwareCapability::Gyroscope],
            MotionOutput::MotionDataStream,
        ))
        .expect("dpad satisfies the baseline");
        assert!(support.motion_gestures);
        assert!(!support.spatial_pointer);
    }

    #[test]
    fn full_census() {
        let support = resolve(&census(
            &[
                HardwareCapability::Dpad,
                HardwareCapability::Accelerometer,
                HardwareCapability::Gyroscope,
                HardwareCapability::Touchpad,
                HardwareCapability::Microphone,
                HardwareCapability::Bluetooth,
                HardwareCapability::Usb,
                HardwareCapability::GameController,
            ],
            MotionOutput::NativePointerEvents,
        ))
        .expect("dpad satisfies the baseline");
        assert_eq!(
            support,
            InteractionSupport {
                spatial_navigation: true,
                motion_gestures: true,
                spatial_pointer: true,
                touch_scrolling: true,
                voice_input: true,
                peripheral_connectivity: true,
                keyboard_mouse: true,
                analogue_navigation: true,
            }
        );
    }

    #[test]
    fn resolve_is_deterministic() {
        let sample = census(
            &[
                HardwareCapability::Dpad,
                HardwareCapability::Accelerometer,
                HardwareCapability::Touchpad,
                HardwareCapability::Bluetooth,
            ],
            MotionOutput::NativePointerEvents,
        );
        let first = resolve(&sample).expect("dpad satisfies the baseline");
        for _ in 0..8 {
            assert_eq!(resolve(&sample), Ok(first));
        }
    }

    #[test]
    fn dpad_is_the_only_required_baseline() {
        for capability in [
            HardwareCapability::Dpad,
            HardwareCapability::Accelerometer,
            HardwareCapability::Gyroscope,
            HardwareCapability::Touchpad,
            HardwareCapability::Microphone,
            HardwareCapability::Bluetooth,
            HardwareCapability::Usb,
            HardwareCapability::GameController,
        ] {
            let expected = if capability == HardwareCapability::Dpad {
                Requirement::Required
            } else {
                Requirement::Optional
            };
            assert_eq!(capability.requirement(), expected);
        }
    }
}
