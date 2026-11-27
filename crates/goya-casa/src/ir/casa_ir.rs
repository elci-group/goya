//! CasaIR — the canonical semantic model (directive §7).
//!
//! CasaIR is the single representation every later stage consumes and
//! every mode produces. It is a faithful, span-anchored record of the
//! stylesheet: nothing the CSS parser yields is silently dropped —
//! constructs CasaIR cannot yet represent are recorded as
//! `CasaRule::Unsupported` with a diagnostic.

use crate::diagnostics::codes;
use crate::diagnostics::reporter::{Location, Reporter};
use crate::ir::value_categories::ValueCategory;
use goya_css_syntax::{AtRule, Block, ComponentValue, DeclarationItem, Rule, Token};
use serde::{Deserialize, Serialize};

/// Bumped on any breaking change to the serialized shape.
pub const IR_SCHEMA_VERSION: u32 = 1;

/// The canonical semantic model of one ingestion unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasaIr {
    pub schema_version: u32,
    /// Entry stylesheet name or path.
    pub origin: String,
    /// Entry sheet first, then imported sheets in resolution order.
    pub sheets: Vec<Sheet>,
    /// Discovered resources, deduplicated in first-reference order.
    pub resources: Vec<ResourceRef>,
}

impl CasaIr {
    pub fn empty(origin: impl Into<String>) -> Self {
        CasaIr {
            schema_version: IR_SCHEMA_VERSION,
            origin: origin.into(),
            sheets: Vec::new(),
            resources: Vec::new(),
        }
    }

    pub fn entry_sheet(&self) -> Option<&Sheet> {
        self.sheets.first()
    }
}

/// One stylesheet (entry sheet or a resolved import).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub name: String,
    pub scope: SheetScope,
    pub rules: Vec<CasaRule>,
}

