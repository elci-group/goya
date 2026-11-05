//! Recursive-descent parser: tokens → Scene. Total on all inputs; every
//! failure is `ParseError { line }`.

use crate::lexer::{lex, Token, TokenKind};
use crate::scene::{
    Anchor, CellKind, EffectRequestDecl, EffectValueType, InteractionClass, InteractionDecl,
    InteractionExpr, InteractionType, InteractionValue, Scene, SceneCell, StateAssignment,
    TypedKey, Vec3,
};
use goya_determinism::EffectKind;
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
    let tokens = match lex(source) {
        Ok(t) => t,
        Err(e) => return Err(ParseError { line: e.line }),
    };
    let mut p = P {
        tokens,
        i: 0,
        interactions: Vec::new(),
    };
    p.expect_at_word("ontology")?;
    let name = p.expect_ident()?;
    p.expect(TokenKind::LBrace)?;
    let mut root = p.parse_cell_body()?;
    // A document is exactly one ontology block. Do not silently accept
    // trailing tokens, which can hide a malformed second root or typo.
    if p.peek().is_some() {
        return Err(ParseError { line: p.line() });
    }
    // Collapse the anonymous ontology wrapper when it holds exactly one
    // named cell and no own properties — the wrapper is syntax, not scene.
    if root.children.len() == 1
        && root.position.is_none()
        && root.extent.is_none()
        && root.position_p.is_none()
        && root.extent_p.is_none()
        && root.fill.is_none()
        && root.stroke.is_none()
        && root.stroke_width.is_none()
        && root.text.is_none()
        && root.text_size.is_none()
        && root.anchor.is_none()
        && root.wrap.is_none()
        && !root.is_input
    {
        root = root.children.remove(0);
    }
    Ok(Scene {
        name,
        root,
        interactions: p.interactions,
    })
}

struct P {
    tokens: Vec<Token>,
    i: usize,
    interactions: Vec<InteractionDecl>,
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
        self.peek()
            .map(|t| t.line)
            .or_else(|| self.tokens.last().map(|t| t.line))
            .unwrap_or(1)
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        match self.peek() {
            Some(t) if t.kind == kind => match self.bump() {
                Some(t) => Ok(t),
                None => Err(ParseError { line: self.line() }),
            },
            _ => Err(ParseError { line: self.line() }),
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
            is_input: false,
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
            wrap: None,
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
                        "cell" | "button" | "input" => {
                            let kind = match word.as_str() {
                                "button" => CellKind::Button,
                                "input" => CellKind::Input,
                                _ => CellKind::Cell,
                            };
                            let name = self.expect_ident()?;
                            self.expect(TokenKind::LBrace)?;
                            let mut child = self.parse_cell_body()?;
                            child.name = name;
                            child.kind = kind;
                            child.is_input = kind == CellKind::Input;
                            cell.children.push(child);
                        }
                        "interaction" => {
                            let name = self.expect_ident()?;
                            let interaction = self.parse_interaction(name)?;
                            self.interactions.push(interaction);
                        }
                        "template" => {
                            // Recognised and skipped with its block: visual
                            // state and templates are Wave-3 expression work.
                            self.expect_ident()?;
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

    fn parse_interaction(&mut self, name: String) -> Result<InteractionDecl, ParseError> {
        self.expect(TokenKind::LBrace)?;
        self.expect_ident_value("trigger")?;
        self.expect(TokenKind::Colon)?;
        self.expect_ident_value("button")?;
        self.expect(TokenKind::Dot)?;
        let trigger_button = self.expect_ident()?;
        self.expect(TokenKind::Dot)?;
        self.expect_ident_value("activate")?;
        self.optional_semi();
        self.expect_ident_value("class")?;
        self.expect(TokenKind::Colon)?;
        let class = match self.expect_ident()?.as_str() {
            "D1" => InteractionClass::D1,
            "D2" => InteractionClass::D2,
            _ => return Err(ParseError { line: self.line() }),
        };
        self.optional_semi();

        let mut declared_effects = Vec::new();
        let mut requests = Vec::new();
        if class == InteractionClass::D2 {
            self.expect_ident_value("effects")?;
            self.expect(TokenKind::LBrace)?;
            while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
                declared_effects.push(self.parse_effect_kind()?);
                self.optional_semi();
            }
            self.expect(TokenKind::RBrace)?;

            self.expect_ident_value("requests")?;
            self.expect(TokenKind::LBrace)?;
            while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
                let request_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                self.expect_ident_value("effect")?;
                let effect_kind = self.parse_effect_kind()?;
                self.expect_ident_value("capability")?;
                let capability = self.expect(TokenKind::Str)?.text;
                self.expect_ident_value("request")?;
                let request_utf8 = self.expect(TokenKind::Str)?.text;
                self.expect_ident_value("result")?;
                let result_type = match self.expect_ident()?.as_str() {
                    "int64" => EffectValueType::Int64,
                    "text" => EffectValueType::Text,
                    "bool" => EffectValueType::Bool,
                    _ => return Err(ParseError { line: self.line() }),
                };
                requests.push(EffectRequestDecl {
                    name: request_name,
                    effect_kind,
                    capability,
                    request_utf8,
                    result_type,
                });
                self.expect(TokenKind::Semicolon)?;
            }
            self.expect(TokenKind::RBrace)?;
        }

        self.expect_ident_value("state")?;
        self.expect(TokenKind::LBrace)?;
        let mut state_reads = Vec::new();
        let mut state_writes = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let mode = self.expect_ident()?;
            let target = match mode.as_str() {
                "read" => &mut state_reads,
                "write" => &mut state_writes,
                _ => return Err(ParseError { line: self.line() }),
            };
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value_type = self.parse_interaction_type()?;
            target.push(TypedKey {
                name: key,
                value_type,
            });
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;

        self.expect_ident_value("input")?;
        self.expect(TokenKind::LBrace)?;
        let mut inputs = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value_type = self.parse_interaction_type()?;
            inputs.push(TypedKey {
                name: key,
                value_type,
            });
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;

        let mut assignments = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            self.expect_ident_value("set")?;
            let key = self.expect_ident()?;
            self.expect(TokenKind::Equal)?;
            let value = self.parse_interaction_expr()?;
            assignments.push(StateAssignment { key, value });
            self.expect(TokenKind::Semicolon)?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(InteractionDecl {
            name,
            trigger_button,
            class,
            declared_effects,
            requests,
            state_reads,
            state_writes,
            inputs,
            assignments,
        })
    }

