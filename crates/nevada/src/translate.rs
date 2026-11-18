//! Interaction Translator (contract `nevada.translate/0.0.1`).
//!
//! Nevada is Goya's Android TV target. Touch/pointer-first interaction
//! vocabulary must be translated into remote-control-native intent: the
//! translator maps *what the user means* onto what the device can actually
//! express, never *how the original platform implemented it*.
//!
//! Semantics are fail-closed: if a required capability is absent and no
//! remote-native substitute exists, the translation is `Refused` with an
//! explicit `IncompatibilityReason`. There is no silent loss of intent.
//!
//! This module is intentionally dependency-free (std-only) and independent
//! of the other Nevada modules (dcr, profiles, focus); the device capability
//! summary is mirrored locally as [`DeviceCapabilities`].

/// A touch/pointer interaction as expressed on a direct-manipulation device.
///
/// This is the "left column" of the Nevada translation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TouchInteraction {
    /// Tap on a focusable element.
    TouchTap,
    /// Directional swipe.
    Swipe,
    /// Scroll wheel / two-finger scroll.
    MouseScroll,
    /// Pointer hover.
    Hover,
    /// Secondary-button click.
    RightClick,
    /// Drag one element onto another.
    DragAndDrop,
    /// Two-finger pinch to zoom.
    PinchZoom,
    /// Free-form text entry.
    TextInput,
    /// A navigation drawer (hamburger menu) interaction.
    NavigationDrawer,
    /// Press-and-hold.
    LongPress,
}

/// A remote-control-native interaction, the "right column" of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RemoteInteraction {
    /// Move focus to the tapped element and activate it (D-pad OK/Enter).
    FocusAndSelect,
    /// D-pad directional navigation.
    DirectionalNavigation,
    /// Scroll content directionally (page/line scrolling without a pointer).
    DirectionalScroll,
    /// Scroll only when a spatial pointer exists.
    PointerScroll,
    /// Focus gained/lost feedback.
    FocusState,
    /// Open a context menu (remote menu button).
    ContextAction,
    /// Select, reposition, confirm — remote equivalent of drag-and-drop.
    SelectRepositionConfirm,
    /// Stepped zoom in/out.
    IncrementalZoom,
    /// On-screen keyboard text entry.
    TvKeyboard,
    /// Voice-dictated text entry.
    Voice,
    /// Text entry via a connected physical keyboard.
    ConnectedKeyboard,
    /// A panel reachable by remote focus traversal (drawer equivalent).
    RemoteAccessiblePanel,
    /// Press-and-hold equivalent on the remote.
    RemoteLongPress,
}

/// Minimal device-capability summary needed by the translator.
///
/// Mirrors the concepts of Nevada's interaction-support profiles without
/// importing them, keeping this module self-contained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DeviceCapabilities {
    /// A spatial pointer (air mouse / pointer remote) is present.
    pub spatial_pointer: bool,
    /// Voice input (microphone on remote) is available.
    pub voice_input: bool,
    /// A physical keyboard is connected/available.
    pub keyboard_available: bool,
    /// The on-screen (TV) keyboard is usable for text entry.
    pub tv_keyboard: bool,
    /// The platform supports long-press gestures on the remote.
    pub long_press_supported: bool,
}

impl DeviceCapabilities {
    /// A bare D-pad-only remote: the minimal Android TV baseline.
    pub fn dpad_only() -> Self {
        Self {
            tv_keyboard: true,
            long_press_supported: false,
            spatial_pointer: false,
            voice_input: false,
            keyboard_available: false,
        }
    }
}

/// Why a translation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IncompatibilityReason {
    /// Text entry was requested but the device offers no text input method.
    NoTextInputMethodAvailable,
    /// The interaction has no remote-native equivalent on this device.
    NoRemoteEquivalent,
}

/// The result of translating one touch interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationOutcome {
    /// The intent survived translation: primary mapping plus ordered
    /// fallbacks (all acceptable; primary is the best match for the device).
    Translated {
        primary: RemoteInteraction,
        fallbacks: Vec<RemoteInteraction>,
    },
    /// The intent could not be expressed; nothing was silently dropped.
    Refused { reason: IncompatibilityReason },
}

