//! Sash Intermediate Representation (IR) v1
//!
//! This module defines a versioned IR that separates admission from codegen.
//! The IR is profile-agnostic and can represent both static and interactive windows.

use serde::{Deserialize, Serialize};

/// IR schema version
pub const IR_VERSION: &str = "1";

/// Top-level IR document
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrDocument {
    /// IR schema version
    pub version: String,
    /// Profile this IR was lowered from (e.g., "sash.win32.static-window/0.0.3")
    pub profile: String,
    /// Window definition
    pub window: Window,
    /// Controls placed in the window
    pub controls: Vec<Control>,
    /// Optional transition logic (for interactive profiles)
    pub transition: Option<Transition>,
    /// Optional gesture policy (for interactive profiles)
    pub gesture_policy: Option<GesturePolicy>,
    /// Window identity information
    pub identity: Identity,
    /// Build resources
    pub resources: Resources,
    /// Source provenance information
    pub provenance: Provenance,
}

/// Window definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    /// Scene name (window title)
    pub scene_name: String,
    /// Root semantic ID
    pub semantic_id: String,
    /// Client width in device-independent pixels
    pub width_dip: i32,
    /// Client height in device-independent pixels
    pub height_dip: i32,
    /// Layout width in millipoints
    pub width_mp: i64,
    /// Layout height in millipoints
    pub height_mp: i64,
}

/// A control placed in the window
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Control {
    /// Unique index in the placement array
    pub index: usize,
    /// Control role
    pub role: ControlRole,
    /// Native control ID
    pub control_id: u16,
    /// Structural name from scene
    pub name: String,
    /// Semantic ID from scene
    pub semantic_id: String,
    /// Display text
    pub text: String,
    /// X position in DIPs
    pub x_dip: i32,
    /// Y position in DIPs
    pub y_dip: i32,
    /// Width in DIPs
    pub width_dip: i32,
    /// Height in DIPs
    pub height_dip: i32,
    /// Source range for text property
    pub text_range: SourceRange,
    /// Source range for id property
    pub id_range: SourceRange,
    /// Source range for extent property
    pub extent_range: SourceRange,
}

/// Control role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlRole {
    Static,
    Edit,
    Button,
}

/// Transition logic for interactive windows
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    /// Action ID
    pub action_id: String,
    /// Action name
    pub action_name: String,
    /// Authored origin of the interaction declaration. The evidence source
    /// map names this range for the `interaction` property.
    pub action_range: SourceRange,
    /// State key being read/written
    pub state_key: String,
    /// Input key being read
    pub input_key: String,
    /// Initial state value
    pub initial_state: i64,
    /// Initial input value
    pub initial_input: i64,
    /// State index in controls array
    pub state_index: usize,
    /// Input index in controls array
    pub input_index: usize,
    /// Button index in controls array
    pub button_index: usize,
    /// Button control ID
    pub button_id: u16,
    /// Expression tree
    pub expr: IntExpr,
}

/// Integer expression for transitions
///
/// Externally tagged on purpose: serde rejects `tag = "..."` on tuple
/// variants such as [`IntExpr::Add`], so the tag form cannot encode the
/// expression tree. No reader depends on a tagged representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntExpr {
    State,
    Input,
    Literal(i64),
    Add(Box<IntExpr>, Box<IntExpr>),
}

/// Gesture policy for editable controls
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GesturePolicy {
    /// Copy allowed
    pub copy_allowed: bool,
    /// Paste allowed
    pub paste_allowed: bool,
    /// Cut mode
    pub cut_mode: CutMode,
}

/// Cut mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CutMode {
    Default,
    Allow,
    Deny,
}

/// Window identity information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Root semantic ID (UTF-8 bytes)
    pub root_id_utf8: Vec<u8>,
    /// Root ID length
    pub root_id_len: usize,
    /// Title (UTF-8 bytes)
    pub title_utf8: Vec<u8>,
    /// Title length
    pub title_len: usize,
}

/// Build resources
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resources {
    /// Cargo.toml content
    pub cargo_toml: String,
    /// build.rs content
    pub build_rs: String,
    /// app.manifest content
    pub manifest_xml: String,
    /// app.rc content
    pub resource_script: String,
}

/// Source provenance for traceability
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Source digest (SHA-256)
    pub source_digest: String,
    /// Ontology source range
    pub ontology_range: SourceRange,
    /// Root ID source range
    pub id_range: SourceRange,
    /// Root extent source range
    pub extent_range: SourceRange,
    /// Control origins
    pub control_origins: Vec<ControlOrigin>,
}

/// Per-control origin information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlOrigin {
    /// Control index
    pub index: usize,
    /// Text source range
    pub text_range: SourceRange,
    /// ID source range
    pub id_range: SourceRange,
    /// Extent source range
    pub extent_range: SourceRange,
}

