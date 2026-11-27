//! Per-node CSS provenance (directive §9).
//!
//! For every element, records which cascade winner produced each
//! property: the matching selector, its specificity, importance and
//! sheet/rule order. This is the audit trail from a computed style back
//! to the authored rules; it re-runs the cascade per element so the
//! provenance always matches what [`crate::context::computed_style`]
//! computed.

use crate::cascade::conditional::Environment;
use crate::cascade::engine::{cascade_element, CascadeEngine};
use crate::context::computed_style::ComputedStyle;
use crate::context::selector_matching::ElementRef;
use crate::context::structural::{NodeId, SceneTree};
use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::CasaIr;
use std::collections::BTreeMap;

/// The cascade winner behind one property on one element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyProvenance {
    pub property: String,
    /// The selector text that matched.
    pub selector: String,
    pub important: bool,
    pub specificity: u32,
    /// Sheet/rule order key (sheet-major, rule-minor).
    pub order: usize,
}

/// Provenance for one element: property → winner, ordered by property
/// name for determinism.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeProvenance {
    pub node: NodeId,
    pub properties: BTreeMap<String, PropertyProvenance>,
}

/// Provenance for a whole tree, keyed by node id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Provenance {
    pub per_node: BTreeMap<NodeId, NodeProvenance>,
}

impl Provenance {
    /// The recorded provenance for a computed property, if any.
    pub fn get(&self, node: NodeId, property: &str) -> Option<&PropertyProvenance> {
        self.per_node.get(&node)?.properties.get(property)
    }
}

/// Collect provenance for every element in the tree. Diagnostics from
/// the re-run cascade are discarded: the caller's cascade pass already
/// reported them, and provenance collection must not duplicate reports.
pub fn collect(ir: &CasaIr, tree: &SceneTree, environment: &Environment) -> Provenance {
    let mut engine = CascadeEngine::new(ir);
    let mut reporter = Reporter::new();
    let mut per_node = BTreeMap::new();
    for node in tree.walk() {
        let element = ElementRef::new(tree, node);
        let winners = cascade_element(&mut engine, ir, &element, environment, &mut reporter);
        let mut properties = BTreeMap::new();
        for (name, winner) in winners {
            properties.insert(
                name.clone(),
                PropertyProvenance {
                    property: name,
                    selector: winner.selector,
                    important: winner.important,
                    specificity: winner.specificity,
                    order: winner.order,
                },
            );
        }
        per_node.insert(node, NodeProvenance { node, properties });
    }
    Provenance { per_node }
}

/// The provenance entries for one computed style, in property order.
pub fn for_style<'a>(
    provenance: &'a Provenance,
    style: &ComputedStyle,
) -> Vec<&'a PropertyProvenance> {
    provenance
        .per_node
        .get(&style.node)
        .map(|node| node.properties.values().collect())
        .unwrap_or_default()
}