/// Translate a single touch interaction into a remote-native outcome.
///
/// Deterministic and total: every input maps to exactly one outcome for a
/// given [`DeviceCapabilities`].
pub fn translate(interaction: TouchInteraction, ctx: &DeviceCapabilities) -> TranslationOutcome {
    match interaction {
        TouchInteraction::TouchTap => TranslationOutcome::Translated {
            primary: RemoteInteraction::FocusAndSelect,
            fallbacks: Vec::new(),
        },
        TouchInteraction::Swipe => TranslationOutcome::Translated {
            primary: RemoteInteraction::DirectionalNavigation,
            fallbacks: Vec::new(),
        },
        TouchInteraction::MouseScroll => {
            if ctx.spatial_pointer {
                TranslationOutcome::Translated {
                    primary: RemoteInteraction::PointerScroll,
                    fallbacks: vec![RemoteInteraction::DirectionalScroll],
                }
            } else {
                TranslationOutcome::Translated {
                    primary: RemoteInteraction::DirectionalScroll,
                    fallbacks: Vec::new(),
                }
            }
        }
        TouchInteraction::Hover => TranslationOutcome::Translated {
            primary: RemoteInteraction::FocusState,
            fallbacks: Vec::new(),
        },
        TouchInteraction::RightClick => TranslationOutcome::Translated {
            primary: RemoteInteraction::ContextAction,
            fallbacks: Vec::new(),
        },
        TouchInteraction::DragAndDrop => TranslationOutcome::Translated {
            primary: RemoteInteraction::SelectRepositionConfirm,
            fallbacks: Vec::new(),
        },
        TouchInteraction::PinchZoom => TranslationOutcome::Translated {
            primary: RemoteInteraction::IncrementalZoom,
            fallbacks: Vec::new(),
        },
        TouchInteraction::TextInput => translate_text_input(ctx),
        TouchInteraction::NavigationDrawer => TranslationOutcome::Translated {
            primary: RemoteInteraction::RemoteAccessiblePanel,
            fallbacks: Vec::new(),
        },
        TouchInteraction::LongPress => {
            if ctx.long_press_supported {
                TranslationOutcome::Translated {
                    primary: RemoteInteraction::RemoteLongPress,
                    fallbacks: vec![RemoteInteraction::ContextAction],
                }
            } else {
                TranslationOutcome::Translated {
                    primary: RemoteInteraction::ContextAction,
                    fallbacks: Vec::new(),
                }
            }
        }
    }
}

/// TextInput picks the best available input method, preferring the
/// connected keyboard (lowest latency, most reliable), then voice, then the
/// on-screen TV keyboard. Refuses if none are available.
fn translate_text_input(ctx: &DeviceCapabilities) -> TranslationOutcome {
    if ctx.keyboard_available {
        TranslationOutcome::Translated {
            primary: RemoteInteraction::ConnectedKeyboard,
            fallbacks: ordered_text_fallbacks(ctx),
        }
    } else if ctx.voice_input {
        TranslationOutcome::Translated {
            primary: RemoteInteraction::Voice,
            fallbacks: ordered_text_fallbacks(ctx),
        }
    } else if ctx.tv_keyboard {
        TranslationOutcome::Translated {
            primary: RemoteInteraction::TvKeyboard,
            fallbacks: Vec::new(),
        }
    } else {
        TranslationOutcome::Refused {
            reason: IncompatibilityReason::NoTextInputMethodAvailable,
        }
    }
}

/// Remaining acceptable text-entry methods, in the same preference order.
fn ordered_text_fallbacks(ctx: &DeviceCapabilities) -> Vec<RemoteInteraction> {
    let mut fallbacks = Vec::new();
    if ctx.keyboard_available {
        fallbacks.push(RemoteInteraction::ConnectedKeyboard);
    }
    if ctx.voice_input {
        fallbacks.push(RemoteInteraction::Voice);
    }
    if ctx.tv_keyboard {
        fallbacks.push(RemoteInteraction::TvKeyboard);
    }
    fallbacks.remove(0); // drop the primary; keep the rest
    fallbacks
}

/// Every touch interaction, in the canonical (declaration) order of the
/// translation table. Used to give reports a stable ordering.
pub const ALL_TOUCH_INTERACTIONS: [TouchInteraction; 10] = [
    TouchInteraction::TouchTap,
    TouchInteraction::Swipe,
    TouchInteraction::MouseScroll,
    TouchInteraction::Hover,
    TouchInteraction::RightClick,
    TouchInteraction::DragAndDrop,
    TouchInteraction::PinchZoom,
    TouchInteraction::TextInput,
    TouchInteraction::NavigationDrawer,
    TouchInteraction::LongPress,
];

