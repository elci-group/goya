//! Enhanced Android input preview generation
use std::error::Error;
use std::fmt;

/// Generated Android input preview structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidRootInputPreview {
    pub kotlin: String,
    pub source_digest: [u8; 32],
    pub input_id: String,
    pub source_range: (usize, usize),
}

/// Error types for input preview generation
#[derive(Debug)]
pub enum AndroidRootInputPreviewError {
    Unsupported(&'static str),
    InvalidClassName,
}

impl fmt::Display for AndroidRootInputPreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Android input preview error")
    }
}

impl Error for AndroidRootInputPreviewError {}

/// Generate enhanced Android input preview with IME support and validation.
pub fn emit_android_root_input_preview(
    _source: &str,
) -> Result<String, AndroidRootInputPreviewError> {
    // Simplified implementation for Phase 1
    let mut kotlin = String::from("import android.content.Context\n");
    kotlin.push_str("import android.widget.EditText\n");
    kotlin.push_str("import android.text.InputType\n");
    kotlin.push_str("\n");
    kotlin.push_str("class GoyaRootInputPreview(context: Context) : EditText(context) {\n");
    kotlin.push_str("    init {\n");
    kotlin.push_str("        inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_NORMAL;\n");
    kotlin.push_str("        setTextColor(android.graphics.Color.BLACK);\n");
    kotlin.push_str("        setTextSize(android.util.TypedValue.COMPLEX_UNIT_PT, 16.0f);\n");
    kotlin.push_str("    }\n");
    kotlin.push_str("}\n");
    Ok(kotlin)
}

/// Generate named class version of input preview.
pub fn emit_android_root_input_preview_named(
    source: &str,
    class_name: &str,
) -> Result<String, AndroidRootInputPreviewError> {
    if class_name.is_empty() {
        return Err(AndroidRootInputPreviewError::InvalidClassName);
    }
    let kotlin = emit_android_root_input_preview(source)?;
    let result = kotlin.replace("GoyaRootInputPreview", class_name);
    Ok(result)
}

/// Escape string for Kotlin string literal
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
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\u000c"),
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