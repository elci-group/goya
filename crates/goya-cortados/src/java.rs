//! Java 17 source syntax and byte offsets into the original UTF-8 input.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Range;
use std::time::{Duration, Instant};

use serde::Serialize;
use tree_sitter::{Node, ParseOptions, ParseState, Parser};

use crate::{Diagnostic, Limits, Span};

#[derive(Debug, Clone, Serialize)]
pub struct ParsedFile {
    pub nodes: Vec<SyntaxNode>,
    pub diagnostics: Vec<Diagnostic>,
    pub syntax_complete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyntaxNode {
    pub kind: String,
    pub name: Option<String>,
    pub span: Span,
    pub parent: Option<usize>,
    pub fields: BTreeMap<String, String>,
}

/// Parse source syntax only. A complete result makes no claim about Java type
/// attribution, external symbol resolution, or native representability.
pub fn parse(path: &str, raw: &[u8], limits: &Limits) -> ParsedFile {
    let mut budget = Budget {
        started: Instant::now(),
        limits,
        owned_bytes: 0,
    };
    let mut parsed = ParsedFile {
        nodes: Vec::new(),
        diagnostics: Vec::new(),
        syntax_complete: false,
    };
    let result = parse_into(path, raw, &mut budget, &mut parsed);
    match result {
        Ok(()) => parsed.syntax_complete = parsed.diagnostics.is_empty(),
        Err(Failure::Invalid(span, message)) => {
            parsed
                .diagnostics
                .push(Diagnostic::new("COR-E100", path, span, message));
        }
        Err(Failure::Limit(limit)) => {
            parsed.diagnostics.push(Diagnostic::new(
                "COR-E500",
                path,
                None,
                format!("Java source analysis exhausted {limit}"),
            ));
        }
    }
    parsed
}

enum Failure {
    Invalid(Option<Span>, &'static str),
    Limit(&'static str),
}

struct Budget<'a> {
    started: Instant,
    limits: &'a Limits,
    owned_bytes: u64,
}

impl Budget<'_> {
    fn expired(&self) -> bool {
        self.started.elapsed() >= Duration::from_secs(self.limits.max_seconds_per_file)
    }

    fn check_time(&self) -> Result<(), Failure> {
        if self.expired() {
            return Err(Failure::Limit("max_seconds_per_file"));
        }
        Ok(())
    }

    // Limit the data this stage retains. The worker also enforces an address
    // space limit, which accounts for the C parser and allocator overhead.
    fn charge(&mut self, bytes: usize) -> Result<(), Failure> {
        self.owned_bytes = self.owned_bytes.saturating_add(bytes as u64);
        if self.owned_bytes > self.limits.max_memory_bytes {
            return Err(Failure::Limit("max_memory_bytes"));
        }
        Ok(())
    }
}

fn parse_into(
    path: &str,
    raw: &[u8],
    budget: &mut Budget<'_>,
    parsed: &mut ParsedFile,
) -> Result<(), Failure> {
    if raw.len() > budget.limits.max_file_bytes || raw.len() > u32::MAX as usize {
        return Err(Failure::Limit("max_file_bytes"));
    }
    budget.check_time()?;
    let raw = std::str::from_utf8(raw).map_err(|error| {
        let start = error.valid_up_to();
        let end = error.error_len().map_or(raw.len(), |len| start + len);
        Failure::Invalid(Some(Span { start, end }), "Java source must be valid UTF-8")
    })?;
    let source = Translated::new(raw, budget)?;
    budget.check_time()?;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .map_err(|_| Failure::Invalid(None, "Java grammar could not be loaded"))?;
    let bytes = source.text.as_bytes();
    let mut read = |offset: usize, _| {
        if offset >= bytes.len() || budget.expired() {
            return &bytes[0..0];
        }
        &bytes[offset..bytes.len().min(offset.saturating_add(4096))]
    };
    let mut cancel = |_: &ParseState| budget.expired();
    let options = ParseOptions::new().progress_callback(&mut cancel);
    let tree = parser.parse_with_options(&mut read, None, Some(options));
    budget.check_time()?;
    let tree = tree.ok_or(Failure::Invalid(
        None,
        "Java parser did not produce a syntax tree",
    ))?;
    let root = tree.root_node();
    let mut walk = Walk {
        source: &source,
        budget,
        parsed,
        path,
        tokens: 0,
        covered: 0,
        unit: CompilationUnit::default(),
    };
    walk.tree(root)?;
    walk.check_gap(bytes.len())?;
    walk.budget.check_time()
}

struct Translated<'a> {
    text: Cow<'a, str>,
    raw: &'a str,
    offsets: Vec<Offset>,
    surrogates: Vec<Range<usize>>,
}

