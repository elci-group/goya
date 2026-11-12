//! goya-messenger — a local two-person thread exemplar.
//!
//! The UI is 100% declared in `apps/messenger.gscene` (GY-LANG/1
//! RUGID-style syntax). This host is the behavioural half: an in-memory
//! room between "me" and "you", eight visible rows, and add/cut/read.
//! Nothing here dials a network or opens a window unless `--live`.
//!
//!   cargo run -p goya-hello --bin messenger --offline
//!   cargo run -p goya-hello --bin messenger -- --live
//!
//! Without `--live` (and whenever `--offline` is present), a scripted
//! acceptance sequence runs headless and prints one PASS line.

use goya_lang::{hit_test, layout, SceneStore};
use goya_runtime::Primitive;

const SCENE_PATH: &str = "apps/messenger.gscene";
const EXTENT_W_PT: f64 = 100.0;
const EXTENT_H_PT: f64 = 160.0;
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 640;
const MAX_LINES: usize = 8;
/// A line body longer than this is rejected. The "you " / "me " prefix
/// is display-only and is not part of the count.
const MAX_CHARS: usize = 40;
const OPENING: &str = "hello";

const CONTROLS: &[&str] = &[
    "window", "room", "me", "you", "note", "line", "add", "cut", "read", "line_0", "line_1",
    "line_2", "line_3", "line_4", "line_5", "line_6", "line_7",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Who {
    Me,
    You,
}

impl Who {
    fn label(self) -> &'static str {
        match self {
            Who::Me => "me",
            Who::You => "you",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    who: Who,
    text: String,
}

/// In-memory room. `lines[0]` is the oldest kept line.
struct Room {
    lines: Vec<Line>,
    draft: String,
    note: String,
}

impl Room {
    fn new() -> Self {
        Room {
            lines: vec![Line {
                who: Who::Me,
                text: OPENING.into(),
            }],
            draft: String::new(),
            note: "ready".into(),
        }
    }

    /// Eight row labels, oldest first, padded with empty rows.
    fn rows(&self) -> Vec<String> {
        let mut rows: Vec<String> = self
            .lines
            .iter()
            .map(|line| format!("{} {}", line.who.label(), line.text))
            .collect();
        rows.truncate(MAX_LINES);
        rows.resize(MAX_LINES, String::new());
        rows
    }

    fn set_line(&mut self, text: &str) {
        self.draft = text.to_string();
    }

    /// Append one glyph-safe character. Past 40 is allowed so add can
    /// reject it; a hard stop keeps the draft bounded.
    fn type_char(&mut self, c: char) -> bool {
        if !glyph_ok(c) || self.draft.chars().count() >= MAX_CHARS + 24 {
            return false;
        }
        self.draft.push(c);
        true
    }

    /// Drop the last draft character. Empty draft: no change.
    fn cut(&mut self) -> bool {
        if self.draft.pop().is_none() {
            return false;
        }
        self.note = "cut".into();
        true
    }

    /// Append the draft as a line from "you".
    /// Empty draft: thread unchanged.
    /// Longer than 40 characters: rejected, thread unchanged.
    /// A ninth kept line drops the oldest.
    fn add(&mut self) -> bool {
        if self.draft.is_empty() {
            return false;
        }
        if self.draft.chars().count() > MAX_CHARS {
            self.note = "too long".into();
            return false;
        }
        if self.lines.len() >= MAX_LINES {
            self.lines.remove(0);
        }
        let text = std::mem::take(&mut self.draft);
        self.lines.push(Line {
            who: Who::You,
            text,
        });
        self.note = "added".into();
        true
    }

    fn mark_read(&mut self) {
        self.note = format!("read {}", self.lines.len());
    }

    fn press(&mut self, name: &str) {
        match name {
            "add" => {
                self.add();
            }
            "cut" => {
                self.cut();
            }
            "read" => self.mark_read(),
            _ => {}
        }
    }
}

fn glyph_ok(c: char) -> bool {
    matches!(
        c,
        '0'..='9'
            | 'a'
            | 'c'
            | 'd'
            | 'e'
            | 'g'
            | 'h'
            | 'i'
            | 'l'
            | 'm'
            | 'n'
            | 'o'
            | 'r'
            | 't'
            | 'u'
            | 'v'
            | 'y'
            | '+'
            | '-'
            | '/'
            | '*'
            | '%'
            | '.'
            | ','
            | '='
            | '!'
            | ' '
    )
}

/// Host-side data binding: the scene declares structure, the host supplies
/// the thread rows, the draft, and the note.
fn with_state(mut nodes: Vec<goya_lang::DrawNode>, room: &Room) -> Vec<goya_lang::DrawNode> {
    let rows = room.rows();
    for n in &mut nodes {
        match n.name.as_str() {
            "line" => n.text = Some(room.draft.clone()),
            "note" => n.text = Some(room.note.clone()),
            name => {
                if let Some(rest) = name.strip_prefix("line_") {
                    if let Ok(i) = rest.parse::<usize>() {
                        if i < MAX_LINES {
                            n.text = Some(rows[i].clone());
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
                let label = if label.is_empty() {
                    n.name.clone()
                } else {
                    label
                };
                Primitive::Button(rect, label)
            } else if n.text.is_some() {
                Primitive::Text(rect, label)
            } else {
                Primitive::Panel(rect)
            }
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|a| a == "--live");
    let offline = args.iter().any(|a| a == "--offline");
    let scene_path = args
        .iter()
        .position(|a| a == "--file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or(SCENE_PATH);

    let source = std::fs::read_to_string(scene_path).unwrap_or_else(|e| {
        eprintln!("goya-messenger: cannot read {scene_path}: {e}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-messenger: {e}");
            std::process::exit(1);
        }
    };

    // `--offline` is headless even if `--live` is also present.
    // Cargo's own `--offline` is not forwarded; no-arg is the same path.
    if offline || !live {
        self_test(&mut store);
        return;
    }

    let mut win = match goya_wayland::GoyaWindow::create("room", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-messenger: live unavailable: {e}");
            return;
        }
    };
    let mut room = Room::new();
    let mut dirty = true;
    while !win.close_requested() {
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT);
                    if let Some(name) = hit_test(
                        &frame,
                        x as i64 * goya_wayland::MP_PER_PX,
                        y as i64 * goya_wayland::MP_PER_PX,
                    ) {
                        if name == "add" || name == "cut" || name == "read" {
                            room.press(&name);
                            dirty = true;
                        }
                    }
                }
                goya_wayland::PlatformEvent::KeyChar(c) => {
                    if c == '\n' || c == '\r' {
                        room.add();
                        dirty = true;
                    } else if c == '\u{8}' {
                        room.cut();
                        dirty = true;
                    } else if room.type_char(c) {
                        dirty = true;
                    }
                }
                goya_wayland::PlatformEvent::Resized(w, h) => {
                    println!("goya-messenger: resized {w}x{h}");
                    dirty = true;
                }
                _ => {}
            }
        }
        if dirty {
            let nodes = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT).nodes;
            let nodes = with_state(nodes, &room);
            let prims = present_nodes(&nodes);
            let fills: Vec<Option<u32>> = nodes.iter().map(|n| n.fill).collect();
            let _ = win.present(&prims, &fills, &[], [235, 235, 240], [20, 20, 28], 3);
            dirty = false;
        }
        win.pump(1);
    }
    println!(
        "goya-messenger live: {} lines, note '{}', generation {}",
        room.lines.len(),
        room.note,
        store.generation()
    );
}

fn bind(store: &SceneStore, room: &Room) -> Vec<goya_lang::DrawNode> {
    with_state(layout(store.get(), EXTENT_W_PT, EXTENT_H_PT).nodes, room)
}

fn row(nodes: &[goya_lang::DrawNode], name: &str) -> String {
    nodes
        .iter()
        .find(|n| n.name == name)
        .and_then(|n| n.text.clone())
        .unwrap_or_default()
}

fn check_glyphs(nodes: &[goya_lang::DrawNode], failures: &mut Vec<String>, label: &str) {
    for n in nodes {
        if let Some(text) = &n.text {
            if let Some(c) = text.chars().find(|c| !glyph_ok(*c)) {
                failures.push(format!("{label}: {} has glyph {c:?}", n.name));
            }
        }
    }
}

/// Headless acceptance. Each spec outcome is asserted, and the named
/// scene controls are required to exist on the laid-out frame.
fn self_test(store: &mut SceneStore) {
    let mut failures = Vec::new();
    let frame = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT);

    for n in CONTROLS {
        if frame.find(n).is_none() {
            failures.push(format!("missing cell/button {n}"));
        }
    }
    for n in ["add", "cut", "read"] {
        if let Some(b) = frame.find(n) {
            if !b.is_button {
                failures.push(format!("{n} is not a button"));
            }
            let hit = hit_test(
                &frame,
                b.rect.x + b.rect.width / 2,
                b.rect.y + b.rect.height / 2,
            );
            if hit.as_deref() != Some(n) {
                failures.push(format!("{n} not hit-testable"));
            }
        }
    }
    for n in ["room", "me", "you", "line", "line_0"] {
        if frame.find(n).is_some_and(|node| node.is_button) {
            failures.push(format!("{n} should be a cell"));
        }
    }
    if let Some(w) = frame.find("window") {
        if w.rect.width != 100_000 || w.rect.height != 160_000 {
            failures.push(format!(
                "window extent: expected 100000x160000, got {}x{}",
                w.rect.width, w.rect.height
            ));
        }
    }
    check_glyphs(&frame.nodes, &mut failures, "scene");

    // The thread starts with one line from "me", shown in row 0.
    let room = Room::new();
    if room.lines.len() != 1 || room.lines[0].who != Who::Me || room.lines[0].text != OPENING {
        failures.push(format!(
            "opening: expected one me line, got {:?}",
            room.lines
        ));
    }
    let nodes = bind(store, &room);
    if row(&nodes, "line_0") != "me hello" {
        failures.push(format!("opening show: got '{}'", row(&nodes, "line_0")));
    }
    for i in 1..MAX_LINES {
        let name = format!("line_{i}");
        if !row(&nodes, &name).is_empty() {
            failures.push(format!("opening row {name} not empty"));
        }
    }
    if row(&nodes, "me") != "me" || row(&nodes, "you") != "you" || row(&nodes, "room") != "room" {
        failures.push("room/me/you labels missing after layout".into());
    }
    check_glyphs(&nodes, &mut failures, "opening");

    // A non-empty line from "you" is appended and shown.
    let mut room = Room::new();
    room.set_line("hi");
    if !room.add() {
        failures.push("you add: rejected".into());
    }
    if room.lines.len() != 2
        || room.lines[0].who != Who::Me
        || room.lines[1].who != Who::You
        || room.lines[1].text != "hi"
    {
        failures.push(format!("you add: thread is {:?}", room.lines));
    }
    let nodes = bind(store, &room);
    if row(&nodes, "line_0") != "me hello" || row(&nodes, "line_1") != "you hi" {
        failures.push(format!(
            "you add show: '{}' / '{}'",
            row(&nodes, "line_0"),
            row(&nodes, "line_1")
        ));
    }
    if !row(&nodes, "line").is_empty() {
        failures.push("you add: draft still shown".into());
    }
    check_glyphs(&nodes, &mut failures, "you add");

    // An empty line does not change the thread.
    let mut room = Room::new();
    let before = room.lines.clone();
    let before_note = room.note.clone();
    if room.add() || room.lines != before || room.note != before_note {
        failures.push("empty add: thread changed".into());
    }
    let nodes = bind(store, &room);
    if row(&nodes, "line_0") != "me hello" || !row(&nodes, "line_1").is_empty() {
        failures.push("empty add: shown thread changed".into());
    }
    room.set_line("hi");
    room.add();
    let after = room.lines.clone();
    room.set_line("");
    if room.add() || room.lines != after {
        failures.push("empty add after a line: thread changed".into());
    }

    // Longer than 40 characters is rejected. 40 is kept.
    let mut room = Room::new();
    let long = "a".repeat(MAX_CHARS + 1);
    room.set_line(&long);
    let before = room.lines.clone();
    if room.add() || room.lines != before {
        failures.push("41-char line was not rejected".into());
    }
    if room.draft != long || room.note != "too long" {
        failures.push(format!(
            "41-char reject state: note '{}', draft len {}",
            room.note,
            room.draft.chars().count()
        ));
    }
    let nodes = bind(store, &room);
    for i in 0..MAX_LINES {
        let name = format!("line_{i}");
        if row(&nodes, &name).contains(&long) {
            failures.push("rejected line was shown in the thread".into());
        }
    }
    if row(&nodes, "line_0") != "me hello" || !row(&nodes, "line_1").is_empty() {
        failures.push("41-char reject changed the shown thread".into());
    }
    if row(&nodes, "note") != "too long" {
        failures.push(format!("reject note: got '{}'", row(&nodes, "note")));
    }
    if !room.cut() || room.draft.chars().count() != MAX_CHARS {
        failures.push("cut did not bring the line to 40".into());
    }
    if !room.add() || room.lines.len() != 2 || room.lines[1].text.chars().count() != MAX_CHARS {
        failures.push("40-char line was not kept".into());
    }
    let nodes = bind(store, &room);
    let expect = format!("you {}", "a".repeat(MAX_CHARS));
    if row(&nodes, "line_1") != expect {
        failures.push(format!("40-char show: got '{}'", row(&nodes, "line_1")));
    }
    check_glyphs(&nodes, &mut failures, "length");

    // At most 8 lines. The opening me-line is line 1. Adds n1..n7 fill
    // lines 2..8. The add of n8 is the ninth line and drops the oldest.
    let mut room = Room::new();
    for i in 1..=7 {
        room.set_line(&format!("n{i}"));
        if !room.add() {
            failures.push(format!("fill add {i} rejected"));
        }
        if room.lines.len() > MAX_LINES {
            failures.push(format!("room grew past 8 on add {i}"));
        }
    }
    if room.lines.len() != MAX_LINES
        || room.lines[0].text != OPENING
        || room.lines[0].who != Who::Me
    {
        failures.push(format!(
            "full room: expected 8 starting with me hello, got {:?}",
            room.lines
        ));
    }
    let oldest = room.lines[0].text.clone();
    room.set_line("n8");
    if !room.add() {
        failures.push("ninth add rejected".into());
    }
    if room.lines.len() != MAX_LINES {
        failures.push(format!("ninth add kept {} lines", room.lines.len()));
    }
    if room.lines.iter().any(|line| line.text == oldest) {
        failures.push("ninth add did not drop the oldest".into());
    }
    if room.lines.first().map(|line| line.text.as_str()) != Some("n1")
        || room.lines.first().map(|line| line.who) != Some(Who::You)
        || room.lines.last().map(|line| line.text.as_str()) != Some("n8")
    {
        failures.push(format!("ninth add window: {:?}", room.lines));
    }
    let nodes = bind(store, &room);
    if row(&nodes, "line_0") != "you n1" || row(&nodes, "line_7") != "you n8" {
        failures.push(format!(
            "ninth add show: '{}' .. '{}'",
            row(&nodes, "line_0"),
            row(&nodes, "line_7")
        ));
    }
    if (0..MAX_LINES).any(|i| row(&nodes, &format!("line_{i}")).contains(OPENING)) {
        failures.push("dropped line still shown".into());
    }
    check_glyphs(&nodes, &mut failures, "ninth");

    // Button names drive the same state machine.
    let mut room = Room::new();
    room.set_line("yo");
    room.press("add");
    if room.lines.len() != 2 || room.lines[1].text != "yo" || room.lines[1].who != Who::You {
        failures.push(format!("press add: {:?}", room.lines));
    }
    room.set_line("ac");
    room.press("cut");
    if room.draft != "a" {
        failures.push(format!("press cut: draft '{}'", room.draft));
    }
    room.press("read");
    if room.note != "read 2" {
        failures.push(format!("press read: note '{}'", room.note));
    }
    let nodes = bind(store, &room);
    if row(&nodes, "add") != "add" || row(&nodes, "cut") != "cut" || row(&nodes, "read") != "read" {
        failures.push("add/cut/read labels missing after layout".into());
    }

    if failures.is_empty() {
        println!("goya-messenger: ALL EXEMPLAR CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("MESSENGER FAILURE: {f}");
        }
        std::process::exit(1);
    }
}
