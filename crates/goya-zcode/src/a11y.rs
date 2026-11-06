//! Wave 8 (GY-ZCODE-008) — live a11y snapshot: DIG construction from the
//! monitor's live state, republish-on-change, and a one-line-per-node
//! dump for headless verification.
//!
//! ## Node model
//! - Root window node `monitor` (role Window, label "zcode-monitor").
//! - Header title node `title` (Label, accessible name = view title) and
//!   status node `status` (Label, name = live status text). The frozen
//!   contract says "StatusText"; goya-ddh's DIG role set has no such
//!   role, so status projects as [`SemanticRole::Label`] (StaticText to
//!   AT) — the mapping table in goya-a11y is total and this keeps the
//!   projection inside the graph rather than as an out-of-tree overlay.
//! - Transcript rows `row_00`..`row_13` (Label, name = live row text).
//!   Empty rows are **omitted** from the DIG: an empty row carries no
//!   announcement value and would only add traversal noise for a screen
//!   reader; this mirrors DDH §6 information hygiene (nothing to say →
//!   not exposed). Hit regions for the omitted rows stay in
//!   [`MonitorSceneRects`] so a later need to re-expose them is pure
//!   policy, not geometry rework.
//! - Composer `composer` (Textbox) per the GY-ZCODE-007 wave-7 pattern:
//!   the scene placeholder lives in the DIG label slot as the default
//!   accessible name, `live_text` carries the composer text when
//!   non-empty, and `focused` mirrors `Composer::focused`.
//!   editable/focusable are derived at projection (enabled Textbox), not
//!   hand-set. Hit region = the scene rect.
//! - Quit button `quit` (Button, name from the scene, `press` action,
//!   hit region from the scene rect).
//!
//! ## Geometry
//! Scene rects are read **once at startup** from the `SceneStore` frame
//! laid out at the fixed 400x320 window size. The monitor window is a
//! fixed-size layout (no relayout on resize in the current host), and
//! AT hit regions at this fidelity do not need per-frame relayout —
//! only *content* (labels, live text, focus) changes per republish.
//! Rects are millipoints, copied verbatim into [`A11yRect`].
//!
//! ## Republish policy
//! goya-a11y's incremental update path (`GtkAtspiBridge::run_with_updates`)
//! is behind the `atspi-gtk` feature and is GTK/AT-SPI territory — not
//! wired here. The headless mechanism is therefore **full-tree
//! publish**: [`MonitorA11y::republish`] rebuilds the DIG and projects
//! it with `A11yTree::from_dig`, and the bin calls it only when the
//! dirty flag fires (view or composer change), never per frame.

use crate::composer::Composer;
use crate::view::{LinkState, SessionView, TAIL_LINES};
use goya_a11y::{A11yNode, A11yRect, A11yTree};
use goya_ddh::{DdhNode, Dig, InteractionState, SemanticRole};
use goya_lang::Frame;

/// Root window object id.
const ROOT_ID: &str = "monitor";
/// Composer object id (matches the scene node name, hit-tested by AT).
const COMPOSER_ID: &str = "composer";
/// Quit button object id (matches the scene node name).
const QUIT_ID: &str = "quit";

