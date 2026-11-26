//! CSS Syntax Level 3 tokenizer (§4, 2021 Candidate Recommendation draft),
//! plus the `unicode-range` token from the 2014 text, which the
//! css-parsing-tests suite still expects.
//!
//! Input preprocessing (§3.3) happens on the fly: CR LF, CR and FF read as one
//! LF, and NUL reads as U+FFFD. Spans are byte offsets into the original
//! input, so a CR LF pair is one character with a two-byte span.

/// An end-exclusive byte range in the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A numeric value: its source representation, its value, and whether the
/// representation is an integer.
#[derive(Debug, Clone, PartialEq)]
pub struct Numeric {
    pub repr: String,
    pub value: f64,
    pub integer: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Function(String),
    AtKeyword(String),
    /// `id` is true when the hash would start an identifier.
    Hash {
        value: String,
        id: bool,
    },
    /// `terminated` is false when the input ended inside the string.
    String {
        value: String,
        terminated: bool,
    },
    BadString,
    /// `terminated` is false when the input ended inside the URL.
    Url {
        value: String,
        terminated: bool,
    },
    BadUrl,
    Delim(char),
    Number(Numeric),
    Percentage(Numeric),
    Dimension {
        number: Numeric,
        unit: String,
    },
    UnicodeRange {
        start: u32,
        end: u32,
    },
    IncludeMatch,
    DashMatch,
    PrefixMatch,
    SuffixMatch,
    SubstringMatch,
    Column,
    Whitespace,
    Cdo,
    Cdc,
    Colon,
    Semicolon,
    Comma,
    LeftSquare,
    RightSquare,
    LeftParen,
    RightParen,
    LeftCurly,
    RightCurly,
}

/// Preprocessed characters with the byte offset of each, plus one final
/// offset equal to the input length.
pub(crate) struct Chars {
    chars: Vec<char>,
    offsets: Vec<usize>,
}

impl Chars {
    pub(crate) fn new(input: &str) -> Self {
        let mut chars = Vec::with_capacity(input.len());
        let mut offsets = Vec::with_capacity(input.len() + 1);
        let mut it = input.char_indices().peekable();
        while let Some((i, c)) = it.next() {
            offsets.push(i);
            match c {
                '\r' => {
                    if let Some((_, '\n')) = it.peek() {
                        it.next();
                    }
                    chars.push('\n');
                }
                '\u{c}' => chars.push('\n'),
                '\0' => chars.push('\u{fffd}'),
                c => chars.push(c),
            }
        }
        offsets.push(input.len());
        Self { chars, offsets }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c as u32 >= 0x80
}

fn is_ident_char(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit() || c == '-'
}

fn is_whitespace(c: char) -> bool {
    matches!(c, '\n' | '\t' | ' ')
}

fn is_non_printable(c: char) -> bool {
    matches!(c as u32, 0..=8 | 0xb | 0xe..=0x1f | 0x7f)
}

struct Tokenizer<'c> {
    c: &'c Chars,
    pos: usize,
}

