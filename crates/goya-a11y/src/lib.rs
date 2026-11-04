//! GY-A11Y/1 — Assistive-technology projection of the DDH DIG.
//!
//! Directive §10 ("Accessibility implemented too late"): accessibility is
//! not a bolt-on overlay but a projection of the same Deterministic
//! Interaction Graph that DDH traverses. Assistive technologies see
//! exactly what the exported DDH view exposes — no more, no less
//! (information hygiene, DDH §6: hidden nodes are invisible to AT too).
//!
//! This crate:
//! - projects [`goya_ddh::DdhNode`]s into [`A11yNode`]s with AT roles,
//!   accessible names, state and a keyboard-operable action set;
//! - validates perceptual requirements against `goya-chroma` (WCAG
//!   contrast) and the RARS text-protection policy (legibility guard).

use goya_chroma::{oklch_to_srgb, Oklch, WcagLevel};
use goya_ddh::{DdhNode, Dig, SemanticRole};
use std::collections::BTreeMap;

#[cfg(feature = "atspi-gtk")]
pub mod gtk_atspi;

/// Contract identifier implemented by this crate.
pub const A11Y_CONTRACT: &str = "GY-A11Y/1";

/// Role exposed to assistive technologies.
///
/// Deliberately modelled on the platform-neutral AT role set (UIA/AX
/// equivalents); not a copy of [`SemanticRole`] so new DIG roles must be
/// mapped explicitly rather than silently passed through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum A11yRole {
    /// Top-level container (`Window`).
    Window,
    /// Grouping container (`Panel`).
    Group,
    /// Static, read-only text (`Label`).
    StaticText,
    /// Keyboard-activatable control (`Button`).
    Button,
    /// Editable text field (`Textbox`).
    TextInput,
    /// Bounded value control (`Slider`).
    Slider,
    /// Item inside a menu container (`MenuItem`).
    MenuItem,
}

impl A11yRole {
    /// True for roles an end user operates directly; keyboard-reachability
    /// auditing applies to these.
    pub fn is_interactive(self) -> bool {
        matches!(
            self,
            A11yRole::Button | A11yRole::TextInput | A11yRole::Slider | A11yRole::MenuItem
        )
    }
}

/// Map a DIG [`SemanticRole`] to its AT equivalent. Total over the DIG
/// role set: a role added to DDH without a mapping here is a compile
/// error, which is the point (§10 — accessibility never lags the graph).
pub fn map_role(role: SemanticRole) -> A11yRole {
    match role {
        SemanticRole::Window => A11yRole::Window,
        SemanticRole::Panel => A11yRole::Group,
        SemanticRole::Label => A11yRole::StaticText,
        SemanticRole::Button => A11yRole::Button,
        SemanticRole::Textbox => A11yRole::TextInput,
        SemanticRole::Slider => A11yRole::Slider,
        SemanticRole::MenuItem => A11yRole::MenuItem,
    }
}

/// Hit region in millipoints, structurally identical to
/// `goya_change::PixelRect` (which `goya_ddh` uses but does not
/// re-export, and which is not a declared dependency of this crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct A11yRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl A11yRect {
    fn from_parts(x: i32, y: i32, width: i32, height: i32) -> A11yRect {
        A11yRect {
            x,
            y,
            width,
            height,
        }
    }
}

/// One node of the assistive-technology tree.
#[derive(Debug, Clone, PartialEq)]
pub struct A11yNode {
    /// Stable semantic object id (same id space as the DIG).
    pub id: String,
    pub role: A11yRole,
    /// Accessible name: what a screen reader announces for this node.
    pub accessible_name: String,
    pub enabled: bool,
    pub focused: bool,
    /// Interactive hit region in millipoints (identical to the DIG's).
    pub hit_region: A11yRect,
    /// DIG parent id (typed "contains" edge, preserved for reachability).
    pub parent: Option<String>,
}

impl A11yNode {
    pub fn from_ddh(node: &DdhNode) -> A11yNode {
        let r = node.hit_region;
        A11yNode {
            id: node.id.clone(),
            role: map_role(node.role),
            accessible_name: node.label.clone(),
            enabled: node.state.enabled,
            focused: node.state.focused,
            hit_region: A11yRect::from_parts(r.x, r.y, r.width, r.height),
            parent: node.parent.clone(),
        }
    }

