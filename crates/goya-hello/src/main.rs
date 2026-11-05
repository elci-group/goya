//! goya-hello — GY-005 first integrated proof.
//!
//! Builds the foundational Goya runtime (IR → state → layout → render) from
//! the embedded hello module, replays a deterministic input script, renders
//! frames to a framebuffer, and writes a PPM trace for visual verification.
//! Runs headless; the Wayland surface adapter lands in Wave 1 continuation.

use goya_ir::{parse, NodeId};
use goya_layout::Constraints;
use goya_runtime::{Event, HitTarget, Primitive, Runtime};
use goya_state::Store;

const MODULE_SRC: &str = "\
# goya-hello: first integrated proof
column 1 2 3 4
text 2 Hello, Goya!
button 3 1 Increment
input 4 Type a message
root 1
";

/// Software framebuffer: 400x300 pixels, 3 bytes per pixel.
const FB_W: usize = 400;
const FB_H: usize = 300;
const SCALE: i64 = 1000 / 4; // millipoints → pixels (4 px per point)

fn main() {
    let live = std::env::args().any(|a| a == "--live");
    let result = if live { run_live() } else { run() };
    if let Err(e) = result {
        eprintln!("goya-hello: {e}");
        std::process::exit(if live { 0 } else { 1 }); // live: graceful headless exit
    }
}

fn run() -> Result<(), String> {
    let module = parse(MODULE_SRC)
        .map_err(|e| format!("embedded module failed to parse at line {}", e.line))?;
    let store = Store::new();
    let constraints = Constraints {
        max_width: (FB_W as i64) * 1000 / SCALE.max(1),
        max_height: (FB_H as i64) * 1000 / SCALE.max(1),
    };
    let mut rt = Runtime::new(module, store, constraints);

    // Deterministic input script: activate the button three times, then quit.
    let events = [
        Event::Activate(goya_ir::ActionId(1)),
        Event::Activate(goya_ir::ActionId(1)),
        Event::Activate(goya_ir::ActionId(1)),
        Event::Quit,
    ];

    let mut frames_written = 0;
    for (i, event) in events.iter().enumerate() {
        let running = rt.handle(event.clone());
        render_to_ppm(&rt, &format!("target/goya-hello-frame-{i}.ppm"));
        frames_written += 1;
        if !running {
            break;
        }
    }

    let count = match rt.store().get("count") {
        Some(goya_state::Value::Int(n)) => n,
        _ => -1,
    };
    println!("goya-hello: {frames_written} frames rendered, count={count}");
    println!(
        "contracts: ir={} state={} layout={} runtime={}",
        goya_ir::IR_CONTRACT,
        goya_state::STATE_CONTRACT,
        goya_layout::LAYOUT_CONTRACT,
        goya_runtime::RUNTIME_CONTRACT
    );
    if count != 2 {
        return Err(format!(
            "proof failed: expected count=2 after three increments, got {count}"
        ));
    }
    Ok(())
}

/// Rasterise the render list into an RGB framebuffer and write a binary PPM.
fn render_to_ppm(rt: &Runtime, path: &str) {
    let mut fb = vec![24u8; FB_W * FB_H * 3]; // dark background
    for primitive in rt.render_list() {
        match primitive {
            Primitive::Panel(r) => fill(&mut fb, r, &[36, 36, 44]),
            Primitive::Text(r, ref s) => {
                fill(&mut fb, r, &[16, 16, 20]);
                draw_text(&mut fb, r, s, &[235, 235, 240]);
            }
            Primitive::Button(r, ref s) => {
                fill(&mut fb, r, &[70, 90, 160]);
                draw_text(&mut fb, r, s, &[240, 240, 250]);
            }
        }
    }
    if let Some(dir) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut out = format!("P6\n{FB_W} {FB_H}\n255\n").into_bytes();
    out.extend_from_slice(&fb);
    let _ = std::fs::write(path, out);
}

fn fill(fb: &mut [u8], r: goya_layout::Rect, rgb: &[u8; 3]) {
    let x0 = (r.x / SCALE).max(0) as usize;
    let y0 = (r.y / SCALE).max(0) as usize;
    let x1 = ((r.x + r.width) / SCALE).clamp(0, FB_W as i64) as usize;
    let y1 = ((r.y + r.height) / SCALE).clamp(0, FB_H as i64) as usize;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * FB_W + x) * 3;
            fb[i..i + 3].copy_from_slice(rgb);
        }
    }
}

