//! Tokeniser for the scene language. Total: never panics; all failures are
//! `LexError { line }`.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind {
    /// `@ontology`, `@cell`, … — at-word (name without the `@`).
    AtWord,
    Ident,
    Number,
    Str,
    Hex,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Colon,
    Semicolon,
    Comma,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub line: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lex error at line {}", self.line)
    }
}

impl std::error::Error for LexError {}

/// Tokenise `source`. `//` comments run to end of line. `#` starts a hex
/// colour token, not a comment.
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0usize;
    let mut line = 1usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            ' ' | '\t' | '\r' => i += 1,
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '{' | '}' | '(' | ')' | ':' | ';' | ',' => {
                push_token(&mut tokens, &chars, &mut i, &mut line)?;
            }
            _ => {
                push_token(&mut tokens, &chars, &mut i, &mut line)?;
            }
        }
    }
    Ok(tokens)
}

fn t(kind: TokenKind, text: &str, line: usize) -> Token {
    Token {
        kind,
        text: text.to_string(),
        line,
    }
}

fn push_token(
    tokens: &mut Vec<Token>,
    chars: &[char],
    i: &mut usize,
    line: &mut usize,
) -> Result<(), LexError> {
    let c = chars[*i];
    let simple = match c {
        '{' => Some(TokenKind::LBrace),
        '}' => Some(TokenKind::RBrace),
        '(' => Some(TokenKind::LParen),
        ')' => Some(TokenKind::RParen),
        ':' => Some(TokenKind::Colon),
        ';' => Some(TokenKind::Semicolon),
        ',' => Some(TokenKind::Comma),
        _ => None,
    };
    if let Some(kind) = simple {
        tokens.push(t(kind, &c.to_string(), *line));
        *i += 1;
        return Ok(());
    }
    if c == '#' {
        // Hex colour: '#' + hex digits (3, 6, or 8).
        let start = *i;
        *i += 1;
        while *i < chars.len() && chars[*i].is_ascii_hexdigit() {
            *i += 1;
        }
        let text: String = chars[start..*i].iter().collect();
        tokens.push(t(TokenKind::Hex, &text, *line));
        return Ok(());
    }
    if c == '"' {
        let start_line = *line;
        *i += 1;
        let mut s = String::new();
        while *i < chars.len() && chars[*i] != '"' {
            if chars[*i] == '\n' {
                *line += 1;
            }
            s.push(chars[*i]);
            *i += 1;
        }
        if *i >= chars.len() {
            return Err(LexError { line: start_line });
        }
        *i += 1; // closing quote
        tokens.push(t(TokenKind::Str, &s, start_line));
        return Ok(());
    }
    if c == '-' || c == '+' || c.is_ascii_digit() || c == '.' {
        let start = *i;
        while *i < chars.len()
            && (chars[*i].is_ascii_digit()
                || chars[*i] == '.'
                || chars[*i] == '-'
                || chars[*i] == '+'
                || chars[*i] == '_')
        {
            *i += 1;
        }
        let text: String = chars[start..*i].iter().collect();
        tokens.push(t(TokenKind::Number, &text, *line));
        return Ok(());
    }
    if c.is_alphabetic() || c == '_' || c == '@' {
        let at = c == '@';
        let start = *i;
        *i += 1;
        while *i < chars.len()
            && (chars[*i].is_alphanumeric() || chars[*i] == '_' || chars[*i] == '-')
        {
            *i += 1;
        }
        let text: String = chars[start..*i].iter().collect();
        let text = if at { text[1..].to_string() } else { text };
        tokens.push(t(
            if at {
                TokenKind::AtWord
            } else {
                TokenKind::Ident
            },
            &text,
            *line,
        ));
        return Ok(());
    }
    Err(LexError { line: *line })
}