    fn parse_effect_kind(&mut self) -> Result<EffectKind, ParseError> {
        match self.expect_ident()?.as_str() {
            "network" => Ok(EffectKind::Network),
            "filesystem" => Ok(EffectKind::FileSystem),
            "clock" => Ok(EffectKind::Clock),
            "random" => Ok(EffectKind::Random),
            "global_mutable" => Ok(EffectKind::GlobalMutable),
            _ => Err(ParseError { line: self.line() }),
        }
    }

    fn parse_interaction_type(&mut self) -> Result<InteractionType, ParseError> {
        let ty = self.expect_ident()?;
        match ty.as_str() {
            "int" => Ok(InteractionType::Int),
            "text" => Ok(InteractionType::Text),
            "bool" => Ok(InteractionType::Bool),
            _ => Err(ParseError { line: self.line() }),
        }
    }

    fn parse_interaction_expr(&mut self) -> Result<InteractionExpr, ParseError> {
        let mut expr = self.parse_interaction_atom()?;
        while matches!(self.peek().map(|t| t.kind), Some(TokenKind::Plus)) {
            self.bump();
            expr = InteractionExpr::Add(Box::new(expr), Box::new(self.parse_interaction_atom()?));
        }
        Ok(expr)
    }

    fn parse_interaction_atom(&mut self) -> Result<InteractionExpr, ParseError> {
        match self.peek().map(|t| t.kind) {
            Some(TokenKind::Number) => {
                let text = self
                    .bump()
                    .ok_or(ParseError { line: self.line() })?
                    .text
                    .replace('_', "");
                let value = text
                    .parse::<i64>()
                    .map_err(|_| ParseError { line: self.line() })?;
                Ok(InteractionExpr::Literal(InteractionValue::Int(value)))
            }
            Some(TokenKind::Str) => Ok(InteractionExpr::Literal(InteractionValue::Text(
                self.bump().ok_or(ParseError { line: self.line() })?.text,
            ))),
            Some(TokenKind::Ident) => {
                let head = self.expect_ident()?;
                match head.as_str() {
                    "true" => Ok(InteractionExpr::Literal(InteractionValue::Bool(true))),
                    "false" => Ok(InteractionExpr::Literal(InteractionValue::Bool(false))),
                    "state" | "input" | "effect" => {
                        self.expect(TokenKind::Dot)?;
                        let key = self.expect_ident()?;
                        if head == "state" {
                            Ok(InteractionExpr::State(key))
                        } else if head == "input" {
                            Ok(InteractionExpr::Input(key))
                        } else {
                            Ok(InteractionExpr::Effect(key))
                        }
                    }
                    _ => Err(ParseError { line: self.line() }),
                }
            }
            _ => Err(ParseError { line: self.line() }),
        }
    }

    fn expect_ident_value(&mut self, value: &str) -> Result<(), ParseError> {
        if self
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == value)
        {
            self.bump();
            Ok(())
        } else {
            Err(ParseError { line: self.line() })
        }
    }

    fn optional_semi(&mut self) {
        if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Semicolon)) {
            self.bump();
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
            "text" => match self.expect(TokenKind::Str) {
                Ok(t) => cell.text = Some(t.text),
                Err(e) => return Err(e),
            },
            "text-size" | "text_size" => cell.text_size = Some(self.parse_number()?),
            // `wrap` is a positive integer column count. 0, negatives,
            // non-integers and out-of-range values are parse errors — a
            // malformed wrap must not silently disable the feature.
            "wrap" => {
                let text = match self.peek() {
                    Some(t) if t.kind == TokenKind::Number => {
                        self.bump().expect("invariant").text.replace('_', "")
                    }
                    _ => return Err(ParseError { line: self.line() }),
                };
                let value: i64 = text
                    .parse()
                    .map_err(|_| ParseError { line: self.line() })?;
                if value <= 0 || value > u32::MAX as i64 {
                    return Err(ParseError { line: self.line() });
                }
                cell.wrap = Some(value as u32);
            }
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
                let text = self.bump().expect("invariant").text.replace('_', "");
                text.parse::<f64>()
                    .map_err(|_| ParseError { line: self.line() })
            }
            _ => Err(ParseError { line: self.line() }),
        }
    }

    fn parse_hex(&mut self) -> Result<u32, ParseError> {
        match self.peek() {
            Some(t) if t.kind == TokenKind::Hex => {
                let hex = self.bump().expect("invariant").text[1..].to_string();
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
