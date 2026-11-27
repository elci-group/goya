//! Shadow DOM encapsulation handling (directive §10).
//!
//! Phase 1 classifies shadow-specific selectors and audits sheets for
//! host selectors that cannot apply outside a shadow root. Selector
//! matching against a shadow tree lands with the context stage.

use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::{CasaRule, Sheet, SheetScope};

/// The shadow-relevant shape of one selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSelector {
    /// `:host` or `:host(...)` — matches the shadow host.
    Host { argument: Option<String> },
    /// `:host-context(...)` — matches the host against an ancestor condition.
    HostContext { argument: String },
    /// `::slotted(...)` or `::slotted` — matches slotted light-DOM children.
    Slotted { argument: Option<String> },
    /// No shadow-specific pseudo-class.
    Plain,
}

/// Classify one selector's shadow relevance. The first shadow pseudo-class
/// wins; combinations are recorded as their first match.
pub fn classify_selector(selector: &str) -> HostSelector {
    if let Some((_, argument)) = find_pseudo(selector, ":host-context") {
        return HostSelector::HostContext {
            argument: argument.unwrap_or_default(),
        };
    }
    if let Some((_, argument)) = find_pseudo(selector, "::slotted") {
        return HostSelector::Slotted { argument };
    }
    if let Some((_, argument)) = find_pseudo(selector, ":host") {
        return HostSelector::Host { argument };
    }
    HostSelector::Plain
}

/// Find a pseudo-class/element occurrence that is not part of a longer
/// identifier, returning its position and parenthesised argument.
fn find_pseudo(selector: &str, name: &str) -> Option<(usize, Option<String>)> {
    let mut from = 0;
    while let Some(offset) = selector[from..].find(name) {
        let at = from + offset;
        let before = selector[..at].chars().next_back();
        let after = &selector[at + name.len()..];
        let ident_continues = before.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_')
            || after
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_');
        // The name must be a whole pseudo-class, not a prefix like `:host`
        // inside `:hostile` or `:host-context`.
        if ident_continues {
            from = at + name.len();
            continue;
        }
        let argument = if after.starts_with('(') {
            Some(balanced_argument(after)?)
        } else {
            None
        };
        return Some((at, argument));
    }
    None
}

/// Extract the balanced `( ... )` argument at the start of `input`.
fn balanced_argument(input: &str) -> Option<String> {
    let mut depth = 0usize;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(input[1..index].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Shadow audit of one sheet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShadowAudit {
    /// Style rules containing a `:host`/`:host-context` selector.
    pub host_rules: usize,
    /// Style rules containing a `::slotted` selector.
    pub slotted_rules: usize,
    /// Whether the sheet is authored for a shadow root.
    pub encapsulated: bool,
}

/// Audit a sheet: count shadow-relevant rules and report host selectors
/// found outside a shadow-root sheet (`CASA-SHD-001`).
pub fn audit_sheet(sheet: &Sheet, reporter: &mut Reporter) -> ShadowAudit {
    let mut audit = ShadowAudit {
        encapsulated: sheet.scope == SheetScope::ShadowRoot,
        ..ShadowAudit::default()
    };
    audit_rules(&sheet.rules, sheet, &mut audit, reporter);
    audit
}

fn audit_rules(
    rules: &[CasaRule],
    sheet: &Sheet,
    audit: &mut ShadowAudit,
    reporter: &mut Reporter,
) {
    for rule in rules {
        match rule {
            CasaRule::Style(style) => {
                let mut counted_host = false;
                let mut counted_slotted = false;
                for selector in &style.selectors {
                    match classify_selector(&selector.text) {
                        HostSelector::Plain => {}
                        HostSelector::Host { .. } | HostSelector::HostContext { .. } => {
                            if !counted_host {
                                audit.host_rules += 1;
                                counted_host = true;
                            }
                            if sheet.scope != SheetScope::ShadowRoot {
                                reporter.emit(
                                    codes::SHADOW_HOST_OUTSIDE_SHADOW_ROOT,
                                    format!(":host selector in light-DOM sheet: {}", selector.text),
                                    None,
                                );
                            }
                        }
                        HostSelector::Slotted { .. } => {
                            if !counted_slotted {
                                audit.slotted_rules += 1;
                                counted_slotted = true;
                            }
                            if sheet.scope != SheetScope::ShadowRoot {
                                reporter.emit(
                                    codes::SHADOW_HOST_OUTSIDE_SHADOW_ROOT,
                                    format!(
                                        "::slotted selector in light-DOM sheet: {}",
                                        selector.text
                                    ),
                                    None,
                                );
                            }
                        }
                    }
                }
            }
            CasaRule::Media(media) => audit_rules(&media.rules, sheet, audit, reporter),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_host_selectors() {
        assert_eq!(
            classify_selector(":host"),
            HostSelector::Host { argument: None }
        );
        assert_eq!(
            classify_selector(":host(.dark) .panel"),
            HostSelector::Host {
                argument: Some(".dark".into())
            }
        );
        assert_eq!(
            classify_selector(":host-context(.rtl) .row"),
            HostSelector::HostContext {
                argument: ".rtl".into()
            }
        );
        assert_eq!(
            classify_selector("::slotted(span)"),
            HostSelector::Slotted {
                argument: Some("span".into())
            }
        );
        assert_eq!(classify_selector("div > p"), HostSelector::Plain);
    }

    #[test]
    fn host_prefix_does_not_swallow_other_pseudos() {
        // :hover shares no prefix with :host; but :host must not match in
        // `:hostile` either.
        assert_eq!(classify_selector(":hover"), HostSelector::Plain);
        assert_eq!(classify_selector(":hostile"), HostSelector::Plain);
    }

    #[test]
    fn audit_reports_host_selectors_in_light_dom_sheets() {
        let css = ":host { color: red } .plain { margin: 0 }";
        let ingested = crate::ingestion::parser::ingest_text("light.css", css);
        let mut reporter = Reporter::new();
        let audit = audit_sheet(&ingested.sheet, &mut reporter);
        assert_eq!(audit.host_rules, 1);
        assert!(!audit.encapsulated);
        assert_eq!(reporter.diagnostics()[0].code, "CASA-SHD-001");
    }

    #[test]
    fn audit_is_quiet_for_shadow_root_sheets() {
        let css = ":host(.dark) { color: red } ::slotted(p) { margin: 0 }";
        let mut ingested = crate::ingestion::parser::ingest_text("shadow.css", css);
        ingested.sheet.scope = SheetScope::ShadowRoot;
        let mut reporter = Reporter::new();
        let audit = audit_sheet(&ingested.sheet, &mut reporter);
        assert_eq!(audit.host_rules, 1);
        assert_eq!(audit.slotted_rules, 1);
        assert!(audit.encapsulated);
        assert!(reporter.is_empty());
    }
}
