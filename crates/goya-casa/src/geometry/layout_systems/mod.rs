//! Layout systems (directive §9, progressive scope).
//!
//! The driver in this module walks the scene tree top-down and hands each
//! element's in-flow children to the layout system named by its `display`
//! property. Systems are pure positioning algorithms: they receive
//! measured child boxes and the parent's content box, and assign each
//! child a border-box [`Rect`] in content-local coordinates. The driver
//! translates those into parent-relative boxes, records them, and
//! recurses.
//!
//! # Determinism and simplifications
//!
//! All algorithms are deterministic (ordered maps, stable iteration).
//! The progressive scope does not implement margin collapsing, intrinsic
//! text measurement (a childless auto-height element resolves to zero
//! height), or line-level inline flow; each system documents its own
//! simplifications next to the affected step.

pub mod absolute;
pub mod block;
pub mod fixed;
pub mod flexbox;
pub mod float;
pub mod grid;
pub mod multicol;
pub mod sticky;
pub mod table;

use crate::context::computed_style::{ComputedStyle, ComputedStyles};
use crate::context::structural::{NodeId, SceneTree};
use crate::diagnostics::codes::{self, DiagnosticCode};
use crate::diagnostics::reporter::Reporter;
use crate::geometry::{Context, Edges, Px, Rect};
use goya_css_syntax::{parse_component_values, ComponentValue, Token};
use std::collections::BTreeMap;

/// The `display` values the layout stage distinguishes; everything else
/// flows as a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Flex,
    Grid,
    Table,
    TableRow,
    TableCell,
    None,
}

/// The `position` values; `Relative` and `Sticky` participate in flow,
/// the rest are removed from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// Resolved per-element box furniture: padding, border and margin edges.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxEdges {
    pub padding: Edges,
    pub border: Edges,
    pub margin: Edges,
}

/// Per-node layout inputs, resolved once per visit.
#[derive(Debug, Clone, Copy)]
pub struct NodeBox {
    /// Authored content (content-box) or border-box size per
    /// `box-sizing`; `None` means the axis is auto.
    pub width: Option<Px>,
    pub height: Option<Px>,
    pub edges: BoxEdges,
    pub border_box_sizing: bool,
    /// Auto margins per edge (top, right, bottom, left).
    pub margin_auto: [bool; 4],
    pub position: Position,
    /// `top right bottom left` offsets for positioned elements.
    pub offsets: [Option<Px>; 4],
    pub display: Display,
}

impl Default for NodeBox {
    fn default() -> Self {
        NodeBox {
            width: None,
            height: None,
            edges: BoxEdges::default(),
            border_box_sizing: false,
            margin_auto: [false; 4],
            position: Position::Static,
            offsets: [None; 4],
            display: Display::Block,
        }
    }
}

/// One in-flow child prepared for a layout system: measured border-box
/// size plus its margins.
#[derive(Debug, Clone, Copy)]
pub struct FlowItem {
    pub node: NodeId,
    /// Border-box size.
    pub width: Px,
    pub height: Px,
    pub margin: Edges,
    pub margin_auto: [bool; 4],
    pub width_definite: bool,
    pub height_definite: bool,
    pub position: Position,
    pub offsets: [Option<Px>; 4],
}

/// Resolved layout for a whole tree: border boxes relative to each
/// element's parent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayoutTree {
    pub boxes: BTreeMap<NodeId, Rect>,
}

impl LayoutTree {
    pub fn get(&self, node: NodeId) -> Option<&Rect> {
        self.boxes.get(&node)
    }
}

/// Lay out the document: root elements stack from the viewport origin.
/// Fixed-position descendants are placed against the viewport.
pub fn layout_document(
    tree: &SceneTree,
    styles: &ComputedStyles,
    context: &Context,
    reporter: &mut Reporter,
) -> LayoutTree {
    let mut state = LayoutState {
        tree,
        styles,
        context: *context,
        reporter,
        boxes: BTreeMap::new(),
        reported_values: BTreeMap::new(),
    };
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: context.viewport_width,
        height: context.viewport_height,
    };
    let mut cursor = 0.0;
    for child in tree.get(NodeId(0)).children.clone() {
        let (width, height) = state.measure(child, viewport.width);
        let rect = Rect {
            x: 0.0,
            y: cursor,
            width,
            height,
        };
        state.place(child, rect, &viewport);
        cursor += height;
    }
    LayoutTree { boxes: state.boxes }
}

