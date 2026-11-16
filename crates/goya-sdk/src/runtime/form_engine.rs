//! Phase 3: Advanced Form Engine
//!
//! This module provides a comprehensive form generation and processing system
//! that leverages the enhanced parser capabilities from Phase 2, including
//! rich input types, advanced validation, accessibility, and internationalization.

use crate::{AndroidRootInputPreview, AndroidRootInputPreviewError, AndroidScrollablePreview, AndroidScrollablePreviewError, kotlin_string_literal as crate_kotlin_string_literal};
use goya_lang::{
    InputType, RichTextFormatting, ValidationRule, InputMasking, AccessibilityConfig,
    LocalizationConfig, ErrorSeverity, InputFormatting, AutocompleteConfig, DragDropConfig,
    CopyPasteConfig, InputHistoryConfig, WCAGCompliance, ScreenReaderConfig, HighContrastConfig,
    RtlConfig, UnicodeSupportConfig, SceneCell, SourceRange,
};
use std::collections::HashMap;
use regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Advanced form field configuration with all Phase 2 enhanced properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFieldConfig {
    pub name: String,
    pub field_type: InputType,
    pub label: Option<String>,
    pub placeholder: Option<String>,
    pub rich_text_formatting: Option<RichTextFormatting>,
    pub validation_rules: Vec<ValidationRule>,
    pub masking: Option<InputMasking>,
    pub accessibility: Option<AccessibilityConfig>,
    pub localization: Option<LocalizationConfig>,
    pub error_severity: Option<ErrorSeverity>,
    pub formatting: Option<InputFormatting>,
    pub autocomplete: Option<AutocompleteConfig>,
    pub drag_drop: Option<DragDropConfig>,
    pub copy_paste: Option<CopyPasteConfig>,
    pub input_history: Option<InputHistoryConfig>,
    pub wcag_compliance: Option<WCAGCompliance>,
    pub screen_reader: Option<ScreenReaderConfig>,
    pub high_contrast: Option<HighContrastConfig>,
    pub rtl_support: Option<RtlConfig>,
    pub unicode_support: Option<UnicodeSupportConfig>,
}

/// Form generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormConfig {
    pub name: String,
    pub fields: Vec<FormFieldConfig>,
    pub scrollable: bool,
    pub submit_action: Option<String>,
    pub validation_mode: ValidationMode,
    pub accessibility: Option<FormAccessibilityConfig>,
    pub localization: Option<FormLocalizationConfig>,
}

/// Form validation modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ValidationMode {
    /// Validate on form submission only
    OnSubmit,
    /// Validate as user types
    RealTime,
    /// Validate on field blur
    OnBlur,
    /// Validate on focus change
    OnFocusChange,
}

/// Form-level accessibility configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormAccessibilityConfig {
    pub screen_reader_announcements: Vec<String>,
    pub focus_management: bool,
    pub live_region_updates: bool,
    pub keyboard_navigation: bool,
}

/// Form-level localization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormLocalizationConfig {
    pub default_language: String,
    pub supported_languages: Vec<String>,
    pub rtl_support: bool,
    pub number_format: Option<String>,
    pub date_format: Option<String>,
}

/// Form generation result with multiple output formats
#[derive(Debug, Clone)]
pub struct FormGenerationResult {
    pub kotlin_code: String,
    pub scrollable_kotlin_code: Option<String>,
    pub d1_runtime: Option<String>,
    pub field_mappings: HashMap<String, FormFieldMapping>,
    pub source_map: FormSourceMap,
}

/// Individual field mapping from source to generated code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFieldMapping {
    pub field_name: String,
    pub source_range: goya_lang::SourceRange,
    pub generated_ranges: Vec<goya_lang::SourceRange>,
    pub validation_rules: Vec<String>,
    pub accessibility_features: Vec<String>,
}

/// Source map for form generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSourceMap {
    pub fields: Vec<FormFieldMapping>,
    pub form_metadata: FormMetadata,
}

/// Form metadata for source mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormMetadata {
    pub form_name: String,
    pub source_digest: [u8; 32],
    pub generation_timestamp: u64,
    pub parser_version: String,
}

/// Validation result for individual field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValidationResult {
    pub field_name: String,
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

/// Individual validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub severity: ErrorSeverity,
    pub field_name: String,
}

/// Individual validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub field_name: String,
}

/// Form validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormValidationResult {
    pub is_valid: bool,
    pub field_results: Vec<FieldValidationResult>,
    pub cross_field_errors: Vec<CrossFieldValidationError>,
}

/// Cross-field validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossFieldValidationError {
    pub field_names: Vec<String>,
    pub code: String,
    pub message: String,
    pub severity: ErrorSeverity,
}

/// Advanced Form Engine - core of Phase 3
pub struct FormEngine {
    config: FormConfig,
    field_configs: HashMap<String, FormFieldConfig>,
}

impl FormEngine {
    /// Create a new form engine from configuration
    pub fn new(config: FormConfig) -> Result<Self, FormEngineError> {
        let mut field_configs = HashMap::new();
        for field in &config.fields {
            if field_configs.contains_key(&field.name) {
                return Err(FormEngineError::DuplicateField(field.name.clone()));
            }
            field_configs.insert(field.name.clone(), field.clone());
        }

        Ok(Self {
            config,
            field_configs,
        })
    }