/// Blocky 5x7 glyph renderer for the proof render — legible, deterministic,
/// and dependency-free. Real text shaping is a Wave 4 accessibility concern.
fn draw_text(fb: &mut [u8], r: goya_layout::Rect, s: &str, rgb: &[u8; 3]) {
    let origin_x = (r.x / SCALE + 2).max(0) as usize;
    let origin_y = (r.y / SCALE + 2).max(0) as usize;
    for (ci, ch) in s.chars().enumerate() {
        let glyph = glyph5x7(ch);
        for (gy, row) in glyph.iter().enumerate() {
            for gx in 0..5 {
                if row & (1 << (4 - gx)) != 0 {
                    let x = origin_x + ci * 6 + gx;
                    let y = origin_y + gy;
                    if x < FB_W && y < FB_H {
                        let i = (y * FB_W + x) * 3;
                        fb[i..i + 3].copy_from_slice(rgb);
                    }
                }
            }
        }
    }
}

/// 5x7 glyphs for the characters goya-hello displays. Bit 4 is the leftmost
/// pixel. Unknown characters render as a filled box.
fn glyph5x7(c: char) -> [u8; 7] {
    match c {
        'a' => [
            0b011110, 0b100010, 0b100010, 0b111110, 0b100010, 0b100010, 0b100010,
        ],
        'c' => [
            0b011110, 0b100000, 0b100000, 0b100000, 0b100000, 0b100000, 0b011110,
        ],
        'e' => [
            0b111110, 0b100000, 0b100000, 0b111100, 0b100000, 0b100000, 0b111110,
        ],
        'g' => [
            0b011110, 0b100000, 0b100000, 0b101110, 0b100010, 0b100010, 0b011110,
        ],
        'h' => [
            0b100010, 0b100010, 0b100010, 0b111110, 0b100010, 0b100010, 0b100010,
        ],
        'I' => [
            0b111110, 0b001000, 0b001000, 0b001000, 0b001000, 0b001000, 0b111110,
        ],
        'l' => [
            0b010000, 0b010000, 0b010000, 0b010000, 0b010000, 0b010000, 0b011110,
        ],
        'n' => [
            0b100010, 0b110010, 0b101010, 0b100110, 0b100010, 0b100010, 0b100010,
        ],
        'o' => [
            0b011110, 0b100010, 0b100010, 0b100010, 0b100010, 0b100010, 0b011110,
        ],
        'r' => [
            0b100100, 0b101000, 0b110000, 0b100000, 0b100000, 0b100000, 0b100000,
        ],
        't' => [
            0b010000, 0b010000, 0b111110, 0b010000, 0b010000, 0b010010, 0b001100,
        ],
        'u' => [
            0b100010, 0b100010, 0b100010, 0b100010, 0b100010, 0b100010, 0b011110,
        ],
        'y' => [
            0b100010, 0b100010, 0b011110, 0b001000, 0b001000, 0b010000, 0b010000,
        ],
        ',' => [0, 0, 0, 0, 0b001100, 0b001000, 0b010000],
        '=' => [0, 0, 0b111110, 0, 0b111110, 0, 0],
        '!' => [
            0b001000, 0b001000, 0b001000, 0b001000, 0b001000, 0, 0b001000,
        ],
        ' ' => [0; 7],
        _ => [0b111110; 7],
    }
}

// ---- GY-WAYLAND-001 live mode -------------------------------------------
//
// `goya-hello --live` opens a real Wayland window, feeds pointer events
// into the goya-runtime pipeline, and presents each frame. Missing display
// → graceful message, exit 0-style nonzero, app stays operational.

fn run_live() -> Result<(), String> {
    let module =
        parse(MODULE_SRC).map_err(|e| format!("module parse failed at line {}", e.line))?;
    let store = Store::new();
    let constraints = Constraints {
        max_width: (FB_W as i64) * 1000 / SCALE.max(1),
        max_height: (FB_H as i64) * 1000 / SCALE.max(1),
    };
    let mut rt = Runtime::new(module, store, constraints);
    let mut win = match goya_wayland::GoyaWindow::create("goya-hello", FB_W as i32, FB_H as i32) {
        Ok(w) => w,
        Err(e) => return Err(format!("live mode unavailable: {e}")),
    };
    let mut frames = 0u32;
    const MAX_FRAMES: u32 = 600; // bounded run: ~10 s at 60 Hz
    while frames < MAX_FRAMES && !win.close_requested() {
        for ev in win.poll() {
            match ev {
                goya_wayland::PlatformEvent::PointerPress(x, y) => {
                    // Surface-local pixels → millipoints → hit test.
                    let mx = (x as i64) * goya_wayland::MP_PER_PX;
                    let my = (y as i64) * goya_wayland::MP_PER_PX;
                    match rt.hit_test_target(mx, my) {
                        Some(HitTarget::Action(action)) => {
                            rt.focus_input(None);
                            let _ = rt.handle(Event::Activate(action));
                        }
                        Some(HitTarget::Input(id)) => {
                            rt.focus_input(Some(id));
                        }
                        None => {
                            rt.focus_input(None);
                        }
                    }
                }
                event => apply_text_event(&mut rt, event),
            }
        }
        if let Some(id) = rt.focused_input() {
            let text = rt.input_text(id).unwrap_or_default();
            let cursor = rt.input_cursor_byte(id).unwrap_or(text.len());
            let anchor = rt.input_anchor_byte(id).unwrap_or(cursor);
            let cursor_rect = rt.frame().rect(id).map_or((0, 0, 1, 20), |rect| {
                (
                    (rect.x / goya_wayland::MP_PER_PX) as i32,
                    (rect.y / goya_wayland::MP_PER_PX) as i32,
                    (rect.width / goya_wayland::MP_PER_PX).max(1) as i32,
                    (rect.height / goya_wayland::MP_PER_PX).max(1) as i32,
                )
            });
            win.set_text_input_state(true, text, cursor as u32, anchor as u32, cursor_rect);
        } else {
            win.set_text_input_state(false, "", 0, 0, (0, 0, 0, 0));
        }
        let _ = win.present(&rt.render_list(), &[], [235, 235, 240], [24, 24, 28], 2);
        win.pump(1);
        frames += 1;
    }
    let count = match rt.store().get("count") {
        Some(goya_state::Value::Int(n)) => n,
        _ => -1,
    };
    println!("goya-hello live: {frames} frames presented, count={count}");
    Ok(())
}

