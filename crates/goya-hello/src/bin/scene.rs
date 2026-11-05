//! goya-scene — build a Goya GUI from a RUGID-style scene file.
//!
//! `cargo run --bin scene [--file app.gscene] [--live]`
//!
//! The scene language (GY-LANG/1) adopts RUGID's grammar: `@ontology`,
//! `@cell`, `@button`, `position`/`extent` (absolute points) and
//! `position_p`/`extent_p` (proportional). Buttons are interactive: clicks
//! print their name. Without `--live`, programmatic checks run headless.

use goya_lang::{hit_test_scrolled, layout, Anchor, DrawNode, RectMp, ScrollOffsets};
use goya_runtime::{Primitive, TextBuffer};
use std::collections::BTreeMap;

const DEFAULT_SCENE: &str = "\
@ontology goya_workspace {
    @cell window {
        position: (0, 0, 0)
        extent: (400, 150, 0)
        fill: #14141c

        @cell title {
            position: (200, 24, 0)
            text: \"Goya Workspace\"
            text-size: 16
            fill: #eaeaef
            text-anchor: middle
        }

        // Three actions, RUGID style — absolute geometry per cell.
        @button open {
            position: (30, 70, 0)
            extent: (100, 40, 0)
            fill: #3267c8
            text: \"Open\"
        }
        @button save {
            position: (150, 70, 0)
            extent: (100, 40, 0)
            fill: #2a8a4a
            text: \"Save\"
        }
        @button quit {
            position: (270, 70, 0)
            extent: (100, 40, 0)
            fill: #8a2a2a
            text: \"Quit\"
        }

        @cell statusbar {
            position: (0, 128, 0)
            extent: (400, 22, 0)
            fill: #1e1e28
            @cell status {
                position_p: (0.5, 0.5, 0)
                text: \"ready\"
                text-size: 9
                fill: #9aa0b0
                text-anchor: middle
            }
        }
    }
}
";

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
    let watch = args.iter().any(|a| a == "--watch");
    let max_frames: u32 = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800);
    let sound = args.iter().any(|a| a == "--sound");
    let path = args
        .iter()
        .position(|a| a == "--file")
        .and_then(|i| args.get(i + 1));

    let source = match path {
        Some(p) => std::fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("goya-scene: cannot read {p}: {e}");
            std::process::exit(1);
        }),
        None => DEFAULT_SCENE.to_string(),
    };

    let mut store = match goya_lang::SceneStore::new(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("goya-scene: {e}");
            std::process::exit(1);
        }
    };
    let mut last_mtime = path
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| modified(&m));
    println!(
        "goya-scene: ontology '{}' parsed ({} nodes drawn at 400x150)",
        store.get().name,
        layout(store.get(), 400.0, 150.0).nodes.len()
    );

    if !live {
        // Headless verification: proportional child centred in statusbar.
        let frame = layout(store.get(), 400.0, 150.0);
        let (status, statusbar) = match (frame.find("status"), frame.find("statusbar")) {
            (Some(s), Some(b)) => (s, b),
            _ => {
                eprintln!("goya-scene: scene is missing status/statusbar cells");
                std::process::exit(1);
            }
        };
        // position_p 0.5 places the cell's ORIGIN at the parent midpoint;
        // with anchor=middle the text centres there. The origin check is
        // against parent.x + 0.5*parent.width.
        let expected_x = statusbar.rect.x + statusbar.rect.width / 2;
        let ok = (status.rect.x - expected_x).abs() < 5_000;
        println!(
            "proportional centring of status cell [{}]",
            if ok { "PASS" } else { "FAIL" }
        );
        let hit = hit_test_scrolled(&frame, &ScrollOffsets::default(), 80_000, 90_000);
        println!(
            "hit test on 'Open' button [{}]",
            if hit.as_deref() == Some("open") {
                "PASS"
            } else {
                "FAIL"
            }
        );
        if !(ok && hit.as_deref() == Some("open")) {
            std::process::exit(1);
        }
        return;
    }

    let mut win = match goya_wayland::GoyaWindow::create("goya-scene", 400, 150) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("goya-scene: live unavailable: {e}");
            return;
        }
    };
    let mut frames = 0u32;
    let mut clicked = std::collections::BTreeMap::new();
    let mut scroll_offsets = ScrollOffsets::default();
    let mut inputs = SceneInputs::default();
    let mut pointer = None;
    // Interaction state (click counts, keyed by button NAME) survives
    // hot-swaps: the transactional store swaps only the scene, never the
    // application's state. A swap that fails to parse leaves the running
    // scene untouched (transactional hot-swap, GY-LANG/1).
    let mut audio = if sound {
        goya_audio_linux::PwCatOutput::spawn()
    } else {
        None
    };
    while frames < max_frames && !win.close_requested() {
        // Hot-swap: poll the scene file; transactional via SceneStore.
        if watch {
            if let Some(p) = path {
                if let Ok(m) = std::fs::metadata(p) {
                    let mtime = modified(&m);
                    if last_mtime != Some(mtime) {
                        last_mtime = Some(mtime);
                        match std::fs::read_to_string(p) {
                            Ok(src) => match store.swap(&src) {
                                Ok(gen) => {
                                    println!("goya-scene: hot-swapped to generation {gen}")
                                }
                                Err(e) => println!(
                                    "goya-scene: swap rejected ({e}) — running scene retained"
                                ),
                            },
                            Err(e) => println!("goya-scene: read failed ({e}) — retained"),
                        }
                    }
                }
            }
        }
        let event_frame = layout(store.get(), 400.0, 150.0);
        inputs.sync_nodes(&event_frame);
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    if let Some(name) = hit_test_scrolled(
                        &event_frame,
                        &scroll_offsets,
                        (x as i64) * goya_wayland::MP_PER_PX,
                        (y as i64) * goya_wayland::MP_PER_PX,
                    ) {
                        if event_frame.find(&name).is_some_and(|node| node.is_input) {
                            inputs.focus(Some(name));
                        } else if event_frame.find(&name).is_some_and(|node| node.is_button) {
                            inputs.focus(None);
                            println!("goya-scene: button '{name}' activated");
                            *clicked.entry(name).or_insert(0u32) += 1;
                            if let Some(out) = audio.as_mut() {
                                let t = goya_audio_dsp::Tap::new(880.0, 48_000.0, 0.12);
                                let mut limiter = goya_audio_dsp::Limiter::new();
                                let buf: Vec<f32> = (0..5_760)
                                    .map(|i| limiter.push(0.5 * t.sample(i as f32 / 48_000.0)))
                                    .collect();
                                let _ = goya_audio_linux::AudioOutput::write(out, &buf);
                            }
                        } else {
                            inputs.focus(None);
                        }
                    } else {
                        inputs.focus(None);
                    }
                }
                goya_wayland::PlatformEvent::PointerMove(x, y) => pointer = Some((x, y)),
                goya_wayland::PlatformEvent::PointerScroll(lines) => {
                    if let Some((x, y)) = pointer {
                        let x = (x as i64) * goya_wayland::MP_PER_PX;
                        let y = (y as i64) * goya_wayland::MP_PER_PX;
                        if let Some(target) = event_frame.scroll_target_at(&scroll_offsets, x, y) {
                            let _ = scroll_offsets.scroll_by(
                                &event_frame,
                                &target,
                                0,
                                -i64::from(lines).saturating_mul(40_000),
                            );
                        }
                    }
                }
                goya_wayland::PlatformEvent::KeyChar(c) => inputs.handle_character(c),
                goya_wayland::PlatformEvent::KeySpecial(key) => {
                    inputs.handle_special(key, &event_frame);
                }
                goya_wayland::PlatformEvent::Preedit(preedit) => {
                    inputs.preedit = preedit.map(|text| {
                        let end = text.len().min(i32::MAX as usize) as i32;
                        (text, 0, end)
                    });
                }
                goya_wayland::PlatformEvent::PreeditWithCursor {
                    text,
                    cursor_begin,
                    cursor_end,
                } => inputs.preedit = text.map(|text| (text, cursor_begin, cursor_end)),
                goya_wayland::PlatformEvent::DeleteSurroundingText {
                    before_bytes,
                    after_bytes,
                } => inputs.delete_surrounding(before_bytes, after_bytes),
                goya_wayland::PlatformEvent::CommitText(text) => inputs.commit(&text),
                goya_wayland::PlatformEvent::Resized(w, h) => {
                    println!("goya-scene: resized to {w}x{h} — relayout on next frame");
                }
                _ => {}
            }
        }
        let frame = layout(store.get(), 400.0, 150.0);
        let mut nodes = frame.scrolled_nodes(&scroll_offsets);
        inputs.sync_nodes(&frame);
        inputs.apply_to_nodes(&mut nodes);
        inputs.update_text_input(&mut win, &nodes);
        let prims: Vec<Primitive> = nodes.iter().map(present_node).collect();
        let clips: Vec<Option<goya_layout::Rect>> = nodes
            .iter()
            .map(|node| {
                node.clip.map(|clip: RectMp| goya_layout::Rect {
                    x: clip.x,
                    y: clip.y,
                    width: clip.width,
                    height: clip.height,
                })
            })
            .collect();
        let text_sizes: Vec<Option<f32>> = nodes
            .iter()
            .map(|node| {
                node.text_size
                    .and_then(|size| (size > 0.0 && size.is_finite()).then_some(size as f32))
            })
            .collect();
        let _ = win.present_clipped_with_text_sizes(
            &prims,
            &[],
            &clips,
            &text_sizes,
            [235, 235, 240],
            [20, 20, 28],
            3,
        );
        win.pump(1);
        // Watch cadence: ~60 fps is pointless for file polling; keep the
        // frame budget long enough to catch edits.
        if watch {
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        frames += 1;
    }
    println!(
        "goya-scene live: {frames} frames, generation {}, clicks: {clicked:?}",
        store.generation()
    );
}