    /// Generate complete form implementation
    pub fn generate(&self) -> Result<FormGenerationResult, FormEngineError> {
        let mut kotlin_parts = Vec::new();
        let mut field_mappings = HashMap::new();
        let mut source_ranges = Vec::new();

        // Generate imports
        kotlin_parts.push(self.generate_imports());

        // Generate form class
        kotlin_parts.push(self.generate_form_class());

        // Generate field implementations
        for field_config in &self.config.fields {
            let field_result = self.generate_field(field_config)?;
            kotlin_parts.push(field_result.kotlin_code);
            field_mappings.insert(field_config.name.clone(), field_result.mapping);
            source_ranges.push(field_result.source_range);
        }

        // Generate validation logic
        kotlin_parts.push(self.generate_validation_logic());

        // Generate accessibility features
        if let Some(accessibility) = &self.config.accessibility {
            kotlin_parts.push(self.generate_accessibility_features(accessibility));
        }

        // Generate localization support
        if let Some(localization) = &self.config.localization {
            kotlin_parts.push(self.generate_localization_features(localization));
        }

        // Generate scrollable version if needed
        let scrollable_kotlin_code = if self.config.scrollable {
            Some(self.generate_scrollable_version()?)
        } else {
            None
        };

        // Generate D1 runtime if submit action exists
        let d1_runtime = if let Some(submit_action) = &self.config.submit_action {
            Some(crate::emit_android_d1_runtime_with_form(submit_action).map_err(|e| {
                FormEngineError::GenerationFailed(e.to_string())
            })?)
        } else {
            None
        };

        let form_source_map = FormSourceMap {
            fields: field_mappings.values().cloned().collect(),
            form_metadata: FormMetadata {
                form_name: self.config.name.clone(),
                source_digest: [0u8; 32], // Would be computed from actual source
                generation_timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                parser_version: "Phase3-Enhanced".to_string(),
            },
        };

        Ok(FormGenerationResult {
            kotlin_code: kotlin_parts.join("\n\n"),
            scrollable_kotlin_code,
            d1_runtime,
            field_mappings,
            source_map: form_source_map,
        })
    }

    /// Generate Android input preview for individual field
    pub fn generate_field_preview(
        &self,
        field_name: &str,
    ) -> Result<AndroidRootInputPreview, AndroidRootInputPreviewError> {
        let field_config = self.field_configs.get(field_name)
            .ok_or(AndroidRootInputPreviewError::Unsupported("Field not found"))?;

        let source = self.field_to_gscene(field_config);
        let kotlin = crate::emit_android_root_input_preview(&source)
            .map_err(|_| AndroidRootInputPreviewError::Unsupported("Failed to generate preview"))?;
        
        // Use a hash of the source as the source digest (simplified)
        use sha2::{Digest, Sha256};
        let source_digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();
        
        Ok(AndroidRootInputPreview {
            kotlin,
            source_digest,
            input_id: field_name.to_string(),
            source_range: (0, 0), // Simplified
        })
    }

    /// Generate scrollable preview for entire form
    pub fn generate_scrollable_preview(&self) -> Result<AndroidScrollablePreview, AndroidScrollablePreviewError> {
        let source = self.form_to_gscene();
        let kotlin = crate::emit_android_scrollable_preview(&source).map_err(|_| {
            AndroidScrollablePreviewError::Unsupported("Failed to generate scrollable preview")
        })?;

        // Use a hash of the source as the source digest (simplified)
        use sha2::{Digest, Sha256};
        let source_digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();

        Ok(AndroidScrollablePreview {
            kotlin,
            source_digest,
            viewport_id: "viewport".to_string(),
            content_id: "content".to_string(),
            source_range: (0, 0),
        })
    }

    /// Validate form data against configuration
    pub fn validate(&self, form_data: &HashMap<String, String>) -> FormValidationResult {
        let mut field_results = Vec::new();

        // Validate individual fields
        for field_config in &self.config.fields {
            let empty_string = String::new();
            let field_data = form_data.get(&field_config.name).unwrap_or(&empty_string);
            let result = self.validate_field(field_config, field_data);
            field_results.push(result);
        }

        // Cross-field validation
        let cross_field_errors = self.validate_cross_field(form_data);

        let is_valid = field_results.iter().all(|r| r.is_valid) && cross_field_errors.is_empty();

        FormValidationResult {
            is_valid,
            field_results,
            cross_field_errors,
        }
    }

    // Private generation methods

    fn generate_imports(&self) -> String {
        let mut imports = String::from(
            "import android.content.Context\n\
            import android.widget.*\n\
            import android.view.*\n\
            import android.text.*\n\
            import android.text.method.*\n\
            import android.text.style.*\n\
            import androidx.core.view.*\n\
            import androidx.core.widget.*\n"
        );

        // Add accessibility imports if needed
        if self.config.accessibility.is_some() {
            imports.push_str(
                "import android.accessibilityservice.*\n\
                import android.view.accessibility.*\n"
            );
        }

        // Add localization imports if needed
        if self.config.localization.is_some() {
            imports.push_str(
                "import java.util.Locale\n\
                import android.icu.text.*\n\
                import android.icu.util.*\n"
            );
        }

        imports
    }

