//! Pattern-based redaction for Deep Copy payloads.
//!
//! Conservative and deterministic: matched spans are replaced with a fixed
//! placeholder and every replacement is recorded so recipients know what was
//! withheld and why.

/// Placeholder substituted for redacted spans.
pub const PLACEHOLDER: &str = "[REDACTED]";

/// One redaction applied to a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionRecord {
    /// Which class of sensitive content was removed.
    pub kind: &'static str,
    /// Human-readable reason, safe to include in the bundle itself.
    pub reason: String,
}

/// Redact emails, bearer/API tokens and long hexadecimal secrets from
/// `text`. Returns the sanitised text and the redaction records.
pub fn redact(text: &str) -> (String, Vec<RedactionRecord>) {
    let mut out = text.to_string();
    let mut records = Vec::new();

    let patterns: &[(&'static str, &str)] = &[
        ("email", "email address removed for privacy"),
        ("api_token", "API token removed for security"),
        ("hex_secret", "long hexadecimal secret removed for security"),
    ];
    for (kind, reason) in patterns {
        let spans = find_spans(&out, kind);
        if !spans.is_empty() {
            for (start, end) in spans.iter().rev() {
                out.replace_range(*start..*end, PLACEHOLDER);
            }
            records.push(RedactionRecord {
                kind,
                reason: reason.to_string(),
            });
        }
    }
    (out, records)
}

/// Locate spans of the given sensitive-content class. Byte offsets, ordered
/// ascending, non-overlapping.
fn find_spans(text: &str, kind: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    match kind {
        "email" => {
            let bytes = text.as_bytes();
            for i in 0..bytes.len() {
                if bytes[i] == b'@' && i > 0 && i + 1 < bytes.len() {
                    let start = text[..i]
                        .rfind(|c: char| c.is_whitespace())
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    let end = text[i + 1..]
                        .find(|c: char| c.is_whitespace())
                        .map(|p| i + 1 + p)
                        .unwrap_or(text.len());
                    if start < i
                        && end > i + 1
                        && text[start..i]
                            .chars()
                            .all(|c| c.is_alphanumeric() || "-._".contains(c))
                        && text[i + 1..end]
                            .chars()
                            .all(|c| c.is_alphanumeric() || ".-".contains(c))
                    {
                        spans.push((start, end));
                    }
                }
            }
        }
        "api_token" => {
            for prefix in ["sk-live-", "sk-", "Bearer "] {
                let mut from = 0;
                while let Some(pos) = text[from..].find(prefix) {
                    let start = from + pos;
                    // The secret body starts after the prefix; find where it
                    // ends (the prefix itself may contain a trailing space).
                    let body = start + prefix.len();
                    let end = text[body..]
                        .find(|c: char| c.is_whitespace())
                        .map(|p| body + p)
                        .unwrap_or(text.len());
                    // Only redact when a secret body follows the prefix, and
                    // never record a span overlapping an earlier match
                    // (e.g. `sk-` inside `sk-live-`).
                    if end > body && !spans.iter().any(|(s, e)| start < *e && end > *s) {
                        spans.push((start, end));
                    }
                    from = end.max(body);
                }
            }
        }
        "hex_secret" => {
            let mut i = 0;
            while i < text.len() {
                if let Some(rest) = text[i..].chars().next() {
                    if rest.is_ascii_hexdigit() {
                        let start = i;
                        let mut end = i;
                        for c in text[i..].chars() {
                            if c.is_ascii_hexdigit() {
                                end += c.len_utf8();
                            } else {
                                break;
                            }
                        }
                        // Long hex runs (>= 32 chars) are treated as secrets.
                        if end - start >= 32 {
                            spans.push((start, end));
                            i = end;
                            continue;
                        }
                    }
                    i += rest.len_utf8();
                } else {
                    break;
                }
            }
        }
        _ => {}
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_email() {
        let (out, recs) = redact("contact alice@example.com now");
        assert_eq!(out, "contact [REDACTED] now");
        assert_eq!(recs[0].kind, "email");
    }

    #[test]
    fn redacts_bearer_token() {
        let (out, _) = redact("auth: Bearer abcdef123456 sent");
        assert_eq!(out, "auth: [REDACTED] sent");
    }

    #[test]
    fn leaves_short_hex_alone() {
        let (out, _) = redact("color #3267C8 is fine");
        assert_eq!(out, "color #3267C8 is fine");
    }

    #[test]
    fn redacts_long_hex_secret() {
        let (out, recs) = redact("key deadbeefdeadbeefdeadbeefdeadbeef end");
        assert_eq!(out, "key [REDACTED] end");
        assert_eq!(recs[0].kind, "hex_secret");
    }

    #[test]
    fn plain_text_untouched() {
        let (out, recs) = redact("Module compilation failed: incompatible state schema.");
        assert_eq!(out, "Module compilation failed: incompatible state schema.");
        assert!(recs.is_empty());
    }
}