/// One entry of a conformance report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationReportEntry {
    pub interaction: TouchInteraction,
    pub outcome: TranslationOutcome,
}

/// A deterministic, ordered per-interaction translation report for the
/// future Conformance Validator.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranslationReport {
    /// Entries that translated, in table order.
    pub translated: Vec<TranslationReportEntry>,
    /// Entries that were refused, in table order.
    pub refused: Vec<TranslationReportEntry>,
}

impl TranslationReport {
    /// All entries, in the canonical table order (translated first within
    /// each interaction position is not required; entries are simply split).
    pub fn entries(&self) -> impl Iterator<Item = &TranslationReportEntry> {
        self.translated.iter().chain(self.refused.iter())
    }
}

/// Translate every interaction in `interactions` and collect the outcomes.
pub fn translate_all(
    interactions: &[TouchInteraction],
    ctx: &DeviceCapabilities,
) -> Vec<TranslationOutcome> {
    interactions.iter().map(|i| translate(*i, ctx)).collect()
}

/// Build a deterministic conformance report.
///
/// Ordering is stable: report rows follow the input slice order; entries
/// that translate go into `translated`, refusals into `refused`.
pub fn conformance_report(
    interactions: &[TouchInteraction],
    ctx: &DeviceCapabilities,
) -> TranslationReport {
    let mut report = TranslationReport::default();
    for interaction in interactions {
        let outcome = translate(*interaction, ctx);
        let entry = TranslationReportEntry {
            interaction: *interaction,
            outcome,
        };
        match entry.outcome {
            TranslationOutcome::Translated { .. } => report.translated.push(entry),
            TranslationOutcome::Refused { .. } => report.refused.push(entry),
        }
    }
    report
}