struct Offset {
    translated_end: usize,
    raw_end: usize,
}

impl<'a> Translated<'a> {
    fn new(raw: &'a str, budget: &mut Budget<'_>) -> Result<Self, Failure> {
        let mut source = Self {
            text: Cow::Borrowed(raw),
            raw,
            offsets: Vec::new(),
            surrogates: Vec::new(),
        };
        let bytes = raw.as_bytes();
        if !bytes
            .iter()
            .any(|byte| matches!(byte, b'\\' | b'\r' | 0x1a))
        {
            return Ok(source);
        }
        budget.charge(raw.len())?;
        source.text = Cow::Owned(String::with_capacity(raw.len()));

        let mut start = 0;
        let mut check_at = 0;
        let mut odd_slashes = false;
        let mut previous_escape = false;
        let mut pending_cr = None;
        while start < raw.len() {
            if start >= check_at {
                budget.check_time()?;
                check_at = start.saturating_add(4096);
            }
            let ascii = bytes[start];
            let eligible = ascii == b'\\' && (previous_escape || !odd_slashes);
            let escaped = eligible && bytes.get(start + 1) == Some(&b'u');
            let (character, end, surrogate) = if escaped {
                let (code_unit, mut end) = unicode_escape(raw, start, budget)?;
                let mut codepoint = u32::from(code_unit);
                if (0xd800..=0xdbff).contains(&code_unit)
                    && bytes.get(end..end.saturating_add(2)) == Some(b"\\u")
                {
                    let (low, pair_end) = unicode_escape(raw, end, budget)?;
                    if (0xdc00..=0xdfff).contains(&low) {
                        codepoint =
                            0x10000 + ((codepoint - 0xd800) << 10) + u32::from(low - 0xdc00);
                        end = pair_end;
                    }
                }
                let character = char::from_u32(codepoint);
                // An isolated UTF-16 surrogate is legal in a Java literal or
                // comment. Use a non-identifier placeholder in the lexer and
                // recover the exact escaped spelling in emitted source fields.
                (character.unwrap_or('\u{fffd}'), end, character.is_none())
            } else {
                let character = if ascii.is_ascii() {
                    char::from(ascii)
                } else {
                    raw[start..].chars().next().expect("UTF-8 boundary")
                };
                (character, start + character.len_utf8(), false)
            };
            odd_slashes = character == '\\' && !odd_slashes;
            previous_escape = escaped;

            // JLS 3.4 recognizes CR, LF, and CRLF. The grammar's line comment
            // token expects LF, so normalize all three while preserving spans.
            if let Some(cr_end) = pending_cr.take() {
                if character == '\n' {
                    source.push('\n', end, false, budget)?;
                    start = end;
                    continue;
                }
                source.push('\n', cr_end, false, budget)?;
            }
            match character {
                '\r' => pending_cr = Some(end),
                '\u{1a}' if end == raw.len() => source.push(' ', end, false, budget)?,
                _ => source.push(character, end, surrogate, budget)?,
            }
            start = end;
        }
        if let Some(end) = pending_cr {
            source.push('\n', end, false, budget)?;
        }
        Ok(source)
    }

    fn push(
        &mut self,
        character: char,
        raw_end: usize,
        surrogate: bool,
        budget: &mut Budget<'_>,
    ) -> Result<(), Failure> {
        let start = self.text.len();
        self.text.to_mut().push(character);
        let translated_end = self.text.len();
        let prior_shift = self
            .offsets
            .last()
            .map_or(0, |point| point.raw_end - point.translated_end);
        if raw_end - translated_end != prior_shift {
            budget.charge(2 * std::mem::size_of::<Offset>())?;
            self.offsets.push(Offset {
                translated_end,
                raw_end,
            });
        }
        if surrogate {
            budget.charge(2 * std::mem::size_of::<Range<usize>>())?;
            self.surrogates.push(start..translated_end);
        }
        Ok(())
    }

    fn original(&self, translated: usize) -> usize {
        let count = self
            .offsets
            .partition_point(|point| point.translated_end <= translated);
        if count == 0 {
            return translated;
        }
        let point = &self.offsets[count - 1];
        point.raw_end + (translated - point.translated_end)
    }

