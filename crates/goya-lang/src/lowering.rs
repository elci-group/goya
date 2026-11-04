//! GY-IR/2 — Lowering of a [`Scene`] into a [`goya_ir::Module`].
//!
//! Unification audit item: both the goya-ir model and the goya-lang scene
//! model existed without a shared path. This module lowers a parsed scene
//! into the IR so both frontends produce one representation:
//!
//! - `@button` cells → [`goya_ir::Node::Button`] with sequential [`goya_ir::ActionId`]s.
//! - text-bearing `@cell`  → a [`goya_ir::Node::Column`] whose first child is
//!   a [`goya_ir::Node::Text`], followed by the lowered children.
//! - plain `@cell` (no text) → [`goya_ir::Node::Column`] containing its children
//!   (a childless plain cell is an empty `Column`, the IR's stand-in for a
//!   positioned surface — the resolved rect lives in the [`NodeTable`], since
//!   the IR vocabulary {Column, Row, Text, Button, Spacer} has no absolute
//!   geometry).
//!
//! Contract: lowering is total (never panics on any scene), deterministic
//! (lowering twice yields an equal [`Lowered`]), and never loses a scene
//! name — duplicate cell names are disambiguated with deterministic
//! numeric suffixes (`"name"`, `"name.2"`, `"name.3"`, ...).

use std::collections::{BTreeMap, HashSet};

use goya_ir::{ActionId, Module, Node, NodeId};

use crate::{layout, CellKind, RectMp, Scene, SceneCell};

/// The stable semantic name of a lowered node: the scene cell name, made
/// unique with deterministic `.N` suffixes when duplicated.
pub type NodeKey = String;

/// The unified node table (GY-IR/2): scene names → IR nodes → geometry.
pub type NodeTable = BTreeMap<NodeKey, NodeRecord>;

/// Role a scene cell played in the lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Plain `@cell` → `Node::Column`.
    Cell,
    /// `@button` → `Node::Button`.
    Button,
    /// Synthetic `Node::Text` emitted as the first child of a text-bearing cell.
    Text,
}

/// One row of the unified node table.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRecord {
    /// The IR node this scene cell became.
    pub ir_node: NodeId,
    pub role: Role,
    /// Owning cell's key, if any (synthetic text rows point at their cell).
    pub parent: Option<NodeKey>,
    /// The resolved rect in millipoints, present only when the cell declared
    /// both a position and an extent (absolute or proportional).
    pub explicit_rect_mp: Option<(i64, i64, i64, i64)>,
    /// Keys of the lowered children, in scene order.
    pub children: Vec<NodeKey>,
}

/// Output of [`lower`]: the shared IR module plus the name→node→rect table.
#[derive(Debug, Clone, PartialEq)]
pub struct Lowered {
    pub module: Module,
    pub table: NodeTable,
}

impl Lowered {
    /// Resolved rect in millipoints for a scene name, if it was explicit.
    pub fn rect_of(&self, key: &str) -> Option<(i64, i64, i64, i64)> {
        self.table.get(key)?.explicit_rect_mp
    }
}

/// Non-panicking lowering failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowerError {
    /// The scene has no usable root cell.
    MissingRoot,
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::MissingRoot => f.write_str("scene has no root cell"),
        }
    }
}

impl std::error::Error for LowerError {}

/// Lower a scene into the unified IR. Runs the deterministic layout engine
/// to resolve geometry, then builds the module and node table. Total and
/// deterministic.
pub fn lower(scene: &Scene, viewport_w_pt: f64, viewport_h_pt: f64) -> Result<Lowered, LowerError> {
    if scene.root.name.is_empty() {
        return Err(LowerError::MissingRoot);
    }
    let frame = layout(scene, viewport_w_pt, viewport_h_pt);
    // `layout` emits nodes in DFS pre-order (parent, then children), which is
    // exactly the traversal order used below — so pairing by index gives each
    // cell its resolved rect even when names repeat.
    let mut lowerer = Lowerer {
        module: Module::default(),
        table: NodeTable::default(),
        used_keys: HashSet::new(),
        frame_nodes: &frame.nodes,
        next_frame: 0,
        next_action: 0,
    };
    let (_root_key, root_id) = lowerer.lower_cell(&scene.root, None);
    lowerer.module.root = Some(root_id);
    debug_assert_eq!(lowerer.next_frame, frame.nodes.len());
    Ok(Lowered {
        module: lowerer.module,
        table: lowerer.table,
    })
}

struct Lowerer<'a> {
    module: Module,
    table: NodeTable,
    used_keys: HashSet<String>,
    frame_nodes: &'a [crate::DrawNode],
    next_frame: usize,
    next_action: u64,
}