/// Clamp a millipoint extent into an [`A11yRect`] component (`i32`).
/// Monitor extents are tiny (≤ 400 px = 400_000 mp); the clamp is a
/// format boundary, not a behavior branch.
fn mp(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

fn rect(r: goya_lang::RectMp) -> A11yRect {
    A11yRect {
        x: mp(r.x),
        y: mp(r.y),
        width: mp(r.width),
        height: mp(r.height),
    }
}

/// Scene geometry captured once at startup (see module docs).
#[derive(Debug, Clone)]
pub struct MonitorSceneRects {
    /// Header title rect.
    pub title: A11yRect,
    /// Status cell rect.
    pub status: A11yRect,
    /// Transcript row rects, index = row number (`row_NN`).
    pub rows: Vec<A11yRect>,
    /// Composer hit region (doubles as the focus-acquisition region).
    pub composer: A11yRect,
    /// Quit button hit region.
    pub quit: A11yRect,
    /// Composer placeholder from the scene (DIG label slot default).
    pub composer_placeholder: String,
    /// Quit button caption from the scene.
    pub quit_label: String,
}

/// Extract the a11y geometry from a laid-out scene frame (400x320).
/// Missing nodes fall back to empty rects / names so the a11y tree
/// degrades gracefully on a pre-Wave-2 scene (no composer).
pub fn scene_rects(frame: &Frame) -> MonitorSceneRects {
    let find = |name: &str| {
        frame
            .find(name)
            .map(|n| (rect(n.rect), n.text.clone().unwrap_or_default()))
            .unwrap_or_default()
    };
    let (title, _) = find("title");
    let (status, _) = find("status");
    let rows = (0..TAIL_LINES)
        .map(|i| find(&format!("row_{i:02}")).0)
        .collect();
    let (composer, composer_placeholder) = find(COMPOSER_ID);
    let (quit, quit_label) = find(QUIT_ID);
    MonitorSceneRects {
        title,
        status,
        rows,
        composer,
        quit,
        composer_placeholder,
        quit_label,
    }
}

/// Live status text, identical to the bin's per-frame render override.
pub fn status_text(view: &SessionView) -> String {
    match &view.link {
        LinkState::Connecting => String::from("linking"),
        LinkState::Up => view.status.clone(),
        LinkState::Down(reason) => format!("down: {reason}"),
    }
}

/// Minimal DIG node constructor: exposed, enabled, default semantics.
fn dig_node(
    id: &str,
    role: SemanticRole,
    label: &str,
    parent: Option<&str>,
    r: A11yRect,
) -> DdhNode {
    DdhNode {
        id: id.to_string(),
        revision: 1,
        role,
        label: label.to_string(),
        hit_region: goya_change::PixelRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        },
        state: InteractionState {
            enabled: true,
            visible: true,
            focused: false,
        },
        actions: if role == SemanticRole::Button {
            vec!["press".to_string()]
        } else {
            Vec::new()
        },
        exposed: true,
        parent: parent.map(|p| p.to_string()),
        semantics: Default::default(),
    }
}

/// Build the monitor DIG from live state + startup geometry.
/// Empty transcript rows are skipped (see module docs).
pub fn build_dig(view: &SessionView, composer: &Composer, rects: &MonitorSceneRects) -> Dig {
    let mut dig = Dig::new();
    dig.put(
        dig_node(
            ROOT_ID,
            SemanticRole::Window,
            "zcode-monitor",
            None,
            A11yRect::default(),
        ),
        1,
    );
    dig.put(
        dig_node(
            "title",
            SemanticRole::Label,
            &view.title,
            Some(ROOT_ID),
            rects.title,
        ),
        1,
    );
    dig.put(
        dig_node(
            "status",
            SemanticRole::Label,
            &status_text(view),
            Some(ROOT_ID),
            rects.status,
        ),
        1,
    );
    let rows = view.rows();
    for (i, text) in rows.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let region = rects.rows.get(i).copied().unwrap_or_default();
        dig.put(
            dig_node(
                &format!("row_{i:02}"),
                SemanticRole::Label,
                text,
                Some(ROOT_ID),
                region,
            ),
            1,
        );
    }
    let mut comp = dig_node(
        COMPOSER_ID,
        SemanticRole::Textbox,
        &rects.composer_placeholder,
        Some(ROOT_ID),
        rects.composer,
    );
    comp.state.focused = composer.focused;
    dig.put(comp, 1);
    dig.put(
        dig_node(
            QUIT_ID,
            SemanticRole::Button,
            &rects.quit_label,
            Some(ROOT_ID),
            rects.quit,
        ),
        1,
    );
    dig
}

/// Project [`build_dig`] into the structural AT tree (live text is
/// patched onto the node snapshot by [`MonitorA11y::republish`]; see
/// its type docs for the upstream mutator gap).
pub fn build_tree(view: &SessionView, composer: &Composer, rects: &MonitorSceneRects) -> A11yTree {
    A11yTree::from_dig(&build_dig(view, composer, rects))
}

/// Republish owner: holds the startup geometry, the latest tree and a
/// publish counter (acceptance asserts one publish per dirty flag).
///
/// Upstream gap (documented, GY-ZCODE-008): `A11yTree` exposes no public
/// per-node mutator — its node map is private, so `live_text` can only
/// be set from inside goya-a11y today. goya-zcode therefore publishes
/// the structural tree with `A11yTree::from_dig` AND keeps the
/// AT-facing node snapshot ([`MonitorA11y::nodes`]) where the composer's
/// live text is patched through `A11yNode`'s public fields
/// (`A11yNode::with_live_text`). The dump and the headless acceptance
/// read the node snapshot; if goya-a11y later grows a public
/// `get_mut`/with-node update API, the patch step collapses into it.
#[derive(Debug, Clone)]
pub struct MonitorA11y {
    rects: MonitorSceneRects,
    tree: A11yTree,
    nodes: Vec<A11yNode>,
    publishes: u64,
}

