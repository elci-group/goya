//! Wave 4 (GY-ZCODE-004) markdown block renderer — pure assistant text →
//! styled plain lines for the 56-column transcript.
//!
//! Supported constructs (frozen Wave 4 contract):
//! - ATX headings `#`..`######` → `■ ` prefix (h1) / `▸ ` (h2+), heading
//!   text uppercased.
//! - Fenced code blocks: lines get a `| ` gutter + 2-space indent, fence
//!   lines are dropped, a leading language tag renders as `| lang: rust`.
//! - Bullets `-`/`*`/`+` → `• `.
//! - Ordered lists kept as-is with a 2-space indent.
//! - Bold `**x**` and italic `*x*` markers stripped; inline code `` `x` ``
//!   kept with backticks dropped; links `[t](u)` → `t`, with `t (u)` only
//!   when the text differs from the URL.
//! - Blank lines separate paragraphs.
//! - Long lines wrapped at the transcript width (same greedy algorithm as
//!   [`crate::view::wrap_line`]).

use crate::view::wrap_line;

/// Render markdown `text` into plain lines, wrapped at `width`.
pub fn render_markdown(text: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_code = false;

    for raw in text.lines() {
        let line = raw.trim_end();
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            if in_code {
                // Closing fence: dropped, exit code mode.
                in_code = false;
            } else {
                in_code = true;
                let lang = rest.trim();
                if !lang.is_empty() {
                    out.push(format!("| lang: {lang}"));
                }
            }
            continue;
        }
        if in_code {
            out.push(format!("|   {line}"));
            continue;
        }
        if line.trim().is_empty() {
            // Paragraph separator (empty lines are skipped by the tail
            // push, but the renderer keeps them as explicit separators).
            out.push(String::new());
            continue;
        }
        let trimmed = line.trim_start();

        // ATX headings.
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) {
            let rest = trimmed[hashes..].trim_start();
            if !rest.is_empty() {
                let prefix = if hashes == 1 { "■ " } else { "▸ " };
                let rendered = format!("{prefix}{}", rest.to_uppercase());
                out.extend(wrap_line(&rendered, width));
                continue;
            }
        }

        // Bullets → `• `.
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            let rendered = format!("• {}", strip_inline(rest));
            out.extend(wrap_line(&rendered, width));
            continue;
        }

        // Ordered lists: kept as-is with a 2-space indent.
        if starts_ordered(trimmed) {
            let rendered = format!("  {}", strip_inline(trimmed));
            out.extend(wrap_line(&rendered, width));
            continue;
        }

        out.extend(wrap_line(&strip_inline(trimmed), width));
    }
    out
}

/// `1.`/`12)` style markers count as ordered-list items.
fn starts_ordered(s: &str) -> bool {
    let mut chars = s.char_indices();
    let Some((_, first)) = chars.next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    let digits_end = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let rest = &s[digits_end..];
    rest.starts_with('.') || rest.starts_with(')')
}

