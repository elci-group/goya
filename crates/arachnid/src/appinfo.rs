//! `appinfo.json` for packaged webOS TV web apps, validated against LG's
//! published rules (webostv.developer.lge.com "appinfo.json", checked on
//! 20 November 2026).
//!
//! * Required fields: `id`, `title`, `type` (`"web"`), `main`, `icon`,
//!   `version`.
//! * `id`: lowercase letters, digits, `-` and `.`; starts alphanumeric; at
//!   least two characters; not under the `com.palm`, `com.webos`, `com.lge`
//!   or `com.palmdts` domains.
//! * `version`: three non-negative integers of up to nine digits, with no
//!   leading zeros.
//!
//! Arachnid always writes `"resolution": "1920x1080"`, which is the
//! ontology limit. It leaves `disableBackHistoryAPI` at its default
//! (`false`), so the platform handles Back: with no history it shows the
//! exit popup on 6.0 and later, and goes Home on 5.0 and earlier.

use crate::diagnostics::{Code, Diagnostic};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppInfo {
    pub id: String,
    pub title: String,
    pub version: String,
    pub vendor: Option<String>,
}

pub const RESERVED_DOMAINS: [&str; 4] = ["com.palm", "com.webos", "com.lge", "com.palmdts"];
pub const MAIN: &str = "index.html";
pub const ICON: &str = "icon.png";
pub const RESOLUTION: &str = "1920x1080";

/// JSON string literal with escapes for `"`, `\` and control characters.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Keep </script> and U+2028/2029 inert when embedded in JS.
            '<' => out.push_str("\\u003c"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn valid_id(id: &str) -> Result<(), String> {
    let b = id.as_bytes();
    if b.len() < 2 {
        return Err("must be at least two characters".into());
    }
    if !b[0].is_ascii_lowercase() && !b[0].is_ascii_digit() {
        return Err("must start with a lowercase letter or digit".into());
    }
    if let Some(c) = id
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '.'))
    {
        return Err(format!(
            "contains {c:?}; only a-z, 0-9, '-' and '.' are allowed"
        ));
    }
    if let Some(domain) = RESERVED_DOMAINS
        .iter()
        .find(|d| id == **d || id.starts_with(&format!("{d}.")))
    {
        return Err(format!("uses the reserved domain {domain}"));
    }
    Ok(())
}

fn valid_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return Err("must have exactly three parts (major.minor.revision)".into());
    }
    for p in parts {
        if p.is_empty() || p.len() > 9 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("part {p:?} must be 1–9 digits"));
        }
        if p.len() > 1 && p.starts_with('0') {
            return Err(format!("part {p:?} has a leading zero"));
        }
    }
    Ok(())
}

fn valid_text(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(())
}

impl AppInfo {
    pub fn validate(&self) -> Result<(), Vec<Diagnostic>> {
        let mut diags = Vec::new();
        let mut push = |field: &str, r: Result<(), String>| {
            if let Err(e) = r {
                diags.push(Diagnostic::new(
                    Code::InvalidAppInfo,
                    format!("appinfo {field}: {e}"),
                ));
            }
        };
        push("id", valid_id(&self.id));
        push("version", valid_version(&self.version));
        push("title", valid_text("title", &self.title));
        if let Some(vendor) = &self.vendor {
            push("vendor", valid_text("vendor", vendor));
        }
        if diags.is_empty() {
            Ok(())
        } else {
            Err(diags)
        }
    }

    /// Deterministic JSON with a fixed key order and a trailing newline.
    pub fn to_json(&self) -> String {
        let mut fields = vec![
            ("id", json_string(&self.id)),
            ("title", json_string(&self.title)),
            ("type", json_string("web")),
            ("main", json_string(MAIN)),
            ("icon", json_string(ICON)),
            ("version", json_string(&self.version)),
        ];
        if let Some(vendor) = &self.vendor {
            fields.push(("vendor", json_string(vendor)));
        }
        fields.push(("resolution", json_string(RESOLUTION)));
        let body: Vec<String> = fields
            .iter()
            .map(|(k, v)| format!("  \"{k}\": {v}"))
            .collect();
        format!("{{\n{}\n}}\n", body.join(",\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, version: &str) -> AppInfo {
        AppInfo {
            id: id.into(),
            title: "Media".into(),
            version: version.into(),
            vendor: None,
        }
    }

    #[test]
    fn accepts_lg_example_shape() {
        assert!(app("com.mycompany.app.appname", "1.0.0").validate().is_ok());
        assert!(app("com.palmer.app", "999999999.0.10").validate().is_ok());
    }

    #[test]
    fn rejects_lg_forbidden_values() {
        for id in [
            "a",
            "Com.x",
            "-ab",
            "com.x_y",
            "com.palm",
            "com.webos.app",
            "com.lge.x",
            "com.palmdts.x",
            "com.x y",
        ] {
            assert!(app(id, "1.0.0").validate().is_err(), "{id}");
        }
        for v in [
            "1.0",
            "2.1.0.0",
            "002.1.0",
            "1.01.0",
            "1.0.a",
            "1234567890.0.0",
            "",
        ] {
            assert!(app("com.x.y", v).validate().is_err(), "{v}");
        }
        let mut bad = app("com.x.y", "1.0.0");
        bad.title = "  ".into();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn json_is_deterministic_and_escaped() {
        let mut a = app("com.x.y", "1.2.3");
        a.title = "Say \"hi\" </script>".into();
        a.vendor = Some("Goya".into());
        let json = a.to_json();
        assert_eq!(json, a.to_json());
        assert!(json.starts_with(
            "{\n  \"id\": \"com.x.y\",\n  \"title\": \"Say \\\"hi\\\" \\u003c/script>\",\n"
        ));
        assert!(json.ends_with("  \"vendor\": \"Goya\",\n  \"resolution\": \"1920x1080\"\n}\n"));
    }
}
