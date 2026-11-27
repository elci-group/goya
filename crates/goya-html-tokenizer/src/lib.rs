//! `goya-html-tokenizer` — the WHATWG HTML tokenizer (HTML Standard §13.2.5)
//! for Goya, with no dependencies.
//!
//! It turns HTML into doctype, start-tag, end-tag, comment and text tokens.
//! Each token, and each attribute's name and value, carries a byte span into
//! the original input. It is checked against the html5lib-tests tokenizer
//! suite (`tests/conformance.rs`).
//!
//! It is a tokenizer, not a tree builder. A tree builder switches the
//! tokenizer into RCDATA, RAWTEXT, script data or PLAINTEXT after certain
//! start tags. [`Tokenizer::html_content_model`] does that switch for the
//! HTML-namespace elements whose content model is fixed by their name, which
//! is what a static reader of HTML documents needs. Implied end tags, foster
//! parenting and the other tree-construction repairs are not applied.
//!
//! Input preprocessing (§13.2.3.5) happens on the fly: CR LF and CR read as
//! one LF. Spans are byte offsets into the original input.

mod entities;

use std::collections::VecDeque;

/// An end-exclusive byte range in the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
    pub name_span: Span,
    /// The value between its quotes, or the unquoted value. `None` when the
    /// attribute has no value.
    pub value_span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Doctype {
        name: Option<String>,
        public_id: Option<String>,
        system_id: Option<String>,
        force_quirks: bool,
    },
    StartTag {
        name: String,
        attributes: Vec<Attribute>,
        self_closing: bool,
    },
    /// Attributes on an end tag are a parse error; they are kept here.
    EndTag {
        name: String,
        attributes: Vec<Attribute>,
    },
    Comment(String),
    /// `<?target data?>` (HTML Standard §13.2.5.72–76).
    ProcessingInstruction {
        target: String,
        data: String,
    },
    /// Consecutive characters, merged.
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned {
    pub token: Token,
    pub span: Span,
}

/// A parse error, by its HTML Standard code, at a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub code: &'static str,
    pub offset: usize,
}

/// The states a caller may start in or switch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialState {
    Data,
    Rcdata,
    Rawtext,
    ScriptData,
    Plaintext,
    CdataSection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    Rcdata,
    Rawtext,
    ScriptData,
    Plaintext,
    TagOpen,
    EndTagOpen,
    TagName,
    RcdataLessThan,
    RcdataEndTagOpen,
    RcdataEndTagName,
    RawtextLessThan,
    RawtextEndTagOpen,
    RawtextEndTagName,
    ScriptDataLessThan,
    ScriptDataEndTagOpen,
    ScriptDataEndTagName,
    ScriptDataEscapeStart,
    ScriptDataEscapeStartDash,
    ScriptDataEscaped,
    ScriptDataEscapedDash,
    ScriptDataEscapedDashDash,
    ScriptDataEscapedLessThan,
    ScriptDataEscapedEndTagOpen,
    ScriptDataEscapedEndTagName,
    ScriptDataDoubleEscapeStart,
    ScriptDataDoubleEscaped,
    ScriptDataDoubleEscapedDash,
    ScriptDataDoubleEscapedDashDash,
    ScriptDataDoubleEscapedLessThan,
    ScriptDataDoubleEscapeEnd,
    BeforeAttributeName,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    AttributeValueDouble,
    AttributeValueSingle,
    AttributeValueUnquoted,
    AfterAttributeValueQuoted,
    SelfClosingStartTag,
    BogusComment,
    ProcessingInstructionOpen,
    ProcessingInstructionTarget,
    AfterProcessingInstructionTarget,
    ProcessingInstructionData,
    ProcessingInstructionQuestionable,
    MarkupDeclarationOpen,
    CommentStart,
    CommentStartDash,
    Comment,
    CommentLessThan,
    CommentLessThanBang,
    CommentLessThanBangDash,
    CommentLessThanBangDashDash,
    CommentEndDash,
    CommentEnd,
    CommentEndBang,
    Doctype,
    BeforeDoctypeName,
    DoctypeName,
    AfterDoctypeName,
    AfterDoctypePublicKeyword,
    BeforeDoctypePublicIdentifier,
    DoctypePublicIdentifierDouble,
    DoctypePublicIdentifierSingle,
    AfterDoctypePublicIdentifier,
    BetweenDoctypePublicAndSystemIdentifiers,
    AfterDoctypeSystemKeyword,
    BeforeDoctypeSystemIdentifier,
    DoctypeSystemIdentifierDouble,
    DoctypeSystemIdentifierSingle,
    AfterDoctypeSystemIdentifier,
    BogusDoctype,
    CdataSection,
    CdataSectionBracket,
    CdataSectionEnd,
    CharacterReference,
    NamedCharacterReference,
    AmbiguousAmpersand,
    NumericCharacterReference,
    HexadecimalCharacterReferenceStart,
    DecimalCharacterReferenceStart,
    HexadecimalCharacterReference,
    DecimalCharacterReference,
    NumericCharacterReferenceEnd,
}

struct PendingAttribute {
    name: String,
    value: String,
    name_start: usize,
    name_end: usize,
    value_start: Option<usize>,
    value_end: usize,
}

struct PendingTag {
    start: bool,
    name: String,
    attributes: Vec<Attribute>,
    attribute: Option<PendingAttribute>,
    self_closing: bool,
}

struct PendingDoctype {
    name: Option<String>,
    public_id: Option<String>,
    system_id: Option<String>,
    force_quirks: bool,
}

const C1_REPLACEMENTS: [(u32, u32); 27] = [
    (0x80, 0x20AC),
    (0x82, 0x201A),
    (0x83, 0x0192),
    (0x84, 0x201E),
    (0x85, 0x2026),
    (0x86, 0x2020),
    (0x87, 0x2021),
    (0x88, 0x02C6),
    (0x89, 0x2030),
    (0x8A, 0x0160),
    (0x8B, 0x2039),
    (0x8C, 0x0152),
    (0x8E, 0x017D),
    (0x91, 0x2018),
    (0x92, 0x2019),
    (0x93, 0x201C),
    (0x94, 0x201D),
    (0x95, 0x2022),
    (0x96, 0x2013),
    (0x97, 0x2014),
    (0x98, 0x02DC),
    (0x99, 0x2122),
    (0x9A, 0x0161),
    (0x9B, 0x203A),
    (0x9C, 0x0153),
    (0x9E, 0x017E),
    (0x9F, 0x0178),
];

fn is_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | ' ')
}

fn is_noncharacter(n: u32) -> bool {
    (0xFDD0..=0xFDEF).contains(&n) || (n & 0xFFFE) == 0xFFFE
}

/// An HTML tokenizer over one input.
pub struct Tokenizer {
    chars: Vec<char>,
    offsets: Vec<usize>,
    pos: usize,
    last_eof: bool,
    done: bool,
    state: State,
    return_state: State,
    queue: VecDeque<Spanned>,
    errors: Vec<ParseError>,
    text: String,
    text_start: Option<usize>,
    text_end: usize,
    token_start: usize,
    tag: Option<PendingTag>,
    doctype: Option<PendingDoctype>,
    comment: String,
    pi_data: String,
    buffer: String,
    reference_start: usize,
    code: u32,
    last_start_tag: Option<String>,
    html_content_model: bool,
    cdata_allowed: bool,
}

