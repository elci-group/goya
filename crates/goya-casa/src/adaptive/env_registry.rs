//! Environmental condition registry (directive §13.1).
//!
//! The single source of truth for which condition features the pipeline
//! can judge at transform time (against [`Environment`]) and which are
//! runtime properties that must be deferred to the host. Every
//! adaptive consumer consults this registry instead of embedding its
//! own feature list, so support decisions stay consistent.

use crate::cascade::conditional::{ColorScheme, Environment, MediaType};
use std::collections::BTreeMap;

/// Whether a condition feature is judgeable at transform time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Judgeable now via the environment.
    Evaluable,
    /// A runtime property (container size, user settings the host must
    /// supply); recorded as a deferred constraint.
    Deferred,
}

/// Look up a feature's support. Names are case-insensitive; unknown
/// features are deferred, never guessed.
pub fn support(feature: &str) -> Support {
    match feature.trim().to_ascii_lowercase().as_str() {
        "width"
        | "height"
        | "min-width"
        | "max-width"
        | "min-height"
        | "max-height"
        | "orientation"
        | "prefers-color-scheme"
        | "media" => Support::Evaluable,
        // Runtime/user-agent properties: container size, input modality,
        // motion and transparency preferences, display modes.
        "container"
        | "hover"
        | "pointer"
        | "prefers-reduced-motion"
        | "prefers-contrast"
        | "display-mode"
        | "resolution"
        | "aspect-ratio"
        | "color"
        | "grid" => Support::Deferred,
        _ => Support::Deferred,
    }
}

/// Whether a feature name is registered at all (evaluable or deferred);
/// unregistered names are the ones adaptive consumers report as
/// unsupported rather than defer.
pub fn is_registered(feature: &str) -> bool {
    let lowered = feature.trim().to_ascii_lowercase();
    known().iter().any(|(name, _)| *name == lowered)
}

/// Every feature the registry knows about, in name order.
pub fn known() -> Vec<(&'static str, Support)> {
    vec![
        ("aspect-ratio", Support::Deferred),
        ("color", Support::Deferred),
        ("container", Support::Deferred),
        ("display-mode", Support::Deferred),
        ("grid", Support::Deferred),
        ("height", Support::Evaluable),
        ("hover", Support::Deferred),
        ("max-height", Support::Evaluable),
        ("max-width", Support::Evaluable),
        ("media", Support::Evaluable),
        ("min-height", Support::Evaluable),
        ("min-width", Support::Evaluable),
        ("orientation", Support::Evaluable),
        ("pointer", Support::Deferred),
        ("prefers-color-scheme", Support::Evaluable),
        ("prefers-contrast", Support::Deferred),
        ("prefers-reduced-motion", Support::Deferred),
        ("resolution", Support::Deferred),
        ("width", Support::Evaluable),
    ]
}

/// Evaluate one registry-known feature directly. This is the explicit
/// form of what `@media` evaluation does inline; only `Evaluable`
/// features are accepted here.
pub fn evaluate(feature: &str, value: &str, environment: &Environment) -> Result<bool, String> {
    if support(feature) != Support::Evaluable {
        return Err(format!(
            "feature is not transform-time evaluable: {feature}"
        ));
    }
    let synthesized = match feature.trim().to_ascii_lowercase().as_str() {
        "media" => value.trim().to_string(),
        other => format!("({other}: {value})"),
    };
    crate::cascade::conditional::evaluate_media(&synthesized, environment)
}

/// The environment expressed as canonical constraints, the registry's
/// own native view of the current host state.
pub fn describe(environment: &Environment) -> BTreeMap<String, String> {
    let mut described = BTreeMap::new();
    described.insert("media".to_string(), media_type_name(environment.media_type));
    if let Some(width) = environment.viewport_width {
        described.insert("width".to_string(), format!("{width}px"));
    }
    if let Some(height) = environment.viewport_height {
        described.insert("height".to_string(), format!("{height}px"));
    }
    if let Some(scheme) = environment.color_scheme {
        described.insert(
            "prefers-color-scheme".to_string(),
            match scheme {
                ColorScheme::Light => "light".to_string(),
                ColorScheme::Dark => "dark".to_string(),
            },
        );
    }
    described
}

fn media_type_name(media_type: MediaType) -> String {
    match media_type {
        MediaType::Screen => "screen".to_string(),
        MediaType::Print => "print".to_string(),
        MediaType::Speech => "speech".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cascade::conditional::ColorScheme;

    fn env() -> Environment {
        Environment {
            viewport_width: Some(1024.0),
            viewport_height: Some(768.0),
            media_type: MediaType::Screen,
            color_scheme: Some(ColorScheme::Dark),
        }
    }

    #[test]
    fn evaluable_features_are_classified() {
        for feature in [
            "width",
            "min-width",
            "max-height",
            "orientation",
            "prefers-color-scheme",
        ] {
            assert_eq!(support(feature), Support::Evaluable, "{feature}");
        }
    }

    #[test]
    fn runtime_features_are_deferred() {
        for feature in ["container", "hover", "pointer", "prefers-reduced-motion"] {
            assert_eq!(support(feature), Support::Deferred, "{feature}");
        }
    }

    #[test]
    fn unknown_features_are_deferred_not_guessed() {
        assert_eq!(support("gusto"), Support::Deferred);
        assert!(!is_registered("gusto"));
        assert!(is_registered("hover"));
        assert!(is_registered("MIN-WIDTH"));
    }

    #[test]
    fn evaluate_judges_known_features() {
        assert_eq!(evaluate("min-width", "500px", &env()), Ok(true));
        assert_eq!(evaluate("max-width", "500px", &env()), Ok(false));
        assert_eq!(evaluate("prefers-color-scheme", "dark", &env()), Ok(true));
        assert_eq!(evaluate("orientation", "landscape", &env()), Ok(true));
    }

    #[test]
    fn evaluate_rejects_deferred_features() {
        assert!(evaluate("hover", "hover", &env()).is_err());
        assert!(evaluate("container", "", &env()).is_err());
    }

    #[test]
    fn describe_reports_known_environment() {
        let described = describe(&env());
        assert_eq!(described.get("width").map(String::as_str), Some("1024px"));
        assert_eq!(
            described.get("prefers-color-scheme").map(String::as_str),
            Some("dark")
        );
        assert!(!described.contains_key("resolution"));
    }
}
