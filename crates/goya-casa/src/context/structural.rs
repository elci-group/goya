//! Structural context: the scene/DOM tree selector matching runs against
//! (directive §7.5.1), built from HTML via `goya-html-tokenizer`, and the
//! binding of FAIR decomposition elements onto scene elements.

use goya_html_tokenizer::{tokenize_document, Token};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// An index into [`SceneTree::nodes`]. Stable for the tree's lifetime.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct NodeId(pub u32);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeNode {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// Tag name, lowercased, e.g. `div`.
    pub name: String,
    /// The `id` attribute, when present.
    pub id: Option<String>,
    /// The `class` attribute split on whitespace, sorted.
    pub classes: Vec<String>,
    /// Other attributes by (lowercased) name.
    pub attributes: BTreeMap<String, String>,
    /// Interaction/user-action state (hover, focus, ...), set by the host
    /// application before matching.
    pub state: Vec<String>,
}

impl TreeNode {
    fn new(name: String) -> Self {
        TreeNode {
            parent: None,
            children: Vec::new(),
            name,
            id: None,
            classes: Vec::new(),
            attributes: BTreeMap::new(),
            state: Vec::new(),
        }
    }

    /// Absorb `id`/`class` into their dedicated fields from an attribute map.
    fn set_attribute(&mut self, name: &str, value: String) {
        match name {
            "id" => self.id = Some(value),
            "class" => {
                self.classes = value
                    .split_ascii_whitespace()
                    .map(|c| c.to_string())
                    .collect();
                self.classes.sort();
            }
            _ => {
                self.attributes.insert(name.to_string(), value);
            }
        }
    }
}

/// A minimal element tree: enough structure for selector matching and
/// style binding, with deterministic node ordering.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneTree {
    pub nodes: Vec<TreeNode>,
}

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

impl SceneTree {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_root(name: &str) -> Self {
        let mut tree = SceneTree { nodes: Vec::new() };
        let root = TreeNode::new(name.to_ascii_lowercase());
        tree.nodes.push(root);
        tree
    }

    /// Append an element; returns its node id.
    pub fn append(&mut self, parent: NodeId, name: &str) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        let mut node = TreeNode::new(name.to_ascii_lowercase());
        node.parent = Some(parent);
        self.nodes.push(node);
        self.nodes[parent.0 as usize].children.push(id);
        id
    }

    /// Add an attribute to a just-created node (builder aid).
    pub fn attribute(&mut self, node: NodeId, name: &str, value: &str) {
        self.nodes[node.0 as usize].set_attribute(&name.to_ascii_lowercase(), value.to_string());
    }

    /// Mark a user-action state (e.g. `hover`, `focus`).
    pub fn set_state(&mut self, node: NodeId, state: &str) {
        let states = &mut self.nodes[node.0 as usize].state;
        if !states.contains(&state.to_string()) {
            states.push(state.to_string());
        }
    }

    pub fn get(&self, node: NodeId) -> &TreeNode {
        &self.nodes[node.0 as usize]
    }

    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.get(node).parent
    }

    /// Pre-order walk of the whole tree.
    pub fn walk(&self) -> Vec<NodeId> {
        match self.nodes.is_empty() {
            true => Vec::new(),
            false => self.walk_from(NodeId(0)),
        }
    }

    /// Pre-order walk of a subtree.
    pub fn walk_from(&self, start: NodeId) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            order.push(node);
            for child in self.get(node).children.iter().rev() {
                stack.push(*child);
            }
        }
        order
    }

    /// Build a tree from an HTML fragment or document. Tolerant: unclosed
    /// elements are closed at the end of input, stray end tags close the
    /// nearest matching open element, void elements self-close.
    pub fn from_html(input: &str) -> (Self, Vec<goya_html_tokenizer::ParseError>) {
        let (tokens, errors) = tokenize_document(input);
        let mut tree = SceneTree::empty();
        // Node 0 is the document root placeholder, never matched by type
        // selectors (its name is not a tag).
        tree.nodes.push(TreeNode::new(String::new()));
        let mut open: Vec<NodeId> = vec![NodeId(0)];
        for spanned in tokens {
            match spanned.token {
                Token::Doctype { .. } | Token::Comment(_) | Token::ProcessingInstruction { .. } => {
                }
                Token::Text(_) => {}
                Token::StartTag {
                    name,
                    attributes,
                    self_closing,
                } => {
                    let parent = *open.last().unwrap();
                    let node = tree.append(parent, &name);
                    for attribute in &attributes {
                        tree.attribute(node, &attribute.name, &attribute.value);
                    }
                    if self_closing || VOID_ELEMENTS.contains(&name.as_str()) {
                        // Already closed: do not push onto the open stack.
                    } else {
                        open.push(node);
                    }
                }
                Token::EndTag { name, .. } => {
                    let name = name.to_ascii_lowercase();
                    if let Some(position) =
                        open.iter().rposition(|node| tree.get(*node).name == name)
                    {
                        if position > 0 {
                            open.truncate(position);
                        }
                    }
                    // Stray end tags are ignored.
                }
            }
        }
        (tree, errors)
    }
}