impl<'a> Lowerer<'a> {
    fn push(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.module.nodes.len() as u64);
        self.module.nodes.push((id, node));
        id
    }

    /// Allocate the next unique key for a raw scene name.
    fn unique_key(&mut self, name: &str) -> NodeKey {
        if self.used_keys.insert(name.to_string()) {
            return name.to_string();
        }
        let mut n = 2u64;
        loop {
            let key = format!("{name}.{n}");
            if self.used_keys.insert(key.clone()) {
                return key;
            }
            n += 1;
        }
    }

    fn resolved_rect(&mut self) -> RectMp {
        let rect = match self.frame_nodes.get(self.next_frame) {
            Some(node) => node.rect,
            None => RectMp {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
        };
        self.next_frame += 1;
        rect
    }

    /// Lower one cell; returns its allocated key and IR node id.
    fn lower_cell(&mut self, cell: &SceneCell, parent_key: Option<&str>) -> (NodeKey, NodeId) {
        let key = self.unique_key(&cell.name);
        let rect = self.resolved_rect();
        let has_explicit_rect =
            cell.position.or(cell.position_p).is_some() && cell.extent.or(cell.extent_p).is_some();
        let children: Vec<NodeKey>;

        let (id, role) = match cell.kind {
            CellKind::Button => {
                let id = self.push(Node::Button {
                    label: cell.text.clone().unwrap_or_default(),
                    on_activate: ActionId(self.next_action),
                });
                self.next_action += 1;
                children = Vec::new();
                (id, Role::Button)
            }
            CellKind::Cell => {
                let id;
                let role;
                if let Some(text) = &cell.text {
                    // Text-bearing cell: synthetic Text first child, then children.
                    let text_key = self.unique_key(&format!("{key}.text"));
                    let text_id = self.push(Node::Text {
                        content: text.clone(),
                    });
                    self.table.insert(
                        text_key.clone(),
                        NodeRecord {
                            ir_node: text_id,
                            role: Role::Text,
                            parent: Some(key.clone()),
                            explicit_rect_mp: explicit(rect, has_explicit_rect),
                            children: Vec::new(),
                        },
                    );
                    let mut kid_keys = vec![text_key];
                    let mut kid_ids = vec![text_id];
                    for child in &cell.children {
                        let (k, i) = self.lower_cell(child, Some(&key));
                        kid_keys.push(k);
                        kid_ids.push(i);
                    }
                    id = self.push(Node::Column { children: kid_ids });
                    children = kid_keys;
                    role = Role::Cell;
                } else {
                    let mut kid_keys = Vec::with_capacity(cell.children.len());
                    let mut kid_ids = Vec::with_capacity(cell.children.len());
                    for child in &cell.children {
                        let (k, i) = self.lower_cell(child, Some(&key));
                        kid_keys.push(k);
                        kid_ids.push(i);
                    }
                    id = self.push(Node::Column { children: kid_ids });
                    children = kid_keys;
                    role = Role::Cell;
                }
                (id, role)
            }
        };

        self.table.insert(
            key.clone(),
            NodeRecord {
                ir_node: id,
                role,
                parent: parent_key.map(str::to_string),
                explicit_rect_mp: explicit(rect, has_explicit_rect),
                children,
            },
        );
        (key, id)
    }
}

