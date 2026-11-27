# goya-css-syntax

CSS Syntax Level 3 for Goya, with no dependencies. It provides a tokenizer
and a parser for component values, rules and declarations. Every token, block,
rule and declaration has an exact byte span into the original input.

```rust
use goya_css_syntax::{parse_stylesheet, block_declarations, Rule, DeclarationItem};

for rule in parse_stylesheet("#a { left: 6.25% }") {
    if let Rule::Qualified(q) = rule {
        for item in block_declarations(&q.block) {
            if let DeclarationItem::Declaration(d) = item {
                assert_eq!(d.name, "left");
            }
        }
    }
}
```

## Behaviour

* It follows the 2021 Candidate Recommendation draft of CSS Syntax Level 3
  (§4 tokenizing, §5 parsing), including `unicode-range` tokens. Input
  preprocessing happens on the fly (CR LF, CR and FF become LF; NUL becomes
  U+FFFD), and spans stay on the original bytes.
* Where the specification discards something, this crate returns it as
  `Invalid` with its span:
  * a declaration that does not parse (`DeclarationItem::Invalid`)
  * a qualified rule cut off by the end of input (`Rule::Invalid`)

  Callers can record what they could not read instead of losing it.
* Strings and URLs that the input ends inside are flagged
  (`terminated: false`).
* **Not included:** the CSS Nesting "block contents" algorithm (qualified
  rules inside style blocks), and `@charset` byte decoding (the input is
  already a `&str`).

## Conformance

`tests/conformance.rs` runs Simon Sapin's
[css-parsing-tests](https://github.com/SimonSapin/css-parsing-tests) (commit
`203ce36`, CC0, vendored in `tests/css-parsing-tests/`). Every case passes in:

* `component_value_list`
* `one_component_value`
* `declaration_list`
* `one_declaration`
* `one_rule`
* `rule_list`
* `stylesheet`

That is 136 cases. `blocks_contents.json` (CSS Nesting) and the colour and
An+B files are not vendored, because those features are out of scope.

`tests/spans.rs` checks the byte spans, which the suite does not cover.

The crate is MIT OR Apache-2.0, as the workspace is.