/// A binding between a FAIR decomposition element and a scene element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairBinding {
    pub fair_element: String,
    pub node: NodeId,
}

/// Bind FAIR elements onto the scene tree: an element carries
/// `data-fair-id="<extractor-assigned id>"`. Unbound FAIR components are
/// reported so the upstream model is never silently lost (directive §17.1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FairBindings {
    pub bindings: Vec<FairBinding>,
    /// FAIR element ids referenced by no scene element.
    pub unbound: Vec<String>,
}

pub fn bind_fair(document: &goya_fair::FairDocument, tree: &SceneTree) -> FairBindings {
    let fair_ids: BTreeSet<&str> = document
        .elements
        .iter()
        .map(|element| element.id.as_str())
        .collect();
    let mut bindings = Vec::new();
    let mut bound = BTreeSet::new();
    for node in tree.walk() {
        if let Some(fair_id) = tree.get(node).attributes.get("data-fair-id") {
            if fair_ids.contains(fair_id.as_str()) {
                bound.insert(fair_id.clone());
                bindings.push(FairBinding {
                    fair_element: fair_id.clone(),
                    node,
                });
            }
        }
    }
    let unbound: Vec<String> = document
        .elements
        .iter()
        .filter(|element| {
            element.kind == goya_fair::ElementKind::Component && !bound.contains(&element.id)
        })
        .map(|element| element.id.clone())
        .collect();
    FairBindings { bindings, unbound }
}

use std::collections::BTreeSet;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tree_from_html_with_void_elements() {
        let (tree, errors) = SceneTree::from_html(
            "<div id=\"a\" class=\"card wide\"><p>Hello<br><span>x</span></p></div>",
        );
        assert!(errors.is_empty());
        let walk = tree.walk();
        // document placeholder, div, p, br, span
        assert_eq!(walk.len(), 5);
        let div = walk[1];
        assert_eq!(tree.get(div).name, "div");
        assert_eq!(tree.get(div).id.as_deref(), Some("a"));
        assert_eq!(tree.get(div).classes, ["card", "wide"]);
        assert_eq!(tree.parent(walk[2]), Some(div));
        // <br> is void: span is a sibling of br, not a child.
        assert_eq!(tree.get(walk[4]).name, "span");
        assert_eq!(tree.parent(walk[4]), Some(walk[2]));
    }

    #[test]
    fn stray_end_tags_are_tolerated() {
        let (tree, _) = SceneTree::from_html("<div><p>x</div></p>");
        assert_eq!(tree.walk().len(), 3);
    }

    #[test]
    fn state_is_recorded_once() {
        let mut tree = SceneTree::with_root("div");
        let node = NodeId(0);
        tree.set_state(node, "hover");
        tree.set_state(node, "hover");
        assert_eq!(tree.get(node).state, ["hover"]);
    }

    #[test]
    fn fair_components_bind_by_attribute() {
        let document: goya_fair::FairDocument = serde_json::from_str(
            r#"{
                "contract": "GY-FAIR/2.0",
                "source": { "root": "app", "files": [
                    { "path": "app.js", "sha256": "00" }
                ] },
                "extractor": { "name": "test", "version": "1", "id_rule": "line" },
                "elements": [
                    { "id": "e1", "kind": "component",
                      "provenance": { "file": "app.js", "line": 1,
                        "span_start": null, "span_end": null,
                        "fallback_origin": false },
                      "representability": { "status": "extracted" },
                      "semantics": {} },
                    { "id": "e2", "kind": "component",
                      "provenance": { "file": "app.js", "line": 2,
                        "span_start": null, "span_end": null,
                        "fallback_origin": false },
                      "representability": { "status": "extracted" },
                      "semantics": {} }
                ]
            }"#,
        )
        .unwrap();
        let (tree, _) = SceneTree::from_html(
            "<div data-fair-id=\"e1\"><button data-fair-id=\"e1\">go</button></div>",
        );
        let bindings = bind_fair(&document, &tree);
        // Both bindings point at distinct nodes; e2 is referenced nowhere.
        assert_eq!(bindings.bindings.len(), 2);
        assert_eq!(bindings.unbound, ["e2"]);
    }
}