    fn span(&self, range: Range<usize>) -> Span {
        Span {
            start: self.original(range.start),
            end: self.original(range.end),
        }
    }

    fn spelling(&self, range: Range<usize>) -> Cow<'_, str> {
        let Some(text) = self.text.get(range.clone()) else {
            return Cow::Borrowed("");
        };
        let first = self
            .surrogates
            .partition_point(|item| item.end <= range.start);
        let mut surrogates = self.surrogates[first..]
            .iter()
            .take_while(|item| item.end <= range.end)
            .peekable();
        if surrogates.peek().is_none() {
            return Cow::Borrowed(text);
        }
        let mut spelling = String::new();
        let mut start = range.start;
        for surrogate in surrogates {
            let original = self.span(surrogate.clone());
            spelling.push_str(&self.text[start..surrogate.start]);
            spelling.push_str(&self.raw[original.start..original.end]);
            start = surrogate.end;
        }
        spelling.push_str(&self.text[start..range.end]);
        Cow::Owned(spelling)
    }
}

fn unicode_escape(raw: &str, start: usize, budget: &Budget<'_>) -> Result<(u16, usize), Failure> {
    let bytes = raw.as_bytes();
    let mut digit = start + 2;
    while bytes.get(digit) == Some(&b'u') {
        if digit.is_multiple_of(4096) {
            budget.check_time()?;
        }
        digit += 1;
    }
    let mut value = 0;
    for offset in digit..digit + 4 {
        let character = bytes
            .get(offset)
            .and_then(|byte| char::from(*byte).to_digit(16));
        let Some(character) = character else {
            let end = offset
                + raw
                    .get(offset..)
                    .and_then(|rest| rest.chars().next())
                    .map_or(0, char::len_utf8);
            return Err(Failure::Invalid(
                Some(Span { start, end }),
                "Malformed Java Unicode escape",
            ));
        };
        value = value * 16 + character as u16;
    }
    Ok((value, digit + 4))
}

struct Walk<'a, 'b> {
    source: &'a Translated<'a>,
    budget: &'a mut Budget<'b>,
    parsed: &'a mut ParsedFile,
    path: &'a str,
    tokens: usize,
    covered: usize,
    unit: CompilationUnit,
}

