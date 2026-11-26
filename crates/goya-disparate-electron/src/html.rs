//! HTML pass (`goya-html-tokenizer`). Every start tag becomes a `component`
//! with its attributes, direct text and enclosing element. Each component has
//! the exact byte span of its start tag.
//!
//! This is a tokenizer pass, not a tree builder: implied end tags, foster
//! parenting and the other tree-construction repairs are not applied, so
//! parents come from the literal tag nesting. Content models (script, style,
//! title, textarea and similar) are switched as the HTML tree builder would.

use crate::emit::{s, Emitter, Origin, Semantics, Status};
use goya_fair::{ElementKind, SemanticsValue};
use goya_html_tokenizer::{tokenize_document, Span, Token};
use std::collections::BTreeMap;

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

#[derive(Default)]
pub struct HtmlOutput {
    /// `src` of each external `<script>`, as written, with its element ID.
    pub scripts: Vec<(String, String)>,
    /// `href` of each `<link rel="stylesheet">`, as written, with its element ID.
    pub stylesheets: Vec<(String, String)>,
}

struct Open {
    tag: String,
    element: usize,
    text: String,
}

fn origin(span: Span) -> Origin {
    Origin::Span(span.start as u32, span.end as u32)
}

fn close(em: &mut Emitter, open: Open) {
    let text = open.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return;
    }
    if open.tag == "script" || open.tag == "style" {
        let provenance = em.elements[open.element].provenance.clone();
        let (observed, reason) = if open.tag == "script" {
            ("inline_script", "INLINE_SCRIPT_NOT_ANALYSED")
        } else {
            ("inline_style", "INLINE_STYLE_NOT_ANALYSED")
        };
        let mut sem = Semantics::new();
        sem.insert("length".into(), SemanticsValue::Int(text.len() as i64));
        let at = match (provenance.span_start, provenance.span_end) {
            (Some(a), Some(b)) => Origin::Span(a as u32, b as u32),
            _ => Origin::Line {
                line: provenance.line,
                fallback: true,
            },
        };
        em.unknown(
            observed,
            at,
            reason,
            "inline content is not analysed by this extractor version",
            sem,
        );
    } else {
        em.elements[open.element]
            .semantics
            .insert("text".into(), s(text));
    }
}

pub fn extract(text: &str, em: &mut Emitter) -> HtmlOutput {
    let (tokens, errors) = tokenize_document(text);
    let mut out = HtmlOutput::default();
    let mut stack: Vec<Open> = Vec::new();
    for spanned in tokens {
        match spanned.token {
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } => {
                let mut attrs = BTreeMap::new();
                let mut inline_handlers = Vec::new();
                for attr in &attributes {
                    if attr.name.len() > 2 && attr.name.starts_with("on") {
                        inline_handlers.push((attr.name.clone(), attr.value.clone()));
                    }
                    attrs.insert(attr.name.clone(), s(attr.value.clone()));
                }
                let attr = |k: &str| {
                    attributes
                        .iter()
                        .find(|a| a.name == k)
                        .map(|a| a.value.clone())
                };
                let script = if name == "script" { attr("src") } else { None };
                let stylesheet = if name == "link"
                    && attr("rel").is_some_and(|r| {
                        r.split_whitespace()
                            .any(|t| t.eq_ignore_ascii_case("stylesheet"))
                    }) {
                    attr("href")
                } else {
                    None
                };
                let mut sem = Semantics::new();
                sem.insert("tag".into(), s(name.clone()));
                if let Some(parent) = stack.last() {
                    sem.insert("parent".into(), s(em.elements[parent.element].id.clone()));
                }
                sem.insert("attributes".into(), SemanticsValue::Map(attrs));
                let index = em.emit(
                    ElementKind::Component,
                    origin(spanned.span),
                    Status::Extracted,
                    sem,
                );
                let id = em.elements[index].id.clone();
                if let Some(src) = script {
                    out.scripts.push((src, id.clone()));
                }
                if let Some(href) = stylesheet {
                    out.stylesheets.push((href, id.clone()));
                }
                for (event, code) in inline_handlers {
                    let mut hs = Semantics::new();
                    hs.insert("event".into(), s(&event[2..]));
                    hs.insert("via".into(), s("attribute"));
                    hs.insert("element".into(), s(id.clone()));
                    hs.insert("code".into(), s(code));
                    em.emit(
                        ElementKind::EventHandler,
                        origin(spanned.span),
                        Status::Undetermined {
                            reason: "INLINE_HANDLER_NOT_ANALYSED",
                            detail: "inline handler code is not analysed by this extractor version"
                                .into(),
                        },
                        hs,
                    );
                }
                if !(VOID.contains(&name.as_str()) || self_closing) {
                    stack.push(Open {
                        tag: name,
                        element: index,
                        text: String::new(),
                    });
                }
            }
            Token::EndTag { name, .. } => {
                if let Some(pos) = stack.iter().rposition(|open| open.tag == name) {
                    while stack.len() > pos {
                        let open = stack.pop().expect("non-empty");
                        close(em, open);
                    }
                }
            }
            Token::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t);
                }
            }
            Token::Comment(_) | Token::Doctype { .. } | Token::ProcessingInstruction { .. } => {}
        }
    }
    while let Some(open) = stack.pop() {
        close(em, open);
    }
    // A tag the input ended inside is dropped by the HTML Standard. Record it.
    for error in errors.iter().filter(|e| e.code == "eof-in-tag") {
        let line = em.line_of(error.offset);
        em.unknown(
            "html_unterminated_tag",
            Origin::Line {
                line,
                fallback: true,
            },
            "PARSE_FAILED",
            "the input ended inside a tag, which the HTML Standard drops",
            Semantics::new(),
        );
    }
    out
}
