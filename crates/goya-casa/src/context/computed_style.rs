//! Per-element computed style resolution (directive §7.5).
//!
//! For each element, top-down: cascade winners → custom property
//! substitution → inheritance and defaulting. The result is a flat
//! property map per element carrying the `Computed` category.

use crate::cascade::conditional::Environment;
use crate::cascade::custom_properties::{substitute, CustomProperties, Substitution};
use crate::cascade::engine::{cascade_element, CascadeEngine};
use crate::context::structural::{NodeId, SceneTree};
use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::CasaIr;
use std::collections::BTreeMap;

/// One element's computed style: property → computed value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComputedStyle {
    pub node: NodeId,
    pub properties: BTreeMap<String, String>,
}

impl ComputedStyle {
    pub fn get(&self, property: &str) -> Option<&str> {
        self.properties.get(property).map(|value| value.as_str())
    }
}

/// Computed styles for a whole tree, keyed by node id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComputedStyles {
    pub per_node: BTreeMap<NodeId, ComputedStyle>,
}

impl ComputedStyles {
    pub fn get(&self, node: NodeId) -> Option<&ComputedStyle> {
        self.per_node.get(&node)
    }
}

/// Compute styles for every element in the tree. Custom properties are
/// resolved parent-first so inheritance composes; cycles cut by the
/// substitution stage surface as diagnostics.
pub fn compute_styles(
    ir: &CasaIr,
    tree: &SceneTree,
    environment: &Environment,
    reporter: &mut Reporter,
) -> ComputedStyles {
    let mut engine = CascadeEngine::new(ir);
    let mut styles = ComputedStyles::default();
    for node in tree.walk() {
        let parent = tree.parent(node);
        let parent_style = parent.and_then(|parent| styles.per_node.get(&parent));
        let style = compute_one(
            &mut engine,
            ir,
            tree,
            node,
            parent_style,
            environment,
            reporter,
        );
        styles.per_node.insert(node, style);
    }
    styles
}

/// Compute one element's style from the cascade plus its parent.
pub fn compute_one(
    engine: &mut CascadeEngine,
    ir: &CasaIr,
    tree: &SceneTree,
    node: NodeId,
    parent: Option<&ComputedStyle>,
    environment: &Environment,
    reporter: &mut Reporter,
) -> ComputedStyle {
    let element = crate::context::selector_matching::ElementRef::new(tree, node);
    let winners = cascade_element(engine, ir, &element, environment, reporter);
    let mut style = ComputedStyle {
        node,
        properties: BTreeMap::new(),
    };

    // 1. Custom properties. Same-element declarations shadow inherited
    //    ones and resolve together (declaration order is irrelevant), so
    //    the set is seeded with raw values first and each entry is then
    //    substituted transitively.
    let mut custom: CustomProperties = parent.map(inherited_custom_properties).unwrap_or_default();
    for (name, winner) in &winners {
        if name.starts_with("--") {
            custom.insert(name.clone(), winner.value.clone());
        }
    }
    let declared: Vec<(String, String)> = winners
        .iter()
        .filter(|(name, _)| name.starts_with("--"))
        .map(|(name, winner)| (name.clone(), winner.value.clone()))
        .collect();
    for (name, raw) in &declared {
        match substitute(raw, &custom, reporter) {
            Substitution::Resolved(text) => {
                custom.insert(name.clone(), text);
            }
            Substitution::Cycle { .. } | Substitution::Undefined { .. } => {
                // Invalid at computed-value time: the property keeps its
                // inherited value, i.e. the raw entry is dropped.
                custom.remove(name);
            }
        }
    }

    // Resolved custom properties stay on the computed style so
    // descendants inherit them through `inherited_custom_properties`.
    for (name, value) in &custom {
        style.properties.insert(name.clone(), value.clone());
    }

    // 2. Regular properties: substitute, then inherit/default.
    for (name, winner) in &winners {
        if name.starts_with("--") {
            continue;
        }
        let substituted = match substitute(&winner.value, &custom, reporter) {
            Substitution::Resolved(text) => Some(text),
            Substitution::Cycle { .. } | Substitution::Undefined { .. } => None,
        };
        if let Some(text) = substituted {
            style.properties.insert(name.clone(), text);
        }
    }
    resolve_inheritance(&mut style.properties, parent);

    // 3. Done: substituted cascade winners plus inherited/defaulted fill.
    style
}

