//! `goya-css-syntax` — CSS Syntax Level 3 for Goya, with no dependencies.
//!
//! It provides tokens, component values, rules and declarations, each with a
//! byte span into the original input. It follows the 2021 Candidate
//! Recommendation draft of CSS Syntax Level 3 (§4 tokenizing, §5 parsing) and
//! is checked against the css-parsing-tests suite (`tests/conformance.rs`).
//!
//! Where the specification discards something (an invalid declaration, a
//! qualified rule cut off by the end of input), this crate returns an
//! `Invalid` item with its span. Callers can then record what they could not
//! read instead of losing it.

mod tokenizer;

pub use tokenizer::{tokenize, Numeric, Span, Token};

/// A `{}`, `[]` or `()` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The opening character: `{`, `[` or `(`.
    pub open: char,
    pub contents: Vec<ComponentValue>,
    /// From the opening token to the closing token, or to the end of input.
    pub span: Span,
    /// Where the contents end: the closing token's start, or the end of input.
    pub inner_end: usize,
    /// False when the input ended before the closing token.
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentValue {
    Token(Token, Span),
    Block(Block),
    Function {
        name: String,
        arguments: Vec<ComponentValue>,
        span: Span,
        closed: bool,
    },
}

impl ComponentValue {
    pub fn span(&self) -> Span {
        match self {
            Self::Token(_, span) | Self::Function { span, .. } => *span,
            Self::Block(block) => block.span,
        }
    }

    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Token(Token::Whitespace, _))
    }

    fn token(&self) -> Option<&Token> {
        match self {
            Self::Token(token, _) => Some(token),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AtRule {
    pub name: String,
    pub prelude: Vec<ComponentValue>,
    pub block: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedRule {
    pub prelude: Vec<ComponentValue>,
    pub block: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    Qualified(QualifiedRule),
    At(AtRule),
    /// A qualified rule the input ended inside. The specification drops it.
    Invalid {
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub name: String,
    pub name_span: Span,
    /// The value after the colon, whitespace kept, with a final `!important`
    /// removed.
    pub value: Vec<ComponentValue>,
    /// From the end of the colon to the end of the declaration (before `;`),
    /// including any `!important`.
    pub value_span: Span,
    pub important: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeclarationItem {
    Declaration(Declaration),
    At(AtRule),
    /// Input the specification discards as an invalid declaration.
    Invalid {
        span: Span,
    },
}

/// Why a single-item parse found nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleError {
    Empty,
    Invalid,
    ExtraInput,
}

fn join(a: Span, b: Span) -> Span {
    Span {
        start: a.start,
        end: b.end,
    }
}

/// A cursor over tokens that builds component values (§5.4.7–5.4.9).
struct TokenCursor {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    input_len: usize,
}

impl TokenCursor {
    fn new(input: &str) -> Self {
        Self {
            tokens: tokenize(input),
            pos: 0,
            input_len: input.len(),
        }
    }

    fn component_value(&mut self) -> Option<ComponentValue> {
        let (token, span) = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(match token {
            Token::LeftCurly => ComponentValue::Block(self.block('{', span)),
            Token::LeftSquare => ComponentValue::Block(self.block('[', span)),
            Token::LeftParen => ComponentValue::Block(self.block('(', span)),
            Token::Function(name) => {
                let mut arguments = Vec::new();
                loop {
                    match self.tokens.get(self.pos) {
                        None => {
                            return Some(ComponentValue::Function {
                                name,
                                arguments,
                                span: Span {
                                    start: span.start,
                                    end: self.input_len,
                                },
                                closed: false,
                            })
                        }
                        Some((Token::RightParen, close)) => {
                            let close = *close;
                            self.pos += 1;
                            return Some(ComponentValue::Function {
                                name,
                                arguments,
                                span: join(span, close),
                                closed: true,
                            });
                        }
                        Some(_) => arguments.extend(self.component_value()),
                    }
                }
            }
            token => ComponentValue::Token(token, span),
        })
    }

    fn block(&mut self, open: char, open_span: Span) -> Block {
        let closing = match open {
            '{' => Token::RightCurly,
            '[' => Token::RightSquare,
            _ => Token::RightParen,
        };
        let mut contents = Vec::new();
        loop {
            match self.tokens.get(self.pos) {
                None => {
                    return Block {
                        open,
                        contents,
                        span: Span {
                            start: open_span.start,
                            end: self.input_len,
                        },
                        inner_end: self.input_len,
                        closed: false,
                    }
                }
                Some((token, close)) if *token == closing => {
                    let close = *close;
                    self.pos += 1;
                    return Block {
                        open,
                        contents,
                        span: join(open_span, close),
                        inner_end: close.start,
                        closed: true,
                    };
                }
                Some(_) => contents.extend(self.component_value()),
            }
        }
    }

    fn all(mut self) -> Vec<ComponentValue> {
        let mut out = Vec::new();
        while let Some(cv) = self.component_value() {
            out.push(cv);
        }
        out
    }
}

/// Parses a list of component values (§5.3.10).
pub fn parse_component_values(input: &str) -> Vec<ComponentValue> {
    TokenCursor::new(input).all()
}

/// A cursor over component values, used by the rule and declaration
/// algorithms (§5.4.1–5.4.6). Component values stand in for tokens: a `{}`
/// block is the "simple block" the algorithms would consume.
struct Values<'v> {
    items: &'v [ComponentValue],
    pos: usize,
}

impl<'v> Values<'v> {
    fn peek(&self) -> Option<&'v ComponentValue> {
        self.items.get(self.pos)
    }

    fn next(&mut self) -> Option<&'v ComponentValue> {
        let item = self.items.get(self.pos);
        self.pos += 1;
        item
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(ComponentValue::is_whitespace) {
            self.pos += 1;
        }
    }

    fn rules(&mut self, top_level: bool) -> Vec<Rule> {
        let mut rules = Vec::new();
        while let Some(item) = self.peek() {
            match item.token() {
                Some(Token::Whitespace) => self.pos += 1,
                Some(Token::Cdo | Token::Cdc) if top_level => self.pos += 1,
                Some(Token::AtKeyword(_)) => rules.push(Rule::At(self.at_rule())),
                _ => rules.push(self.qualified_rule()),
            }
        }
        rules
    }

    fn at_rule(&mut self) -> AtRule {
        let first = self.next().expect("at-keyword");
        let Some(Token::AtKeyword(name)) = first.token() else {
            unreachable!("called on an at-keyword")
        };
        let mut prelude = Vec::new();
        let mut span = first.span();
        loop {
            match self.next() {
                None => break,
                Some(ComponentValue::Token(Token::Semicolon, s)) => {
                    span = join(span, *s);
                    break;
                }
                Some(ComponentValue::Block(block)) if block.open == '{' => {
                    return AtRule {
                        name: name.clone(),
                        prelude,
                        block: Some(block.clone()),
                        span: join(span, block.span),
                    };
                }
                Some(cv) => {
                    span = join(span, cv.span());
                    prelude.push(cv.clone());
                }
            }
        }
        AtRule {
            name: name.clone(),
            prelude,
            block: None,
            span,
        }
    }

    fn qualified_rule(&mut self) -> Rule {
        let mut prelude = Vec::new();
        let start = self.peek().map(ComponentValue::span).unwrap_or_default();
        let mut span = start;
        loop {
            match self.next() {
                None => return Rule::Invalid { span },
                Some(ComponentValue::Block(block)) if block.open == '{' => {
                    return Rule::Qualified(QualifiedRule {
                        prelude,
                        block: block.clone(),
                        span: join(start, block.span),
                    })
                }
                Some(cv) => {
                    span = join(start, cv.span());
                    prelude.push(cv.clone());
                }
            }
        }
    }

    /// §5.4.5 "consume a list of declarations".
    fn declarations(&mut self, end_of_list: usize) -> Vec<DeclarationItem> {
        let mut items = Vec::new();
        while let Some(item) = self.peek() {
            match item.token() {
                Some(Token::Whitespace | Token::Semicolon) => self.pos += 1,
                Some(Token::AtKeyword(_)) => items.push(DeclarationItem::At(self.at_rule())),
                Some(Token::Ident(_)) => {
                    let start = self.pos;
                    while self
                        .peek()
                        .is_some_and(|cv| cv.token() != Some(&Token::Semicolon))
                    {
                        self.pos += 1;
                    }
                    let end = self.peek().map(|cv| cv.span().start).unwrap_or(end_of_list);
                    let run = &self.items[start..self.pos];
                    items.push(match declaration(run, end) {
                        Some(d) => DeclarationItem::Declaration(d),
                        None => DeclarationItem::Invalid {
                            span: run_span(run),
                        },
                    });
                }
                _ => {
                    let start = self.pos;
                    while self
                        .peek()
                        .is_some_and(|cv| cv.token() != Some(&Token::Semicolon))
                    {
                        self.pos += 1;
                    }
                    items.push(DeclarationItem::Invalid {
                        span: run_span(&self.items[start..self.pos]),
                    });
                }
            }
        }
        items
    }
}

fn run_span(run: &[ComponentValue]) -> Span {
    match (run.first(), run.last()) {
        (Some(a), Some(b)) => join(a.span(), b.span()),
        _ => Span::default(),
    }
}

/// §5.4.6 "consume a declaration" over one `;`-free run. `end` is the byte
/// offset where the declaration ends (the `;`, the block end or the input end).
fn declaration(run: &[ComponentValue], end: usize) -> Option<Declaration> {
    let (name, name_span) = match run.first()? {
        ComponentValue::Token(Token::Ident(name), span) => (name.clone(), *span),
        _ => return None,
    };
    let mut i = 1;
    while run.get(i).is_some_and(ComponentValue::is_whitespace) {
        i += 1;
    }
    let colon = match run.get(i)? {
        ComponentValue::Token(Token::Colon, span) => *span,
        _ => return None,
    };
    i += 1;
    // The value keeps its whitespace, as css-parsing-tests expects. A final
    // `! important` is removed together with everything after the `!`.
    let mut value: Vec<ComponentValue> = run[i..].to_vec();
    let mut important = false;
    let non_ws: Vec<usize> = (0..value.len())
        .filter(|&k| !value[k].is_whitespace())
        .collect();
    if let [.., bang, last] = non_ws[..] {
        let is_bang = value[bang].token() == Some(&Token::Delim('!'));
        let is_important = matches!(value[last].token(), Some(Token::Ident(s)) if s.eq_ignore_ascii_case("important"));
        if is_bang && is_important {
            value.truncate(bang);
            important = true;
        }
    }
    Some(Declaration {
        name,
        name_span,
        value,
        value_span: Span {
            start: colon.end,
            end: end.max(colon.end),
        },
        important,
        span: Span {
            start: name_span.start,
            end: end.max(colon.end),
        },
    })
}

/// Parses a stylesheet (§5.3.3): top-level rules, ignoring `<!--` and `-->`.
pub fn parse_stylesheet(input: &str) -> Vec<Rule> {
    let values = parse_component_values(input);
    Values {
        items: &values,
        pos: 0,
    }
    .rules(true)
}

/// Parses a list of rules (§5.3.4).
pub fn parse_rule_list(input: &str) -> Vec<Rule> {
    let values = parse_component_values(input);
    Values {
        items: &values,
        pos: 0,
    }
    .rules(false)
}

/// Parses a list of declarations (§5.3.8).
pub fn parse_declaration_list(input: &str) -> Vec<DeclarationItem> {
    let values = parse_component_values(input);
    Values {
        items: &values,
        pos: 0,
    }
    .declarations(input.len())
}

/// Parses a block's contents as a list of declarations, for example a style
/// rule's `{}` block.
pub fn block_declarations(block: &Block) -> Vec<DeclarationItem> {
    Values {
        items: &block.contents,
        pos: 0,
    }
    .declarations(block.inner_end)
}

/// Parses one rule (§5.3.5).
pub fn parse_one_rule(input: &str) -> Result<Rule, SingleError> {
    let values = parse_component_values(input);
    let mut v = Values {
        items: &values,
        pos: 0,
    };
    v.skip_whitespace();
    let rule = match v.peek().ok_or(SingleError::Empty)?.token() {
        Some(Token::AtKeyword(_)) => Rule::At(v.at_rule()),
        _ => match v.qualified_rule() {
            Rule::Invalid { .. } => return Err(SingleError::Invalid),
            rule => rule,
        },
    };
    v.skip_whitespace();
    if v.peek().is_some() {
        return Err(SingleError::ExtraInput);
    }
    Ok(rule)
}

/// Parses one declaration (§5.3.6).
pub fn parse_one_declaration(input: &str) -> Result<Declaration, SingleError> {
    let values = parse_component_values(input);
    let mut v = Values {
        items: &values,
        pos: 0,
    };
    v.skip_whitespace();
    match v.peek() {
        None => Err(SingleError::Empty),
        Some(ComponentValue::Token(Token::Ident(_), _)) => {
            declaration(&values[v.pos..], input.len()).ok_or(SingleError::Invalid)
        }
        Some(_) => Err(SingleError::Invalid),
    }
}

/// Parses one component value (§5.3.9).
pub fn parse_one_component_value(input: &str) -> Result<ComponentValue, SingleError> {
    let values = parse_component_values(input);
    let mut v = Values {
        items: &values,
        pos: 0,
    };
    v.skip_whitespace();
    let value = v.next().ok_or(SingleError::Empty)?.clone();
    v.skip_whitespace();
    if v.peek().is_some() {
        return Err(SingleError::ExtraInput);
    }
    Ok(value)
}

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
