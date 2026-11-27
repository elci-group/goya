//! Property-level dependency graphs (directive §7.3).
//!
//! Edges are discovered from `var(--x)` references inside declared
//! values: the declaring property depends on the referenced custom
//! property. The graph is deterministic (`BTreeMap`/`BTreeSet`), so
//! traversal results are stable across runs.

use crate::ir::casa_ir::{CasaIr, CasaRule, DeclaredProperty};
use goya_css_syntax::{tokenize, Token};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertyDependencyGraph {
    edges: BTreeMap<String, BTreeSet<String>>,
}

impl PropertyDependencyGraph {
    /// Build the graph for a whole IR, including group-rule nesting.
    pub fn build(ir: &CasaIr) -> Self {
        let mut graph = Self::default();
        for sheet in &ir.sheets {
            graph.add_rules(&sheet.rules);
        }
        graph
    }

    fn add_rules(&mut self, rules: &[CasaRule]) {
        for rule in rules {
            match rule {
                CasaRule::Style(style) => self.add_declarations(&style.declarations),
                CasaRule::Media(media) => self.add_rules(&media.rules),
                CasaRule::FontFace(font) => self.add_declarations(&font.declarations),
                CasaRule::Keyframes(keyframes) => {
                    for step in &keyframes.steps {
                        self.add_declarations(&step.declarations);
                    }
                }
                _ => {}
            }
        }
    }

    fn add_declarations(&mut self, declarations: &[DeclaredProperty]) {
        for declaration in declarations {
            let references = var_references(&declaration.value);
            if references.is_empty() && !declaration.name.starts_with("--") {
                continue;
            }
            for reference in references {
                self.add_edge(declaration.name.clone(), reference);
            }
            if declaration.name.starts_with("--") && !self.edges.contains_key(&declaration.name) {
                self.edges.insert(declaration.name.clone(), BTreeSet::new());
            }
        }
    }

    pub fn add_edge(&mut self, property: impl Into<String>, depends_on: impl Into<String>) {
        self.edges
            .entry(property.into())
            .or_default()
            .insert(depends_on.into());
    }

    pub fn depends_on(&self, property: &str) -> Option<&BTreeSet<String>> {
        self.edges.get(property)
    }

    pub fn properties(&self) -> impl Iterator<Item = &String> {
        self.edges.keys()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.values().map(BTreeSet::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// First dependency cycle in deterministic order, as a path ending
    /// where it starts (e.g. `["--a", "--b", "--a"]`).
    pub fn find_cycle(&self) -> Option<Vec<String>> {
        let mut state: BTreeMap<String, u8> = BTreeMap::new(); // 1 = open, 2 = done
        let mut stack: Vec<String> = Vec::new();
        for property in self.edges.keys() {
            if state.get(property).copied().unwrap_or(0) == 0 {
                if let Some(cycle) = dfs(&self.edges, property, &mut state, &mut stack) {
                    return Some(cycle);
                }
            }
        }
        None
    }

    /// A topological order with dependencies before their dependents, or
    /// `None` when the graph is cyclic.
    pub fn topological_order(&self) -> Option<Vec<String>> {
        let mut indegree: BTreeMap<&String, usize> = BTreeMap::new();
        for property in self.edges.keys() {
            indegree.entry(property).or_insert(0);
        }
        // Referenced properties are nodes even when never declared here;
        // resolution treats them as externally defined.
        for dependencies in self.edges.values() {
            for dependency in dependencies {
                indegree.entry(dependency).or_insert(0);
            }
        }
        let mut dependents: BTreeMap<&String, BTreeSet<&String>> = BTreeMap::new();
        for (property, dependencies) in &self.edges {
            for dependency in dependencies {
                *indegree.entry(property).or_insert(0) += 1;
                dependents.entry(dependency).or_default().insert(property);
            }
        }
        let mut ready: BTreeSet<&String> = indegree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(&property, _)| property)
            .collect();
        let mut order = Vec::with_capacity(indegree.len());
        while let Some(node) = ready.pop_first() {
            if let Some(dependents) = dependents.get(node) {
                for dependent in dependents {
                    let degree = indegree.get_mut(dependent).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(dependent);
                    }
                }
            }
            order.push(node.clone());
        }
        if order.len() == indegree.len() {
            Some(order)
        } else {
            None
        }
    }
}