    fn generate_form_class(&self) -> String {
        format!(
            "/** Generated advanced form: {} */\n\
            class {}(context: Context) : LinearLayout(context) {{\n\
                private val fields: MutableMap<String, View> = mutableMapOf()\n\
                private val fieldConfigs: Map<String, FormFieldConfig> = mapOf(\n\
                    {}\n\
                )\n\n\
                init {{\n\
                    orientation = VERTICAL\n\
                    setPadding(16, 16, 16, 16)\n\
                    setupFields()\n\
                }}\n\
                private fun setupFields() {{\n\
                    {}\n\
                }}\n",
            self.config.name,
            self.config.name,
            self.config.fields.iter()
                .map(|f| format!("\"{}\" to FormFieldConfig(...)", f.name))
                .collect::<Vec<_>>()
                .join(",\n"),
            self.config.fields.iter()
                .map(|f| format!("createField(\"{}\")", f.name))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    fn generate_field(&self, config: &FormFieldConfig) -> Result<FieldGenerationResult, FormEngineError> {
        let mut kotlin = String::new();
        let mut generated_ranges: Vec<goya_lang::SourceRange> = Vec::new();
        let start_offset = 0; // Would be computed from actual generated code

        // Generate field creation method
        kotlin.push_str(&format!(
            "    private fun createField(\"{}\"): View {{\n",
            config.name
        ));

        // Generate based on field type
        match config.field_type {
            InputType::Text | InputType::Email | InputType::Password => {
                kotlin.push_str(&self.generate_edit_text_field(config)?);
            }
            InputType::Number => {
                kotlin.push_str(&self.generate_number_field(config)?);
            }
            InputType::Date | InputType::DateTime | InputType::Time => {
                kotlin.push_str(&self.generate_date_time_field(config)?);
            }
            InputType::Search => {
                kotlin.push_str(&self.generate_search_field(config)?);
            }
            InputType::File => {
                kotlin.push_str(&self.generate_file_field(config)?);
            }
            _ => {
                kotlin.push_str(&self.generate_edit_text_field(config)?);
            }
        }

        kotlin.push_str("    }\n");

        let mapping = FormFieldMapping {
            field_name: config.name.clone(),
            source_range: goya_lang::SourceRange { start: 0, end: 0 }, // Would be from parser
            generated_ranges: vec![goya_lang::SourceRange {
                start: start_offset,
                end: start_offset + kotlin.len(),
            }],
            validation_rules: config.validation_rules.iter().map(|r| format!("{:?}", r)).collect(),
            accessibility_features: config.accessibility.as_ref()
                .map(|a| vec![format!("{:?}", a)])
                .unwrap_or_default(),
        };

        let kotlin_len = kotlin.len();
        Ok(FieldGenerationResult {
            kotlin_code: kotlin,
            mapping,
            source_range: goya_lang::SourceRange { start: 0, end: kotlin_len },
        })
    }

    fn generate_edit_text_field(&self, config: &FormFieldConfig) -> Result<String, FormEngineError> {
        let mut code = String::new();

        code.push_str(&format!(
            "        val editText = EditText(context).apply {{\n"
        ));

        // Set input type
        let input_type = match config.field_type {
            InputType::Password => "InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD",
            InputType::Email => "InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS",
            InputType::Number => "InputType.TYPE_CLASS_NUMBER",
            _ => "InputType.TYPE_CLASS_TEXT",
        };
        code.push_str(&format!("            inputType = {}\n", input_type));

        // Set placeholder
        if let Some(placeholder) = &config.placeholder {
            code.push_str(&format!("            hint = \"{}\"\n", crate_kotlin_string_literal(placeholder)));
        }

        // Set max length
        if let Some(ValidationRule::MaxLength(len)) = config.validation_rules.iter()
            .find(|r| matches!(r, ValidationRule::MaxLength(_)))
        {
            code.push_str(&format!("            setFilters(arrayOf(InputFilter.LengthFilter({})))\n", len));
        }

        // Apply rich text formatting
        if let Some(formatting) = &config.rich_text_formatting {
            code.push_str(&self.generate_rich_text_formatting(formatting));
        }

        // Apply masking
        if let Some(masking) = &config.masking {
            code.push_str(&self.generate_input_masking(masking));
        }

        // Apply formatting
        if let Some(formatting) = &config.formatting {
            code.push_str(&self.generate_input_formatting(formatting));
        }

        // Add text change listener for real-time validation
        if self.config.validation_mode == ValidationMode::RealTime {
            code.push_str(&self.generate_real_time_validation(config));
        }

        // Add autocomplete
        if let Some(autocomplete) = &config.autocomplete {
            code.push_str(&self.generate_autocomplete(autocomplete));
        }

        // Add accessibility
        if let Some(accessibility) = &config.accessibility {
            code.push_str(&self.generate_field_accessibility(accessibility));
        }

        // Add drag-drop
        if let Some(drag_drop) = &config.drag_drop {
            code.push_str(&self.generate_drag_drop(drag_drop));
        }

        // Add copy-paste
        if let Some(copy_paste) = &config.copy_paste {
            code.push_str(&self.generate_copy_paste(copy_paste));
        }

        // Add input history
        if let Some(history) = &config.input_history {
            code.push_str(&self.generate_input_history(history));
        }

        code.push_str("        }\n        return editText\n");

        Ok(code)
    }

    fn generate_number_field(&self, config: &FormFieldConfig) -> Result<String, FormEngineError> {
        let mut code = String::new();

        code.push_str(
            "        val editText = EditText(context).apply {\n\
                inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_FLAG_DECIMAL\n"
        );

        if let Some(placeholder) = &config.placeholder {
            code.push_str(&format!("            hint = \"{}\"\n", crate_kotlin_string_literal(placeholder)));
        }

        if let Some(ValidationRule::MaxLength(len)) = config.validation_rules.iter()
            .find(|r| matches!(r, ValidationRule::MaxLength(_)))
        {
            code.push_str(&format!("            setFilters(arrayOf(InputFilter.LengthFilter({})))\n", len));
        }

        // Add min/max validation if present
        if let Some(ValidationRule::MinLength(min)) = config.validation_rules.iter()
            .find(|r| matches!(r, ValidationRule::MinLength(_)))
        {
            code.push_str(&format!("            // Min value: {}\n", min));
        }

        code.push_str("        }\n        return editText\n");

        Ok(code)
    }

    fn generate_date_time_field(&self, config: &FormFieldConfig) -> Result<String, FormEngineError> {
        let mut code = String::new();

        let input_type = match config.field_type {
            InputType::Date => "InputType.TYPE_CLASS_DATETIME or InputType.TYPE_DATETIME_VARIATION_DATE",
            InputType::Time => "InputType.TYPE_CLASS_DATETIME or InputType.TYPE_DATETIME_VARIATION_TIME",
            InputType::DateTime => "InputType.TYPE_CLASS_DATETIME",
            _ => "InputType.TYPE_CLASS_DATETIME or InputType.TYPE_DATETIME_VARIATION_NORMAL",
        };

        code.push_str(&format!(
            "        val editText = EditText(context).apply {{\n\
                inputType = {}\n",
            input_type
        ));

        if let Some(placeholder) = &config.placeholder {
            code.push_str(&format!("            hint = \"{}\"\n", crate_kotlin_string_literal(placeholder)));
        }

        // Add date/time formatting
        if let Some(formatting) = &config.formatting {
            code.push_str(&format!("            // Date format: {:?}\n", formatting.format_type));
        }

        code.push_str("        }\n        return editText\n");

        Ok(code)
    }

    fn generate_search_field(&self, config: &FormFieldConfig) -> Result<String, FormEngineError> {
        let mut code = String::new();

        code.push_str(
            "        val searchView = SearchView(context).apply {\n\
                setIconified(false)\n\
                setQueryHint(\"Search...\")\n"
        );

        if let Some(placeholder) = &config.placeholder {
            code.push_str(&format!("            queryHint = \"{}\"\n", crate_kotlin_string_literal(placeholder)));
        }

        // Add autocomplete for search
        if let Some(autocomplete) = &config.autocomplete {
            code.push_str(&self.generate_autocomplete(autocomplete));
        }

        code.push_str("        }\n        return searchView\n");

        Ok(code)
    }

    fn generate_file_field(&self, config: &FormFieldConfig) -> Result<String, FormEngineError> {
        let mut code = String::new();

        code.push_str(
            "        val fileInput = LinearLayout(context).apply {\n\
                orientation = VERTICAL\n"
        );

        if let Some(drag_drop) = &config.drag_drop {
            code.push_str(&self.generate_drag_drop(drag_drop));
        }

        code.push_str("        }\n        return fileInput\n");

        Ok(code)
    }

    fn generate_rich_text_formatting(&self, formatting: &RichTextFormatting) -> String {
        let mut code = String::new();

        if formatting.bold {
            code.push_str("            // Bold formatting\n");
            code.push_str("            setTypeface(Typeface.defaultFromStyle(Typeface.BOLD))\n");
        }
        if formatting.italic {
            code.push_str("            // Italic formatting\n");
            code.push_str("            setTypeface(Typeface.defaultFromStyle(Typeface.ITALIC))\n");
        }
        if formatting.underline {
            code.push_str("            // Underline formatting\n");
            code.push_str("            setPaintFlags(paintFlags | Paint.UNDERLINE_TEXT_FLAG)\n");
        }
        if formatting.strikethrough {
            code.push_str("            // Strikethrough formatting\n");
            code.push_str("            setPaintFlags(paintFlags | Paint.STRIKE_THRU_TEXT_FLAG)\n");
        }
        if let Some(font_size) = formatting.font_size {
            code.push_str(&format!("            setTextSize(TypedValue.COMPLEX_UNIT_PT, {:.1}f)\n", font_size));
        }
        if let Some(font_family) = &formatting.font_family {
            code.push_str(&format!("            // Font family: {}\n", font_family));
        }
        if let Some(text_align) = &formatting.text_align {
            let gravity = match text_align {
                goya_lang::TextAlign::Left => "Gravity.START",
                goya_lang::TextAlign::Center => "Gravity.CENTER_HORIZONTAL",
                goya_lang::TextAlign::Right => "Gravity.END",
                goya_lang::TextAlign::Justify => "Gravity.FILL_HORIZONTAL",
            };
            code.push_str(&format!("            gravity = {}\n", gravity));
        }
        if let Some(line_height) = formatting.line_height {
            code.push_str(&format!("            // Line height: {:.1}\n", line_height));
        }
        if let Some(letter_spacing) = formatting.letter_spacing {
            code.push_str(&format!("            letterSpacing = {:.2}f\n", letter_spacing));
        }
        if let Some(text_color) = formatting.text_color {
            let red = (text_color >> 16) & 0xff;
            let green = (text_color >> 8) & 0xff;
            let blue = text_color & 0xff;
            code.push_str(&format!(
                "            setTextColor(android.graphics.Color.rgb({}, {}, {}))\n",
                red, green, blue
            ));
        }
        if let Some(background_color) = formatting.background_color {
            let red = (background_color >> 16) & 0xff;
            let green = (background_color >> 8) & 0xff;
            let blue = background_color & 0xff;
            code.push_str(&format!(
                "            setBackgroundColor(android.graphics.Color.rgb({}, {}, {}))\n",
                red, green, blue
            ));
        }

        code
    }

    fn generate_input_masking(&self, masking: &InputMasking) -> String {
        let mut code = String::new();

        let mask_pattern = match &masking.mask_type {
            goya_lang::MaskType::Password => "•",
            goya_lang::MaskType::SSN => "XXX-XX-XXXX",
            goya_lang::MaskType::CreditCard => "#### #### #### ####",
            goya_lang::MaskType::Phone => "(###) ###-####",
            goya_lang::MaskType::Currency => "$#,##0.00",
            goya_lang::MaskType::Custom(pattern) => pattern.as_str(),
        };

        let placeholder_char = masking.placeholder_char.unwrap_or('•');

        code.push_str(&format!(
            "            // Input masking: {}\n",
            mask_pattern
        ));
        code.push_str(&format!(
            "            // Placeholder char: {}\n",
            placeholder_char
        ));
        if masking.reveal_on_focus {
            code.push_str("            // Reveal on focus enabled\n");
        }
        if masking.copy_protection {
            code.push_str("            // Copy protection enabled\n");
            code.push_str("            setCustomSelectionActionModeCallback(ActionMode.Callback { _, _ -> false })\n");
        }
        if masking.paste_protection {
            code.push_str("            // Paste protection enabled\n");
        }

        code
    }

    fn generate_input_formatting(&self, formatting: &InputFormatting) -> String {
        let mut code = String::new();

        let format_type = match &formatting.format_type {
            goya_lang::FormatType::Currency => "Currency",
            goya_lang::FormatType::Date => "Date",
            goya_lang::FormatType::Phone => "Phone",
            goya_lang::FormatType::Ssn => "SSN",
            goya_lang::FormatType::Custom(pattern) => pattern.as_str(),
        };

        code.push_str(&format!("            // Format: {}\n", format_type));

        if let Some(placeholder) = &formatting.placeholder {
            code.push_str(&format!("            // Format placeholder: {}\n", crate_kotlin_string_literal(placeholder)));
        }
        if let Some(custom_format) = &formatting.custom_format {
            code.push_str(&format!("            // Custom format: {}\n", crate_kotlin_string_literal(custom_format)));
        }

        code
    }

    fn generate_autocomplete(&self, autocomplete: &AutocompleteConfig) -> String {
        let mut code = String::new();

        if autocomplete.enabled {
            code.push_str("            // Autocomplete enabled\n");
            if !autocomplete.suggestions.is_empty() {
                code.push_str("            // Suggestions: [");
                for (i, suggestion) in autocomplete.suggestions.iter().enumerate() {
                    if i > 0 { code.push_str(", "); }
                    code.push_str(&format!("\"{}\"", crate_kotlin_string_literal(suggestion)));
                }
                code.push_str("]\n");
            }
            let source = match &autocomplete.source {
                goya_lang::AutocompleteSource::Static => "Static",
                goya_lang::AutocompleteSource::Api => "API",
                goya_lang::AutocompleteSource::LocalStorage => "LocalStorage",
                goya_lang::AutocompleteSource::Custom(s) => s.as_str(),
            };
            code.push_str(&format!("            // Autocomplete source: {}\n", source));
            code.push_str(&format!("            // Min query length: {}\n", autocomplete.min_length));
            code.push_str(&format!("            // Max suggestions: {}\n", autocomplete.max_suggestions));
        }

        code
    }

    fn generate_drag_drop(&self, drag_drop: &DragDropConfig) -> String {
        let mut code = String::new();

        if drag_drop.enabled {
            code.push_str("            // Drag & drop enabled\n");
            if drag_drop.allow_files {
                code.push_str("            // Allow file drops\n");
            }
            if drag_drop.allow_text {
                code.push_str("            // Allow text drops\n");
            }
            if drag_drop.preview {
                code.push_str("            // Show drop preview\n");
            }
        }

        code
    }

    fn generate_copy_paste(&self, copy_paste: &CopyPasteConfig) -> String {
        let mut code = String::new();

        if copy_paste.smart_copy {
            code.push_str("            // Smart copy enabled\n");
        }
        if copy_paste.smart_paste {
            code.push_str("            // Smart paste enabled\n");
        }
        if copy_paste.format_paste {
            code.push_str("            // Format paste enabled\n");
        }
        if copy_paste.preserve_formatting {
            code.push_str("            // Preserve formatting on paste\n");
        }

        code
    }

    fn generate_input_history(&self, history: &InputHistoryConfig) -> String {
        let mut code = String::new();

        if history.enabled {
            code.push_str("            // Input history enabled\n");
            code.push_str(&format!("            // Max history: {}\n", history.max_history));
            if history.auto_save {
                code.push_str("            // Auto-save enabled\n");
            }
            if history.allow_undo {
                code.push_str("            // Undo enabled\n");
            }
            if history.allow_redo {
                code.push_str("            // Redo enabled\n");
            }
        }

        code
    }

    fn generate_field_accessibility(&self, accessibility: &AccessibilityConfig) -> String {
        let mut code = String::new();

        if let Some(aria_label) = &accessibility.aria_label {
            code.push_str(&format!("            contentDescription = \"{}\"\n", crate_kotlin_string_literal(aria_label)));
        }
        if let Some(aria_describedby) = &accessibility.aria_describedby {
            code.push_str(&format!("            // aria-describedby: {}\n", crate_kotlin_string_literal(aria_describedby)));
        }
        if let Some(role) = &accessibility.role {
            code.push_str(&format!("            // ARIA role: {}\n", role));
        }
        if let Some(live_region) = &accessibility.live_region {
            let mode = match live_region {
                goya_lang::LiveRegion::PolitenessAssertive => "ViewCompat.ACCESSIBILITY_LIVE_REGION_ASSERTIVE",
                goya_lang::LiveRegion::PolitenessGentle => "ViewCompat.ACCESSIBILITY_LIVE_REGION_POLITE",
                goya_lang::LiveRegion::PolitenessOff => "ViewCompat.ACCESSIBILITY_LIVE_REGION_NONE",
            };
            code.push_str(&format!("            ViewCompat.setAccessibilityLiveRegion(this, {})\n", mode));
        }
        if accessibility.announce_value_changes {
            code.push_str("            // Announce value changes\n");
        }
        if accessibility.announce_validation_errors {
            code.push_str("            // Announce validation errors\n");
        }
        for announcement in &accessibility.screen_reader_announcements {
            code.push_str(&format!("            // Screen reader: {}\n", crate_kotlin_string_literal(announcement)));
        }

        code
    }

    fn generate_real_time_validation(&self, config: &FormFieldConfig) -> String {
        let mut code = String::new();

        code.push_str(
            "            addTextChangedListener(object : TextWatcher {\n\
                override fun afterTextChanged(s: Editable?) {\n"
        );

        for rule in &config.validation_rules {
            match rule {
                ValidationRule::MinLength(len) => {
                    code.push_str(&format!(
                        "                    if (s?.length ?: 0 < {}) {{\n\
                            setError(\"Minimum length is {}\")\n\
                        }}\n",
                        len, len
                    ));
                }
                ValidationRule::MaxLength(len) => {
                    code.push_str(&format!(
                        "                    if (s?.length ?: 0 > {}) {{\n\
                            setError(\"Maximum length is {}\")\n\
                        }}\n",
                        len, len
                    ));
                }
                ValidationRule::Pattern(pattern) => {
                    code.push_str(&format!(
                        "                    if (!s?.toString()?.matches(\"{}\".toRegex())!!) {{\n\
                            setError(\"Invalid format\")\n\
                        }}\n",
                        pattern.replace('"', "\\\"")
                    ));
                }
                ValidationRule::CustomValidation(validator) => {
                    code.push_str(&format!(
                        "                    // Custom validation: {}\n",
                        validator
                    ));
                }
                _ => {}
            }
        }

        code.push_str(
                "                }\n\
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}\n\
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}\n\
            })\n"
        );

        code
    }

    fn generate_validation_logic(&self) -> String {
        let mut code = String::new();

        code.push_str(
            "    fun validateForm(): Boolean {\n\
                var isValid = true\n"
        );

        for field_config in &self.config.fields {
            code.push_str(&format!(
                "        val {} = fields[\"{}\"] as? EditText\n",
                field_config.name, field_config.name
            ));

            for rule in &field_config.validation_rules {
                match rule {
                    ValidationRule::MinLength(len) => {
                        code.push_str(&format!(
                            "        if ({}?.text?.length ?: 0 < {}) {{\n\
                                {}?.setError(\"Minimum length is {}\")\n\
                                isValid = false\n\
                            }}\n",
                            field_config.name, len, field_config.name, len
                        ));
                    }
                    ValidationRule::MaxLength(len) => {
                        code.push_str(&format!(
                            "        if ({}?.text?.length ?: 0 > {}) {{\n\
                                {}?.setError(\"Maximum length is {}\")\n\
                                isValid = false\n\
                            }}\n",
                            field_config.name, len, field_config.name, len
                        ));
                    }
                    ValidationRule::Pattern(pattern) => {
                        code.push_str(&format!(
                            "        if (!{}?.text?.toString()?.matches(\"{}\".toRegex())!!) {{\n\
                                {}?.setError(\"Invalid format\")\n\
                                isValid = false\n\
                            }}\n",
                            field_config.name,
                            pattern.replace('"', "\\\""),
                            field_config.name
                        ));
                    }
                    _ => {}
                }
            }
        }

        code.push_str(
            "        return isValid\n\
    }\n"
        );

        // Cross-field validation
        // This would be enhanced based on CrossFieldRule configurations
        code.push_str(
            "    fun validateCrossField(): Boolean {\n\
                // Cross-field validation logic\n\
                return true\n\
    }\n"
        );

        code
    }

    fn generate_accessibility_features(&self, config: &FormAccessibilityConfig) -> String {
        let mut code = String::new();

        code.push_str(
            "    private fun setupAccessibility() {\n"
        );

        if config.focus_management {
            code.push_str("        // Focus management\n");
            code.push_str("        setFocusableInTouchMode(true)\n");
        }

        if config.live_region_updates {
            code.push_str("        // Live region updates\n");
            code.push_str("        ViewCompat.setAccessibilityLiveRegion(this, ViewCompat.ACCESSIBILITY_LIVE_REGION_POLITE)\n");
        }

        if config.keyboard_navigation {
            code.push_str("        // Keyboard navigation\n");
            code.push_str("        setOnKeyListener { _, keyCode, event ->\n");
            code.push_str("            // Handle keyboard navigation\n");
            code.push_str("            false\n");
            code.push_str("        }\n");
        }

        for announcement in &config.screen_reader_announcements {
            code.push_str(&format!(
                "        // Screen reader: {}\n",
                crate_kotlin_string_literal(announcement)
            ));
        }

        code.push_str("    }\n");

        code
    }

    fn generate_localization_features(&self, config: &FormLocalizationConfig) -> String {
        let mut code = String::new();

        code.push_str(
            "    private fun setupLocalization() {\n"
        );

        code.push_str(&format!(
            "        val locale = Locale(\"{}\")\n",
            config.default_language
        ));

        if config.rtl_support {
            code.push_str("        // RTL support\n");
            code.push_str("        layoutDirection = LAYOUT_DIRECTION_RTL\n");
        }

        if let Some(number_format) = &config.number_format {
            code.push_str(&format!("        // Number format: {}\n", number_format));
        }

        if let Some(date_format) = &config.date_format {
            code.push_str(&format!("        // Date format: {}\n", date_format));
        }

        for lang in &config.supported_languages {
            code.push_str(&format!("        // Supported language: {}\n", lang));
        }

        code.push_str("    }\n");

        code
    }

    fn generate_scrollable_version(&self) -> Result<String, FormEngineError> {
        let base_kotlin = self.generate()?.kotlin_code;

        let scrollable_kotlin = format!(
            "/** Scrollable version of {} */\n\
            class {}Scrollable(context: Context) : ScrollView(context) {{\n\
                private val formContainer = {}(context)\n\n\
                init {{\n\
                    fillViewport = true\n\
                    addView(formContainer)\n\
                }}\n\
            }}",
            self.config.name,
            self.config.name,
            self.config.name
        );

        Ok(scrollable_kotlin)
    }

    fn generate_d1_runtime(&self, submit_action: &str) -> Result<String, FormEngineError> {
        // This would generate a D1 runtime for form submission
        // For now, return a basic template
        let d1_runtime = format!(
            "/** Generated D1 runtime for form submission */\n\
            class FormSubmissionRuntime(initialState: Map<String, FormSubmissionRuntime.Value>, initialRevision: ULong = 0uL) {{\n\
                sealed class Value {{ data class IntValue(val value: Long) : Value(); data class TextValue(val value: String) : Value(); data class BoolValue(val value: Boolean) : Value() }}\n\
                enum class ErrorCode {{ UNKNOWN_ACTION, INPUT_SCHEMA, VALIDATION_FAILED }}\n\
                data class Snapshot(val revision: ULong, val state: Map<String, Value>)\n\
                sealed class DispatchResult {{ data class Success(val revision: ULong) : DispatchResult(); data class Failure(val code: ErrorCode) : DispatchResult() }}\n\n\
                private var values: Map<String, Value>\n\
                @get:Synchronized var revision: ULong = initialRevision\n                    private set\n\
                @get:Synchronized val state: Map<String, Value> get() = values.toMap()\n\
                @Synchronized fun snapshot(): Snapshot = Snapshot(revision, values.toMap())\n\n\
                init {{\n\
                    values = initialState.toMap()\n\
                }}\n\n\
                @Synchronized fun dispatch(actionId: String, inputs: Map<String, Value>): DispatchResult {{\n\
                    return when (actionId) {{\n\
                        \"{}\" -> {{\n\
                            // Validate form inputs\n\
                            // Apply form submission logic\n\
                            val next = values.toMutableMap()\n\
                            // Update submission state\n\
                            if (revision == ULong.MAX_VALUE) return DispatchResult.Failure(ErrorCode.UNKNOWN_ACTION)\n\
                            val nextRevision = revision + 1uL\n\
                            values = next.toMap()\n\
                            revision = nextRevision\n\
                            DispatchResult.Success(revision)\n\
                        }}\n\
                        else -> DispatchResult.Failure(ErrorCode.UNKNOWN_ACTION)\n\
                    }}\n\
                }}\n\
            }}",
            submit_action
        );

        Ok(d1_runtime)
    }

    fn validate_field(&self, config: &FormFieldConfig, value: &str) -> FieldValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for rule in &config.validation_rules {
            match rule {
                ValidationRule::MinLength(len) => {
                    if value.len() < *len {
                        errors.push(ValidationError {
                            code: "MIN_LENGTH".to_string(),
                            message: format!("Minimum length is {}", len),
                            severity: config.error_severity.unwrap_or(ErrorSeverity::Error),
                            field_name: config.name.clone(),
                        });
                    }
                }
                ValidationRule::MaxLength(len) => {
                    if value.len() > *len {
                        errors.push(ValidationError {
                            code: "MAX_LENGTH".to_string(),
                            message: format!("Maximum length is {}", len),
                            severity: config.error_severity.unwrap_or(ErrorSeverity::Error),
                            field_name: config.name.clone(),
                        });
                    }
                }
                ValidationRule::Pattern(pattern) => {
                    let regex = regex::Regex::new(&format!("^{}$", pattern));
                    if let Ok(regex) = regex {
                        if !regex.is_match(value) {
                            errors.push(ValidationError {
                                code: "PATTERN_MISMATCH".to_string(),
                                message: "Invalid format".to_string(),
                                severity: config.error_severity.unwrap_or(ErrorSeverity::Error),
                                field_name: config.name.clone(),
                            });
                        }
                    } else {
                        warnings.push(ValidationWarning {
                            code: "INVALID_REGEX".to_string(),
                            message: format!("Invalid regex pattern: {}", pattern),
                            field_name: config.name.clone(),
                        });
                    }
                }
                ValidationRule::CustomValidation(validator) => {
                    warnings.push(ValidationWarning {
                        code: "CUSTOM_VALIDATION".to_string(),
                        message: format!("Custom validator: {}", validator),
                        field_name: config.name.clone(),
                    });
                }
                _ => {}
            }
        }

        FieldValidationResult {
            field_name: config.name.clone(),
            is_valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    fn validate_cross_field(&self, _form_data: &HashMap<String, String>) -> Vec<CrossFieldValidationError> {
        // Cross-field validation would be implemented based on CrossFieldRule configurations
        Vec::new()
    }

    fn field_to_gscene(&self, config: &FormFieldConfig) -> String {
        // Convert field config back to .gscene format for preview generation
        let mut gscene = String::new();
        gscene.push_str(&format!("@ontology {} {{\n", self.config.name));
        gscene.push_str(&format!("  @cell {} {{\n", config.name));

        if let Some(label) = &config.label {
            gscene.push_str(&format!("    text: \"{}\"\n", label));
        }
        if let Some(placeholder) = &config.placeholder {
            gscene.push_str(&format!("    placeholder: \"{}\"\n", placeholder));
        }

        // Add input type
        let input_type_str = match config.field_type {
            InputType::Text => "text",
            InputType::Password => "password",
            InputType::Email => "email",
            InputType::Number => "number",
            InputType::Tel => "tel",
            InputType::Url => "url",
            InputType::Search => "search",
            InputType::Date => "date",
            InputType::DateTime => "datetime-local",
            InputType::Time => "time",
            InputType::File => "file",
            _ => "text",
        };
        gscene.push_str(&format!("    input-type: \"{}\"\n", input_type_str));

        gscene.push_str("  }\n");
        gscene.push_str("}\n");

        gscene
    }

    fn form_to_gscene(&self) -> String {
        let mut gscene = String::new();
        gscene.push_str(&format!("@ontology {} {{\n", self.config.name));
        gscene.push_str("  @cell root {\n");
        gscene.push_str("    scroll: true\n");

        for field_config in &self.config.fields {
            gscene.push_str(&format!("    @cell {} {{\n", field_config.name));
            if let Some(label) = &field_config.label {
                gscene.push_str(&format!("      text: \"{}\"\n", label));
            }
            if let Some(placeholder) = &field_config.placeholder {
                gscene.push_str(&format!("      placeholder: \"{}\"\n", placeholder));
            }
            gscene.push_str("    }\n");
        }

        gscene.push_str("  }\n");
        gscene.push_str("}\n");

        gscene
    }
}

/// Result of field generation
struct FieldGenerationResult {
    kotlin_code: String,
    mapping: FormFieldMapping,
    source_range: goya_lang::SourceRange,
}

/// Form engine errors
#[derive(Debug, thiserror::Error)]
pub enum FormEngineError {
    #[error("Duplicate field name: {0}")]
    DuplicateField(String),
    #[error("Field not found: {0}")]
    FieldNotFound(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Generation failed: {0}")]
    GenerationFailed(String),
}

/// High-level form builder for fluent API
pub struct FormBuilder {
    config: FormConfig,
}

impl FormBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: FormConfig {
                name: name.into(),
                fields: Vec::new(),
                scrollable: false,
                submit_action: None,
                validation_mode: ValidationMode::OnSubmit,
                accessibility: None,
                localization: None,
            },
        }
    }

    pub fn field(mut self, name: impl Into<String>) -> FieldBuilder {
        FieldBuilder {
            builder: self,
            config: FormFieldConfig {
                name: name.into(),
                field_type: InputType::Text,
                label: None,
                placeholder: None,
                rich_text_formatting: None,
                validation_rules: Vec::new(),
                masking: None,
                accessibility: None,
                localization: None,
                error_severity: None,
                formatting: None,
                autocomplete: None,
                drag_drop: None,
                copy_paste: None,
                input_history: None,
                wcag_compliance: None,
                screen_reader: None,
                high_contrast: None,
                rtl_support: None,
                unicode_support: None,
            },
        }
    }

    pub fn scrollable(mut self, scrollable: bool) -> Self {
        self.config.scrollable = scrollable;
        self
    }

    pub fn submit_action(mut self, action: impl Into<String>) -> Self {
        self.config.submit_action = Some(action.into());
        self
    }

    pub fn validation_mode(mut self, mode: ValidationMode) -> Self {
        self.config.validation_mode = mode;
        self
    }

    pub fn accessibility(mut self, config: FormAccessibilityConfig) -> Self {
        self.config.accessibility = Some(config);
        self
    }

    pub fn localization(mut self, config: FormLocalizationConfig) -> Self {
        self.config.localization = Some(config);
        self
    }

    pub fn build(self) -> Result<FormEngine, FormEngineError> {
        FormEngine::new(self.config)
    }
}

/// Fluent builder for individual field configuration
pub struct FieldBuilder {
    builder: FormBuilder,
    config: FormFieldConfig,
}

impl FieldBuilder {
    pub fn field_type(mut self, field_type: InputType) -> Self {
        self.config.field_type = field_type;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.config.placeholder = Some(placeholder.into());
        self
    }

