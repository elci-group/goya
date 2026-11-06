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
        // Hot reloads can shrink or remove scrollable content. Keep saved
        // offsets valid before routing pointer events against the new frame.
        scroll_offsets.reconcile(&event_frame);
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
                goya_wayland::PlatformEvent::PointerScrollDelta { x: dx, y: dy } => {
                    if let Some((x, y)) = pointer {
                        let x = (x as i64) * goya_wayland::MP_PER_PX;
                        let y = (y as i64) * goya_wayland::MP_PER_PX;
                        if let Some(target) = event_frame.scroll_target_at(&scroll_offsets, x, y) {
                            let _ = scroll_offsets.scroll_by(
                                &event_frame,
                                &target,
                                (-dx * goya_wayland::MP_PER_PX as f64) as i64,
                                (-dy * goya_wayland::MP_PER_PX as f64) as i64,
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
        let text_overlays = inputs.apply_to_nodes(&mut nodes);
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
        let fills: Vec<Option<u32>> = nodes.iter().map(|node| node.fill).collect();
        let _ = win.present_with_text_overlays(
            &prims,
            &fills,
            &clips,
            &text_sizes,
            &text_overlays,
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
        let Some(buffer) = self
            .focused
            .as_ref()
            .and_then(|name| self.buffers.get_mut(name))
        else {
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

    fn commit(&mut self, text: &str) {
        self.preedit = None;
        if let Some(buffer) = self
            .focused
            .as_ref()
            .and_then(|name| self.buffers.get_mut(name))
        {
            buffer.insert(text);
        }
    }

    fn apply_to_nodes(&self, nodes: &mut [DrawNode]) -> Vec<goya_wayland::TextOverlay> {
        let mut overlays = Vec::new();
        for (primitive_index, node) in nodes.iter_mut().enumerate() {
            if !node.is_input {
                continue;
            }
            let placeholder = node.text.clone().unwrap_or_default();
            let Some(buffer) = self.buffers.get(&node.name) else {
                continue;
            };
            let focused = self.focused.as_deref() == Some(node.name.as_str());
            let text = buffer.text();
            let cursor = buffer.cursor_byte().min(text.len());
            let mut value = String::new();
            value.push_str(&text[..cursor]);
            let preedit_start = value.len();
            if let Some((preedit, _, _)) = &self.preedit {
                value.push_str(preedit);
            }
            value.push_str(&text[cursor..]);
            if text.is_empty() && self.preedit.is_none() {
                node.text = Some(placeholder);
            } else {
                node.text = Some(value);
            }
            if focused {
                let (caret_byte_offset, selection_byte_range) = match &self.preedit {
                    Some((preedit, cursor_begin, cursor_end)) => {
                        let begin = floor_char_boundary(preedit, (*cursor_begin).max(0) as usize);
                        let caret = preedit_start + begin;
                        let end = floor_char_boundary(preedit, (*cursor_end).max(0) as usize);
                        let selection =
                            (begin != end).then_some((preedit_start + begin, preedit_start + end));
                        (Some(caret), selection)
                    }
                    None => (
                        Some(cursor),
                        buffer
                            .selection_bytes()
                            .map(|selection| (selection.start, selection.end)),
                    ),
                };
                overlays.push(goya_wayland::TextOverlay {
                    primitive_index,
                    caret_byte_offset,
                    selection_byte_range,
                    caret_rgb: [255, 220, 101],
                    selection_rgb: [55, 110, 210],
                });
            }
        }
        overlays
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
        let (x, y, width, height) = input_cursor_rect(node, buffer);
        window.set_text_input_state(
            true,
            buffer.text(),
            buffer.cursor_byte().min(u32::MAX as usize) as u32,
            buffer.anchor_byte().min(u32::MAX as usize) as u32,
            (x, y, width, height),
        );
    }
}

/// Candidate-window geometry from the same shaped metrics used by the scene
/// renderer. Node geometry is already translated for ancestor scroll offsets.
fn input_cursor_rect(node: &DrawNode, buffer: &TextBuffer) -> (i32, i32, i32, i32) {
    let px = |value: i64| value / goya_wayland::MP_PER_PX;
    let width = px(node.rect.width).max(1) as f32;
    let height = px(node.rect.height).max(1) as f32;
    let font_px = node
        .text_size
        .filter(|size| size.is_finite() && *size > 0.0)
        .map(|size| (size * 4.0) as f32)
        .unwrap_or(21.0); // present uses glyph_px=3 => 7 px/glyph
    let caret = goya_wayland::measure_text_caret(
        buffer.text(),
        (width - 4.0).max(1.0),
        (height - 4.0).max(1.0),
        font_px,
        buffer.cursor_byte(),
    );
    let (caret_x, caret_y, caret_height) = caret
        .map(|caret| (caret.x_px, caret.y_px, caret.height_px))
        .unwrap_or((0.0, 0.0, font_px * 1.4));
    let left = px(node.rect.x) as f32 + 2.0;
    let top = px(node.rect.y) as f32 + 2.0;
    let right = px(node.rect.x.saturating_add(node.rect.width)) as f32 - 1.0;
    let bottom = px(node.rect.y.saturating_add(node.rect.height)) as f32 - 1.0;
    let clip = node.clip.map(|clip| {
        (
            px(clip.x) as f32,
            px(clip.y) as f32,
            px(clip.x.saturating_add(clip.width)) as f32,
            px(clip.y.saturating_add(clip.height)) as f32,
        )
    });
    let (clip_left, clip_top, clip_right, clip_bottom) =
        clip.unwrap_or((left, top, right + 1.0, bottom + 1.0));
    let min_x = left.max(clip_left);
    let max_x = right.min(clip_right - 1.0).max(min_x);
    let min_y = top.max(clip_top);
    let max_y = bottom.min(clip_bottom - 1.0).max(min_y);
    let x = (left + caret_x).clamp(min_x, max_x);
    let y = (top + caret_y).clamp(min_y, max_y);
    let h = caret_height.round().clamp(1.0, (max_y - y + 1.0).max(1.0));
    (x.round() as i32, y.round() as i32, 1, h as i32)
}

fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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

#[cfg(test)]
mod input_tests {
    use super::*;

    #[test]
    fn ime_cursor_rect_tracks_multibyte_wrapped_text_and_scroll_clip() {
        let source =
            "@ontology demo { @input name { extent: (15, 80, 0) text-size: 3 text: \"\" } }";
        let scene = goya_lang::parse(source).unwrap();
        let frame = layout(&scene, 100.0, 100.0);
        let mut node = frame.find("name").unwrap().clone();
        let mut buffer = TextBuffer::new("é漢字かな漢字かな");
        let end_rect = input_cursor_rect(&node, &buffer);
        buffer.move_home(false);
        let start_rect = input_cursor_rect(&node, &buffer);
        assert!(
            end_rect.1 > start_rect.1,
            "shaped caret should follow wrapping"
        );

        node.rect.y -= 10_000;
        node.clip = Some(RectMp {
            x: 0,
            y: 5_000,
            width: 100_000,
            height: 50_000,
        });
        let clipped = input_cursor_rect(&node, &buffer);
        assert!(clipped.1 >= 20); // clip top (20 px) is respected
    }

    #[test]
    fn scene_input_routes_unicode_navigation_and_preedit_at_cursor() {
        let scene = goya_lang::parse(
            "@ontology demo { @input name { extent: (200, 40, 0) text: \"Search\" } }",
        )
        .unwrap();
        let frame = layout(&scene, 300.0, 100.0);
        let mut input = SceneInputs::default();
        input.sync_nodes(&frame);
        input.focus(Some("name".into()));
        input.handle_character('日');
        input.handle_special(goya_wayland::KeySpecial::ArrowLeft, &frame);
        input.handle_character('é');
        assert_eq!(input.buffers["name"].text(), "é日");

        input.commit("語");
        assert_eq!(input.buffers["name"].text(), "é語日");
        // GY-ZCODE-006: surrounding-text deletion left the contract surface
        // (no `DeleteSurroundingText` platform event); move the caret back
        // through the navigation path instead.
        input.handle_special(goya_wayland::KeySpecial::ArrowLeft, &frame);
        input.handle_special(goya_wayland::KeySpecial::Delete, &frame);
        assert_eq!(input.buffers["name"].text(), "é日");

        input.preedit = Some(("かな".into(), 3, 6));
        let mut nodes = frame.nodes.clone();
        let overlays = input.apply_to_nodes(&mut nodes);
        assert_eq!(
            nodes
                .iter()
                .find(|node| node.name == "name")
                .unwrap()
                .text
                .as_deref(),
            Some("éかな日")
        );
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].caret_byte_offset, Some("éか".len()));
        assert_eq!(
            overlays[0].selection_byte_range,
            Some(("éか".len(), "éかな".len()))
        );
    }

    #[test]
    fn scene_input_overlays_follow_text_buffer_selection_and_focus() {
        let scene = goya_lang::parse(
            "@ontology demo { @input name { extent: (200, 40, 0) text: \"Search\" } }",
        )
        .unwrap();
        let frame = layout(&scene, 300.0, 100.0);
        let mut input = SceneInputs::default();
        input.sync_nodes(&frame);
        input.focus(Some("name".into()));
        let buffer = input.buffers.get_mut("name").unwrap();
        buffer.insert("é🙂");
        buffer.select_all();

        let mut nodes = frame.scrolled_nodes(&ScrollOffsets::default());
        let overlays = input.apply_to_nodes(&mut nodes);
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].caret_byte_offset, Some("é🙂".len()));
        assert_eq!(overlays[0].selection_byte_range, Some((0, "é🙂".len())));
        assert_eq!(
            nodes
                .iter()
                .find(|node| node.name == "name")
                .unwrap()
                .text
                .as_deref(),
            Some("é🙂")
        );

        input.focus(None);
        let overlays = input.apply_to_nodes(&mut nodes);
        assert!(overlays.is_empty());
    }
}