impl Walk<'_, '_> {
    fn tree(&mut self, root: Node<'_>) -> Result<(), Failure> {
        let mut cursor = root.walk();
        let mut parents = Vec::new();
        loop {
            self.budget.check_time()?;
            if parents.len() >= self.budget.limits.max_ast_depth {
                return Err(Failure::Limit("max_ast_depth"));
            }
            let node = cursor.node();
            let (parent, inside_string) = parents.last().copied().unwrap_or((None, false));
            let own_index = self.visit(node, parent, inside_string, parents.len() == 1)?;

            if cursor.goto_first_child() {
                parents.push((
                    own_index.or(parent),
                    inside_string || node.kind() == "string_literal",
                ));
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    return Ok(());
                }
                parents.pop();
            }
        }
    }

    fn visit(
        &mut self,
        node: Node<'_>,
        parent: Option<usize>,
        inside_string: bool,
        top_level: bool,
    ) -> Result<Option<usize>, Failure> {
        let kind = node.kind();
        let range = node.byte_range();
        // Text blocks can contain hidden grammar tokens for line continuations.
        // Their entire spelling is validated as a literal below.
        if (kind == "string_literal" || (node.child_count() == 0 && kind != "program"))
            && !node.is_missing()
        {
            self.check_gap(range.start)?;
            self.covered = self.covered.max(range.end);
        }
        // Comments are Java input elements, not tokens. A whole string or
        // text block is a token, including its lexer fragments and escapes.
        if !node.is_missing()
            && !inside_string
            && !is_comment(kind)
            && (kind == "string_literal" || (node.child_count() == 0 && kind != "program"))
        {
            if self.tokens >= self.budget.limits.max_tokens_per_file {
                return Err(Failure::Limit("max_tokens_per_file"));
            }
            self.tokens += 1;
        }

        if node.is_error() || node.is_missing() {
            let message = if node.is_missing() {
                format!("Java parser expected {kind}")
            } else {
                "Invalid Java syntax".to_owned()
            };
            self.diagnose("COR-E100", range.clone(), message)?;
        } else if let Some(message) = later_feature(node, &self.source.text) {
            self.diagnose("COR-E200", range.clone(), message)?;
        } else if top_level {
            if let Some((code, message)) = self.unit.accept(kind) {
                self.diagnose(code, range.clone(), message)?;
            }
        }

        let text = self.source.text.get(range.clone()).unwrap_or_default();
        match kind {
            "string_literal" if !has_template_parent(node) && !node.has_error() => {
                self.check_literal(range.start, text, false)?;
            }
            "character_literal" => self.check_literal(range.start, text, true)?,
            "octal_integer_literal" if text.starts_with("0o") || text.starts_with("0O") => {
                self.diagnose(
                    "COR-E100",
                    range.clone(),
                    "Java octal literals begin with 0",
                )?;
            }
            "hex_floating_point_literal" if !text.contains(['p', 'P']) => {
                self.diagnose(
                    "COR-E100",
                    range.clone(),
                    "Java hexadecimal floating-point literals require a binary exponent",
                )?;
            }
            _ => {}
        }

        if !node.is_named() && !node.is_missing() {
            return Ok(None);
        }
        self.budget
            .charge(2 * std::mem::size_of::<SyntaxNode>() + kind.len())?;
        let mut fields = BTreeMap::new();
        self.fields(node, &mut fields)?;
        if node.is_missing() {
            self.field(&mut fields, "missing", "true")?;
        }
        let name = fields.get("name").cloned();
        let index = self.parsed.nodes.len();
        self.parsed.nodes.push(SyntaxNode {
            kind: kind.into(),
            name,
            span: self.source.span(range),
            parent,
            fields,
        });
        Ok(Some(index))
    }

    fn diagnose(
        &mut self,
        code: &str,
        range: Range<usize>,
        message: impl Into<String>,
    ) -> Result<(), Failure> {
        let message = message.into();
        self.budget.charge(
            2 * std::mem::size_of::<Diagnostic>() + self.path.len() + message.len() + 150,
        )?;
        self.parsed.diagnostics.push(Diagnostic::new(
            code,
            self.path,
            Some(self.source.span(range)),
            message,
        ));
        Ok(())
    }

    fn check_gap(&mut self, end: usize) -> Result<(), Failure> {
        if self.covered >= end {
            return Ok(());
        }
        let start = self.covered;
        let gap = self.source.text.get(start..end).ok_or(Failure::Invalid(
            None,
            "Java parser returned an invalid UTF-8 boundary",
        ))?;
        if let Some((offset, ch)) = gap.char_indices().find(|(_, ch)| !is_java_whitespace(*ch)) {
            let start = start + offset;
            self.diagnose(
                "COR-E100",
                start..start + ch.len_utf8(),
                "Unexpected input between Java tokens",
            )?;
        }
        self.covered = end;
        Ok(())
    }

    fn check_literal(&mut self, offset: usize, text: &str, character: bool) -> Result<(), Failure> {
        if let Some((range, message)) = literal_error(text, character) {
            self.diagnose(
                "COR-E100",
                offset + range.start..offset + range.end,
                message,
            )?;
        }
        Ok(())
    }

    fn field(
        &mut self,
        fields: &mut BTreeMap<String, String>,
        key: &str,
        value: &str,
    ) -> Result<(), Failure> {
        let copies = usize::from(key == "name") + 1;
        self.budget.charge(96 + key.len() + copies * value.len())?;
        fields.insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn fields(
        &mut self,
        node: Node<'_>,
        fields: &mut BTreeMap<String, String>,
    ) -> Result<(), Failure> {
        let kind = node.kind();
        let source = self.source;
        if node.child_count() == 0
            || matches!(
                kind,
                "modifiers" | "string_literal" | "scoped_type_identifier"
            )
        {
            let text = source.spelling(node.byte_range());
            self.field(fields, "text", &text)?;
            if matches!(
                kind,
                "identifier" | "type_identifier" | "scoped_type_identifier"
            ) {
                self.field(fields, "name", &text)?;
            }
        }
        if node.child_count() == 0 {
            return Ok(());
        }

        let mut cursor = node.walk();
        if !cursor.goto_first_child() {
            return Ok(());
        }
        let declaration_name = matches!(kind, "package_declaration" | "import_declaration");
        let mut is_static = false;
        let mut wildcard = false;
        let mut reference_separator = false;
        loop {
            self.budget.check_time()?;
            let child = cursor.node();
            let child_kind = child.kind();
            if declaration_name {
                match child_kind {
                    "identifier" | "scoped_identifier" => {
                        self.field(fields, "name", &qualified_name(child, source, self.budget)?)?;
                    }
                    "static" => is_static = true,
                    "asterisk" => wildcard = true,
                    _ => {}
                }
            } else if kind == "method_reference" && !child.is_missing() && !is_comment(child_kind) {
                if child_kind == "::" {
                    reference_separator = true;
                } else if !reference_separator {
                    self.field(fields, "object", &source.spelling(child.byte_range()))?;
                } else if matches!(child_kind, "new" | "identifier") {
                    self.field(fields, "name", &source.spelling(child.byte_range()))?;
                }
            } else if kind == "modifiers" && !child.is_named() {
                self.field(fields, child_kind, "true")?;
            } else if let Some(key) = cursor.field_name() {
                if !child.is_missing()
                    && matches!(
                        key,
                        "name"
                            | "type"
                            | "object"
                            | "field"
                            | "operator"
                            | "module"
                            | "package"
                            | "provided"
                            | "constructor"
                    )
                {
                    let value = if child_kind == "scoped_identifier" {
                        Cow::Owned(qualified_name(child, source, self.budget)?)
                    } else {
                        source.spelling(child.byte_range())
                    };
                    self.field(fields, key, &value)?;
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        if kind == "import_declaration" {
            self.field(fields, "static", if is_static { "true" } else { "false" })?;
            self.field(fields, "wildcard", if wildcard { "true" } else { "false" })?;
        }
        Ok(())
    }
}

fn qualified_name(
    node: Node<'_>,
    source: &Translated<'_>,
    budget: &Budget<'_>,
) -> Result<String, Failure> {
    let mut name = String::new();
    let mut cursor = node.walk();
    loop {
        budget.check_time()?;
        let current = cursor.node();
        match current.kind() {
            "." | "identifier" => name.push_str(&source.spelling(current.byte_range())),
            "scoped_identifier" if cursor.goto_first_child() => continue,
            _ => {}
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Ok(name);
            }
        }
    }
}

fn is_comment(kind: &str) -> bool {
    matches!(kind, "line_comment" | "block_comment")
}

fn is_java_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c')
}

fn has_template_parent(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| parent.kind() == "template_expression")
}

