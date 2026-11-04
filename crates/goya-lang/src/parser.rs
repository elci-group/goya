//! Recursive-descent parser: tokens → Scene. Total on all inputs; every
//! failure is `ParseError { line }`.

use crate::lexer::{lex, Token, TokenKind};
use crate::scene::{Anchor, CellKind, Scene, SceneCell, Vec3};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at line {}", self.line)
    }
}

impl std::error::Error for ParseError {}

/// Parse a scene document.
pub fn parse(source: &str) -> Result<Scene, ParseError> {
    let tokens = lex(source).map_err(|e| ParseError { line: e.line })?;
    let mut p = P { tokens, i: 0 };
    p.expect_at_word("ontology")?;
    let name = p.expect_ident()?;
    p.expect(TokenKind::LBrace)?;
    let mut root = p.parse_cell_body()?;
    // Collapse the anonymous ontology wrapper when it holds exactly one
    // named cell and no own properties — the wrapper is syntax, not scene.
    if root.children.len() == 1
        && root.position.is_none()
        && root.extent.is_none()
        && root.position_p.is_none()
        && root.extent_p.is_none()
        && root.fill.is_none()
        && root.text.is_none()
    {
        root = root.children.remove(0);
    }
    Ok(Scene { name, root })
}

struct P {
    tokens: Vec<Token>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.i)
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.i).cloned();
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn line(&self) -> usize {
        self.peek().map(|t| t.line).unwrap_or(0)
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if matches!(self.peek().map(|t| t.kind), Some(k) if k == kind) {
            // peek() confirmed the token exists, so bump() cannot fail here;
            // the impossible arm returns a parse error rather than panicking.
            match self.bump() {
                Some(t) => Ok(t),
                None => Err(ParseError { line: self.line() }),
            }
        } else {
            Err(ParseError { line: self.line() })
        }
    }

    fn expect_at_word(&mut self, word: &str) -> Result<(), ParseError> {
        match self.peek() {
            Some(t) if t.kind == TokenKind::AtWord && t.text == word => {
                self.bump();
                Ok(())
            }
            _ => Err(ParseError { line: self.line() }),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Ident)) {
            match self.bump() {
                Some(t) => Ok(t.text),
                None => Err(ParseError { line: self.line() }),
            }
        } else {
            Err(ParseError { line: self.line() })
        }
    }

    /// Parse cells/buttons until the closing brace (consumed).
    fn parse_cell_body(&mut self) -> Result<SceneCell, ParseError> {
        // Header may be `@cell name {` or `@button name {`; the caller for
        // the ontology root uses a virtual cell.
        let mut cell = SceneCell {
            name: String::new(),
            kind: CellKind::Cell,
            position: None,
            extent: None,
            position_p: None,
            extent_p: None,
            fill: None,
            stroke: None,
            stroke_width: None,
            text: None,
            text_size: None,
            anchor: None,
            children: Vec::new(),
        };
        loop {
            match self.peek().map(|t| t.kind) {
                None => return Err(ParseError { line: self.line() }),
                Some(TokenKind::RBrace) => {
                    self.bump();
                    return Ok(cell);
                }
                Some(TokenKind::AtWord) => {
                    // The match guard guarantees a token exists; still
                    // explicit rather than unwrap.
                    let Some(tok) = self.bump() else {
                        return Err(ParseError { line: self.line() });
                    };
                    let word = tok.text;
                    match word.as_str() {
                        "cell" | "button" => {
                            let kind = if word == "button" {
                                CellKind::Button
                            } else {
                                CellKind::Cell
                            };
                            let name = self.expect_ident()?;
                            self.expect(TokenKind::LBrace)?;
                            let mut child = self.parse_cell_body()?;
                            child.name = name;
                            child.kind = kind;
                            cell.children.push(child);
                        }
                        "interaction" | "template" => {
                            // Recognised and skipped with its block: visual
                            // state and templates are Wave-3 expression work.
                            let _ = self.expect_ident()?;
                            self.skip_block()?;
                        }
                        _ => return Err(ParseError { line: self.line() }),
                    }
                }
                Some(TokenKind::Ident) => {
                    self.parse_property(&mut cell)?;
                }
                Some(_) => return Err(ParseError { line: self.line() }),
            }
        }
    }

    fn skip_block(&mut self) -> Result<(), ParseError> {
        // Consume optional identifier(s) up to `{`, then balanced braces.
        while matches!(self.peek().map(|t| t.kind), Some(TokenKind::Ident)) {
            self.bump();
        }
        self.expect(TokenKind::LBrace)?;
        let mut depth = 1usize;
        while depth > 0 {
            match self.bump().map(|t| t.kind) {
                Some(TokenKind::LBrace) => depth += 1,
                Some(TokenKind::RBrace) => depth -= 1,
                Some(_) => {}
                None => return Err(ParseError { line: self.line() }),
            }
        }
        Ok(())
    }

    fn parse_property(&mut self, cell: &mut SceneCell) -> Result<(), ParseError> {
        let key = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        match key.as_str() {
            "position" => cell.position = Some(self.parse_vec3()?),
            "extent" => cell.extent = Some(self.parse_vec3()?),
            "position_p" => cell.position_p = Some(self.parse_vec3()?),
            "extent_p" => cell.extent_p = Some(self.parse_vec3()?),
            "fill" => cell.fill = Some(self.parse_hex()?),
            "stroke" => cell.stroke = Some(self.parse_hex()?),
            "stroke-width" | "stroke_width" => cell.stroke_width = Some(self.parse_number()?),
            "text" => {
                let t = self.expect(TokenKind::Str)?;
                cell.text = Some(t.text);
            }
            "text-size" | "text_size" => cell.text_size = Some(self.parse_number()?),
            "text-anchor" | "text_anchor" => {
                let name = self.expect_ident()?;
                cell.anchor = Some(match name.as_str() {
                    "start" => Anchor::Start,
                    "middle" => Anchor::Middle,
                    "end" => Anchor::End,
                    _ => return Err(ParseError { line: self.line() }),
                });
            }
            // Unknown properties are consumed as scalar values so the
            // language can grow without breaking older parsers.
            _ => {
                self.skip_value()?;
            }
        }
        // Optional single-line separator.
        if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Semicolon)) {
            self.bump();
        }
        Ok(())
    }

    fn parse_vec3(&mut self) -> Result<Vec3, ParseError> {
        self.expect(TokenKind::LParen)?;
        let x = self.parse_number()?;
        self.expect(TokenKind::Comma)?;
        let y = self.parse_number()?;
        let z = if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Comma)) {
            self.bump();
            self.parse_number()?
        } else {
            0.0
        };
        self.expect(TokenKind::RParen)?;
        Ok(Vec3 { x, y, z })
    }

    fn parse_number(&mut self) -> Result<f64, ParseError> {
        match self.peek() {
            Some(t) if t.kind == TokenKind::Number => {
                let text = self.bump().unwrap().text.replace('_', "");
                text.parse::<f64>()
                    .map_err(|_| ParseError { line: self.line() })
            }
            _ => Err(ParseError { line: self.line() }),
        }
    }

    fn parse_hex(&mut self) -> Result<u32, ParseError> {
        match self.peek() {
            Some(t) if t.kind == TokenKind::Hex => {
                let hex = self.bump().unwrap().text[1..].to_string();
                let v = match hex.len() {
                    3 => {
                        let r = u32::from_str_radix(&hex[0..1], 16).unwrap_or(0);
                        let g = u32::from_str_radix(&hex[1..2], 16).unwrap_or(0);
                        let b = u32::from_str_radix(&hex[2..3], 16).unwrap_or(0);
                        (r << 20) | (r << 16) | (g << 12) | (g << 8) | (b << 4) | b
                    }
                    6 => u32::from_str_radix(&hex, 16).unwrap_or(0),
                    8 => u32::from_str_radix(&hex[..6], 16).unwrap_or(0),
                    _ => return Err(ParseError { line: self.line() }),
                };
                Ok(v)
            }
            _ => Err(ParseError { line: self.line() }),
        }
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        match self.peek().map(|t| t.kind) {
            Some(TokenKind::LParen) => {
                let _ = self.parse_vec3()?;
                Ok(())
            }
            Some(TokenKind::Str) => {
                self.bump();
                Ok(())
            }
            Some(TokenKind::Hex) => {
                self.bump();
                Ok(())
            }
            Some(TokenKind::Number) => {
                self.bump();
                Ok(())
            }
            Some(TokenKind::Ident) => {
                self.bump();
                Ok(())
            }
            _ => Err(ParseError { line: self.line() }),
        }
    }
}
