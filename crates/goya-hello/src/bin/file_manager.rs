//! goya-file-manager — the FILE MANAGER exemplar Goya application.
//!
//! The UI is 100% declared in `apps/file_manager.gscene` (GY-LANG/1
//! RUGID-style syntax). This host is the behavioural half: it walks a
//! deterministic IN-MEMORY virtual filesystem, overrides the `path_text`,
//! `entry_0..entry_7` and `status_text` cells with live state, routes
//! up/open through hit-test, and supports entry selection by clicking
//! the entry rows.
//!
//!   cargo run --bin file_manager -- --live
//!
//! Without `--live`, a scripted acceptance sequence runs headless and
//! prints PASS/FAIL per check (root listing, open dir, open file, up,
//! up-at-root, open-on-empty no-op, padding).

use goya_lang::{hit_test, layout, SceneStore};

const SCENE_PATH: &str = "apps/file_manager.gscene";
const EXTENT_W_PT: f64 = 100.0;
const EXTENT_H_PT: f64 = 150.0;
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 600;
const ENTRY_COUNT: usize = 8;
/// Entry rows start at pt y = 35, each 12 pt tall (11 pt cell + 1 pt gap).
const ENTRY_Y0_PT: i64 = 35;
const ENTRY_PITCH_PT: i64 = 12;

// ---- Deterministic in-memory virtual filesystem -----------------------

#[derive(Debug)]
struct VfsNode {
    name: &'static str,
    dir: bool,
    children: Vec<VfsNode>,
}

fn vfs() -> VfsNode {
    VfsNode {
        name: "root",
        dir: true,
        children: vec![
            VfsNode {
                name: "docs",
                dir: true,
                children: vec![
                    VfsNode { name: "intro.doc", dir: false, children: Vec::new() },
                    VfsNode { name: "notes.txt", dir: false, children: Vec::new() },
                ],
            },
            VfsNode {
                name: "media",
                dir: true,
                children: vec![
                    VfsNode { name: "movie1", dir: false, children: Vec::new() },
                    VfsNode { name: "movie2", dir: false, children: Vec::new() },
                ],
            },
            VfsNode { name: "pics", dir: true, children: Vec::new() },
            VfsNode { name: "readme.txt", dir: false, children: Vec::new() },
        ],
    }
}

/// Walk the tree along `path`; the root itself is never in the path.
fn node_at<'a>(root: &'a VfsNode, path: &[String]) -> &'a VfsNode {
    let mut cur = root;
    for seg in path {
        cur = cur
            .children
            .iter()
            .find(|c| c.dir && c.name == seg)
            .unwrap_or(cur);
    }
    cur
}

// ---- File manager state machine (pure logic, no rendering) ------------

struct FileManager {
    tree: VfsNode,
    /// Directory path below root (e.g. ["docs"] for root/docs).
    path: Vec<String>,
    /// Index of the highlighted entry row.
    selected: usize,
    status: String,
}

impl FileManager {
    fn new() -> Self {
        FileManager {
            tree: vfs(),
            path: Vec::new(),
            selected: 0,
            status: "ready".into(),
        }
    }

    /// Current directory's children, sorted by name, dirs marked '/'.
    fn listing(&self) -> Vec<String> {
        let mut labels: Vec<String> = node_at(&self.tree, &self.path)
            .children
            .iter()
            .map(|c| {
                if c.dir {
                    format!("{}/", c.name)
                } else {
                    c.name.to_string()
                }
            })
            .collect();
        labels.sort();
        labels
    }

    /// The 8 entry rows: listing padded with empty strings.
    fn entries(&self) -> Vec<String> {
        let mut v = self.listing();
        v.resize(ENTRY_COUNT, String::new());
        v
    }

    /// Path bar text: "root/docs" — no leading separator ('/' only
    /// exists as the directory marker in listings).
    fn path_text(&self) -> String {
        let mut s = String::from("root");
        for seg in &self.path {
            s.push('/');
            s.push_str(seg);
        }
        s
    }

    /// Open the entry at `idx`. Dir => enter it; file => select it;
    /// out of range / empty row => no-op.
    fn open(&mut self, idx: usize) {
        let listing = self.listing();
        let Some(label) = listing.get(idx) else {
            return; // open nonexistent (empty) entry: no-op
        };
        if label.ends_with('/') {
            self.path.push(label.trim_end_matches('/').to_string());
            self.selected = 0;
            self.status = "ready".into();
        } else {
            self.selected = idx;
            self.status = format!("selected {label}");
        }
    }

