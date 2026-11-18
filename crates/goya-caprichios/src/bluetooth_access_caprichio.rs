//! A bounded alternative-input mapping capability for iOS and Apple Silicon
//! Mac hosts: the Blue Moon Access core.
//!
//! The negotiated artifact binds to this crate's process-local Bluetooth
//! runtime. It does not receive real hardware events, dispatch real actions,
//! or prove that Core Bluetooth or any assistive technology is linked in a
//! consumer crate. Hosts on Apple hardware report their real accessibility
//! facilities through [`RuntimeCapabilities`]. System-managed accessibility
//! devices remain system-managed: a declaration that asks to intercept
//! protected input streams is rejected at normalization, and keyboards,
//! wearables and hearing devices follow system paths that this recipe never
//! models.

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.bluetooth.access.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_BINDINGS: usize = crate::bluetooth_runtime::MAX_BINDINGS;
pub const MAX_DEBOUNCE_MS: u32 = crate::bluetooth_runtime::MAX_DEBOUNCE_MS;
pub const MAX_LONG_PRESS_MS: u32 = crate::bluetooth_runtime::MAX_LONG_PRESS_MS;
pub const MAX_DOUBLE_PRESS_WINDOW_MS: u32 =
    crate::bluetooth_runtime::MAX_DOUBLE_PRESS_WINDOW_MS;