impl MonitorA11y {
    /// Capture geometry once from a laid-out scene frame.
    pub fn new(frame: &Frame) -> MonitorA11y {
        MonitorA11y {
            rects: scene_rects(frame),
            tree: A11yTree::default(),
            nodes: Vec::new(),
            publishes: 0,
        }
    }

    /// Rebuild + republish. Called by the host ONLY on the dirty flag
    /// (view or composer change), never per frame.
    pub fn republish(&mut self, view: &SessionView, composer: &Composer) {
        self.tree = A11yTree::from_dig(&build_dig(view, composer, &self.rects));
        let live = (!composer.text().is_empty()).then(|| composer.text().to_owned());
        self.nodes = self
            .tree
            .iter()
            .map(|node| {
                if node.id == COMPOSER_ID {
                    node.clone()
                        .with_live_text(live.clone().unwrap_or_default())
                } else {
                    node.clone()
                }
            })
            .collect();
        // An empty live_text override announces the placeholder, matching
        // the resolved-name fallback; normalize to None for clarity.
        if live.is_none() {
            if let Some(comp) = self.nodes.iter_mut().find(|n| n.id == COMPOSER_ID) {
                comp.live_text = None;
            }
        }
        self.publishes += 1;
    }

    /// Latest published structural tree (no live text; see type docs).
    pub fn tree(&self) -> &A11yTree {
        &self.tree
    }

    /// Latest published AT-facing node snapshot (composer carries
    /// `live_text`).
    pub fn nodes(&self) -> &[A11yNode] {
        &self.nodes
    }

    /// Number of republishes since startup (acceptance counter).
    pub fn publishes(&self) -> u64 {
        self.publishes
    }

    /// Compact one-line-per-node dump (`--a11y-dump` mode): id, role,
    /// resolved (announced) name, states, actions — id-ordered.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for node in &self.nodes {
            out.push_str(&dump_line(node));
            out.push('\n');
        }
        out
    }
}