    /// Move to the parent directory; at root: stay, report it.
    fn up(&mut self) {
        if self.path.pop().is_none() {
            self.status = "at root".into();
        } else {
            self.selected = 0;
            self.status = "ready".into();
        }
    }

    /// A click on entry row `idx`: select it, then open it.
    fn click_entry(&mut self, idx: usize) {
        self.selected = idx;
        self.open(idx);
    }

    /// Toolbar press by scene name.
    fn press(&mut self, name: &str) {
        match name {
            "btn_up" => self.up(),
            "btn_open" => self.open(self.selected),
            _ => {}
        }
    }
}

// ---- Scene plumbing ---------------------------------------------------

/// Host-side data binding: override the path bar, entry rows and status
/// with live state (the scene declares structure, the host supplies it).
fn with_state(mut nodes: Vec<goya_lang::DrawNode>, fm: &FileManager) -> Vec<goya_lang::DrawNode> {
    let entries = fm.entries();
    for n in &mut nodes {
        match n.name.as_str() {
            "path_text" => n.text = Some(fm.path_text()),
            "status_text" => n.text = Some(fm.status.clone()),
            name => {
                if let Some(rest) = name.strip_prefix("entry_") {
                    if let Ok(i) = rest.parse::<usize>() {
                        if i < ENTRY_COUNT {
                            n.text = Some(entries[i].clone());
                        }
                    }
                }
            }
        }
    }
    nodes
}

fn present_nodes(nodes: &[goya_lang::DrawNode]) -> Vec<goya_runtime::Primitive> {
    nodes
        .iter()
        .map(|n| {
            let rect = goya_layout::Rect {
                x: n.rect.x,
                y: n.rect.y,
                width: n.rect.width,
                height: n.rect.height,
            };
            let label = n.text.clone().unwrap_or_default();
            if n.is_button {
                let label = if label.is_empty() { n.name.clone() } else { label };
                Primitive::Button(rect, label)
            } else if n.text.is_some() {
                Primitive::Text(rect, label)
            } else {
                Primitive::Panel(rect)
            }
        })
        .collect()
}

use goya_runtime::Primitive;

/// Which entry row (if any) holds the given window point? Points are
/// converted from pixels (px = pt * 4) and matched against the row bands.
fn entry_at(px: i32, py: i32) -> Option<usize> {
    let x_pt = i64::from(px) / 4;
    let y_pt = i64::from(py) / 4;
    if !(5..95).contains(&x_pt) {
        return None;
    }
    for i in 0..ENTRY_COUNT {
        let top = ENTRY_Y0_PT + i as i64 * ENTRY_PITCH_PT;
        if y_pt >= top && y_pt < top + ENTRY_PITCH_PT - 1 {
            return Some(i);
        }
    }
    None
}

fn modified(m: &std::fs::Metadata) -> u64 {
    use std::time::UNIX_EPOCH;
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");
    let scene_path = args
        .iter()
        .position(|a| a == "--file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or(SCENE_PATH);

    let source = std::fs::read_to_string(scene_path).unwrap_or_else(|e| {
        eprintln!("goya-file-manager: cannot read {scene_path}: {e}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-file-manager: {e}");
            std::process::exit(1);
        }
    };

    if !live {
        self_test(&mut store);
        return;
    }

    let mut win = match goya_wayland::GoyaWindow::create("goya-files", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-file-manager: live unavailable: {e}");
            return;
        }
    };
    let mut fm = FileManager::new();
    let mut dirty = true; // first frame always paints
    while !win.close_requested() {
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT);
                    if let Some(i) = entry_at(x, y) {
                        fm.click_entry(i);
                        dirty = true;
                    } else if let Some(name) =
                        hit_test(&frame, i64::from(x) * goya_wayland::MP_PER_PX, i64::from(y) * goya_wayland::MP_PER_PX)
                    {
                        if name == "btn_up" || name == "btn_open" {
                            fm.press(&name);
                            dirty = true;
                        }
                    }
                }
                goya_wayland::PlatformEvent::Resized(w, h) => {
                    println!("goya-file-manager: resized {w}x{h}");
                    dirty = true;
                }
                _ => {}
            }
        }
        if dirty {
            let nodes = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT).nodes;
            let nodes = with_state(nodes, &fm);
            let prims = present_nodes(&nodes);
            let fills: Vec<Option<u32>> = nodes.iter().map(|n| n.fill).collect();
            let _ = win.present(&prims, &fills, [235, 235, 240], [20, 20, 28], 3);
            dirty = false;
        }
        win.pump(1);
    }
    println!(
        "goya-file-manager live: last path '{}', status '{}', generation {}",
        fm.path_text(),
        fm.status,
        store.generation()
    );
}

