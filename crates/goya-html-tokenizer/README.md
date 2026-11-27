# goya-html-tokenizer

The WHATWG HTML tokenizer (HTML Standard §13.2.5) for Goya, with no
dependencies. It produces doctype, start-tag, end-tag, comment,
processing-instruction and text tokens. Every token, and each attribute's name
and value, has an exact byte span into the original input. Parse errors are
reported with their HTML Standard codes.

```rust
use goya_html_tokenizer::{tokenize_document, Token};

let (tokens, errors) = tokenize_document("<p id=a>Hi &amp; bye</p>");
assert!(matches!(&tokens[0].token, Token::StartTag { name, .. } if name == "p"));
```

`tokenize_document` switches content models as the HTML tree builder would
for `script`, `style`, `title`, `textarea`, `xmp`, `iframe`, `noembed`,
`noframes` and `plaintext`. For full control, use `Tokenizer` directly
(`set_state`, `set_last_start_tag`, `html_content_model`, `cdata_allowed`).

## Scope

* **Included:** all 80 tokenizer states, the processing-instruction states
  added to the Standard in 2025, named and numeric character references (the
  full 2,231-entry table), input-stream preprocessing (CR LF and CR become
  LF), and the parse errors defined for those stages.
* **Not included:** tree construction. Implied end tags, foster parenting,
  the adoption agency and foreign-content switching (CDATA sections are off
  unless `cdata_allowed` is set) are the tree builder's job. Encoding
  sniffing is out of scope too: the input is already a `&str`.

## Conformance

`tests/conformance.rs` runs the html5lib-tests tokenizer suite (commit
`c5495c5`, vendored in `tests/html5lib-tokenizer/`). Each test runs once per
initial state:

* **Tokens:** 7,042 of 7,042 runs match.
* **Parse-error codes:** 7,042 of 7,042 match (as multisets; positions not
  compared).
* **Skipped:** 4 tests, whose input contains a lone UTF-16 surrogate. A Rust
  string and a UTF-8 document cannot contain one.

`tests/spans.rs` checks the byte spans, which the suite does not cover.

## Third-party material

* `src/entities.rs` is generated from the WHATWG
  [`entities.json`](https://html.spec.whatwg.org/entities.json) (SHA-256
  `d741d877…f362e6`, fetched 26 November 2026). It is part of the HTML
  Standard, © WHATWG (Apple, Google, Mozilla, Microsoft), under
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
* `tests/html5lib-tokenizer/` is from
  [html5lib-tests](https://github.com/html5lib/html5lib-tests) under the MIT
  licence (`tests/html5lib-tokenizer/LICENSE`).

The crate itself is MIT OR Apache-2.0, as the workspace is.