fn later_feature(node: Node<'_>, source: &str) -> Option<&'static str> {
    match node.kind() {
        "pattern" | "guard" => Some("Switch patterns require preview or a later Java release"),
        "record_pattern" => Some("Record patterns require a later Java release"),
        "template_expression" | "string_interpolation" => {
            Some("String templates require a later Java preview release")
        }
        "underscore_pattern" => Some("Unnamed patterns require a later Java release"),
        "identifier" | "type_identifier" if source.get(node.byte_range()) == Some("_") => {
            Some("A single underscore is reserved in Java 17")
        }
        "null_literal"
            if node
                .parent()
                .is_some_and(|parent| parent.kind() == "switch_label") =>
        {
            Some("Null switch labels require preview or a later Java release")
        }
        _ => None,
    }
}

#[derive(Default)]
struct CompilationUnit {
    package: bool,
    imports: bool,
    types: bool,
    module: bool,
}

impl CompilationUnit {
    fn accept(&mut self, kind: &str) -> Option<(&'static str, &'static str)> {
        match kind {
            "package_declaration"
                if !self.package && !self.imports && !self.types && !self.module =>
            {
                self.package = true
            }
            "import_declaration" if !self.types && !self.module => self.imports = true,
            "module_declaration" if !self.package && !self.types && !self.module => {
                self.module = true
            }
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
            | ";"
                if !self.module =>
            {
                self.types = true
            }
            "line_comment" | "block_comment" => {}
            "method_declaration" | "local_variable_declaration" => {
                return Some(("COR-E200", "Top-level members require an implicitly declared class in a later Java release"));
            }
            _ => {
                return Some((
                    "COR-E100",
                    "Declaration or statement is out of place in a Java 17 compilation unit",
                ))
            }
        }
        None
    }
}

fn literal_error(text: &str, character: bool) -> Option<(Range<usize>, &'static str)> {
    let multiline = !character && text.starts_with("\"\"\"");
    let quotes = if multiline { 3 } else { 1 };
    let end = text.len().checked_sub(quotes)?;
    let mut start = quotes;
    let bytes = text.as_bytes();
    if multiline {
        while matches!(bytes.get(start), Some(b' ' | b'\t' | b'\x0c')) {
            start += 1;
        }
        if bytes.get(start) != Some(&b'\n') {
            return Some((
                0..start,
                "Java text block opening delimiter must end with a line terminator",
            ));
        }
    }
    let mut units = 0;
    while start < end {
        let ch = text[start..].chars().next()?;
        if ch == '\\' {
            let escape = start;
            start += 1;
            let following = text[start..end].chars().next()?;
            start += following.len_utf8();
            match following {
                'b' | 't' | 'n' | 'f' | 'r' | 's' | '"' | '\'' | '\\' => {}
                '\n' if multiline => {}
                '0'..='7' => {
                    let extra = if following <= '3' { 2 } else { 1 };
                    for _ in 0..extra {
                        if start >= end || !matches!(bytes[start], b'0'..=b'7') {
                            break;
                        }
                        start += 1;
                    }
                }
                _ => return Some((escape..start, "Invalid Java literal escape sequence")),
            }
            units += 1;
        } else {
            let next = start + ch.len_utf8();
            if !multiline && ch == '\n' {
                return Some((
                    start..next,
                    "Line terminator in Java string or character literal",
                ));
            }
            units += ch.len_utf16();
            start = next;
        }
    }
    if character && units != 1 {
        return Some((
            quotes..end,
            "Java character literal must denote one UTF-16 code unit",
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_source(source: &str) -> ParsedFile {
        parse("Corpus.java", source.as_bytes(), &Limits::default())
    }

    fn assert_complete(source: &str) -> ParsedFile {
        let parsed = parse_source(source);
        assert!(parsed.syntax_complete, "{:#?}", parsed.diagnostics);
        for (index, node) in parsed.nodes.iter().enumerate() {
            assert!(source.get(node.span.start..node.span.end).is_some());
            if let Some(parent) = node.parent {
                assert!(parent < index);
                let parent = parsed.nodes[parent].span;
                assert!(parent.start <= node.span.start && parent.end >= node.span.end);
            }
        }
        parsed
    }

    fn has_node(parsed: &ParsedFile, kind: &str) -> bool {
        parsed.nodes.iter().any(|node| node.kind == kind)
    }

    #[test]
    fn java17_modern_and_nested_constructs() {
        let source = r#"
            @Deprecated package example;
            import java.util.List;
            import static java.util.Collections./* trivia */emptyList;
            import java.util.*;
            ;
            sealed interface Shape permits Disk, OpenShape {}
            record Disk(int radius) implements Shape {
                Disk { if (radius < 0) throw new IllegalArgumentException(); }
            }
            non-sealed class OpenShape implements Shape {}
            @interface Mark { String value() default "ok"; }
            enum Status { READY, WAITING; }
            class Corpus<T extends Number & Comparable<T>> {
                static { int[] values = { 0x32, 0b10, 077, 1_000 }; }
                String message = """
                    first\s
                    second\
                    third
                    """;
                String assess(Object o, List<? extends T> values) {
                    class Local { int n = values.size(); }
                    Runnable anonymous = new Runnable() { public void run() {} };
                    Runnable reference = anonymous::run;
                    Supplier<Local> factory = Local::new;
                    Runnable lambda = () -> reference.run();
                    try (var resource = open()) {
                        for (T value : values) System.out.println(value);
                        if (o instanceof final Disk d) return switch (d.radius()) {
                            case 0, 1 -> "small";
                            default -> { yield "large"; }
                        };
                        return o instanceof String s ? s : message;
                    } catch (RuntimeException | Error e) { throw e; }
                }
            }
        "#;
        let parsed = assert_complete(source);
        for kind in [
            "record_declaration",
            "compact_constructor_declaration",
            "permits",
            "enum_declaration",
            "annotation_type_declaration",
            "switch_expression",
            "instanceof_expression",
            "yield_statement",
            "try_with_resources_statement",
            "lambda_expression",
            "method_reference",
            "object_creation_expression",
            "class_declaration",
            "multiline_string_fragment",
            "hex_integer_literal",
        ] {
            assert!(has_node(&parsed, kind), "missing {kind}");
        }
        let invocation = parsed
            .nodes
            .iter()
            .find(|node| {
                node.kind == "method_invocation" && node.name.as_deref() == Some("println")
            })
            .unwrap();
        assert_eq!(invocation.fields["object"], "System.out");
        let reference = parsed
            .nodes
            .iter()
            .find(|node| node.kind == "method_reference" && node.name.as_deref() == Some("new"))
            .unwrap();
        assert_eq!(reference.fields["object"], "Local");
        let static_import = parsed
            .nodes
            .iter()
            .find(|node| {
                node.name.as_deref() == Some("java.util.Collections.emptyList")
                    && node.kind == "import_declaration"
            })
            .unwrap();
        assert_eq!(static_import.fields["static"], "true");
        assert_eq!(static_import.fields["wildcard"], "false");
        assert!(parsed.nodes.iter().any(|node| {
            node.kind == "import_declaration" && node.fields["wildcard"] == "true"
        }));
    }

    #[test]
    fn java17_modular_compilation_unit() {
        let source = r#"
            import java.lang.Deprecated;
            @Deprecated module my /* trivia */ .tool {
                requires static java.sql;
                requires transitive java.logging;
                exports my.tool.api to test.module1, test.module2;
                opens my.tool.internal to other.module;
                uses my.tool.Service;
                provides my.tool.Service with my.tool.Provider, my.tool.Other;
            }
        "#;
        let parsed = assert_complete(source);
        let module = parsed
            .nodes
            .iter()
            .find(|node| node.kind == "module_declaration")
            .unwrap();
        assert_eq!(module.name.as_deref(), Some("my.tool"));
        for kind in [
            "requires_module_directive",
            "exports_module_directive",
            "opens_module_directive",
            "uses_module_directive",
            "provides_module_directive",
        ] {
            assert!(has_node(&parsed, kind), "missing {kind}");
        }
        assert_complete("open module service { requires java.base; }");
    }

    #[test]
    fn original_utf8_spans_cover_escapes_comments_and_astral_identifiers() {
        let source = concat!(
            r"\u002f\u002f π 😀\u000d",
            "\n",
            r"cl\uuuu0061ss \uD801\uDC00 {",
            "\r\n",
            r#"  String café = "pré\uD83D\uDE00";"#,
            "\r",
            r"  char c = '\uD800';\u000d  int value; }",
        );
        let parsed = assert_complete(source);
        let name = parsed
            .nodes
            .iter()
            .find(|node| node.name.as_deref() == Some("𐐀") && node.kind == "identifier")
            .unwrap();
        assert_eq!(&source[name.span.start..name.span.end], r"\uD801\uDC00");
        let class = parsed
            .nodes
            .iter()
            .find(|node| node.kind == "class_declaration")
            .unwrap();
        assert!(source[class.span.start..class.span.end].starts_with(r"cl\uuuu0061ss"));
        let comment = parsed
            .nodes
            .iter()
            .find(|node| node.kind == "line_comment")
            .unwrap();
        assert_eq!(
            &source[comment.span.start..comment.span.end],
            r"\u002f\u002f π 😀"
        );
        assert_eq!(comment.fields["text"], "// π 😀");
        let literal = parsed
            .nodes
            .iter()
            .find(|node| node.kind == "string_fragment")
            .unwrap();
        assert_eq!(
            &source[literal.span.start..literal.span.end],
            r"pré\uD83D\uDE00"
        );
        assert_eq!(literal.fields["text"], "pré😀");
        let character = parsed
            .nodes
            .iter()
            .find(|node| node.kind == "character_literal")
            .unwrap();
        assert_eq!(character.fields["text"], r"'\uD800'");
    }

    #[test]
    fn unicode_eligibility_tracks_raw_parity_and_previous_escape() {
        let source = r#"class A {
            String ignored = "\\u00xz";
            String decoded = "\\\u0061";
            String consecutive = "\u005c\u005c";
            String parityAfterEscape = "\u005c\\u0061";
        }"#;
        let parsed = assert_complete(source);
        let strings: Vec<_> = parsed
            .nodes
            .iter()
            .filter(|node| node.kind == "string_literal")
            .map(|node| node.fields["text"].as_str())
            .collect();
        assert_eq!(strings, [r#""\\u00xz""#, r#""\\a""#, r#""\\""#, r#""\\a""#]);

        let source = r#"class A { String bad = "\u005cu005A"; }"#;
        let parsed = parse_source(source);
        assert!(!parsed.syntax_complete);
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|item| item.message == "Invalid Java literal escape sequence")
            .unwrap();
        let span = diagnostic.span.unwrap();
        assert_eq!(&source[span.start..span.end], r"\u005cu");
    }

    #[test]
    fn malformed_utf8_unicode_and_syntax_report_original_spans() {
        let invalid = b"class A \xff";
        let parsed = parse("Corpus.java", invalid, &Limits::default());
        assert!(!parsed.syntax_complete);
        assert_eq!(parsed.diagnostics[0].span, Some(Span { start: 8, end: 9 }));

        let invalid = r"// ignored? \uu0Å00";
        let parsed = parse_source(invalid);
        assert!(!parsed.syntax_complete);
        let diagnostic = &parsed.diagnostics[0];
        assert_eq!(diagnostic.code, "COR-E100");
        let span = diagnostic.span.unwrap();
        assert_eq!(&invalid[span.start..span.end], r"\uu0Å");

        let invalid = r"cl\u0061ss A { int value }";
        let parsed = parse_source(invalid);
        assert!(!parsed.syntax_complete);
        assert!(parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "COR-E100"));
        assert!(parsed
            .nodes
            .iter()
            .any(|node| node.kind == "ERROR" || node.fields.contains_key("missing")));
        for diagnostic in parsed.diagnostics {
            let span = diagnostic.span.unwrap();
            assert!(span.start >= invalid.find("int").unwrap());
            assert!(invalid.get(span.start..span.end).is_some());
        }
    }

    #[test]
    fn rejects_later_and_preview_forms_with_java_profile_diagnostic() {
        for source in [
            "class A { int f(Object o) { return switch (o) { case String s -> 1; default -> 0; }; } }",
            "class A { int f(Object o) { return switch (o) { case null -> 1; default -> 0; }; } }",
            "class A { boolean f(Object o) { return o instanceof P(int x); } }",
            r#"class A { String s = STR."hello \{1}"; }"#,
            "void main() {}",
            r"class A { int \u005f; }",
        ] {
            let result = parse_source(source);
            assert!(!result.syntax_complete, "{source}");
            assert!(result.diagnostics.iter().any(|diagnostic| diagnostic.code == "COR-E200" && diagnostic.span.is_some()), "{source}: {:?}", result.diagnostics);
        }
    }

    #[test]
    fn rejects_permissive_grammar_extensions_and_bad_literal_tokens() {
        for source in [
            "while (true) {}",
            "class A {} import java.util.List;",
            "class A {} module bad {}",
            "package foo; package bar;",
            "class A { String s = \"broken\nstring\"; }",
            r#"class A { String s = "\q"; }"#,
            r#"class A { String s = """no newline"""; }"#,
            r#"class A { char c = 'abc'; }"#,
            r#"class A { char c = '\777'; }"#,
            r#"class A { char c = '\uD83D\uDE00'; }"#,
            "class A { int octal = 0o123; }",
            "class A { double noExponent = 0x1.f; }",
            "class\u{000b}A {}",
        ] {
            let result = parse_source(source);
            assert!(!result.syntax_complete, "{source}");
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "COR-E100" && diagnostic.span.is_some()),
                "{source}: {:?}",
                result.diagnostics
            );
        }
    }

    #[test]
    fn rejects_resource_budget_exhaustion() {
        let source = "/* comment */ class A { String s = \"hi\\nworld\"; }";
        let limits = Limits {
            max_tokens_per_file: 9,
            ..Limits::default()
        };
        assert!(parse("Corpus.java", source.as_bytes(), &limits).syntax_complete);
        for (limit, limits) in [
            (
                "max_file_bytes",
                Limits {
                    max_file_bytes: 1,
                    ..Limits::default()
                },
            ),
            (
                "max_seconds_per_file",
                Limits {
                    max_seconds_per_file: 0,
                    ..Limits::default()
                },
            ),
            (
                "max_tokens_per_file",
                Limits {
                    max_tokens_per_file: 8,
                    ..Limits::default()
                },
            ),
            (
                "max_ast_depth",
                Limits {
                    max_ast_depth: 2,
                    ..Limits::default()
                },
            ),
            (
                "max_memory_bytes",
                Limits {
                    max_memory_bytes: 1,
                    ..Limits::default()
                },
            ),
        ] {
            let result = parse("Corpus.java", source.as_bytes(), &limits);
            assert!(!result.syntax_complete, "{limit}");
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "COR-E500"
                        && diagnostic.message.contains(limit)),
                "{limit}: {:?}",
                result.diagnostics
            );
        }
    }
}
