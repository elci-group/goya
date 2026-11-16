//! Enhanced Android scrollable preview generation
use std::error::Error;
use std::fmt;

/// Generated Android scrollable preview structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidScrollablePreview {
    pub kotlin: String,
    pub source_digest: [u8; 32],
    pub viewport_id: String,
    pub content_id: String,
    pub source_range: (usize, usize),
}

/// Error types for scrollable preview generation
#[derive(Debug)]
pub enum AndroidScrollablePreviewError {
    Unsupported(&'static str),
    InvalidClassName,
}

impl fmt::Display for AndroidScrollablePreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Android scrollable preview error")
    }
}

impl Error for AndroidScrollablePreviewError {}

/// Generate enhanced Android scrollable preview with viewport management.
pub fn emit_android_scrollable_preview(
    source: &str,
) -> Result<String, AndroidScrollablePreviewError> {
    emit_android_scrollable_preview_named(source, "GoyaScrollablePreview")
}

/// Generate named class version of scrollable preview.
pub fn emit_android_scrollable_preview_named(
    _source: &str,
    class_name: &str,
) -> Result<String, AndroidScrollablePreviewError> {
    if class_name.is_empty() {
        return Err(AndroidScrollablePreviewError::InvalidClassName);
    }
    // Simplified implementation for Phase 1
    let mut kotlin = String::from("import android.content.Context\n");
    kotlin.push_str("import android.widget.ScrollView\n");
    kotlin.push_str("import android.widget.LinearLayout\n");
    kotlin.push_str("import android.view.ViewGroup\n");
    kotlin.push_str("import android.graphics.Color\n");
    kotlin.push_str("\n");
    kotlin.push_str("class GoyaScrollablePreview(context: Context) : ScrollView(context) {\n");
    kotlin.push_str("    private val contentLayout: LinearLayout\n");
    kotlin.push_str("    \n");
    kotlin.push_str("    init {\n");
    kotlin.push_str("        layoutParams = ViewGroup.LayoutParams(\n");
    kotlin.push_str("            ViewGroup.LayoutParams.MATCH_PARENT,\n");
    kotlin.push_str("            ViewGroup.LayoutParams.MATCH_PARENT\n");
    kotlin.push_str("        );\n");
    kotlin.push_str("        setFillViewport(true);\n");
    kotlin.push_str("        setOnScrollChangeListener(this);\n");
    kotlin.push_str("        contentLayout = LinearLayout(context).apply {\n");
    kotlin.push_str("            layoutParams = ViewGroup.LayoutParams(\n");
    kotlin.push_str("                ViewGroup.LayoutParams.MATCH_PARENT,\n");
    kotlin.push_str("                ViewGroup.LayoutParams.WRAP_CONTENT\n");
    kotlin.push_str("            );\n");
    kotlin.push_str("            orientation = LinearLayout.VERTICAL;\n");
    kotlin.push_str("            setPadding(16, 16, 16, 16);\n");
    kotlin.push_str("            setBackgroundColor(Color.WHITE);\n");
    kotlin.push_str("        };\n");
    kotlin.push_str("        addView(contentLayout);\n");
    kotlin.push_str("    }\n");
    kotlin.push_str("}\n");
    let result = kotlin.replace("GoyaScrollablePreview", class_name);
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