struct LayoutState<'a> {
    tree: &'a SceneTree,
    styles: &'a ComputedStyles,
    context: Context,
    reporter: &'a mut Reporter,
    boxes: BTreeMap<NodeId, Rect>,
    /// Values already reported as unresolvable, so a value seen in both
    /// the measure and the place pass is only reported once.
    reported_values: BTreeMap<String, ()>,
}

impl<'a> LayoutState<'a> {
    // ---- Property resolution ------------------------------------------

    fn node_box(&mut self, node: NodeId) -> NodeBox {
        let Some(style) = self.styles.get(node) else {
            return NodeBox::default();
        };
        let padding = Edges {
            top: self.edge(style, "padding-top", "padding"),
            right: self.edge(style, "padding-right", "padding"),
            bottom: self.edge(style, "padding-bottom", "padding"),
            left: self.edge(style, "padding-left", "padding"),
        };
        let border = Edges {
            top: self.border_width(style, "border-top-width", "border-top-style"),
            right: self.border_width(style, "border-right-width", "border-right-style"),
            bottom: self.border_width(style, "border-bottom-width", "border-bottom-style"),
            left: self.border_width(style, "border-left-width", "border-left-style"),
        };
        let margin = Edges {
            top: self
                .margin_edge(style, "margin-top", "margin")
                .unwrap_or(0.0),
            right: self
                .margin_edge(style, "margin-right", "margin")
                .unwrap_or(0.0),
            bottom: self
                .margin_edge(style, "margin-bottom", "margin")
                .unwrap_or(0.0),
            left: self
                .margin_edge(style, "margin-left", "margin")
                .unwrap_or(0.0),
        };
        let margin_auto = [
            self.margin_is_auto(style, "margin-top", "margin"),
            self.margin_is_auto(style, "margin-right", "margin"),
            self.margin_is_auto(style, "margin-bottom", "margin"),
            self.margin_is_auto(style, "margin-left", "margin"),
        ];
        let offsets = [
            self.offset(style, "top"),
            self.offset(style, "right"),
            self.offset(style, "bottom"),
            self.offset(style, "left"),
        ];
        let display = match style.get("display").map(|d| d.to_ascii_lowercase()) {
            Some(ref d) if d == "flex" => Display::Flex,
            Some(ref d) if d == "grid" => Display::Grid,
            Some(ref d) if d == "table" => Display::Table,
            Some(ref d) if d == "table-row" => Display::TableRow,
            Some(ref d) if d == "table-cell" => Display::TableCell,
            Some(ref d) if d == "none" => Display::None,
            _ => Display::Block,
        };
        let position = match style.get("position").map(|p| p.to_ascii_lowercase()) {
            Some(ref p) if p == "relative" => Position::Relative,
            Some(ref p) if p == "absolute" => Position::Absolute,
            Some(ref p) if p == "fixed" => Position::Fixed,
            Some(ref p) if p == "sticky" => Position::Sticky,
            _ => Position::Static,
        };
        NodeBox {
            width: self.size(style, "width"),
            height: self.size(style, "height"),
            edges: BoxEdges {
                padding,
                border,
                margin,
            },
            border_box_sizing: style
                .get("box-sizing")
                .is_some_and(|value| value.eq_ignore_ascii_case("border-box")),
            margin_auto,
            position,
            offsets,
            display,
        }
    }

    /// Resolve a padding edge from the longhand, falling back to the
    /// shorthand's corresponding component.
    fn edge(&mut self, style: &ComputedStyle, longhand: &str, shorthand: &str) -> Px {
        if let Some(value) = style.get(longhand) {
            if let Some(px) = self.length(value, self.context.viewport_width) {
                return px;
            }
        }
        let Some(value) = style.get(shorthand) else {
            return 0.0;
        };
        let index = match shorthand_edge_index(longhand) {
            Some(index) => index,
            None => return 0.0,
        };
        shorthand_component(&split_shorthand(value), index)
            .and_then(|component| self.length(component, self.context.viewport_width))
            .unwrap_or(0.0)
    }