#[derive(Default)]
struct SceneInputs {
    buffers: BTreeMap<String, TextBuffer>,
    focused: Option<String>,
    preedit: Option<(String, i32, i32)>,
}

impl SceneInputs {
    fn sync_nodes(&mut self, frame: &goya_lang::Frame) {
        self.buffers
            .retain(|name, _| frame.find(name).is_some_and(|node| node.is_input));
        for node in &frame.nodes {
            if node.is_input {
                self.buffers.entry(node.name.clone()).or_default();
            }
        }
        if self
            .focused
            .as_ref()
            .is_some_and(|name| !self.buffers.contains_key(name))
        {
            self.focused = None;
            self.preedit = None;
        }
    }

    fn focus(&mut self, name: Option<String>) {
        if self.focused != name {
            self.preedit = None;
            self.focused = name.filter(|name| self.buffers.contains_key(name));
        }
    }

    fn handle_character(&mut self, character: char) {
        let Some(buffer) = self.focused.as_ref().and_then(|name| self.buffers.get_mut(name)) else {
            return;
        };
        match character {
            '\u{8}' => {
                self.preedit = None;
                buffer.delete_backward();
            }
            '\n' | '\r' => {}
            character if !character.is_control() => {
                self.preedit = None;
                buffer.insert(&character.to_string());
            }
            _ => {}
        }
    }