    pub fn rich_text_formatting(mut self, formatting: RichTextFormatting) -> Self {
        self.config.rich_text_formatting = Some(formatting);
        self
    }

    pub fn validation_rule(mut self, rule: ValidationRule) -> Self {
        self.config.validation_rules.push(rule);
        self
    }

    pub fn min_length(mut self, len: usize) -> Self {
        self.config.validation_rules.push(ValidationRule::MinLength(len));
        self
    }

    pub fn max_length(mut self, len: usize) -> Self {
        self.config.validation_rules.push(ValidationRule::MaxLength(len));
        self
    }

    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.config.validation_rules.push(ValidationRule::Pattern(pattern.into()));
        self
    }

    pub fn masking(mut self, masking: InputMasking) -> Self {
        self.config.masking = Some(masking);
        self
    }

    pub fn accessibility(mut self, accessibility: AccessibilityConfig) -> Self {
        self.config.accessibility = Some(accessibility);
        self
    }

    pub fn localization(mut self, localization: LocalizationConfig) -> Self {
        self.config.localization = Some(localization);
        self
    }

    pub fn error_severity(mut self, severity: ErrorSeverity) -> Self {
        self.config.error_severity = Some(severity);
        self
    }

    pub fn formatting(mut self, formatting: InputFormatting) -> Self {
        self.config.formatting = Some(formatting);
        self
    }