    /// Resolve a margin edge; `auto` resolves to zero here and is
    /// signalled separately by [`LayoutState::margin_is_auto`].
    fn margin_edge(
        &mut self,
        style: &ComputedStyle,
        longhand: &str,
        shorthand: &str,
    ) -> Option<Px> {
        if let Some(value) = style.get(longhand) {
            if !value.eq_ignore_ascii_case("auto") {
                return self.length(value, self.context.viewport_width);
            }
            return None;
        }
        let value = style.get(shorthand)?;
        let index = shorthand_edge_index(longhand)?;
        let component = shorthand_component(&split_shorthand(value), index)?.clone();
        if component.eq_ignore_ascii_case("auto") {
            return None;
        }
        self.length(&component, self.context.viewport_width)
    }

    fn margin_is_auto(&self, style: &ComputedStyle, longhand: &str, shorthand: &str) -> bool {
        if let Some(value) = style.get(longhand) {
            return value.eq_ignore_ascii_case("auto");
        }
        let Some(value) = style.get(shorthand) else {
            return false;
        };
        let index = shorthand_edge_index(longhand).unwrap_or(0);
        shorthand_component(&split_shorthand(value), index)
            .is_some_and(|component| component.eq_ignore_ascii_case("auto"))
    }

    /// Resolve a border width. A border only contributes when its style
    /// is declared and not `none`/`hidden`; keyword widths follow the
    /// CSS 2.1 convention (thin 1px, medium 3px, thick 5px).
    fn border_width(&mut self, style: &ComputedStyle, longhand: &str, style_property: &str) -> Px {
        let Some(declared) = style.get(style_property) else {
            return 0.0;
        };
        let line_style = declared.to_ascii_lowercase();
        if line_style == "none" || line_style == "hidden" {
            return 0.0;
        }
        let width = style
            .get(longhand)
            .map(str::to_string)
            .unwrap_or_else(|| "medium".to_string());
        match width.to_ascii_lowercase().as_str() {
            "thin" => 1.0,
            "medium" => 3.0,
            "thick" => 5.0,
            other => self
                .length(other, self.context.viewport_width)
                .unwrap_or(3.0),
        }
    }

    /// Resolve a `top`/`right`/... offset; `auto` is `None`.
    fn offset(&mut self, style: &ComputedStyle, property: &str) -> Option<Px> {
        let value = style.get(property)?;
        if value.eq_ignore_ascii_case("auto") {
            return None;
        }
        self.length(value, self.context.viewport_width)
    }

    /// Resolve an authored `width`/`height` value; `auto` is `None`.
    fn size(&mut self, style: &ComputedStyle, property: &str) -> Option<Px> {
        let value = style.get(property)?;
        if value.eq_ignore_ascii_case("auto") {
            return None;
        }
        self.length(value, self.context.viewport_width)
    }

    /// Resolve any length, percentage or math value to pixels.
    /// Percentages resolve against `reference`; failures report
    /// `CASA-GEO-001`/`CASA-GEO-002` once per distinct value.
    fn length(&mut self, value: &str, reference: Px) -> Option<Px> {
        let context = self.context;
        let lowered = value.to_ascii_lowercase();
        let is_math = ["calc(", "min(", "max(", "clamp("]
            .iter()
            .any(|marker| lowered.contains(marker));
        let resolved = if is_math {
            crate::geometry::calc::evaluate_value(value, reference, &context)
        } else {
            crate::geometry::dimensional::resolve_value(value, reference, &context)
        };
        match resolved {
            Some(px) => Some(px),
            None => {
                let (code, why) = if is_math {
                    (
                        codes::GEOMETRY_CALC_INVALID,
                        format!("invalid math expression: {value}"),
                    )
                } else {
                    (
                        codes::GEOMETRY_UNRESOLVABLE,
                        format!("unresolvable length: {value}"),
                    )
                };
                self.report_once(code, why);
                None
            }
        }
    }