    fn handle_special(&mut self, key: goya_wayland::KeySpecial, frame: &goya_lang::Frame) {
        let Some(name) = self.focused.clone() else {
            return;
        };
        match key {
            goya_wayland::KeySpecial::Escape => self.focus(None),
            goya_wayland::KeySpecial::Tab => {
                let names: Vec<_> = frame
                    .nodes
                    .iter()
                    .filter(|node| node.is_input)
                    .map(|node| node.name.clone())
                    .collect();
                let next = names
                    .iter()
                    .position(|candidate| candidate == &name)
                    .and_then(|index| names.get((index + 1) % names.len()))
                    .cloned();
                self.focus(next);
            }
            goya_wayland::KeySpecial::ArrowLeft
            | goya_wayland::KeySpecial::ArrowRight
            | goya_wayland::KeySpecial::Home
            | goya_wayland::KeySpecial::End
            | goya_wayland::KeySpecial::Delete => {
                self.preedit = None;
                if let Some(buffer) = self.buffers.get_mut(&name) {
                    match key {
                        goya_wayland::KeySpecial::ArrowLeft => buffer.move_left(false),
                        goya_wayland::KeySpecial::ArrowRight => buffer.move_right(false),
                        goya_wayland::KeySpecial::Home => buffer.move_home(false),
                        goya_wayland::KeySpecial::End => buffer.move_end(false),
                        goya_wayland::KeySpecial::Delete => buffer.delete_forward(),
                        _ => unreachable!(),
                    }
                }
            }
            _ => {}
        }
    }

