//! Typed Manuscrito Intermediate Representation report contract.

use serde::{Deserialize, Serialize};

pub const MIR_VERSION: &str = "goya.manuscrito.mir/1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirReport {
    pub version: String,
    pub status: MirStatus,
    pub nodes: Vec<MirNode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MirStatus {
    ExtractionOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirNode {
    pub id: String,
    pub parent: Option<String>,
    pub function: Option<String>,
    pub kind: SemanticKind,
    pub source: String,
    pub start: u64,
    pub end: u64,
    pub source_digest: String,
    pub verification_status: String,
    pub transformation_history: Vec<String>,
    pub source_kind: String,
    pub inferred_type: Option<String>,
    #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SemanticKind {
    Function,
    Branch,
    Iteration,
    Suspension,
    Throw,
    Return,
    Invocation,
    Mutation,
    Binding,
}

impl MirReport {
    pub fn validate_version(&self) -> Result<(), String> {
        if self.version != MIR_VERSION {
            return Err(format!(
                "unsupported Manuscrito MIR version: {} (expected {})",
                self.version, MIR_VERSION
            ));
        }
        for node in &self.nodes {
            if node.id.is_empty() {
                return Err("MIR node is missing an id".to_owned());
            }
            if node.source.is_empty() {
                return Err(format!("MIR node {} is missing a source module", node.id));
            }
            if !is_sha256_digest(&node.source_digest) {
                return Err(format!(
                    "MIR node {} has a malformed source digest: {}",
                    node.id, node.source_digest
                ));
            }
            if node.verification_status.is_empty() {
                return Err(format!(
                    "MIR node {} is missing a verification status",
                    node.id
                ));
            }
            if node.start > node.end {
                return Err(format!("invalid source range on MIR node {}", node.id));
            }
        }
        Ok(())
    }
}

/// Digests are emitted by the trusted frontend as `sha256:<64 lowercase hex>`.
fn is_sha256_digest(value: &str) -> bool {
    let digest = match value.strip_prefix("sha256:") {
        Some(digest) => digest,
        None => return false,
    };
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}