/// Strip bold/italic markers and link syntax; inline code keeps its text
/// with the backticks dropped.
fn strip_inline(s: &str) -> String {
    // Links first: `[t](u)` → `t` (or `t (u)` when t != u).
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                // Inline code: keep content, drop both fences.
                if let Some(end) = s[i + 1..].find('`') {
                    out.push_str(&s[i + 1..i + 1 + end]);
                    i += end + 2;
                } else {
                    i += 1; // unmatched fence: drop the backtick
                }
            }
            b'[' => {
                if let Some(close) = s[i + 1..].find(']') {
                    let text = &s[i + 1..i + 1 + close];
                    if s[i + 2 + close..].starts_with('(') {
                        if let Some(paren) = s[i + 3 + close..].find(')') {
                            let url = &s[i + 3 + close..i + 3 + close + paren];
                            out.push_str(text);
                            if text != url {
                                out.push_str(&format!(" ({url})"));
                            }
                            i += 3 + close + paren + 1;
                            continue;
                        }
                    }
                }
                out.push('[');
                i += 1;
            }
            b'*' => {
                // Bold/italic markers stripped; content kept.
                i += if bytes.get(i + 1) == Some(&b'*') { 2 } else { 1 };
            }
            _ => {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(s: &str) -> Vec<String> {
        render_markdown(s, 56)
    }

    #[test]
    fn h1_gets_square_and_uppercase() {
        assert_eq!(render("# a small title"), vec!["■ A SMALL TITLE"]);
    }

    #[test]
    fn h2_and_deeper_get_arrow() {
        assert_eq!(render("## sub heading"), vec!["▸ SUB HEADING"]);
        assert_eq!(render("###### deep"), vec!["▸ DEEP"]);
    }

    #[test]
    fn fences_become_gutter_lines() {
        let lines = render("```rust\nfn main() {}\nlet x = 1;\n```\nafter");
        assert_eq!(
            lines,
            vec!["| lang: rust", "|   fn main() {}", "|   let x = 1;", "after"]
        );
    }

    #[test]
    fn fence_without_lang_has_no_tag() {
        let lines = render("```\ncode\n```");
        assert_eq!(lines, vec!["|   code"]);
    }

    #[test]
    fn bullets_become_dots() {
        assert_eq!(render("- first"), vec!["• first"]);
        assert_eq!(render("* second"), vec!["• second"]);
        assert_eq!(render("+ third"), vec!["• third"]);
    }

    #[test]
    fn ordered_lists_kept_with_indent() {
        assert_eq!(render("1. one"), vec!["  1. one"]);
        assert_eq!(render("12. twelve"), vec!["  12. twelve"]);
        assert_eq!(render("2) paren"), vec!["  2) paren"]);
    }

    #[test]
    fn bold_italic_stripped_code_kept() {
        assert_eq!(render("this is **bold** text"), vec!["this is bold text"]);
        assert_eq!(render("an *italic* word"), vec!["an italic word"]);
        assert_eq!(render("run `cargo check` now"), vec!["run cargo check now"]);
        assert_eq!(
            render("**bold** and *italic* mixed"),
            vec!["bold and italic mixed"]
        );
    }

    #[test]
    fn links_simplify() {
        // Same text and URL: URL dropped.
        assert_eq!(
            render("see [docs](docs) here"),
            vec!["see docs here"]
        );
        // Different: URL kept in parens.
        assert_eq!(
            render("see [the docs](https://example.com) now"),
            vec!["see the docs (https://example.com) now"]
        );
    }

    #[test]
    fn blank_lines_separate_paragraphs() {
        let lines = render("para one\n\npara two");
        assert_eq!(lines, vec!["para one", "", "para two"]);
    }

    #[test]
    fn long_lines_wrap_at_width() {
        let long = "word ".repeat(40).trim_end().to_string();
        for line in render(&long) {
            assert!(line.chars().count() <= 56, "line too long: {line}");
        }
        // Re-joining wrapped pieces preserves all words.
        let joined = render(&long).join(" ");
        assert_eq!(joined, long);
    }

    #[test]
    fn heading_writes_survive_wrapping() {
        let lines = render("# the quick brown fox jumps over the lazy dog again");
        assert!(lines[0].starts_with("■ "));
        assert!(lines.iter().all(|l| l.chars().count() <= 56));
    }

    #[test]
    fn combined_document_renders_every_construct() {
        let doc = "# report\n\n- item one\n- item two with **bold**\n\n```rust\nlet a = 1;\n```\n\n1. first\n2. second\n\nPlain closing [text](text).";
        let lines = render(doc);
        assert_eq!(
            lines,
            vec![
                "■ REPORT",
                "",
                "• item one",
                "• item two with bold",
                "",
                "| lang: rust",
                "|   let a = 1;",
                "",
                "  1. first",
                "  2. second",
                "",
                "Plain closing text.",
            ]
        );
    }
}
