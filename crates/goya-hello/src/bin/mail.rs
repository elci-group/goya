//! goya-mail — the MAIL exemplar Goya application.
//!
//! The UI is 100% declared in `apps/mail.gscene` (GY-LANG/1
//! RUGID-style syntax). This host is the behavioural half: it keeps a
//! deterministic IN-MEMORY mailbox (no sockets, no SMTP, no files),
//! overrides the `in_0..in_3`, `read_text`, `note_text` and `status_text`
//! cells with live state, and routes read/cut/deliver through hit-test.
//! Choosing an inbox row shows that message body.
//!
//!   cargo run -p goya-hello --bin mail --offline
//!   cargo run -p goya-hello --bin mail -- --live
//!
//! Without `--live`, a scripted acceptance sequence runs headless and
//! prints PASS/FAIL (two seed messages, choose shows the body, deliver
//! appends a note and selects it, empty deliver is a no-op, buttons exist).

use goya_lang::{hit_test, layout, SceneStore};
use goya_runtime::Primitive;

const SCENE_PATH: &str = "apps/mail.gscene";
const EXTENT_W_PT: f64 = 100.0;
const EXTENT_H_PT: f64 = 150.0;
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 600;
const ENTRY_COUNT: usize = 4;
/// Inbox rows start at pt y = 24, each 12 pt tall (11 pt cell + 1 pt gap).
const ENTRY_Y0_PT: i64 = 24;
const ENTRY_PITCH_PT: i64 = 12;
/// Note field cap so a held key cannot grow the line without bound.
const NOTE_LIMIT: usize = 48;

const LINE_YOU: &str = "you hello";
const BODY_YOU: &str = "hello there";
const LINE_ME: &str = "me hi";
const BODY_ME: &str = "i am in";
const NOTE_DELIVERED: &str = "call me later";

// ---- In-memory mailbox ------------------------------------------------

struct Message {
    line: String,
    body: String,
}

struct Mail {
    inbox: Vec<Message>,
    /// Chosen row; `None` until a message is chosen or delivered.
    selected: Option<usize>,
    /// Body currently shown in `read_text`.
    shown: String,
    /// Note being written. Delivered only when non-empty.
    note: String,
    status: String,
}

impl Mail {
    fn new() -> Self {
        Mail {
            inbox: vec![
                Message {
                    line: LINE_YOU.into(),
                    body: BODY_YOU.into(),
                },
                Message {
                    line: LINE_ME.into(),
                    body: BODY_ME.into(),
                },
            ],
            selected: None,
            shown: String::new(),
            note: String::new(),
            status: "mail".into(),
        }
    }

    /// Four inbox rows: a `=` marks the chosen line, a `-` marks the rest.
    fn rows(&self) -> Vec<String> {
        let mut rows = Vec::with_capacity(ENTRY_COUNT);
        for i in 0..ENTRY_COUNT {
            rows.push(match self.inbox.get(i) {
                Some(msg) if self.selected == Some(i) => format!("= {}", msg.line),
                Some(msg) => format!("- {}", msg.line),
                None => String::new(),
            });
        }
        rows
    }

    /// Choose inbox row `idx` and show its body. Out of range is a no-op.
    fn choose(&mut self, idx: usize) {
        let Some(msg) = self.inbox.get(idx) else {
            return;
        };
        self.shown = msg.body.clone();
        self.selected = Some(idx);
        self.status = "read".into();
    }

    /// Replace the note being written.
    fn write(&mut self, text: &str) {
        self.note = text.to_string();
        self.status = "note".into();
    }

    /// Append one glyph-safe character. Anything outside the 5x7 set is ignored.
    fn type_char(&mut self, c: char) {
        if self.note.chars().count() >= NOTE_LIMIT || !glyph_char(c) {
            return;
        }
        self.note.push(c);
        self.status = "note".into();
    }

    fn backspace(&mut self) {
        self.note.pop();
        self.status = "note".into();
    }

    fn cut(&mut self) {
        self.note.clear();
        self.status = "cut".into();
    }