fn apply_text_event(rt: &mut Runtime, event: goya_wayland::PlatformEvent) {
    use goya_wayland::{KeySpecial, PlatformEvent};
    let runtime_event = match event {
        PlatformEvent::KeyChar(c) => Some(Event::Character(c)),
        PlatformEvent::KeySpecial(KeySpecial::ArrowLeft) => {
            Some(Event::TextMoveLeft { extend: false })
        }
        PlatformEvent::KeySpecial(KeySpecial::ArrowRight) => {
            Some(Event::TextMoveRight { extend: false })
        }
        PlatformEvent::KeySpecial(KeySpecial::Home) => Some(Event::TextMoveHome { extend: false }),
        PlatformEvent::KeySpecial(KeySpecial::End) => Some(Event::TextMoveEnd { extend: false }),
        PlatformEvent::KeySpecial(KeySpecial::Delete) => Some(Event::TextDelete),
        PlatformEvent::KeySpecial(KeySpecial::Escape) => {
            rt.focus_input(None);
            None
        }
        PlatformEvent::KeySpecial(KeySpecial::Tab) => {
            let next = (rt.focused_input() != Some(NodeId(4))).then_some(NodeId(4));
            rt.focus_input(next);
            None
        }
        PlatformEvent::Preedit(preedit) => Some(Event::ImeBatch {
            preedit: preedit.map(|text| (text, 0, 0)),
            commit: None,
            delete: None,
        }),
        PlatformEvent::CommitText(text) => Some(Event::ImeBatch {
            preedit: None,
            commit: Some(text),
            delete: None,
        }),
        _ => None,
    };
    if let Some(event) = runtime_event {
        let _ = rt.handle(event);
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;

    #[test]
    fn live_editor_routes_unicode_cursor_and_ime_events_to_the_focused_input() {
        let module = parse(MODULE_SRC).unwrap();
        let mut runtime = Runtime::new(
            module,
            Store::new(),
            Constraints {
                max_width: 400_000,
                max_height: 300_000,
            },
        );
        let input_rect = runtime.frame().rect(NodeId(4)).unwrap();
        assert_eq!(
            runtime.hit_test_target(input_rect.x + 1, input_rect.y + 1),
            Some(HitTarget::Input(NodeId(4)))
        );
        assert!(runtime.focus_input(Some(NodeId(4))));
        apply_text_event(&mut runtime, goya_wayland::PlatformEvent::KeyChar('日'));
        apply_text_event(
            &mut runtime,
            goya_wayland::PlatformEvent::KeySpecial(goya_wayland::KeySpecial::ArrowLeft),
        );
        apply_text_event(&mut runtime, goya_wayland::PlatformEvent::KeyChar('é'));
        assert_eq!(runtime.input_text(NodeId(4)), Some("é日"));
        apply_text_event(
            &mut runtime,
            goya_wayland::PlatformEvent::Preedit(Some("かな".into())),
        );
        assert_eq!(runtime.input_text(NodeId(4)), Some("é日"));
        apply_text_event(
            &mut runtime,
            goya_wayland::PlatformEvent::CommitText("仮名".into()),
        );
        assert_eq!(runtime.input_text(NodeId(4)), Some("é仮名日"));
        assert!(matches!(
            runtime.render_list().last(),
            Some(Primitive::Text(_, value)) if value == "é仮名日"
        ));
    }
}