fn explicit(rect: RectMp, has: bool) -> Option<(i64, i64, i64, i64)> {
    if has {
        Some((rect.x, rect.y, rect.width, rect.height))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const HELLO: &str = "\
@ontology hello {
    @cell root {
        position: (0, 0, 0)
        extent: (800, 600, 0)

        @cell title {
            position: (400, 60, 0)
            extent: (200, 40, 0)
            text: \"Hello, Goya!\"
        }

        @button go {
            position: (300, 300, 0)
            extent: (200, 60, 0)
            text: \"Go\"
        }
    }
}
";

    #[test]
    fn hello_lowers_with_sequential_action_ids() {
        let scene = parse(HELLO).unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        let mut actions = Vec::new();
        for (id, node) in &lowered.module.nodes {
            if let Node::Button { on_activate, .. } = node {
                actions.push((*id, *on_activate));
            }
        }
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].1, ActionId(0));
        assert_eq!(lowered.module.root, Some(lowered.table["root"].ir_node));
    }

    #[test]
    fn hello_button_and_text_shape() {
        let scene = parse(HELLO).unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        let root = lowered.table.get("root").unwrap();
        assert_eq!(root.role, Role::Cell);
        assert_eq!(root.children, vec!["title".to_string(), "go".to_string()]);
        // Root's IR node is a Column whose children are title's column and go.
        let root_node = lowered.module.node(root.ir_node).unwrap();
        let Node::Column { children } = root_node else {
            panic!("root must lower to a Column");
        };
        assert_eq!(children.len(), 2);
        // `go` is a Button with label "Go".
        let go = lowered.table.get("go").unwrap();
        assert_eq!(go.role, Role::Button);
        let Node::Button { label, on_activate } = lowered.module.node(go.ir_node).unwrap() else {
            panic!("go must lower to a Button");
        };
        assert_eq!(label, "Go");
        assert_eq!(*on_activate, ActionId(0));
        // `title` is a Column whose first child is the synthetic Text.
        let title = lowered.table.get("title").unwrap();
        let Node::Column { children } = lowered.module.node(title.ir_node).unwrap() else {
            panic!("title must lower to a Column");
        };
        assert_eq!(children.len(), 1);
        let Node::Text { content } = lowered.module.node(children[0]).unwrap() else {
            panic!("title's first child must be Text");
        };
        assert_eq!(content, "Hello, Goya!");
    }

    #[test]
    fn node_table_round_trips_every_scene_name() {
        let scene = parse(HELLO).unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        // Walk the scene DFS; every cell name must be findable in the table
        // (uniquely, since this scene has no duplicates) and its ir_node must
        // resolve in the module.
        fn walk(cell: &SceneCell, lowered: &Lowered) {
            let rec = lowered
                .table
                .get(&cell.name)
                .unwrap_or_else(|| panic!("missing table entry for {}", cell.name));
            assert!(
                lowered.module.node(rec.ir_node).is_some(),
                "dangling NodeId for {}",
                cell.name
            );
            for child in &cell.children {
                walk(child, lowered);
            }
        }
        walk(&scene.root, &lowered);
    }

    #[test]
    fn duplicate_names_disambiguated_deterministically() {
        let src = "\
@ontology d {
    @cell dup {
        position: (0, 0, 0)
        extent: (100, 100, 0)
        @cell dup { position: (0, 0, 0); extent: (50, 50, 0); text: \"inner\" }
        @cell other { position: (0, 0, 0); extent: (10, 10, 0) }
        @cell dup { position: (0, 0, 0); extent: (20, 20, 0) }
    }
}
";
        let scene = parse(src).unwrap();
        let a = lower(&scene, 800.0, 600.0).unwrap();
        let b = lower(&scene, 800.0, 600.0).unwrap();
        assert_eq!(a, b, "duplicate disambiguation must be deterministic");
        assert!(a.table.contains_key("dup"), "first keeps the bare name");
        assert!(a.table.contains_key("dup.2"), "second gets .2");
        assert!(a.table.contains_key("dup.3"), "third gets .3");
        // Cells: dup (root), dup.2 (nested, text-bearing), other, dup.3,
        // plus dup.2's synthetic text row.
        assert_eq!(a.table.len(), 5);
        assert!(a.table.contains_key("dup.2.text"));
        assert_eq!(a.module.root, Some(a.table["dup"].ir_node));
    }

    #[test]
    fn lowering_is_deterministic() {
        let scene = parse(HELLO).unwrap();
        let a = lower(&scene, 800.0, 600.0).unwrap();
        let b = lower(&scene, 800.0, 600.0).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn explicit_rects_come_from_layout() {
        let scene = parse(HELLO).unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        let go = lowered.table.get("go").unwrap();
        // position (300, 300) pt, extent (200, 60) pt → mp.
        assert_eq!(
            go.explicit_rect_mp,
            Some((300_000, 300_000, 200_000, 60_000))
        );
        // A synthetic text row inherits its cell's explicit rect.
        let title_text = lowered.table.get("title.text").unwrap();
        assert_eq!(title_text.role, Role::Text);
        assert_eq!(title_text.parent.as_deref(), Some("title"));
        assert_eq!(
            title_text.explicit_rect_mp,
            Some((400_000, 60_000, 200_000, 40_000))
        );
        assert_eq!(
            lowered.rect_of("go"),
            Some((300_000, 300_000, 200_000, 60_000))
        );
    }

    #[test]
    fn missing_root_is_an_error_not_a_panic() {
        let mut scene = parse(HELLO).unwrap();
        scene.root.name = String::new();
        assert_eq!(lower(&scene, 800.0, 600.0), Err(LowerError::MissingRoot));
    }

    /// Fuzz-ish totality: empty cells, deep nesting, many siblings — the
    /// lowering never panics and always produces a consistent module.
    #[test]
    fn lower_is_total_on_stress_scenes() {
        // 50 siblings.
        let mut src =
            String::from("@ontology s {\n@cell root { position: (0,0,0) extent: (800,600,0)\n");
        for i in 0..50 {
            src.push_str(&format!(
                "@cell s{i} {{ position: (0,0,0); extent: (10,10,0) }}\n"
            ));
        }
        src.push_str("}\n}\n");
        let scene = parse(&src).unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        assert_eq!(lowered.module.nodes.len(), 51); // root Column + 50 sibling Columns
        for (id, _) in &lowered.module.nodes {
            assert!(lowered.module.node(*id).is_some());
        }

        // 60-deep nesting of empty cells (no position/extent/text at all).
        let mut deep = String::new();
        for _ in 0..60 {
            deep.push_str("@cell d { ");
        }
        deep.push_str("@cell leaf { text: \"x\" }");
        for _ in 0..60 {
            deep.push_str(" }");
        }
        let src = format!("@ontology deep {{ {deep} }}");
        let scene = parse(&src).unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        assert!(lowered.module.node(lowered.module.root.unwrap()).is_some());

        // Fully empty cells with no declared geometry still lower.
        let scene = parse("@ontology e { @cell root { @cell a { } @cell b { } } }").unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        assert_eq!(lowered.module.nodes.len(), 3);
        for rec in lowered.table.values() {
            assert_eq!(rec.explicit_rect_mp, None, "undeclared geometry stays None");
        }
    }
}
