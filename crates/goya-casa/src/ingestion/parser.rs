//! CSS parser integration with goya-css-syntax.

use goya_css_syntax::{parse_stylesheet, Rule, DeclarationItem};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("CSS syntax error: {0}")]
    Syntax(String),
    #[error("Import resolution failed: {0}")]
    ImportResolution(String),
}

pub struct ParsedStylesheet {
    pub rules: Vec<Rule>,
    pub source_map: SourceMap,
}

pub struct SourceMap {
    // Byte spans into original input
}

impl ParsedStylesheet {
    pub fn from_str(input: &str) -> Result<Self, ParseError> {
        let rules = parse_stylesheet(input);
        Ok(Self {
            rules,
            source_map: SourceMap {},
        })
    }
}

pub fn parse_stylesheet_string(input: &str) -> Result<Vec<Rule>, ParseError> {
    Ok(goya_css_syntax::parse_stylesheet(input))
}

pub fn parse_declaration_list(input: &str) -> Result<Vec<DeclarationItem>, ParseError> {
    Ok(goya_css_syntax::parse_declaration_list(input))
}