    pub fn autocomplete(mut self, autocomplete: AutocompleteConfig) -> Self {
        self.config.autocomplete = Some(autocomplete);
        self
    }

    pub fn drag_drop(mut self, drag_drop: DragDropConfig) -> Self {
        self.config.drag_drop = Some(drag_drop);
        self
    }

    pub fn copy_paste(mut self, copy_paste: CopyPasteConfig) -> Self {
        self.config.copy_paste = Some(copy_paste);
        self
    }

    pub fn input_history(mut self, history: InputHistoryConfig) -> Self {
        self.config.input_history = Some(history);
        self
    }

    pub fn wcag_compliance(mut self, compliance: WCAGCompliance) -> Self {
        self.config.wcag_compliance = Some(compliance);
        self
    }

    pub fn screen_reader(mut self, reader: ScreenReaderConfig) -> Self {
        self.config.screen_reader = Some(reader);
        self
    }

    pub fn high_contrast(mut self, contrast: HighContrastConfig) -> Self {
        self.config.high_contrast = Some(contrast);
        self
    }

    pub fn rtl_support(mut self, rtl: RtlConfig) -> Self {
        self.config.rtl_support = Some(rtl);
        self
    }

    pub fn unicode_support(mut self, unicode: UnicodeSupportConfig) -> Self {
        self.config.unicode_support = Some(unicode);
        self
    }