    fn report_once(&mut self, code: DiagnosticCode, message: String) {
        if self.reported_values.insert(message.clone(), ()).is_none() {
            self.reporter.emit(code, message, None);
        }
    }

    // ---- Sizing --------------------------------------------------------

    /// Content width for a given available (parent content) width.
    fn content_width(&self, node_box: &NodeBox, available_width: Px) -> Px {
        match node_box.width {
            Some(width) => {
                if node_box.border_box_sizing {
                    (width
                        - node_box.edges.padding.horizontal()
                        - node_box.edges.border.horizontal())
                    .max(0.0)
                } else {
                    width
                }
            }
            None => (available_width
                - node_box.edges.padding.horizontal()
                - node_box.edges.border.horizontal())
            .max(0.0),
        }
    }

    /// Outer (border-box) width from the content width.
    fn outer_width(&self, node_box: &NodeBox, content_width: Px) -> Px {
        match node_box.width {
            Some(width) if node_box.border_box_sizing => width,
            _ => {
                content_width
                    + node_box.edges.padding.horizontal()
                    + node_box.edges.border.horizontal()
            }
        }
    }

    /// Outer (border-box) height from the content height.
    fn outer_height(&self, node_box: &NodeBox, content_height: Px) -> Px {
        match node_box.height {
            Some(height) if node_box.border_box_sizing => height,
            _ => {
                content_height
                    + node_box.edges.padding.vertical()
                    + node_box.edges.border.vertical()
            }
        }
    }

    /// A flex container's cross size: the authored height (rows) or
    /// width (columns) reduced to its content box. Stretch only applies
    /// when the cross size is definite.
    fn flex_cross_size(&self, node: NodeId, node_box: &NodeBox) -> (bool, Option<Px>) {
        let column = self
            .styles
            .get(node)
            .and_then(|style| style.get("flex-direction"))
            .is_some_and(|d| d.to_ascii_lowercase().contains("column"));
        let (definite, edges) = if column {
            (
                node_box.width,
                node_box.edges.padding.horizontal() + node_box.edges.border.horizontal(),
            )
        } else {
            (
                node_box.height,
                node_box.edges.padding.vertical() + node_box.edges.border.vertical(),
            )
        };
        let content_cross = definite.map(|size| {
            if node_box.border_box_sizing {
                (size - edges).max(0.0)
            } else {
                size
            }
        });
        (content_cross.is_some(), content_cross)
    }

    /// The content box inside a recorded border box.
    fn content_rect(&self, node_box: &NodeBox, rect: &Rect) -> Rect {
        Rect {
            x: rect.x + node_box.edges.border.left + node_box.edges.padding.left,
            y: rect.y + node_box.edges.border.top + node_box.edges.padding.top,
            width: (rect.width
                - node_box.edges.padding.horizontal()
                - node_box.edges.border.horizontal())
            .max(0.0),
            height: (rect.height
                - node_box.edges.padding.vertical()
                - node_box.edges.border.vertical())
            .max(0.0),
        }
    }

    // ---- Driver --------------------------------------------------------

    /// Measure an element's border-box size against an available content
    /// width without recording boxes. Out-of-flow children do not
    /// contribute.
    fn measure(&mut self, node: NodeId, available_width: Px) -> (Px, Px) {
        let node_box = self.node_box(node);
        if node_box.display == Display::None {
            return (0.0, 0.0);
        }
        let content_width = self.content_width(&node_box, available_width);
        // An authored height wins over flow content: border-box sizing
        // subtracts the edges, content-box sizing uses it as-is.
        let content_height = match node_box.height {
            Some(height) if node_box.border_box_sizing => {
                (height - node_box.edges.padding.vertical() - node_box.edges.border.vertical())
                    .max(0.0)
            }
            Some(height) => height,
            // Measure-only pass: no boxes are recorded, so there is no
            // containing block to thread; the systems only consult it
            // when `record` is set.
            None => self.children_height(node, &node_box, content_width, false, &Rect::default()),
        };
        (
            self.outer_width(&node_box, content_width),
            self.outer_height(&node_box, content_height),
        )
    }