    /// Action set exposed to assistive technologies.
    ///
    /// Keyboard operability is mandatory for interactive roles: buttons
    /// are always keyboard-activatable, so `keyboard_activate` is exposed
    /// unconditionally for them regardless of the DIG's declared pointer
    /// actions. Static text exposes no actions — it is read-only.
    pub fn actions_for_at(&self) -> Vec<&'static str> {
        match self.role {
            A11yRole::Button => vec!["press", "keyboard_activate"],
            A11yRole::TextInput => vec!["set_value", "focus"],
            A11yRole::Slider => vec!["set_value", "focus"],
            A11yRole::MenuItem => vec!["invoke", "focus"],
            A11yRole::Window | A11yRole::Group | A11yRole::StaticText => Vec::new(),
        }
    }
}

/// The assistive-technology tree: the exported view of the DIG.
#[derive(Debug, Clone, Default)]
pub struct A11yTree {
    nodes: BTreeMap<String, A11yNode>,
}

impl A11yTree {
    /// Project the DIG's exported view (exposed AND visible — hidden
    /// nodes are invisible to assistive technology too, DDH §6) into an
    /// AT tree, preserving parent edges.
    pub fn from_dig(dig: &Dig) -> A11yTree {
        let mut nodes = BTreeMap::new();
        for node in dig.exported() {
            nodes.insert(node.id.clone(), A11yNode::from_ddh(node));
        }
        A11yTree { nodes }
    }

