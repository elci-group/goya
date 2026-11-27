//! Property inheritance and defaulting (directive §7.5.2).
//!
//! Explicit tables for the properties the cascade resolves at this
//! stage. The geometry and appearance stages extend these tables as
//! they take over properties.

/// Properties that inherit from parent to child by default.
pub const INHERITED: &[&str] = &[
    "color",
    "direction",
    "font-family",
    "font-size",
    "font-style",
    "font-variant",
    "font-weight",
    "letter-spacing",
    "line-height",
    "list-style",
    "list-style-image",
    "list-style-position",
    "list-style-type",
    "orphans",
    "text-align",
    "text-decoration-color",
    "text-indent",
    "text-transform",
    "visibility",
    "white-space",
    "widows",
    "word-spacing",
];

/// Initial (root) values for known properties. Properties absent here
/// have no cascade-relevant default at this stage.
pub const INITIALS: &[(&str, &str)] = &[
    ("background-color", "transparent"),
    ("background-image", "none"),
    ("border-width", "medium"),
    ("color", "#000000"),
    ("direction", "ltr"),
    ("display", "inline"),
    ("font-family", "sans-serif"),
    ("font-size", "16px"),
    ("font-style", "normal"),
    ("font-weight", "normal"),
    ("line-height", "normal"),
    ("margin", "0"),
    ("opacity", "1"),
    ("overflow", "visible"),
    ("padding", "0"),
    ("position", "static"),
    ("text-align", "start"),
    ("text-transform", "none"),
    ("visibility", "visible"),
    ("white-space", "normal"),
    ("z-index", "auto"),
];

/// Whether a property inherits from parent to child by default.
pub fn is_inherited(property: &str) -> bool {
    INHERITED.binary_search(&property).is_ok()
}

/// Whether the property is known to the cascade at all; `@supports`
/// checks and strictness diagnostics key off this.
pub fn is_known_property(property: &str) -> bool {
    property.starts_with("--")
        || INHERITED.binary_search(&property).is_ok()
        || INITIALS
            .binary_search_by_key(&property, |(name, _)| *name)
            .is_ok()
        || matches!(
            property,
            "align-items"
                | "background"
                | "border"
                | "border-color"
                | "border-radius"
                | "bottom"
                | "box-shadow"
                | "flex"
                | "flex-direction"
                | "flex-wrap"
                | "gap"
                | "grid-template-columns"
                | "grid-template-rows"
                | "height"
                | "justify-content"
                | "left"
                | "margin-bottom"
                | "margin-left"
                | "margin-right"
                | "margin-top"
                | "max-height"
                | "max-width"
                | "min-height"
                | "min-width"
                | "padding-bottom"
                | "padding-left"
                | "padding-right"
                | "padding-top"
                | "right"
                | "text-decoration"
                | "top"
                | "transform"
                | "width"
        )
}

/// The initial (root) value of a property, when known.
pub fn initial_value(property: &str) -> Option<&'static str> {
    INITIALS
        .binary_search_by_key(&property, |(name, _)| *name)
        .ok()
        .map(|index| INITIALS[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_for_binary_search() {
        assert!(INHERITED.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(INITIALS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn inherited_versus_not() {
        assert!(is_inherited("color"));
        assert!(is_inherited("font-size"));
        assert!(!is_inherited("margin"));
        assert!(!is_inherited("--custom")); // custom properties inherit via their own path
    }

    #[test]
    fn known_properties_include_tables_and_custom() {
        assert!(is_known_property("display"));
        assert!(is_known_property("margin-top"));
        assert!(is_known_property("--anything"));
        assert!(!is_known_property("not-a-property"));
    }

    #[test]
    fn initial_values_cover_core_properties() {
        assert_eq!(initial_value("display"), Some("inline"));
        assert_eq!(initial_value("margin"), Some("0"));
        assert_eq!(initial_value("made-up"), None);
    }
}
