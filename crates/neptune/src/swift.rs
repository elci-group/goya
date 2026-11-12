//! A deliberately small Swift expression representation for future lowering.
//! Rendering is deterministic and never accepts raw source fragments.

use std::fmt::Write;

/// A conservative Swift identifier; keywords and non-ASCII identifiers are
/// rejected rather than relying on context-dependent escaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identifier(String);

impl Identifier {
    pub fn new(value: &str) -> Option<Self> {
        let mut chars = value.chars();
        let first = chars.next()?;
        if !(first.is_ascii_alphabetic() || first == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            || matches!(
                value,
                "associatedtype"
                    | "class"
                    | "deinit"
                    | "enum"
                    | "extension"
                    | "fileprivate"
                    | "func"
                    | "import"
                    | "init"
                    | "inout"
                    | "internal"
                    | "let"
                    | "open"
                    | "operator"
                    | "private"
                    | "protocol"
                    | "public"
                    | "rethrows"
                    | "static"
                    | "struct"
                    | "subscript"
                    | "typealias"
                    | "var"
                    | "break"
                    | "case"
                    | "continue"
                    | "default"
                    | "defer"
                    | "do"
                    | "else"
                    | "fallthrough"
                    | "for"
                    | "guard"
                    | "if"
                    | "in"
                    | "repeat"
                    | "return"
                    | "switch"
                    | "where"
                    | "while"
                    | "as"
                    | "Any"
                    | "catch"
                    | "false"
                    | "is"
                    | "nil"
                    | "super"
                    | "self"
                    | "Self"
                    | "throw"
                    | "throws"
                    | "true"
                    | "try"
                    | "_"
                    | "await"
                    | "async"
                    | "actor"
                    | "some"
                    | "any"
                    | "macro"
                    | "package"
            )
        {
            return None;
        }
        Some(Self(value.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    StringLiteral(String),
    Identifier(Identifier),
    Call {
        callee: Identifier,
        arguments: Vec<Argument>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argument {
    pub label: Option<Identifier>,
    pub value: Expr,
}

impl Expr {
    /// Render a canonical expression with no trailing newline.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Self::Identifier(id) => out.push_str(&id.0),
            Self::StringLiteral(value) => {
                out.push('"');
                for c in value.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => {
                            let _ = write!(out, "\\u{{{:x}}}", c as u32);
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Self::Call { callee, arguments } => {
                out.push_str(&callee.0);
                out.push('(');
                for (index, argument) in arguments.iter().enumerate() {
                    if index != 0 {
                        out.push_str(", ");
                    }
                    if let Some(label) = &argument.label {
                        out.push_str(&label.0);
                        out.push_str(": ");
                    }
                    argument.value.write_to(out);
                }
                out.push(')');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_content_cannot_break_out_or_interpolate() {
        let value = Expr::StringLiteral("\"\\(danger)\n\0é\u{2028}".into());
        assert_eq!(value.render(), "\"\\\"\\\\(danger)\\n\\u{0}é\\u{2028}\"");
    }

    #[test]
    fn validates_identifiers_and_formats_calls() {
        for name in ["", "2bad", "foo.bar", "class", "é", "_"] {
            assert!(Identifier::new(name).is_none());
        }
        let call = Expr::Call {
            callee: Identifier::new("Text").unwrap(),
            arguments: vec![Argument {
                label: None,
                value: Expr::StringLiteral("Hello".into()),
            }],
        };
        assert_eq!(call.render(), "Text(\"Hello\")");
    }
}