/// Source range (half-open UTF-8 byte offsets).
///
/// This is a structural duplicate of `goya_lang::SourceRange`
/// (`crates/goya-lang/src/parser.rs`). `sash-ir` intentionally does not
/// depend on `goya-lang`, so the two types must be kept in sync by hand:
/// any change to field names, types, or ordering on one side requires the
/// matching change here. Layout parity is checked by the
/// `source_range_layout_parity` test in `crates/sash/src/ir_lower.rs`,
/// which is the only crate that depends on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
}

impl SourceRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn empty() -> Self {
        Self { start: 0, end: 0 }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// One document per profile shape, built by hand so the round-trip does
    /// not depend on the sash crate.
    fn static_document() -> IrDocument {
        IrDocument {
            version: IR_VERSION.to_string(),
            profile: "sash.win32.static-window/0.0.3".to_string(),
            window: Window {
                scene_name: "hello".to_string(),
                semantic_id: "window.root".to_string(),
                width_dip: 320,
                height_dip: 160,
                width_mp: 80_000,
                height_mp: 40_000,
            },
            controls: vec![Control {
                index: 0,
                role: ControlRole::Static,
                control_id: 1,
                name: "root".to_string(),
                semantic_id: "window.root".to_string(),
                text: "Hello".to_string(),
                x_dip: 0,
                y_dip: 0,
                width_dip: 320,
                height_dip: 160,
                text_range: SourceRange::new(40, 46),
                id_range: SourceRange::new(12, 25),
                extent_range: SourceRange::new(52, 68),
            }],
            transition: None,
            gesture_policy: None,
            identity: Identity {
                root_id_utf8: b"window.root".to_vec(),
                root_id_len: 11,
                title_utf8: b"hello".to_vec(),
                title_len: 5,
            },
            resources: Resources {
                cargo_toml: "[package]\nname = \"app\"\n".to_string(),
                build_rs: "fn main() {}\n".to_string(),
                manifest_xml: "<assembly/>".to_string(),
                resource_script: "1 24 \"app.manifest\"\n".to_string(),
            },
            provenance: Provenance {
                source_digest: "ab".repeat(32),
                ontology_range: SourceRange::new(0, 17),
                id_range: SourceRange::new(12, 25),
                extent_range: SourceRange::new(52, 68),
                control_origins: vec![ControlOrigin {
                    index: 0,
                    text_range: SourceRange::new(40, 46),
                    id_range: SourceRange::new(12, 25),
                    extent_range: SourceRange::new(52, 68),
                }],
            },
        }
    }

    fn interactive_document() -> IrDocument {
        let mut document = static_document();
        document.profile = "sash.win32.interactive/0.0.2".to_string();
        document.controls[0].role = ControlRole::Edit;
        document.transition = Some(Transition {
            action_id: "action.add".to_string(),
            action_name: "add".to_string(),
            action_range: SourceRange::new(300, 420),
            state_key: "count".to_string(),
            input_key: "amount".to_string(),
            initial_state: 0,
            initial_input: 1,
            state_index: 3,
            input_index: 1,
            button_index: 2,
            button_id: 301,
            expr: IntExpr::Add(
                Box::new(IntExpr::State),
                Box::new(IntExpr::Input),
            ),
        });
        document.gesture_policy = Some(GesturePolicy {
            copy_allowed: true,
            paste_allowed: false,
            cut_mode: CutMode::Default,
        });
        document
    }

    #[test]
    fn json_round_trip_is_lossless() {
        for document in [static_document(), interactive_document()] {
            let json = serde_json::to_string(&document).unwrap();
            let back: IrDocument = serde_json::from_str(&json).unwrap();
            assert_eq!(back, document);
        }
    }

    #[test]
    fn json_names_version_profile_and_roles() {
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&static_document()).unwrap()).unwrap();
        assert_eq!(json["version"], "1");
        assert_eq!(json["profile"], "sash.win32.static-window/0.0.3");
        assert_eq!(json["controls"][0]["role"], "static");
        assert_eq!(json["transition"], serde_json::Value::Null);
        assert_eq!(json["window"]["width_dip"], 320);

        let json: serde_json::Value = serde_json::from_str(
            &serde_json::to_string(&interactive_document()).unwrap(),
        )
        .unwrap();
        assert_eq!(json["controls"][0]["role"], "edit");
        assert_eq!(json["gesture_policy"]["cut_mode"], "default");
        // Externally tagged, snake_case: unit variants are bare strings and
        // tuple variants are one-key objects.
        assert_eq!(
            json["transition"]["expr"],
            serde_json::json!({ "add": ["state", "input"] })
        );
    }

    #[test]
    fn cut_mode_discriminants_match_the_emitted_constants() {
        // The generated program checks CUT_MODE against the interactive_clipboard
        // constants: default 0, allow 1, deny 2.
        assert_eq!(CutMode::Default as u8, 0);
        assert_eq!(CutMode::Allow as u8, 1);
        assert_eq!(CutMode::Deny as u8, 2);
    }
}