    pub fn done(mut self) -> FormBuilder {
        self.builder.config.fields.push(self.config);
        self.builder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_form_builder_basic() {
        let engine = FormBuilder::new("LoginForm")
            .field("username")
                .field_type(InputType::Text)
                .label("Username")
                .placeholder("Enter username")
                .min_length(3)
                .max_length(20)
                .done()
            .field("password")
                .field_type(InputType::Password)
                .label("Password")
                .placeholder("Enter password")
                .min_length(8)
                .done()
            .scrollable(true)
            .submit_action("login")
            .build();

        assert!(engine.is_ok());
        let engine = engine.unwrap();
        assert_eq!(engine.config.fields.len(), 2);
        assert_eq!(engine.config.name, "LoginForm");
        assert!(engine.config.scrollable);
    }

    #[test]
    fn test_form_generation() {
        let engine = FormBuilder::new("TestForm")
            .field("email")
                .field_type(InputType::Email)
                .label("Email")
                .placeholder("Enter email")
                .pattern(r"^[^@]+@[^@]+\.[^@]+$")
                .done()
            .build()
            .unwrap();

        let result = engine.generate();
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.kotlin_code.contains("TestForm"));
        assert!(result.kotlin_code.contains("email"));
        assert!(result.field_mappings.contains_key("email"));
    }