/// Convenience: conformance report over the full canonical table.
pub fn full_conformance_report(ctx: &DeviceCapabilities) -> TranslationReport {
    conformance_report(&ALL_TOUCH_INTERACTIONS, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_translated(outcome: &TranslationOutcome) -> (&RemoteInteraction, &[RemoteInteraction]) {
        match outcome {
            TranslationOutcome::Translated { primary, fallbacks } => (primary, fallbacks),
            TranslationOutcome::Refused { .. } => panic!("expected Translated, got Refused"),
        }
    }

    fn as_refused(outcome: &TranslationOutcome) -> IncompatibilityReason {
        match outcome {
            TranslationOutcome::Refused { reason } => *reason,
            TranslationOutcome::Translated { .. } => panic!("expected Refused, got Translated"),
        }
    }

    #[test]
    fn every_row_translates_in_dpad_only_context() {
        let ctx = DeviceCapabilities::dpad_only();
        let expected = [
            (
                TouchInteraction::TouchTap,
                RemoteInteraction::FocusAndSelect,
            ),
            (
                TouchInteraction::Swipe,
                RemoteInteraction::DirectionalNavigation,
            ),
            (
                TouchInteraction::MouseScroll,
                RemoteInteraction::DirectionalScroll,
            ),
            (TouchInteraction::Hover, RemoteInteraction::FocusState),
            (
                TouchInteraction::RightClick,
                RemoteInteraction::ContextAction,
            ),
            (
                TouchInteraction::DragAndDrop,
                RemoteInteraction::SelectRepositionConfirm,
            ),
            (
                TouchInteraction::PinchZoom,
                RemoteInteraction::IncrementalZoom,
            ),
            (TouchInteraction::TextInput, RemoteInteraction::TvKeyboard),
            (
                TouchInteraction::NavigationDrawer,
                RemoteInteraction::RemoteAccessiblePanel,
            ),
            // long-press unsupported -> ContextAction
            (
                TouchInteraction::LongPress,
                RemoteInteraction::ContextAction,
            ),
        ];
        for (touch, remote) in expected {
            let outcome = translate(touch, &ctx);
            let (primary, _) = as_translated(&outcome);
            assert_eq!(primary, &remote, "wrong mapping for {touch:?}");
        }
    }

    #[test]
    fn long_press_prefers_remote_long_press_when_supported() {
        let mut ctx = DeviceCapabilities::dpad_only();
        ctx.long_press_supported = true;
        let outcome = translate(TouchInteraction::LongPress, &ctx);
        let (primary, fallbacks) = as_translated(&outcome);
        assert_eq!(primary, &RemoteInteraction::RemoteLongPress);
        assert_eq!(fallbacks, &[RemoteInteraction::ContextAction]);
    }

    #[test]
    fn mouse_scroll_uses_pointer_only_when_supported() {
        let mut ctx = DeviceCapabilities::dpad_only();
        ctx.spatial_pointer = true;
        let outcome = translate(TouchInteraction::MouseScroll, &ctx);
        let (primary, fallbacks) = as_translated(&outcome);
        assert_eq!(primary, &RemoteInteraction::PointerScroll);
        assert_eq!(fallbacks, &[RemoteInteraction::DirectionalScroll]);
    }

    #[test]
    fn mouse_scroll_falls_back_to_directional_scroll_without_pointer() {
        let ctx = DeviceCapabilities::dpad_only();
        let outcome = translate(TouchInteraction::MouseScroll, &ctx);
        let (primary, fallbacks) = as_translated(&outcome);
        assert_eq!(primary, &RemoteInteraction::DirectionalScroll);
        assert!(fallbacks.is_empty());
    }

    #[test]
    fn text_input_refuses_when_no_input_method_available() {
        let ctx = DeviceCapabilities {
            tv_keyboard: false,
            voice_input: false,
            keyboard_available: false,
            ..DeviceCapabilities::default()
        };
        let reason = as_refused(&translate(TouchInteraction::TextInput, &ctx));
        assert_eq!(reason, IncompatibilityReason::NoTextInputMethodAvailable);
    }

    #[test]
    fn text_input_picks_per_device_support_with_fallbacks() {
        let mut ctx = DeviceCapabilities::dpad_only();
        ctx.voice_input = true;
        ctx.keyboard_available = true;
        let outcome = translate(TouchInteraction::TextInput, &ctx);
        let (primary, fallbacks) = as_translated(&outcome);
        assert_eq!(primary, &RemoteInteraction::ConnectedKeyboard);
        assert_eq!(
            fallbacks,
            &[RemoteInteraction::Voice, RemoteInteraction::TvKeyboard]
        );

        ctx.keyboard_available = false;
        let outcome = translate(TouchInteraction::TextInput, &ctx);
        let (primary, fallbacks) = as_translated(&outcome);
        assert_eq!(primary, &RemoteInteraction::Voice);
        assert_eq!(fallbacks, &[RemoteInteraction::TvKeyboard]);
    }

    #[test]
    fn report_is_deterministic() {
        let ctx = DeviceCapabilities::dpad_only();
        let a = full_conformance_report(&ctx);
        let b = full_conformance_report(&ctx);
        assert_eq!(a, b);
    }

    #[test]
    fn report_splits_translated_and_refused_stably() {
        let ctx = DeviceCapabilities {
            tv_keyboard: false,
            ..DeviceCapabilities::default()
        };
        let interactions = [
            TouchInteraction::TouchTap,
            TouchInteraction::TextInput,
            TouchInteraction::Swipe,
            TouchInteraction::TextInput,
        ];
        let report = conformance_report(&interactions, &ctx);
        assert_eq!(report.translated.len(), 2);
        assert_eq!(report.refused.len(), 2);
        assert_eq!(report.translated[0].interaction, TouchInteraction::TouchTap);
        assert_eq!(report.translated[1].interaction, TouchInteraction::Swipe);
        for entry in &report.refused {
            assert_eq!(
                entry.outcome,
                TranslationOutcome::Refused {
                    reason: IncompatibilityReason::NoTextInputMethodAvailable
                }
            );
        }
    }

    #[test]
    fn translate_all_matches_individual_translation() {
        let ctx = DeviceCapabilities::dpad_only();
        let outcomes = translate_all(&ALL_TOUCH_INTERACTIONS, &ctx);
        assert_eq!(outcomes.len(), ALL_TOUCH_INTERACTIONS.len());
        for (interaction, outcome) in ALL_TOUCH_INTERACTIONS.iter().zip(&outcomes) {
            assert_eq!(&translate(*interaction, &ctx), outcome);
        }
    }

    #[test]
    fn full_table_translates_on_dpad_only_device() {
        // No silent loss anywhere: on a TV keyboard device, the whole table
        // translates with zero refusals.
        let report = full_conformance_report(&DeviceCapabilities::dpad_only());
        assert_eq!(report.translated.len(), ALL_TOUCH_INTERACTIONS.len());
        assert!(report.refused.is_empty());
    }
}
