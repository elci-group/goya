use super::Definition;

pub const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "display.surfaces",
        summary: "Describe display surfaces and their observable geometry.",
        operations: &["enumerate surfaces", "read surface bounds and scale"],
        invariants: &[
            "surface identifiers are stable for a session",
            "bounds use a documented coordinate space",
        ],
    },
    Definition {
        id: "display.modes",
        summary: "Inspect supported display modes and the active mode.",
        operations: &["list supported modes", "read the active mode"],
        invariants: &[
            "reported modes have positive dimensions",
            "the active mode belongs to the reported supported set",
        ],
    },
    Definition {
        id: "display.brightness",
        summary: "Read or adjust display brightness where exposed.",
        operations: &["read brightness", "set brightness"],
        invariants: &[
            "brightness values have a defined normalized range",
            "failed changes leave the prior value intact",
        ],
    },
    Definition {
        id: "display.orientation",
        summary: "Inspect and change a display's orientation.",
        operations: &["read orientation", "set orientation"],
        invariants: &[
            "orientation is one of the declared values",
            "geometry reflects an applied orientation change",
        ],
    },
    Definition {
        id: "display.color_profile",
        summary: "Inspect color profiles associated with display surfaces.",
        operations: &["list available profiles", "read the active profile"],
        invariants: &[
            "profile references identify a single surface",
            "an unavailable profile is never reported as active",
        ],
    },
    Definition {
        id: "display.refresh_rate",
        summary: "Inspect and select display refresh rates.",
        operations: &["list refresh rates", "select a refresh rate"],
        invariants: &[
            "rates are positive and expressed in hertz",
            "selected rates are supported by the target surface",
        ],
    },
    Definition {
        id: "display.configuration",
        summary: "Read and update the arrangement of display surfaces.",
        operations: &["read surface arrangement", "apply surface arrangement"],
        invariants: &[
            "a surface cannot be positioned relative to itself",
            "configuration updates are validated before application",
        ],
    },
    Definition {
        id: "devices.inventory",
        summary: "Enumerate attached devices and their descriptive properties.",
        operations: &["list devices", "read device properties"],
        invariants: &[
            "device identifiers are unique within the inventory",
            "properties distinguish unknown values from empty values",
        ],
    },
    Definition {
        id: "devices.attachments",
        summary: "Observe device attachment and detachment changes.",
        operations: &[
            "subscribe to attachment changes",
            "read current attachment state",
        ],
        invariants: &[
            "events include the affected device identifier",
            "event ordering is defined within a subscription",
        ],
    },
    Definition {
        id: "devices.input.keyboard",
        summary: "Describe keyboard capabilities and key state events.",
        operations: &[
            "read keyboard capabilities",
            "subscribe to key state events",
        ],
        invariants: &[
            "key events identify press or release",
            "event values use a documented key naming scheme",
        ],
    },
    Definition {
        id: "devices.input.pointer",
        summary: "Observe pointer position, buttons, and movement events.",
        operations: &["read pointer position", "subscribe to pointer events"],
        invariants: &[
            "coordinates identify their reference surface",
            "button events distinguish press from release",
        ],
    },
    Definition {
        id: "devices.input.touch",
        summary: "Observe touch contact lifecycle and positions.",
        operations: &["subscribe to touch contacts", "read active contact state"],
        invariants: &[
            "contact identifiers remain stable during a contact",
            "each contact has a defined start and end",
        ],
    },
    Definition {
        id: "devices.input.gamepad",
        summary: "Read game controller controls and state changes.",
        operations: &[
            "read controller layout",
            "subscribe to control state changes",
        ],
        invariants: &[
            "axis values declare their range",
            "button state changes identify the control",
        ],
    },
    Definition {
        id: "devices.input.accessibility",
        summary: "Inspect input accessibility settings and available controls.",
        operations: &[
            "read input accessibility settings",
            "update a supported setting",
        ],
        invariants: &[
            "settings report their effective values",
            "unsupported settings are distinguishable from disabled settings",
        ],
    },
    Definition {
        id: "power.battery",
        summary: "Read battery presence, charge, and health information.",
        operations: &["read battery status", "read battery health"],
        invariants: &[
            "charge values include units or a normalized range",
            "unknown battery data is not represented as zero",
        ],
    },
    Definition {
        id: "power.external_supply",
        summary: "Inspect external power connection and supply state.",
        operations: &["read external supply state", "subscribe to supply changes"],
        invariants: &[
            "connection state distinguishes connected from unknown",
            "reported supply changes identify their source",
        ],
    },
    Definition {
        id: "power.sleep_state",
        summary: "Observe system sleep and wake transitions.",
        operations: &["read sleep state", "subscribe to sleep and wake events"],
        invariants: &[
            "transition events identify the transition type",
            "sleep state is reported consistently with emitted events",
        ],
    },
    Definition {
        id: "power.idle_policy",
        summary: "Inspect and configure idle-related power policies.",
        operations: &["read idle policy", "set idle timeout"],
        invariants: &[
            "timeouts use a declared duration unit",
            "disabled timeout is distinct from an immediate timeout",
        ],
    },
    Definition {
        id: "power.thermal_state",
        summary: "Read thermal condition indicators relevant to power behavior.",
        operations: &["read thermal state", "subscribe to thermal state changes"],
        invariants: &[
            "thermal levels have an ordered documented meaning",
            "missing sensor data is represented as unknown",
        ],
    },
    Definition {
        id: "power.energy_policy",
        summary: "Inspect and select a system energy policy.",
        operations: &["list energy policies", "select an energy policy"],
        invariants: &[
            "selected policy is among the reported options",
            "policy changes expose their effective state",
        ],
    },
    Definition {
        id: "media.audio_outputs",
        summary: "Enumerate audio output endpoints and their properties.",
        operations: &["list audio outputs", "read output properties"],
        invariants: &[
            "endpoint identifiers are unique in an inventory",
            "unavailable endpoints are distinguishable from silent endpoints",
        ],
    },
    Definition {
        id: "media.audio_routes",
        summary: "Inspect and select routes for audio playback.",
        operations: &["read active audio route", "select an audio route"],
        invariants: &[
            "an active route refers to an available endpoint",
            "route changes identify the affected stream or default",
        ],
    },
    Definition {
        id: "media.volume",
        summary: "Read and adjust playback volume and mute state.",
        operations: &["read volume and mute state", "set volume or mute state"],
        invariants: &[
            "volume values use a declared range",
            "mute state is independent of the stored volume level",
        ],
    },
    Definition {
        id: "media.audio_inputs",
        summary: "Enumerate audio capture endpoints and their properties.",
        operations: &["list audio inputs", "read input properties"],
        invariants: &[
            "endpoint identifiers are unique in an inventory",
            "capture availability is distinct from permission state",
        ],
    },
    Definition {
        id: "media.playback_sessions",
        summary: "Observe media playback sessions and their transport state.",
        operations: &["list playback sessions", "read or update transport state"],
        invariants: &[
            "session identifiers are unique while active",
            "transport state uses a documented finite set of values",
        ],
    },
    Definition {
        id: "media.capture_sessions",
        summary: "Inspect and control media capture sessions.",
        operations: &["list capture sessions", "start or stop a capture session"],
        invariants: &[
            "session state distinguishes pending from active",
            "stopping a session finalizes its captured data consistently",
        ],
    },
];