    /// Place an element and record its subtree. `rect` is the final
    /// border box relative to the parent; `containing` is the padding
    /// box of the nearest positioned ancestor (the viewport for fixed
    /// elements and document roots).
    fn place(&mut self, node: NodeId, rect: Rect, containing: &Rect) {
        let node_box = self.node_box(node);
        if node_box.display == Display::None {
            return;
        }
        self.boxes.insert(node, rect);
        let content = self.content_rect(&node_box, &rect);
        // A positioned element becomes the containing block for its
        // absolutely positioned descendants.
        let descendant_containing = if node_box.position == Position::Static {
            *containing
        } else {
            Rect {
                x: rect.x + node_box.edges.border.left,
                y: rect.y + node_box.edges.border.top,
                width: content.width,
                height: content.height,
            }
        };
        self.children_height(node, &node_box, content.width, true, &descendant_containing);
        self.place_out_of_flow(node, &descendant_containing);
    }

    /// Lay out (and optionally record) the in-flow children of `node`.
    /// Returns the flow content height: the flow extent the assigned
    /// system produced (the caller overrides it with a definite authored
    /// height where present).
    fn children_height(
        &mut self,
        node: NodeId,
        node_box: &NodeBox,
        content_width: Px,
        record: bool,
        containing: &Rect,
    ) -> Px {
        if node_box.display == Display::Table {
            // Table layout spans row and cell levels; it drives its own
            // recursion.
            return table::layout(self, node, node_box, content_width, record, containing);
        }
        let children = self.in_flow_children(node);
        let mut items = Vec::with_capacity(children.len());
        for child in &children {
            let (width, height) = self.measure(*child, content_width);
            let child_box = self.node_box(*child);
            items.push(FlowItem {
                node: *child,
                width,
                height,
                margin: child_box.edges.margin,
                margin_auto: child_box.margin_auto,
                width_definite: child_box.width.is_some(),
                height_definite: child_box.height.is_some(),
                position: child_box.position,
                offsets: child_box.offsets,
            });
        }
        let content_local = Rect {
            x: 0.0,
            y: 0.0,
            width: content_width,
            height: 0.0,
        };
        // Dispatch to the element's layout system.
        let items_snapshot = items.clone();
        let (assigned, flow_extent) = match node_box.display {
            Display::Flex => {
                let (cross_definite, cross_size) = self.flex_cross_size(node, node_box);
                let props = flexbox::props(self, node, cross_definite, cross_size);
                let items = flexbox::prepare(self, items, content_width);
                flexbox::assign(items, &props, &content_local)
            }
            Display::Grid => {
                let props = grid::props(self, node, content_width);
                let placements = grid::placements(self, &items);
                grid::assign(items, placements, &props, &content_local)
            }
            _ => {
                let columns = multicol::column_count(self, node, content_width);
                if columns > 1 {
                    let props = multicol::props(self, node, columns);
                    multicol::assign(items, &props, &content_local)
                } else {
                    let floated: Vec<Option<float::FloatSide>> = items
                        .iter()
                        .map(|item| float::side(self.styles.get(item.node)))
                        .collect();
                    if floated.iter().any(Option::is_some) {
                        float::assign(items, &floated, &content_local)
                    } else {
                        block::assign(items, &content_local)
                    }
                }
            }
        };
        if !record {
            return flow_extent;
        }
        for (index, local) in assigned.into_iter().enumerate() {
            self.record_child(node, items_snapshot[index], local, containing);
        }
        flow_extent
    }

