use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub path: String,
    pub span: Option<Span>,
    pub requirement: String,
    pub message: String,
    pub remediation: String,
}

impl Diagnostic {
    pub fn new(code: &str, path: &str, span: Option<Span>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: "error".into(),
            path: path.into(),
            span,
            requirement: "CORTADOS-JAVA-SOURCE/17.0".into(),
            message: message.into(),
            remediation: "Supply supported source and declared dependencies; inspect the exact source location.".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    pub max_entries: usize,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_path_depth: usize,
    pub max_path_bytes: usize,
    pub max_tokens_per_file: usize,
    pub max_ast_depth: usize,
    pub max_seconds_per_file: u64,
    pub max_memory_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 20_000,
            max_file_bytes: 32 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            max_path_depth: 64,
            max_path_bytes: 1024 * 1024,
            max_tokens_per_file: 8_000_000,
            max_ast_depth: 256,
            max_seconds_per_file: 60,
            max_memory_bytes: 1024 * 1024 * 1024,
        }
    }
}

pub const PROFILE: &str = "CORTADOS-JAVA-SOURCE/17.0";