impl Tokenizer {
    pub fn new(input: &str) -> Self {
        let mut chars = Vec::with_capacity(input.len());
        let mut offsets = Vec::with_capacity(input.len() + 1);
        let mut it = input.char_indices().peekable();
        while let Some((i, c)) = it.next() {
            offsets.push(i);
            if c == '\r' {
                if let Some((_, '\n')) = it.peek() {
                    it.next();
                }
                chars.push('\n');
            } else {
                chars.push(c);
            }
        }
        offsets.push(input.len());
        // §13.2.3.5: controls (other than ASCII whitespace and NUL) and
        // noncharacters in the input stream are parse errors.
        let mut errors = Vec::new();
        for (k, &c) in chars.iter().enumerate() {
            let n = c as u32;
            let control = (n < 0x20 || (0x7F..=0x9F).contains(&n))
                && !matches!(n, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20);
            if control {
                errors.push(ParseError {
                    code: "control-character-in-input-stream",
                    offset: offsets[k],
                });
            } else if is_noncharacter(n) {
                errors.push(ParseError {
                    code: "noncharacter-in-input-stream",
                    offset: offsets[k],
                });
            }
        }
        Self {
            chars,
            offsets,
            pos: 0,
            last_eof: false,
            done: false,
            state: State::Data,
            return_state: State::Data,
            queue: VecDeque::new(),
            errors,
            text: String::new(),
            text_start: None,
            text_end: 0,
            token_start: 0,
            tag: None,
            doctype: None,
            comment: String::new(),
            pi_data: String::new(),
            buffer: String::new(),
            reference_start: 0,
            code: 0,
            last_start_tag: None,
            html_content_model: false,
            cdata_allowed: false,
        }
    }

    /// After a start tag for `script`, `style`, `title`, `textarea`, `xmp`,
    /// `iframe`, `noembed`, `noframes` or `plaintext`, switch to the state the
    /// HTML tree builder would choose. Off by default.
    pub fn html_content_model(mut self, on: bool) -> Self {
        self.html_content_model = on;
        self
    }

    /// Lets `<![CDATA[` open a CDATA section, as it does in foreign content.
    /// Off by default, where it is a bogus comment.
    pub fn cdata_allowed(mut self, on: bool) -> Self {
        self.cdata_allowed = on;
        self
    }

    /// Switches state, as a tree builder does.
    pub fn set_state(&mut self, state: InitialState) {
        self.state = match state {
            InitialState::Data => State::Data,
            InitialState::Rcdata => State::Rcdata,
            InitialState::Rawtext => State::Rawtext,
            InitialState::ScriptData => State::ScriptData,
            InitialState::Plaintext => State::Plaintext,
            InitialState::CdataSection => State::CdataSection,
        };
    }

    /// Sets the last start tag name, which decides which end tag closes
    /// RCDATA, RAWTEXT and script data.
    pub fn set_last_start_tag(&mut self, name: Option<&str>) {
        self.last_start_tag = name.map(str::to_string);
    }

    /// Parse errors so far.
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// The next token, or `None` at the end of input.
    pub fn next_token(&mut self) -> Option<Spanned> {
        while self.queue.is_empty() && !self.done {
            self.step();
        }
        self.queue.pop_front()
    }

    // ---- input ------------------------------------------------------------

    fn consume(&mut self) -> Option<char> {
        match self.chars.get(self.pos) {
            Some(&c) => {
                self.pos += 1;
                self.last_eof = false;
                Some(c)
            }
            None => {
                self.last_eof = true;
                None
            }
        }
    }