    fn delete_surrounding(&mut self, before_bytes: u32, after_bytes: u32) {
        self.preedit = None;
        if let Some(buffer) = self.focused.as_ref().and_then(|name| self.buffers.get_mut(name)) {
            buffer.delete_surrounding(before_bytes, after_bytes);
        }
    }

    fn commit(&mut self, text: &str) {
        self.preedit = None;
        if let Some(buffer) = self.focused.as_ref().and_then(|name| self.buffers.get_mut(name)) {
            buffer.insert(text);
        }
    }

    fn apply_to_nodes(&self, nodes: &mut [DrawNode]) {
        for node in nodes.iter_mut().filter(|node| node.is_input) {
            let placeholder = node.text.clone().unwrap_or_default();
            let Some(buffer) = self.buffers.get(&node.name) else {
                continue;
            };
            let focused = self.focused.as_deref() == Some(node.name.as_str());
            if buffer.text().is_empty() && self.preedit.is_none() {
                node.text = Some(if focused {
                    format!("│{placeholder}")
                } else {
                    placeholder
                });
                continue;
            }
            let text = buffer.text();
            let cursor = buffer.cursor_byte().min(text.len());
            let mut value = String::new();
            value.push_str(&text[..cursor]);
            if let Some((preedit, cursor_begin, _)) = &self.preedit {
                if focused && *cursor_begin >= 0 {
                    let mut offset = (*cursor_begin as usize).min(preedit.len());
                    while !preedit.is_char_boundary(offset) {
                        offset -= 1;
                    }
                    value.push_str(&preedit[..offset]);
                    value.push('│');
                    value.push_str(&preedit[offset..]);
                } else {
                    value.push_str(preedit);
                }
            } else if focused {
                value.push('│');
            }
            value.push_str(&text[cursor..]);
            node.text = Some(value);
        }
    }

    fn update_text_input(&self, window: &mut goya_wayland::GoyaWindow, nodes: &[DrawNode]) {
        let Some(name) = self.focused.as_deref() else {
            window.set_text_input_state(false, "", 0, 0, (0, 0, 0, 0));
            return;
        };
        let Some(buffer) = self.buffers.get(name) else {
            window.set_text_input_state(false, "", 0, 0, (0, 0, 0, 0));
            return;
        };
        let Some(node) = nodes.iter().find(|node| node.name == name) else {
            window.set_text_input_state(false, "", 0, 0, (0, 0, 0, 0));
            return;
        };
        let font_px = node
            .text_size
            .filter(|size| size.is_finite() && *size > 0.0)
            .map(|size| size * 4.0)
            .unwrap_or(21.0);
        let advance = font_px * 0.6;
        let cursor_x = (node.rect.x / goya_wayland::MP_PER_PX) as f64
            + 2.0
            + buffer.cursor_grapheme() as f64 * advance;
        let right = ((node.rect.x + node.rect.width) / goya_wayland::MP_PER_PX - 1) as f64;
        let x = cursor_x.clamp((node.rect.x / goya_wayland::MP_PER_PX) as f64, right.max(0.0)) as i32;
        let y = (node.rect.y / goya_wayland::MP_PER_PX) as i32 + 2;
        let height = (font_px * 1.4)
            .round()
            .clamp(1.0, node.rect.height as f64 / goya_wayland::MP_PER_PX as f64) as i32;
        window.set_text_input_state(
            true,
            buffer.text(),
            buffer.cursor_byte().min(u32::MAX as usize) as u32,
            buffer.anchor_byte().min(u32::MAX as usize) as u32,
            (x, y, 1, height),
        );
    }
}

/// Convert a scene draw node into the renderer's primitive vocabulary.
fn present_node(n: &DrawNode) -> Primitive {
    let rect = goya_layout::Rect {
        x: n.rect.x,
        y: n.rect.y,
        width: n.rect.width,
        height: n.rect.height,
    };
    let mut label = n.text.clone().unwrap_or_default();
    if n.is_button && label.is_empty() {
        label = n.name.clone();
    }
    if n.is_button {
        Primitive::Button(rect, label)
    } else if n.text.is_some() {
        Primitive::Text(rect, label)
    } else {
        Primitive::Panel(rect)
    }
}

// Anchor is part of the language surface; keep the import honest until the
// renderer supports anchored text (Wave 4 typography).
#[allow(unused)]
fn _anchor_check(a: Anchor) -> bool {
    matches!(a, Anchor::Start | Anchor::Middle | Anchor::End)
}
