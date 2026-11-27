//! Byte spans, which the html5lib suite does not check.

use goya_html_tokenizer::{tokenize_document, Span, Token};

fn slice(input: &str, span: Span) -> &str {
    &input[span.start..span.end]
}

#[test]
fn tags_attributes_and_text_have_exact_spans() {
    let input = "<p id=\"a\" title='x &amp; y' hidden data-n=7>Hi &copy;</p>";
    let (tokens, errors) = tokenize_document(input);
    assert!(errors.is_empty(), "{errors:?}");
    let Token::StartTag { attributes, .. } = &tokens[0].token else {
        panic!()
    };
    assert_eq!(
        slice(input, tokens[0].span),
        "<p id=\"a\" title='x &amp; y' hidden data-n=7>"
    );
    let spans: Vec<(&str, Option<&str>)> = attributes
        .iter()
        .map(|a| {
            (
                slice(input, a.name_span),
                a.value_span.map(|v| slice(input, v)),
            )
        })
        .collect();
    assert_eq!(
        spans,
        [
            ("id", Some("a")),
            ("title", Some("x &amp; y")),
            ("hidden", None),
            ("data-n", Some("7"))
        ]
    );
    assert_eq!(attributes[1].value, "x & y");
    assert_eq!(tokens[1].token, Token::Text("Hi ©".into()));
    assert_eq!(slice(input, tokens[1].span), "Hi &copy;");
    assert_eq!(slice(input, tokens[2].span), "</p>");
}

#[test]
fn a_tag_across_lines_starts_at_its_open_bracket() {
    let input = "<main>\n  <input id=a\n         value=\"1\">\n</main>";
    let (tokens, _) = tokenize_document(input);
    let input_tag = tokens
        .iter()
        .find(|t| matches!(&t.token, Token::StartTag { name, .. } if name == "input"))
        .unwrap();
    assert_eq!(input_tag.span.start, input.find("<input").unwrap());
    assert!(slice(input, input_tag.span).ends_with("value=\"1\">"));
}

#[test]
fn crlf_reads_as_one_newline_and_spans_cover_both_bytes() {
    let input = "<b\r\nclass=c>x\r\ny</b>";
    let (tokens, _) = tokenize_document(input);
    assert_eq!(slice(input, tokens[0].span), "<b\r\nclass=c>");
    assert_eq!(tokens[1].token, Token::Text("x\ny".into()));
    assert_eq!(slice(input, tokens[1].span), "x\r\ny");
}

#[test]
fn script_and_style_content_is_raw_text() {
    let input =
        "<script>if (a < b) { x = '</p>'; }</script><style>p > a {}</style><title>&amp;</title>";
    let (tokens, _) = tokenize_document(input);
    let kinds: Vec<String> = tokens
        .iter()
        .map(|t| match &t.token {
            Token::StartTag { name, .. } => format!("<{name}>"),
            Token::EndTag { name, .. } => format!("</{name}>"),
            Token::Text(s) => s.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "<script>",
            "if (a < b) { x = '</p>'; }",
            "</script>",
            "<style>",
            "p > a {}",
            "</style>",
            "<title>",
            "&",
            "</title>"
        ]
    );
}

#[test]
fn an_unterminated_tag_is_reported_not_invented() {
    let (tokens, errors) = tokenize_document("<p>ok</p><div class=\"x");
    assert_eq!(tokens.len(), 3);
    assert_eq!(errors.last().map(|e| e.code), Some("eof-in-tag"));
}