/// Whether the sheet is a light-DOM sheet or authored for a shadow root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetScope {
    Light,
    ShadowRoot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CasaRule {
    Style(StyleRule),
    Media(MediaRule),
    /// `@import` recorded at its source position; `resolved` names the
    /// sheet it was replaced by, when resolution succeeded.
    Import(ImportRule),
    FontFace(FontFaceRule),
    Keyframes(KeyframesRule),
    /// Parsed but not yet represented; never dropped silently.
    Unsupported(UnsupportedAtRule),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<DeclaredProperty>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Selector {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclaredProperty {
    pub name: String,
    pub value: String,
    pub important: bool,
    pub start: usize,
    pub end: usize,
    pub category: ValueCategory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaRule {
    pub condition: String,
    pub rules: Vec<CasaRule>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportRule {
    pub url: String,
    pub start: usize,
    pub end: usize,
    /// Sheet name this import resolved to, when it was inlined.
    pub resolved: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontFaceRule {
    pub declarations: Vec<DeclaredProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyframesRule {
    pub name: String,
    pub steps: Vec<KeyframeStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyframeStep {
    /// Offset selectors as written, e.g. `from`, `0%`, `0%, 100%`.
    pub offsets: String,
    pub declarations: Vec<DeclaredProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnsupportedAtRule {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRef {
    pub kind: ResourceKind,
    pub url: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Font,
    Image,
    Other,
}

/// Everything the lowering needs to read one sheet: its text, its name
/// (for diagnostic locations) and its output sinks.
pub struct Lowering<'a> {
    pub sheet: &'a str,
    pub source: &'a str,
    pub reporter: &'a mut Reporter,
    pub resources: &'a mut Vec<ResourceRef>,
}

impl<'a> Lowering<'a> {
    pub fn new(
        sheet: &'a str,
        source: &'a str,
        reporter: &'a mut Reporter,
        resources: &'a mut Vec<ResourceRef>,
    ) -> Self {
        Lowering {
            sheet,
            source,
            reporter,
            resources,
        }
    }

    fn location(&self, start: usize, end: usize) -> Location {
        Location {
            source: self.sheet.to_string(),
            start,
            end,
        }
    }

    /// Lower a parsed rule list into CasaIR rules.
    pub fn rules(&mut self, rules: Vec<Rule>) -> Vec<CasaRule> {
        rules.iter().filter_map(|rule| self.rule(rule)).collect()
    }

    /// Lower one parsed rule. `None` when the specification discards it.
    pub fn rule(&mut self, rule: &Rule) -> Option<CasaRule> {
        match rule {
            Rule::Qualified(qualified) => {
                let selectors = self.selectors(&qualified.prelude);
                let declarations = self.declarations(&qualified.block);
                Some(CasaRule::Style(StyleRule {
                    selectors,
                    declarations,
                    start: qualified.span.start,
                    end: qualified.span.end,
                }))
            }
            Rule::At(at) => self.at_rule(at),
            Rule::Invalid { span } => {
                self.reporter.emit(
                    codes::SYNTAX_INVALID,
                    "stylesheet ended inside a rule; the rule is dropped per CSS syntax",
                    Some(self.location(span.start, span.end)),
                );
                None
            }
        }
    }

    fn at_rule(&mut self, at: &AtRule) -> Option<CasaRule> {
        match at.name.as_str() {
            "media" => {
                let condition = self.prelude_text(&at.prelude);
                let rules = match &at.block {
                    Some(block) => self.group_contents(&block.contents),
                    None => Vec::new(),
                };
                Some(CasaRule::Media(MediaRule {
                    condition,
                    rules,
                    start: at.span.start,
                    end: at.span.end,
                }))
            }
            "import" => match self.import_url(&at.prelude) {
                Some(url) => Some(CasaRule::Import(ImportRule {
                    url,
                    start: at.span.start,
                    end: at.span.end,
                    resolved: None,
                })),
                None => {
                    self.reporter.emit(
                        codes::SYNTAX_INVALID,
                        "@import without a string or url() target",
                        Some(self.location(at.span.start, at.span.end)),
                    );
                    None
                }
            },
            "font-face" => {
                let declarations = match &at.block {
                    Some(block) => self.declarations(block),
                    None => Vec::new(),
                };
                Some(CasaRule::FontFace(FontFaceRule { declarations }))
            }
            "keyframes" | "-webkit-keyframes" | "-moz-keyframes" => {
                let name = self.prelude_text(&at.prelude);
                let steps = match &at.block {
                    Some(block) => self.keyframe_steps(&block.contents),
                    None => Vec::new(),
                };
                Some(CasaRule::Keyframes(KeyframesRule { name, steps }))
            }
            other => {
                self.reporter.emit(
                    codes::IR_UNSUPPORTED,
                    format!("@{other} is not represented in CasaIR yet"),
                    Some(self.location(at.span.start, at.span.end)),
                );
                Some(CasaRule::Unsupported(UnsupportedAtRule {
                    name: other.to_string(),
                    start: at.span.start,
                    end: at.span.end,
                }))
            }
        }
    }

    /// Group-rule bodies (`@media { ... }`) hold a rule list: selector
    /// prelude component values followed by one declaration block per
    /// rule. Unknown leftovers are reported, never dropped silently.
    fn group_contents(&mut self, contents: &[ComponentValue]) -> Vec<CasaRule> {
        let mut rules = Vec::new();
        let mut prelude: Vec<&ComponentValue> = Vec::new();
        for value in contents {
            match value {
                ComponentValue::Token(Token::Whitespace, _) => {}
                ComponentValue::Block(block) => {
                    let selectors = self.prelude_selectors(&prelude);
                    rules.push(CasaRule::Style(StyleRule {
                        selectors,
                        declarations: self.declarations(block),
                        start: prelude
                            .first()
                            .map(|v| v.span().start)
                            .unwrap_or(block.span.start),
                        end: block.span.end,
                    }));
                    prelude.clear();
                }
                other => prelude.push(other),
            }
        }
        if let (Some(first), Some(last)) = (prelude.first(), prelude.last()) {
            self.reporter.emit(
                codes::IR_UNSUPPORTED,
                "construct inside a group rule body is not represented in CasaIR yet",
                Some(self.location(first.span().start, last.span().end)),
            );
        }
        rules
    }

    fn declarations(&mut self, block: &Block) -> Vec<DeclaredProperty> {
        let mut properties = Vec::new();
        for item in goya_css_syntax::block_declarations(block) {
            match item {
                DeclarationItem::Declaration(declaration) => {
                    let value = self.value_text(&declaration.value);
                    self.collect_resources(&declaration.name, &declaration.value);
                    properties.push(DeclaredProperty {
                        name: declaration.name,
                        value,
                        important: declaration.important,
                        start: declaration.span.start,
                        end: declaration.span.end,
                        category: ValueCategory::Declared,
                    });
                }
                DeclarationItem::At(at) => {
                    self.reporter.emit(
                        codes::IR_UNSUPPORTED,
                        format!(
                            "@{} inside a declaration block is not represented yet",
                            at.name
                        ),
                        Some(self.location(at.span.start, at.span.end)),
                    );
                }
                DeclarationItem::Invalid { span } => {
                    self.reporter.emit(
                        codes::SYNTAX_INVALID,
                        "invalid declaration dropped per CSS syntax",
                        Some(self.location(span.start, span.end)),
                    );
                }
            }
        }
        properties
    }

    fn keyframe_steps(&mut self, contents: &[ComponentValue]) -> Vec<KeyframeStep> {
        let mut steps = Vec::new();
        let mut offsets: Vec<&ComponentValue> = Vec::new();
        for value in contents {
            match value {
                ComponentValue::Token(Token::Whitespace, _) => {}
                ComponentValue::Block(block) => {
                    let offsets_text = match (offsets.first(), offsets.last()) {
                        (Some(first), Some(last)) => self
                            .slice(first.span().start, last.span().end)
                            .trim()
                            .to_string(),
                        _ => String::new(),
                    };
                    steps.push(KeyframeStep {
                        offsets: offsets_text,
                        declarations: self.declarations(block),
                    });
                    offsets.clear();
                }
                other => offsets.push(other),
            }
        }
        steps
    }

    /// Split a rule prelude on top-level commas. Commas nested in blocks
    /// or functions live inside those variants and never split here.
    fn selectors(&mut self, prelude: &[ComponentValue]) -> Vec<Selector> {
        let refs: Vec<&ComponentValue> = prelude.iter().collect();
        self.split_selector_segments(&refs)
    }

    /// Same split for an accumulated group-rule prelude.
    fn prelude_selectors(&mut self, prelude: &[&ComponentValue]) -> Vec<Selector> {
        self.split_selector_segments(prelude)
    }

    fn split_selector_segments(&mut self, prelude: &[&ComponentValue]) -> Vec<Selector> {
        let mut selectors = Vec::new();
        let mut segment: Option<(usize, usize)> = None;
        let flush = |segment: &mut Option<(usize, usize)>,
                     selectors: &mut Vec<Selector>,
                     lowering: &mut Self| {
            if let Some((start, end)) = segment.take() {
                let owned = lowering.slice(start, end);
                let text = owned.trim();
                if text.is_empty() {
                    lowering.reporter.emit(
                        codes::SYNTAX_INVALID,
                        "empty selector segment",
                        Some(lowering.location(start, end)),
                    );
                } else {
                    selectors.push(Selector {
                        text: text.to_string(),
                        start,
                        end,
                    });
                }
            }
        };
        for value in prelude {
            match value {
                ComponentValue::Token(Token::Comma, _) => flush(&mut segment, &mut selectors, self),
                ComponentValue::Token(Token::Whitespace, span) => {
                    if let Some((_, end)) = segment.as_mut() {
                        *end = span.end.max(*end);
                    }
                }
                other => {
                    let span = other.span();
                    segment = Some(match segment {
                        Some((start, end)) => (start, span.end.max(end)),
                        None => (span.start, span.end),
                    });
                }
            }
        }
        flush(&mut segment, &mut selectors, self);
        selectors
    }

    fn prelude_text(&self, prelude: &[ComponentValue]) -> String {
        let mut text = String::new();
        for value in prelude {
            let span = value.span();
            text.push_str(&self.slice(span.start, span.end));
        }
        text.trim().to_string()
    }

    fn import_url(&self, prelude: &[ComponentValue]) -> Option<String> {
        for value in prelude {
            match value {
                ComponentValue::Token(Token::Whitespace, _) => continue,
                ComponentValue::Token(Token::String { value, .. }, _) => {
                    return Some(value.clone())
                }
                ComponentValue::Token(Token::Url { value, .. }, _) => return Some(value.clone()),
                ComponentValue::Function {
                    name, arguments, ..
                } if name.eq_ignore_ascii_case("url") => {
                    for argument in arguments {
                        if let ComponentValue::Token(Token::String { value, .. }, _) = argument {
                            return Some(value.clone());
                        }
                    }
                    return None;
                }
                _ => return None,
            }
        }
        None
    }

    /// Resource discovery inside one declaration value: url() tokens and
    /// `url("...")` functions. Kind follows the declaring property.
    fn collect_resources(&mut self, property: &str, values: &[ComponentValue]) {
        let kind = resource_kind_for(property);
        for value in values {
            match value {
                ComponentValue::Token(Token::Url { value, .. }, span) => {
                    self.resources.push(ResourceRef {
                        kind,
                        url: value.clone(),
                        start: span.start,
                        end: span.end,
                    });
                }
                ComponentValue::Function {
                    name, arguments, ..
                } if name.eq_ignore_ascii_case("url") => {
                    let span = value.span();
                    for argument in arguments {
                        if let ComponentValue::Token(Token::String { value, .. }, _) = argument {
                            self.resources.push(ResourceRef {
                                kind,
                                url: value.clone(),
                                start: span.start,
                                end: span.end,
                            });
                        }
                    }
                }
                ComponentValue::Block(block) => {
                    self.collect_resources(property, &block.contents);
                }
                ComponentValue::Function { arguments, .. } => {
                    self.collect_resources(property, arguments);
                }
                _ => {}
            }
        }
    }

    /// Value text from the value's component values: the span from the
    /// first to the last component excludes the `!important` (already
    /// removed by the parser) and any leading whitespace, while keeping
    /// original inner spacing.
    fn value_text(&self, values: &[ComponentValue]) -> String {
        match (values.first(), values.last()) {
            (Some(first), Some(last)) => {
                let text = self.slice(first.span().start, last.span().end);
                text.trim().to_string()
            }
            _ => String::new(),
        }
    }

    fn slice(&self, start: usize, end: usize) -> String {
        let end = end.min(self.source.len());
        let start = start.min(end);
        self.source[start..end].to_string()
    }
}

fn resource_kind_for(property: &str) -> ResourceKind {
    let property = property.to_ascii_lowercase();
    if property == "src" {
        // @font-face src.
        ResourceKind::Font
    } else if property.starts_with("background")
        || property.starts_with("mask")
        || property == "content"
        || property.starts_with("list-style")
        || property.starts_with("border-image")
    {
        ResourceKind::Image
    } else {
        ResourceKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::parser::ParsedStylesheet;
    use std::str::FromStr;

    fn lower(css: &str) -> (Vec<CasaRule>, Reporter) {
        let mut reporter = Reporter::new();
        let mut resources = Vec::new();
        let mut lowering = Lowering::new("test.css", css, &mut reporter, &mut resources);
        let rules = lowering.rules(goya_css_syntax::parse_stylesheet(css));
        (rules, reporter)
    }

    #[test]
    fn style_rule_lowers_with_selectors_and_declarations() {
        let (rules, reporter) = lower("body, .card { color: red !important; --gap: 8px }");
        assert!(reporter.is_empty());
        assert_eq!(rules.len(), 1);
        match &rules[0] {
            CasaRule::Style(style) => {
                let names: Vec<_> = style.selectors.iter().map(|s| s.text.as_str()).collect();
                assert_eq!(names, ["body", ".card"]);
                assert_eq!(style.declarations.len(), 2);
                assert_eq!(style.declarations[0].name, "color");
                assert_eq!(style.declarations[0].value, "red");
                assert!(style.declarations[0].important);
                assert_eq!(style.declarations[1].name, "--gap");
                assert_eq!(style.declarations[1].value, "8px");
                assert!(!style.declarations[1].important);
                assert!(style
                    .declarations
                    .iter()
                    .all(|d| d.category == ValueCategory::Declared));
            }
            other => panic!("expected style rule, got {other:?}"),
        }
    }

    #[test]
    fn media_font_face_and_keyframes_lower() {
        let css = "@media (min-width: 400px) { body { margin: 0 } } \
                   @font-face { font-family: X; src: url(a.woff2) } \
                   @keyframes spin { from { transform: rotate(0) } to { transform: rotate(1turn) } }";
        let (rules, reporter) = lower(css);
        assert!(reporter.is_empty(), "{reporter:?}");
        assert_eq!(rules.len(), 3);
        match &rules[0] {
            CasaRule::Media(media) => {
                assert_eq!(media.condition, "(min-width: 400px)");
                assert_eq!(media.rules.len(), 1);
            }
            other => panic!("expected media rule, got {other:?}"),
        }
        assert!(matches!(&rules[1], CasaRule::FontFace(_)));
        match &rules[2] {
            CasaRule::Keyframes(keyframes) => {
                assert_eq!(keyframes.name, "spin");
                assert_eq!(keyframes.steps.len(), 2);
                assert_eq!(keyframes.steps[0].offsets, "from");
                assert_eq!(keyframes.steps[1].offsets, "to");
            }
            other => panic!("expected keyframes, got {other:?}"),
        }
    }

    #[test]
    fn invalid_declaration_and_unknown_at_rule_are_recorded_not_dropped() {
        let (rules, reporter) = lower("body { color red; } @unknown x {}");
        assert_eq!(rules.len(), 2); // style rule + unsupported at-rule
        let diagnostics = reporter.diagnostics();
        assert_eq!(diagnostics[0].code, "CASA-SYN-001");
        assert_eq!(diagnostics[1].code, "CASA-IR-003");
    }

    #[test]
    fn url_resources_are_discovered_with_property_kind() {
        let css = "@font-face { src: url(font.woff2) } \
                   body { background-image: url(\"bg.png\"), url(icon.svg) } \
                   div { --x: url(data.bin) }";
        let mut reporter = Reporter::new();
        let mut resources = Vec::new();
        let mut lowering = Lowering::new("test.css", css, &mut reporter, &mut resources);
        let _ = lowering.rules(goya_css_syntax::parse_stylesheet(css));
        assert_eq!(resources.len(), 4);
        assert_eq!(resources[0].kind, ResourceKind::Font);
        assert_eq!(resources[0].url, "font.woff2");
        assert_eq!(resources[1].kind, ResourceKind::Image);
        assert_eq!(resources[1].url, "bg.png");
        assert_eq!(resources[3].kind, ResourceKind::Other);
    }

    #[test]
    fn parsed_stylesheet_round_trips_through_fromstr() {
        let parsed = ParsedStylesheet::from_str("body { color: red }").unwrap();
        assert_eq!(parsed.rules.len(), 1);
    }
}