/// One compact line for an [`A11yNode`] (used by `--a11y-dump`).
fn dump_line(node: &A11yNode) -> String {
    let states = [
        ("enabled", node.enabled),
        ("focused", node.focused),
        ("editable", node.editable),
        ("focusable", node.focusable),
    ]
    .into_iter()
    .filter(|(_, v)| *v)
    .map(|(k, _)| k)
    .collect::<Vec<_>>()
    .join(",");
    format!(
        "{} role={:?} name={:?} live={:?} states=[{}] actions=[{}]",
        node.id,
        node.role,
        node.resolved_name(),
        node.live_text.as_deref().unwrap_or(""),
        states,
        node.actions_for_at().join(","),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_a11y::A11yRole;

    /// Synthetic live state: a session title, an idle status, a couple
    /// of transcript rows and a focused composer holding typed text.
    fn synthetic_state() -> (SessionView, Composer) {
        let mut view = SessionView::default();
        view.title = "session alpha".to_string();
        view.push_line("> hello");
        view.push_line("final answer");
        let mut composer = Composer::new();
        composer.focused = true;
        for ch in "typing".chars() {
            composer.handle_char(ch);
        }
        (view, composer)
    }

    /// Startup geometry independent of any scene file (acceptance uses
    /// the same construction as `MonitorA11y::new`, but with a stub
    /// frame so the test stays hermetic).
    fn rects_for_frame(frame: &Frame) -> MonitorSceneRects {
        scene_rects(frame)
    }

    fn stub_rects() -> MonitorSceneRects {
        MonitorSceneRects {
            title: A11yRect::default(),
            status: A11yRect::default(),
            rows: (0..TAIL_LINES).map(|_| A11yRect::default()).collect(),
            composer: A11yRect {
                x: 6_000,
                y: 292_000,
                width: 270_000,
                height: 24_000,
            },
            quit: A11yRect {
                x: 290_000,
                y: 292_000,
                width: 100_000,
                height: 24_000,
            },
            composer_placeholder: "type here…".to_string(),
            quit_label: "Quit".to_string(),
        }
    }

    fn empty_frame() -> Frame {
        Frame::default()
    }

    fn fresh(rects: MonitorSceneRects) -> MonitorA11y {
        MonitorA11y {
            rects,
            tree: A11yTree::default(),
            nodes: Vec::new(),
            publishes: 0,
        }
    }

    #[test]
    fn dig_dump_carries_composer_rows_and_quit() {
        let (view, composer) = synthetic_state();
        let rects = stub_rects();
        let mut a11y = fresh(rects.clone());
        a11y.republish(&view, &composer);
        let find = |id: &str| {
            a11y.nodes()
                .iter()
                .find(|n| n.id == id)
                .cloned()
                .unwrap_or_else(|| panic!("{id} projected"))
        };

        // Composer: Textbox with placeholder name, live text override,
        // focused/editable/focusable states, scene hit region.
        let comp = find(COMPOSER_ID);
        assert_eq!(comp.role, A11yRole::TextInput);
        assert_eq!(comp.accessible_name, "type here…");
        assert_eq!(comp.resolved_name(), "typing", "live text wins");
        assert_eq!(comp.live_text.as_deref(), Some("typing"));
        assert!(comp.focused);
        assert!(comp.editable);
        assert!(comp.focusable);
        assert_eq!(comp.hit_region, rects.composer);
        assert_eq!(comp.actions_for_at(), vec!["set_value", "focus"]);

        // Rows carry their text; empty tail rows are omitted. Two lines
        // sit at the bottom of the 14-row window (rows() pads the front).
        assert_eq!(find("row_12").resolved_name(), "> hello");
        assert_eq!(find("row_13").resolved_name(), "final answer");
        assert!(
            a11y.nodes().iter().all(|n| !n.id.starts_with("row_0")),
            "empty rows are skipped"
        );

        // Quit: pressable button with its hit region.
        let quit = find(QUIT_ID);
        assert_eq!(quit.role, A11yRole::Button);
        assert!(quit.focusable);
        assert_eq!(quit.actions_for_at(), vec!["press", "keyboard_activate"]);
        assert_eq!(quit.hit_region, rects.quit);

        // The dump prints one line per node and mentions the key facts.
        let dump = a11y.dump();
        let lines: Vec<&str> = dump.lines().collect();
        assert_eq!(lines.len(), a11y.nodes().len(), "one line per node");
        let comp_line = lines.iter().find(|l| l.starts_with(COMPOSER_ID)).unwrap();
        assert!(comp_line.contains("TextInput") && comp_line.contains("typing"));
        let quit_line = lines.iter().find(|l| l.starts_with(QUIT_ID)).unwrap();
        assert!(quit_line.contains("Button") && quit_line.contains("Quit"));
    }

    #[test]
    fn republish_fires_exactly_once_per_dirty() {
        let (view, composer) = synthetic_state();
        let mut a11y = fresh(stub_rects());
        assert_eq!(a11y.publishes(), 0);
        a11y.republish(&view, &composer);
        assert_eq!(a11y.publishes(), 1, "one dirty → one republish");
        a11y.republish(&view, &composer);
        assert_eq!(a11y.publishes(), 2, "each dirty flag republishes once");
        // Republish reflects new content (composer committed → cleared,
        // live_text falls back to the placeholder).
        let mut committed = composer.clone();
        committed.handle_char('\n');
        a11y.republish(&view, &committed);
        let comp = a11y
            .nodes()
            .iter()
            .find(|n| n.id == COMPOSER_ID)
            .expect("composer present");
        assert_eq!(comp.live_text, None, "empty text → placeholder name");
        assert_eq!(comp.resolved_name(), "type here…");
        assert_eq!(a11y.publishes(), 3);
    }

    #[test]
    fn scene_rects_are_read_from_a_frame_once() {
        // Hermetic frame check: scene_rects tolerates a missing scene
        // (graceful degradation) and `MonitorA11y::new` accepts any frame.
        let rects = rects_for_frame(&empty_frame());
        assert_eq!(rects.composer_placeholder, "", "no scene → no placeholder");
        assert_eq!(rects.composer, A11yRect::default());
        let mut a11y = MonitorA11y::new(&empty_frame());
        let (view, composer) = synthetic_state();
        a11y.republish(&view, &composer);
        // Degraded geometry still projects content with the scene-name
        // placeholder fallback for the composer label slot.
        let comp = a11y
            .nodes()
            .iter()
            .find(|n| n.id == COMPOSER_ID)
            .expect("composer present");
        assert_eq!(comp.resolved_name(), "typing");
        assert_eq!(a11y.publishes(), 1);
    }

    #[test]
    fn status_text_tracks_link_state() {
        let mut view = SessionView::default();
        assert_eq!(status_text(&view), "linking");
        view.link = LinkState::Up;
        view.status = "subscribed".to_string();
        assert_eq!(status_text(&view), "subscribed");
        view.link = LinkState::Down("eof".into());
        assert_eq!(status_text(&view), "down: eof");
    }
}