    #[test]
    fn test_form_validation() {
        let engine = FormBuilder::new("ValidationForm")
            .field("name")
                .field_type(InputType::Text)
                .min_length(2)
                .max_length(50)
                .done()
            .field("age")
                .field_type(InputType::Number)
                .min_length(1)
                .max_length(3)
                .done()
            .build()
            .unwrap();

        let mut form_data = HashMap::new();
        form_data.insert("name".to_string(), "John".to_string());
        form_data.insert("age".to_string(), "25".to_string());

        let result = engine.validate(&form_data);
        assert!(result.is_valid);
        assert_eq!(result.field_results.len(), 2);

        // Test invalid data
        form_data.insert("name".to_string(), "J".to_string()); // Too short
        let result = engine.validate(&form_data);
        assert!(!result.is_valid);
        assert!(!result.field_results[0].is_valid);
    }

    #[test]
    fn test_rich_text_formatting() {
        let formatting = RichTextFormatting {
            bold: true,
            italic: false,
            underline: true,
            strikethrough: false,
            font_size: Some(16.0),
            font_family: Some("Roboto".to_string()),
            text_align: Some(goya_lang::TextAlign::Center),
            line_height: Some(1.5),
            letter_spacing: Some(0.5),
            text_color: Some(0xFF0000FF), // Blue
            background_color: Some(0xFFFFFFFF), // White
        };

        let engine = FormBuilder::new("FormattedForm")
            .field("formatted_text")
                .field_type(InputType::Text)
                .rich_text_formatting(formatting)
                .done()
            .build()
            .unwrap();

        let result = engine.generate();
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.kotlin_code.contains("formatted_text"));
    }

    #[test]
    fn test_accessibility_config() {
        let accessibility = AccessibilityConfig {
            screen_reader_announcements: vec!["Username field".to_string()],
            aria_label: Some("Username input".to_string()),
            aria_describedby: Some("username-help".to_string()),
            role: Some("textbox".to_string()),
            live_region: Some(goya_lang::LiveRegion::PolitenessGentle),
            announce_value_changes: true,
            announce_validation_errors: true,
        };

        let engine = FormBuilder::new("AccessibleForm")
            .field("accessible_field")
                .field_type(InputType::Text)
                .accessibility(accessibility)
                .done()
            .accessibility(FormAccessibilityConfig {
                screen_reader_announcements: vec!["Form loaded".to_string()],
                focus_management: true,
                live_region_updates: true,
                keyboard_navigation: true,
            })
            .build()
            .unwrap();

        let result = engine.generate();
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.kotlin_code.contains("accessible_field"));
    }
}