/// Headless acceptance: drive the state machine exactly as clicks would,
/// verify path/listing/status, and sanity-check scene structure.
fn self_test(store: &mut SceneStore) {
    let mut failures = Vec::new();
    let mut fm = FileManager::new();
    let frame = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT);

    // Structural checks: toolbar buttons present, cells present.
    for n in [
        "path_bar",
        "path_text",
        "btn_up",
        "btn_open",
        "status_text",
        "entry_0",
        "entry_1",
        "entry_2",
        "entry_3",
        "entry_4",
        "entry_5",
        "entry_6",
        "entry_7",
    ] {
        if frame.find(n).is_none() {
            failures.push(format!("missing cell/button {n}"));
        }
    }
    if let Some(b) = frame.find("btn_open") {
        let hit = hit_test(&frame, b.rect.x + 1_000, b.rect.y + 1_000);
        if hit.as_deref() != Some("btn_open") {
            failures.push("btn_open not hit-testable".into());
        }
    }
    // Window extent 100x150 pt => 400x600 px window.
    if let Some(w) = frame.find("window") {
        if w.rect.width != 100_000 || w.rect.height != 150_000 {
            failures.push(format!(
                "window extent: expected 100000x150000, got {}x{}",
                w.rect.width, w.rect.height
            ));
        }
    }

    // Root listing: sorted, dirs marked '/'.
    let entries = fm.entries();
    let expected_root = ["docs/", "media/", "pics/", "readme.txt"];
    if entries[..4] != expected_root {
        failures.push(format!("root listing: expected {expected_root:?}, got {:?}", &entries[..4]));
    }
    if entries.len() != ENTRY_COUNT {
        failures.push(format!("entry count: expected {ENTRY_COUNT}, got {}", entries.len()));
    }
    if entries[4..].iter().any(|e| !e.is_empty()) {
        failures.push("padding: rows 4..8 should be empty".into());
    }
    if fm.path_text() != "root" {
        failures.push(format!("path bar: expected root, got {}", fm.path_text()));
    }

    // Open "docs" (row 0) -> path bar "root/docs", listing intro.doc notes.txt.
    fm.click_entry(0);
    if fm.path_text() != "root/docs" {
        failures.push(format!("path bar: expected root/docs, got {}", fm.path_text()));
    }
    let sub = fm.entries();
    if sub[0] != "intro.doc" || sub[1] != "notes.txt" {
        failures.push(format!("docs listing: expected intro.doc/notes.txt, got {sub:?}"));
    }

    // Open a file (notes.txt, row 1) -> status set.
    fm.click_entry(1);
    if fm.status != "selected notes.txt" {
        failures.push(format!("open file: expected 'selected notes.txt', got '{}'", fm.status));
    }

    // Up returns to root.
    fm.up();
    if fm.path_text() != "root" {
        failures.push(format!("up: expected root, got {}", fm.path_text()));
    }
    if fm.entries()[..4] != expected_root {
        failures.push("up: root listing not restored".into());
    }

    // Up at root stays at root and reports it.
    fm.up();
    if fm.path_text() != "root" {
        failures.push(format!("up at root: path moved to {}", fm.path_text()));
    }
    if fm.status != "at root" {
        failures.push(format!("up at root: expected 'at root', got '{}'", fm.status));
    }

    // Open nonexistent (empty entry) is a no-op.
    let before = (fm.path_text(), fm.status.clone());
    fm.click_entry(6); // row 6 is empty padding at root
    if (fm.path_text(), fm.status.clone()) != before {
        failures.push("open empty entry: state changed (expected no-op)".into());
    }

    if failures.is_empty() {
        println!("files: ALL CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("FILES FAILURE: {f}");
        }
        std::process::exit(1);
    }
}
