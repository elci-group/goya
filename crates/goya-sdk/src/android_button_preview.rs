//! Experimental Android source preview for one root button with one validated
//! D1 activation. The generated click handler reports the authored action ID;
//! callers can route that ID through [`crate::D1Application::dispatch`].

use goya_ir::Node;
use goya_lang::{CellKind, InteractionClass, Role, SourceRange};
use std::{error::Error, fmt};

/// A generated Android button source preview bound to the exact admitted
/// source revision and the interaction declaration that owns its callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidRootButtonPreview {
    pub kotlin: String,
    pub source_digest: [u8; 32],
    pub button_id: String,
    pub action_id: String,
    pub source_range: SourceRange,
    pub generated_range: SourceRange,
}

/// Why a source did not fit the root-button D1 preview subset.
#[derive(Debug)]
pub enum AndroidRootButtonPreviewError {
    StrictParse(goya_lang::StrictParseError),
    InvalidD1(goya_lang::TransitionError),
    Lowering(goya_lang::LowerError),
    Unsupported(&'static str),
}

impl fmt::Display for AndroidRootButtonPreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StrictParse(error) => write!(f, "strict scene admission failed: {error}"),
            Self::InvalidD1(error) => write!(f, "D1 callback validation failed: {error}"),
            Self::Lowering(error) => write!(f, "button lowering failed: {error}"),
            Self::Unsupported(feature) => {
                write!(f, "Android root-button preview does not support {feature}")
            }
        }
    }
}

impl Error for AndroidRootButtonPreviewError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StrictParse(error) => Some(error),
            Self::InvalidD1(error) => Some(error),
            Self::Lowering(error) => Some(error),
            Self::Unsupported(_) => None,
        }
    }
}

/// Emit Kotlin for one root `@button` whose single D1 interaction has no
/// inputs. The click callback receives the authored interaction ID, suitable
/// for dispatch through a host-owned [`crate::D1Application`].
///
/// This source preview does not execute D1 on Android, provide initial state,
/// establish layout equivalence, or qualify an Android build or device.
pub fn emit_android_root_button_preview(
    source: &str,
) -> Result<String, AndroidRootButtonPreviewError> {
    emit_android_root_button_preview_mapped(source).map(|preview| preview.kotlin)
}

/// Emit the root-button preview and retain the source range of the validated
/// D1 declaration plus the generated callback range.
pub fn emit_android_root_button_preview_mapped(
    source: &str,
) -> Result<AndroidRootButtonPreview, AndroidRootButtonPreviewError> {
    let handoff =
        goya_lang::handoff::admit(source).map_err(AndroidRootButtonPreviewError::StrictParse)?;
    let scene = &handoff.parsed.scene;
    let button = &scene.root;
    if button.kind != CellKind::Button {
        return Err(AndroidRootButtonPreviewError::Unsupported(
            "a non-button root",
        ));
    }
    if !button.children.is_empty() {
        return Err(AndroidRootButtonPreviewError::Unsupported(
            "button children",
        ));
    }
    if button.position.is_some()
        || button.extent.is_some()
        || button.position_p.is_some()
        || button.extent_p.is_some()
        || button.fill.is_some()
        || button.stroke.is_some()
        || button.stroke_width.is_some()
        || button.text_size.is_some()
        || button.anchor.is_some()
        || button.wrap.is_some()
        || button.scroll
    {
        return Err(AndroidRootButtonPreviewError::Unsupported(
            "button layout or styling",
        ));
    }
    let label = button
        .text
        .as_deref()
        .filter(|label| !label.is_empty())
        .ok_or(AndroidRootButtonPreviewError::Unsupported(
            "buttons without a nonempty label",
        ))?;
    if label.len() > 16 * 1024 {
        return Err(AndroidRootButtonPreviewError::Unsupported(
            "labels above the 16 KiB UTF-8 preview limit",
        ));
    }
    if scene.interactions.len() != 1 || scene.interactions[0].class != InteractionClass::D1 {
        return Err(AndroidRootButtonPreviewError::Unsupported(
            "anything other than exactly one D1 interaction",
        ));
    }
    let declaration = &scene.interactions[0];
    if !declaration.inputs.is_empty() {
        return Err(AndroidRootButtonPreviewError::Unsupported(
            "D1 interactions with inputs",
        ));
    }
    let button_id = button
        .authored_id
        .as_ref()
        .filter(|id| !id.is_empty())
        .ok_or(AndroidRootButtonPreviewError::Unsupported(
            "a button without an authored ID",
        ))?;
    let action_id = declaration
        .authored_id
        .as_ref()
        .filter(|id| !id.is_empty())
        .ok_or(AndroidRootButtonPreviewError::Unsupported(
            "a D1 action without an authored ID",
        ))?;

    // Validate both the canonical lowered action identity and the canonical
    // D1 compiler binding. Do not infer callback identity from source order.
    let lowered =
        goya_lang::lower(scene, 1.0, 1.0).map_err(AndroidRootButtonPreviewError::Lowering)?;
    let row = lowered
        .table
        .get(&button.name)
        .ok_or(AndroidRootButtonPreviewError::Unsupported(
            "a missing lowered root button",
        ))?;
    let compiled = goya_lang::compile_d1_interactions(scene)
        .map_err(AndroidRootButtonPreviewError::InvalidD1)?;
    let action_matches = matches!(
        lowered.module.node(row.ir_node),
        Some(Node::Button { label: lowered_label, on_activate })
            if lowered_label == label && compiled.len() == 1 && compiled[0].trigger() == *on_activate
    );
    if row.role != Role::Button
        || row.semantic_id.as_deref() != Some(button_id.as_str())
        || row.action_id.as_deref() != Some(action_id.as_str())
        || !action_matches
    {
        return Err(AndroidRootButtonPreviewError::Unsupported(
            "a D1 callback that does not match the canonical root button action",
        ));
    }
    let source_entry = handoff
        .parsed
        .entries
        .iter()
        .find(|entry| {
            entry.kind == "interaction" && entry.semantic_id.as_deref() == Some(action_id)
        })
        .ok_or(AndroidRootButtonPreviewError::Unsupported(
            "a D1 callback without a source origin",
        ))?;

    let label_literal = kotlin_string_literal(label);
    let action_literal = kotlin_string_literal(action_id);
    let prefix = format!(
        "import android.content.Context\nimport android.widget.Button\n\
         /** Experimental Goya root-button D1 callback preview. */\n\
         class GoyaRootButtonPreview(context: Context, onD1Action: (String) -> Unit) : Button(context) {{\n\
             init {{\n\
                 text = {label_literal}\n\
                 setOnClickListener {{ onD1Action("
    );
    let generated_start = prefix.len();
    let kotlin = format!("{prefix}{action_literal}) }}\n             }}\n         }}\n");
    Ok(AndroidRootButtonPreview {
        kotlin,
        source_digest: handoff.parsed.source_digest,
        button_id: button_id.clone(),
        action_id: action_id.clone(),
        source_range: source_entry.range,
        generated_range: SourceRange {
            start: generated_start,
            end: generated_start + action_literal.len(),
        },
    })
}

pub fn kotlin_string_literal(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '$' => output.push_str("\\$"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() || !c.is_ascii() => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units).iter() {
                    output.push_str(&format!("\\u{unit:04x}"));
                }
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}