impl Tokenizer<'_> {
    fn at(&self, k: usize) -> Option<char> {
        self.c.chars.get(self.pos + k).copied()
    }

    /// §4.3.8: a backslash not followed by a newline. A backslash at the end
    /// of input is a valid escape (it becomes U+FFFD).
    fn valid_escape(a: Option<char>, b: Option<char>) -> bool {
        a == Some('\\') && b != Some('\n')
    }

    fn would_start_ident(a: Option<char>, b: Option<char>, c: Option<char>) -> bool {
        match a {
            Some('-') => {
                b.is_some_and(|b| is_ident_start(b) || b == '-') || Self::valid_escape(b, c)
            }
            Some('\\') => Self::valid_escape(a, b),
            Some(a) => is_ident_start(a),
            None => false,
        }
    }

    fn starts_number(a: Option<char>, b: Option<char>, c: Option<char>) -> bool {
        match a {
            Some('+' | '-') => {
                b.is_some_and(|b| b.is_ascii_digit())
                    || (b == Some('.') && c.is_some_and(|c| c.is_ascii_digit()))
            }
            Some('.') => b.is_some_and(|b| b.is_ascii_digit()),
            Some(a) => a.is_ascii_digit(),
            None => false,
        }
    }

    fn consume_comments(&mut self) {
        while self.at(0) == Some('/') && self.at(1) == Some('*') {
            self.pos += 2;
            loop {
                match self.at(0) {
                    None => return,
                    Some('*') if self.at(1) == Some('/') => {
                        self.pos += 2;
                        break;
                    }
                    Some(_) => self.pos += 1,
                }
            }
        }
    }

    /// After a backslash has been consumed.
    fn consume_escape(&mut self) -> char {
        match self.at(0) {
            None => '\u{fffd}',
            Some(c) if c.is_ascii_hexdigit() => {
                let mut value = 0u32;
                let mut n = 0;
                while n < 6 {
                    match self.at(0).and_then(|c| c.to_digit(16)) {
                        Some(d) => {
                            value = value * 16 + d;
                            self.pos += 1;
                            n += 1;
                        }
                        None => break,
                    }
                }
                if self.at(0).is_some_and(is_whitespace) {
                    self.pos += 1;
                }
                match char::from_u32(value) {
                    Some(c) if value != 0 => c,
                    _ => '\u{fffd}',
                }
            }
            Some(c) => {
                self.pos += 1;
                c
            }
        }
    }

    fn consume_name(&mut self) -> String {
        let mut name = String::new();
        loop {
            match self.at(0) {
                Some(c) if is_ident_char(c) => {
                    name.push(c);
                    self.pos += 1;
                }
                a if Self::valid_escape(a, self.at(1)) => {
                    self.pos += 1;
                    name.push(self.consume_escape());
                }
                _ => return name,
            }
        }
    }

    fn consume_digits(&mut self, repr: &mut String) {
        while let Some(c) = self.at(0).filter(char::is_ascii_digit) {
            repr.push(c);
            self.pos += 1;
        }
    }

    fn consume_number(&mut self) -> Numeric {
        let mut repr = String::new();
        let mut integer = true;
        if let Some(c @ ('+' | '-')) = self.at(0) {
            repr.push(c);
            self.pos += 1;
        }
        self.consume_digits(&mut repr);
        if self.at(0) == Some('.') && self.at(1).is_some_and(|c| c.is_ascii_digit()) {
            repr.push('.');
            self.pos += 1;
            integer = false;
            self.consume_digits(&mut repr);
        }
        if let Some(e @ ('e' | 'E')) = self.at(0) {
            let signed = matches!(self.at(1), Some('+' | '-'))
                && self.at(2).is_some_and(|c| c.is_ascii_digit());
            if signed || self.at(1).is_some_and(|c| c.is_ascii_digit()) {
                repr.push(e);
                self.pos += 1;
                if signed {
                    repr.push(self.at(0).expect("sign"));
                    self.pos += 1;
                }
                integer = false;
                self.consume_digits(&mut repr);
            }
        }
        let value = repr.parse::<f64>().unwrap_or(0.0);
        Numeric {
            repr,
            value,
            integer,
        }
    }

    fn consume_numeric(&mut self) -> Token {
        let number = self.consume_number();
        if Self::would_start_ident(self.at(0), self.at(1), self.at(2)) {
            let unit = self.consume_name();
            Token::Dimension { number, unit }
        } else if self.at(0) == Some('%') {
            self.pos += 1;
            Token::Percentage(number)
        } else {
            Token::Number(number)
        }
    }

    fn consume_bad_url_remnants(&mut self) {
        loop {
            match self.at(0) {
                None => return,
                Some(')') => {
                    self.pos += 1;
                    return;
                }
                a if Self::valid_escape(a, self.at(1)) => {
                    self.pos += 1;
                    self.consume_escape();
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    fn consume_url(&mut self) -> Token {
        let mut url = String::new();
        while self.at(0).is_some_and(is_whitespace) {
            self.pos += 1;
        }
        loop {
            match self.at(0) {
                None => {
                    return Token::Url {
                        value: url,
                        terminated: false,
                    }
                }
                Some(')') => {
                    self.pos += 1;
                    return Token::Url {
                        value: url,
                        terminated: true,
                    };
                }
                Some(c) if is_whitespace(c) => {
                    while self.at(0).is_some_and(is_whitespace) {
                        self.pos += 1;
                    }
                    match self.at(0) {
                        None => {
                            return Token::Url {
                                value: url,
                                terminated: false,
                            }
                        }
                        Some(')') => {
                            self.pos += 1;
                            return Token::Url {
                                value: url,
                                terminated: true,
                            };
                        }
                        Some(_) => {
                            self.consume_bad_url_remnants();
                            return Token::BadUrl;
                        }
                    }
                }
                Some(c) if c == '"' || c == '\'' || c == '(' || is_non_printable(c) => {
                    self.pos += 1;
                    self.consume_bad_url_remnants();
                    return Token::BadUrl;
                }
                Some('\\') => {
                    if Self::valid_escape(self.at(0), self.at(1)) {
                        self.pos += 1;
                        url.push(self.consume_escape());
                    } else {
                        self.pos += 1;
                        self.consume_bad_url_remnants();
                        return Token::BadUrl;
                    }
                }
                Some(c) => {
                    url.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn consume_ident_like(&mut self) -> Token {
        let name = self.consume_name();
        if name.eq_ignore_ascii_case("url") && self.at(0) == Some('(') {
            self.pos += 1;
            while self.at(0).is_some_and(is_whitespace) && self.at(1).is_some_and(is_whitespace) {
                self.pos += 1;
            }
            let quote = |c: Option<char>| matches!(c, Some('"' | '\''));
            if quote(self.at(0)) || (self.at(0).is_some_and(is_whitespace) && quote(self.at(1))) {
                Token::Function(name)
            } else {
                self.consume_url()
            }
        } else if self.at(0) == Some('(') {
            self.pos += 1;
            Token::Function(name)
        } else {
            Token::Ident(name)
        }
    }

    fn consume_string(&mut self, ending: char) -> Token {
        let mut s = String::new();
        loop {
            match self.at(0) {
                None => {
                    return Token::String {
                        value: s,
                        terminated: false,
                    }
                }
                Some(c) if c == ending => {
                    self.pos += 1;
                    return Token::String {
                        value: s,
                        terminated: true,
                    };
                }
                Some('\n') => return Token::BadString,
                Some('\\') => match self.at(1) {
                    None => self.pos += 1,
                    Some('\n') => self.pos += 2,
                    Some(_) => {
                        self.pos += 1;
                        s.push(self.consume_escape());
                    }
                },
                Some(c) => {
                    s.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    /// After `U+` or `u+` has been consumed.
    fn consume_unicode_range(&mut self) -> Token {
        let mut digits = String::new();
        while digits.len() < 6 {
            match self.at(0) {
                Some(c) if c.is_ascii_hexdigit() => digits.push(c),
                _ => break,
            }
            self.pos += 1;
        }
        let mut wild = 0;
        while digits.len() + wild < 6 && self.at(0) == Some('?') {
            wild += 1;
            self.pos += 1;
        }
        let parse = |s: &str| u32::from_str_radix(s, 16).unwrap_or(0);
        if wild > 0 {
            let low = format!("{digits}{}", "0".repeat(wild));
            let high = format!("{digits}{}", "F".repeat(wild));
            return Token::UnicodeRange {
                start: parse(&low),
                end: parse(&high),
            };
        }
        let start = parse(&digits);
        let mut end = start;
        if self.at(0) == Some('-') && self.at(1).is_some_and(|c| c.is_ascii_hexdigit()) {
            self.pos += 1;
            let mut high = String::new();
            while high.len() < 6 {
                match self.at(0) {
                    Some(c) if c.is_ascii_hexdigit() => high.push(c),
                    _ => break,
                }
                self.pos += 1;
            }
            end = parse(&high);
        }
        Token::UnicodeRange { start, end }
    }

    fn next(&mut self) -> Option<(Token, Span)> {
        self.consume_comments();
        let start = self.pos;
        let c = self.at(0)?;
        self.pos += 1;
        let token = match c {
            c if is_whitespace(c) => {
                while self.at(0).is_some_and(is_whitespace) {
                    self.pos += 1;
                }
                Token::Whitespace
            }
            '"' | '\'' => self.consume_string(c),
            '#' => {
                if self.at(0).is_some_and(is_ident_char)
                    || Self::valid_escape(self.at(0), self.at(1))
                {
                    let id = Self::would_start_ident(self.at(0), self.at(1), self.at(2));
                    Token::Hash {
                        value: self.consume_name(),
                        id,
                    }
                } else {
                    Token::Delim('#')
                }
            }
            '$' | '*' | '^' | '~' | '|' if self.at(0) == Some('=') => {
                self.pos += 1;
                match c {
                    '$' => Token::SuffixMatch,
                    '*' => Token::SubstringMatch,
                    '^' => Token::PrefixMatch,
                    '~' => Token::IncludeMatch,
                    _ => Token::DashMatch,
                }
            }
            '|' if self.at(0) == Some('|') => {
                self.pos += 1;
                Token::Column
            }
            '(' => Token::LeftParen,
            ')' => Token::RightParen,
            '[' => Token::LeftSquare,
            ']' => Token::RightSquare,
            '{' => Token::LeftCurly,
            '}' => Token::RightCurly,
            ',' => Token::Comma,
            ':' => Token::Colon,
            ';' => Token::Semicolon,
            '+' | '.' if Self::starts_number(Some(c), self.at(0), self.at(1)) => {
                self.pos -= 1;
                self.consume_numeric()
            }
            '-' => {
                if Self::starts_number(Some(c), self.at(0), self.at(1)) {
                    self.pos -= 1;
                    self.consume_numeric()
                } else if self.at(0) == Some('-') && self.at(1) == Some('>') {
                    self.pos += 2;
                    Token::Cdc
                } else if Self::would_start_ident(Some(c), self.at(0), self.at(1)) {
                    self.pos -= 1;
                    self.consume_ident_like()
                } else {
                    Token::Delim('-')
                }
            }
            '<' if self.at(0) == Some('!')
                && self.at(1) == Some('-')
                && self.at(2) == Some('-') =>
            {
                self.pos += 3;
                Token::Cdo
            }
            '@' => {
                if Self::would_start_ident(self.at(0), self.at(1), self.at(2)) {
                    Token::AtKeyword(self.consume_name())
                } else {
                    Token::Delim('@')
                }
            }
            '\\' => {
                if Self::valid_escape(Some(c), self.at(0)) {
                    self.pos -= 1;
                    self.consume_ident_like()
                } else {
                    Token::Delim('\\')
                }
            }
            'u' | 'U'
                if self.at(0) == Some('+')
                    && self
                        .at(1)
                        .is_some_and(|c| c.is_ascii_hexdigit() || c == '?') =>
            {
                self.pos += 1;
                self.consume_unicode_range()
            }
            c if c.is_ascii_digit() => {
                self.pos -= 1;
                self.consume_numeric()
            }
            c if is_ident_start(c) => {
                self.pos -= 1;
                self.consume_ident_like()
            }
            c => Token::Delim(c),
        };
        let span = Span {
            start: self.c.offsets[start],
            end: self.c.offsets[self.pos],
        };
        Some((token, span))
    }
}

/// Tokenizes `input`. Comments produce no tokens.
pub fn tokenize(input: &str) -> Vec<(Token, Span)> {
    let chars = Chars::new(input);
    let mut t = Tokenizer { c: &chars, pos: 0 };
    let mut out = Vec::new();
    while let Some(token) = t.next() {
        out.push(token);
    }
    out
}