pub use crate::bluetooth_runtime::{
    EventOutcome, InputBinding, InputDispatch, InputEventKind, InputMappingSession, InputSource,
    SemanticAction,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothAccessDeclaration {
    /// Declared semantic mappings from input-source event kinds to actions.
    /// At least one is required: mappings are restricted to declared
    /// application requirements.
    pub bindings: Vec<InputBinding>,
    /// Debounce window for repeated identical events, 0 through
    /// `MAX_DEBOUNCE_MS` (0 disables suppression).
    pub debounce_ms: u32,
    /// Press duration at or beyond which a release is recognised as a long
    /// press, from one through `MAX_LONG_PRESS_MS`.
    pub long_press_threshold_ms: u32,
    /// Window within which a second single press is recognised as a double
    /// press, from one through `MAX_DOUBLE_PRESS_WINDOW_MS`.
    pub double_press_window_ms: u32,
    /// Must be `false`: system-managed accessibility devices remain
    /// system-managed and protected input streams are never intercepted.
    pub intercept_system_managed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothAccessIr {
    bindings: Vec<InputBinding>,
    debounce_ms: u32,
    long_press_threshold_ms: u32,
    double_press_window_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// iOS device with a Bluetooth-capable radio.
    Ios,
    /// Apple Silicon Mac with a Bluetooth-capable radio.
    AppleSiliconMac,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Host exposes a raw accessibility event surface (press/release where
    /// the device provides raw events).
    pub accessibility_event_surface: bool,
    /// Host assistive technologies surface the declared accessible labels.
    pub accessible_labels_supported: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    NoBindings,
    TooManyBindings,
    DuplicateBinding,
    DebounceBoundTooLarge,
    ZeroLongPressBound,
    LongPressBoundTooLarge,
    ZeroDoublePressBound,
    DoublePressBoundTooLarge,
    SystemManagedDevicesProtected,
    UnsupportedTarget,
    MissingAccessibilityEventSurface,
    MissingAccessibleLabels,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoBindings => {
                "at least one input binding is required; mappings are restricted \
                 to declared application requirements"
            }
            Self::TooManyBindings => "input binding count exceeds the supported bound",
            Self::DuplicateBinding => {
                "an input source and event kind was bound more than once"
            }
            Self::DebounceBoundTooLarge => "debounce bound exceeds the supported bound",
            Self::ZeroLongPressBound => "long-press threshold must be positive",
            Self::LongPressBoundTooLarge => {
                "long-press threshold exceeds the supported bound"
            }
            Self::ZeroDoublePressBound => "double-press window must be positive",
            Self::DoublePressBoundTooLarge => {
                "double-press window exceeds the supported bound"
            }
            Self::SystemManagedDevicesProtected => {
                "system-managed accessibility devices remain system-managed; \
                 protected input streams cannot be intercepted"
            }
            Self::UnsupportedTarget => {
                "the Bluetooth access factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingAccessibilityEventSurface => {
                "target runtime must provide a raw accessibility event surface"
            }
            Self::MissingAccessibleLabels => {
                "target assistive technologies must surface declared accessible labels"
            }
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled,
/// that any hardware event arrived, or that any semantic action dispatched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothAccessArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl BluetoothAccessDeclaration {
    pub fn normalize(self) -> Result<BluetoothAccessIr, Diagnostic> {
        if self.intercept_system_managed {
            return Err(Diagnostic::SystemManagedDevicesProtected);
        }
        if self.bindings.is_empty() {
            return Err(Diagnostic::NoBindings);
        }
        if self.bindings.len() > MAX_BINDINGS {
            return Err(Diagnostic::TooManyBindings);
        }
        let mut seen = Vec::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            let key = (binding.source(), binding.event());
            if seen.contains(&key) {
                return Err(Diagnostic::DuplicateBinding);
            }
            seen.push(key);
        }
        if self.debounce_ms > MAX_DEBOUNCE_MS {
            return Err(Diagnostic::DebounceBoundTooLarge);
        }
        if self.long_press_threshold_ms == 0 {
            return Err(Diagnostic::ZeroLongPressBound);
        }
        if self.long_press_threshold_ms > MAX_LONG_PRESS_MS {
            return Err(Diagnostic::LongPressBoundTooLarge);
        }
        if self.double_press_window_ms == 0 {
            return Err(Diagnostic::ZeroDoublePressBound);
        }
        if self.double_press_window_ms > MAX_DOUBLE_PRESS_WINDOW_MS {
            return Err(Diagnostic::DoublePressBoundTooLarge);
        }
        Ok(BluetoothAccessIr {
            bindings: self.bindings,
            debounce_ms: self.debounce_ms,
            long_press_threshold_ms: self.long_press_threshold_ms,
            double_press_window_ms: self.double_press_window_ms,
        })
    }
}

impl BluetoothAccessIr {
    pub fn bindings(&self) -> &[InputBinding] {
        &self.bindings
    }

    pub fn debounce_ms(&self) -> u32 {
        self.debounce_ms
    }

    pub fn long_press_threshold_ms(&self) -> u32 {
        self.long_press_threshold_ms
    }

    pub fn double_press_window_ms(&self) -> u32 {
        self.double_press_window_ms
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::Ios => "ios",
            Target::AppleSiliconMac => "apple_silicon_mac",
            Target::CrossPlatform | Target::Browser | Target::Node => "unsupported",
        }
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsession=ble_access_bounded_mapping\nbindings={}\ndebounce_ms={}\nlong_press_threshold_ms={}\ndouble_press_window_ms={}\nsystem_managed_devices=untouched\nprotected_streams_intercepted=none\n",
            Self::target_label(target),
            self.bindings.len(),
            self.debounce_ms,
            self.long_press_threshold_ms,
            self.double_press_window_ms
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<BluetoothAccessArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.accessibility_event_surface {
            return Err(Diagnostic::MissingAccessibilityEventSurface);
        }
        if !capabilities.accessible_labels_supported {
            return Err(Diagnostic::MissingAccessibleLabels);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        let binding_arguments = self
            .bindings
            .iter()
            .map(|binding| {
                format!(
                    "crate::bluetooth_runtime::InputBinding::new(crate::bluetooth_runtime::InputSource::{source}, crate::bluetooth_runtime::InputEventKind::{event}, crate::bluetooth_runtime::SemanticAction::{action}, {label:?})?,",
                    source = match binding.source() {
                        InputSource::AccessibilitySwitch => "AccessibilitySwitch",
                        InputSource::AdaptiveController => "AdaptiveController",
                        InputSource::GameController => "GameController",
                        InputSource::CustomButton => "CustomButton",
                    },
                    event = match binding.event() {
                        InputEventKind::SinglePress => "SinglePress",
                        InputEventKind::DoublePress => "DoublePress",
                        InputEventKind::LongPress => "LongPress",
                    },
                    action = match binding.action() {
                        SemanticAction::Select => "Select".to_owned(),
                        SemanticAction::Confirm => "Confirm".to_owned(),
                        SemanticAction::NavigateNext => "NavigateNext".to_owned(),
                        SemanticAction::NavigatePrevious => "NavigatePrevious".to_owned(),
                        SemanticAction::Back => "Back".to_owned(),
                        SemanticAction::Custom(name) => format!("Custom({name:?}.to_owned())"),
                    },
                    label = binding.label()
                )
            })
            .collect::<Vec<_>>()
            .join("\n    ");
        Ok(BluetoothAccessArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe=apple-ble-access-bounded-mapping; {} input mapping session, no hardware event is received and no system-managed input stream is touched.\npub fn make_input_mapping_session() -> Result<crate::bluetooth_runtime::InputMappingSession, crate::bluetooth_runtime::BluetoothError> {{\n    crate::bluetooth_runtime::InputMappingSession::new(vec![\n    {binding_arguments}\n    ], {}u32, {}u32, {}u32)\n}}\n",
                Self::target_label(target),
                self.debounce_ms,
                self.long_press_threshold_ms,
                self.double_press_window_ms
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        accessibility_event_surface: true,
        accessible_labels_supported: true,
        utf8_strings: true,
    };

    fn binding(
        source: InputSource,
        event: InputEventKind,
        action: SemanticAction,
        label: &str,
    ) -> InputBinding {
        InputBinding::new(source, event, action, label).unwrap()
    }

    fn declaration() -> BluetoothAccessDeclaration {
        BluetoothAccessDeclaration {
            bindings: vec![
                binding(
                    InputSource::AccessibilitySwitch,
                    InputEventKind::SinglePress,
                    SemanticAction::Select,
                    "Primary select",
                ),
                binding(
                    InputSource::AccessibilitySwitch,
                    InputEventKind::DoublePress,
                    SemanticAction::NavigateNext,
                    "Move to the next element",
                ),
                binding(
                    InputSource::AccessibilitySwitch,
                    InputEventKind::LongPress,
                    SemanticAction::Back,
                    "Return to the previous screen",
                ),
                binding(
                    InputSource::GameController,
                    InputEventKind::SinglePress,
                    SemanticAction::Confirm,
                    "Confirm the focused action",
                ),
            ],
            debounce_ms: 500,
            long_press_threshold_ms: 400,
            double_press_window_ms: 300,
            intercept_system_managed: false,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.bindings().len(), 4);
        assert_eq!(ir.debounce_ms(), 500);
        assert_eq!(ir.long_press_threshold_ms(), 400);
        assert_eq!(ir.double_press_window_ms(), 300);
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first.canonical_ir.contains("session=ble_access_bounded_mapping\n"));
        assert!(first.canonical_ir.contains("bindings=4\n"));
        assert!(first.canonical_ir.contains("debounce_ms=500\n"));
        assert!(first.canonical_ir.contains("long_press_threshold_ms=400\n"));
        assert!(first.canonical_ir.contains("double_press_window_ms=300\n"));
        assert!(first.canonical_ir.contains("system_managed_devices=untouched\n"));
        assert!(first
            .canonical_ir
            .contains("protected_streams_intercepted=none\n"));
        assert!(first.source.contains(
            "InputBinding::new(crate::bluetooth_runtime::InputSource::AccessibilitySwitch, crate::bluetooth_runtime::InputEventKind::SinglePress, crate::bluetooth_runtime::SemanticAction::Select, \"Primary select\")?"
        ));
        assert!(first.source.contains(
            "InputMappingSession::new(vec![\n    crate::bluetooth_runtime::InputBinding::new(crate::bluetooth_runtime::InputSource::AccessibilitySwitch"
        ));
    }

    #[test]
    fn rejects_protected_streams_and_bad_declarations() {
        assert_eq!(
            BluetoothAccessDeclaration {
                intercept_system_managed: true,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::SystemManagedDevicesProtected)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                bindings: Vec::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NoBindings)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                bindings: vec![
                    binding(
                        InputSource::AccessibilitySwitch,
                        InputEventKind::SinglePress,
                        SemanticAction::Select,
                        "one"
                    ),
                    binding(
                        InputSource::AccessibilitySwitch,
                        InputEventKind::SinglePress,
                        SemanticAction::Confirm,
                        "two"
                    ),
                ],
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::DuplicateBinding)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                bindings: (0..=MAX_BINDINGS)
                    .map(|index| binding(
                        InputSource::CustomButton,
                        InputEventKind::SinglePress,
                        SemanticAction::Custom(format!("action_{index}")),
                        "label"
                    ))
                    .collect(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::TooManyBindings)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                debounce_ms: MAX_DEBOUNCE_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::DebounceBoundTooLarge)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                long_press_threshold_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroLongPressBound)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                long_press_threshold_ms: MAX_LONG_PRESS_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::LongPressBoundTooLarge)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                double_press_window_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroDoublePressBound)
        );
        assert_eq!(
            BluetoothAccessDeclaration {
                double_press_window_ms: MAX_DOUBLE_PRESS_WINDOW_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::DoublePressBoundTooLarge)
        );
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration().normalize().unwrap();
        for target in [Target::CrossPlatform, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, IOS),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    accessibility_event_surface: false,
                    ..IOS
                },
                Diagnostic::MissingAccessibilityEventSurface,
            ),
            (
                RuntimeCapabilities {
                    accessible_labels_supported: false,
                    ..IOS
                },
                Diagnostic::MissingAccessibleLabels,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..IOS
                },
                Diagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::Ios, missing), Err(expected));
        }
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_bluetooth_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::Ios, IOS)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bluetooth_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod bluetooth_runtime;\n{}\nuse bluetooth_runtime::{{BluetoothError, EventOutcome, InputEventKind, InputSource, SemanticAction}};\nfn main() {{\n    let mut session = make_input_mapping_session().expect(\"factory should construct session\");\n    assert_eq!(session.debounce_ms(), 500);\n    assert_eq!(session.long_press_threshold_ms(), 400);\n    assert_eq!(session.double_press_window_ms(), 300);\n    assert_eq!(session.bindings(), 4);\n    assert_eq!(session.release(InputSource::AccessibilitySwitch, 10), Err(BluetoothError::WrongState));\n    session.press(InputSource::AccessibilitySwitch, 10).unwrap();\n    assert_eq!(session.press(InputSource::AccessibilitySwitch, 20), Err(BluetoothError::WrongState));\n    assert_eq!(session.release(InputSource::AccessibilitySwitch, 5), Err(BluetoothError::InvalidTimestamp));\n    let dispatch = match session.release(InputSource::AccessibilitySwitch, 50).unwrap() {{\n        EventOutcome::Dispatched(dispatch) => dispatch,\n        EventOutcome::Suppressed => panic!(\"first event must dispatch\"),\n    }};\n    assert_eq!(dispatch.event(), InputEventKind::SinglePress);\n    assert_eq!(dispatch.action(), &SemanticAction::Select);\n    assert_eq!(dispatch.label(), \"Primary select\");\n    session.press(InputSource::AccessibilitySwitch, 100).unwrap();\n    let dispatch = match session.release(InputSource::AccessibilitySwitch, 150).unwrap() {{\n        EventOutcome::Dispatched(dispatch) => dispatch,\n        EventOutcome::Suppressed => panic!(\"double press must dispatch\"),\n    }};\n    assert_eq!(dispatch.event(), InputEventKind::DoublePress);\n    assert_eq!(dispatch.action(), &SemanticAction::NavigateNext);\n    session.press(InputSource::AccessibilitySwitch, 1000).unwrap();\n    let dispatch = match session.release(InputSource::AccessibilitySwitch, 1600).unwrap() {{\n        EventOutcome::Dispatched(dispatch) => dispatch,\n        EventOutcome::Suppressed => panic!(\"long press must dispatch\"),\n    }};\n    assert_eq!(dispatch.event(), InputEventKind::LongPress);\n    assert_eq!(dispatch.action(), &SemanticAction::Back);\n    session.press(InputSource::GameController, 2000).unwrap();\n    let dispatch = match session.release(InputSource::GameController, 2050).unwrap() {{\n        EventOutcome::Dispatched(dispatch) => dispatch,\n        EventOutcome::Suppressed => panic!(\"controller press must dispatch\"),\n    }};\n    assert_eq!(dispatch.action(), &SemanticAction::Confirm);\n    session.press(InputSource::GameController, 2100).unwrap();\n    assert_eq!(session.release(InputSource::GameController, 2150), Err(BluetoothError::UnmappedEvent));\n    assert_eq!(session.dispatched(), 4);\n    assert_eq!(session.unmapped(), 1);\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-bluetooth-access-factory-{}-{nonce}",
            std::process::id()
        ));
        let mut rustc = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&output_path)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        rustc
            .stdin
            .take()
            .expect("rustc stdin")
            .write_all(harness.as_bytes())
            .expect("write generated factory harness");
        let compile = rustc.wait_with_output().expect("wait for rustc");
        assert!(
            compile.status.success(),
            "generated factory failed to compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let execution = Command::new(&output_path)
            .output()
            .expect("run generated Bluetooth access factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