    pub fn get(&self, id: &str) -> Option<&A11yNode> {
        self.nodes.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &A11yNode> {
        self.nodes.values()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Screen-reader lookup by accessible name (exact match, first in id
    /// order). This is the AT analogue of `ddh.find`.
    pub fn find_by_name(&self, name: &str) -> Option<&A11yNode> {
        self.nodes.values().find(|n| n.accessible_name == name)
    }

    /// Keyboard-operability audit: every ENABLED interactive node
    /// (Button/TextInput/Slider — menu items are reached through their
    /// menu container) must be reachable via an unbroken parent chain to
    /// a root (a node with no parent that is itself in the tree). A node
    /// whose parent chain is broken — e.g. its parent is hidden and thus
    /// projected out — is flagged as not keyboard-reachable, matching the
    /// "orphan node" failure mode.
    pub fn keyboard_reachable(&self) -> Vec<String> {
        let mut reachable: BTreeMap<&str, ()> = BTreeMap::new();
        for root in self.nodes.values().filter(|n| n.parent.is_none()) {
            mark_reachable(&self.nodes, &root.id, &mut reachable);
        }
        self.nodes
            .values()
            .filter(|n| {
                n.enabled
                    && matches!(
                        n.role,
                        A11yRole::Button | A11yRole::TextInput | A11yRole::Slider
                    )
                    && !reachable.contains_key(n.id.as_str())
            })
            .map(|n| n.id.clone())
            .collect()
    }
}

fn mark_reachable<'a>(
    nodes: &'a BTreeMap<String, A11yNode>,
    id: &str,
    reachable: &mut BTreeMap<&'a str, ()>,
) {
    if reachable.contains_key(id) {
        return;
    }
    if let Some(node) = nodes.get(id) {
        reachable.insert(node.id.as_str(), ());
        for child in nodes.values().filter(|n| n.parent.as_deref() == Some(id)) {
            mark_reachable(nodes, &child.id, reachable);
        }
    }
}

/// Result of a WCAG contrast validation.
#[derive(Debug, Clone, PartialEq)]
pub struct ContrastReport {
    /// WCAG 2.x contrast ratio (1.0 – 21.0).
    pub ratio: f64,
    /// Whether the ratio meets the requested level.
    pub passes: bool,
}

/// Validate foreground/background contrast at a WCAG level.
///
/// `goya_chroma::resolver::contrast_ratio` exists but lives in the
/// crate-private `resolver` module (it is not re-exported from
/// `goya_chroma`), so the identical WCAG ratio is computed here from the
/// public `oklch_to_srgb` + `Rgb8::relative_luminance` API.
pub fn validate_contrast(fg: Oklch, bg: Oklch, level: WcagLevel) -> ContrastReport {
    let lf = oklch_to_srgb(fg).relative_luminance();
    let lb = oklch_to_srgb(bg).relative_luminance();
    let (hi, lo) = if lf >= lb { (lf, lb) } else { (lb, lf) };
    let ratio = (hi + 0.05) / (lo + 0.05);
    ContrastReport {
        ratio,
        passes: ratio >= level.ratio(),
    }
}

/// Text legibility guard, mirroring the RARS fidelity policy: when text
/// is protected, the block-glyph scale must never drop below 2 px;
/// unprotected text may go to 1 px.
pub fn text_legibility_guard(glyph_px: u32, protect_text: bool) -> bool {
    if protect_text {
        glyph_px >= 2
    } else {
        glyph_px >= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_ddh::InteractionState;

    fn node(id: &str, role: SemanticRole, label: &str, parent: Option<&str>) -> DdhNode {
        // `goya_change::PixelRect` is not a declared dependency of this
        // crate (goya-ddh does not re-export it), so build it via
        // Default + public field assignment.
        let hit_region = goya_change::PixelRect {
            width: 100,
            height: 40,
            ..Default::default()
        };
        DdhNode {
            id: id.into(),
            revision: 1,
            role,
            label: label.into(),
            hit_region,
            state: InteractionState {
                enabled: true,
                visible: true,
                focused: false,
            },
            actions: vec!["press".into()],
            exposed: true,
            parent: parent.map(|p| p.to_string()),
        }
    }

    fn oklch_grey(l: f64) -> Oklch {
        Oklch { l, c: 0.0, h: 0.0 }
    }

    #[test]
    fn role_mapping_is_total_over_semantic_roles() {
        let cases = [
            (SemanticRole::Window, A11yRole::Window),
            (SemanticRole::Panel, A11yRole::Group),
            (SemanticRole::Label, A11yRole::StaticText),
            (SemanticRole::Button, A11yRole::Button),
            (SemanticRole::Textbox, A11yRole::TextInput),
            (SemanticRole::Slider, A11yRole::Slider),
            (SemanticRole::MenuItem, A11yRole::MenuItem),
        ];
        for (semantic, expected) in cases {
            assert_eq!(map_role(semantic), expected, "mapping for {semantic:?}");
        }
    }

    #[test]
    fn buttons_always_expose_keyboard_activation() {
        let n = A11yNode::from_ddh(&node("b", SemanticRole::Button, "OK", None));
        let actions = n.actions_for_at();
        assert!(actions.contains(&"press"));
        // Keyboard operability is mandatory for buttons, even if the DIG
        // never declared a keyboard action.
        assert!(actions.contains(&"keyboard_activate"));
        // Static text is read-only: no AT actions.
        let label = A11yNode::from_ddh(&node("l", SemanticRole::Label, "Name", None));
        assert!(label.actions_for_at().is_empty());
    }

    #[test]
    fn hidden_nodes_are_excluded_from_the_at_tree() {
        let mut dig = Dig::new();
        dig.put(node("root", SemanticRole::Window, "Main", None), 1);
        dig.put(
            node("btn", SemanticRole::Button, "Visible", Some("root")),
            1,
        );
        let mut secret = node("secret", SemanticRole::Button, "Secret", Some("root"));
        secret.exposed = false;
        dig.put(secret, 1);
        let mut invisible = node("ghost", SemanticRole::Button, "Ghost", Some("root"));
        invisible.state.visible = false;
        dig.put(invisible, 1);

        let tree = A11yTree::from_dig(&dig);
        assert_eq!(tree.len(), 2, "only exposed+visible nodes project");
        assert!(tree.find_by_name("Secret").is_none());
        assert!(tree.find_by_name("Ghost").is_none());
        assert!(tree.find_by_name("Visible").is_some());
    }

    #[test]
    fn keyboard_reachability_flags_orphans() {
        let mut dig = Dig::new();
        dig.put(node("root", SemanticRole::Window, "Main", None), 1);
        dig.put(
            node("panel", SemanticRole::Panel, "Controls", Some("root")),
            1,
        );
        dig.put(node("ok", SemanticRole::Button, "OK", Some("panel")), 1);
        // Orphan: parent "vanished" is not in the DIG at all.
        dig.put(
            node("orphan", SemanticRole::Button, "Orphan", Some("vanished")),
            1,
        );
        // Child of an unexposed parent: the parent chain breaks at the
        // projection boundary, so it is unreachable too.
        let mut hidden_parent = node("hidden", SemanticRole::Panel, "Hidden", Some("root"));
        hidden_parent.exposed = false;
        dig.put(hidden_parent, 1);
        dig.put(
            node("buried", SemanticRole::Textbox, "Buried", Some("hidden")),
            1,
        );

        let tree = A11yTree::from_dig(&dig);
        let mut unreachable = tree.keyboard_reachable();
        unreachable.sort();
        assert_eq!(unreachable, vec!["buried", "orphan"]);
        assert!(!unreachable.contains(&"ok".to_string()));
    }

    #[test]
    fn enabled_state_gates_keyboard_reachability() {
        let mut dig = Dig::new();
        dig.put(node("root", SemanticRole::Window, "Main", None), 1);
        let mut off = node("off", SemanticRole::Button, "Disabled", Some("root"));
        off.state.enabled = false;
        dig.put(off, 1);
        // A disabled orphan is not flagged: disabled controls are not
        // expected to be keyboard-operable.
        let mut off_orphan = node("off2", SemanticRole::Button, "Off2", None);
        off_orphan.state.enabled = false;
        dig.put(off_orphan, 1);
        let tree = A11yTree::from_dig(&dig);
        assert!(tree.keyboard_reachable().is_empty());
    }

    #[test]
    fn contrast_aa_white_on_black_passes() {
        let white = oklch_grey(1.0);
        let black = oklch_grey(0.0);
        let report = validate_contrast(white, black, WcagLevel::AA);
        assert!(report.passes, "white on black ratio {}", report.ratio);
        assert!((report.ratio - 21.0).abs() < 0.5);
        // Symmetric in argument order.
        let flipped = validate_contrast(black, white, WcagLevel::AA);
        assert_eq!(flipped.ratio, report.ratio);
        // Same colour on itself never passes any level.
        let same = validate_contrast(white, white, WcagLevel::AA);
        assert!(!same.passes);
        assert!((same.ratio - 1.0).abs() < 1e-9);
    }

    #[test]
    fn contrast_aa_grey_on_grey_fails() {
        // #777 on #888: two mid greys, ratio well below 4.5:1.
        let g1 = goya_chroma::hex_anchor("#777777").expect("#777 parses");
        let g2 = goya_chroma::hex_anchor("#888888").expect("#888 parses");
        let report = validate_contrast(g1, g2, WcagLevel::AA);
        assert!(!report.passes, "#777 on #888 ratio {}", report.ratio);
        assert!(report.ratio < 2.0);
    }

    #[test]
    fn contrast_aaa_threshold_is_stricter() {
        let white = oklch_grey(1.0);
        // #767676 on white ≈ 4.54:1 — the canonical AA-only grey.
        let grey = goya_chroma::hex_anchor("#767676").expect("#767676 parses");
        let aa = validate_contrast(white, grey, WcagLevel::AA);
        let aaa = validate_contrast(white, grey, WcagLevel::AAA);
        assert_eq!(aa.ratio, aaa.ratio);
        assert!(aa.passes, "ratio {}", aa.ratio);
        assert!(
            !aaa.passes,
            "#767676 on white is AA ({:.2}) but not AAA",
            aa.ratio
        );
    }

    #[test]
    fn legibility_guard_mirrors_rars_policy() {
        // Protected text never below glyph 2.
        assert!(!text_legibility_guard(1, true));
        assert!(text_legibility_guard(2, true));
        assert!(text_legibility_guard(4, true));
        // Unprotected text may go to 1 px.
        assert!(text_legibility_guard(1, false));
        // Zero glyphs is always illegible.
        assert!(!text_legibility_guard(0, false));
    }

    #[test]
    fn tree_preserves_parent_structure() {
        let mut dig = Dig::new();
        dig.put(node("root", SemanticRole::Window, "Main", None), 1);
        dig.put(
            node("panel", SemanticRole::Panel, "Controls", Some("root")),
            1,
        );
        dig.put(node("ok", SemanticRole::Button, "OK", Some("panel")), 1);
        let tree = A11yTree::from_dig(&dig);
        assert_eq!(
            tree.get("ok").expect("ok present").parent.as_deref(),
            Some("panel")
        );
        assert_eq!(
            tree.get("panel").expect("panel present").parent.as_deref(),
            Some("root")
        );
        assert_eq!(tree.get("root").expect("root present").parent, None);
        // Roles survive projection.
        assert_eq!(tree.get("ok").map(|n| n.role), Some(A11yRole::Button));
        assert_eq!(tree.get("panel").map(|n| n.role), Some(A11yRole::Group));
        // Hit region survives projection (millipoints, copied verbatim).
        let hit = tree.get("ok").expect("ok present").hit_region;
        assert_eq!(
            hit,
            A11yRect {
                x: 0,
                y: 0,
                width: 100,
                height: 40
            }
        );
    }

    #[test]
    fn name_lookup_finds_exact_accessible_names() {
        let mut dig = Dig::new();
        dig.put(node("root", SemanticRole::Window, "Main", None), 1);
        dig.put(node("save", SemanticRole::Button, "Save", Some("root")), 1);
        dig.put(
            node("save_as", SemanticRole::Button, "Save As", Some("root")),
            1,
        );
        let tree = A11yTree::from_dig(&dig);
        assert_eq!(
            tree.find_by_name("Save").map(|n| n.id.as_str()),
            Some("save"),
            "exact match only, first in id order"
        );
        assert_eq!(tree.find_by_name("save").map(|n| n.id.as_str()), None);
        assert_eq!(
            tree.find_by_name("Save As").map(|n| n.id.as_str()),
            Some("save_as")
        );
        assert!(tree.find_by_name("Nothing").is_none());
    }
}
