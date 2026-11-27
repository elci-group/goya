//! Breakpoint economy (directive §13.2): instead of one compiled variant
//! per media condition, collect the distinct widths a stylesheet consults
//! into one ordered set — the canonical breakpoint list the host previews
//! against. Wide-viewport-only features collapse into the "desktop" band
//! rather than multiplying variants.

use crate::ir::casa_ir::CasaRule;

/// The ordered set of distinct breakpoint widths a rule set consults,
/// with the width-less conditions recorded separately (orientation,
/// color-scheme, host-only features) since they cannot join a numeric
/// breakpoint ladder.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BreakpointSet {
    /// Distinct widths from `width`/`min-width`/`max-width` conditions,
    /// ascending. `f64` is not `Ord`, so the ladder is a sorted vec.
    pub widths: Vec<f64>,
    /// Conditions that name no width feature, in first-encounter order.
    pub non_width_conditions: Vec<String>,
}

impl BreakpointSet {
    pub fn is_empty(&self) -> bool {
        self.widths.is_empty() && self.non_width_conditions.is_empty()
    }

    /// The widest collected width, if any — the "desktop" band boundary.
    pub fn widest(&self) -> Option<f64> {
        self.widths.last().copied()
    }
}

/// Walk a rule set and collect every breakpoint the `@media` conditions
/// mention. Container conditions are excluded — their widths describe
/// elements, not viewports.
pub fn collect(rules: &[CasaRule]) -> BreakpointSet {
    let mut set = BreakpointSet::default();
    visit(rules, &mut set);
    set
}

/// Insert a width keeping the ladder sorted and duplicate-free.
fn insert_width(set: &mut BreakpointSet, width: f64) {
    match set
        .widths
        .binary_search_by(|w| w.partial_cmp(&width).expect("widths are finite"))
    {
        Ok(_) => {}
        Err(pos) => set.widths.insert(pos, width),
    }
}

fn visit(rules: &[CasaRule], set: &mut BreakpointSet) {
    for rule in rules {
        match rule {
            CasaRule::Media(media) => {
                collect_from_condition(&media.condition, set);
                visit(&media.rules, set);
            }
            CasaRule::Container(container) => visit(&container.rules, set),
            CasaRule::Layer(layer) => visit(&layer.rules, set),
            CasaRule::Supports(supports) => visit(&supports.rules, set),
            _ => {}
        }
    }
}

fn collect_from_condition(condition: &str, set: &mut BreakpointSet) {
    let mut mentioned_width = false;
    for feature in split_features(condition) {
        if let Some(width) = width_feature(&feature) {
            insert_width(set, width);
            mentioned_width = true;
        }
    }
    if !mentioned_width && set.non_width_conditions.last() != Some(&condition.to_string()) {
        set.non_width_conditions.push(condition.to_string());
    }
}

/// Parenthesised feature groups of a condition, as raw texts.
fn split_features(condition: &str) -> Vec<String> {
    let bytes = condition.as_bytes();
    let mut groups = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => {
                depth += 1;
                if depth == 1 {
                    start = Some(i + 1);
                }
            }
            b')' => {
                if depth == 1 {
                    if let Some(s) = start {
                        groups.push(condition[s..i].to_string());
                    }
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    groups
}

/// `min-width: 400px` → 400.0; features that are not a width query → None.
fn width_feature(feature: &str) -> Option<f64> {
    let (name, value) = feature.split_once(':')?;
    let name = name.trim().to_ascii_lowercase();
    if !matches!(name.as_str(), "width" | "min-width" | "max-width") {
        return None;
    }
    parse_px(value.trim())
}

/// Parse a px (or unitless) length; other units yield `None`.
fn parse_px(value: &str) -> Option<f64> {
    let number: String = value
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    if number.is_empty() {
        return None;
    }
    let rest = value.trim()[number.len()..].trim();
    match rest {
        "" | "px" => number.parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::reporter::Reporter;
    use crate::ir::casa_ir::Lowering;

    fn lower(css: &str) -> Vec<CasaRule> {
        let mut reporter = Reporter::new();
        let mut resources = Vec::new();
        let mut lowering = Lowering::new("test.css", css, &mut reporter, &mut resources);
        lowering.rules(goya_css_syntax::parse_stylesheet(css))
    }

    #[test]
    fn collects_distinct_widths_sorted() {
        let rules = lower(
            "@media (min-width: 768px) { .a { color: red } } \
             @media (min-width: 400px) { .b { color: blue } } \
             @media (max-width: 768px) { .c { color: green } }",
        );
        let set = collect(&rules);
        let widths: Vec<f64> = set.widths.to_vec();
        assert_eq!(widths, vec![400.0, 768.0]);
    }

    #[test]
    fn non_width_conditions_recorded_once() {
        let rules = lower(
            "@media (orientation: landscape) { .a { color: red } } \
             @media (orientation: landscape) { .b { color: blue } } \
             @media (prefers-color-scheme: dark) { .c { color: green } }",
        );
        let set = collect(&rules);
        assert!(set.widths.is_empty());
        assert_eq!(
            set.non_width_conditions,
            vec![
                "(orientation: landscape)".to_string(),
                "(prefers-color-scheme: dark)".to_string()
            ]
        );
    }

    #[test]
    fn container_widths_excluded() {
        // A container width describes an element, not a viewport band.
        let rules = lower("@container card (min-width: 400px) { .a { color: red } }");
        let set = collect(&rules);
        assert!(set.is_empty());
    }

    #[test]
    fn nested_media_visits_inner_conditions() {
        let rules =
            lower("@media (min-width: 400px) { @media (min-height: 300px) { .a { color: red } } }");
        let set = collect(&rules);
        let widths: Vec<f64> = set.widths.to_vec();
        assert_eq!(widths, vec![400.0]);
        // min-height is not a width feature → condition recorded.
        assert_eq!(set.non_width_conditions.len(), 1);
    }
}
