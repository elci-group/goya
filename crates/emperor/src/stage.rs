//! Thirteen pipeline stages. A blocked stage stays blocked.

use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Blocked,
    NotApplicable,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Blocked => "BLOCKED",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

pub const NAMES: [&str; 13] = [
    "lexical-syntactic-admission",
    "type-semantic-validation",
    "canonical-goya-ir",
    "effects-capabilities",
    "target-negotiation",
    "native-linux-lowering",
    "optimisation",
    "runtime-platform-linking",
    "machine-code-generation",
    "elf-assembly-linking",
    "static-verification",
    "runtime-conformance",
    "packaging-signing-release",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    pub id: u8,
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stages {
    items: Vec<Stage>,
}

impl Stages {
    pub fn new() -> Self {
        Self {
            items: Vec::with_capacity(NAMES.len()),
        }
    }

    pub fn push(&mut self, status: Status, detail: impl Into<String>) {
        let index = self.items.len();
        assert!(index < NAMES.len(), "too many stages");
        self.items.push(Stage {
            id: (index as u8) + 1,
            name: NAMES[index],
            status,
            detail: detail.into(),
        });
    }

    pub fn fill(&mut self, status: Status, detail: &str) {
        while self.items.len() < NAMES.len() {
            self.push(status, detail);
        }
    }

    pub fn items(&self) -> &[Stage] {
        &self.items
    }

    pub fn has(&self, status: Status) -> bool {
        self.items.iter().any(|stage| stage.status == status)
    }

    pub fn json(&self) -> serde_json::Value {
        json!(self
            .items
            .iter()
            .map(|stage| {
                json!({
                    "id": stage.id,
                    "name": stage.name,
                    "status": stage.status.as_str(),
                    "detail": stage.detail,
                })
            })
            .collect::<Vec<_>>())
    }
}

impl Default for Stages {
    fn default() -> Self {
        Self::new()
    }
}