    /// Translate a content-local child rect into parent coordinates,
    /// apply relative/sticky offsets, record it, and recurse.
    fn record_child(&mut self, parent: NodeId, item: FlowItem, local: Rect, containing: &Rect) {
        let parent_box = self.boxes.get(&parent).copied().unwrap_or_default();
        let parent_node_box = self.node_box(parent);
        let content_x =
            parent_box.x + parent_node_box.edges.border.left + parent_node_box.edges.padding.left;
        let content_y =
            parent_box.y + parent_node_box.edges.border.top + parent_node_box.edges.padding.top;
        let mut rect = Rect {
            x: content_x + local.x,
            y: content_y + local.y,
            width: local.width,
            height: local.height,
        };
        match item.position {
            Position::Relative => {
                if let Some(top) = item.offsets[0] {
                    rect.y += top;
                }
                if let Some(left) = item.offsets[3] {
                    rect.x += left;
                }
            }
            Position::Sticky => sticky::clamp(&mut rect, &parent_box, item.offsets),
            _ => {}
        }
        self.boxes.insert(item.node, rect);
        self.place(item.node, rect, containing);
    }

    fn in_flow_children(&self, node: NodeId) -> Vec<NodeId> {
        self.tree
            .get(node)
            .children
            .iter()
            .copied()
            .filter(|child| {
                let Some(style) = self.styles.get(*child) else {
                    return true;
                };
                if style
                    .get("display")
                    .is_some_and(|display| display.eq_ignore_ascii_case("none"))
                {
                    return false;
                }
                !style.get("position").is_some_and(|position| {
                    matches!(position.to_ascii_lowercase().as_str(), "absolute" | "fixed")
                })
            })
            .collect()
    }

    /// Place `absolute` and `fixed` children against their containing
    /// block (the viewport for `fixed`).
    fn place_out_of_flow(&mut self, node: NodeId, containing: &Rect) {
        let children: Vec<NodeId> = self
            .tree
            .get(node)
            .children
            .iter()
            .copied()
            .filter(|child| {
                self.styles
                    .get(*child)
                    .and_then(|style| style.get("position"))
                    .map(|position| {
                        matches!(position.to_ascii_lowercase().as_str(), "absolute" | "fixed")
                    })
                    .unwrap_or(false)
            })
            .collect();
        for child in children {
            let is_fixed = self
                .styles
                .get(child)
                .and_then(|style| style.get("position"))
                .is_some_and(|position| position.eq_ignore_ascii_case("fixed"));
            let fixed_containing = fixed::viewport_containing(self.context);
            let effective = if is_fixed {
                &fixed_containing
            } else {
                containing
            };
            absolute::place(self, child, effective);
        }
    }
}

/// Index into a 4-component box shorthand (`padding`/`margin`):
/// top, right, bottom, left per CSS 2.1 expansion.
pub fn shorthand_edge_index(longhand: &str) -> Option<usize> {
    match longhand {
        "padding-top" | "margin-top" => Some(0),
        "padding-right" | "margin-right" => Some(1),
        "padding-bottom" | "margin-bottom" => Some(2),
        "padding-left" | "margin-left" => Some(3),
        _ => None,
    }
}

/// Pick a shorthand's component for one edge, implementing the CSS 2.1
/// 1–4 component expansion: 1 value = all edges; 2 = vertical/horizontal;
/// 3 = top / horizontal / bottom; 4 = top right bottom left.
pub fn shorthand_component(components: &[String], edge: usize) -> Option<&String> {
    match components.len() {
        1 => components.first(),
        2 => match edge {
            0 | 2 => components.first(),
            1 | 3 => components.get(1),
            _ => None,
        },
        3 => match edge {
            0 => components.first(),
            1 | 3 => components.get(1),
            2 => components.get(2),
            _ => None,
        },
        4 => components.get(edge),
        _ => None,
    }
}

/// Split a shorthand value on top-level whitespace, keeping function
/// arguments (`calc(4px + 2px)`) intact. Component values retain source
/// spans over the parsed string, so each one is spliced back verbatim.
pub fn split_shorthand(value: &str) -> Vec<String> {
    let values = parse_component_values(value);
    let mut components: Vec<String> = Vec::new();
    let mut current = String::new();
    for parsed in &values {
        match parsed {
            ComponentValue::Token(Token::Whitespace, _) => {
                if !current.is_empty() {
                    components.push(std::mem::take(&mut current));
                }
            }
            other => {
                let span = other.span();
                current.push_str(&value[span.start.min(value.len())..span.end.min(value.len())]);
            }
        }
    }
    if !current.is_empty() {
        components.push(current);
    }
    components
}