fn dfs(
    edges: &BTreeMap<String, BTreeSet<String>>,
    node: &str,
    state: &mut BTreeMap<String, u8>,
    stack: &mut Vec<String>,
) -> Option<Vec<String>> {
    state.insert(node.to_string(), 1);
    stack.push(node.to_string());
    if let Some(dependencies) = edges.get(node) {
        for dependency in dependencies {
            match state.get(dependency).copied().unwrap_or(0) {
                1 => {
                    let start = stack.iter().position(|entry| entry == dependency).unwrap();
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(dependency.clone());
                    return Some(cycle);
                }
                2 => {}
                _ => {
                    if let Some(cycle) = dfs(edges, dependency, state, stack) {
                        return Some(cycle);
                    }
                }
            }
        }
    }
    stack.pop();
    state.insert(node.to_string(), 2);
    None
}

/// Custom property references in a value: the first argument of each
/// `var(...)` function whose name starts with `--`.
pub fn var_references(value: &str) -> Vec<String> {
    let mut references = Vec::new();
    let mut expecting_name = false;
    for (token, _) in tokenize(value) {
        match token {
            Token::Function(name) => expecting_name = name.eq_ignore_ascii_case("var"),
            Token::Whitespace => {}
            Token::Ident(name) if expecting_name && name.starts_with("--") => {
                references.push(name.clone());
                expecting_name = false;
            }
            _ => expecting_name = false,
        }
    }
    references
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::parser::ingest_to_ir;

    #[test]
    fn var_references_are_extracted() {
        assert_eq!(var_references("var(--x)"), ["--x"]);
        assert_eq!(var_references("var(--gap, 4px) 8px"), ["--gap"]);
        assert_eq!(var_references("var(--a) var(--b)"), ["--a", "--b"]);
        assert!(var_references("10px solid red").is_empty());
        // `varx(--x)` is not a var() reference.
        assert!(var_references("varx(--x)").is_empty());
    }

    #[test]
    fn graph_edges_follow_var_references() {
        let ir = ingest_to_ir(
            "app.css",
            ":root { --accent: blue } \
             body { --pad: var(--base-pad); color: var(--accent); margin: 0 }",
        );
        let graph = PropertyDependencyGraph::build(&ir);
        assert_eq!(
            graph.depends_on("color").unwrap(),
            &BTreeSet::from(["--accent".to_string()])
        );
        assert_eq!(
            graph.depends_on("--pad").unwrap(),
            &BTreeSet::from(["--base-pad".to_string()])
        );
        // --accent and --pad are nodes even without outgoing edges.
        assert!(graph.properties().any(|p| p == "--accent"));
        assert!(graph.properties().any(|p| p == "--pad"));
        assert!(graph.find_cycle().is_none());
        assert!(graph.topological_order().is_some());
    }

    #[test]
    fn cycles_are_detected_deterministically() {
        let ir = ingest_to_ir("app.css", ":root { --a: var(--b); --b: var(--a) }");
        let graph = PropertyDependencyGraph::build(&ir);
        let cycle = graph.find_cycle().expect("cycle exists");
        assert_eq!(cycle.first(), cycle.last());
        assert_eq!(cycle.len(), 3); // --a, --b, --a
        assert!(graph.topological_order().is_none());
    }

    #[test]
    fn media_nesting_is_walked() {
        let ir = ingest_to_ir("app.css", "@media screen { body { color: var(--c) } }");
        let graph = PropertyDependencyGraph::build(&ir);
        assert!(graph.depends_on("color").unwrap().contains("--c"));
    }
}
