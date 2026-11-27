//! Resource discovery and deduplication (directive §15).
//!
//! Phase 1 discovers and registers references; content addressing and
//! ingestion into the scene resource pool land with the resource stage.

use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::{ResourceKind, ResourceRef};
use std::collections::BTreeSet;

/// Deduplicating resource registry: first occurrence of a URL wins.
#[derive(Debug, Default)]
pub struct ResourceRegistry {
    entries: Vec<ResourceRef>,
    seen: BTreeSet<String>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a registry from a collected reference list, reporting
    /// duplicates.
    pub fn from_refs(references: Vec<ResourceRef>, reporter: &mut Reporter) -> Self {
        let mut registry = Self::new();
        for reference in references {
            registry.register(reference, reporter);
        }
        registry
    }

    /// Register a reference; returns false when the URL was already
    /// registered (and emits `CASA-RES-002`).
    pub fn register(&mut self, reference: ResourceRef, reporter: &mut Reporter) -> bool {
        if !self.seen.insert(reference.url.clone()) {
            reporter.emit(
                codes::RESOURCE_DUPLICATE,
                format!("{} referenced again; first occurrence kept", reference.url),
                None,
            );
            return false;
        }
        self.entries.push(reference);
        true
    }

    pub fn entries(&self) -> &[ResourceRef] {
        &self.entries
    }

    pub fn of_kind(&self, kind: ResourceKind) -> Vec<&ResourceRef> {
        self.entries.iter().filter(|r| r.kind == kind).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Deduplicate a collected reference list, keeping first-reference order.
pub fn dedup(references: Vec<ResourceRef>, reporter: &mut Reporter) -> Vec<ResourceRef> {
    ResourceRegistry::from_refs(references, reporter).entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(url: &str, kind: ResourceKind) -> ResourceRef {
        ResourceRef {
            kind,
            url: url.to_string(),
            start: 0,
            end: 0,
        }
    }

    #[test]
    fn duplicates_keep_first_occurrence_and_report() {
        let mut reporter = Reporter::new();
        let mut registry = ResourceRegistry::new();
        assert!(registry.register(reference("a.png", ResourceKind::Image), &mut reporter));
        assert!(!registry.register(reference("a.png", ResourceKind::Image), &mut reporter));
        assert_eq!(registry.len(), 1);
        assert_eq!(reporter.diagnostics()[0].code, "CASA-RES-002");
    }

    #[test]
    fn kind_filters() {
        let mut reporter = Reporter::new();
        let registry = ResourceRegistry::from_refs(
            vec![
                reference("f.woff2", ResourceKind::Font),
                reference("a.png", ResourceKind::Image),
                reference("a.png", ResourceKind::Image),
            ],
            &mut reporter,
        );
        assert_eq!(registry.len(), 2);
        assert_eq!(registry.of_kind(ResourceKind::Font).len(), 1);
        assert_eq!(registry.of_kind(ResourceKind::Image).len(), 1);
    }
}