/// Custom properties visible from a parent style (resolved values).
fn inherited_custom_properties(parent: &ComputedStyle) -> CustomProperties {
    parent
        .properties
        .iter()
        .filter(|(name, _)| name.starts_with("--"))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

/// Fill unset properties: inherited properties take the parent value;
/// known properties take their initial value.
pub fn resolve_inheritance(
    properties: &mut BTreeMap<String, String>,
    parent: Option<&ComputedStyle>,
) {
    for property in crate::cascade::inheritance::INHERITED {
        if properties.contains_key(*property) {
            continue;
        }
        if let Some(value) = parent.and_then(|parent| parent.properties.get(*property)) {
            properties.insert(property.to_string(), value.clone());
        }
    }
    for (property, initial) in crate::cascade::inheritance::INITIALS {
        properties
            .entry(property.to_string())
            .or_insert_with(|| initial.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::parser::ingest_to_ir;

    fn styles(css: &str, html: &str) -> (SceneTree, ComputedStyles, Reporter) {
        let ir = ingest_to_ir("app.css", css);
        let (tree, _) = SceneTree::from_html(html);
        let mut reporter = Reporter::new();
        let styles = compute_styles(&ir, &tree, &Environment::default(), &mut reporter);
        (tree, styles, reporter)
    }

    #[test]
    fn inheritance_composes_top_down() {
        let (tree, styles, reporter) = styles(
            "body { color: navy; margin: 4px } p { margin: 0 }",
            "<html><body><p>text</p></body></html>",
        );
        assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
        let p = tree
            .walk()
            .into_iter()
            .find(|n| tree.get(*n).name == "p")
            .unwrap();
        let style = styles.get(p).unwrap();
        // Inherited from body.
        assert_eq!(style.get("color"), Some("navy"));
        // Declared on p itself.
        assert_eq!(style.get("margin"), Some("0"));
    }

    #[test]
    fn initial_values_fill_known_uninherited_properties() {
        let (tree, styles, _) = styles(
            "body { color: navy }",
            "<html><body><p>text</p></body></html>",
        );
        let p = tree
            .walk()
            .into_iter()
            .find(|n| tree.get(*n).name == "p")
            .unwrap();
        let style = styles.get(p).unwrap();
        assert_eq!(style.get("color"), Some("navy"));
        assert_eq!(style.get("margin"), Some("0"));
        assert_eq!(style.get("position"), Some("static"));
    }

    #[test]
    fn custom_properties_resolve_and_inherit() {
        let (tree, styles, reporter) = styles(
            "body { --accent: teal; --pad: calc(2px + var(--unit)); --unit: 3px } \
             p { color: var(--accent); padding: var(--pad) }",
            "<html><body><p>text</p></body></html>",
        );
        assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
        let p = tree
            .walk()
            .into_iter()
            .find(|n| tree.get(*n).name == "p")
            .unwrap();
        let style = styles.get(p).unwrap();
        assert_eq!(style.get("color"), Some("teal"));
        assert_eq!(style.get("padding"), Some("calc(2px + 3px)"));
    }

    #[test]
    fn fallback_applies_for_undefined_properties() {
        let (tree, styles, reporter) = styles(
            "p { margin: var(--nothing, 7px) }",
            "<html><body><p>text</p></body></html>",
        );
        assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
        let p = tree
            .walk()
            .into_iter()
            .find(|n| tree.get(*n).name == "p")
            .unwrap();
        assert_eq!(styles.get(p).unwrap().get("margin"), Some("7px"));
    }

    #[test]
    fn specificity_and_order_decide_winners() {
        let (tree, styles, reporter) = styles(
            "p { color: red } .lead { color: green } #intro { color: blue } \
             .lead { color: gold }",
            r#"<html><body><p id="intro" class="lead">hi</p></body></html>"#,
        );
        assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
        let p = tree
            .walk()
            .into_iter()
            .find(|n| tree.get(*n).name == "p")
            .unwrap();
        // ID (1,0,0) beats class (0,1,0) beats type (0,0,1); equal
        // specificities fall to source order (.lead gold later).
        assert_eq!(styles.get(p).unwrap().get("color"), Some("blue"));
    }

    #[test]
    fn important_beats_specificity_and_later_order() {
        let (tree, styles, reporter) = styles(
            "#intro { color: red } p { color: green !important }",
            r#"<html><body><p id="intro">hi</p></body></html>"#,
        );
        assert!(reporter.is_empty(), "{:?}", reporter.diagnostics());
        let p = tree
            .walk()
            .into_iter()
            .find(|n| tree.get(*n).name == "p")
            .unwrap();
        assert_eq!(styles.get(p).unwrap().get("color"), Some("green"));
    }
}