    /// Append a non-empty note to the inbox and select it. Empty is a no-op
    /// other than the status line.
    fn deliver(&mut self) {
        if self.note.is_empty() {
            self.status = "no note".into();
            return;
        }
        let text = std::mem::take(&mut self.note);
        self.inbox.push(Message {
            line: text.clone(),
            body: text.clone(),
        });
        let idx = self.inbox.len() - 1;
        self.selected = Some(idx);
        self.shown = text;
        self.status = "in".into();
    }

    /// Toolbar press by scene name.
    fn press(&mut self, name: &str) {
        match name {
            "btn_read" => {
                let idx = self.selected.unwrap_or(0);
                self.choose(idx);
            }
            "btn_cut" => self.cut(),
            "btn_deliver" => self.deliver(),
            _ => {}
        }
    }
}

fn glyph_char(c: char) -> bool {
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

fn glyph_safe(s: &str) -> bool {
    s.chars().all(glyph_char)
}

// ---- Scene plumbing ---------------------------------------------------

/// Host-side data binding: override inbox rows, the open body, the note
/// and the status line (the scene declares structure, the host supplies it).
fn with_state(mut nodes: Vec<goya_lang::DrawNode>, mail: &Mail) -> Vec<goya_lang::DrawNode> {
    let rows = mail.rows();
    for n in &mut nodes {
        match n.name.as_str() {
            "read_text" => n.text = Some(mail.shown.clone()),
            "note_text" => n.text = Some(mail.note.clone()),
            "status_text" => n.text = Some(mail.status.clone()),
            name => {
                if let Some(rest) = name.strip_prefix("in_") {
                    if let Ok(i) = rest.parse::<usize>() {
                        if i < ENTRY_COUNT {
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

/// Which inbox row (if any) holds the given window point? Points are
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
        eprintln!("goya-mail: cannot read {scene_path}: {e}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-mail: {e}");
            std::process::exit(1);
        }
    };

    if !live {
        self_test(&mut store);
        return;
    }

    let mut win = match goya_wayland::GoyaWindow::create("goya-mail", WIN_W_PX, WIN_H_PX) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-mail: live unavailable: {e}");
            return;
        }
    };
    let mut mail = Mail::new();
    let mut dirty = true; // first frame always paints
    while !win.close_requested() {
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    let frame = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT);
                    if let Some(i) = entry_at(x as i32, y as i32) {
                        mail.choose(i);
                        dirty = true;
                    } else if let Some(name) = hit_test(
                        &frame,
                        x as i64 * goya_wayland::MP_PER_PX,
                        y as i64 * goya_wayland::MP_PER_PX,
                    ) {
                        if name == "btn_read" || name == "btn_cut" || name == "btn_deliver" {
                            mail.press(&name);
                            dirty = true;
                        }
                    }
                }
                goya_wayland::PlatformEvent::KeyChar(c) => {
                    if c == '\n' {
                        mail.deliver();
                    } else if c == '\u{8}' {
                        mail.backspace();
                    } else {
                        mail.type_char(c);
                    }
                    dirty = true;
                }
                goya_wayland::PlatformEvent::Resized(w, h) => {
                    println!("goya-mail: resized {w}x{h}");
                    dirty = true;
                }
                _ => {}
            }
        }
        if dirty {
            let nodes = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT).nodes;
            let nodes = with_state(nodes, &mail);
            let prims = present_nodes(&nodes);
            let fills: Vec<Option<u32>> = nodes.iter().map(|n| n.fill).collect();
            let _ = win.present(&prims, &fills, &[], [235, 235, 240], [20, 20, 28], 3);
            dirty = false;
        }
        win.pump(1);
    }
    println!(
        "goya-mail live: {} in, shown '{}', note '{}', generation {}",
        mail.inbox.len(),
        mail.shown,
        mail.note,
        store.generation()
    );
}

fn node_text(nodes: &[goya_lang::DrawNode], name: &str) -> String {
    nodes
        .iter()
        .find(|n| n.name == name)
        .and_then(|n| n.text.clone())
        .unwrap_or_default()
}

fn audit_glyphs(nodes: &[goya_lang::DrawNode], tag: &str, failures: &mut Vec<String>) {
    for n in nodes {
        if let Some(text) = &n.text {
            if !glyph_safe(text) {
                failures.push(format!("{tag}: {} text not glyph safe: {text}", n.name));
            }
        }
    }
}

