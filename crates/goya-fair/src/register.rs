//! The GY-FAIR/2 reason-code register. It mirrors
//! `docs/disparates/contracts/reason-codes.md`, and a test keeps the two
//! equal. Codes are added by contract revision and never renamed or removed.

/// `(code, meaning)`, sorted by code.
pub const REASON_CODES: &[(&str, &str)] = &[
    ("ASAR_UNPACKED_NOT_SUPPLIED", "The file lives in app.asar.unpacked, which was not supplied."),
    ("BRIDGE_KEY_COMPUTED", "A preload bridge key is not a string literal."),
    ("CSS_AT_RULE_NOT_ANALYSED", "A CSS at-rule was recorded but not analysed."),
    ("DEPENDENCY_TREE_NOT_ANALYSED", "An installed dependency tree was recorded but not inventoried."),
    ("DYNAMIC_REQUIRE_UNRESOLVED", "A module specifier is computed, so its target is unknown."),
    ("ELECTRON_API_UNCLASSIFIED", "An Electron API outside the extractor's classification tables."),
    ("EXCLUDED_DIRECTORY", "Material that is not application source, recorded and skipped."),
    ("INLINE_HANDLER_NOT_ANALYSED", "Inline event-handler code in markup was not analysed."),
    ("INLINE_SCRIPT_NOT_ANALYSED", "An inline script in markup was not analysed."),
    ("INLINE_STYLE_NOT_ANALYSED", "An inline style block in markup was not analysed."),
    ("IPC_CHANNEL_COMPUTED", "An IPC channel name is not a string literal."),
    ("NATIVE_MODULE_OPAQUE", "A native module's behaviour cannot be read from source."),
    ("NON_UTF8_SOURCE", "Source text is not valid UTF-8."),
    ("NO_FAIR_KIND", "The construct has no kind in the FAIR vocabulary."),
    ("NO_GOYA_CAPABILITY_CHILD_PROCESS", "Goya has no capability for spawning processes."),
    ("PARSE_FAILED", "The source could not be parsed."),
    ("SYMLINK_NOT_FOLLOWED", "A symbolic link was recorded and not followed."),
];

/// True if `code` is in the register.
pub fn is_registered(code: &str) -> bool {
    REASON_CODES.binary_search_by(|(c, _)| c.cmp(&code)).is_ok()
}