    fn reconsume(&mut self, state: State) {
        if !self.last_eof {
            self.pos -= 1;
        }
        self.state = state;
    }

    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.pos + k).copied()
    }

    fn lookahead(&self, word: &str, case_insensitive: bool) -> bool {
        word.chars().enumerate().all(|(k, w)| match self.peek(k) {
            Some(c) if case_insensitive => c.eq_ignore_ascii_case(&w),
            Some(c) => c == w,
            None => false,
        })
    }

    fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    fn here(&self) -> usize {
        self.offsets[self.pos.min(self.offsets.len() - 1)]
    }

    fn error(&mut self, code: &'static str) {
        let offset = self.offsets[self.pos.saturating_sub(1).min(self.offsets.len() - 1)];
        self.errors.push(ParseError { code, offset });
    }

    // ---- output -----------------------------------------------------------

    fn emit_char_from(&mut self, c: char, start: usize) {
        if self.text_start.is_none() {
            self.text_start = Some(start);
        }
        self.text.push(c);
        self.text_end = self.pos;
    }

    fn emit_char(&mut self, c: char) {
        let start = self.pos.saturating_sub(1);
        self.emit_char_from(c, start);
    }

    fn emit_str(&mut self, s: &str) {
        let start = self.pos.saturating_sub(s.chars().count());
        for c in s.chars() {
            self.emit_char_from(c, start);
        }
    }

    fn flush_text(&mut self) {
        if let Some(start) = self.text_start.take() {
            let token = Token::Text(std::mem::take(&mut self.text));
            let span = Span {
                start: self.offsets[start],
                end: self.offsets[self.text_end],
            };
            self.queue.push_back(Spanned { token, span });
        }
    }

    fn emit(&mut self, token: Token) {
        self.flush_text();
        let span = Span {
            start: self.offsets[self.token_start],
            end: self.here(),
        };
        self.queue.push_back(Spanned { token, span });
    }

    fn emit_eof(&mut self) {
        self.flush_text();
        self.done = true;
    }

    // ---- tags -------------------------------------------------------------

    fn new_tag(&mut self, start: bool) {
        self.tag = Some(PendingTag {
            start,
            name: String::new(),
            attributes: Vec::new(),
            attribute: None,
            self_closing: false,
        });
    }

    fn tag(&mut self) -> &mut PendingTag {
        self.tag.as_mut().expect("a tag is being built")
    }

    fn finish_attribute(&mut self) {
        let offsets = &self.offsets;
        let Some(tag) = self.tag.as_mut() else { return };
        let Some(a) = tag.attribute.take() else {
            return;
        };
        if tag.attributes.iter().any(|x| x.name == a.name) {
            self.errors.push(ParseError {
                code: "duplicate-attribute",
                offset: offsets[a.name_start],
            });
            return;
        }
        tag.attributes.push(Attribute {
            name: a.name,
            value: a.value,
            name_span: Span {
                start: offsets[a.name_start],
                end: offsets[a.name_end],
            },
            value_span: a.value_start.map(|s| Span {
                start: offsets[s],
                end: offsets[a.value_end.max(s)],
            }),
        });
    }

    fn start_attribute(&mut self) {
        self.finish_attribute();
        let start = self.pos.saturating_sub(if self.last_eof { 0 } else { 1 });
        self.tag().attribute = Some(PendingAttribute {
            name: String::new(),
            value: String::new(),
            name_start: start,
            name_end: start,
            value_start: None,
            value_end: start,
        });
    }

    fn attribute(&mut self) -> &mut PendingAttribute {
        self.tag()
            .attribute
            .as_mut()
            .expect("an attribute is being built")
    }

    fn push_attribute_name(&mut self, c: char) {
        let end = self.pos;
        let a = self.attribute();
        a.name.push(c);
        a.name_end = end;
    }

    fn mark_value_start(&mut self, at: usize) {
        let a = self.attribute();
        a.value_start.get_or_insert(at);
        a.value_end = at;
    }

    fn push_attribute_value(&mut self, c: char) {
        let end = self.pos;
        let start = end.saturating_sub(1);
        let a = self.attribute();
        a.value_start.get_or_insert(start);
        a.value.push(c);
        a.value_end = end;
    }

    fn emit_tag(&mut self) {
        self.finish_attribute();
        let tag = self.tag.take().expect("a tag is being built");
        if tag.start {
            self.last_start_tag = Some(tag.name.clone());
            let name = tag.name.clone();
            self.emit(Token::StartTag {
                name: tag.name,
                attributes: tag.attributes,
                self_closing: tag.self_closing,
            });
            if self.html_content_model {
                self.state = match name.as_str() {
                    "title" | "textarea" => State::Rcdata,
                    "style" | "xmp" | "iframe" | "noembed" | "noframes" => State::Rawtext,
                    "script" => State::ScriptData,
                    "plaintext" => State::Plaintext,
                    _ => self.state,
                };
            }
        } else {
            if !tag.attributes.is_empty() {
                self.error("end-tag-with-attributes");
            }
            if tag.self_closing {
                self.error("end-tag-with-trailing-solidus");
            }
            self.emit(Token::EndTag {
                name: tag.name,
                attributes: tag.attributes,
            });
        }
    }

    fn appropriate_end_tag(&self) -> bool {
        match (&self.tag, &self.last_start_tag) {
            (Some(t), Some(last)) => !t.start && &t.name == last,
            _ => false,
        }
    }

    fn emit_comment(&mut self) {
        let data = std::mem::take(&mut self.comment);
        self.emit(Token::Comment(data));
    }

    /// "Convert the temporary buffer to a comment": `?` plus the buffer.
    fn buffer_to_comment(&mut self) {
        self.comment = format!("?{}", self.buffer);
    }

    fn emit_processing_instruction(&mut self) {
        let target = std::mem::take(&mut self.buffer);
        let data = std::mem::take(&mut self.pi_data);
        self.emit(Token::ProcessingInstruction { target, data });
    }

    fn new_doctype(&mut self) {
        self.doctype = Some(PendingDoctype {
            name: None,
            public_id: None,
            system_id: None,
            force_quirks: false,
        });
    }

    fn doctype(&mut self) -> &mut PendingDoctype {
        self.doctype.as_mut().expect("a doctype is being built")
    }

    fn emit_doctype(&mut self) {
        let d = self.doctype.take().expect("a doctype is being built");
        self.emit(Token::Doctype {
            name: d.name,
            public_id: d.public_id,
            system_id: d.system_id,
            force_quirks: d.force_quirks,
        });
    }

    fn in_attribute(&self) -> bool {
        matches!(
            self.return_state,
            State::AttributeValueDouble
                | State::AttributeValueSingle
                | State::AttributeValueUnquoted
        )
    }

    fn flush_reference(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        if self.in_attribute() {
            for c in buffer.chars() {
                self.push_attribute_value(c);
            }
        } else {
            let start = self.reference_start;
            for c in buffer.chars() {
                self.emit_char_from(c, start);
            }
        }
    }

    /// The text that ends an RCDATA, RAWTEXT or script end-tag attempt that
    /// was not appropriate: `</` plus the buffer.
    fn emit_failed_end_tag(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        self.emit_str("</");
        self.emit_str(&buffer);
        self.tag = None;
    }

    // ---- the state machine ------------------------------------------------

    fn step(&mut self) {
        use State::*;
        match self.state {
            Data => match self.consume() {
                Some('&') => {
                    self.return_state = Data;
                    self.state = CharacterReference;
                }
                Some('<') => {
                    self.token_start = self.pos - 1;
                    self.state = TagOpen;
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.emit_char('\0');
                }
                Some(c) => self.emit_char(c),
                None => self.emit_eof(),
            },
            Rcdata => match self.consume() {
                Some('&') => {
                    self.return_state = Rcdata;
                    self.state = CharacterReference;
                }
                Some('<') => {
                    self.token_start = self.pos - 1;
                    self.state = RcdataLessThan;
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.emit_char('\u{fffd}');
                }
                Some(c) => self.emit_char(c),
                None => self.emit_eof(),
            },
            Rawtext => match self.consume() {
                Some('<') => {
                    self.token_start = self.pos - 1;
                    self.state = RawtextLessThan;
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.emit_char('\u{fffd}');
                }
                Some(c) => self.emit_char(c),
                None => self.emit_eof(),
            },
            ScriptData => match self.consume() {
                Some('<') => {
                    self.token_start = self.pos - 1;
                    self.state = ScriptDataLessThan;
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.emit_char('\u{fffd}');
                }
                Some(c) => self.emit_char(c),
                None => self.emit_eof(),
            },
            Plaintext => match self.consume() {
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.emit_char('\u{fffd}');
                }
                Some(c) => self.emit_char(c),
                None => self.emit_eof(),
            },
            TagOpen => match self.consume() {
                Some('!') => self.state = MarkupDeclarationOpen,
                Some('/') => self.state = EndTagOpen,
                Some(c) if c.is_ascii_alphabetic() => {
                    self.new_tag(true);
                    self.reconsume(TagName);
                }
                Some('?') => {
                    self.buffer.clear();
                    self.state = ProcessingInstructionOpen;
                }
                None => {
                    self.error("eof-before-tag-name");
                    self.emit_str("<");
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("invalid-first-character-of-tag-name");
                    self.pos -= 1;
                    self.emit_str("<");
                    self.state = Data;
                }
            },
            EndTagOpen => match self.consume() {
                Some(c) if c.is_ascii_alphabetic() => {
                    self.new_tag(false);
                    self.reconsume(TagName);
                }
                Some('>') => {
                    self.error("missing-end-tag-name");
                    self.state = Data;
                }
                None => {
                    self.error("eof-before-tag-name");
                    self.emit_str("</");
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("invalid-first-character-of-tag-name");
                    self.comment.clear();
                    self.reconsume(BogusComment);
                }
            },
            TagName => match self.consume() {
                Some(c) if is_whitespace(c) => self.state = BeforeAttributeName,
                Some('/') => self.state = SelfClosingStartTag,
                Some('>') => {
                    self.state = Data;
                    self.emit_tag();
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.tag().name.push('\u{fffd}');
                }
                Some(c) => self.tag().name.push(c.to_ascii_lowercase()),
                None => {
                    self.error("eof-in-tag");
                    self.emit_eof();
                }
            },
            RcdataLessThan => self.less_than(Rcdata, RcdataEndTagOpen),
            RcdataEndTagOpen => self.end_tag_open(Rcdata, RcdataEndTagName),
            RcdataEndTagName => self.end_tag_name(Rcdata),
            RawtextLessThan => self.less_than(Rawtext, RawtextEndTagOpen),
            RawtextEndTagOpen => self.end_tag_open(Rawtext, RawtextEndTagName),
            RawtextEndTagName => self.end_tag_name(Rawtext),
            ScriptDataLessThan => match self.consume() {
                Some('/') => {
                    self.buffer.clear();
                    self.state = ScriptDataEndTagOpen;
                }
                Some('!') => {
                    self.state = ScriptDataEscapeStart;
                    self.emit_str("<!");
                }
                _ => {
                    self.emit_char_from('<', self.token_start);
                    self.reconsume(ScriptData);
                }
            },
            ScriptDataEndTagOpen => self.end_tag_open(ScriptData, ScriptDataEndTagName),
            ScriptDataEndTagName => self.end_tag_name(ScriptData),
            ScriptDataEscapeStart => match self.consume() {
                Some('-') => {
                    self.state = ScriptDataEscapeStartDash;
                    self.emit_char('-');
                }
                _ => self.reconsume(ScriptData),
            },
            ScriptDataEscapeStartDash => match self.consume() {
                Some('-') => {
                    self.state = ScriptDataEscapedDashDash;
                    self.emit_char('-');
                }
                _ => self.reconsume(ScriptData),
            },
            ScriptDataEscaped => match self.consume() {
                Some('-') => {
                    self.state = ScriptDataEscapedDash;
                    self.emit_char('-');
                }
                Some('<') => {
                    self.token_start = self.pos - 1;
                    self.state = ScriptDataEscapedLessThan;
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.emit_char('\u{fffd}');
                }
                Some(c) => self.emit_char(c),
                None => {
                    self.error("eof-in-script-html-comment-like-text");
                    self.emit_eof();
                }
            },
            ScriptDataEscapedDash => match self.consume() {
                Some('-') => {
                    self.state = ScriptDataEscapedDashDash;
                    self.emit_char('-');
                }
                Some('<') => {
                    self.token_start = self.pos - 1;
                    self.state = ScriptDataEscapedLessThan;
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.state = ScriptDataEscaped;
                    self.emit_char('\u{fffd}');
                }
                Some(c) => {
                    self.state = ScriptDataEscaped;
                    self.emit_char(c);
                }
                None => {
                    self.error("eof-in-script-html-comment-like-text");
                    self.emit_eof();
                }
            },
            ScriptDataEscapedDashDash => match self.consume() {
                Some('-') => self.emit_char('-'),
                Some('<') => {
                    self.token_start = self.pos - 1;
                    self.state = ScriptDataEscapedLessThan;
                }
                Some('>') => {
                    self.state = ScriptData;
                    self.emit_char('>');
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.state = ScriptDataEscaped;
                    self.emit_char('\u{fffd}');
                }
                Some(c) => {
                    self.state = ScriptDataEscaped;
                    self.emit_char(c);
                }
                None => {
                    self.error("eof-in-script-html-comment-like-text");
                    self.emit_eof();
                }
            },
            ScriptDataEscapedLessThan => match self.consume() {
                Some('/') => {
                    self.buffer.clear();
                    self.state = ScriptDataEscapedEndTagOpen;
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    self.buffer.clear();
                    self.emit_char_from('<', self.token_start);
                    self.reconsume(ScriptDataDoubleEscapeStart);
                }
                _ => {
                    self.emit_char_from('<', self.token_start);
                    self.reconsume(ScriptDataEscaped);
                }
            },
            ScriptDataEscapedEndTagOpen => {
                self.end_tag_open(ScriptDataEscaped, ScriptDataEscapedEndTagName)
            }
            ScriptDataEscapedEndTagName => self.end_tag_name(ScriptDataEscaped),
            ScriptDataDoubleEscapeStart => match self.consume() {
                Some(c) if is_whitespace(c) || c == '/' || c == '>' => {
                    self.state = if self.buffer == "script" {
                        ScriptDataDoubleEscaped
                    } else {
                        ScriptDataEscaped
                    };
                    self.emit_char(c);
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    self.buffer.push(c.to_ascii_lowercase());
                    self.emit_char(c);
                }
                _ => self.reconsume(ScriptDataEscaped),
            },
            ScriptDataDoubleEscaped => match self.consume() {
                Some('-') => {
                    self.state = ScriptDataDoubleEscapedDash;
                    self.emit_char('-');
                }
                Some('<') => {
                    self.state = ScriptDataDoubleEscapedLessThan;
                    self.emit_char('<');
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.emit_char('\u{fffd}');
                }
                Some(c) => self.emit_char(c),
                None => {
                    self.error("eof-in-script-html-comment-like-text");
                    self.emit_eof();
                }
            },
            ScriptDataDoubleEscapedDash => match self.consume() {
                Some('-') => {
                    self.state = ScriptDataDoubleEscapedDashDash;
                    self.emit_char('-');
                }
                Some('<') => {
                    self.state = ScriptDataDoubleEscapedLessThan;
                    self.emit_char('<');
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.state = ScriptDataDoubleEscaped;
                    self.emit_char('\u{fffd}');
                }
                Some(c) => {
                    self.state = ScriptDataDoubleEscaped;
                    self.emit_char(c);
                }
                None => {
                    self.error("eof-in-script-html-comment-like-text");
                    self.emit_eof();
                }
            },
            ScriptDataDoubleEscapedDashDash => match self.consume() {
                Some('-') => self.emit_char('-'),
                Some('<') => {
                    self.state = ScriptDataDoubleEscapedLessThan;
                    self.emit_char('<');
                }
                Some('>') => {
                    self.state = ScriptData;
                    self.emit_char('>');
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.state = ScriptDataDoubleEscaped;
                    self.emit_char('\u{fffd}');
                }
                Some(c) => {
                    self.state = ScriptDataDoubleEscaped;
                    self.emit_char(c);
                }
                None => {
                    self.error("eof-in-script-html-comment-like-text");
                    self.emit_eof();
                }
            },
            ScriptDataDoubleEscapedLessThan => match self.consume() {
                Some('/') => {
                    self.buffer.clear();
                    self.state = ScriptDataDoubleEscapeEnd;
                    self.emit_char('/');
                }
                _ => self.reconsume(ScriptDataDoubleEscaped),
            },
            ScriptDataDoubleEscapeEnd => match self.consume() {
                Some(c) if is_whitespace(c) || c == '/' || c == '>' => {
                    self.state = if self.buffer == "script" {
                        ScriptDataEscaped
                    } else {
                        ScriptDataDoubleEscaped
                    };
                    self.emit_char(c);
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    self.buffer.push(c.to_ascii_lowercase());
                    self.emit_char(c);
                }
                _ => self.reconsume(ScriptDataDoubleEscaped),
            },
            BeforeAttributeName => match self.consume() {
                Some(c) if is_whitespace(c) => {}
                Some('/' | '>') | None => self.reconsume(AfterAttributeName),
                Some('=') => {
                    self.error("unexpected-equals-sign-before-attribute-name");
                    self.start_attribute();
                    self.push_attribute_name('=');
                    self.state = AttributeName;
                }
                Some(_) => {
                    self.start_attribute();
                    self.reconsume(AttributeName);
                }
            },
            AttributeName => match self.consume() {
                Some(c) if is_whitespace(c) || c == '/' || c == '>' => {
                    self.reconsume(AfterAttributeName)
                }
                None => self.reconsume(AfterAttributeName),
                Some('=') => self.state = BeforeAttributeValue,
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.push_attribute_name('\u{fffd}');
                }
                Some(c) => {
                    if matches!(c, '"' | '\'' | '<') {
                        self.error("unexpected-character-in-attribute-name");
                    }
                    self.push_attribute_name(c.to_ascii_lowercase());
                }
            },
            AfterAttributeName => match self.consume() {
                Some(c) if is_whitespace(c) => {}
                Some('/') => self.state = SelfClosingStartTag,
                Some('=') => self.state = BeforeAttributeValue,
                Some('>') => {
                    self.state = Data;
                    self.emit_tag();
                }
                None => {
                    self.error("eof-in-tag");
                    self.emit_eof();
                }
                Some(_) => {
                    self.start_attribute();
                    self.reconsume(AttributeName);
                }
            },
            BeforeAttributeValue => match self.consume() {
                Some(c) if is_whitespace(c) => {}
                Some('"') => {
                    let at = self.pos;
                    self.mark_value_start(at);
                    self.state = AttributeValueDouble;
                }
                Some('\'') => {
                    let at = self.pos;
                    self.mark_value_start(at);
                    self.state = AttributeValueSingle;
                }
                Some('>') => {
                    self.error("missing-attribute-value");
                    self.state = Data;
                    self.emit_tag();
                }
                _ => self.reconsume(AttributeValueUnquoted),
            },
            AttributeValueDouble => self.quoted_value('"', AttributeValueDouble),
            AttributeValueSingle => self.quoted_value('\'', AttributeValueSingle),
            AttributeValueUnquoted => match self.consume() {
                Some(c) if is_whitespace(c) => self.state = BeforeAttributeName,
                Some('&') => {
                    self.return_state = AttributeValueUnquoted;
                    self.reference_start = self.pos - 1;
                    self.state = CharacterReference;
                }
                Some('>') => {
                    self.state = Data;
                    self.emit_tag();
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.push_attribute_value('\u{fffd}');
                }
                None => {
                    self.error("eof-in-tag");
                    self.emit_eof();
                }
                Some(c) => {
                    if matches!(c, '"' | '\'' | '<' | '=' | '`') {
                        self.error("unexpected-character-in-unquoted-attribute-value");
                    }
                    self.push_attribute_value(c);
                }
            },
            AfterAttributeValueQuoted => match self.consume() {
                Some(c) if is_whitespace(c) => self.state = BeforeAttributeName,
                Some('/') => self.state = SelfClosingStartTag,
                Some('>') => {
                    self.state = Data;
                    self.emit_tag();
                }
                None => {
                    self.error("eof-in-tag");
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("missing-whitespace-between-attributes");
                    self.reconsume(BeforeAttributeName);
                }
            },
            SelfClosingStartTag => match self.consume() {
                Some('>') => {
                    self.tag().self_closing = true;
                    self.state = Data;
                    self.emit_tag();
                }
                None => {
                    self.error("eof-in-tag");
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("unexpected-solidus-in-tag");
                    self.reconsume(BeforeAttributeName);
                }
            },
            BogusComment => match self.consume() {
                Some('>') => {
                    self.state = Data;
                    self.emit_comment();
                }
                None => {
                    self.emit_comment();
                    self.emit_eof();
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.comment.push('\u{fffd}');
                }
                Some(c) => self.comment.push(c),
            },
            ProcessingInstructionOpen => match self.consume() {
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    self.reconsume(ProcessingInstructionTarget)
                }
                None => {
                    self.error("eof-in-processing-instruction");
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("invalid-first-character-of-processing-instruction-target");
                    self.buffer_to_comment();
                    self.reconsume(BogusComment);
                }
            },
            ProcessingInstructionTarget => match self.consume() {
                Some(c) if is_whitespace(c) || c == '?' || c == '>' => {
                    if self.buffer.eq_ignore_ascii_case("xml")
                        || self.buffer.eq_ignore_ascii_case("xml-stylesheet")
                    {
                        self.error("disallowed-processing-instruction-target");
                        self.buffer_to_comment();
                        self.reconsume(BogusComment);
                    } else {
                        self.pi_data.clear();
                        self.reconsume(AfterProcessingInstructionTarget);
                    }
                }
                Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '_' => self.buffer.push(c),
                None => {
                    self.error("eof-in-processing-instruction");
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("invalid-processing-instruction-target");
                    self.buffer_to_comment();
                    self.reconsume(BogusComment);
                }
            },
            AfterProcessingInstructionTarget => match self.consume() {
                Some(c) if is_whitespace(c) => {}
                _ => self.reconsume(ProcessingInstructionData),
            },
            ProcessingInstructionData => match self.consume() {
                Some('?') => self.state = ProcessingInstructionQuestionable,
                Some('>') => {
                    self.state = Data;
                    self.emit_processing_instruction();
                }
                None => {
                    self.error("eof-in-processing-instruction");
                    self.emit_eof();
                }
                Some(c) => self.pi_data.push(c),
            },
            ProcessingInstructionQuestionable => match self.consume() {
                Some('>') => {
                    self.state = Data;
                    self.emit_processing_instruction();
                }
                None => {
                    self.error("eof-in-processing-instruction");
                    self.emit_eof();
                }
                Some(_) => {
                    self.pi_data.push('?');
                    self.reconsume(ProcessingInstructionData);
                }
            },
            MarkupDeclarationOpen => {
                if self.lookahead("--", false) {
                    self.skip(2);
                    self.comment.clear();
                    self.state = CommentStart;
                } else if self.lookahead("DOCTYPE", true) {
                    self.skip(7);
                    self.state = Doctype;
                } else if self.lookahead("[CDATA[", false) {
                    self.skip(7);
                    if self.cdata_allowed {
                        self.state = CdataSection;
                    } else {
                        self.error("cdata-in-html-content");
                        self.comment = "[CDATA[".into();
                        self.state = BogusComment;
                    }
                } else {
                    self.error("incorrectly-opened-comment");
                    self.comment.clear();
                    self.state = BogusComment;
                }
            }
            CommentStart => match self.consume() {
                Some('-') => self.state = CommentStartDash,
                Some('>') => {
                    self.error("abrupt-closing-of-empty-comment");
                    self.state = Data;
                    self.emit_comment();
                }
                _ => self.reconsume(Comment),
            },
            CommentStartDash => match self.consume() {
                Some('-') => self.state = CommentEnd,
                Some('>') => {
                    self.error("abrupt-closing-of-empty-comment");
                    self.state = Data;
                    self.emit_comment();
                }
                None => {
                    self.error("eof-in-comment");
                    self.emit_comment();
                    self.emit_eof();
                }
                Some(_) => {
                    self.comment.push('-');
                    self.reconsume(Comment);
                }
            },
            Comment => match self.consume() {
                Some('<') => {
                    self.comment.push('<');
                    self.state = CommentLessThan;
                }
                Some('-') => self.state = CommentEndDash,
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.comment.push('\u{fffd}');
                }
                Some(c) => self.comment.push(c),
                None => {
                    self.error("eof-in-comment");
                    self.emit_comment();
                    self.emit_eof();
                }
            },
            CommentLessThan => match self.consume() {
                Some('!') => {
                    self.comment.push('!');
                    self.state = CommentLessThanBang;
                }
                Some('<') => self.comment.push('<'),
                _ => self.reconsume(Comment),
            },
            CommentLessThanBang => match self.consume() {
                Some('-') => self.state = CommentLessThanBangDash,
                _ => self.reconsume(Comment),
            },
            CommentLessThanBangDash => match self.consume() {
                Some('-') => self.state = CommentLessThanBangDashDash,
                _ => self.reconsume(CommentEndDash),
            },
            CommentLessThanBangDashDash => match self.consume() {
                Some('>') | None => self.reconsume(CommentEnd),
                Some(_) => {
                    self.error("nested-comment");
                    self.reconsume(CommentEnd);
                }
            },
            CommentEndDash => match self.consume() {
                Some('-') => self.state = CommentEnd,
                None => {
                    self.error("eof-in-comment");
                    self.emit_comment();
                    self.emit_eof();
                }
                Some(_) => {
                    self.comment.push('-');
                    self.reconsume(Comment);
                }
            },
            CommentEnd => match self.consume() {
                Some('>') => {
                    self.state = Data;
                    self.emit_comment();
                }
                Some('!') => self.state = CommentEndBang,
                Some('-') => self.comment.push('-'),
                None => {
                    self.error("eof-in-comment");
                    self.emit_comment();
                    self.emit_eof();
                }
                Some(_) => {
                    self.comment.push_str("--");
                    self.reconsume(Comment);
                }
            },
            CommentEndBang => match self.consume() {
                Some('-') => {
                    self.comment.push_str("--!");
                    self.state = CommentEndDash;
                }
                Some('>') => {
                    self.error("incorrectly-closed-comment");
                    self.state = Data;
                    self.emit_comment();
                }
                None => {
                    self.error("eof-in-comment");
                    self.emit_comment();
                    self.emit_eof();
                }
                Some(_) => {
                    self.comment.push_str("--!");
                    self.reconsume(Comment);
                }
            },
            Doctype => match self.consume() {
                Some(c) if is_whitespace(c) => self.state = BeforeDoctypeName,
                Some('>') => self.reconsume(BeforeDoctypeName),
                None => {
                    self.error("eof-in-doctype");
                    self.new_doctype();
                    self.doctype().force_quirks = true;
                    self.emit_doctype();
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("missing-whitespace-before-doctype-name");
                    self.reconsume(BeforeDoctypeName);
                }
            },
            BeforeDoctypeName => match self.consume() {
                Some(c) if is_whitespace(c) => {}
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.new_doctype();
                    self.doctype().name = Some('\u{fffd}'.to_string());
                    self.state = DoctypeName;
                }
                Some('>') => {
                    self.error("missing-doctype-name");
                    self.new_doctype();
                    self.doctype().force_quirks = true;
                    self.state = Data;
                    self.emit_doctype();
                }
                None => {
                    self.error("eof-in-doctype");
                    self.new_doctype();
                    self.doctype().force_quirks = true;
                    self.emit_doctype();
                    self.emit_eof();
                }
                Some(c) => {
                    self.new_doctype();
                    self.doctype().name = Some(c.to_ascii_lowercase().to_string());
                    self.state = DoctypeName;
                }
            },
            DoctypeName => match self.consume() {
                Some(c) if is_whitespace(c) => self.state = AfterDoctypeName,
                Some('>') => {
                    self.state = Data;
                    self.emit_doctype();
                }
                Some('\0') => {
                    self.error("unexpected-null-character");
                    self.doctype()
                        .name
                        .get_or_insert_with(String::new)
                        .push('\u{fffd}');
                }
                None => {
                    self.error("eof-in-doctype");
                    self.doctype().force_quirks = true;
                    self.emit_doctype();
                    self.emit_eof();
                }
                Some(c) => self
                    .doctype()
                    .name
                    .get_or_insert_with(String::new)
                    .push(c.to_ascii_lowercase()),
            },
            AfterDoctypeName => {
                // Peek first: the keywords are matched from the current character.
                match self.peek(0) {
                    Some(c) if is_whitespace(c) => self.skip(1),
                    Some('>') => {
                        self.skip(1);
                        self.state = Data;
                        self.emit_doctype();
                    }
                    None => {
                        self.consume();
                        self.error("eof-in-doctype");
                        self.doctype().force_quirks = true;
                        self.emit_doctype();
                        self.emit_eof();
                    }
                    Some(_) if self.lookahead("PUBLIC", true) => {
                        self.skip(6);
                        self.state = AfterDoctypePublicKeyword;
                    }
                    Some(_) if self.lookahead("SYSTEM", true) => {
                        self.skip(6);
                        self.state = AfterDoctypeSystemKeyword;
                    }
                    Some(_) => {
                        self.skip(1);
                        self.error("invalid-character-sequence-after-doctype-name");
                        self.doctype().force_quirks = true;
                        self.pos -= 1;
                        self.state = BogusDoctype;
                    }
                }
            }
            AfterDoctypePublicKeyword => self.after_identifier_keyword(true),
            BeforeDoctypePublicIdentifier => self.before_identifier(true),
            DoctypePublicIdentifierDouble => self.identifier('"', true),
            DoctypePublicIdentifierSingle => self.identifier('\'', true),
            AfterDoctypePublicIdentifier => match self.consume() {
                Some(c) if is_whitespace(c) => {
                    self.state = BetweenDoctypePublicAndSystemIdentifiers
                }
                Some('>') => {
                    self.state = Data;
                    self.emit_doctype();
                }
                Some(q @ ('"' | '\'')) => {
                    self.error("missing-whitespace-between-doctype-public-and-system-identifiers");
                    self.doctype().system_id = Some(String::new());
                    self.state = if q == '"' {
                        DoctypeSystemIdentifierDouble
                    } else {
                        DoctypeSystemIdentifierSingle
                    };
                }
                None => {
                    self.error("eof-in-doctype");
                    self.doctype().force_quirks = true;
                    self.emit_doctype();
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("missing-quote-before-doctype-system-identifier");
                    self.doctype().force_quirks = true;
                    self.reconsume(BogusDoctype);
                }
            },
            BetweenDoctypePublicAndSystemIdentifiers => match self.consume() {
                Some(c) if is_whitespace(c) => {}
                Some('>') => {
                    self.state = Data;
                    self.emit_doctype();
                }
                Some(q @ ('"' | '\'')) => {
                    self.doctype().system_id = Some(String::new());
                    self.state = if q == '"' {
                        DoctypeSystemIdentifierDouble
                    } else {
                        DoctypeSystemIdentifierSingle
                    };
                }
                None => {
                    self.error("eof-in-doctype");
                    self.doctype().force_quirks = true;
                    self.emit_doctype();
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("missing-quote-before-doctype-system-identifier");
                    self.doctype().force_quirks = true;
                    self.reconsume(BogusDoctype);
                }
            },
            AfterDoctypeSystemKeyword => self.after_identifier_keyword(false),
            BeforeDoctypeSystemIdentifier => self.before_identifier(false),
            DoctypeSystemIdentifierDouble => self.identifier('"', false),
            DoctypeSystemIdentifierSingle => self.identifier('\'', false),
            AfterDoctypeSystemIdentifier => match self.consume() {
                Some(c) if is_whitespace(c) => {}
                Some('>') => {
                    self.state = Data;
                    self.emit_doctype();
                }
                None => {
                    self.error("eof-in-doctype");
                    self.doctype().force_quirks = true;
                    self.emit_doctype();
                    self.emit_eof();
                }
                Some(_) => {
                    self.error("unexpected-character-after-doctype-system-identifier");
                    self.reconsume(BogusDoctype);
                }
            },
            BogusDoctype => match self.consume() {
                Some('>') => {
                    self.state = Data;
                    self.emit_doctype();
                }
                Some('\0') => self.error("unexpected-null-character"),
                None => {
                    self.emit_doctype();
                    self.emit_eof();
                }
                Some(_) => {}
            },
            CdataSection => match self.consume() {
                Some(']') => self.state = CdataSectionBracket,
                None => {
                    self.error("eof-in-cdata");
                    self.emit_eof();
                }
                Some(c) => self.emit_char(c),
            },
            CdataSectionBracket => match self.consume() {
                Some(']') => self.state = CdataSectionEnd,
                _ => {
                    self.emit_str("]");
                    self.reconsume(CdataSection);
                }
            },
            CdataSectionEnd => match self.consume() {
                Some(']') => self.emit_char(']'),
                Some('>') => self.state = Data,
                _ => {
                    self.emit_str("]]");
                    self.reconsume(CdataSection);
                }
            },
            CharacterReference => {
                if !self.in_attribute() {
                    self.reference_start = self.pos - 1;
                }
                self.buffer = "&".into();
                match self.consume() {
                    Some(c) if c.is_ascii_alphanumeric() => self.reconsume(NamedCharacterReference),
                    Some('#') => {
                        self.buffer.push('#');
                        self.state = NumericCharacterReference;
                    }
                    _ => {
                        self.flush_reference();
                        let r = self.return_state;
                        self.reconsume(r);
                    }
                }
            }
            NamedCharacterReference => self.named_reference(),
            AmbiguousAmpersand => match self.consume() {
                Some(c) if c.is_ascii_alphanumeric() => {
                    if self.in_attribute() {
                        self.push_attribute_value(c);
                    } else {
                        self.emit_char(c);
                    }
                }
                Some(';') => {
                    self.error("unknown-named-character-reference");
                    let r = self.return_state;
                    self.reconsume(r);
                }
                _ => {
                    let r = self.return_state;
                    self.reconsume(r);
                }
            },
            NumericCharacterReference => {
                self.code = 0;
                match self.consume() {
                    Some(c @ ('x' | 'X')) => {
                        self.buffer.push(c);
                        self.state = HexadecimalCharacterReferenceStart;
                    }
                    _ => self.reconsume(DecimalCharacterReferenceStart),
                }
            }
            HexadecimalCharacterReferenceStart => match self.consume() {
                Some(c) if c.is_ascii_hexdigit() => self.reconsume(HexadecimalCharacterReference),
                _ => {
                    self.error("absence-of-digits-in-numeric-character-reference");
                    self.flush_reference();
                    let r = self.return_state;
                    self.reconsume(r);
                }
            },
            DecimalCharacterReferenceStart => match self.consume() {
                Some(c) if c.is_ascii_digit() => self.reconsume(DecimalCharacterReference),
                _ => {
                    self.error("absence-of-digits-in-numeric-character-reference");
                    self.flush_reference();
                    let r = self.return_state;
                    self.reconsume(r);
                }
            },
            HexadecimalCharacterReference => match self.consume() {
                Some(c) if c.is_ascii_hexdigit() => {
                    self.code = self
                        .code
                        .saturating_mul(16)
                        .saturating_add(c.to_digit(16).expect("hex"))
                        .min(0x110000);
                }
                Some(';') => self.state = NumericCharacterReferenceEnd,
                _ => {
                    self.error("missing-semicolon-after-character-reference");
                    self.reconsume(NumericCharacterReferenceEnd);
                }
            },
            DecimalCharacterReference => match self.consume() {
                Some(c) if c.is_ascii_digit() => {
                    self.code = self
                        .code
                        .saturating_mul(10)
                        .saturating_add(c.to_digit(10).expect("digit"))
                        .min(0x110000);
                }
                Some(';') => self.state = NumericCharacterReferenceEnd,
                _ => {
                    self.error("missing-semicolon-after-character-reference");
                    self.reconsume(NumericCharacterReferenceEnd);
                }
            },
            NumericCharacterReferenceEnd => {
                let mut n = self.code;
                if n == 0 {
                    self.error("null-character-reference");
                    n = 0xFFFD;
                } else if n > 0x10FFFF {
                    self.error("character-reference-outside-unicode-range");
                    n = 0xFFFD;
                } else if (0xD800..=0xDFFF).contains(&n) {
                    self.error("surrogate-character-reference");
                    n = 0xFFFD;
                } else if is_noncharacter(n) {
                    self.error("noncharacter-character-reference");
                } else if n == 0x0D
                    || ((n < 0x20 || (0x7F..=0x9F).contains(&n))
                        && !matches!(n, 0x09 | 0x0A | 0x0C | 0x20))
                {
                    self.error("control-character-reference");
                    if let Some((_, r)) = C1_REPLACEMENTS.iter().find(|(k, _)| *k == n) {
                        n = *r;
                    }
                }
                self.buffer = char::from_u32(n).unwrap_or('\u{fffd}').to_string();
                self.flush_reference();
                self.state = self.return_state;
            }
        }
    }

    fn quoted_value(&mut self, quote: char, here: State) {
        match self.consume() {
            Some(c) if c == quote => {
                let end = self.pos - 1;
                let a = self.attribute();
                a.value_end = end;
                self.state = State::AfterAttributeValueQuoted;
            }
            Some('&') => {
                self.return_state = here;
                self.reference_start = self.pos - 1;
                self.state = State::CharacterReference;
            }
            Some('\0') => {
                self.error("unexpected-null-character");
                self.push_attribute_value('\u{fffd}');
            }
            None => {
                self.error("eof-in-tag");
                self.emit_eof();
            }
            Some(c) => self.push_attribute_value(c),
        }
    }

    fn less_than(&mut self, text: State, end_tag_open: State) {
        match self.consume() {
            Some('/') => {
                self.buffer.clear();
                self.state = end_tag_open;
            }
            _ => {
                self.emit_char_from('<', self.token_start);
                self.reconsume(text);
            }
        }
    }

    fn end_tag_open(&mut self, text: State, end_tag_name: State) {
        match self.consume() {
            Some(c) if c.is_ascii_alphabetic() => {
                self.new_tag(false);
                self.reconsume(end_tag_name);
            }
            _ => {
                let start = self.token_start;
                self.emit_char_from('<', start);
                self.emit_char_from('/', start);
                self.reconsume(text);
            }
        }
    }

    fn end_tag_name(&mut self, text: State) {
        match self.consume() {
            Some(c) if is_whitespace(c) && self.appropriate_end_tag() => {
                self.state = State::BeforeAttributeName
            }
            Some('/') if self.appropriate_end_tag() => self.state = State::SelfClosingStartTag,
            Some('>') if self.appropriate_end_tag() => {
                self.state = State::Data;
                self.emit_tag();
            }
            Some(c) if c.is_ascii_alphabetic() => {
                self.tag().name.push(c.to_ascii_lowercase());
                self.buffer.push(c);
            }
            _ => {
                self.emit_failed_end_tag();
                self.reconsume(text);
            }
        }
    }

    fn after_identifier_keyword(&mut self, public: bool) {
        use State::*;
        let (before, double, single) = if public {
            (
                BeforeDoctypePublicIdentifier,
                DoctypePublicIdentifierDouble,
                DoctypePublicIdentifierSingle,
            )
        } else {
            (
                BeforeDoctypeSystemIdentifier,
                DoctypeSystemIdentifierDouble,
                DoctypeSystemIdentifierSingle,
            )
        };
        match self.consume() {
            Some(c) if is_whitespace(c) => self.state = before,
            Some(q @ ('"' | '\'')) => {
                self.error(if public {
                    "missing-whitespace-after-doctype-public-keyword"
                } else {
                    "missing-whitespace-after-doctype-system-keyword"
                });
                self.set_identifier(public);
                self.state = if q == '"' { double } else { single };
            }
            Some('>') => {
                self.error(if public {
                    "missing-doctype-public-identifier"
                } else {
                    "missing-doctype-system-identifier"
                });
                self.doctype().force_quirks = true;
                self.state = Data;
                self.emit_doctype();
            }
            None => {
                self.error("eof-in-doctype");
                self.doctype().force_quirks = true;
                self.emit_doctype();
                self.emit_eof();
            }
            Some(_) => {
                self.error(if public {
                    "missing-quote-before-doctype-public-identifier"
                } else {
                    "missing-quote-before-doctype-system-identifier"
                });
                self.doctype().force_quirks = true;
                self.reconsume(BogusDoctype);
            }
        }
    }

    fn set_identifier(&mut self, public: bool) {
        let d = self.doctype();
        if public {
            d.public_id = Some(String::new());
        } else {
            d.system_id = Some(String::new());
        }
    }

    fn before_identifier(&mut self, public: bool) {
        use State::*;
        let (double, single) = if public {
            (DoctypePublicIdentifierDouble, DoctypePublicIdentifierSingle)
        } else {
            (DoctypeSystemIdentifierDouble, DoctypeSystemIdentifierSingle)
        };
        match self.consume() {
            Some(c) if is_whitespace(c) => {}
            Some(q @ ('"' | '\'')) => {
                self.set_identifier(public);
                self.state = if q == '"' { double } else { single };
            }
            Some('>') => {
                self.error(if public {
                    "missing-doctype-public-identifier"
                } else {
                    "missing-doctype-system-identifier"
                });
                self.doctype().force_quirks = true;
                self.state = Data;
                self.emit_doctype();
            }
            None => {
                self.error("eof-in-doctype");
                self.doctype().force_quirks = true;
                self.emit_doctype();
                self.emit_eof();
            }
            Some(_) => {
                self.error(if public {
                    "missing-quote-before-doctype-public-identifier"
                } else {
                    "missing-quote-before-doctype-system-identifier"
                });
                self.doctype().force_quirks = true;
                self.reconsume(BogusDoctype);
            }
        }
    }

    fn identifier(&mut self, quote: char, public: bool) {
        let push = |t: &mut Self, c: char| {
            let d = t.doctype();
            let id = if public {
                &mut d.public_id
            } else {
                &mut d.system_id
            };
            id.get_or_insert_with(String::new).push(c);
        };
        match self.consume() {
            Some(c) if c == quote => {
                self.state = if public {
                    State::AfterDoctypePublicIdentifier
                } else {
                    State::AfterDoctypeSystemIdentifier
                }
            }
            Some('\0') => {
                self.error("unexpected-null-character");
                push(self, '\u{fffd}');
            }
            Some('>') => {
                self.error(if public {
                    "abrupt-doctype-public-identifier"
                } else {
                    "abrupt-doctype-system-identifier"
                });
                self.doctype().force_quirks = true;
                self.state = State::Data;
                self.emit_doctype();
            }
            None => {
                self.error("eof-in-doctype");
                self.doctype().force_quirks = true;
                self.emit_doctype();
                self.emit_eof();
            }
            Some(c) => push(self, c),
        }
    }

    /// §13.2.5.73: the longest table name that matches the input.
    fn named_reference(&mut self) {
        let mut best: Option<(usize, &'static str, &'static str)> = None;
        let mut candidate = String::new();
        for k in 0..entities::MAX_NAME {
            let Some(c) = self.peek(k) else { break };
            if !c.is_ascii() {
                break;
            }
            candidate.push(c);
            if let Ok(i) =
                entities::ENTITIES.binary_search_by(|(name, _)| name.cmp(&candidate.as_str()))
            {
                let (name, value) = entities::ENTITIES[i];
                best = Some((k + 1, name, value));
            }
            if !c.is_ascii_alphanumeric() {
                break;
            }
        }
        match best {
            Some((len, name, value)) => {
                self.skip(len);
                self.buffer.push_str(name);
                let ends_with_semicolon = name.ends_with(';');
                let next = self.peek(0);
                if self.in_attribute()
                    && !ends_with_semicolon
                    && next.is_some_and(|c| c == '=' || c.is_ascii_alphanumeric())
                {
                    self.flush_reference();
                    self.state = self.return_state;
                } else {
                    if !ends_with_semicolon {
                        self.error("missing-semicolon-after-character-reference");
                    }
                    self.buffer = value.to_string();
                    self.flush_reference();
                    self.state = self.return_state;
                }
            }
            None => {
                self.flush_reference();
                self.state = State::AmbiguousAmpersand;
            }
        }
    }
}

/// Tokenizes a whole document, switching content models as an HTML tree
/// builder would ([`Tokenizer::html_content_model`]).
pub fn tokenize_document(input: &str) -> (Vec<Spanned>, Vec<ParseError>) {
    let mut t = Tokenizer::new(input).html_content_model(true);
    let mut tokens = Vec::new();
    while let Some(token) = t.next_token() {
        tokens.push(token);
    }
    let errors = t.errors().to_vec();
    (tokens, errors)
}

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
