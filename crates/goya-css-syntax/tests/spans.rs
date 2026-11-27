//! Byte spans, which css-parsing-tests does not check.

use goya_css_syntax::*;

fn slice(input: &str, span: Span) -> &str {
    &input[span.start..span.end]
}

#[test]
fn rules_and_declarations_have_exact_spans() {
    let input = "/* c */\n#a, .b > p {\n  left: 6.25% ;\n  color : red !important;\n  broken\n}\n@media x { p {} }\n";
    let rules = parse_stylesheet(input);
    assert_eq!(rules.len(), 2);
    let Rule::Qualified(q) = &rules[0] else {
        panic!()
    };
    assert_eq!(
        slice(input, q.span),
        "#a, .b > p {\n  left: 6.25% ;\n  color : red !important;\n  broken\n}"
    );
    assert_eq!(input[q.span.start..q.block.span.start].trim(), "#a, .b > p");
    assert_eq!(q.block.inner_end, input.find("\n}").unwrap() + 1);
    let items = block_declarations(&q.block);
    let summary: Vec<String> = items
        .iter()
        .map(|i| match i {
            DeclarationItem::Declaration(d) => {
                format!(
                    "{}={}|{}",
                    d.name,
                    slice(input, d.value_span).trim(),
                    d.important
                )
            }
            DeclarationItem::Invalid { span } => format!("invalid:{}", slice(input, *span).trim()),
            DeclarationItem::At(a) => format!("@{}", a.name),
        })
        .collect();
    assert_eq!(
        summary,
        [
            "left=6.25%|false",
            "color=red !important|true",
            "invalid:broken"
        ]
    );
    let Rule::At(a) = &rules[1] else { panic!() };
    assert_eq!(slice(input, a.span), "@media x { p {} }");
}

#[test]
fn crlf_and_nul_are_preprocessed_but_spans_stay_on_input_bytes() {
    let input = "a\r\n{x:\0}";
    let tokens = tokenize(input);
    assert_eq!(tokens[1], (Token::Whitespace, Span { start: 1, end: 3 }));
    let Rule::Qualified(q) = &parse_stylesheet(input)[0] else {
        panic!()
    };
    let DeclarationItem::Declaration(d) = &block_declarations(&q.block)[0] else {
        panic!()
    };
    assert_eq!(
        d.value,
        vec![ComponentValue::Token(
            Token::Ident("\u{fffd}".into()),
            Span { start: 6, end: 7 }
        )]
    );
}

#[test]
fn a_rule_cut_off_by_the_end_of_input_is_reported() {
    let rules = parse_stylesheet("p { a: b }\n.unfinished");
    assert!(matches!(rules[1], Rule::Invalid { span } if span == Span { start: 11, end: 22 }));
}

#[test]
fn unterminated_strings_and_urls_are_flagged() {
    let tokens = tokenize("'abc");
    assert_eq!(
        tokens[0].0,
        Token::String {
            value: "abc".into(),
            terminated: false
        }
    );
    let tokens = tokenize("url(x");
    assert_eq!(
        tokens[0].0,
        Token::Url {
            value: "x".into(),
            terminated: false
        }
    );
}