/// Headless acceptance: drive the mailbox exactly as clicks would,
/// verify the four mail outcomes, and sanity-check scene structure.
fn self_test(store: &mut SceneStore) {
    let mut failures = Vec::new();
    let mut mail = Mail::new();
    let frame = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT);

    // Named scene buttons exist after layout and are hit-testable.
    for (name, label) in [
        ("btn_read", "read"),
        ("btn_cut", "cut"),
        ("btn_deliver", "deliver"),
    ] {
        match frame.find(name) {
            Some(b) if b.is_button && b.text.as_deref() == Some(label) => {
                let hit = hit_test(&frame, b.rect.x + 1_000, b.rect.y + 1_000);
                if hit.as_deref() != Some(name) {
                    failures.push(format!("{name} not hit-testable"));
                }
            }
            Some(_) => failures.push(format!("{name} is not a {label} button")),
            None => failures.push(format!("missing button {name}")),
        }
    }
    for n in [
        "title",
        "in_label",
        "in_0",
        "in_1",
        "in_2",
        "in_3",
        "note_label",
        "note_text",
        "out_label",
        "read_text",
        "status_text",
    ] {
        if frame.find(n).is_none() {
            failures.push(format!("missing cell {n}"));
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
    if let Some(row) = frame.find("in_0") {
        if row.rect.y != ENTRY_Y0_PT * 1_000 {
            failures.push(format!(
                "in_0 origin: expected {}, got {}",
                ENTRY_Y0_PT * 1_000,
                row.rect.y
            ));
        }
    }
    if entry_at(40, (ENTRY_Y0_PT as i32) * 4 + 2) != Some(0)
        || entry_at(40, ((ENTRY_Y0_PT + ENTRY_PITCH_PT) as i32) * 4 + 2) != Some(1)
    {
        failures.push("inbox row hit bands do not match the scene".into());
    }
    audit_glyphs(&frame.nodes, "scene", &mut failures);

    // Outcome 1: two glyph-safe messages are present at start.
    if mail.inbox.len() != 2 {
        failures.push(format!(
            "start: expected 2 messages, got {}",
            mail.inbox.len()
        ));
    }
    if mail.inbox.first().map(|m| m.line.as_str()) != Some(LINE_YOU)
        || mail.inbox.first().map(|m| m.body.as_str()) != Some(BODY_YOU)
        || mail.inbox.get(1).map(|m| m.line.as_str()) != Some(LINE_ME)
        || mail.inbox.get(1).map(|m| m.body.as_str()) != Some(BODY_ME)
    {
        failures.push("start: seed messages differ".into());
    }
    if !mail.shown.is_empty() {
        failures.push(format!("start: body already shown '{}'", mail.shown));
    }
    let start_nodes = with_state(frame.nodes.clone(), &mail);
    audit_glyphs(&start_nodes, "start", &mut failures);
    if node_text(&start_nodes, "in_0") != format!("- {LINE_YOU}")
        || node_text(&start_nodes, "in_1") != format!("- {LINE_ME}")
        || !node_text(&start_nodes, "in_2").is_empty()
        || !node_text(&start_nodes, "in_3").is_empty()
        || !node_text(&start_nodes, "read_text").is_empty()
    {
        failures.push(format!(
            "start rows: {:?} / {:?} / read '{}'",
            node_text(&start_nodes, "in_0"),
            node_text(&start_nodes, "in_1"),
            node_text(&start_nodes, "read_text")
        ));
    }

    // Outcome 4 (early): delivering an empty note does not add a message.
    let before = mail.inbox.len();
    mail.press("btn_deliver");
    if mail.inbox.len() != before || mail.selected.is_some() {
        failures.push("empty deliver: inbox changed".into());
    }
    if mail.status != "no note" {
        failures.push(format!(
            "empty deliver: expected 'no note', got '{}'",
            mail.status
        ));
    }

    // Outcome 2: choosing a message shows its body.
    mail.choose(1);
    if mail.shown != BODY_ME || mail.selected != Some(1) {
        failures.push(format!(
            "choose me: shown '{}', selected {:?}",
            mail.shown, mail.selected
        ));
    }
    mail.choose(0);
    if mail.shown != BODY_YOU || mail.selected != Some(0) {
        failures.push(format!(
            "choose you: shown '{}', selected {:?}",
            mail.shown, mail.selected
        ));
    }
    // Row click is the same choose path the live window uses.
    if entry_at(40, ((ENTRY_Y0_PT + ENTRY_PITCH_PT) as i32) * 4 + 2) == Some(1) {
        mail.choose(1);
    }
    if mail.shown != BODY_ME {
        failures.push(format!(
            "row choose: expected '{BODY_ME}', got '{}'",
            mail.shown
        ));
    }
    let chosen = with_state(frame.nodes.clone(), &mail);
    audit_glyphs(&chosen, "choose", &mut failures);
    if node_text(&chosen, "read_text") != BODY_ME
        || node_text(&chosen, "in_1") != format!("= {LINE_ME}")
    {
        failures.push(format!(
            "choose binding: read '{}', row '{}'",
            node_text(&chosen, "read_text"),
            node_text(&chosen, "in_1")
        ));
    }
    // Empty padding row does not change the open body.
    let shown_before = mail.shown.clone();
    mail.choose(3);
    if mail.shown != shown_before || mail.inbox.len() != 2 {
        failures.push("choose empty row: state changed".into());
    }

    // Outcome 3: writing a note and delivering it appends that note and selects it.
    mail.write(NOTE_DELIVERED);
    if mail.note != NOTE_DELIVERED {
        failures.push(format!(
            "write: expected '{NOTE_DELIVERED}', got '{}'",
            mail.note
        ));
    }
    mail.press("btn_deliver");
    if mail.inbox.len() != 3 {
        failures.push(format!(
            "deliver: expected 3 messages, got {}",
            mail.inbox.len()
        ));
    }
    if mail.inbox.last().map(|m| m.body.as_str()) != Some(NOTE_DELIVERED)
        || mail.inbox.last().map(|m| m.line.as_str()) != Some(NOTE_DELIVERED)
    {
        failures.push("deliver: appended note mismatch".into());
    }
    if mail.selected != Some(2) || mail.shown != NOTE_DELIVERED {
        failures.push(format!(
            "deliver select: shown '{}', selected {:?}",
            mail.shown, mail.selected
        ));
    }
    if !mail.note.is_empty() {
        failures.push(format!("deliver: note not cleared, '{}'", mail.note));
    }
    let delivered = with_state(frame.nodes.clone(), &mail);
    audit_glyphs(&delivered, "deliver", &mut failures);
    if node_text(&delivered, "read_text") != NOTE_DELIVERED
        || node_text(&delivered, "in_2") != format!("= {NOTE_DELIVERED}")
        || node_text(&delivered, "note_text") != ""
        || node_text(&delivered, "in_0") != format!("- {LINE_YOU}")
    {
        failures.push(format!(
            "deliver binding: read '{}', row '{}', note '{}'",
            node_text(&delivered, "read_text"),
            node_text(&delivered, "in_2"),
            node_text(&delivered, "note_text")
        ));
    }
    // The two seed messages are still there, unchanged.
    if mail.inbox[0].body != BODY_YOU || mail.inbox[1].body != BODY_ME {
        failures.push("deliver: seed messages were rewritten".into());
    }

    // Outcome 4: delivering an empty note does not add a message.
    mail.press("btn_deliver");
    if mail.inbox.len() != 3 || mail.selected != Some(2) || mail.shown != NOTE_DELIVERED {
        failures.push(format!(
            "empty deliver after send: len {}, selected {:?}, shown '{}'",
            mail.inbox.len(),
            mail.selected,
            mail.shown
        ));
    }
    // cut clears a written note, so a following deliver still adds nothing.
    mail.write("meet me");
    mail.press("btn_cut");
    if !mail.note.is_empty() || mail.status != "cut" {
        failures.push(format!(
            "cut: note '{}', status '{}'",
            mail.note, mail.status
        ));
    }
    mail.press("btn_deliver");
    if mail.inbox.len() != 3 {
        failures.push(format!(
            "deliver after cut: expected 3 messages, got {}",
            mail.inbox.len()
        ));
    }

    if failures.is_empty() {
        println!("goya-mail: ALL EXEMPLAR CHECKS PASS");
    } else {
        for f in &failures {
            eprintln!("MAIL FAILURE: {f}");
        }
        std::process::exit(1);
    }
}
