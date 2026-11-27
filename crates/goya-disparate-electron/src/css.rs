//! CSS pass (`goya-css-syntax`). Each qualified rule becomes a `style_rule`
//! with its selector text and its declarations in source order, duplicates
//! kept. Declarations the CSS specification discards are kept as `unparsed`.
//! At-rules, and rules the input ended inside, are recorded as not analysed.

use crate::emit::{list, s, Emitter, Origin, Semantics, Status};
use goya_css_syntax::{block_declarations, parse_stylesheet, DeclarationItem, Rule, Span};
use goya_fair::{ElementKind, SemanticsValue};
use std::collections::BTreeMap;

fn origin(span: Span) -> Origin {
    Origin::Span(span.start as u32, span.end as u32)
}

pub fn extract(text: &str, em: &mut Emitter) {
    let slice = |span: Span| text[span.start..span.end].trim().to_string();
    for rule in parse_stylesheet(text) {
        match rule {
            Rule::At(at) => {
                let mut sem = Semantics::new();
                sem.insert("at_rule".into(), s(format!("@{}", at.name)));
                em.unknown(
                    &format!("css_at_rule:@{}", at.name),
                    origin(at.span),
                    "CSS_AT_RULE_NOT_ANALYSED",
                    "at-rules are not analysed by this extractor version",
                    sem,
                );
            }
            Rule::Qualified(rule) => {
                let selector = text[rule.span.start..rule.block.span.start]
                    .trim()
                    .to_string();
                let mut declarations = Vec::new();
                for item in block_declarations(&rule.block) {
                    let mut decl = BTreeMap::new();
                    match item {
                        DeclarationItem::Declaration(d) => {
                            decl.insert("name".into(), s(d.name));
                            decl.insert("value".into(), s(slice(d.value_span)));
                        }
                        DeclarationItem::Invalid { span } => {
                            decl.insert("unparsed".into(), s(slice(span)));
                        }
                        DeclarationItem::At(at) => {
                            decl.insert("unparsed".into(), s(slice(at.span)));
                        }
                    }
                    declarations.push(SemanticsValue::Map(decl));
                }
                let mut sem = Semantics::new();
                sem.insert("selector".into(), s(selector));
                sem.insert("declarations".into(), list(declarations));
                em.emit(
                    ElementKind::StyleRule,
                    origin(rule.span),
                    Status::Extracted,
                    sem,
                );
            }
            Rule::Invalid { span } => {
                let mut sem = Semantics::new();
                sem.insert("selector".into(), s(slice(span)));
                sem.insert("declarations".into(), list(Vec::new()));
                em.emit(
                    ElementKind::StyleRule,
                    origin(span),
                    Status::Undetermined {
                        reason: "PARSE_FAILED",
                        detail: "a rule prelude without a declaration block".into(),
                    },
                    sem,
                );
            }
        }
    }
}
